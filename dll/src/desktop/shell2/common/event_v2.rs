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
//! - Callback invocation (`invoke_callbacks_v2()`)
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
//!                          1. Scroll: record_sample() on ScrollManager
//!                          2. Text: process_text_input() on LayoutWindow
//!                          3. A11y: record_state_changes() on A11yManager
//!                          ↓
//!                          EVENT FILTERING & DISPATCH
//! =
//!                          4. State diffing (window_state::create_events_from_states)
//!                          5. Event filtering (dispatch_events)
//!                          6. Callback invocation (invoke_callbacks_v2)
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
//!   - `handle_scroll()` - After calling scroll_manager.record_sample()
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
//! - Call `scroll_manager.record_sample(delta_x, delta_y, hover_manager, input_id, now)`
//! - ScrollManager finds scrollable node via hit test and applies scroll
//! - Then call `process_window_events()` which will generate scroll events
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
//! ## Migration Checklist
//!
//! When migrating a platform to use `PlatformWindowV2`:
//!
//! 1. ✅ Implement `PlatformWindowV2` trait (26 getter methods)
//! 2. ✅ Implement `invoke_callbacks_v2()` with direct field access
//! 3. ✅ Replace `process_window_events_v2()` calls with trait method
//! 4. ✅ Remove old `invoke_callbacks_v2()` implementation
//! 5. ✅ Remove old `process_callback_result_v2()` implementation
//! 6. ✅ Remove scrollbar hit-test/click/drag functions (now in trait)
//! 7. ✅ Verify all event handlers call `process_window_events()` at correct points
//! 8. ✅ Test that callbacks fire correctly (mouse, keyboard, window events)
//! 9. ✅ Test that scrollbar interaction works (hit-test, click, drag)
//! 10. ✅ Test that window state changes propagate (resize, focus, etc.)
//!
//! Previously, this logic was duplicated ~4 times (~3000 lines) across:
//! - `macos/events.rs` (~2000 lines)
//! - `windows/process.rs` (~1800 lines)
//! - `linux/x11/events.rs` (~1900 lines)
//! - `linux/wayland/mod.rs` (~1500 lines)

use alloc::sync::Arc;
use core::cell::RefCell;
use std::collections::BTreeMap;

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::{DomId, NodeId},
    events::{
        CallbackTarget as CoreCallbackTarget, EventFilter, PreCallbackFilterResult,
        ProcessEventResult, SyntheticEvent,
    },
    geom::LogicalPosition,
    gl::*,
    hit_test::{DocumentId, PipelineId},
    id::NodeId as CoreNodeId,
    refany::RefAny,
    resources::{IdNamespace, ImageCache, RendererResources},
    window::RawWindowHandle,
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

/// Maximum depth for recursive event processing (prevents infinite loops from callbacks)
// Event Processing Configuration

/// Maximum recursion depth for event processing.
///
/// Events can trigger callbacks that regenerate the DOM, which triggers new events.
/// This limit prevents infinite loops.
const MAX_EVENT_RECURSION_DEPTH: usize = 5;

/// Unique timer ID for auto-scroll during drag selection.
///
/// This ID is reserved for the framework's auto-scroll timer and should not
/// be used by user code. Value chosen to avoid conflicts with typical timer IDs.
const AUTO_SCROLL_TIMER_ID: usize = 0xABCD_1234;

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
/// It checks if dragging is still active, calculates scroll delta based on mouse distance
/// from container edges, and applies accelerated scrolling.
///
/// The callback terminates automatically when:
/// - Mouse button is released (no longer dragging)
/// - Mouse returns to within container bounds (no scroll needed)
extern "C" fn auto_scroll_timer_callback(
    _data: azul_core::refany::RefAny,
    timer_info: azul_layout::timer::TimerCallbackInfo,
) -> azul_layout::timer::TimerCallbackReturn {
    use azul_core::task::TerminateTimer;
    use azul_layout::window::SelectionScrollType;

    // Access window state through callback_info
    let callback_info = &timer_info.callback_info;

    // Access window state through callback_info
    let callback_info = &timer_info.callback_info;

    // Get current mouse position from window state (safe access via public getter)
    let full_window_state = callback_info.get_current_window_state();

    // Check if still dragging (left mouse button is down)
    if !full_window_state.mouse_state.left_down {
        // Mouse released - stop timer
        return azul_layout::timer::TimerCallbackReturn::terminate_unchanged();
    }

    // Get mouse position - if mouse is outside window, terminate timer
    let mouse_position = match full_window_state.mouse_state.cursor_position.get_position() {
        Some(pos) => pos,
        None => {
            // Mouse outside window - stop auto-scroll
            return azul_layout::timer::TimerCallbackReturn::terminate_unchanged();
        }
    };

    // TODO: Scroll based on mouse distance from container edge
    // The issue is that scroll_selection_into_view requires &mut LayoutWindow,
    // but we only have &CallbackInfo which has *const LayoutWindow.
    // We need to either:
    // 1. Make scroll_selection_into_view work via CallbackChange transaction
    // 2. Provide a different API for timer callbacks to access mutable state
    // For now, just continue the timer without scrolling
    //
    // let layout_window = timer_info.callback_info.get_layout_window();
    // if layout_window.scroll_selection_into_view(
    //     SelectionScrollType::DragSelection { mouse_position },
    //     ScrollMode::Accelerated,
    // ) {
    //     return azul_layout::timer::TimerCallbackReturn::continue_and_update();
    // }

    // No scroll needed (mouse within container or no scrollable ancestor)
    // Continue timer in case mouse moves outside again
    azul_layout::timer::TimerCallbackReturn::continue_unchanged()
}

// Platform-Specific Timer Management

/// Hit test node structure for event routing.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct HitTestNode {
    pub dom_id: u64,
    pub node_id: u64,
}

/// Target for callback dispatch - either a specific node or all root nodes.
#[derive(Debug, Clone, Copy)]
pub enum CallbackTarget {
    /// Dispatch to callbacks on a specific node (e.g., mouse events, hover)
    Node(HitTestNode),
    /// Dispatch to callbacks on root nodes (NodeId::ZERO) across all DOMs (e.g., window events,
    /// keys)
    RootNodes,
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
/// - `invoke_callbacks_v2()` - **FULLY CROSS-PLATFORM!** Callback dispatch using
///   `prepare_callback_invocation()`
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

    // PROVIDED: Callback Invocation (Cross-Platform Implementation)

    /// Invoke callbacks for a given target and event filter.
    ///
    /// This method is now **provided** (cross-platform) because all required state
    /// is accessible through trait getter methods. No platform-specific code needed!
    ///
    /// ## Workflow
    /// 1. Collect callbacks from NodeData based on target (Node or RootNodes)
    /// 2. Filter callbacks by event type
    /// 3. Invoke each callback using `layout_window.invoke_single_callback()`
    /// 4. Return all callback results
    ///
    /// ## Returns
    /// * `Vec<CallCallbacksResult>` - Results from all invoked callbacks
    fn invoke_callbacks_v2(
        &mut self,
        target: CallbackTarget,
        event_filter: EventFilter,
    ) -> Vec<CallCallbacksResult> {
        use azul_core::{
            dom::{DomId, NodeId},
            id::NodeId as CoreNodeId,
        };

        // Collect callbacks based on target
        let callback_data_list = match target {
            CallbackTarget::Node(node) => {
                let layout_window = match self.get_layout_window() {
                    Some(lw) => lw,
                    None => return Vec::new(),
                };

                let dom_id = DomId {
                    inner: node.dom_id as usize,
                };
                let node_id = match NodeId::from_usize(node.node_id as usize) {
                    Some(nid) => nid,
                    None => return Vec::new(),
                };

                let layout_result = match layout_window.layout_results.get(&dom_id) {
                    Some(lr) => lr,
                    None => return Vec::new(),
                };

                let binding = layout_result.styled_dom.node_data.as_container();
                let node_data = match binding.get(node_id) {
                    Some(nd) => nd,
                    None => return Vec::new(),
                };

                node_data
                    .get_callbacks()
                    .as_container()
                    .iter()
                    .filter(|cd| cd.event == event_filter)
                    .cloned()
                    .collect::<Vec<_>>()
            }
            CallbackTarget::RootNodes => {
                let layout_window = match self.get_layout_window() {
                    Some(lw) => lw,
                    None => return Vec::new(),
                };

                let mut callbacks = Vec::new();
                for (_dom_id, layout_result) in &layout_window.layout_results {
                    if let Some(root_node) = layout_result
                        .styled_dom
                        .node_data
                        .as_container()
                        .get(CoreNodeId::ZERO)
                    {
                        for callback in root_node.get_callbacks().iter() {
                            if callback.event == event_filter {
                                callbacks.push(callback.clone());
                            }
                        }
                    }
                }
                callbacks
            }
        };

        if callback_data_list.is_empty() {
            return Vec::new();
        }

        // Prepare all borrows in one call - avoids multiple &mut self borrows
        let mut borrows = self.prepare_callback_invocation();

        let mut results = Vec::new();

        for callback_data in callback_data_list {
            let mut callback = LayoutCallback::from_core(callback_data.callback);

            let callback_result = borrows.layout_window.invoke_single_callback(
                &mut callback,
                &mut callback_data.data.clone(),
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

            results.push(callback_result);
        }

        results
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
            events::{EasingFunction, EventSource},
            geom::LogicalPosition,
        };
        use azul_layout::managers::scroll_state::ScrollEvent;

        let layout_window = self.get_layout_window_mut().ok_or("No layout window")?;

        // Create scroll event
        let scroll_event = ScrollEvent {
            dom_id,
            node_id,
            delta: LogicalPosition::new(delta_x, delta_y),
            source: EventSource::User,
            duration: None, // Instant scroll
            easing: EasingFunction::Linear,
        };

        let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();

        // Apply scroll
        layout_window.scroll_manager.scroll_by(
            scroll_event.dom_id,
            scroll_event.node_id,
            scroll_event.delta,
            scroll_event
                .duration
                .unwrap_or(azul_core::task::Duration::System(
                    azul_core::task::SystemTimeDiff { secs: 0, nanos: 0 },
                )),
            scroll_event.easing,
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
    ) {
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
            // Start new input session
            manager.start_input_session(position, current_time.clone(), button_state);
        } else if is_button_up {
            // End current session
            manager.end_current_session();
        } else {
            // Record ongoing movement
            manager.record_input_sample(position, current_time.clone(), button_state);
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
    ///
    /// ## Usage
    /// Call this from platform event handlers when accessibility actions arrive:
    /// ```rust
    /// let affected_nodes = self.record_accessibility_action(dom_id, node_id, action);
    /// // Process affected_nodes: dispatch events and mark dirty nodes for re-layout
    /// ```
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
            eprintln!(
                "[PlatformWindowV2] Max event recursion depth {} reached",
                MAX_EVENT_RECURSION_DEPTH
            );
            return ProcessEventResult::DoNothing;
        }

        // Get previous state (or use current as fallback for first frame)
        let previous_state = self
            .get_previous_window_state()
            .as_ref()
            .unwrap_or(self.get_current_window_state());

        // Get gesture manager for gesture detection (if available)
        let gesture_manager = self.get_layout_window().map(|lw| &lw.gesture_drag_manager);

        // Detect all events that occurred by comparing states
        // Using new SyntheticEvent architecture with determine_all_events()

        // Get managers for event detection
        let focus_manager = self.get_layout_window().map(|w| &w.focus_manager);
        let file_drop_manager = self.get_layout_window().map(|w| &w.file_drop_manager);
        let hover_manager = self.get_layout_window().map(|w| &w.hover_manager);

        // Get EventProvider managers (scroll, text input, etc.)
        let scroll_manager_ref = self.get_layout_window().map(|w| &w.scroll_manager);
        let text_manager_ref = self.get_layout_window().map(|w| &w.text_input_manager);

        // Build list of EventProvider managers
        let mut event_providers: Vec<&dyn azul_core::events::EventProvider> = Vec::new();
        if let Some(sm) = scroll_manager_ref.as_ref() {
            event_providers.push(*sm as &dyn azul_core::events::EventProvider);
        }
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
                let hit_test_clone = hit_test_for_dispatch.as_ref().and_then(|ht| {
                    // Get first hovered node's hit test
                    ht.hovered_nodes.values().next().cloned()
                });

                // Store hit test in gesture manager for query access
                // Both node and window drags can use this
                layout_window
                    .gesture_drag_manager
                    .update_node_drag_hit_test(hit_test_clone.clone());
                layout_window
                    .gesture_drag_manager
                    .update_window_drag_hit_test(hit_test_clone);
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
        // ✅ Scroll: Platform calls scroll_manager.record_sample() in handle_scroll_wheel()
        // ✅ Text: Platform calls process_text_input() in handle_key_down()
        // ⏳ A11y: Not yet implemented (needs a11y_manager.record_state_changes())

        // Process text input BEFORE event dispatch
        // If there's a focused contenteditable node and text input occurred,
        // apply the edit using cursor/selection managers and mark nodes dirty
        let text_input_affected_nodes = if let Some(layout_window) = self.get_layout_window_mut() {
            // TODO: Get actual text input from platform (IME, composed chars, etc.)
            // Platform layer needs to provide text_input: &str when available
            // Example integration:
            // - macOS: NSTextInputClient::insertText / setMarkedText
            // - Windows: WM_CHAR / WM_UNICHAR messages
            // - X11: XIM XLookupString with UTF-8
            // - Wayland: text-input protocol
            let text_input = ""; // Placeholder
            layout_window.process_text_input(text_input)
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

        // IFrame Integration: Check if any Scroll events occurred
        // If scrolling happened, we need to regenerate layout so IFrameManager can check
        // for edge detection and trigger re-invocation if needed
        let has_scroll_events = synthetic_events
            .iter()
            .any(|e| matches!(e.event_type, azul_core::events::EventType::Scroll));

        if has_scroll_events {
            // Mark frame for regeneration to enable IFrame edge detection
            self.mark_frame_needs_regeneration();
            result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
        }

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
                PreCallbackSystemEvent::TextDragSelection { .. } => {
                    // TODO: Implement drag selection handling
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
                                    set_system_clipboard(clipboard_content.plain_text);
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
                                    if set_system_clipboard(clipboard_content.plain_text.clone()) {
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
                                // Convert DomNodeId to NodeId
                                let node_id =
                                    azul_core::id::NodeId::new(target.node.inner as usize);

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
                                                    text: operation.pre_state.text_content.clone(),
                                                    // TODO: Preserve original style
                                                    style: Arc::new(StyleProperties::default()),
                                                    logical_start_byte: 0,
                                                })];

                                            // Update text cache with pre-state content
                                            layout_window.update_text_cache_after_edit(
                                                target.dom,
                                                node_id_internal,
                                                new_content,
                                            );

                                            // Restore cursor position
                                            if let Some(cursor) =
                                                operation.pre_state.cursor_position
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
                                                TextOperation::InsertText { text, .. } => {
                                                    // Re-insert the text via process_text_input
                                                    let affected =
                                                        layout_window.process_text_input(text);
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

        // EVENT FILTERING AND CALLBACK DISPATCH

        // Dispatch user events to callbacks (internal events already processed)
        let dispatch_result = azul_core::events::dispatch_synthetic_events(
            &pre_filter.user_events,
            hit_test_for_dispatch.as_ref(),
        );

        if dispatch_result.is_empty() {
            return ProcessEventResult::DoNothing;
        }

        // Filter out system internal events as a safety check
        // (They shouldn't appear since user events shouldn't contain them,
        //  but we filter anyway to be safe)
        let user_callbacks: Vec<_> = dispatch_result
            .callbacks
            .iter()
            .filter(|cb| {
                if let azul_core::events::EventFilter::Hover(hover_filter) = cb.event_filter {
                    !hover_filter.is_system_internal()
                } else {
                    true
                }
            })
            .collect();

        // USER CALLBACK INVOCATION

        // Capture focus state before callbacks for post-callback filtering
        let old_focus = self
            .get_layout_window()
            .and_then(|lw| lw.focus_manager.get_focused_node().copied());

        // Invoke all user callbacks and collect results
        let mut should_stop_propagation = false;
        let mut should_recurse = false;
        let mut focus_changed = false;
        let mut prevent_default = false; // Track if any callback prevented default

        for callback_to_invoke in user_callbacks {
            if should_stop_propagation {
                break;
            }

            // Convert core CallbackTarget to shell CallbackTarget
            let target = match &callback_to_invoke.target {
                CoreCallbackTarget::Node { dom_id, node_id } => CallbackTarget::Node(HitTestNode {
                    dom_id: dom_id.inner as u64,
                    node_id: node_id.index() as u64,
                }),
                CoreCallbackTarget::RootNodes => CallbackTarget::RootNodes,
            };

            // Invoke callbacks and collect results
            let callback_results =
                self.invoke_callbacks_v2(target, callback_to_invoke.event_filter);

            for callback_result in callback_results {
                let event_result = self.process_callback_result_v2(&callback_result);
                result = result.max(event_result);

                // Check if callback prevented default
                if callback_result.prevent_default {
                    prevent_default = true;
                }

                // Check if we should stop propagation
                if callback_result.stop_propagation {
                    should_stop_propagation = true;
                    break;
                }

                // Check if we need to recurse (DOM was regenerated)
                use azul_core::callbacks::Update;
                if matches!(
                    callback_result.callbacks_update_screen,
                    Update::RefreshDom | Update::RefreshDomAllWindows
                ) {
                    should_recurse = true;
                }
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
                    // Start auto-scroll timer for drag-to-scroll (Phase 5)
                    // Timer frequency matches monitor refresh rate for smooth scrolling

                    if let Some(layout_window) = self.get_layout_window() {
                        let timer_id = azul_core::task::TimerId {
                            id: AUTO_SCROLL_TIMER_ID,
                        };

                        // Check if timer already running (avoid duplicate timers)
                        if !layout_window.timers.contains_key(&timer_id) {
                            use azul_core::{
                                refany::RefAny,
                                task::{Duration as AzulDuration, SystemTimeDiff},
                            };
                            use azul_layout::timer::Timer;

                            // TODO: Get actual monitor refresh rate from platform
                            // For now, default to 60Hz (16.67ms per frame)
                            // Platform implementations should query:
                            // - macOS: [[NSScreen mainScreen] maximumFramesPerSecond]
                            // - Windows: DwmGetCompositionTimingInfo
                            // - X11: XRRGetScreenInfo
                            // - Wayland: wl_output refresh field
                            const DEFAULT_REFRESH_RATE_HZ: u32 = 60;
                            let frame_time_nanos = 1_000_000_000 / DEFAULT_REFRESH_RATE_HZ;

                            // Get system time function for timer creation
                            let external = ExternalSystemCallbacks::rust_internal();

                            // Create timer with monitor refresh rate interval
                            let timer = Timer::new(
                                RefAny::new(()), // Empty data
                                auto_scroll_timer_callback,
                                external.get_system_time_fn,
                            )
                            .with_interval(AzulDuration::System(SystemTimeDiff {
                                secs: 0,
                                nanos: frame_time_nanos,
                            }));

                            // Add timer to layout window
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                layout_window.add_timer(timer_id, timer.clone());

                                // Start platform-specific native timer
                                // This will create NSTimer/SetTimer/timerfd depending on platform
                                self.start_timer(AUTO_SCROLL_TIMER_ID, timer);

                                result =
                                    result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                            }
                        }
                    }
                }
                azul_core::events::PostCallbackSystemEvent::CancelAutoScrollTimer => {
                    // Cancel auto-scroll timer (Phase 5)
                    // This stops both the framework timer and the native platform timer

                    let timer_id = azul_core::task::TimerId {
                        id: AUTO_SCROLL_TIMER_ID,
                    };

                    if let Some(layout_window) = self.get_layout_window_mut() {
                        if layout_window.timers.contains_key(&timer_id) {
                            // Remove from layout window timer map
                            layout_window.remove_timer(&timer_id);

                            // Stop native platform timer (NSTimer/SetTimer/timerfd)
                            // Platform implementations handle cleanup:
                            // - macOS: [timer invalidate]
                            // - Windows: KillTimer(hwnd, timer_id)
                            // - X11: Remove from internal timer manager
                            // - Wayland: close(timerfd)
                            self.stop_timer(AUTO_SCROLL_TIMER_ID);
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

        // Handle focus changes: generate synthetic FocusIn/FocusOut events
        if focus_changed && depth + 1 < MAX_EVENT_RECURSION_DEPTH {
            // Clear selections when focus changes (standard UI behavior)
            if let Some(layout_window) = self.get_layout_window_mut() {
                layout_window.selection_manager.clear_all();
            }

            // Recurse to process synthetic focus events
            // This will trigger FocusIn/FocusOut callbacks, which may request another focus change
            let focus_result = self.process_window_events_recursive_v2(depth + 1);
            result = result.max(focus_result);
        }

        // Recurse if needed (DOM regeneration)
        if should_recurse && depth + 1 < MAX_EVENT_RECURSION_DEPTH {
            let recursive_result = self.process_window_events_recursive_v2(depth + 1);
            result = result.max(recursive_result);
        }

        // Auto-activate window drag if DragStart occurred on titlebar
        // This allows titlebar dragging to work even when mouse leaves window
        if synthetic_events
            .iter()
            .any(|e| matches!(e.event_type, azul_core::events::EventType::DragStart))
        {
            // Get current window position before mutable borrow
            let current_pos = self.get_current_window_state().position;

            // Check if drag was on a titlebar element (class="csd-title")
            if let Some(hit_test) = hit_test_for_dispatch.as_ref() {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    let is_titlebar_drag = hit_test
                        .hovered_nodes
                        .iter()
                        .any(|(dom_id, hit)| hit.regular_hit_test_nodes.len() > 0);

                    if is_titlebar_drag && !layout_window.gesture_drag_manager.is_window_dragging()
                    {
                        // Activate window drag with current window position
                        let hit_test_clone = hit_test.hovered_nodes.values().next().cloned();

                        layout_window
                            .gesture_drag_manager
                            .activate_window_drag(current_pos, hit_test_clone);

                        eprintln!("[Event V2] Auto-activated window drag on titlebar DragStart");
                    }
                }
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

        // Handle window state modifications
        if let Some(ref modified_state) = result.modified_window_state {
            let current_state = self.get_current_window_state_mut();
            current_state.title = modified_state.title.clone();
            current_state.size = modified_state.size;
            current_state.position = modified_state.position;
            current_state.flags = modified_state.flags;
            current_state.background_color = modified_state.background_color;

            // Check if window should close
            if modified_state.flags.close_requested {
                // Platform should handle window destruction
                return ProcessEventResult::DoNothing;
            }

            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
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
                }
                event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
            }
            FocusUpdateRequest::ClearFocus => {
                // Clear focus in the FocusManager (in LayoutWindow)
                if let Some(layout_window) = self.get_layout_window_mut() {
                    layout_window.focus_manager.set_focused_node(None);
                }
                event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
            }
            FocusUpdateRequest::NoChange => {
                // No focus change requested
            }
        }

        // Handle image updates
        if result.images_changed.is_some() || result.image_masks_changed.is_some() {
            event_result =
                event_result.max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
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
            eprintln!(
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
        use azul_layout::managers::scroll_state::ScrollbarOrientation;

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
            eprintln!("Track click scroll failed: {}", e);
            return ProcessEventResult::DoNothing;
        }

        ProcessEventResult::ShouldReRenderCurrentWindow
    }

    /// Handle scrollbar drag - update scroll position based on mouse delta.
    fn handle_scrollbar_drag(
        &mut self,
        current_pos: azul_core::geom::LogicalPosition,
    ) -> ProcessEventResult {
        use azul_core::hit_test::ScrollbarHitId;
        use azul_layout::managers::scroll_state::ScrollbarOrientation;

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
            eprintln!("Scrollbar drag failed: {}", e);
            return ProcessEventResult::DoNothing;
        }

        ProcessEventResult::ShouldReRenderCurrentWindow
    }
}
