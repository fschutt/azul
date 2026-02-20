//! Cross-platform V2 event processing system
//!
//! This module contains the **complete unified event processing logic** that is shared across all
//! platforms (macOS, Windows, X11, Wayland). The V2 system uses state-diffing between frames to
//! detect events, eliminating platform-specific event handling differences.
//!
//! ## Architecture
//!
//! The `PlatformWindowV2` trait provides **default implementations** for all complex logic:
//! - Event processing (state diffing via `process_window_events()`)
//! - Callback invocation (`dispatch_events_propagated()`)
//! - Callback result handling (`process_callback_result_v2()`)
//! - Hit testing (`perform_scrollbar_hit_test()`)
//! - Scrollbar interaction (`handle_scrollbar_click()`, `handle_scrollbar_drag()`)
//!
//! Platform implementations only need to:
//! 1. Implement simple getter methods to access their window state
//! 2. Call `process_window_events()` after updating platform state
//! 3. Update the screen based on the returned `ProcessEventResult`
//!
//! ## Event Processing Flow
//!
//! ```text
//! Platform Input → Update Window State → Update Hit Tests → process_window_events()
//!                                                                      ↓
//!                                      ┌───────────────────────────────┘
//!                                      ↓
//!                          PRE-EVENT-DISPATCH PROCESSING
//! =
//!                          1. Scroll: record_scroll_from_hit_test() → physics timer → ScrollTo
//!                          2. Text: process_text_input() on LayoutWindow
//!                          3. A11y: record_state_changes() on A11yManager
//!                          ↓
//!                          EVENT FILTERING & DISPATCH
//! =
//!                          4. State diffing (window_state::create_events_from_states)
//!                          5. Event filtering (dispatch_events)
//!                          6. Callback invocation (dispatch_events_propagated)
//!                          ↓
//!                          POST-CALLBACK PROCESSING
//! =
//!                          7. Process callback results (update DOM, layout, etc.)
//!                          8. Re-layout if necessary
//!                          9. Mark dirty nodes for re-render
//! ```
//!
//! ## Platform Integration Points
//!
//! ### macOS (dll/src/desktop/shell2/macos/events.rs)
//!
//! **Where to call `process_window_events()`:**
//! - In each native event handler AFTER updating `current_window_state`
//! - Examples:
//!   - `handle_mouse_down()` - After setting mouse button state and hit test
//!   - `handle_mouse_up()` - After clearing mouse button state
//!   - `handle_mouse_moved()` - After updating cursor position and hit test
//!   - `handle_key_down()` - After updating keyboard state
//!   - `handle_scroll()` - After calling scroll_manager.record_scroll_from_hit_test()
//!   - `handle_text_input()` - Platform should provide text_input: &str to process_text_input()
//!   - `handle_window_resize()` - After updating size in window state
//!
//! **Hit-Testing Requirements:**
//! - Call `update_hit_test()` before `process_window_events()` for mouse/touch events
//! - Hit test updates `hover_manager.push_hit_test(InputPointId::Mouse, hit_test)`
//! - For multi-touch: call for each touch with `InputPointId::Touch(id)`
//!
//! **Scroll Integration:**
//! - Get scroll delta from NSEvent
//! - Call `scroll_manager.record_scroll_from_hit_test(delta_x, delta_y, source, ...)`
//! - ScrollManager queues input for physics timer
//! - Timer pushes `CallbackChange::ScrollTo`, event processing applies offsets
//! - Then call `process_window_events()` which will process the scroll changes
//!
//! **Text Input Integration:**
//! - Get composed text from NSTextInputClient (insertText/setMarkedText)
//! - Platform should store text_input string temporarily
//! - `process_window_events()` will call `process_text_input(text_input)`
//! - Framework applies edit, updates cursor, marks nodes dirty
//!
//! **Peculiarities:**
//! - Uses NSEvent for native input
//! - Hit-testing done via `update_hit_test()` before processing
//! - Scrollbar drag state stored in window struct
//! - Must call `present()` for RequestRedraw results
//!
//! ### Windows (dll/src/desktop/shell2/windows/mod.rs)
//!
//! **Where to call `process_window_events()`:**
//! - In WndProc message handlers AFTER updating `current_window_state`
//! - Examples:
//!   - `WM_LBUTTONDOWN/WM_RBUTTONDOWN` - After setting mouse state
//!   - `WM_LBUTTONUP/WM_RBUTTONUP` - After clearing mouse state
//!   - `WM_MOUSEMOVE` - After updating cursor position
//!   - `WM_KEYDOWN/WM_KEYUP` - After updating keyboard state
//!   - `WM_MOUSEWHEEL` - After updating scroll delta
//!   - `WM_SIZE` - After updating window size
//!
//! **Peculiarities:**
//! - Uses Win32 message loop (WndProc)
//! - Hit-testing via WebRender on every mouse move
//! - Must handle WM_PAINT separately for rendering
//! - DPI scaling handled via GetDpiForWindow
//!
//! ### X11 (dll/src/desktop/shell2/linux/x11/events.rs)
//!
//! **Where to call `process_window_events()`:**
//! - In event loop AFTER processing XEvent and updating `current_window_state`
//! - Examples:
//!   - `ButtonPress/ButtonRelease` - After setting mouse button state
//!   - `MotionNotify` - After updating cursor position and hit test
//!   - `KeyPress/KeyRelease` - After XIM processing and keyboard state update
//!   - `ConfigureNotify` - After updating window size/position
//!   - `EnterNotify/LeaveNotify` - After updating cursor in/out state
//!
//! **Peculiarities:**
//! - XIM (X Input Method) for international text input
//! - XFilterEvent must be called before processing for IME
//! - Manual coordinate translation (relative to root window)
//! - Expose events trigger redraw separately
//!
//! ### Wayland (dll/src/desktop/shell2/linux/wayland/mod.rs)
//!
//! **Where to call `process_window_events()`:**
//! - In Wayland event handlers AFTER updating `current_window_state`
//! - Examples:
//!   - `wl_pointer::button` - After setting mouse button state
//!   - `wl_pointer::motion` - After updating cursor position
//!   - `wl_keyboard::key` - After updating keyboard state
//!   - `xdg_toplevel::configure` - After updating window size
//!
//! **Peculiarities:**
//! - Compositor-driven (no XY coordinates, uses surface-local coords)
//! - Frame callbacks for rendering synchronization
//! - Client-side decorations (CSD) always enabled
//! - Seat-based input (single seat assumption for now)
//!
//! When migrating a platform to use `PlatformWindowV2`.

use alloc::sync::Arc;
use core::cell::RefCell;
use std::collections::BTreeMap;

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::{DomId, NodeId},
    events::{
        EventFilter, FocusEventFilter, PreCallbackFilterResult,
        ProcessEventResult, SyntheticEvent,
    },
    geom::LogicalPosition,
    gl::*,
    hit_test::{DocumentId, PipelineId},
    id::NodeId as CoreNodeId,
    refany::RefAny,
    resources::{IdNamespace, ImageCache, RendererResources},
    window::RawWindowHandle,
    FastBTreeSet,
};
use azul_layout::{
    callbacks::{
        CallCallbacksResult, Callback as LayoutCallback, CallbackInfo, ExternalSystemCallbacks,
    },
    event_determination::determine_all_events,
    hit_test::FullHitTest,
    window::{LayoutWindow, ScrollbarDragState},
    window_state::{self, FullWindowState},
};
use rust_fontconfig::FcFontCache;

use crate::desktop::wr_translate2::{self, AsyncHitTester, WrRenderApi};
use crate::{log_debug, log_warn};

/// Maximum depth for recursive event processing (prevents infinite loops from callbacks)
// Event Processing Configuration

/// Maximum recursion depth for event processing.
///
/// Events can trigger callbacks that regenerate the DOM, which triggers new events.
/// This limit prevents infinite loops.
const MAX_EVENT_RECURSION_DEPTH: usize = 7;

// Platform-specific Clipboard Helpers

/// Get clipboard text content (platform-specific)
#[inline]
fn get_system_clipboard() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        crate::desktop::shell2::windows::clipboard::get_clipboard_content()
    }
    #[cfg(target_os = "macos")]
    {
        crate::desktop::shell2::macos::clipboard::get_clipboard_content()
    }
    #[cfg(all(target_os = "linux", feature = "x11"))]
    {
        crate::desktop::shell2::linux::x11::clipboard::get_clipboard_content()
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        all(target_os = "linux", feature = "x11")
    )))]
    {
        None
    }
}

/// Set clipboard text content (platform-specific)
#[inline]
fn set_system_clipboard(text: String) -> bool {
    #[cfg(target_os = "windows")]
    {
        use clipboard_win::{formats, set_clipboard};
        set_clipboard(formats::Unicode, &text).is_ok()
    }
    #[cfg(target_os = "macos")]
    {
        crate::desktop::shell2::macos::clipboard::write_to_clipboard(&text).is_ok()
    }
    #[cfg(all(target_os = "linux", feature = "x11"))]
    {
        crate::desktop::shell2::linux::x11::clipboard::write_to_clipboard(&text).is_ok()
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        all(target_os = "linux", feature = "x11")
    )))]
    {
        false
    }
}

/// Timer callback for auto-scroll during drag selection.
///
/// This callback fires at the monitor's refresh rate during drag-to-scroll operations.
/// It checks if dragging is still active, finds the scrollable container ancestor,
/// calculates scroll delta based on mouse distance from container edges, and
/// pushes `CallbackChange::ScrollTo` to move the scroll position.
///
/// The callback terminates automatically when:
/// - Mouse button is released (no longer dragging)
/// - Mouse returns to within container bounds (no scroll needed)
extern "C" fn auto_scroll_timer_callback(
    _data: azul_core::refany::RefAny,
    mut timer_info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    use azul_core::task::TerminateTimer;

    // Access window state through callback_info
    let callback_info = &timer_info.callback_info;

    // Get current mouse position from window state
    let full_window_state = callback_info.get_current_window_state();

    // Check if still dragging (left mouse button is down)
    if !full_window_state.mouse_state.left_down {
        return azul_core::callbacks::TimerCallbackReturn::terminate_unchanged();
    }

    // Get mouse position - if mouse is outside window, terminate timer
    let mouse_position = match full_window_state.mouse_state.cursor_position.get_position() {
        Some(pos) => pos,
        None => {
            return azul_core::callbacks::TimerCallbackReturn::terminate_unchanged();
        }
    };

    // Get the focused node (the node being drag-selected in)
    let focused_node = match callback_info.get_focused_node() {
        Some(node) => node,
        None => {
            return azul_core::callbacks::TimerCallbackReturn::continue_unchanged();
        }
    };

    let dom_id = focused_node.dom;
    let node_id = match focused_node.node.into_crate_internal() {
        Some(id) => id,
        None => {
            return azul_core::callbacks::TimerCallbackReturn::continue_unchanged();
        }
    };

    // Find the scrollable ancestor of the focused node
    let scroll_parent = match callback_info.find_scroll_parent(dom_id, node_id) {
        Some(parent_id) => parent_id,
        None => {
            // No scrollable ancestor — continue timer but nothing to do
            return azul_core::callbacks::TimerCallbackReturn::continue_unchanged();
        }
    };

    // Get scroll node info for the scrollable ancestor
    let scroll_info = match callback_info.get_scroll_node_info(dom_id, scroll_parent) {
        Some(info) => info,
        None => {
            return azul_core::callbacks::TimerCallbackReturn::continue_unchanged();
        }
    };

    // Calculate scroll delta based on mouse distance from container edges
    let container = scroll_info.container_rect;
    let edge_threshold = 30.0_f32; // pixels from edge before auto-scroll starts
    let max_speed = 15.0_f32; // max pixels per tick

    let mut delta_x = 0.0_f32;
    let mut delta_y = 0.0_f32;

    // Check vertical edges
    if mouse_position.y < container.origin.y + edge_threshold {
        // Mouse above container — scroll up
        let distance = (container.origin.y + edge_threshold) - mouse_position.y;
        delta_y = -(distance / edge_threshold * max_speed).min(max_speed);
    } else if mouse_position.y > container.origin.y + container.size.height - edge_threshold {
        // Mouse below container — scroll down
        let distance = mouse_position.y - (container.origin.y + container.size.height - edge_threshold);
        delta_y = (distance / edge_threshold * max_speed).min(max_speed);
    }

    // Check horizontal edges
    if mouse_position.x < container.origin.x + edge_threshold {
        let distance = (container.origin.x + edge_threshold) - mouse_position.x;
        delta_x = -(distance / edge_threshold * max_speed).min(max_speed);
    } else if mouse_position.x > container.origin.x + container.size.width - edge_threshold {
        let distance = mouse_position.x - (container.origin.x + container.size.width - edge_threshold);
        delta_x = (distance / edge_threshold * max_speed).min(max_speed);
    }

    if delta_x.abs() < 0.01 && delta_y.abs() < 0.01 {
        // Mouse within container bounds — no scroll needed but keep timer running
        return azul_core::callbacks::TimerCallbackReturn::continue_unchanged();
    }

    // Calculate new scroll position and push ScrollTo
    let new_pos = azul_core::geom::LogicalPosition {
        x: (scroll_info.current_offset.x + delta_x).max(0.0).min(scroll_info.max_scroll_x),
        y: (scroll_info.current_offset.y + delta_y).max(0.0).min(scroll_info.max_scroll_y),
    };

    let hierarchy_id = azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(scroll_parent));
    timer_info.scroll_to(dom_id, hierarchy_id, new_pos);

    azul_core::callbacks::TimerCallbackReturn {
        should_update: azul_core::callbacks::Update::RefreshDom,
        should_terminate: TerminateTimer::Continue,
    }
}

// Focus Restyle Helper

/// Apply focus change restyle and determine the ProcessEventResult.
///
/// Uses ChangeAccumulator to classify restyle changes granularly:
/// - Paint-only changes (e.g. color) → ShouldUpdateDisplayListCurrentWindow
/// - Layout-affecting changes → ShouldIncrementalRelayout (no DOM rebuild!)
/// - No changes → ShouldReRenderCurrentWindow
fn apply_focus_restyle(
    layout_window: &mut LayoutWindow,
    old_focus: Option<NodeId>,
    new_focus: Option<NodeId>,
) -> ProcessEventResult {
    use azul_core::styled_dom::FocusChange;
    use azul_core::diff::ChangeAccumulator;

    // Get the first (primary) layout result
    let Some((_, layout_result)) = layout_window.layout_results.iter_mut().next() else {
        return ProcessEventResult::ShouldReRenderCurrentWindow;
    };

    // Apply restyle for focus change
    let restyle_result = layout_result.styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: old_focus,
            gained_focus: new_focus,
        }),
        None, // hover
        None, // active
    );

    log_debug!(
        super::debug_server::LogCategory::Input,
        "[Event V2] Focus restyle: needs_layout={}, needs_display_list={}, changed_nodes={}, max_scope={:?}",
        restyle_result.needs_layout,
        restyle_result.needs_display_list,
        restyle_result.changed_nodes.len(),
        restyle_result.max_relayout_scope
    );

    if restyle_result.changed_nodes.is_empty() {
        return ProcessEventResult::ShouldReRenderCurrentWindow;
    }

    if restyle_result.gpu_only_changes {
        return ProcessEventResult::ShouldReRenderCurrentWindow;
    }

    // Feed RestyleResult through ChangeAccumulator for granular classification
    let mut accumulator = ChangeAccumulator::new();
    accumulator.merge_restyle_result(&restyle_result);

    if accumulator.needs_layout() {
        // Restyle changed layout-affecting properties → incremental relayout
        // (no DOM rebuild needed — the StyledDom already has updated states)
        ProcessEventResult::ShouldIncrementalRelayout
    } else if accumulator.needs_paint_only() {
        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
    } else {
        ProcessEventResult::ShouldReRenderCurrentWindow
    }
}

// Platform-Specific Timer Management

/// Hit test node structure for event routing.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct HitTestNode {
    pub dom_id: u64,
    pub node_id: u64,
}

/// Borrowed resources needed for `invoke_single_callback`.
///
/// This struct borrows individual fields from the window, allowing the borrow checker
/// to see that we're borrowing distinct fields rather than `&mut self` multiple times.
/// This avoids borrow checker conflicts when calling trait methods.
pub struct InvokeSingleCallbackBorrows<'a> {
    /// Mutable layout window for callback invocation
    pub layout_window: &'a mut LayoutWindow,
    /// Raw window handle for platform identification
    pub window_handle: RawWindowHandle,
    /// OpenGL context pointer
    pub gl_context_ptr: &'a OptionGlContextPtr,
    /// Mutable image cache
    pub image_cache: &'a mut ImageCache,
    /// Cloned font cache (FcFontCache doesn't support &mut access)
    pub fc_cache_clone: FcFontCache,
    /// System style (Arc, cheap to clone)
    pub system_style: Arc<azul_css::system::SystemStyle>,
    /// Previous window state
    pub previous_window_state: &'a Option<FullWindowState>,
    /// Current window state
    pub current_window_state: &'a FullWindowState,
    /// Renderer resources
    pub renderer_resources: &'a mut RendererResources,
}

/// Trait that platform-specific window types must implement to use the unified V2 event system.
///
/// This trait provides **default implementations** for all complex cross-platform logic.
/// Platform implementations only need to implement the simple getter methods (27 methods).
///
/// ## Required Methods (Simple Getters - 27 total)
///
/// Platforms must implement these methods to expose their internal state:
/// - Layout window access (`get_layout_window`, `get_layout_window_mut`)
/// - Window state access (`get_current_window_state`, `get_previous_window_state`, etc.)
/// - Resource access (`get_image_cache_mut`, `get_renderer_resources_mut`, etc.)
/// - Hit testing state (`get_hit_tester`, `get_scrollbar_drag_state`, etc.)
/// - Frame regeneration (`needs_frame_regeneration`, `mark_frame_needs_regeneration`, etc.)
/// - Raw window handle (`get_raw_window_handle`)
/// - **Callback preparation (`prepare_callback_invocation`)** - Returns all borrows needed for
///   callbacks
///
/// ## Provided Methods (Complete Logic - All Cross-Platform!)
///
/// These methods have default implementations with the full cross-platform logic:
/// - `dispatch_events_propagated()` - **FULLY CROSS-PLATFORM!** W3C event dispatch using
///   `propagate_event()` + `prepare_callback_invocation()`
/// - `process_window_events_recursive_v2()` - Main event processing with recursion
/// - `process_callback_result_v2()` - Handle callback results
/// - `perform_scrollbar_hit_test()` - Scrollbar interaction
/// - `handle_scrollbar_click()` - Scrollbar click handling
/// - `handle_scrollbar_drag()` - Scrollbar drag handling
/// - `gpu_scroll()` - GPU-accelerated smooth scrolling
///
/// ## Platform Implementation Checklist
///
/// To integrate a new platform:
/// 1. Implement the 26 required getter methods
/// 2. Import the trait: `use crate::desktop::shell2::common::event_v2::PlatformWindowV2;`
/// 3. Call `self.process_window_events_recursive_v2(0)` after updating window state
/// 4. Done! All event processing is now unified.
pub trait PlatformWindowV2 {
    // REQUIRED: Simple Getter Methods (Platform Must Implement)

    // Layout Window Access

    /// Get mutable access to the layout window
    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow>;

    /// Get immutable access to the layout window
    fn get_layout_window(&self) -> Option<&LayoutWindow>;

    // Window State Access

    /// Get the current window state
    fn get_current_window_state(&self) -> &FullWindowState;

    /// Get mutable access to the current window state
    fn get_current_window_state_mut(&mut self) -> &mut FullWindowState;

    /// Get the previous window state (if available)
    fn get_previous_window_state(&self) -> &Option<FullWindowState>;

    /// Set the previous window state
    fn set_previous_window_state(&mut self, state: FullWindowState);

    // Resource Access

    /// Get mutable access to image cache
    fn get_image_cache_mut(&mut self) -> &mut ImageCache;

    /// Get mutable access to renderer resources
    fn get_renderer_resources_mut(&mut self) -> &mut RendererResources;

    /// Get the font cache
    fn get_fc_cache(&self) -> &Arc<FcFontCache>;

    /// Get the OpenGL context pointer
    fn get_gl_context_ptr(&self) -> &OptionGlContextPtr;

    /// Get the system style
    fn get_system_style(&self) -> &Arc<azul_css::system::SystemStyle>;

    /// Get the shared application data
    fn get_app_data(&self) -> &Arc<RefCell<RefAny>>;

    // Scrollbar State

    /// Get the current scrollbar drag state
    fn get_scrollbar_drag_state(&self) -> Option<&ScrollbarDragState>;

    /// Get mutable access to scrollbar drag state
    fn get_scrollbar_drag_state_mut(&mut self) -> &mut Option<ScrollbarDragState>;

    /// Set scrollbar drag state
    fn set_scrollbar_drag_state(&mut self, state: Option<ScrollbarDragState>);

    // Hit Testing

    /// Get the async hit tester
    fn get_hit_tester(&self) -> &AsyncHitTester;

    /// Get mutable access to hit tester
    fn get_hit_tester_mut(&mut self) -> &mut AsyncHitTester;

    /// Get the last hovered node
    fn get_last_hovered_node(&self) -> Option<&HitTestNode>;

    /// Set the last hovered node
    fn set_last_hovered_node(&mut self, node: Option<HitTestNode>);

    // WebRender Infrastructure

    /// Get the document ID
    fn get_document_id(&self) -> DocumentId;

    /// Get the ID namespace
    fn get_id_namespace(&self) -> IdNamespace;

    /// Get the render API
    fn get_render_api(&self) -> &WrRenderApi;

    /// Get mutable access to render API
    fn get_render_api_mut(&mut self) -> &mut WrRenderApi;

    /// Get the renderer (if available)
    fn get_renderer(&self) -> Option<&webrender::Renderer>;

    /// Get mutable access to renderer
    fn get_renderer_mut(&mut self) -> Option<&mut webrender::Renderer>;

    // Timers and Threads

    /// Get raw window handle for spawning child windows
    fn get_raw_window_handle(&self) -> RawWindowHandle;

    // Frame Regeneration

    /// Check if frame needs regeneration
    fn needs_frame_regeneration(&self) -> bool;

    /// Mark that the frame needs regeneration
    fn mark_frame_needs_regeneration(&mut self);

    /// Clear frame regeneration flag
    fn clear_frame_regeneration_flag(&mut self);

    // Callback Invocation Preparation

    /// Borrow all resources needed for `invoke_single_callback` in one call.
    ///
    /// This method returns a struct with individual field borrows, allowing the borrow
    /// checker to see that we're borrowing distinct fields rather than `&mut self` multiple times.
    ///
    /// ## Returns
    /// * `InvokeSingleCallbackBorrows` - All borrowed resources needed for callback invocation
    fn prepare_callback_invocation(&mut self) -> InvokeSingleCallbackBorrows;

    // REQUIRED: Timer Management (Platform-Specific Implementation)

    /// Start a timer with the given ID and interval.
    ///
    /// When the timer fires, the platform should tick timers in the layout window
    /// and trigger event processing to invoke timer callbacks.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **Windows**: Use `SetTimer(hwnd, timer_id, interval_ms, NULL)`
    /// - **macOS**: Use `NSTimer::scheduledTimerWithTimeInterval` with userInfo containing timer_id
    /// - **X11**: Add timer to internal manager, use select() timeout to check expiration
    /// - **Wayland**: Create timerfd with timerfd_create(), add to event loop poll
    ///
    /// ## Parameters
    /// * `timer_id` - Unique timer identifier (from TimerId.id)
    /// * `timer` - Timer configuration with interval and callback info
    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer);

    /// Stop a timer with the given ID.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **Windows**: Use `KillTimer(hwnd, timer_id)`
    /// - **macOS**: Call `[timer invalidate]` on stored NSTimer
    /// - **X11**: Remove timer from internal manager
    /// - **Wayland**: Close timerfd with close(fd)
    ///
    /// ## Parameters
    /// * `timer_id` - Timer identifier to stop
    fn stop_timer(&mut self, timer_id: usize);

    // REQUIRED: Thread Management (Platform-Specific Implementation)

    /// Start the thread polling timer (typically 16ms interval).
    ///
    /// This timer should check all active threads for completed work and trigger
    /// event processing if any threads have finished.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **Windows**: Use `SetTimer(hwnd, 0xFFFF, 16, NULL)` with reserved ID 0xFFFF
    /// - **macOS**: Use `NSTimer::scheduledTimerWithTimeInterval` with 0.016 interval
    /// - **X11**: Add 16ms timeout to select() when threads exist
    /// - **Wayland**: Create 16ms timerfd for thread polling
    fn start_thread_poll_timer(&mut self);

    /// Stop the thread polling timer.
    ///
    /// Called when the last thread is removed from the thread pool.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **Windows**: Use `KillTimer(hwnd, 0xFFFF)`
    /// - **macOS**: Call `[timer invalidate]` on thread_timer_running
    /// - **X11**: Stop using 16ms timeout in select()
    /// - **Wayland**: Close thread polling timerfd
    fn stop_thread_poll_timer(&mut self);

    /// Add threads to the thread pool.
    ///
    /// Threads are stored in `layout_window.threads` and polled periodically by
    /// the thread polling timer to check for completion.
    ///
    /// ## Parameters
    /// * `threads` - Threads to add to the pool (BTreeMap from CallCallbacksResult)
    fn add_threads(
        &mut self,
        threads: std::collections::BTreeMap<azul_core::task::ThreadId, azul_layout::thread::Thread>,
    );

    /// Remove threads from the thread pool.
    ///
    /// ## Parameters
    /// * `thread_ids` - Thread IDs to remove
    fn remove_threads(
        &mut self,
        thread_ids: &std::collections::BTreeSet<azul_core::task::ThreadId>,
    );

    // REQUIRED: Menu Display (Platform-Specific Implementation)

    /// Show a menu at the specified position.
    ///
    /// This method is called when a callback uses `info.open_menu()` or `info.open_menu_at()`.
    /// The platform should display the menu either as a native menu or a fallback DOM-based menu
    /// depending on the window's `use_native_context_menus` flag.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **macOS**: Use NSMenu with popUpMenuPositioningItem or show fallback window
    /// - **Windows**: Use TrackPopupMenu or show fallback window
    /// - **X11**: Create GTK popup menu or show fallback window
    /// - **Wayland**: Use xdg_popup protocol or show fallback window
    ///
    /// ## Parameters
    /// * `menu` - The menu structure to display
    /// * `position` - The position where the menu should appear (logical coordinates)
    fn show_menu_from_callback(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
    );

    // REQUIRED: Tooltip Display (Platform-Specific Implementation)

    /// Show a tooltip with the given text at the specified position.
    ///
    /// This method is called when a callback uses `info.show_tooltip()` or
    /// `info.show_tooltip_at()`. The platform should display a native tooltip at the given
    /// position.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **Windows**: Use TOOLTIPS_CLASS with TTM_TRACKACTIVATE
    /// - **macOS**: Use NSPopover with NSViewController
    /// - **X11**: Create transient window with override_redirect
    /// - **Wayland**: Use zwlr_layer_shell_v1 for tooltip surface
    ///
    /// ## Parameters
    /// * `text` - The tooltip text to display
    /// * `position` - The position where the tooltip should appear (logical coordinates)
    fn show_tooltip_from_callback(
        &mut self,
        text: &str,
        position: azul_core::geom::LogicalPosition,
    );

    /// Hide the currently displayed tooltip.
    ///
    /// This method is called when a callback uses `info.hide_tooltip()`.
    /// The platform should hide any currently displayed tooltip.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **Windows**: Use TTM_TRACKACTIVATE with FALSE
    /// - **macOS**: Call [popover close]
    /// - **X11**: Unmap the tooltip window
    /// - **Wayland**: Destroy the tooltip surface
    fn hide_tooltip_from_callback(&mut self);

    /// Handle a request to begin an interactive window move.
    ///
    /// On Wayland: calls `xdg_toplevel_move(toplevel, seat, serial)` to let the
    /// compositor manage the window move. This is the only way to move windows on Wayland.
    /// On other platforms: no-op (use `set_window_position` via `ModifyWindowState` instead).
    ///
    /// Default implementation does nothing (appropriate for macOS, Win32, X11).
    fn handle_begin_interactive_move(&mut self) {
        // No-op on non-Wayland platforms
    }

    /// Synchronize the platform window properties (title, size, position, etc.)
    /// with `current_window_state`. Called after callbacks have potentially
    /// modified window state via `ModifyWindowState`.
    fn sync_window_state(&mut self);

    // PROVIDED: Hit Testing (Cross-Platform Implementation)

    /// Update hit test at given position and store in hover manager.
    ///
    /// This method performs WebRender hit testing at the given logical position
    /// and updates the HoverManager with the results. This is needed for:
    /// - Normal mouse movement events (platform calls this)
    /// - Synthetic mouse events from debug API (process_callback_result_v2 calls this)
    ///
    /// ## Parameters
    /// * `position` - The logical position to hit test at
    fn update_hit_test_at(&mut self, position: azul_core::geom::LogicalPosition) {
        use azul_core::window::CursorPosition;
        use azul_layout::managers::hover::InputPointId;

        let document_id = self.get_document_id();
        let hidpi_factor = self.get_current_window_state().size.get_hidpi_factor();

        // Get focused node before borrowing layout_window
        let focused_node = self
            .get_layout_window()
            .and_then(|lw| lw.focus_manager.get_focused_node().copied());

        // Check if layout window exists
        let has_layout_window = self.get_layout_window().is_some();
        if !has_layout_window {
            return;
        }

        // Resolve hit tester first (this mutates self.hit_tester from Requested to Resolved)
        let resolved_hit_tester = self.get_hit_tester_mut().resolve();

        // Now get layout window immutably for hit testing
        let hit_test = {
            let layout_window = self.get_layout_window().unwrap();

            crate::desktop::wr_translate2::fullhittest_new_webrender(
                &*resolved_hit_tester,
                document_id,
                focused_node,
                &layout_window.layout_results,
                &CursorPosition::InWindow(position),
                hidpi_factor,
            )
        };

        // Store hit test in hover manager
        if let Some(layout_window) = self.get_layout_window_mut() {
            layout_window
                .hover_manager
                .push_hit_test(InputPointId::Mouse, hit_test);
        }
    }

    // PROVIDED: Callback Invocation (Cross-Platform Implementation)

    /// Invoke callbacks for a given target and event filter.
    ///
    /// This method is now **provided** (cross-platform) because all required state
    /// is accessible through trait getter methods. No platform-specific code needed!
    ///
    /// ## Workflow
    /// 1. Collect callbacks from NodeData based on target (Node or RootNodes)
    /// 2. Filter callbacks by event type
    /// 3. Build an event chain from target node up to root (JS-style bubbling)
    /// 4. Invoke callbacks in bubbling order, stopping if stopPropagation() is called
    /// Dispatch events using W3C Capture→Target→Bubble propagation model.
    ///
    /// This replaces the old `invoke_callbacks_v2()` method with proper W3C event propagation:
    /// - **HoverEventFilter**: Capture→Target→Bubble through DOM tree via `propagate_event()`
    /// - **FocusEventFilter**: Fires on focused node only (no propagation)
    /// - **WindowEventFilter**: Fires on ALL nodes with matching callback (brute-force)
    ///
    /// ## Arguments
    /// * `events` - SyntheticEvents to dispatch (already filtered to user events)
    ///
    /// ## Returns
    /// * `Vec<CallCallbacksResult>` - Results from all invoked callbacks
    /// * `bool` - Whether any callback called preventDefault()
    fn dispatch_events_propagated(
        &mut self,
        events: &[azul_core::events::SyntheticEvent],
    ) -> (Vec<CallCallbacksResult>, bool) {
        use azul_core::{
            callbacks::CoreCallbackData,
            dom::{DomId, NodeId as CoreNodeId},
            events::{EventFilter, EventPhase, SyntheticEvent},
            id::NodeId,
            styled_dom::NodeHierarchyItem,
        };

        // Internal struct to track a planned callback invocation
        #[derive(Clone)]
        struct PlannedInvocation {
            dom_id: DomId,
            node_id: NodeId,
            callback_data: CoreCallbackData,
        }

        // ===================================================================
        // Phase 1: Build dispatch plan (read-only access to layout_window)
        // ===================================================================
        let planned_callbacks: Vec<PlannedInvocation> = {
            let layout_window = match self.get_layout_window() {
                Some(lw) => lw,
                None => return (Vec::new(), false),
            };

            let focused_node = layout_window.focus_manager.get_focused_node().cloned();
            let mut planned = Vec::new();

            for event in events {
                let event_filters = azul_core::events::event_type_to_filters(
                    event.event_type,
                    &event.data,
                );

                for filter in &event_filters {
                    match filter {
                        EventFilter::Hover(_) => {
                            // W3C propagation: Capture → Target → Bubble
                            let dom_id = event.target.dom;
                            let layout_result = match layout_window.layout_results.get(&dom_id) {
                                Some(lr) => lr,
                                None => continue,
                            };

                            // Build NodeHierarchy from NodeHierarchyItemVec for propagation
                            let node_hierarchy = {
                                let items = layout_result.styled_dom.node_hierarchy.as_container();
                                let nodes: Vec<azul_core::id::Node> = (0..items.len())
                                    .map(|i| {
                                        let item = &items.internal[i];
                                        azul_core::id::Node {
                                            parent: NodeId::from_usize(item.parent),
                                            previous_sibling: NodeId::from_usize(
                                                item.previous_sibling,
                                            ),
                                            next_sibling: NodeId::from_usize(item.next_sibling),
                                            last_child: NodeId::from_usize(item.last_child),
                                        }
                                    })
                                    .collect();
                                azul_core::id::NodeHierarchy::new(nodes)
                            };

                            // Build callback map: NodeId → Vec<EventFilter>
                            let node_data_container =
                                layout_result.styled_dom.node_data.as_container();
                            let mut callback_map: std::collections::BTreeMap<
                                NodeId,
                                Vec<EventFilter>,
                            > = std::collections::BTreeMap::new();

                            for node_idx in 0..node_data_container.len() {
                                let node_id = match NodeId::from_usize(node_idx + 1) {
                                    // +1 for 1-based encoding
                                    Some(nid) => nid,
                                    None => NodeId::new(node_idx),
                                };
                                // NodeId::new(idx) creates 0-based NodeId directly
                                let node_id = NodeId::new(node_idx);
                                if let Some(nd) = node_data_container.get(node_id) {
                                    let matching_filters: Vec<EventFilter> = nd
                                        .get_callbacks()
                                        .as_ref()
                                        .iter()
                                        .filter(|cb| cb.event == *filter)
                                        .map(|cb| cb.event)
                                        .collect();
                                    if !matching_filters.is_empty() {
                                        callback_map.insert(node_id, matching_filters);
                                    }
                                }
                            }

                            if callback_map.is_empty() {
                                continue;
                            }

                            // Run W3C event propagation
                            let mut event_clone = event.clone();
                            let prop_result = azul_core::events::propagate_event(
                                &mut event_clone,
                                &node_hierarchy,
                                &callback_map,
                            );

                            // Collect actual CoreCallbackData for each matched node+filter
                            for (node_id, matched_filter) in &prop_result.callbacks_to_invoke {
                                if let Some(nd) = node_data_container.get(*node_id) {
                                    for cb in nd.get_callbacks().as_ref().iter() {
                                        if cb.event == *matched_filter {
                                            planned.push(PlannedInvocation {
                                                dom_id,
                                                node_id: *node_id,
                                                callback_data: cb.clone(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        EventFilter::Focus(_) => {
                            // Focus events fire on the focused node only
                            if let Some(ref focused) = focused_node {
                                let dom_id = focused.dom;
                                if let Some(node_id) = focused.node.into_crate_internal() {
                                    if let Some(lr) =
                                        layout_window.layout_results.get(&dom_id)
                                    {
                                        let ndc = lr.styled_dom.node_data.as_container();
                                        if let Some(nd) = ndc.get(node_id) {
                                            for cb in nd.get_callbacks().as_ref().iter() {
                                                if cb.event == *filter {
                                                    planned.push(PlannedInvocation {
                                                        dom_id,
                                                        node_id,
                                                        callback_data: cb.clone(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        EventFilter::Window(_) => {
                            // Window events fire on ALL nodes with matching callback
                            for (dom_id, lr) in &layout_window.layout_results {
                                let ndc = lr.styled_dom.node_data.as_container();
                                for node_idx in 0..ndc.len() {
                                    let node_id = NodeId::new(node_idx);
                                    if let Some(nd) = ndc.get(node_id) {
                                        for cb in nd.get_callbacks().as_ref().iter() {
                                            if cb.event == *filter {
                                                planned.push(PlannedInvocation {
                                                    dom_id: *dom_id,
                                                    node_id,
                                                    callback_data: cb.clone(),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        EventFilter::Application(_) => {
                            // Application events: same as window (fire on all matching nodes)
                            for (dom_id, lr) in &layout_window.layout_results {
                                let ndc = lr.styled_dom.node_data.as_container();
                                for node_idx in 0..ndc.len() {
                                    let node_id = NodeId::new(node_idx);
                                    if let Some(nd) = ndc.get(node_id) {
                                        for cb in nd.get_callbacks().as_ref().iter() {
                                            if cb.event == *filter {
                                                planned.push(PlannedInvocation {
                                                    dom_id: *dom_id,
                                                    node_id,
                                                    callback_data: cb.clone(),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        // Not/Component filters: not used in event dispatch
                        EventFilter::Not(_) | EventFilter::Component(_) => {}
                    }
                }
            }

            planned
        };

        // ===================================================================
        // Phase 2: Invoke planned callbacks (mutable access)
        // ===================================================================
        if planned_callbacks.is_empty() {
            return (Vec::new(), false);
        }

        let mut borrows = self.prepare_callback_invocation();
        let mut results = Vec::new();
        let mut any_prevent_default = false;

        // Track propagation control flags (W3C semantics):
        //  - stop_propagation: remaining handlers on the *same* node still fire,
        //    but handlers on different nodes are skipped.
        //  - stop_immediate_propagation: no further handlers fire at all.
        let mut propagation_stopped = false;
        let mut propagation_stopped_node: Option<(DomId, NodeId)> = None;

        for planned in planned_callbacks {
            // W3C stopImmediatePropagation: break immediately
            if propagation_stopped && propagation_stopped_node.map_or(true, |(dom, nid)| {
                dom != planned.dom_id || nid != planned.node_id
            }) {
                // We crossed to a different node and stop_propagation was called → skip
                break;
            }

            let mut callback = LayoutCallback::from_core(planned.callback_data.callback);
            let callback_result = borrows.layout_window.invoke_single_callback(
                &mut callback,
                &mut planned.callback_data.refany.clone(),
                &borrows.window_handle,
                borrows.gl_context_ptr,
                borrows.image_cache,
                &mut borrows.fc_cache_clone,
                borrows.system_style.clone(),
                &ExternalSystemCallbacks::rust_internal(),
                borrows.previous_window_state,
                borrows.current_window_state,
                borrows.renderer_resources,
            );

            if callback_result.prevent_default {
                any_prevent_default = true;
            }

            // stopImmediatePropagation: break immediately, don't even run remaining same-node handlers
            if callback_result.stop_immediate_propagation {
                results.push(callback_result);
                break;
            }

            // stopPropagation: record that we should stop after remaining same-node handlers
            if callback_result.stop_propagation && !propagation_stopped {
                propagation_stopped = true;
                propagation_stopped_node = Some((planned.dom_id, planned.node_id));
            }

            results.push(callback_result);
        }

        (results, any_prevent_default)
    }

    // PROVIDED: Complete Logic (Default Implementations)

    /// GPU-accelerated smooth scrolling.
    ///
    /// This applies a scroll delta to a node and updates WebRender's display list
    /// for smooth GPU-based scrolling.
    ///
    /// ## Parameters
    /// * `dom_id` - The DOM ID containing the scrollable node
    /// * `node_id` - The scrollable node ID
    /// * `delta_x` - Horizontal scroll delta (pixels)
    /// * `delta_y` - Vertical scroll delta (pixels)
    ///
    /// ## Returns
    /// * `Ok(())` - Scroll applied successfully
    /// * `Err(msg)` - Error message if scroll failed
    fn gpu_scroll(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        delta_x: f32,
        delta_y: f32,
    ) -> Result<(), String> {
        use azul_core::{
            events::EasingFunction,
            geom::LogicalPosition,
        };

        let layout_window = self.get_layout_window_mut().ok_or("No layout window")?;

        let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();

        // Apply scroll
        layout_window.scroll_manager.scroll_by(
            dom_id,
            node_id,
            LogicalPosition::new(delta_x, delta_y),
            azul_core::task::Duration::System(
                azul_core::task::SystemTimeDiff { secs: 0, nanos: 0 },
            ),
            EasingFunction::Linear,
            (external.get_system_time_fn.cb)(),
        );

        self.mark_frame_needs_regeneration();
        Ok(())
    }

    // PROVIDED: Input Recording for Gesture Detection

    /// Record input sample for gesture detection.
    ///
    /// Call this from platform event handlers to feed input data into the gesture manager:
    /// - On mouse button down: Start new session
    /// - On mouse move (while button down): Record movement
    /// - On mouse button up: End session
    ///
    /// The gesture manager will analyze these samples to detect:
    /// - Drags (movement beyond threshold)
    /// - Double-clicks (two clicks within time/distance)
    /// - Long-presses (button held down without much movement)
    ///
    /// ## Parameters
    /// - `position`: Current mouse position in logical coordinates
    /// - `button_state`: Button state bitfield (0x01=left, 0x02=right, 0x04=middle)
    /// - `is_button_down`: Whether a button was just pressed (starts new session)
    /// - `is_button_up`: Whether a button was just released (ends session)
    fn record_input_sample(
        &mut self,
        position: azul_core::geom::LogicalPosition,
        button_state: u8,
        is_button_down: bool,
        is_button_up: bool,
        platform_screen_position: Option<azul_core::geom::LogicalPosition>,
    ) {
        // Capture window position BEFORE borrowing layout_window mutably
        let window_position = self.get_current_window_state().position;

        // Compute screen-absolute cursor position for stable drag delta.
        //
        // If the platform provides a native screen-absolute position
        // (e.g. Win32 GetCursorPos, X11 x_root/y_root), use that directly.
        // Otherwise, compute as window_pos + cursor_local_pos.
        //
        // This is stable during window drags because even though the window
        // moves (changing cursor_local), the sum always equals the true screen
        // position. The screen-space delta between first and last sample is
        // therefore immune to the feedback loop that causes "jiggling".
        let screen_position = if let Some(native_screen_pos) = platform_screen_position {
            // Platform provided native screen coords (e.g. GetCursorPos on Win32,
            // x_root/y_root on X11) - these are always correct regardless of DPI.
            native_screen_pos
        } else {
            // Fallback: compute from window position + cursor local position.
            // Correct on macOS (both are in logical points).
            // On Wayland: window_position is Uninitialized → falls back to window-local.
            match window_position {
                azul_core::window::WindowPosition::Initialized(pos) => {
                    azul_core::geom::LogicalPosition::new(
                        pos.x as f32 + position.x,
                        pos.y as f32 + position.y,
                    )
                }
                azul_core::window::WindowPosition::Uninitialized => position,
            }
        };

        // Get access to gesture manager
        let layout_window = match self.get_layout_window_mut() {
            Some(lw) => lw,
            None => return,
        };

        // Get current time (platform-specific, use system clock)
        #[cfg(feature = "std")]
        let current_time = azul_core::task::Instant::from(std::time::Instant::now());

        #[cfg(not(feature = "std"))]
        let current_time = azul_core::task::Instant::Tick(azul_core::task::SystemTick::new(0));

        let manager = &mut layout_window.gesture_drag_manager;

        // Record based on event type
        if is_button_down {
            // Start new input session — pass current window position and
            // screen-absolute cursor position for stable drag delta
            manager.start_input_session(
                position,
                current_time.clone(),
                button_state,
                window_position,
                screen_position,
            );
        } else if is_button_up {
            // End current session
            manager.end_current_session();
        } else {
            // Record ongoing movement
            manager.record_input_sample(
                position,
                current_time.clone(),
                button_state,
                screen_position,
            );
        }

        // Periodically clear old samples (every frame is fine)
        manager.clear_old_sessions(current_time);
    }

    // PROVIDED: Event Processing (Cross-Platform Implementation)

    /// V2: Record accessibility action and return affected nodes.
    ///
    /// Similar to `record_input_sample()` for gestures, this method takes an incoming
    /// accessibility action from assistive technologies (screen readers), applies
    /// necessary state changes to managers (scroll, focus, cursor, selection), and
    /// returns information about which nodes were affected.
    ///
    /// ## Workflow
    /// 1. Apply manager state changes (focus, scroll, cursor, selection)
    /// 2. Generate synthetic EventFilters for callback actions
    /// 3. Return map of affected nodes with events and dirty flags
    ///
    /// ## Parameters
    /// * `dom_id` - DOM containing the target node
    /// * `node_id` - Target node for the action
    /// * `action` - Accessibility action from screen reader
    ///
    /// ## Returns
    /// * `BTreeMap<DomNodeId, (Vec<EventFilter>, bool)>` - Map of:
    ///   - Key: Affected node
    ///   - Value: (Synthetic events to dispatch, needs_relayout flag)
    ///   - Empty map = action not applicable or nothing changed
    #[cfg(feature = "a11y")]
    fn record_accessibility_action(
        &mut self,
        dom_id: azul_core::dom::DomId,
        node_id: azul_core::dom::NodeId,
        action: azul_core::dom::AccessibilityAction,
    ) -> BTreeMap<azul_core::dom::DomNodeId, (Vec<EventFilter>, bool)> {
        use std::collections::BTreeMap;

        let layout_window = match self.get_layout_window_mut() {
            Some(lw) => lw,
            None => return BTreeMap::new(),
        };

        let now = std::time::Instant::now();

        // Delegate to LayoutWindow's process_accessibility_action
        // This has direct mutable access to all managers and returns affected nodes
        layout_window.process_accessibility_action(dom_id, node_id, action, now)
    }

    /// Process all window events using the V2 state-diffing system.
    ///
    /// V2: Main entry point for processing window events.
    ///
    /// This is the **main entry point** for event processing. Call this after updating
    /// the current window state with platform events.
    ///
    /// ## Workflow
    /// 1. Compare current vs previous window state to detect events
    /// 2. Use `dispatch_events()` to determine which callbacks to invoke
    /// 3. Invoke callbacks and collect results
    /// 4. Handle callback results (regenerate DOM, update display list, etc.)
    /// 5. Recurse if needed (DOM was regenerated)
    ///
    /// ## Returns
    /// * `ProcessEventResult` - Tells the platform what action to take (redraw, close, etc.)
    ///
    /// ## Implementation
    /// Recursively processes events with depth limiting (max 5 levels) to prevent
    /// infinite loops from callbacks that regenerate the DOM.
    fn process_window_events_recursive_v2(&mut self, depth: usize) -> ProcessEventResult {

        if depth >= MAX_EVENT_RECURSION_DEPTH {
            log_warn!(
                super::debug_server::LogCategory::EventLoop,
                "[PlatformWindowV2] Max event recursion depth {} reached",
                MAX_EVENT_RECURSION_DEPTH
            );
            return ProcessEventResult::DoNothing;
        }

        // Get previous state (or use current as fallback for first frame)
        let has_previous = self.get_previous_window_state().is_some();
        let previous_state = self
            .get_previous_window_state()
            .as_ref()
            .unwrap_or(self.get_current_window_state());

        let current_state = self.get_current_window_state();

        // DEBUG: Print state comparison for mouse buttons

        // Get gesture manager for gesture detection (if available)
        let gesture_manager = self.get_layout_window().map(|lw| &lw.gesture_drag_manager);

        // Detect all events that occurred by comparing states
        // Using new SyntheticEvent architecture with determine_all_events()

        // Get managers for event detection
        let focus_manager = self.get_layout_window().map(|w| &w.focus_manager);
        let file_drop_manager = self.get_layout_window().map(|w| &w.file_drop_manager);
        let hover_manager = self.get_layout_window().map(|w| &w.hover_manager);

        // Get EventProvider managers (text input, etc.)
        let text_manager_ref = self.get_layout_window().map(|w| &w.text_input_manager);

        // Build list of EventProvider managers
        let mut event_providers: Vec<&dyn azul_core::events::EventProvider> = Vec::new();
        if let Some(tm) = text_manager_ref.as_ref() {
            event_providers.push(*tm as &dyn azul_core::events::EventProvider);
        }

        // Get current timestamp
        #[cfg(feature = "std")]
        let timestamp = azul_core::task::Instant::from(std::time::Instant::now());
        #[cfg(not(feature = "std"))]
        let timestamp = azul_core::task::Instant::Tick(azul_core::task::SystemTick::new(0));

        // Determine all events (returns Vec<SyntheticEvent>)
        let synthetic_events = if let (Some(fm), Some(fdm), Some(hm)) =
            (focus_manager, file_drop_manager, hover_manager)
        {
            determine_all_events(
                self.get_current_window_state(),
                previous_state,
                hm,
                fm,
                fdm,
                gesture_manager,
                &event_providers,
                timestamp,
            )
        } else {
            // Fallback: no events if managers not available
            Vec::new()
        };

        if synthetic_events.is_empty() {
            return ProcessEventResult::DoNothing;
        }

        // Update active drag position with current mouse position.
        // This must happen BEFORE callbacks so titlebar_drag (and other drag
        // callbacks) see the updated DragContext.current_position.
        {
            let mouse_pos = self.get_current_window_state()
                .mouse_state.cursor_position.get_position();
            if let (Some(pos), Some(layout_window)) = (mouse_pos, self.get_layout_window_mut()) {
                if layout_window.gesture_drag_manager.is_dragging() {
                    layout_window.gesture_drag_manager.update_active_drag_positions(pos);
                }
            }
        }

        // Get mouse hit test if available (clone early to avoid borrow conflicts)
        use azul_layout::managers::hover::InputPointId;
        let hit_test_for_dispatch = self
            .get_layout_window()
            .and_then(|lw| lw.hover_manager.get_current(&InputPointId::Mouse))
            .cloned();

        // If DragStart event occurred and we have a hit test, save it in the manager
        // This allows callbacks to query which nodes were hit at drag start
        if synthetic_events
            .iter()
            .any(|e| matches!(e.event_type, azul_core::events::EventType::DragStart))
        {
            if let Some(layout_window) = self.get_layout_window_mut() {
                // Extract first hit from current state (the hovered DOM node)
                // NOTE: With the unified DragContext system, hit tests are stored
                // directly in the DragContext when the drag is activated.
                // No need for separate update_*_hit_test() calls.
                let _hit_test_clone = hit_test_for_dispatch.as_ref().and_then(|ht| {
                    // Get first hovered node's hit test
                    ht.hovered_nodes.values().next().cloned()
                });
            }
        }

        // PRE-EVENT-DISPATCH PROCESSING
        // Process input BEFORE event filtering and callback invocation.
        // This ensures framework state (scroll, text, a11y) is updated before
        // callbacks see the events.
        //
        // IMPORTANT: Hit tests must already be done by platform layer!
        // Platform code should call update_hit_test() before calling this function.
        //
        // IMPLEMENTATION STATUS:
        // [ OK ] Scroll: Platform calls scroll_manager.record_sample() in handle_scroll_wheel()
        // [ OK ] Text: Platform calls process_text_input() in handle_key_down()
        // [ WAIT ] A11y: Not yet implemented (needs a11y_manager.record_state_changes())

        // Process text input BEFORE event dispatch
        // If there's a focused contenteditable node and text input occurred,
        // apply the edit using cursor/selection managers and mark nodes dirty
        //
        // NOTE: Debug server text input is now handled via CallbackChange::CreateTextInput
        // which triggers text_input_triggered in CallCallbacksResult, processed in
        // process_callback_result_v2()
        let text_input_affected_nodes: BTreeMap<azul_core::dom::DomNodeId, (Vec<azul_core::events::EventFilter>, bool)> = if let Some(_layout_window) = self.get_layout_window_mut() {
            // TODO: Get actual text input from platform (IME, composed chars, etc.)
            // Platform layer needs to provide text_input: &str when available
            // Example integration:
            // - macOS: NSTextInputClient::insertText / setMarkedText
            // - Windows: WM_CHAR / WM_UNICHAR messages
            // - X11: XIM XLookupString with UTF-8
            // - Wayland: text-input protocol
            BTreeMap::new()
        } else {
            BTreeMap::new()
        };
        // TODO: Process accessibility events
        // if let Some(layout_window) = self.get_layout_window_mut() {
        //     layout_window.a11y_manager.record_state_changes(...);
        // }

        // PRE-CALLBACK INTERNAL EVENT FILTERING
        // Analyze events BEFORE user callbacks to extract internal system events
        // (text selection, etc.) that the framework handles.
        //
        // Managers have already been updated with current state (hit test, clicks, etc.)
        // Now we query them to detect multi-frame event patterns.

        let current_window_state = self.get_current_window_state();

        // Filter events to separate internal system events from user events
        // Query managers for state-based analysis (no local tracking needed)
        let pre_filter = if let Some(layout_window) = self.get_layout_window() {
            azul_core::events::pre_callback_filter_internal_events(
                &synthetic_events,
                hit_test_for_dispatch.as_ref(),
                &current_window_state.keyboard_state,
                &current_window_state.mouse_state,
                &layout_window.selection_manager,
                &layout_window.focus_manager,
            )
        } else {
            // No layout window - no internal events possible
            PreCallbackFilterResult {
                internal_events: Vec::new(),
                user_events: synthetic_events.clone(),
            }
        };

        // Track overall processing result
        let mut result = ProcessEventResult::DoNothing;

        // NOTE: IFrame re-invocation for scroll edge detection is now handled
        // transparently in the ScrollTo processing path (process_callback_result_v2),
        // not here via synthetic Scroll events.

        // Get external callbacks for system time
        let external = ExternalSystemCallbacks::rust_internal();

        // Process internal system events (text selection) BEFORE user callbacks
        let mut text_selection_affected_nodes = Vec::new();
        for internal_event in &pre_filter.internal_events {
            use azul_core::events::PreCallbackSystemEvent;

            match internal_event {
                PreCallbackSystemEvent::TextClick {
                    target,
                    position,
                    click_count,
                    timestamp,
                } => {
                    // Get current time using system callbacks
                    let current_instant = (external.get_system_time_fn.cb)();

                    // Calculate milliseconds since event timestamp
                    let duration_since_event = current_instant.duration_since(timestamp);
                    let current_time_ms = match duration_since_event {
                        azul_core::task::Duration::System(d) => {
                            #[cfg(feature = "std")]
                            {
                                let std_duration: std::time::Duration = d.into();
                                std_duration.as_millis() as u64
                            }
                            #[cfg(not(feature = "std"))]
                            {
                                0u64
                            }
                        }
                        azul_core::task::Duration::Tick(t) => t.tick_diff as u64,
                    };

                    // Process text selection click
                    if let Some(layout_window) = self.get_layout_window_mut() {
                        if let Some(affected_nodes) = layout_window
                            .process_mouse_click_for_selection(*position, current_time_ms)
                        {
                            text_selection_affected_nodes.extend(affected_nodes);
                        }
                    }
                }
                PreCallbackSystemEvent::TextDragSelection {
                    start_position,
                    current_position,
                    is_dragging,
                    ..
                } => {
                    // Suppress text selection if a node drag is active
                    let node_dragging = self.get_layout_window()
                        .map(|lw| lw.gesture_drag_manager.is_node_dragging_any())
                        .unwrap_or(false);

                    if *is_dragging && !node_dragging {
                        // Extend selection from start to current position
                        if let Some(layout_window) = self.get_layout_window_mut() {
                            if let Some(affected_nodes) = layout_window
                                .process_mouse_drag_for_selection(*start_position, *current_position)
                            {
                                text_selection_affected_nodes.extend(affected_nodes);
                            }
                        }
                    }
                }
                PreCallbackSystemEvent::ArrowKeyNavigation { .. } => {
                    // TODO: Implement arrow key navigation
                }
                PreCallbackSystemEvent::KeyboardShortcut { target, shortcut } => {
                    use azul_core::events::KeyboardShortcut;

                    match shortcut {
                        KeyboardShortcut::Copy => {
                            // Handle Ctrl+C: Copy selected text to clipboard
                            if let Some(layout_window) = self.get_layout_window() {
                                // TODO: Map target to correct DOM
                                let dom_id = azul_core::dom::DomId { inner: 0 };
                                if let Some(clipboard_content) =
                                    layout_window.get_selected_content_for_clipboard(&dom_id)
                                {
                                    // Copy text to system clipboard
                                    set_system_clipboard(
                                        clipboard_content.plain_text.as_str().to_string(),
                                    );
                                }
                            }
                        }
                        KeyboardShortcut::Cut => {
                            // Handle Ctrl+X: Copy to clipboard and delete selection
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                // TODO: Map target to correct DOM
                                let dom_id = azul_core::dom::DomId { inner: 0 };

                                // First, copy to clipboard
                                if let Some(clipboard_content) =
                                    layout_window.get_selected_content_for_clipboard(&dom_id)
                                {
                                    if set_system_clipboard(
                                        clipboard_content.plain_text.as_str().to_string(),
                                    ) {
                                        // Then delete the selection
                                        if let Some(affected_nodes) =
                                            layout_window.delete_selection(*target, false)
                                        {
                                            text_selection_affected_nodes.extend(affected_nodes);
                                        }
                                    }
                                }
                            }
                        }
                        KeyboardShortcut::Paste => {
                            // Handle Ctrl+V: Insert clipboard text at cursor
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                if let Some(clipboard_text) = get_system_clipboard() {
                                    // Insert text at current cursor position
                                    // TODO: Implement paste operation through TextInputManager
                                    // For now, treat it like text input
                                    let affected_nodes =
                                        layout_window.process_text_input(&clipboard_text);
                                    for (node_id, _) in affected_nodes {
                                        text_selection_affected_nodes.push(node_id);
                                    }
                                }
                            }
                        }
                        KeyboardShortcut::SelectAll => {
                            // Handle Ctrl+A: Select all text in focused node
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                // TODO: Implement select_all operation
                                // This should select all text in the focused contenteditable node
                            }
                        }
                        KeyboardShortcut::Undo | KeyboardShortcut::Redo => {
                            // Handle Ctrl+Z (Undo) / Ctrl+Y or Ctrl+Shift+Z (Redo)
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                // Convert DomNodeId to NodeId using proper decoding
                                let node_id = match target.node.into_crate_internal() {
                                    Some(id) => id,
                                    None => continue,
                                };

                                // Get external callbacks for system time
                                let external = ExternalSystemCallbacks::rust_internal();
                                let timestamp = (external.get_system_time_fn.cb)().into();

                                if *shortcut == KeyboardShortcut::Undo {
                                    // Pop from undo stack
                                    if let Some(operation) =
                                        layout_window.undo_redo_manager.pop_undo(node_id)
                                    {
                                        // Create revert changeset
                                        use azul_layout::managers::undo_redo::create_revert_changeset;
                                        let revert_changeset =
                                            create_revert_changeset(&operation, timestamp);

                                        // TODO: Allow user callback to preventDefault

                                        // Apply the revert - restore pre-state text completely
                                        let node_id_internal = target.node.into_crate_internal();
                                        if let Some(node_id_internal) = node_id_internal {
                                            // Create InlineContent from pre-state text
                                            use std::sync::Arc;

                                            use azul_layout::text3::cache::{
                                                InlineContent, StyleProperties, StyledRun,
                                            };

                                            let new_content =
                                                vec![InlineContent::Text(StyledRun {
                                                    text: operation
                                                        .pre_state
                                                        .text_content
                                                        .as_str()
                                                        .to_string(),
                                                    // TODO: Preserve original style
                                                    style: Arc::new(StyleProperties::default()),
                                                    logical_start_byte: 0,
                                                    source_node_id: None, // Undo operation - node context not available
                                                })];

                                            // Update text cache with pre-state content
                                            layout_window.update_text_cache_after_edit(
                                                target.dom,
                                                node_id_internal,
                                                new_content,
                                            );

                                            // Restore cursor position
                                            if let Some(cursor) =
                                                operation.pre_state.cursor_position.into_option()
                                            {
                                                layout_window.cursor_manager.move_cursor_to(
                                                    cursor,
                                                    target.dom,
                                                    node_id_internal,
                                                );
                                            }
                                        }

                                        // Push to redo stack after successful undo
                                        layout_window.undo_redo_manager.push_redo(operation);

                                        // Mark node for re-render
                                        text_selection_affected_nodes.push(*target);
                                    }
                                } else {
                                    // Redo operation
                                    if let Some(operation) =
                                        layout_window.undo_redo_manager.pop_redo(node_id)
                                    {
                                        // TODO: Allow user callback to preventDefault

                                        // Re-apply the original changeset by re-executing text
                                        // input
                                        let node_id_internal = target.node.into_crate_internal();
                                        if let Some(node_id_internal) = node_id_internal {
                                            // For redo, we use the text input system to re-apply
                                            // the change
                                            use azul_layout::managers::changeset::TextOperation;

                                            // Determine what to re-apply based on the operation
                                            match &operation.changeset.operation {
                                                TextOperation::InsertText(op) => {
                                                    // Re-insert the text via process_text_input
                                                    let affected =
                                                        layout_window.process_text_input(&op.text);
                                                    for (node, _) in affected {
                                                        text_selection_affected_nodes.push(node);
                                                    }
                                                }
                                                _ => {
                                                    // For other operations, just mark for re-render
                                                    // Full implementation would handle each
                                                    // operation type
                                                }
                                            }
                                        }

                                        // Push to undo stack after successful redo
                                        layout_window.undo_redo_manager.push_undo(operation);

                                        // Mark node for re-render
                                        text_selection_affected_nodes.push(*target);
                                    }
                                }
                            }
                        }
                    }
                }
                PreCallbackSystemEvent::DeleteSelection { target, forward } => {
                    // Handle Backspace/Delete key
                    // For now, we directly call delete_selection
                    // TODO: Integrate with TextInputManager changeset system
                    // This should:
                    // 1. Create DeleteText changeset
                    // 2. Fire On::TextInput callback with preventDefault support
                    // 3. Apply deletion if !preventDefault
                    // 4. Record to undo stack
                    if let Some(layout_window) = self.get_layout_window_mut() {
                        if let Some(affected_nodes) =
                            layout_window.delete_selection(*target, *forward)
                        {
                            text_selection_affected_nodes.extend(affected_nodes);
                        }
                    }
                }
            }
        }

        // If text selection changed, mark for re-render
        if !text_selection_affected_nodes.is_empty() {
            result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // EVENT FILTERING AND CALLBACK DISPATCH (W3C Propagation Model)

        // Capture focus state before callbacks for post-callback filtering
        let old_focus = self
            .get_layout_window()
            .and_then(|lw| lw.focus_manager.get_focused_node().copied());

        // Dispatch user events using W3C Capture→Target→Bubble propagation
        let (callback_results, prevent_default) =
            self.dispatch_events_propagated(&pre_filter.user_events);

        // Process all callback results
        let mut should_recurse = false;
        let mut focus_changed = false;

        for callback_result in &callback_results {
            let event_result = self.process_callback_result_v2(callback_result);
            result = result.max(event_result);

            // Check if we need to recurse (DOM was regenerated)
            use azul_core::callbacks::Update;
            if matches!(
                callback_result.callbacks_update_screen,
                Update::RefreshDom | Update::RefreshDomAllWindows
            ) {
                should_recurse = true;
            }
        }

        // AUTO-ACTIVATE NODE DRAG
        // If a DragStart event was dispatched and the deepest hit node has draggable=true,
        // automatically activate the node drag in the gesture manager.
        // This is needed because activate_node_drag() is not called by user code.
        let had_drag_start = pre_filter.user_events.iter().any(|e| {
            matches!(e.event_type, azul_core::events::EventType::DragStart)
        });

        if had_drag_start {
            // Find the deepest hit node and check if it (or an ancestor) has draggable=true
            if let Some(layout_window) = self.get_layout_window_mut() {
                use azul_layout::managers::hover::InputPointId;
                let hit_test = layout_window.hover_manager
                    .get_current(&InputPointId::Mouse)
                    .cloned();

                if let Some(hit_test) = hit_test {
                    let mut activated = false;
                    'outer: for (dom_id, hit_test_data) in &hit_test.hovered_nodes {
                        if let Some(layout_result) = layout_window.layout_results.get(dom_id) {
                            let node_data_container = layout_result.styled_dom.node_data.as_container();
                            let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();

                            // Find deepest hit node
                            let deepest_node = hit_test_data
                                .regular_hit_test_nodes
                                .iter()
                                .max_by_key(|(node_id, _)| {
                                    let mut depth = 0usize;
                                    let mut current = Some(**node_id);
                                    while let Some(nid) = current {
                                        depth += 1;
                                        current = node_hierarchy.get(nid).and_then(|h| h.parent_id());
                                    }
                                    depth
                                });

                            if let Some((target_node_id, _)) = deepest_node {
                                // Walk from deepest to root, find first draggable=true
                                let mut current = Some(*target_node_id);
                                while let Some(node_id) = current {
                                    if let Some(node_data) = node_data_container.get(node_id) {
                                        let is_draggable = node_data.attributes.as_ref().iter().any(|attr| {
                                            matches!(attr, azul_core::dom::AttributeType::Draggable(true))
                                        });
                                        if is_draggable {
                                            let drag_data = azul_core::drag::DragData::new();
                                            layout_window.gesture_drag_manager.activate_node_drag(
                                                *dom_id,
                                                node_id,
                                                drag_data,
                                                None,
                                            );
                                            activated = true;
                                            break 'outer;
                                        }
                                    }
                                    current = node_hierarchy.get(node_id).and_then(|h| h.parent_id());
                                }
                            }
                        }
                    }
                    // If no draggable=true node found, activate as window drag
                    // This enables CSD titlebar drag and other window-move operations
                    if !activated {
                        let win_pos = self.get_current_window_state().position.clone();
                        // Re-borrow after get_current_window_state
                        if let Some(layout_window) = self.get_layout_window_mut() {
                            layout_window.gesture_drag_manager.activate_window_drag(
                                win_pos,
                                None,
                            );
                        }
                    }
                }
            }
        }

        // SYNC DRAG-DROP MANAGER AND SET :dragging PSEUDO-STATE
        // After auto-activating node drag, sync the DragDropManager and set
        // the :dragging CSS pseudo-state on the source node for styling.
        if had_drag_start {
            if let Some(layout_window) = self.get_layout_window_mut() {
                // Sync DragDropManager from GestureAndDragManager
                if let Some(ctx) = layout_window.gesture_drag_manager.get_drag_context() {
                    layout_window.drag_drop_manager.active_drag = Some(ctx.clone());
                }

                // Set :dragging pseudo-state on the source node
                if let Some(ctx) = layout_window.gesture_drag_manager.get_drag_context() {
                    if let Some(node_drag) = ctx.as_node_drag() {
                        let dom_id = node_drag.dom_id;
                        let node_id = node_drag.node_id;
                        if let Some(layout_result) = layout_window.layout_results.get_mut(&dom_id) {
                            let mut styled_nodes = layout_result.styled_dom.styled_nodes.as_container_mut();
                            if let Some(styled_node) = styled_nodes.get_mut(node_id) {
                                styled_node.styled_node_state.dragging = true;
                            }
                        }

                        // Add GPU transform key for the dragged node so it can be
                        // visually moved via GPU-accelerated transform during drag.
                        // The display list will include a PushReferenceFrame for this node.
                        let gpu_cache = layout_window.gpu_state_manager.get_or_create_cache(dom_id);
                        if !gpu_cache.transform_keys.contains_key(&node_id) {
                            let transform_key = azul_core::resources::TransformKey::unique();
                            let identity = azul_core::transform::ComputedTransform3D::IDENTITY;
                            gpu_cache.transform_keys.insert(node_id, transform_key);
                            gpu_cache.current_transform_values.insert(node_id, identity);
                        }
                    }
                }
            }
            // DragStart should trigger re-render to show :dragging pseudo-state
            result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // SET :drag-over PSEUDO-STATE ON DragEnter / DragLeave TARGETS
        // When a dragged item enters a new node, set :drag-over on it;
        // when it leaves, clear :drag-over. These events are synthesized
        // by event_determination.rs with the correct target nodes.
        {
            let mut had_drag_over_change = false;

            for event in &pre_filter.user_events {
                match event.event_type {
                    azul_core::events::EventType::DragEnter => {
                        let target = &event.target;
                        if let Some(target_node_id) = target.node.into_crate_internal() {
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                if let Some(layout_result) = layout_window.layout_results.get_mut(&target.dom) {
                                    let mut styled_nodes = layout_result.styled_dom.styled_nodes.as_container_mut();
                                    if let Some(styled_node) = styled_nodes.get_mut(target_node_id) {
                                        styled_node.styled_node_state.drag_over = true;
                                        had_drag_over_change = true;
                                    }
                                }

                                // Update current_drop_target in the drag context
                                if let Some(ctx) = layout_window.gesture_drag_manager.get_drag_context_mut() {
                                    if let Some(node_drag) = ctx.as_node_drag_mut() {
                                        node_drag.previous_drop_target = node_drag.current_drop_target.clone();
                                        node_drag.current_drop_target = azul_core::dom::OptionDomNodeId::Some(target.clone());
                                    }
                                }
                            }
                        }
                    }
                    azul_core::events::EventType::DragLeave => {
                        let target = &event.target;
                        if let Some(target_node_id) = target.node.into_crate_internal() {
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                if let Some(layout_result) = layout_window.layout_results.get_mut(&target.dom) {
                                    let mut styled_nodes = layout_result.styled_dom.styled_nodes.as_container_mut();
                                    if let Some(styled_node) = styled_nodes.get_mut(target_node_id) {
                                        styled_node.styled_node_state.drag_over = false;
                                        had_drag_over_change = true;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            if had_drag_over_change {
                // :drag-over change requires re-render to update visual appearance
                result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
            }
        }

        // FORCE RE-RENDER DURING ACTIVE DRAG
        // When dragging is active, every mouse move must trigger a re-render so the
        // dragged node's visual position is updated via GPU transform.
        if let Some(layout_window) = self.get_layout_window_mut() {
            if layout_window.gesture_drag_manager.is_node_dragging_any() {
                // Update the GPU transform value for the dragged node
                if let Some(ctx) = layout_window.gesture_drag_manager.get_drag_context() {
                    if let Some(node_drag) = ctx.as_node_drag() {
                        let dom_id = node_drag.dom_id;
                        let node_id = node_drag.node_id;
                        let delta_x = ctx.current_position().x - ctx.start_position().x;
                        let delta_y = ctx.current_position().y - ctx.start_position().y;
                        let gpu_cache = layout_window.gpu_state_manager.get_or_create_cache(dom_id);
                        let new_transform = azul_core::transform::ComputedTransform3D::new_translation(
                            delta_x, delta_y, 0.0
                        );
                        gpu_cache.current_transform_values.insert(node_id, new_transform);
                    }
                }
                result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
            }
        }

        // AUTO-DEACTIVATE DRAG ON DRAG END
        let had_drag_end = pre_filter.user_events.iter().any(|e| {
            matches!(e.event_type, azul_core::events::EventType::DragEnd)
        });
        if had_drag_end {
            if let Some(layout_window) = self.get_layout_window_mut() {
                // Clear :dragging pseudo-state on the source node BEFORE ending drag
                if let Some(ctx) = layout_window.gesture_drag_manager.get_drag_context() {
                    if let Some(node_drag) = ctx.as_node_drag() {
                        let dom_id = node_drag.dom_id;
                        let node_id = node_drag.node_id;
                        if let Some(layout_result) = layout_window.layout_results.get_mut(&dom_id) {
                            let mut styled_nodes = layout_result.styled_dom.styled_nodes.as_container_mut();
                            if let Some(styled_node) = styled_nodes.get_mut(node_id) {
                                styled_node.styled_node_state.dragging = false;
                            }
                        }
                    }
                }

                // Clear :drag-over pseudo-state on any current drop target
                if let Some(ctx) = layout_window.gesture_drag_manager.get_drag_context() {
                    if let Some(node_drag) = ctx.as_node_drag() {
                        if let azul_core::dom::OptionDomNodeId::Some(drop_target) = &node_drag.current_drop_target {
                            let dom_id = drop_target.dom;
                            if let Some(target_node_id) = drop_target.node.into_crate_internal() {
                                if let Some(layout_result) = layout_window.layout_results.get_mut(&dom_id) {
                                    let mut styled_nodes = layout_result.styled_dom.styled_nodes.as_container_mut();
                                    if let Some(styled_node) = styled_nodes.get_mut(target_node_id) {
                                        styled_node.styled_node_state.drag_over = false;
                                    }
                                }
                            }
                        }
                    }
                }

                // Remove GPU transform key for the dragged node on drag end
                if let Some(ctx) = layout_window.gesture_drag_manager.get_drag_context() {
                    if let Some(node_drag) = ctx.as_node_drag() {
                        let dom_id = node_drag.dom_id;
                        let node_id = node_drag.node_id;
                        let gpu_cache = layout_window.gpu_state_manager.get_or_create_cache(dom_id);
                        gpu_cache.transform_keys.remove(&node_id);
                        gpu_cache.current_transform_values.remove(&node_id);
                    }
                }

                if layout_window.gesture_drag_manager.is_dragging() {
                    layout_window.gesture_drag_manager.end_drag();
                }

                // Sync: also clear DragDropManager
                layout_window.drag_drop_manager.active_drag = None;
            }
        }

        // POST-CALLBACK INTERNAL EVENT FILTERING
        // Process callback results to determine what internal processing continues

        let new_focus = self
            .get_layout_window()
            .and_then(|lw| lw.focus_manager.get_focused_node().copied());

        let post_filter = azul_core::events::post_callback_filter_internal_events(
            prevent_default,
            &pre_filter.internal_events,
            old_focus,
            new_focus,
        );

        // Process system events returned from post-callback filter
        for system_event in &post_filter.system_events {
            match system_event {
                azul_core::events::PostCallbackSystemEvent::FocusChanged => {
                    focus_changed = true;
                }
                azul_core::events::PostCallbackSystemEvent::ApplyTextInput => {
                    // Text input will be applied below
                }
                azul_core::events::PostCallbackSystemEvent::ApplyTextChangeset => {
                    // TODO: Apply text changesets from Phase 2 refactoring
                    // This will be implemented when changesets are fully integrated
                }
                azul_core::events::PostCallbackSystemEvent::ScrollIntoView => {
                    // Scroll cursor/selection into view after text change
                    if let Some(layout_window) = self.get_layout_window_mut() {
                        use azul_layout::window::{ScrollMode, SelectionScrollType};

                        // Determine what to scroll based on focus manager state
                        let scroll_type =
                            if let Some(focused_node) = layout_window.focus_manager.focused_node {
                                // Check if focused node has a text cursor or selection
                                if layout_window
                                    .selection_manager
                                    .get_selection(&focused_node.dom)
                                    .is_some()
                                {
                                    SelectionScrollType::Selection
                                } else {
                                    SelectionScrollType::Cursor
                                }
                            } else {
                                // No focus, nothing to scroll
                                continue;
                            };

                        // Scroll with instant mode (user-initiated action, not auto-scroll)
                        layout_window.scroll_selection_into_view(scroll_type, ScrollMode::Instant);

                        // Mark for re-render since scrolling changed viewport
                        result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                    }
                }
                azul_core::events::PostCallbackSystemEvent::StartAutoScrollTimer => {
                    // Start auto-scroll timer for drag-to-scroll
                    // Timer frequency matches monitor refresh rate for smooth scrolling

                    if let Some(layout_window) = self.get_layout_window() {
                        let timer_id = azul_core::task::DRAG_AUTOSCROLL_TIMER_ID;

                        // Check if timer already running (avoid duplicate timers)
                        if !layout_window.timers.contains_key(&timer_id) {
                            use azul_core::{
                                refany::RefAny,
                                task::{Duration as AzulDuration, SystemTimeDiff},
                            };
                            use azul_layout::timer::{Timer, TimerCallbackType};

                            const DEFAULT_REFRESH_RATE_HZ: u32 = 60;
                            let frame_time_nanos = 1_000_000_000 / DEFAULT_REFRESH_RATE_HZ;

                            let external = ExternalSystemCallbacks::rust_internal();

                            let timer = Timer::create(
                                RefAny::new(()), // Empty data
                                auto_scroll_timer_callback as TimerCallbackType,
                                external.get_system_time_fn,
                            )
                            .with_interval(AzulDuration::System(SystemTimeDiff {
                                secs: 0,
                                nanos: frame_time_nanos,
                            }));

                            if let Some(layout_window) = self.get_layout_window_mut() {
                                layout_window.add_timer(timer_id, timer.clone());
                                self.start_timer(azul_core::task::DRAG_AUTOSCROLL_TIMER_ID.id, timer);
                                result =
                                    result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                            }
                        }
                    }
                }
                azul_core::events::PostCallbackSystemEvent::CancelAutoScrollTimer => {
                    // Cancel auto-scroll timer
                    let timer_id = azul_core::task::DRAG_AUTOSCROLL_TIMER_ID;

                    if let Some(layout_window) = self.get_layout_window_mut() {
                        if layout_window.timers.contains_key(&timer_id) {
                            layout_window.remove_timer(&timer_id);
                            self.stop_timer(azul_core::task::DRAG_AUTOSCROLL_TIMER_ID.id);
                        }
                    }
                }
            }
        }

        // POST-CALLBACK TEXT INPUT PROCESSING
        // Apply text changeset if preventDefault was not set.
        // This is where we:
        // 1. Compute and cache the text changes (reshape glyphs)
        // 2. Scroll cursor into view if needed
        // 3. Mark dirty nodes for re-layout
        // 4. Potentially trigger another event cycle if scrolling occurred

        let should_apply_text_input = post_filter
            .system_events
            .contains(&azul_core::events::PostCallbackSystemEvent::ApplyTextInput);

        if should_apply_text_input && !text_input_affected_nodes.is_empty() {
            if let Some(layout_window) = self.get_layout_window_mut() {
                // Apply text changes and get list of dirty nodes
                let dirty_nodes = layout_window.apply_text_changeset();

                // Mark dirty nodes for re-layout
                for node in dirty_nodes {
                    // TODO: Mark node as needing re-layout
                    // This will be handled by the existing dirty tracking system
                    let _ = node;
                }

                // Request re-render since text changed
                result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
            }

            // After text changes, scroll cursor into view if we have a focused text input
            // Note: This needs to happen AFTER relayout to get accurate cursor position
            if let Some(layout_window) = self.get_layout_window() {
                if let Some(cursor_rect) = layout_window.get_focused_cursor_rect() {
                    // Get the focused node to find its scroll container
                    if let Some(focused_node_id) = layout_window.focus_manager.focused_node {
                        // Find the nearest scrollable ancestor
                        if let Some(scroll_container) =
                            layout_window.find_scrollable_ancestor(focused_node_id)
                        {
                            // Get the scroll state for this container
                            let scroll_node_id = scroll_container.node.into_crate_internal();
                            if let Some(scroll_node_id) = scroll_node_id {
                                if let Some(scroll_state) = layout_window
                                    .scroll_manager
                                    .get_scroll_state(scroll_container.dom, scroll_node_id)
                                {
                                    // Get the container's layout rect
                                    if let Some(container_rect) =
                                        layout_window.get_node_layout_rect(scroll_container)
                                    {
                                        // Calculate the visible area (container rect adjusted by
                                        // scroll offset)
                                        let visible_area = azul_core::geom::LogicalRect::new(
                                            azul_core::geom::LogicalPosition::new(
                                                container_rect.origin.x
                                                    + scroll_state.current_offset.x,
                                                container_rect.origin.y
                                                    + scroll_state.current_offset.y,
                                            ),
                                            container_rect.size,
                                        );

                                        // Add padding around cursor for comfortable visibility
                                        const SCROLL_PADDING: f32 = 5.0;

                                        // Calculate how much to scroll to bring cursor into view
                                        let mut scroll_delta =
                                            azul_core::geom::LogicalPosition::zero();

                                        // Check horizontal overflow
                                        if cursor_rect.origin.x
                                            < visible_area.origin.x + SCROLL_PADDING
                                        {
                                            // Cursor is too far left
                                            scroll_delta.x = cursor_rect.origin.x
                                                - (visible_area.origin.x + SCROLL_PADDING);
                                        } else if cursor_rect.origin.x + cursor_rect.size.width
                                            > visible_area.origin.x + visible_area.size.width
                                                - SCROLL_PADDING
                                        {
                                            // Cursor is too far right
                                            scroll_delta.x = (cursor_rect.origin.x
                                                + cursor_rect.size.width)
                                                - (visible_area.origin.x + visible_area.size.width
                                                    - SCROLL_PADDING);
                                        }

                                        // Check vertical overflow
                                        if cursor_rect.origin.y
                                            < visible_area.origin.y + SCROLL_PADDING
                                        {
                                            // Cursor is too far up
                                            scroll_delta.y = cursor_rect.origin.y
                                                - (visible_area.origin.y + SCROLL_PADDING);
                                        } else if cursor_rect.origin.y + cursor_rect.size.height
                                            > visible_area.origin.y + visible_area.size.height
                                                - SCROLL_PADDING
                                        {
                                            // Cursor is too far down
                                            scroll_delta.y = (cursor_rect.origin.y
                                                + cursor_rect.size.height)
                                                - (visible_area.origin.y
                                                    + visible_area.size.height
                                                    - SCROLL_PADDING);
                                        }

                                        // Apply scroll if needed
                                        if scroll_delta.x != 0.0 || scroll_delta.y != 0.0 {
                                            // Get current time from system callbacks
                                            let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
                                            let now = (external.get_system_time_fn.cb)();

                                            if let Some(layout_window_mut) =
                                                self.get_layout_window_mut()
                                            {
                                                // Instant scroll (duration = 0) for cursor
                                                // scrolling
                                                layout_window_mut.scroll_manager.scroll_by(
                                                    scroll_container.dom,
                                                    scroll_node_id,
                                                    scroll_delta,
                                                    std::time::Duration::from_millis(0).into(),
                                                    azul_core::events::EasingFunction::Linear,
                                                    now.into(),
                                                );
                                                // Scrolling may trigger more events, so recurse
                                                result = result.max(
                                                    ProcessEventResult::ShouldReRenderCurrentWindow,
                                                );
                                                should_recurse = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // MOUSE CLICK-TO-FOCUS (W3C default behavior)
        // When the user clicks on a focusable element, focus should move to that element.
        // We check for MouseDown events and find the deepest focusable ancestor.
        let mut mouse_click_focus_changed = false;
        if !prevent_default {
            let has_mouse_down = synthetic_events.iter().any(|e| {
                matches!(e.event_type, azul_core::events::EventType::MouseDown)
            });

            if has_mouse_down {
                // Get the hit test data to find which node was clicked
                if let Some(ref hit_test) = hit_test_for_dispatch {
                    // Find the deepest focusable node in the hit chain
                    let mut clicked_focusable_node: Option<azul_core::dom::DomNodeId> = None;

                    for (dom_id, hit_test_data) in &hit_test.hovered_nodes {
                        // Find deepest hit node first
                        let deepest = hit_test_data.regular_hit_test_nodes
                            .iter()
                            .max_by_key(|(_, hit_item)| {
                                // Higher hit_depth = further from camera, so we want lowest
                                // But we actually want the topmost (frontmost) which is depth 0
                                std::cmp::Reverse(hit_item.hit_depth)
                            });

                        if let Some((node_id, _)) = deepest {
                            if let Some(layout_window) = self.get_layout_window() {
                                if let Some(layout_result) = layout_window.layout_results.get(dom_id) {
                                    let node_data = layout_result.styled_dom.node_data.as_container();
                                    let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();

                                    // Walk from clicked node to root, find first focusable
                                    let mut current = Some(*node_id);
                                    while let Some(nid) = current {
                                        if let Some(nd) = node_data.get(nid) {
                                            if nd.is_focusable() {
                                                clicked_focusable_node = Some(azul_core::dom::DomNodeId {
                                                    dom: *dom_id,
                                                    node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(nid)),
                                                });
                                                break;
                                            }
                                        }
                                        current = node_hierarchy.get(nid).and_then(|h| h.parent_id());
                                    }
                                }
                            }
                        }
                    }

                    // If we found a focusable node, set focus to it
                    if let Some(new_focus) = clicked_focusable_node {
                        let old_focus_node_id = old_focus.and_then(|f| f.node.into_crate_internal());
                        let new_focus_node_id = new_focus.node.into_crate_internal();

                        // Only change focus if clicking on a different node
                        if old_focus_node_id != new_focus_node_id {
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                layout_window.focus_manager.set_focused_node(Some(new_focus));
                                mouse_click_focus_changed = true;

                                // SCROLL INTO VIEW: Scroll newly focused node into visible area
                                use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;
                                let now = azul_core::task::Instant::now();
                                layout_window.scroll_node_into_view(
                                    new_focus,
                                    ScrollIntoViewOptions::nearest(),
                                    now,
                                );

                                // RESTYLE: Update StyledNodeState and compute CSS changes
                                let restyle_result = apply_focus_restyle(
                                    layout_window,
                                    old_focus_node_id,
                                    new_focus_node_id,
                                );
                                result = result.max(restyle_result);
                            }

                            log_debug!(
                                super::debug_server::LogCategory::Input,
                                "[Event V2] Click-to-focus: {:?} -> {:?}",
                                old_focus,
                                new_focus
                            );
                        }
                    }
                }
            }
        }

        // KEYBOARD DEFAULT ACTIONS (Tab navigation, Enter/Space activation, Escape)
        // Process keyboard default actions if not prevented by callbacks
        // This implements W3C focus navigation and element activation behavior
        let mut default_action_focus_changed = false;
        let mut synthetic_click_target: Option<azul_core::dom::DomNodeId> = None;

        if !prevent_default {
            // Check if we have a keyboard event (KeyDown specifically)
            let has_key_event = pre_filter.user_events.iter().any(|e| {
                matches!(e.event_type, azul_core::events::EventType::KeyDown)
            });

            if has_key_event {
                // Get keyboard state and focused node for default action determination
                let keyboard_state = &self.get_current_window_state().keyboard_state;
                let focused_node = old_focus;

                // Get layout results for querying node properties
                let layout_results = self.get_layout_window()
                    .map(|lw| &lw.layout_results);

                if let Some(layout_results) = layout_results {
                    // Determine what default action should occur
                    let default_action_result = azul_layout::default_actions::determine_keyboard_default_action(
                        keyboard_state,
                        focused_node,
                        layout_results,
                        prevent_default,
                    );

                    // Process the default action if not prevented
                    if default_action_result.has_action() {
                        use azul_core::events::DefaultAction;
                        use azul_core::callbacks::FocusTarget;
                        use azul_layout::managers::focus_cursor::resolve_focus_target;

                        match &default_action_result.action {
                            DefaultAction::FocusNext | DefaultAction::FocusPrevious |
                            DefaultAction::FocusFirst | DefaultAction::FocusLast => {
                                // Convert DefaultAction to FocusTarget
                                let focus_target = azul_layout::default_actions::default_action_to_focus_target(&default_action_result.action);

                                if let Some(focus_target) = focus_target {
                                    // Resolve the focus target to an actual node
                                    let resolve_result = resolve_focus_target(
                                        &focus_target,
                                        layout_results,
                                        focused_node,
                                    );

                                    if let Ok(new_focus_node) = resolve_result {
                                        // Get the old focus node ID for restyle
                                        let old_focus_node_id = focused_node.and_then(|f| f.node.into_crate_internal());
                                        let new_focus_node_id = new_focus_node.and_then(|f| f.node.into_crate_internal());

                                        // Update focus manager and get timer action
                                        let timer_action = if let Some(layout_window) = self.get_layout_window_mut() {
                                            layout_window.focus_manager.set_focused_node(new_focus_node);
                                            default_action_focus_changed = true;

                                            // SCROLL INTO VIEW: Scroll newly focused node into visible area
                                            if let Some(focus_node) = new_focus_node {
                                                use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;
                                                let now = azul_core::task::Instant::now();
                                                layout_window.scroll_node_into_view(
                                                    focus_node,
                                                    ScrollIntoViewOptions::nearest(),
                                                    now,
                                                );
                                            }

                                            // CURSOR BLINK TIMER: Start/stop timer based on contenteditable focus
                                            let window_state = layout_window.current_window_state.clone();
                                            let timer_action = layout_window.handle_focus_change_for_cursor_blink(
                                                new_focus_node,
                                                &window_state,
                                            );

                                            // RESTYLE: Update StyledNodeState and compute CSS changes
                                            if old_focus_node_id != new_focus_node_id {
                                                let restyle_result = apply_focus_restyle(
                                                    layout_window,
                                                    old_focus_node_id,
                                                    new_focus_node_id,
                                                );
                                                result = result.max(restyle_result);
                                            } else {
                                                result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                                            }

                                            Some(timer_action)
                                        } else {
                                            None
                                        };

                                        // Apply timer action outside the layout_window borrow
                                        if let Some(timer_action) = timer_action {
                                            match timer_action {
                                                azul_layout::CursorBlinkTimerAction::Start(timer) => {
                                                    self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                                                }
                                                azul_layout::CursorBlinkTimerAction::Stop => {
                                                    self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                                                }
                                                azul_layout::CursorBlinkTimerAction::NoChange => {}
                                            }
                                        }

                                        log_debug!(
                                            super::debug_server::LogCategory::Input,
                                            "[Event V2] Default action: {:?} -> {:?}",
                                            default_action_result.action,
                                            new_focus_node
                                        );
                                    }
                                }
                            }

                            DefaultAction::ClearFocus => {
                                // Clear focus (Escape key)
                                // Get old focus before clearing
                                let old_focus_node_id = old_focus.and_then(|f| f.node.into_crate_internal());

                                let timer_action = if let Some(layout_window) = self.get_layout_window_mut() {
                                    layout_window.focus_manager.set_focused_node(None);
                                    default_action_focus_changed = true;

                                    // CURSOR BLINK TIMER: Stop timer when focus is cleared
                                    let window_state = layout_window.current_window_state.clone();
                                    let timer_action = layout_window.handle_focus_change_for_cursor_blink(
                                        None,
                                        &window_state,
                                    );

                                    // RESTYLE: Update StyledNodeState when focus is cleared
                                    if old_focus_node_id.is_some() {
                                        let restyle_result = apply_focus_restyle(
                                            layout_window,
                                            old_focus_node_id,
                                            None,
                                        );
                                        result = result.max(restyle_result);
                                    } else {
                                        result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                                    }

                                    Some(timer_action)
                                } else {
                                    None
                                };

                                // Apply timer action outside the layout_window borrow
                                if let Some(timer_action) = timer_action {
                                    match timer_action {
                                        azul_layout::CursorBlinkTimerAction::Start(_) => {}
                                        azul_layout::CursorBlinkTimerAction::Stop => {
                                            self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                                        }
                                        azul_layout::CursorBlinkTimerAction::NoChange => {}
                                    }
                                }

                                log_debug!(
                                    super::debug_server::LogCategory::Input,
                                    "[Event V2] Default action: ClearFocus"
                                );
                            }

                            DefaultAction::ActivateFocusedElement { target } => {
                                // Queue synthetic click for later dispatch
                                synthetic_click_target = Some(target.clone());

                                log_debug!(
                                    super::debug_server::LogCategory::Input,
                                    "[Event V2] Default action: ActivateFocusedElement -> {:?}",
                                    target
                                );
                            }

                            DefaultAction::ScrollFocusedContainer { direction, amount } => {
                                // TODO: Implement keyboard scrolling
                                log_debug!(
                                    super::debug_server::LogCategory::Input,
                                    "[Event V2] Default action: ScrollFocusedContainer {:?} {:?} (not yet implemented)",
                                    direction,
                                    amount
                                );
                            }

                            DefaultAction::None => {}

                            // Additional default actions not yet implemented
                            DefaultAction::SubmitForm { .. } |
                            DefaultAction::CloseModal { .. } |
                            DefaultAction::SelectAllText => {
                                // These are placeholder for future implementation
                            }
                        }
                    }
                }
            }
        }

        // SYNTHETIC CLICK DISPATCH (for Enter/Space activation)
        // Process synthetic clicks from keyboard activation
        if let Some(click_target) = synthetic_click_target {
            if depth + 1 < MAX_EVENT_RECURSION_DEPTH {
                // Create a SyntheticEvent for the click and dispatch through propagation
                let click_event = azul_core::events::SyntheticEvent::new(
                    azul_core::events::EventType::Click,
                    azul_core::events::EventSource::User,
                    click_target,
                    {
                        #[cfg(feature = "std")]
                        { azul_core::task::Instant::from(std::time::Instant::now()) }
                        #[cfg(not(feature = "std"))]
                        { azul_core::task::Instant::Tick(azul_core::task::SystemTick::new(0)) }
                    },
                    azul_core::events::EventData::None,
                );

                let (click_results, _) = self.dispatch_events_propagated(&[click_event]);

                for callback_result in &click_results {
                    let event_result = self.process_callback_result_v2(callback_result);
                    result = result.max(event_result);

                    use azul_core::callbacks::Update;
                    if matches!(
                        callback_result.callbacks_update_screen,
                        Update::RefreshDom | Update::RefreshDomAllWindows
                    ) {
                        should_recurse = true;
                    }
                }

                log_debug!(
                    super::debug_server::LogCategory::Input,
                    "[Event V2] Dispatched synthetic click for element activation: {:?}",
                    click_target
                );
            }
        }

        // Handle focus changes: generate synthetic FocusIn/FocusOut events
        log_debug!(
            super::debug_server::LogCategory::Input,
            "[Event V2] Focus check: focus_changed={}, default_action_focus_changed={}, mouse_click_focus_changed={}, depth={}, old_focus={:?}",
            focus_changed,
            default_action_focus_changed,
            mouse_click_focus_changed,
            depth,
            old_focus
        );

        if (focus_changed || default_action_focus_changed || mouse_click_focus_changed) && depth + 1 < MAX_EVENT_RECURSION_DEPTH {
            // Get the new focus BEFORE clearing selections
            let new_focus = self
                .get_layout_window()
                .and_then(|lw| lw.focus_manager.get_focused_node().copied());

            log_debug!(
                super::debug_server::LogCategory::Input,
                "[Event V2] Focus changed! old_focus={:?}, new_focus={:?}",
                old_focus,
                new_focus
            );

            // Clear selections when focus changes (standard UI behavior)
            if let Some(layout_window) = self.get_layout_window_mut() {
                layout_window.selection_manager.clear_all();
            }

            // DISPATCH FOCUS CALLBACKS: FocusLost on old node, FocusReceived on new node
            // Create synthetic focus events and dispatch through propagation
            {
                let now = {
                    #[cfg(feature = "std")]
                    { azul_core::task::Instant::from(std::time::Instant::now()) }
                    #[cfg(not(feature = "std"))]
                    { azul_core::task::Instant::Tick(azul_core::task::SystemTick::new(0)) }
                };

                let mut focus_events = Vec::new();

                // FocusLost (Blur) on old node
                if let Some(old_node) = old_focus {
                    log_debug!(
                        super::debug_server::LogCategory::Input,
                        "[Event V2] Dispatching FocusLost to node {:?}",
                        old_node
                    );
                    focus_events.push(azul_core::events::SyntheticEvent::new(
                        azul_core::events::EventType::Blur,
                        azul_core::events::EventSource::User,
                        old_node,
                        now.clone(),
                        azul_core::events::EventData::None,
                    ));
                }

                // FocusReceived on new node
                if let Some(new_node) = new_focus {
                    log_debug!(
                        super::debug_server::LogCategory::Input,
                        "[Event V2] Dispatching FocusReceived to node {:?}",
                        new_node
                    );
                    focus_events.push(azul_core::events::SyntheticEvent::new(
                        azul_core::events::EventType::Focus,
                        azul_core::events::EventSource::User,
                        new_node,
                        now.clone(),
                        azul_core::events::EventData::None,
                    ));
                }

                if !focus_events.is_empty() {
                    let (focus_results, _) = self.dispatch_events_propagated(&focus_events);
                    for callback_result in &focus_results {
                        let event_result = self.process_callback_result_v2(callback_result);
                        result = result.max(event_result);
                    }
                }
            }

            // CRITICAL: Update previous_state BEFORE recursing to prevent the same
            // keyboard events from being detected again. Without this, a Tab key
            // would trigger FocusNext on every recursion level.
            let current = self.get_current_window_state().clone();
            self.set_previous_window_state(current);

            // Recurse to process any further events that may have been triggered
            let focus_result = self.process_window_events_recursive_v2(depth + 1);
            result = result.max(focus_result);
        }

        // Recurse if needed (DOM regeneration)
        if should_recurse && depth + 1 < MAX_EVENT_RECURSION_DEPTH {
            // CRITICAL: Update previous_state BEFORE recursing to prevent the same
            // mouse/keyboard events from being detected again. Without this, a MouseUp
            // event would trigger the callback on every recursion level, causing
            // the callback to fire multiple times for a single click.
            let current = self.get_current_window_state().clone();
            self.set_previous_window_state(current);

            let recursive_result = self.process_window_events_recursive_v2(depth + 1);
            result = result.max(recursive_result);
        }

        // NOTE: Window drag is handled entirely by titlebar callbacks.
        // The DragStart/Drag callbacks on the csd-title node read the
        // gesture manager's drag delta and window_position_at_session_start
        // to compute the new window position via modify_window_state().

        // W3C "flag and defer" pattern: Finalize pending focus changes after all events processed
        //
        // This is called at the end of event processing to initialize the cursor for
        // contenteditable elements. The cursor wasn't initialized during focus event handling
        // because text layout may not have been available. Now that all events have been
        // processed and layout has had a chance to update, we can safely initialize the cursor.
        //
        // After successful cursor initialization, we also start the cursor blink timer.
        // NOTE: We need to carefully manage borrows here - first do all layout_window work,
        // then create the timer separately if needed.
        let timer_creation_needed = if let Some(layout_window) = self.get_layout_window_mut() {
            let needs_init = layout_window.focus_manager.needs_cursor_initialization();
            if needs_init {
                let cursor_initialized = layout_window.finalize_pending_focus_changes();
                if cursor_initialized {
                    log_debug!(
                        super::debug_server::LogCategory::Input,
                        "[Event V2] Cursor initialized via finalize_pending_focus_changes"
                    );
                    result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);

                    // Check if blink timer is not already active
                    if !layout_window.cursor_manager.is_blink_timer_active() {
                        layout_window.cursor_manager.set_blink_timer_active(true);
                        true // Signal that we need to create and start the timer
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        // Create and start the blink timer outside of the mutable layout_window borrow
        if timer_creation_needed {
            // Now we can safely get both window_state and layout_window
            let timer = if let Some(layout_window) = self.get_layout_window() {
                let current_window_state = self.get_current_window_state();
                Some(layout_window.create_cursor_blink_timer(current_window_state))
            } else {
                None
            };

            if let Some(timer) = timer {
                self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                log_debug!(
                    super::debug_server::LogCategory::Input,
                    "[Event V2] Started cursor blink timer after focus finalization"
                );
            }
        }

        result
    }

    /// V2: Process callback result and determine what action to take.
    ///
    /// This converts the callback result into a `ProcessEventResult` that tells
    /// the platform what to do next (redraw, regenerate layout, etc.).
    ///
    /// This method handles:
    /// - Window state modifications (title, size, position, flags)
    /// - Focus changes
    /// - Image/image mask updates
    /// - Timer/thread management
    /// - New window creation
    /// - DOM regeneration triggering
    fn process_callback_result_v2(&mut self, result: &CallCallbacksResult) -> ProcessEventResult {
        use azul_core::callbacks::Update;

        let mut event_result = ProcessEventResult::DoNothing;
        let mut mouse_state_changed = false;
        let mut keyboard_state_changed = false;

        // Handle window state modifications
        if let Some(ref modified_state) = result.modified_window_state {
            // Check if mouse_state changed (for synthetic event injection)
            // NOTE: We must save previous state BEFORE modifying current state
            // so that process_window_events_recursive_v2 can detect the change
            let old_mouse_state = self.get_current_window_state().mouse_state.clone();
            if old_mouse_state != modified_state.mouse_state {
                mouse_state_changed = true;
                // Save current state as previous BEFORE updating
                // This is critical for synthetic events from debug API
                let old_state = self.get_current_window_state().clone();
                self.set_previous_window_state(old_state);
            }

            // Check if keyboard_state changed (for synthetic keyboard events)
            let old_keyboard_state = self.get_current_window_state().keyboard_state.clone();
            if old_keyboard_state != modified_state.keyboard_state {
                keyboard_state_changed = true;
                // Save current state as previous BEFORE updating (if not already saved for mouse)
                if !mouse_state_changed {
                    let old_state = self.get_current_window_state().clone();
                    self.set_previous_window_state(old_state);
                }
            }

            // Now update current state
            let current_state = self.get_current_window_state_mut();
            current_state.title = modified_state.title.clone();
            current_state.size = modified_state.size;
            current_state.position = modified_state.position;
            current_state.flags = modified_state.flags;
            current_state.background_color = modified_state.background_color;
            // Also copy mouse_state for synthetic event injection
            current_state.mouse_state = modified_state.mouse_state.clone();
            // Also copy keyboard_state for synthetic keyboard events
            current_state.keyboard_state = modified_state.keyboard_state.clone();

            // Check if window should close
            if modified_state.flags.close_requested {
                // Platform should handle window destruction
                return ProcessEventResult::DoNothing;
            }

            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // If mouse_state changed, trigger event processing to invoke callbacks
        // This enables synthetic mouse events from debug API and automation
        if mouse_state_changed {
            // First, update hit testing at the new mouse position
            // This is critical for synthetic events - without hit testing,
            // dispatch_synthetic_events won't know which nodes are under the mouse
            let mouse_pos = self
                .get_current_window_state()
                .mouse_state
                .cursor_position
                .get_position();
            if let Some(pos) = mouse_pos {
                self.update_hit_test_at(pos);
            }

            // Re-process events with the new mouse state
            // This will detect the mouse state change and invoke appropriate callbacks
            let nested_result = self.process_window_events_recursive_v2(0);
            event_result = event_result.max(nested_result);
        }

        // If keyboard_state changed, trigger event processing to invoke callbacks
        // This enables synthetic keyboard events from debug API (Tab, Enter, etc.)
        if keyboard_state_changed && !mouse_state_changed {
            // Re-process events with the new keyboard state
            // This will detect the keyboard state change and invoke appropriate callbacks
            let nested_result = self.process_window_events_recursive_v2(0);
            event_result = event_result.max(nested_result);
        }

        // Handle queued window state sequence (for simulating clicks, etc.)
        // Each state is applied in order, with event processing between states
        // to detect the transitions (e.g., mouse down → mouse up)
        if !result.queued_window_states.is_empty() {
            for (i, queued_state) in result.queued_window_states.iter().enumerate() {
                // Save current state as previous
                let old_state = self.get_current_window_state().clone();
                self.set_previous_window_state(old_state.clone());

                // Apply the queued state
                let current_state = self.get_current_window_state_mut();
                current_state.mouse_state = queued_state.mouse_state.clone();
                current_state.keyboard_state = queued_state.keyboard_state.clone();
                current_state.title = queued_state.title.clone();
                current_state.size = queued_state.size;
                current_state.position = queued_state.position;
                current_state.flags = queued_state.flags;

                // Update hit testing at the new mouse position
                let mouse_pos = queued_state.mouse_state.cursor_position.get_position();
                if let Some(pos) = mouse_pos {
                    self.update_hit_test_at(pos);
                }

                // Process events with this state (will detect state changes)
                let nested_result = self.process_window_events_recursive_v2(0);
                event_result = event_result.max(nested_result);
            }
        }

        // Handle focus changes
        use azul_layout::callbacks::FocusUpdateRequest;
        match result.update_focused_node {
            FocusUpdateRequest::FocusNode(new_focus) => {
                // Update focus in the FocusManager (in LayoutWindow)
                if let Some(layout_window) = self.get_layout_window_mut() {
                    layout_window
                        .focus_manager
                        .set_focused_node(Some(new_focus));

                    // SCROLL INTO VIEW: Scroll newly focused node into visible area
                    use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;
                    let now = azul_core::task::Instant::now();
                    layout_window.scroll_node_into_view(
                        new_focus,
                        ScrollIntoViewOptions::nearest(),
                        now,
                    );

                    // CURSOR BLINK TIMER: Start/stop timer based on contenteditable focus
                    let window_state = layout_window.current_window_state.clone();
                    let timer_action = layout_window.handle_focus_change_for_cursor_blink(
                        Some(new_focus),
                        &window_state,
                    );

                    match timer_action {
                        azul_layout::CursorBlinkTimerAction::Start(timer) => {
                            self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                        }
                        azul_layout::CursorBlinkTimerAction::Stop => {
                            self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                        }
                        azul_layout::CursorBlinkTimerAction::NoChange => {}
                    }
                }
                event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
            }
            FocusUpdateRequest::ClearFocus => {
                // Clear focus in the FocusManager (in LayoutWindow)
                if let Some(layout_window) = self.get_layout_window_mut() {
                    layout_window.focus_manager.set_focused_node(None);

                    // CURSOR BLINK TIMER: Stop timer when focus is cleared
                    let window_state = layout_window.current_window_state.clone();
                    let timer_action = layout_window.handle_focus_change_for_cursor_blink(
                        None,
                        &window_state,
                    );

                    match timer_action {
                        azul_layout::CursorBlinkTimerAction::Start(_timer) => {
                            // Shouldn't happen when clearing focus, but handle it
                        }
                        azul_layout::CursorBlinkTimerAction::Stop => {
                            self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                        }
                        azul_layout::CursorBlinkTimerAction::NoChange => {}
                    }
                }
                event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
            }
            FocusUpdateRequest::NoChange => {
                // No focus change requested
            }
        }

        // Handle scroll position changes from callbacks (e.g., scroll_node_by API, physics timer ScrollTo)
        if let Some(ref nodes_scrolled) = result.nodes_scrolled_in_callbacks {
            if !nodes_scrolled.is_empty() {
                // Get current time for scroll animation
                let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
                let now = (external.get_system_time_fn.cb)();

                // Phase 1: Set scroll positions and detect IFrames needing re-invocation
                let mut iframes_to_reinvoke: Vec<(DomId, NodeId, azul_core::geom::LogicalRect)> = Vec::new();

                if let Some(layout_window) = self.get_layout_window_mut() {
                    for (dom_id, node_map) in nodes_scrolled {
                        for (hierarchy_id, target_position) in node_map {
                            // Convert NodeHierarchyItemId to NodeId
                            if let Some(node_id) = hierarchy_id.into_crate_internal() {
                                // Use instant scroll (duration = 0) for programmatic scrolling
                                layout_window.scroll_manager.scroll_to(
                                    *dom_id,
                                    node_id,
                                    *target_position,
                                    std::time::Duration::from_millis(0).into(),
                                    azul_core::events::EasingFunction::Linear,
                                    now.clone().into(),
                                );

                                // IFrame re-invocation check: after setting new scroll position,
                                // check if this node hosts an IFrame that needs re-invocation
                                // (e.g., user scrolled near an edge for lazy loading).
                                // This is transparent — the timer doesn't know about IFrames.
                                let scroll_state = layout_window.scroll_manager
                                    .get_scroll_state(*dom_id, node_id);
                                let layout_bounds = scroll_state
                                    .map(|s| s.container_rect)
                                    .unwrap_or_default();

                                if let Some(_reason) = layout_window.iframe_manager.check_reinvoke(
                                    *dom_id,
                                    node_id,
                                    &layout_window.scroll_manager,
                                    layout_bounds,
                                ) {
                                    iframes_to_reinvoke.push((*dom_id, node_id, layout_bounds));
                                }
                            }
                        }
                    }

                    // Normal scrolling always needs a re-render
                    event_result =
                        event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                }
                // layout_window borrow is released here

                // Phase 2: Re-invoke IFrame callbacks (needs split borrows via prepare_callback_invocation)
                if !iframes_to_reinvoke.is_empty() {
                    let borrows = self.prepare_callback_invocation();
                    let system_callbacks = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();

                    for (dom_id, node_id, bounds) in &iframes_to_reinvoke {
                        // invoke_iframe_callback calls the IFrame's RefAny callback,
                        // swaps the child Dom, and re-layouts only the IFrame sub-tree.
                        // It does NOT call the main layout() callback.
                        let _ = borrows.layout_window.invoke_iframe_callback(
                            *dom_id,
                            *node_id,
                            *bounds,
                            borrows.current_window_state,
                            borrows.renderer_resources,
                            &system_callbacks,
                            &mut None,
                        );
                    }

                    // IFrame Doms were swapped — rebuild display list (but NOT the main DOM)
                    event_result =
                        event_result.max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
                }
            }
        }

        // Handle image updates
        if result.images_changed.is_some() || result.image_masks_changed.is_some() {
            event_result =
                event_result.max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
        }

        // Handle image callback re-invocation (OpenGL texture updates, etc.)
        // Signal that a redraw is needed; actual invocation happens in
        // wr_translate2::process_image_callback_updates() during WebRender
        // transaction building — invoking here would cause double invocation.
        {
            let has_specific = result.image_callbacks_changed.as_ref()
                .map(|m| !m.is_empty()).unwrap_or(false);

            if result.update_all_image_callbacks || has_specific {
                // Signal that we need a new frame — the rendering path will
                // re-invoke image callbacks and register textures with WebRender.
                event_result = event_result.max(
                    ProcessEventResult::ShouldReRenderCurrentWindow
                );
            }
        }

        // Handle text changes (words_changed) — previously dead field, now wired
        if let Some(ref words) = result.words_changed {
            use azul_core::diff::ChangeAccumulator;
            let mut accumulator = ChangeAccumulator::new();
            for (_dom_id, nodes) in words {
                for (node_id, new_text) in nodes {
                    // Text changes need IFC reshape at minimum
                    accumulator.add_text_change(
                        *node_id,
                        String::new(), // old text not available here
                        new_text.as_str().to_string(),
                    );
                }
            }
            if accumulator.needs_layout() {
                // Phase 3: Apply text changes to the cached StyledDom so that
                // incremental_relayout() sees the updated text content.
                // Without this, the StyledDom would still have the old text
                // and the re-layout would produce identical (stale) output.
                if let Some(layout_window) = self.get_layout_window_mut() {
                    for (dom_id, nodes) in words {
                        if let Some(layout_result) = layout_window.layout_results.get_mut(dom_id) {
                            for (node_id, new_text) in nodes {
                                let idx = node_id.index();
                                if idx < layout_result.styled_dom.node_data.as_ref().len() {
                                    layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                                        .set_node_type(azul_core::dom::NodeType::Text(new_text.clone()));
                                }
                            }
                        }
                    }
                }
                event_result = event_result.max(ProcessEventResult::ShouldIncrementalRelayout);
            } else if accumulator.needs_paint_only() {
                event_result = event_result.max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
            }
        }

        // Handle CSS property changes — previously dead field, now wired
        if let Some(ref css) = result.css_properties_changed {
            use azul_core::diff::ChangeAccumulator;
            use azul_css::props::property::RelayoutScope;
            let mut accumulator = ChangeAccumulator::new();
            for (_dom_id, nodes) in css {
                for (node_id, properties) in nodes {
                    for prop in properties.as_ref().iter() {
                        let prop_type = prop.get_type();
                        let scope = prop_type.relayout_scope(true);
                        accumulator.add_css_change(*node_id, prop_type, scope);
                    }
                }
            }
            if accumulator.needs_layout() || accumulator.needs_paint_only() {
                // Phase 3: Apply CSS property changes to the cached StyledDom so
                // that incremental_relayout() sees the updated inline styles.
                if let Some(layout_window) = self.get_layout_window_mut() {
                    for (dom_id, nodes) in css {
                        if let Some(layout_result) = layout_window.layout_results.get_mut(dom_id) {
                            for (node_id, properties) in nodes {
                                let idx = node_id.index();
                                if idx < layout_result.styled_dom.node_data.as_ref().len() {
                                    // Replace the node's inline CSS properties with the new ones.
                                    // Each CssProperty is wrapped in CssPropertyWithConditions
                                    // with no conditions (unconditional / inline-style semantics).
                                    use azul_css::dynamic_selector::CssPropertyWithConditions;
                                    let new_props: Vec<CssPropertyWithConditions> = properties
                                        .as_ref()
                                        .iter()
                                        .map(|p| CssPropertyWithConditions::simple(p.clone()))
                                        .collect();
                                    layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                                        .set_css_props(new_props.into());
                                }
                            }
                        }
                    }
                }
                if accumulator.needs_layout() {
                    event_result = event_result.max(ProcessEventResult::ShouldIncrementalRelayout);
                } else {
                    event_result = event_result.max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
                }
            }
        }

        // Handle timers and threads
        if result.timers.is_some()
            || result.timers_removed.is_some()
            || result.threads.is_some()
            || result.threads_removed.is_some()
        {
            // Process timers - call platform-specific start/stop methods
            if let Some(timers) = &result.timers {
                for (timer_id, timer) in timers {
                    self.start_timer(timer_id.id, timer.clone());
                }
            }

            if let Some(timers_removed) = &result.timers_removed {
                for timer_id in timers_removed {
                    self.stop_timer(timer_id.id);
                }
            }

            // Process threads - add/remove from layout_window and manage polling timer
            let should_start_thread_timer;
            let should_stop_thread_timer;

            // First, check if we had threads before
            let had_threads = if let Some(layout_window) = self.get_layout_window() {
                !layout_window.threads.is_empty()
            } else {
                false
            };

            // Add new threads
            if let Some(threads) = result.threads.clone() {
                self.add_threads(threads);
            }

            // Remove old threads
            if let Some(threads_removed) = &result.threads_removed {
                self.remove_threads(threads_removed);
            }

            // Now check if we have threads after modifications
            let has_threads = if let Some(layout_window) = self.get_layout_window() {
                !layout_window.threads.is_empty()
            } else {
                false
            };

            // Determine if we need to start/stop the thread polling timer
            should_start_thread_timer = !had_threads && has_threads;
            should_stop_thread_timer = had_threads && !has_threads;

            // Start thread polling timer if we now have threads
            if should_start_thread_timer {
                self.start_thread_poll_timer();
            }

            // Stop thread polling timer if we no longer have threads
            if should_stop_thread_timer {
                self.stop_thread_poll_timer();
            }

            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle new windows spawned in callbacks
        if !result.windows_created.is_empty() {
            // TODO: Signal to event loop to create new windows
            // For now, just log
            log_debug!(
                super::debug_server::LogCategory::Window,
                "[PlatformWindowV2] {} new windows requested (not yet implemented)",
                result.windows_created.len()
            );
        }

        // Handle menus requested to be opened
        if !result.menus_to_open.is_empty() {
            for (menu, position_override) in &result.menus_to_open {
                // Use override position if provided, otherwise use (0, 0) as default
                // The Menu.position field is a MenuPopupPosition enum (AutoCursor, etc.),
                // not a specific coordinate. For callback-opened menus, the position_override
                // specifies where to show it.
                let position = position_override.unwrap_or(LogicalPosition::new(0.0, 0.0));

                // Show menu (native or fallback based on flags)
                self.show_menu_from_callback(menu, position);
            }
            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle tooltip show requests
        if !result.tooltips_to_show.is_empty() {
            // Show only the last tooltip requested (if multiple were requested in one callback)
            if let Some((text, position)) = result.tooltips_to_show.last() {
                self.show_tooltip_from_callback(text.as_str(), *position);
            }
        }

        // Handle tooltip hide request
        if result.hide_tooltip {
            self.hide_tooltip_from_callback();
        }

        // Handle begin interactive move (Wayland: xdg_toplevel_move)
        if result.begin_interactive_move {
            self.handle_begin_interactive_move();
        }

        // Handle explicit hit test update request (from Debug API)
        // This is separate from mouse_state_changed to allow explicit hit test updates
        // without modifying mouse position
        if let Some(position) = result.hit_test_update_requested {
            self.update_hit_test_at(position);
        }

        // Process text_input_triggered from CreateTextInput
        // This is how debug server text input flows:
        // 1. debug_timer_callback calls callback_info.create_text_input(text)
        // 2. apply_callback_changes processes CreateTextInput
        // 3. process_text_input() is called, returning affected nodes
        // 4. text_input_triggered is populated and forwarded here
        // 5. We trigger recursive event processing to invoke user callbacks
        if !result.text_input_triggered.is_empty() {
            use crate::desktop::shell2::common::debug_server::{log, LogLevel, LogCategory};
            log(LogLevel::Debug, LogCategory::EventLoop,
                format!("[process_callback_result_v2] Processing {} text_input_triggered events", result.text_input_triggered.len()), None);

            // Build synthetic events for text input callbacks
            let now = {
                #[cfg(feature = "std")]
                { azul_core::task::Instant::from(std::time::Instant::now()) }
                #[cfg(not(feature = "std"))]
                { azul_core::task::Instant::Tick(azul_core::task::SystemTick::new(0)) }
            };

            // Convert text_input_triggered to SyntheticEvents for dispatch
            let mut text_events = Vec::new();
            for (dom_node_id, _event_filters) in &result.text_input_triggered {
                log(LogLevel::Debug, LogCategory::EventLoop,
                    format!("[process_callback_result_v2] Node {:?} triggered text input", dom_node_id), None);

                text_events.push(azul_core::events::SyntheticEvent::new(
                    azul_core::events::EventType::Input,
                    azul_core::events::EventSource::User,
                    *dom_node_id,
                    now.clone(),
                    azul_core::events::EventData::None,
                ));
            }

            if !text_events.is_empty() {
                let (text_results, text_prevented) = self.dispatch_events_propagated(&text_events);
                for callback_result in &text_results {
                    if callback_result.prevent_default {
                        log(LogLevel::Debug, LogCategory::EventLoop,
                            "[process_callback_result_v2] preventDefault called - text input will be rejected".to_string(), None);
                    }
                    if matches!(callback_result.callbacks_update_screen, Update::RefreshDom | Update::RefreshDomAllWindows) {
                        event_result = event_result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
                    }
                }
            }

            // After processing callbacks, apply the text changeset if not rejected
            // This updates the visual cache
            if let Some(layout_window) = self.get_layout_window_mut() {
                let dirty_nodes = layout_window.apply_text_changeset();
                if !dirty_nodes.is_empty() {
                    log(LogLevel::Debug, LogCategory::EventLoop,
                        format!("[process_callback_result_v2] Applied text changeset, {} dirty nodes", dirty_nodes.len()), None);
                    event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);

                    // CRITICAL FIX: Scroll cursor into view after text edit
                    // Without this, typing at the end of a long text doesn't scroll
                    // the view to keep the cursor visible.
                    layout_window.scroll_selection_into_view(
                        azul_layout::window::SelectionScrollType::Cursor,
                        azul_layout::window::ScrollMode::Instant,
                    );
                } else {
                    log(LogLevel::Debug, LogCategory::EventLoop,
                        "[process_callback_result_v2] apply_text_changeset returned 0 dirty nodes".to_string(), None);
                }
            } else {
                log(LogLevel::Debug, LogCategory::EventLoop,
                    "[process_callback_result_v2] No layout_window available for apply_text_changeset".to_string(), None);
            }
        }

        // Process Update screen command
        match result.callbacks_update_screen {
            Update::DoNothing => {}
            Update::RefreshDom => {
                self.mark_frame_needs_regeneration();
                event_result =
                    event_result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
            }
            Update::RefreshDomAllWindows => {
                self.mark_frame_needs_regeneration();
                event_result = event_result.max(ProcessEventResult::ShouldRegenerateDomAllWindows);
            }
        }

        event_result
    }

    /// Process all expired timer callbacks and pending thread callbacks.
    ///
    /// This is the single method that replaces the 8× copy-pasted timer/thread
    /// processing boilerplate that previously existed in each platform's tick handler.
    ///
    /// Returns `true` if a redraw is needed (i.e. any callback requested a visual update).
    /// The platform is then responsible for triggering the actual OS redraw.
    ///
    /// Each platform's tick handler becomes a one-liner:
    /// ```ignore
    /// if self.process_timers_and_threads() {
    ///     self.trigger_platform_redraw(); // setNeedsDisplay / InvalidateRect / etc.
    /// }
    /// ```
    fn process_timers_and_threads(&mut self) -> bool {
        use azul_core::events::ProcessEventResult;

        let timer_results = self.invoke_expired_timers();
        let mut needs_redraw = false;

        for result in &timer_results {
            if result.needs_processing() {
                let old_state = self.get_current_window_state().clone();
                self.set_previous_window_state(old_state);
                let process_result = self.process_callback_result_v2(result);
                self.sync_window_state();
                if process_result >= ProcessEventResult::ShouldReRenderCurrentWindow {
                    needs_redraw = true;
                }
            }
            if result.needs_redraw() {
                needs_redraw = true;
            }
        }

        if let Some(thread_result) = self.invoke_thread_callbacks() {
            if thread_result.needs_processing() {
                let old_state = self.get_current_window_state().clone();
                self.set_previous_window_state(old_state);
                let process_result = self.process_callback_result_v2(&thread_result);
                self.sync_window_state();
                if process_result >= ProcessEventResult::ShouldReRenderCurrentWindow {
                    needs_redraw = true;
                }
            }
            if thread_result.needs_redraw() {
                needs_redraw = true;
            }
        }

        if needs_redraw {
            self.mark_frame_needs_regeneration();
        }

        needs_redraw
    }

    /// Perform scrollbar hit-test at the given position.
    ///
    /// Returns `Some(ScrollbarHitId)` if a scrollbar was hit, `None` otherwise.
    ///
    /// This uses WebRender's hit-tester to check for scrollbar tags.
    fn perform_scrollbar_hit_test(
        &self,
        position: azul_core::geom::LogicalPosition,
    ) -> Option<azul_core::hit_test::ScrollbarHitId> {
        use webrender::api::units::WorldPoint;

        let hit_tester = match self.get_hit_tester() {
            AsyncHitTester::Resolved(ht) => ht,
            _ => return None,
        };

        let world_point = WorldPoint::new(position.x, position.y);
        let hit_result = hit_tester.hit_test(world_point);

        // Check each hit item for scrollbar tag
        for item in hit_result.items.iter() {
            if let Some(scrollbar_id) =
                wr_translate2::translate_item_tag_to_scrollbar_hit_id(item.tag)
            {
                return Some(scrollbar_id);
            }
        }

        None
    }

    /// Handle scrollbar click (thumb or track).
    ///
    /// Returns `ProcessEventResult` indicating whether to redraw.
    fn handle_scrollbar_click(
        &mut self,
        hit_id: azul_core::hit_test::ScrollbarHitId,
        position: azul_core::geom::LogicalPosition,
    ) -> ProcessEventResult {
        use azul_core::hit_test::ScrollbarHitId;

        match hit_id {
            ScrollbarHitId::VerticalThumb(dom_id, node_id)
            | ScrollbarHitId::HorizontalThumb(dom_id, node_id) => {
                // Start drag
                let layout_window = match self.get_layout_window() {
                    Some(lw) => lw,
                    None => return ProcessEventResult::DoNothing,
                };

                let scroll_offset = layout_window
                    .scroll_manager
                    .get_current_offset(dom_id, node_id)
                    .unwrap_or_default();

                self.set_scrollbar_drag_state(Some(ScrollbarDragState {
                    hit_id,
                    initial_mouse_pos: position,
                    initial_scroll_offset: scroll_offset,
                }));

                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            ScrollbarHitId::VerticalTrack(dom_id, node_id) => {
                self.handle_track_click(dom_id, node_id, position, true)
            }

            ScrollbarHitId::HorizontalTrack(dom_id, node_id) => {
                self.handle_track_click(dom_id, node_id, position, false)
            }
        }
    }

    /// Handle track click - jump scroll to clicked position.
    fn handle_track_click(
        &mut self,
        dom_id: DomId,
        node_id: CoreNodeId,
        click_position: azul_core::geom::LogicalPosition,
        is_vertical: bool,
    ) -> ProcessEventResult {
        use azul_core::dom::ScrollbarOrientation;

        // Get scrollbar state to calculate target position
        let layout_window = match self.get_layout_window() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        // Get current scrollbar geometry
        let scrollbar_state = if is_vertical {
            layout_window.scroll_manager.get_scrollbar_state(
                dom_id,
                node_id,
                ScrollbarOrientation::Vertical,
            )
        } else {
            layout_window.scroll_manager.get_scrollbar_state(
                dom_id,
                node_id,
                ScrollbarOrientation::Horizontal,
            )
        };

        let scrollbar_state = match scrollbar_state {
            Some(s) if s.visible => s,
            _ => return ProcessEventResult::DoNothing,
        };

        // Get current scroll state
        let scroll_state = match layout_window
            .scroll_manager
            .get_scroll_state(dom_id, node_id)
        {
            Some(s) => s,
            None => return ProcessEventResult::DoNothing,
        };

        // Calculate which position on the track was clicked (0.0 = top/left, 1.0 = bottom/right)
        let click_ratio = if is_vertical {
            let track_top = scrollbar_state.track_rect.origin.y;
            let track_height = scrollbar_state.track_rect.size.height;
            ((click_position.y - track_top) / track_height).clamp(0.0, 1.0)
        } else {
            let track_left = scrollbar_state.track_rect.origin.x;
            let track_width = scrollbar_state.track_rect.size.width;
            ((click_position.x - track_left) / track_width).clamp(0.0, 1.0)
        };

        // Calculate target scroll position
        let container_size = if is_vertical {
            scroll_state.container_rect.size.height
        } else {
            scroll_state.container_rect.size.width
        };

        let content_size = if is_vertical {
            scroll_state.content_rect.size.height
        } else {
            scroll_state.content_rect.size.width
        };

        let max_scroll = (content_size - container_size).max(0.0);
        let target_scroll = click_ratio * max_scroll;

        // Calculate delta from current position
        let current_scroll = if is_vertical {
            scroll_state.current_offset.y
        } else {
            scroll_state.current_offset.x
        };

        let scroll_delta = target_scroll - current_scroll;

        // Apply scroll using gpu_scroll
        if let Err(e) = self.gpu_scroll(
            dom_id,
            node_id,
            if is_vertical { 0.0 } else { scroll_delta },
            if is_vertical { scroll_delta } else { 0.0 },
        ) {
            log_warn!(
                super::debug_server::LogCategory::Input,
                "Track click scroll failed: {}",
                e
            );
            return ProcessEventResult::DoNothing;
        }

        ProcessEventResult::ShouldReRenderCurrentWindow
    }

    // PROVIDED: Timer Invocation (Cross-Platform Implementation)

    /// Invoke all expired timer callbacks.
    ///
    /// This method checks for expired timers via `tick_timers()` and invokes
    /// the callback for each expired timer using `run_single_timer()`.
    ///
    /// ## Returns
    /// * `Vec<CallCallbacksResult>` - Results from all invoked timer callbacks
    ///
    /// ## Platform Usage
    /// Call this from platform event loops when:
    /// - **Windows**: In `WM_TIMER` handler
    /// - **macOS**: In `performSelector:withObject:afterDelay:` callback
    /// - **X11**: After `select()` timeout
    /// - **Wayland**: After `timerfd` read
    fn invoke_expired_timers(&mut self) -> Vec<azul_layout::callbacks::CallCallbacksResult> {
        use azul_core::callbacks::Update;
        use azul_core::task::TimerId;
        use azul_layout::callbacks::{CallCallbacksResult, ExternalSystemCallbacks};

        // Get current system time
        let system_callbacks = ExternalSystemCallbacks::rust_internal();
        let current_time = (system_callbacks.get_system_time_fn.cb)();
        let frame_start: azul_core::task::Instant = current_time.clone().into();

        // First, get expired timer IDs without borrowing self
        let expired_timer_ids: Vec<TimerId> = {
            let layout_window = match self.get_layout_window_mut() {
                Some(lw) => lw,
                None => return Vec::new(),
            };
            layout_window.tick_timers(current_time)
        };

        if expired_timer_ids.is_empty() {
            return Vec::new();
        }

        let mut all_results = Vec::new();

        // Process each expired timer
        for timer_id in expired_timer_ids {
            // Prepare borrows fresh for each timer invocation
            let mut borrows = self.prepare_callback_invocation();

            let result = borrows.layout_window.run_single_timer(
                timer_id.id,
                frame_start.clone(),
                &borrows.window_handle,
                borrows.gl_context_ptr,
                borrows.image_cache,
                &mut borrows.fc_cache_clone,
                borrows.system_style.clone(),
                &ExternalSystemCallbacks::rust_internal(),
                borrows.previous_window_state,
                borrows.current_window_state,
                borrows.renderer_resources,
            );

            // Apply timer/thread results directly to layout_window for inter-callback
            // correctness (e.g. if timer A removes timer B, the next timer in this
            // tick shouldn't try to invoke timer B). process_callback_result_v2
            // will also call start_timer()/stop_timer() for platform-level registration.
            if let Some(ref new_timers) = result.timers {
                for (timer_id, timer) in new_timers {
                    borrows
                        .layout_window
                        .timers
                        .insert(*timer_id, timer.clone());
                }
            }
            if let Some(ref removed_timers) = result.timers_removed {
                for timer_id in removed_timers {
                    borrows.layout_window.timers.remove(timer_id);
                }
            }
            if let Some(ref new_threads) = result.threads {
                for (thread_id, thread) in new_threads {
                    borrows
                        .layout_window
                        .threads
                        .insert(*thread_id, thread.clone());
                }
            }
            if let Some(ref removed_threads) = result.threads_removed {
                for thread_id in removed_threads {
                    borrows.layout_window.threads.remove(thread_id);
                }
            }

            // Mark frame for redraw if callback requested it
            if result.callbacks_update_screen == Update::RefreshDom
                || result.callbacks_update_screen == Update::RefreshDomAllWindows
            {
                self.mark_frame_needs_regeneration();
            }

            all_results.push(result);
        }

        all_results
    }

    // PROVIDED: Thread Callback Invocation (Cross-Platform Implementation)

    /// Invoke all pending thread callbacks (writeback messages).
    ///
    /// This method polls all active threads for completed work and invokes
    /// the writeback callbacks for any threads that have finished.
    ///
    /// ## Returns
    /// * `Option<CallCallbacksResult>` - Combined result from all thread writeback callbacks, or None if no threads processed
    ///
    /// ## Platform Usage
    /// Call this from platform event loops when:
    /// - **Windows**: In `WM_TIMER` handler with thread timer ID (0xFFFF)
    /// - **macOS**: In thread poll timer callback (NSTimer every 16ms)
    /// - **X11**: After `select()` timeout when threads exist
    /// - **Wayland**: After thread timerfd read
    fn invoke_thread_callbacks(&mut self) -> Option<azul_layout::callbacks::CallCallbacksResult> {
        use azul_layout::callbacks::ExternalSystemCallbacks;

        // Check if we have threads to poll
        let has_threads = {
            let layout_window = match self.get_layout_window() {
                Some(lw) => lw,
                None => return None,
            };
            !layout_window.threads.is_empty()
        };

        if !has_threads {
            return None;
        }

        // Get app_data from the platform window (shared across all windows)
        let app_data_arc = self.get_app_data().clone();

        // Prepare borrows for thread invocation
        let mut borrows = self.prepare_callback_invocation();

        // Call run_all_threads on the layout_window
        let mut app_data = app_data_arc.borrow_mut();
        let result = borrows.layout_window.run_all_threads(
            &mut *app_data,
            &borrows.window_handle,
            borrows.gl_context_ptr,
            borrows.image_cache,
            &mut borrows.fc_cache_clone,
            borrows.system_style.clone(),
            &ExternalSystemCallbacks::rust_internal(),
            borrows.previous_window_state,
            borrows.current_window_state,
            borrows.renderer_resources,
        );

        Some(result)
    }

    /// Handle scrollbar drag - update scroll position based on mouse delta.
    fn handle_scrollbar_drag(
        &mut self,
        current_pos: azul_core::geom::LogicalPosition,
    ) -> ProcessEventResult {
        use azul_core::dom::ScrollbarOrientation;
        use azul_core::hit_test::ScrollbarHitId;

        let drag_state = match self.get_scrollbar_drag_state() {
            Some(ds) => ds.clone(),
            None => return ProcessEventResult::DoNothing,
        };

        let layout_window = match self.get_layout_window() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        // Calculate delta
        let (dom_id, node_id, is_vertical) = match drag_state.hit_id {
            ScrollbarHitId::VerticalThumb(dom_id, node_id) => (dom_id, node_id, true),
            ScrollbarHitId::HorizontalThumb(dom_id, node_id) => (dom_id, node_id, false),
            _ => return ProcessEventResult::DoNothing,
        };

        let pixel_delta = if is_vertical {
            current_pos.y - drag_state.initial_mouse_pos.y
        } else {
            current_pos.x - drag_state.initial_mouse_pos.x
        };

        // Get scrollbar geometry
        let orientation = if is_vertical {
            ScrollbarOrientation::Vertical
        } else {
            ScrollbarOrientation::Horizontal
        };

        let scrollbar_state =
            match layout_window
                .scroll_manager
                .get_scrollbar_state(dom_id, node_id, orientation)
            {
                Some(s) if s.visible => s,
                _ => return ProcessEventResult::DoNothing,
            };

        let scroll_state = match layout_window
            .scroll_manager
            .get_scroll_state(dom_id, node_id)
        {
            Some(s) => s,
            None => return ProcessEventResult::DoNothing,
        };

        // Convert pixel delta to scroll delta
        // pixel_delta / track_size = scroll_delta / max_scroll
        let track_size = if is_vertical {
            scrollbar_state.track_rect.size.height
        } else {
            scrollbar_state.track_rect.size.width
        };

        let container_size = if is_vertical {
            scroll_state.container_rect.size.height
        } else {
            scroll_state.container_rect.size.width
        };

        let content_size = if is_vertical {
            scroll_state.content_rect.size.height
        } else {
            scroll_state.content_rect.size.width
        };

        let max_scroll = (content_size - container_size).max(0.0);

        // Account for thumb size: usable track size is track_size - thumb_size
        let thumb_size = scrollbar_state.thumb_size_ratio * track_size;
        let usable_track_size = (track_size - thumb_size).max(1.0);

        // Calculate scroll delta
        let scroll_delta = if usable_track_size > 0.0 {
            (pixel_delta / usable_track_size) * max_scroll
        } else {
            0.0
        };

        // Calculate target scroll position (initial + delta from drag start)
        let target_scroll = if is_vertical {
            drag_state.initial_scroll_offset.y + scroll_delta
        } else {
            drag_state.initial_scroll_offset.x + scroll_delta
        };

        // Clamp to valid range
        let target_scroll = target_scroll.clamp(0.0, max_scroll);

        // Calculate delta from current position
        let current_scroll = if is_vertical {
            scroll_state.current_offset.y
        } else {
            scroll_state.current_offset.x
        };

        let delta_from_current = target_scroll - current_scroll;

        // Use gpu_scroll to update scroll position
        if let Err(e) = self.gpu_scroll(
            dom_id,
            node_id,
            if is_vertical { 0.0 } else { delta_from_current },
            if is_vertical { delta_from_current } else { 0.0 },
        ) {
            log_warn!(
                super::debug_server::LogCategory::Input,
                "Scrollbar drag failed: {}",
                e
            );
            return ProcessEventResult::DoNothing;
        }

        ProcessEventResult::ShouldReRenderCurrentWindow
    }
}
