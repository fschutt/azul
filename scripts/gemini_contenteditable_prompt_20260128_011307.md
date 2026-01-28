# Azul GUI Framework - ContentEditable Text Input Bug Analysis

## Context

I'm developing Azul, a desktop GUI framework in Rust with C bindings. I've implemented 
a contenteditable text input system but it has several critical bugs. I need your help
analyzing the code and identifying the root causes.

## The Bugs

After implementing the text input system, the following bugs were observed:

1. **Cursor not appearing on click**: Clicking on a contenteditable element focuses it
   (blue outline appears) but the text cursor doesn't start blinking.

2. **Double input bug**: Pressing 'j' inserts 'jj' (duplicated character).

3. **Wrong text input affected**: When typing in the first input, the SECOND input
   gets modified instead of the first one.

4. **Mouse move triggers horrible resize**: Moving the mouse causes the first text
   input to resize incorrectly and text explodes across multiple lines.

5. **Line breaking bug**: Single-line input breaks onto many lines, ignoring
   `white-space: nowrap` CSS property.

6. **No scroll into view**: Text doesn't scroll into view when typing.

## Debug Output Analysis

The debug output shows:
- `old_text` is ALWAYS "Hello World - Click here and type!" (34 chars) - it never updates
- This suggests the text input's internal state is not being updated between keystrokes
- The text should be accumulating, but it's resetting each time

## Architecture Overview

The text input flow should be:
1. OS key event → debug_server.rs or platform event handler
2. `create_text_input()` creates a `CallbackChange::CreateTextInput`
3. `apply_callback_changes()` in window.rs processes it
4. `process_text_input()` is called, which calls `process_text_input_on_focused()`
5. Result is stored in `text_input_triggered` field
6. `process_callback_result_v2()` in event_v2.rs invokes user callbacks
7. User callback receives `PendingTextEdit` via `CallbackInfo::getTextChangeset()`

## Recent Git Commits

```
f5ddf8b3 Add plan for full text input implementation and architecture
90f50a9e Debug contenteditable E2E test with _getTextChangeset
2417cb6e Minor compilation fixes
74428dd7 Track dirty text nodes and text constraints for relayout
f451c089 Add CallbackInfo::create_text_input to push synthetic text inputs
c96271be Add TextInput-via-API processing in event_v2
68d3cbe8 Add TextInput support in debug_server API
77cc2f1a inline -> inline_axis to avoid ident clash in C
378e4f95 Add PendingTextEdit to public API
9d30e2e3 feat(css): add -azul-caret-width property and white default caret color
6785e73c feat(debug): add CursorRect and SelectionRect to Debug API JSON
73dc48f1 feat(event_v2): integrate cursor blink timer with focus handling
05b44d12 feat(window): add cursor blink timer and IFC layout lookup
c5c2d7fe feat(cursor): add blink timer support and visibility management
d787ec2f fix(display_list): use unified IFC lookup for cursor/selection painting
```

## Relevant Source Files

### dll/src/desktop/shell2/common/event_v2.rs

```rust
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
//! When migrating a platform to use `PlatformWindowV2`.

use alloc::sync::Arc;
use core::cell::RefCell;
use std::collections::BTreeMap;

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::{DomId, NodeId},
    events::{
        CallbackTarget as CoreCallbackTarget, EventFilter, FocusEventFilter, PreCallbackFilterResult,
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
use crate::{log_debug, log_warn};

/// Maximum depth for recursive event processing (prevents infinite loops from callbacks)
// Event Processing Configuration

/// Maximum recursion depth for event processing.
///
/// Events can trigger callbacks that regenerate the DOM, which triggers new events.
/// This limit prevents infinite loops.
const MAX_EVENT_RECURSION_DEPTH: usize = 7;

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
) -> azul_core::callbacks::TimerCallbackReturn {
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
        return azul_core::callbacks::TimerCallbackReturn::terminate_unchanged();
    }

    // Get mouse position - if mouse is outside window, terminate timer
    let mouse_position = match full_window_state.mouse_state.cursor_position.get_position() {
        Some(pos) => pos,
        None => {
            // Mouse outside window - stop auto-scroll
            return azul_core::callbacks::TimerCallbackReturn::terminate_unchanged();
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
    //     return azul_core::callbacks::TimerCallbackReturn::continue_and_update();
    // }

    // No scroll needed (mouse within container or no scrollable ancestor)
    // Continue timer in case mouse moves outside again
    azul_core::callbacks::TimerCallbackReturn::continue_unchanged()
}

// Focus Restyle Helper

/// Apply focus change restyle and determine the ProcessEventResult.
///
/// This helper function consolidates the duplicated restyle logic that was
/// previously repeated for FocusNext/Previous/First/Last and ClearFocus handlers.
///
/// # Arguments
/// * `layout_window` - Mutable reference to the layout window
/// * `old_focus` - The node that is losing focus (if any)
/// * `new_focus` - The node that is gaining focus (if any)
///
/// # Returns
/// The appropriate ProcessEventResult based on what CSS properties changed.
fn apply_focus_restyle(
    layout_window: &mut LayoutWindow,
    old_focus: Option<NodeId>,
    new_focus: Option<NodeId>,
) -> ProcessEventResult {
    use azul_core::styled_dom::FocusChange;
    
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
        "[Event V2] Focus restyle: needs_layout={}, needs_display_list={}, changed_nodes={}",
        restyle_result.needs_layout,
        restyle_result.needs_display_list,
        restyle_result.changed_nodes.len()
    );
    
    // Determine ProcessEventResult based on what changed
    if restyle_result.needs_layout {
        ProcessEventResult::ShouldRegenerateDomCurrentWindow
    } else if restyle_result.needs_display_list {
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
    /// 5. Return all callback results
    ///
    /// ## Event Bubbling
    /// For hover events (clicks, mouse moves, etc.), this implements JavaScript-style
    /// event bubbling:
    /// 1. Find the deepest (target) node that was hit
    /// 2. Build a chain: target → parent → grandparent → ... → root
    /// 3. Invoke callbacks at each level in order
    /// 4. Stop propagation if a callback calls `stop_propagation()`
    ///
    /// ## Returns
    /// * `Vec<CallCallbacksResult>` - Results from all invoked callbacks
    fn invoke_callbacks_v2(
        &mut self,
        target: CallbackTarget,
        event_filter: EventFilter,
    ) -> Vec<CallCallbacksResult> {
        use azul_core::{
            callbacks::CoreCallbackData,
            dom::{DomId, NodeId},
            id::NodeId as CoreNodeId,
        };

        // Internal struct to track callback with its source node for bubbling
        #[derive(Clone)]
        struct NodeCallback {
            dom_id: DomId,
            node_id: NodeId,
            depth: usize, // 0 = target (deepest), higher = closer to root
            callback: CoreCallbackData,
        }

        // Collect callbacks based on target, now with node info for bubbling
        let node_callbacks: Vec<NodeCallback> = match target {
            CallbackTarget::Node(node) => {
                let layout_window = match self.get_layout_window() {
                    Some(lw) => lw,
                    None => return Vec::new(),
                };

                let dom_id = DomId {
                    inner: node.dom_id as usize,
                };
                // Note: node.node_id is 0-based, use NodeId::new() directly instead of from_usize
                // from_usize expects 1-based encoding (0=None, n=NodeId(n-1))
                let node_id = NodeId::new(node.node_id as usize);

                let layout_result = match layout_window.layout_results.get(&dom_id) {
                    Some(lr) => lr,
                    None => return Vec::new(),
                };

                let binding = layout_result.styled_dom.node_data.as_container();
                let node_data = match binding.get(node_id) {
                    Some(nd) => nd,
                    None => return Vec::new(),
                };

                // For targeted node, just collect its callbacks (no bubbling for explicit target)
                node_data
                    .get_callbacks()
                    .as_container()
                    .iter()
                    .filter(|cd| cd.event == event_filter)
                    .map(|cb| NodeCallback {
                        dom_id,
                        node_id,
                        depth: 0,
                        callback: cb.clone(),
                    })
                    .collect()
            }
            CallbackTarget::RootNodes => {
                let layout_window = match self.get_layout_window() {
                    Some(lw) => lw,
                    None => return Vec::new(),
                };

                let mut node_callbacks = Vec::new();

                // Check if this is a HoverEventFilter - if so, implement event bubbling
                let is_hover_event = matches!(event_filter, EventFilter::Hover(_));

                if is_hover_event {
                    // For hover events, implement JS-style event bubbling:
                    // Find deepest hit node, then bubble up to root
                    use azul_layout::managers::hover::InputPointId;

                    if let Some(hit_test) = layout_window
                        .hover_manager
                        .get_current(&InputPointId::Mouse)
                    {
                        for (dom_id, hit_test_data) in &hit_test.hovered_nodes {
                            if let Some(layout_result) = layout_window.layout_results.get(dom_id) {
                                let node_data_container =
                                    layout_result.styled_dom.node_data.as_container();
                                let node_hierarchy =
                                    layout_result.styled_dom.node_hierarchy.as_container();

                                // Find the deepest hit node (target)
                                // In regular_hit_test_nodes, the last node is typically the deepest
                                // but we should find the one with the maximum depth
                                let deepest_node = hit_test_data
                                    .regular_hit_test_nodes
                                    .iter()
                                    .max_by_key(|(node_id, _)| {
                                        // Count depth by traversing to root
                                        let mut depth = 0usize;
                                        let mut current = Some(**node_id);
                                        while let Some(nid) = current {
                                            depth += 1;
                                            current =
                                                node_hierarchy.get(nid).and_then(|h| h.parent_id());
                                        }
                                        depth
                                    });

                                if let Some((target_node_id, _)) = deepest_node {
                                    // Build event chain: target → parent → ... → root
                                    let mut current_node = Some(*target_node_id);
                                    let mut depth = 0usize;

                                    while let Some(node_id) = current_node {
                                        // Collect callbacks from this node
                                        if let Some(node_data) = node_data_container.get(node_id) {
                                            for callback in node_data.get_callbacks().iter() {
                                                if callback.event == event_filter {
                                                    node_callbacks.push(NodeCallback {
                                                        dom_id: *dom_id,
                                                        node_id,
                                                        depth,
                                                        callback: callback.clone(),
                                                    });
                                                }
                                            }
                                        }

                                        // Move to parent
                                        current_node =
                                            node_hierarchy.get(node_id).and_then(|h| h.parent_id());
                                        depth += 1;
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // For non-hover events (window events, etc.), search only root nodes
                    for (dom_id, layout_result) in &layout_window.layout_results {
                        if let Some(root_node) = layout_result
                            .styled_dom
                            .node_data
                            .as_container()
                            .get(CoreNodeId::ZERO)
                        {
                            for callback in root_node.get_callbacks().iter() {
                                if callback.event == event_filter {
                                    let node_id = match NodeId::from_usize(0) {
                                        Some(nid) => nid,
                                        None => continue,
                                    };
                                    node_callbacks.push(NodeCallback {
                                        dom_id: *dom_id,
                                        node_id,
                                        depth: 0,
                                        callback: callback.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
                node_callbacks
            }
        };

        if node_callbacks.is_empty() {
            return Vec::new();
        }

        // Sort by depth (0 = target first, then parents)
        // This ensures JS-style bubbling order: target → parent → grandparent → root
        let mut sorted_callbacks = node_callbacks;
        sorted_callbacks.sort_by_key(|nc| nc.depth);

        // Prepare all borrows in one call - avoids multiple &mut self borrows
        let mut borrows = self.prepare_callback_invocation();

        let mut results = Vec::new();

        for node_callback in sorted_callbacks {
            let mut callback = LayoutCallback::from_core(node_callback.callback.callback);

            let callback_result = borrows.layout_window.invoke_single_callback(
                &mut callback,
                &mut node_callback.callback.refany.clone(),
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

            // Check if stopPropagation() was called - if so, stop bubbling
            let should_stop = callback_result.stop_propagation;

            results.push(callback_result);

            if should_stop {
                // Stop event propagation - don't invoke callbacks on parent nodes
                break;
            }
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

        for (i, ev) in synthetic_events.iter().enumerate() {
        }

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
                PreCallbackSystemEvent::TextDragSelection {
                    start_position,
                    current_position,
                    is_dragging,
                    ..
                } => {
                    if *is_dragging {
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

        // EVENT FILTERING AND CALLBACK DISPATCH

        // DEBUG: Log user events
        for (i, ev) in pre_filter.user_events.iter().enumerate() {
        }
        
        // DEBUG: Check hit test
        if let Some(ref ht) = hit_test_for_dispatch {
            for (dom_id, dom_ht) in &ht.hovered_nodes {
                for (node_id, _) in dom_ht.regular_hit_test_nodes.iter() {
                }
            }
        } else {
        }

        // Dispatch user events to callbacks (internal events already processed)
        let dispatch_result = azul_core::events::dispatch_synthetic_events(
            &pre_filter.user_events,
            hit_test_for_dispatch.as_ref(),
        );

        for (i, cb) in dispatch_result.callbacks.iter().enumerate() {
        }

        if dispatch_result.is_empty() {
            // Return accumulated result from internal processing, not DoNothing
            // Internal events (text selection, keyboard shortcuts) may have set
            // result to ShouldReRenderCurrentWindow even if no user callbacks exist.
            return result;
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
                            use azul_layout::timer::{Timer, TimerCallbackType};

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
                            let timer = Timer::create(
                                RefAny::new(()), // Empty data
                                auto_scroll_timer_callback as TimerCallbackType,
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
       

... [FILE TRUNCATED - original size: 157371 bytes] ...
```

### dll/src/desktop/shell2/common/debug_server.rs

```rust
//! HTTP Debug Server for Azul
//!
//! This module provides an HTTP debug server that integrates with Azul's timer system
//! for cross-platform automated testing and debugging.
//!
//! ## Architecture
//!
//! The debug server is started in `App::create()` and runs on a background thread.
//! It accepts JSON commands on "/" and forwards them to the timer callback for
//! cross-platform processing via CallbackInfo.
//!
//! ## Usage
//!
//! ```bash
//! # Start app with debug server
//! AZUL_DEBUG=8765 cargo run --bin my_app
//!
//! # Send events (blocks until processed)
//! curl -X POST http://localhost:8765/ -d '{"type":"get_state"}'
//! ```

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

// Import the NativeScreenshotExt trait for native screenshots
use crate::desktop::native_screenshot::NativeScreenshotExt;

#[cfg(feature = "std")]
use std::sync::{mpsc, Arc, Mutex, OnceLock};

// ==================== Types ====================

/// Request from HTTP thread to timer callback
#[cfg(feature = "std")]
pub struct DebugRequest {
    pub request_id: u64,
    pub event: DebugEvent,
    pub window_id: Option<String>,
    pub wait_for_render: bool,
    pub response_tx: mpsc::Sender<DebugResponseData>,
}

/// Response data from timer callback to HTTP thread (internal)
#[cfg(feature = "std")]
#[derive(Debug, Clone)]
pub enum DebugResponseData {
    /// Successful response with optional data
    Ok {
        window_state: Option<WindowStateSnapshot>,
        data: Option<ResponseData>,
    },
    /// Error response
    Err(String),
}

/// Typed response data variants
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ResponseData {
    /// Screenshot data (base64 encoded PNG)
    Screenshot(ScreenshotData),
    /// Node CSS properties
    NodeCssProperties(NodeCssPropertiesResponse),
    /// Node layout
    NodeLayout(NodeLayoutResponse),
    /// All nodes layout
    AllNodesLayout(AllNodesLayoutResponse),
    /// DOM tree
    DomTree(DomTreeResponse),
    /// Node hierarchy
    NodeHierarchy(NodeHierarchyResponse),
    /// Layout tree
    LayoutTree(LayoutTreeResponse),
    /// Display list
    DisplayList(DisplayListResponse),
    /// Scroll states
    ScrollStates(ScrollStatesResponse),
    /// Scrollable nodes
    ScrollableNodes(ScrollableNodesResponse),
    /// Scroll node by delta result
    ScrollNodeBy(ScrollNodeByResponse),
    /// Scroll node to position result
    ScrollNodeTo(ScrollNodeToResponse),
    /// Scroll node into view result
    ScrollIntoView(ScrollIntoViewResponse),
    /// Hit test result
    HitTest(HitTestResponse),
    /// HTML string
    HtmlString(HtmlStringResponse),
    /// Log messages
    Logs(LogsResponse),
    /// Health check
    Health(HealthResponse),
    /// Find node result
    FindNode(FindNodeResponse),
    /// Click node result
    ClickNode(ClickNodeResponse),
    /// Scrollbar info result
    ScrollbarInfo(ScrollbarInfoResponse),
    /// Selection state result
    SelectionState(SelectionStateResponse),
    /// Full selection manager dump
    SelectionManagerDump(SelectionManagerDump),
    /// App state as JSON
    AppState(AppStateResponse),
    /// App state set result
    AppStateSet(AppStateSetResponse),
    /// Drag state from unified drag system
    DragState(DragStateResponse),
    /// Detailed drag context
    DragContext(DragContextResponse),
    /// Focus state (which node has keyboard focus)
    FocusState(FocusStateResponse),
    /// Cursor state (cursor position and blink state)
    CursorState(CursorStateResponse),
}

/// Metadata about a RefAny's type
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct RefAnyMetadata {
    /// The compiler-generated type ID
    pub type_id: u64,
    /// Human-readable type name (e.g., "app::MyStruct")
    pub type_name: String,
    /// Whether this RefAny supports JSON serialization
    pub can_serialize: bool,
    /// Whether this RefAny type supports JSON deserialization
    pub can_deserialize: bool,
    /// Number of active references to this data
    pub ref_count: usize,
}

/// Error information for RefAny operations
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "error_type", content = "message", rename_all = "snake_case")]
pub enum RefAnyError {
    /// Type does not support JSON serialization
    NotSerializable,
    /// Type does not support JSON deserialization
    NotDeserializable,
    /// Serde serialization/deserialization failed
    SerdeError(String),
    /// Valid JSON but cannot construct RefAny (type mismatch, missing fields, etc.)
    TypeConstructionError(String),
}

/// App state response (JSON serialized) with full metadata
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppStateResponse {
    /// Metadata about the RefAny type
    pub metadata: RefAnyMetadata,
    /// The serialized JSON data (null if serialization failed or not supported)
    pub state: serde_json::Value,
    /// Error message if serialization failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RefAnyError>,
}

/// App state set result
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppStateSetResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Optional error details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RefAnyError>,
}

/// Screenshot response data
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScreenshotData {
    /// Base64 encoded PNG with data URI prefix
    pub data: String,
}

/// Hit test response
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct HitTestResponse {
    pub x: f32,
    pub y: f32,
    pub node_id: Option<u64>,
    pub node_tag: Option<String>,
}

/// Find node response - returns location and size of found node
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct FindNodeResponse {
    pub found: bool,
    pub node_id: Option<u64>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub tag: Option<String>,
    pub classes: Option<Vec<String>>,
}

/// Click node response
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClickNodeResponse {
    pub success: bool,
    pub message: String,
}

/// HTML string response
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct HtmlStringResponse {
    pub html: String,
}

/// Logs response
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct LogsResponse {
    pub logs: Vec<LogMessage>,
}

/// Health check response
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthResponse {
    pub port: u16,
    pub pending_logs: usize,
    pub logs: Vec<LogMessageJson>,
}

/// JSON-friendly log message
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct LogMessageJson {
    pub timestamp_us: u64,
    pub level: String,
    pub category: String,
    pub message: String,
}

/// HTTP response wrapper for serialization
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status")]
pub enum DebugHttpResponse {
    #[serde(rename = "ok")]
    Ok(DebugHttpResponseOk),
    #[serde(rename = "error")]
    Error(DebugHttpResponseError),
}

/// Successful HTTP response body
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DebugHttpResponseOk {
    pub request_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_state: Option<WindowStateSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ResponseData>,
}

/// Error HTTP response body
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DebugHttpResponseError {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<u64>,
    pub message: String,
}

/// A log message
#[derive(Debug, Clone)]
pub struct LogMessage {
    pub timestamp_us: u64,
    pub level: LogLevel,
    pub category: LogCategory,
    pub message: String,
    pub location: String,
    pub window_id: Option<String>,
}

#[cfg(feature = "std")]
impl serde::Serialize for LogMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("LogMessage", 6)?;
        s.serialize_field("timestamp_us", &self.timestamp_us)?;
        s.serialize_field("level", &format!("{:?}", self.level))?;
        s.serialize_field("category", &format!("{:?}", self.category))?;
        s.serialize_field("message", &self.message)?;
        s.serialize_field("location", &self.location)?;
        s.serialize_field("window_id", &self.window_id)?;
        s.end()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogCategory {
    General,
    Window,
    EventLoop,
    Input,
    Layout,
    Text,
    DisplayList,
    Rendering,
    Resources,
    Callbacks,
    Timer,
    DebugServer,
    Platform,
}

/// Snapshot of window state for response
#[derive(Debug, Clone)]
pub struct WindowStateSnapshot {
    pub window_id: String,
    pub logical_width: f32,
    pub logical_height: f32,
    pub physical_width: u32,
    pub physical_height: u32,
    pub dpi: u32,
    pub hidpi_factor: f32,
    pub focused: bool,
    pub dom_node_count: usize,
    pub focused_node: Option<u64>,
}

#[cfg(feature = "std")]
impl serde::Serialize for WindowStateSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("WindowStateSnapshot", 10)?;
        s.serialize_field("window_id", &self.window_id)?;
        s.serialize_field("logical_width", &self.logical_width)?;
        s.serialize_field("logical_height", &self.logical_height)?;
        s.serialize_field("physical_width", &self.physical_width)?;
        s.serialize_field("physical_height", &self.physical_height)?;
        s.serialize_field("dpi", &self.dpi)?;
        s.serialize_field("hidpi_factor", &self.hidpi_factor)?;
        s.serialize_field("focused", &self.focused)?;
        s.serialize_field("dom_node_count", &self.dom_node_count)?;
        s.serialize_field("focused_node", &self.focused_node)?;
        s.end()
    }
}

// ==================== Response Data Structures ====================

/// Response for GetNodeCssProperties
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeCssPropertiesResponse {
    pub node_id: u64,
    pub property_count: usize,
    pub properties: Vec<String>,
}

/// Response for GetNodeLayout
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct NodeLayoutResponse {
    pub node_id: u64,
    pub size: Option<LogicalSizeJson>,
    pub position: Option<LogicalPositionJson>,
    pub rect: Option<LogicalRectJson>,
}

/// Response for GetAllNodesLayout
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct AllNodesLayoutResponse {
    pub dom_id: u32,
    pub node_count: usize,
    pub nodes: Vec<NodeLayoutInfo>,
}

/// Layout info for a single node
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeLayoutInfo {
    pub node_id: usize,
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub rect: Option<LogicalRectJson>,
}

/// Response for GetDomTree
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct DomTreeResponse {
    pub dom_id: u32,
    pub node_count: usize,
    pub dpi: u32,
    pub hidpi_factor: f32,
    pub logical_width: f32,
    pub logical_height: f32,
}

/// Response for GetNodeHierarchy
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeHierarchyResponse {
    pub root: i64,
    pub node_count: usize,
    pub nodes: Vec<HierarchyNodeInfo>,
}

/// Hierarchy info for a single node
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct HierarchyNodeInfo {
    pub index: usize,
    #[serde(rename = "type")]
    pub node_type: String,
    pub text: Option<String>,
    pub parent: i64,
    pub prev_sibling: i64,
    pub next_sibling: i64,
    pub last_child: i64,
    pub children: Vec<usize>,
}

/// Response for GetLayoutTree
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct LayoutTreeResponse {
    pub root: usize,
    pub node_count: usize,
    pub nodes: Vec<LayoutNodeInfo>,
}

/// Layout tree info for a single node
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct LayoutNodeInfo {
    pub layout_idx: usize,
    pub dom_idx: i64,
    #[serde(rename = "type")]
    pub node_type: String,
    pub is_anonymous: bool,
    pub anonymous_type: Option<String>,
    pub formatting_context: String,
    pub parent: i64,
    pub children: Vec<usize>,
}

/// Response for GetDisplayList
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DisplayListResponse {
    pub total_items: usize,
    pub rect_count: usize,
    pub text_count: usize,
    pub border_count: usize,
    pub image_count: usize,
    pub other_count: usize,
    pub items: Vec<DisplayListItemInfo>,
    /// Clip chain analysis - shows push/pop balance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_analysis: Option<ClipChainAnalysis>,
}

/// Clip chain analysis for debugging clipping issues
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClipChainAnalysis {
    /// Final clip depth (should be 0 if balanced)
    pub final_clip_depth: i32,
    /// Final scroll depth (should be 0 if balanced)
    pub final_scroll_depth: i32,
    /// Final stacking context depth (should be 0 if balanced)
    pub final_stacking_depth: i32,
    /// Whether all push/pop pairs are balanced
    pub balanced: bool,
    /// List of clip operations in order
    pub operations: Vec<ClipOperation>,
}

/// A single clip/scroll/stacking operation
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClipOperation {
    /// Index in display list
    pub index: usize,
    /// Operation type
    pub op: String,
    /// Clip depth after this operation
    pub clip_depth: i32,
    /// Scroll depth after this operation
    pub scroll_depth: i32,
    /// Stacking context depth after this operation
    pub stacking_depth: i32,
    /// Bounds if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<LogicalRectJson>,
    /// Content size (for scroll frames)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_size: Option<LogicalSizeJson>,
    /// Scroll ID (for scroll frames)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_id: Option<u64>,
}

/// Display list item info
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DisplayListItemInfo {
    pub index: usize,
    #[serde(rename = "type")]
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glyph_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z_index: Option<i32>,
    /// Current clip depth when this item is rendered
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_depth: Option<i32>,
    /// Current scroll depth when this item is rendered
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_depth: Option<i32>,
    /// Content size (for scroll frames)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_size: Option<LogicalSizeJson>,
    /// Scroll ID (for scroll frames)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_id: Option<u64>,
    /// Debug info string (for debugging scrollbar bounds, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_info: Option<String>,
    /// Border colors per side (for border items)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_colors: Option<BorderColorsJson>,
    /// Border widths per side (for border items)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_widths: Option<BorderWidthsJson>,
}

/// Border colors for all four sides (JSON output)
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct BorderColorsJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<String>,
}

/// Border widths for all four sides (JSON output)
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct BorderWidthsJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<f32>,
}

/// Response for GetScrollStates
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScrollStatesResponse {
    pub scroll_node_count: usize,
    pub scroll_states: Vec<ScrollStateInfo>,
}

/// Scroll state info for a single node
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ScrollStateInfo {
    pub node_id: usize,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub content_width: f32,
    pub content_height: f32,
    pub container_width: f32,
    pub container_height: f32,
    pub max_scroll_x: f32,
    pub max_scroll_y: f32,
}

/// Response for GetScrollableNodes
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScrollableNodesResponse {
    pub scrollable_node_count: usize,
    pub scrollable_nodes: Vec<ScrollableNodeInfo>,
}

/// Scrollable node info
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ScrollableNodeInfo {
    pub node_id: usize,
    pub dom_node_id: Option<usize>,
    pub container_width: f32,
    pub container_height: f32,
    pub can_scroll_x: bool,
    pub can_scroll_y: bool,
}

/// Response for ScrollNodeBy
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ScrollNodeByResponse {
    pub scrolled: bool,
    pub node_id: u64,
    pub delta_x: f32,
    pub delta_y: f32,
}

/// Response for ScrollNodeTo
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ScrollNodeToResponse {
    pub scrolled: bool,
    pub node_id: u64,
    pub x: f32,
    pub y: f32,
}

/// Response for ScrollIntoView
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ScrollIntoViewResponse {
    pub scrolled: bool,
    pub node_id: u64,
    pub adjustments_count: usize,
}

/// Response for GetScrollbarInfo - detailed scrollbar geometry and state
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScrollbarInfoResponse {
    /// Whether a scrollbar was found
    pub found: bool,
    /// Node ID of the scrollable element
    pub node_id: u64,
    /// DOM node ID (may differ from layout node ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dom_node_id: Option<u64>,
    /// Requested orientation
    pub orientation: String,
    /// Horizontal scrollbar info (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizontal: Option<ScrollbarGeometry>,
    /// Vertical scrollbar info (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical: Option<ScrollbarGeometry>,
    /// Current scroll position
    pub scroll_x: f32,
    pub scroll_y: f32,
    /// Maximum scroll values
    pub max_scroll_x: f32,
    pub max_scroll_y: f32,
    /// Container (viewport) rect
    pub container_rect: LogicalRectJson,
    /// Content rect (total scrollable area)
    pub content_rect: LogicalRectJson,
}

/// Detailed scrollbar geometry for hit-testing and automation
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ScrollbarGeometry {
    /// Is this scrollbar visible?
    pub visible: bool,
    /// The full track rect (includes buttons at each end)
    pub track_rect: LogicalRectJson,
    /// Center of the track (for clicking)
    pub track_center: LogicalPositionJson,
    /// Base size (button width/height)
    pub button_size: f32,
    /// Top/Left button rect
    pub top_button_rect: LogicalRectJson,
    /// Bottom/Right button rect  
    pub bottom_button_rect: LogicalRectJson,
    /// Thumb rect (the draggable part)
    pub thumb_rect: LogicalRectJson,
    /// Center of the thumb (for dragging)
    pub thumb_center: LogicalPositionJson,
    /// Thumb position ratio (0.0 = top/left, 1.0 = bottom/right)
    pub thumb_position_ratio: f32,
    /// Thumb size ratio (relative to track)
    pub thumb_size_ratio: f32,
}

/// Response for GetSelectionState - text selection state
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionStateResponse {
    /// Whether any selection exists
    pub has_selection: bool,
    /// Number of DOMs with selections
    pub selection_count: usize,
    /// Selections per DOM
    pub selections: Vec<DomSelectionInfo>,
}

/// Selection info for a single DOM
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DomSelectionInfo {
    /// DOM ID
    pub dom_id: u32,
    /// Node that contains the selection
    pub node_id: Option<u64>,
    /// CSS selector path to the node (e.g. "div#main > p.intro")
    pub selector: Option<String>,
    /// Selection ranges within this DOM
    pub ranges: Vec<SelectionRangeInfo>,
    /// Selection rectangles (visual bounds of each selected region)
    pub rectangles: Vec<LogicalRectJson>,
}

/// Information about a single selection range
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionRangeInfo {
    /// Selection type: "cursor", "range", or "block"
    pub selection_type: String,
    /// For cursor: the cursor position (character index)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_position: Option<usize>,
    /// For range: start character index
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<usize>,
    /// For range: end character index
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<usize>,
    /// Direction: "forward", "backward", or "none"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
}

/// JSON-serializable LogicalSize
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct LogicalSizeJson {
    pub width: f32,
    pub height: f32,
}

/// JSON-serializable LogicalPosition
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct LogicalPositionJson {
    pub x: f32,
    pub y: f32,
}

/// JSON-serializable LogicalRect
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct LogicalRectJson {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Full dump of the SelectionManager for debugging
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionManagerDump {
    /// All selections indexed by DOM ID
    pub selections: Vec<SelectionDumpEntry>,
    /// Click state for multi-click detection
    pub click_state: ClickStateDump,
}

/// Single selection entry in the dump
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionDumpEntry {
    /// DOM ID
    pub dom_id: u32,
    /// Node ID 
    pub node_id: Option<u64>,
    /// CSS selector for the node
    pub selector: Option<String>,
    /// All selections on this node
    pub selections: Vec<SelectionDump>,
}

/// Dump of a single Selection
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct SelectionDump {
    /// "cursor" or "range"
    pub selection_type: String,
    /// Raw debug representation
    pub debug: String,
}

/// Dump of click state
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClickStateDump {
    /// Last clicked node
    pub last_node: Option<String>,
    /// Last click position
    pub last_position: LogicalPositionJson,
    /// Last click time in ms
    pub last_time_ms: u64,
    /// Current click count (1=single, 2=double, 3=triple)
    pub click_count: u8,
}

/// Response for GetDragState - current drag state from unified drag system
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DragStateResponse {
    /// Whether any drag is currently active
    pub is_dragging: bool,
    /// Type of active drag (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drag_type: Option<String>,
    /// Brief description of the drag state
    pub description: String,
}

/// Response for GetDragContext - detailed drag context information
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DragContextResponse {
    /// Whether any drag is currently active
    pub is_dragging: bool,
    /// Type of active drag (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drag_type: Option<String>,
    /// Start position of the drag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_position: Option<LogicalPositionJson>,
    /// Current position of the drag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_position: Option<LogicalPositionJson>,
    /// Target node ID (for node drags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_node_id: Option<u64>,
    /// Target DOM ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_dom_id: Option<u32>,
    /// Scrollbar axis (for scrollbar drags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrollbar_axis: Option<String>,
    /// Window resize edge (for window resize drags)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resize_edge: Option<String>,
    /// Files being dragged (for file drops)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
    /// Drag data (MIME type -> data)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drag_data: Option<std::collections::BTreeMap<String, String>>,
    /// Current drag effect
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drag_effect: Option<String>,
    /// Full debug representation
    pub debug: String,
}

/// Response for GetFocusState - which node has keyboard focus
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct FocusStateResponse {
    /// Whether any node has focus
    pub has_focus: bool,
    /// Focused node information (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused_node: Option<FocusedNodeInfo>,
}

/// Information about the focused node
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct FocusedNodeInfo {
    /// DOM ID
    pub dom_id: u32,
    /// Node ID within the DOM
    pub node_id: u64,
    /// CSS selector for the node
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// Whether the node is contenteditable
    pub is_contenteditable: bool,
    /// Text content of the node (if text node)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_content: Option<String>,
}

/// Response for GetCursorState - cursor position and blink state
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct CursorStateResponse {
    /// Whether a cursor is active
    pub has_cursor: bool,
    /// Cursor information (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<CursorInfo>,
}

/// Information about the text cursor
#[cfg(feature = "std")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct CursorInfo {
    /// DOM ID where cursor is located
    pub dom_id: u32,
    /// Node ID within the DOM
    pub node_id: u64,
    /// Cursor position (grapheme cluster index)
    pub position: usize,
    /// Cursor affinity ("upstream" or "downstream")
    pub affinity: String,
    /// Whether the cursor is currently visible (false during blink off phase)
    pub is_visible: bool,
    /// Whether the cursor blink timer is active
    pub blink_timer_active: bool,
}

// ==================== Debug Events ====================

#[derive(Debug, Clone)]
#[cfg_attr(feature = "std", derive(serde::Deserialize))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum DebugEvent {
    // Mouse Events
    MouseMove {
        x: f32,
        y: f32,
    },
    MouseDown {
        x: f32,
        y: f32,
        #[serde(default)]
        button: MouseButton,
    },
    MouseUp {
        x: f32,
        y: f32,
        #[serde(default)]
        button: MouseButton,
    },
    Click {
        /// X position (used if no selector/node_id provided)
        #[serde(default)]
        x: Option<f32>,
        /// Y position (used if no selector/node_id provided)
        #[serde(default)]
        y: Option<f32>,
        /// CSS selector (e.g. ".button", "#my-id", "div")
        #[serde(default)]
        selector: Option<String>,
        /// Direct node ID to click
        #[serde(default)]
        node_id: Option<u64>,
        /// Text content to find and click
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        button: MouseButton,
    },
    DoubleClick {
        x: f32,
        y: f32,
        #[serde(default)]
        button: MouseButton,
    },
    Scroll {
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
    },

    // Keyboard Events
    KeyDown {
        key: String,
        #[serde(default)]
        modifiers: Modifiers,
    },
    KeyUp {
        key: String,
        #[serde(default)]
        modifiers: Modifiers,
    },
    TextInput {
        text: String,
    },

    // Window Events
    Resize {
        width: f32,
        height: f32,
    },
    Move {
        x: i32,
        y: i32,
    },
    Focus,
    Blur,
    Close,
    DpiChanged {
        dpi: u32,
    },

    // Queries
    GetState,
    GetDom,
    HitTest {
        x: f32,
        y: f32,
    },
    GetLogs {
        #[serde(default)]
        since_request_id: Option<u64>,
    },

    // DOM Inspection
    /// Get the HTML representation of the DOM
    GetHtmlString,
    /// Get all computed CSS properties for a node (supports selector, node_id, or text)
    GetNodeCssProperties {
        #[serde(default)]
        node_id: Option<u64>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        text: Option<String>,
    },
    /// Get node layout information (position, size) - supports selector, node_id, or text
    GetNodeLayout {
        #[serde(default)]
        node_id: Option<u64>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        text: Option<String>,
    },
    /// Get all nodes with their layout info
    GetAllNodesLayout,
    /// Get detailed DOM tree structure
    GetDomTree,
    /// Get the raw node hierarchy (for debugging DOM structure issues)
    GetNodeHierarchy,
    /// Get the layout tree structure (for debugging layout tree building)
    GetLayoutTree,
    /// Get the display list items (what's actually being rendered)
    GetDisplayList,
    /// Get all scroll states (scroll positions for scrollable nodes)
    GetScrollStates,
    /// Get all scrollable nodes (nodes with overflow that can be scrolled)
    GetScrollableNodes,
    /// Scroll a specific node by a delta amount (supports selector, node_id, or text)
    ScrollNodeBy {
        #[serde(default)]
        node_id: Option<u64>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        text: Option<String>,
        delta_x: f32,
        delta_y: f32,
    },
    /// Scroll a specific node to an absolute position (supports selector, node_id, or text)
    ScrollNodeTo {
        #[serde(default)]
        node_id: Option<u64>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        text: Option<String>,
        x: f32,
        y: f32,
    },
    /// Scroll a node into view (W3C scrollIntoView API)
    /// Scrolls the element into the visible area of its scroll container
    ScrollIntoView {
        #[serde(default)]
        node_id: Option<u64>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        text: Option<String>,
        /// Vertical alignment: "start", "center", "end", "nearest" (default)
        #[serde(default)]
        block: Option<String>,
        /// Horizontal alignment: "start", "center", "end", "nearest" (default)
        #[serde(default)]
        inline: Option<String>,
        /// Animation: "auto" (default), "instant", "smooth"
        #[serde(default)]
        behavior: Option<String>,
    },

    // Node Finding
    /// Find a node by text content (returns node_id and bounds)
    FindNodeByText {
        text: String,
    },
    /// Click on a specific node by its ID (deprecated, use Click with node_id)
    ClickNode {
        node_id: u64,
        #[serde(default)]
        button: MouseButton,
    },

    /// Get detailed scrollbar information for a node (supports selector, node_id, or text)
    /// Returns geometry for both horizontal and vertical scrollbars if present
    GetScrollbarInfo {
        #[serde(default)]
        node_id: Option<u64>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        text: Option<String>,
        /// Which scrollbar to query: "horizontal", "vertical", or "both" (default)
        #[serde(default)]
        orientation: Option<String>,
    },

    // Selection
    /// Get the current text selection state (selection ranges, cursor positions)
    GetSelectionState,
    /// Dump the entire selection manager state for debugging
    DumpSelectionManager,

    // Drag State
    /// Get the current drag state from the unified drag system
    GetDragState,
    /// Get detailed drag context information (for debugging drag operations)
    GetDragContext,

    // Control
    Relayout,
    Redraw,

    // Testing
    WaitFrame,
    Wait {
        ms: u64,
    },

    // Screenshots
    TakeScreenshot,
    TakeNativeScreenshot,

    // App State (JSON Serialization)
    /// Get the global app state as JSON (requires RefAny with serialize_fn)
    GetAppState,
    /// Set the global app state from JSON (requires RefAny with deserialize_fn)
    SetAppState {
        /// The JSON value to set as the new app state
        state: serde_json::Value,
    },

    // Focus and Cursor State
    /// Get the current focus state (which node has keyboard focus)
    GetFocusState,
    /// Get the current cursor state (position, blink state)
    GetCursorState,
}

// ==================== Node Resolution Helper ====================

/// Resolves a node target (selector, node_id, or text) to a NodeId.
/// Returns the first matching node or None if no match found.
#[cfg(feature = "std")]
fn resolve_node_target(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    selector: Option<&str>,
    node_id: Option<u64>,
    text: Option<&str>,
) -> Option<azul_core::id::NodeId> {
    use azul_core::dom::DomId;
    use azul_core::id::NodeId;

    let dom_id = DomId { inner: 0 };

    // Direct node ID
    if let Some(nid) = node_id {
        return Some(NodeId::new(nid as usize));
    }

    // CSS selector
    if let Some(sel) = selector {
        use azul_core::style::matches_html_element;
        use azul_css::parser2::parse_css_path;

        let layout_window = callback_info.get_layout_window();
        if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
            if let Ok(css_path) = parse_css_path(sel) {
                let styled_dom = &layout_result.styled_dom;
                let node_hierarchy = styled_dom.node_hierarchy.as_container();
                let node_data = styled_dom.node_data.as_container();
                let cascade_info = styled_dom.cascade_info.as_container();

                for i in 0..node_data.len() {
                    let node_id = NodeId::new(i);
                    if matches_html_element(
                        &css_path,
                        node_id,
                        &node_hierarchy,
                        &node_data,
                        &cascade_info,
                        None,
                    ) {
                        return Some(node_id);
                    }
                }
            }
        }
    }

    // Text content
    if let Some(txt) = text {
        let layout_window = callback_info.get_layout_window();
        if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
            let styled_dom = &layout_result.styled_dom;
            let node_data = styled_dom.node_data.as_container();

            for i in 0..node_data.len() {
                let data = &node_data[NodeId::new(i)];
                if let azul_core::dom::NodeType::Text(t) = data.get_node_type() {
                    if t.as_str().contains(txt) {
                        return Some(NodeId::new(i));
                    }
                }
            }
        }
    }

    None
}

/// Builds a CSS selector string for a node (e.g., "div#my-id.class1.class2")
/// Returns a selector that can be used to find this node again
#[cfg(feature = "std")]
fn build_selector_for_node(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    dom_id: azul_core::dom::DomId,
    node_id: azul_core::id::NodeId,
) -> Option<String> {
    use alloc::string::ToString;

    let layout_window = callback_info.get_layout_window();
    let layout_result = layout_window.layout_results.get(&dom_id)?;
    let styled_dom = &layout_result.styled_dom;
    let node_data_container = styled_dom.node_data.as_container();
    
    if node_id.index() >= node_data_container.len() {
        return None;
    }
    
    let node_data = &node_data_container[node_id];
    
    // Get tag name from NodeTypeTag (lowercase HTML tag name)
    let node_type_tag = node_data.get_node_type().get_path();
    let tag_name = alloc::format!("{:?}", node_type_tag).to_lowercase();
    
    let mut selector = tag_name;
    
    // Add ID if present (first ID wins)
    let ids_and_classes = node_data.get_ids_and_classes();
    for idc in ids_and_classes.iter() {
        if let Some(id) = idc.as_id() {
            selector.push('#');
            selector.push_str(id);
            break; // Only one ID
        }
    }
    
    // Add all classes
    for idc in ids_and_classes.iter() {
        if let Some(class) = idc.as_class() {
            selector.push('.');
            selector.push_str(class);
        }
    }
    
    // If no ID or classes, add node index to make it unique
    let has_id_or_class = ids_and_classes.iter().any(|idc| idc.as_id().is_some() || idc.as_class().is_some());
    if !has_id_or_class {
        selector.push_str(&alloc::format!(":nth-child({})", node_id.index() + 1));
    }
    
    Some(selector)
}

/// Resolves a node target to center position (x, y) for clicking
#[cfg(feature = "std")]
fn resolve_node_center(
    callback_info: &azul_layout::callbacks::CallbackInfo,
    selector: Option<&str>,
    node_id: Option<u64>,
    text: Option<&str>,
) -> Option<(f32, f32)> {
    use azul_core::dom::{DomId, DomNodeId};
    use azul_core::id::NodeId;

    let dom_id = DomId { inner: 0 };

    if let Some(nid) = resolve_node_target(callback_info, selector, node_id, text) {
        let dom_node_id = DomNodeId {
            dom: dom_id,
            node: Some(nid).into(),
        };
        if let Some(rect) = callback_info.get_node_rect(dom_node_id) {
            return Some((
                rect.origin.x + rect.size.width / 2.0,
                rect.origin.y + rect.size.height / 2.0,
            ));
        }
    }

    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
pub struct Modifiers {
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub meta: bool,
}

// ==================== Global State ====================

#[cfg(feature = "std")]
static REQUEST_QUEUE: OnceLock<Mutex<VecDeque<DebugRequest>>> = OnceLock::new();

#[cfg(feature = "std")]
static LOG_QUEUE: OnceLock<Mutex<Vec<LogMessage>>> = OnceLock::new();

#[cfg(feature = "std")]
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(feature = "std")]
static SERVER_START_TIME: OnceLock<std::time::Instant> = OnceLock::new();

#[cfg(feature = "std")]
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "std")]
static DEBUG_PORT: OnceLock<u16> = OnceLock::new();

// ==================== Debug Server Handle ====================

/// Handle to the debug server for clean shutdown
#[cfg(feature = "std")]
pub struct DebugServerHandle {
    pub shutdown_tx: mpsc::Sender<()>,
    pub thread_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
    pub port: u16,
}

#[cfg(feature = "std")]
impl std::fmt::Debug for DebugServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DebugServerHandle")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "std")]
impl DebugServerHandle {
    /// Signal the server to shut down
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
        // Give the server thread a moment to exit
        if let Ok(mut guard) = self.thread_handle.lock() {
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }
    }
}

#[cfg(feature = "std")]
impl Drop for DebugServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ==================== Public API ====================

/// Check if debug mode is enabled
#[cfg(feature = "std")]
pub fn is_debug_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::SeqCst)
}

/// Get debug server port from environment
///
/// The `AZUL_DEBUG` environment variable should be set to a port number (e.g., `AZUL_DEBUG=8765`).
/// Ports below 1024 require root/administrator privileges.
/// Returns `None` if not set or not a valid port number.
#[cfg(feature = "std")]
pub fn get_debug_port() -> Option<u16> {
    std::env::var("AZUL_DEBUG")
        .ok()
        .and_then(|s| s.parse().ok())
}

/// Initialize and start the debug server.
///
/// This function:
/// 1. Binds to the port (exits process if port is taken)
/// 2. Starts the HTTP server thread
/// 3. Blocks until the server is ready to accept connections
/// 4. Returns a handle for clean shutdown
///
/// Should be called from App::create().
#[cfg(feature = "std")]
pub fn start_debug_server(port: u16) -> DebugServerHandle {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    // Initialize static state
    SERVER_START_TIME.get_or_init(std::time::Instant::now);
    REQUEST_QUEUE.get_or_init(|| Mutex::new(VecDeque::new()));
    LOG_QUEUE.get_or_init(|| Mutex::new(Vec::new()));
    let _ = DEBUG_PORT.set(port);
    DEBUG_ENABLED.store(true, Ordering::SeqCst);

    // Try to bind - exit if port is taken
    let listener = match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(l) => l,
        Err(e) => {
            std::process::exit(1);
        }
    };

    // Set a short timeout for accept() so we can check for shutdown
    listener.set_nonblocking(false).ok();

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

    // Channel to signal when server is ready
    let (ready_tx, ready_rx) = mpsc::channel::<()>();

    // Start server thread
    let thread_handle = thread::Builder::new()
        .name("azul-debug-server".to_string())
        .spawn(move || {
            // Signal that we're ready
            let _ = ready_tx.send(());

            // Set a timeout on the listener so we can check for shutdown
            listener.set_nonblocking(true).ok();

            log_internal(
                LogLevel::Info,
                LogCategory::DebugServer,
                format!("Debug server listening on http://127.0.0.1:{}", port),
                None,
            );

            loop {
                // Check for shutdown signal (non-blocking)
                if shutdown_rx.try_recv().is_ok() {
                    log_internal(
                        LogLevel::Info,
                        LogCategory::DebugServer,
                        "Debug server shutting down",
                        None,
                    );
                    break;
                }

                // Try to accept a connection (non-blocking)
                match listener.accept() {
                    Ok((mut stream, _addr)) => {
                        // NOTE: Stream explicitly set to blocking mode
                        // The listener is non-blocking, but accepted streams may inherit this.
                        // This causes the final read loop to fail immediately with WouldBlock,
                        // closing the socket before the client has read all data.
                        stream.set_nonblocking(false).ok();
                        // Set read timeout
                        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                        // Increase write timeout to 30s for large screenshot transfers
                        stream.set_write_timeout(Some(Duration::from_secs(30))).ok();
                        handle_http_connection(&mut stream);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // No connection pending, sleep a bit
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => {
                        // Other error, continue
                        thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        })
        .expect("Failed to spawn debug server thread");

    // Wait for server to be ready
    let _ = ready_rx.recv_timeout(Duration::from_secs(5));

    // Verify server is actually accepting connections
    for _ in 0..10 {
        if TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", port).parse().unwrap(),
            Duration::from_millis(100),
        )
        .is_ok()
        {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    log_internal(
        LogLevel::Info,
        LogCategory::DebugServer,
        format!("Debug server ready on http://127.0.0.1:{}", port),
        None,
    );

    DebugServerHandle {
        shutdown_tx,
        thread_handle: Mutex::new(Some(thread_handle)),
        port,
    }
}

/// Log a message (thread-safe, lock-free when debug is disabled)
#[cfg(feature = "std")]
#[track_caller]
pub fn log(
    level: LogLevel,
    category: LogCategory,
    message: impl Into<String>,
    window_id: Option<&str>,
) {
    if !is_debug_enabled() {
        return;
    }
    log_internal(level, category, message, window_id);
}

#[cfg(feature = "std")]
#[track_caller]
fn log_internal(
    level: LogLevel,
    category: LogCategory,
    message: impl Into<String>,
    window_id: Option<&str>,
) {
    let location = core::panic::Location::caller();
    let timestamp_us = SERVER_START_TIME
        .get()
        .map(|t| t.elapsed().as_micros() as u64)
        .unwrap_or(0);

    let msg = LogMessage {
        timestamp_us,
        level,
        category,
        message: message.into(),
        location: format!("{}:{}", location.file(), location.line()),
        window_id: window_id.map(String::from),
    };

    if let Some(queue) = LOG_QUEUE.get() {
        if let Ok(mut q) = queue.lock() {
            q.push(msg);
        }
    }
}

/// Pop a debug request from the queue (called by timer callback)
#[cfg(feature = "std")]
pub fn pop_request() -> Option<DebugRequest> {
    REQUEST_QUEUE.get()?.lock().ok()?.pop_front()
}

/// Take all log messages
#[cfg(feature = "std")]
pub fn take_logs() -> Vec<LogMessage> {
    if let Some(queue) = LOG_QUEUE.get() {
        if let Ok(mut q) = queue.lock() {
            return core::mem::take(&mut *q);
        }
    }
    Vec::new()
}

/// Send a successful response to a debug request
#[cfg(feature = "std")]
pub fn send_ok(
    request: &DebugRequest,
    window_state: Option<WindowStateSnapshot>,
    data: Option<ResponseData>,
) {
    // Clear logs to prevent memory buildup
    let _ = take_logs();
    let response = DebugResponseData::Ok { window_state, data };
    if let Err(e) = request.response_tx.send(response) {
    }
}

/// Send an error response to a debug request
#[cfg(feature = "std")]
pub fn send_err(request: &DebugRequest, message: impl Into<String>) {
    // Clear logs to prevent memory buildup
    let _ = take_logs();
    let response = DebugResponseData::Err(message.into());
    if let Err(e) = request.response_tx.send(response) {
    }
}

/// Helper function for serializing DebugHttpResponse
#[cfg(feature = "std")]
fn serialize_http_response(response: &DebugHttpResponse) -> String {
    serde_json::to_string_pretty(response)
        .unwrap_or_else(|_| r#"{"status":"error","message":"Serialization failed"}"#.to_string())
}

// ==================== HTTP Server ====================

#[cfg(feature = "std")]
fn handle_http_connection(stream: &mut std::net::TcpStream) {
    use std::io::{Read, Write};

    let mut buffer = [0u8; 16384];
    let bytes_read = match stream.read(&mut buffer) {
        Ok(n) if n > 0 => n,
        _ => return,
    };

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);

    // Parse HTTP request
    let lines: Vec<&str> = request.lines().collect();
    if lines.is_empty() {
        return;
    }

    let first_line = lines[0];
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }

    let method = parts[0];
    let path = parts[1];

    let response_json = match (method, path) {
        // Health check - GET /
        ("GET", "/") | ("GET", "/health") => {
            let logs = take_logs();
            let health = HealthResponse {
                port: DEBUG_PORT.get().copied().unwrap_or(0),
                pending_logs: logs.len(),
                logs: logs
                    .iter()
                    .map(|l| LogMessageJson {
                        timestamp_us: l.timestamp_us,
                        level: format!("{:?}", l.level),
                        category: format!("{:?}", l.category),
                        message: l.message.clone(),
                    })
                    .collect(),
            };
            serialize_http_response(&DebugHttpResponse::Ok(DebugHttpResponseOk {
                request_id: 0,
                window_state: None,
                data: Some(ResponseData::Health(health)),
            }))
        }

        // Event handling - POST /
        ("POST", "/") => {
            // Parse body
            let body_start = request
                .find("\r\n\r\n")
                .map(|i| i + 4)
                .or_else(|| request.find("\n\n").map(|i| i + 2));

            if let Some(start) = body_start {
                let body = &request[start..];
                handle_event_request(body)
            } else {
                serialize_http_response(&DebugHttpResponse::Error(DebugHttpResponseError {
                    request_id: None,
                    message: "No request body".to_string(),
                }))
            }
        }

        _ => serialize_http_response(&DebugHttpResponse::Error(DebugHttpResponseError {
            request_id: None,
            message: "Use GET / for status or POST / with JSON body".to_string(),
        })),
    };

    // Calculate length for Content-Length header
    let body_bytes = response_json.as_bytes();
    let header = format!(
        "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );

    // Set NoDelay to push packets immediately
    stream.set_nodelay(true).ok();

    // 1. Write Header (Small, safe to write all at once)
    if stream.write_all(header.as_bytes()).is_err() {
        return;
    }

    // 2. Write Body in Chunks (Safer for large data like screenshots)
    let mut bytes_written = 0usize;
    for chunk in body_bytes.chunks(8192) {
        match stream.write_all(chunk) {
            Ok(_) => {
                bytes_written += chunk.len();
            }
            Err(e) => {
                return;
            }
        }
    }

    // 3. Flush ensures data is in the kernel buffer
    if stream.flush().is_err() {
        return;
    }

    // Graceful Shutdown Pattern
    // 1. Shutdown WRITE side only. This sends TCP FIN to the client.
    if stream.shutdown(std::net::Shutdown::Write).is_err() {
        return;
    }

    // 2. Read until EOF. This keeps the socket alive until the client
    //    confirms receipt and closes their end. This prevents the OS
    //    from destroying the socket while data is still in flight (RST).
    let mut buffer = [0u8; 512];
    while let Ok(n) = stream.read(&mut buffer) {
        if n == 0 {
            break;
        } // EOF received, client closed connection
    }
}

#[cfg(feature = "std")]
fn handle_event_request(body: &str) -> String {
    use std::time::Duration;

    // Parse the event request
    #[derive(serde::Deserialize)]
    struct EventRequest {
        #[serde(flatten)]
        event: DebugEvent,
        #[serde(default)]
        window_id: Option<String>,
        #[serde(default)]
        wait_for_render: bool,
    }

    let parsed: Result<EventRequest, _> = serde_json::from_str(body);

    match parsed {
        Ok(req) => {
            // Create request and channel
            let (tx, rx) = mpsc::channel();
            let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst);

            let request = DebugRequest {
                request_id,
                event: req.event,
                window_id: req.window_id,
                wait_for_render: req.wait_for_render,
                response_tx: tx,
            };

            // Push to queue
            if let Some(queue) = REQUEST_QUEUE.get() {
                if let Ok(mut q) = queue.lock() {
                    q.push_back(request);
                }
            }

            // Wait for response (with timeout)
            match rx.recv_timeout(Duration::from_secs(30)) {
                Ok(response_data) => {
                    let http_response = match response_data {
                        DebugResponseData::Ok { window_state, data } => {
                            DebugHttpResponse::Ok(DebugHttpResponseOk {
                                request_id,
                                window_state,
                                data,
                            })
                        }
                        DebugResponseData::Err(message) => DebugHttpResponse::Error(DebugHttpResponseError {
                            request_id: Some(request_id),
                            message,
                        }),
                    };
                    serialize_http_response(&http_response)
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    serialize_http_response(&DebugHttpResponse::Error(DebugHttpResponseError {
                        request_id: Some(request_id),
                        message: "Timeout waiting for response (is the timer running?)".to_string(),
                    }))
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    serialize_http_response(&DebugHttpResponse::Error(DebugHttpResponseError {
                        request_id: Some(request_id),
                        message: "Event loop disconnected".to_string(),
                    }))
                }
            }
        }
        Err(e) => serialize_http_response(&DebugHttpResponse::Error(DebugHttpResponseError {
            request_id: None,
            message: format!("Invalid JSON: {}", e),
        })),
    }
}

// ==================== Timer Callback ====================

/// Timer callback that processes debug requests.
/// Called every ~16ms when debug mode is enabled.
#[cfg(feature = "std")]
pub extern "C" fn debug_timer_callback(
    mut timer_data: azul_core::refany::RefAny,
    mut timer_info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    use azul_core::callbacks::{TimerCallbackReturn, Update};
    use azul_core::task::TerminateTimer;

    // Check queue length first (without popping)
    let _queue_len = REQUEST_QUEUE
        .get()
        .and_then(|q| q.lock().ok())
        .map(|q| q.len())
        .unwrap_or(0);

    // Process all pending requests
    let mut needs_update = false;
    let mut _processed_count = 0;

    while let Some(request) = pop_request() {
        log(
            LogLevel::Debug,
            LogCategory::DebugServer,
            format!("Processing: {:?}", request.event),
            request.window_id.as_deref(),
        );

        // Pass the app_data (stored in timer_data) to process_debug_event
        let result = process_debug_event(&request, &mut timer_info.callback_info, &mut timer_data);
        needs_update = needs_update || result;
        _processed_count += 1;
    }

    TimerCallbackReturn {
        should_update: if needs_update {
            Update::RefreshDom
        } else {
            Update::DoNothing
        },
        should_terminate: TerminateTimer::Continue,
    }
}

/// Process a single debug event
#[cfg(feature = "std")]
fn build_clip_analysis(
    items: &[azul_layout::solver3::display_list::DisplayListItem],
) -> ClipChainAnalysis {
    use azul_layout::solver3::display_list::DisplayListItem;

    let mut clip_depth = 0i32;
    let mut scroll_depth = 0i32;
    let mut stacking_depth = 0i32;
    let mut operations = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        let op_info = match item {
            DisplayListItem::PushClip { bounds, .. } => {
                clip_depth += 1;
                Some(ClipOperation {
                    index: idx,
                    op: "PushClip".to_string(),
                    clip_depth,
                    scroll_depth,
                    stacking_depth,
                    bounds: Some(LogicalRectJson {
                        x: bounds.origin.x,
                        y: bounds.origin.y,
                        width: bounds.size.width,
                        height: bounds.size.height,
                    }),
                    content_size: None,
                    scroll_id: None,
                })
            }
            DisplayListItem::PopClip => {
                let op = ClipOperation {
                    index: idx,
                    op: "PopClip".to_string(),
                    clip_depth: clip_depth - 1,
                    scroll_depth,
                    stacking_depth,
                    bounds: None,
                    content_size: None,
                    scroll_id: None,
                };
                clip_depth -= 1;
                Some(op)
            }
            DisplayListItem::PushScrollFrame {
                clip_bounds,
                content_size,
                scroll_id,
            } => {
                scroll_depth += 1;
                Some(ClipOperation {
                    index: idx,
                    op: "PushScrollFrame".to_string(),
                    clip_depth,
                    scroll_depth,
                    stacking_depth,
                    bounds: Some(LogicalRectJson {
                        x: clip_bounds.origin.x,
                        y: clip_bounds.origin.y,
                        width: clip_bounds.size.width,
                        height: clip_bounds.size.height,
                    }),
                    content_size: Some(LogicalSizeJson {
                        width: content_size.width,
                        height: content_size.height,
                    }),
                    scroll_id: Some(*scroll_id),
                })
            }
            DisplayListItem::PopScrollFrame => {
                let op = ClipOperation {
                    index: idx,
                    op: "PopScrollFrame".to_string(),
                    clip_depth,
                    scroll_depth: scroll_depth - 1,
                    stacking_depth,
                    bounds: None,
                    content_size: None,
                    scroll_id: None,
                };
                scroll_depth -= 1;
                Some(op)
            }
            DisplayListItem::PushStackingContext { bounds, .. } => {
                stacking_depth += 1;
                Some(ClipOperation {
                    index: idx,
                    op: "PushStackingContext".to_string(),
                    clip_depth,
                    scroll_depth,
                    stacking_depth,
                    bounds: Some(LogicalRectJson {
                        x: bounds.origin.x,
                        y: bounds.origin.y,
                        width: bounds.size.width,
                        height: bounds.size.height,
                    }),
                    content_size: None,
                    scroll_id: None,
                })
            }
            DisplayListItem::PopStackingContext => {
                let op = ClipOperation {
                    index: idx,
                    op: "PopStackingContext".to_string(),
                    clip_depth,
                    scroll_depth,
                    stacking_depth: stacking_depth - 1,
                    bounds: None,
                    content_size: None,
                    scroll_id: None,
                };
                stacking_depth -= 1;
                Some(op)
            }
            _ => None,
        };

        if let Some(op) = op_info {
            operations.push(op);
        }
    }

    ClipChainAnalysis {
        final_clip_depth: clip_depth,
        final_scroll_depth: scroll_depth,
        final_stacking_depth: stacking_depth,
        balanced: clip_depth == 0 && scroll_depth == 0 && stacking_depth == 0,
        operations,
    }
}

/// Parse a key string to a VirtualKeyCode
#[cfg(feature = "std")]
fn parse_virtual_keycode(key: &str) -> Option<azul_core::window::VirtualKeyCode> {
    use azul_core::window::VirtualKeyCode;
    
    match key.to_lowercase().as_str() {
        // Letters
        "a" => Some(VirtualKeyCode::A),
        "b" => Some(VirtualKeyCode::B),
        "c" => Some(VirtualKeyCode::C),
        "d" => Some(VirtualKeyCode::D),
        "e" => Some(VirtualKeyCode::E),
        "f" => Some(VirtualKeyCode::F),
        "g" => Some(VirtualKeyCode::G),
        "h" => Some(VirtualKeyCode::H),
        "i" => Some(VirtualKeyCode::I),
        "j" => Some(VirtualKeyCode::J),
        "k" => Some(VirtualKeyCode::K),
        "l" => Some(VirtualKeyCode::L),
        "m" => Some(VirtualKeyCode::M),
        "n" => Some(VirtualKeyCode::N),
        "o" => Some(VirtualKeyCode::O),
        "p" => Some(VirtualKeyCode::P),
        "q" => Some(VirtualKeyCode::Q),
        "r" => Some(VirtualKeyCode::R),
        "s" => Some(VirtualKeyCode::S),
        "t" => Some(VirtualKeyCode::T),
        "u" => Some(VirtualKeyCode::U),
        "v" => Some(VirtualKeyCode::V),
        "w" => Some(VirtualKeyCode::W),
        "x" => Some(VirtualKeyCode::X),
        "y" => Some(VirtualKeyCode::Y),
        "z" => Some(VirtualKeyCode::Z),
        
        // Numbers
        "0" | "key0" => Some(VirtualKeyCode::Key0),
        "1" | "key1" => Some(VirtualKeyCode::Key1),
        "2" | "key2" => Some(VirtualKeyCode::Key2),
        "3" | "key3" => Some(VirtualKeyCode::Key3),
        "4" | "key4" => Some(VirtualKeyCode::Key4),
        "5" | "key5" => Some(VirtualKeyCode::Key5),
        "6" | "key6" => Some(VirtualKeyCode::Key6),
        "7" | "key7" => Some(VirtualKeyCode::Key7),
        "8" | "key8" => Some(VirtualKeyCode::Key8),
        "9" | "key9" => Some(VirtualKeyCode::Key9),
        
        // Special keys
        "tab" => Some(VirtualKeyCode::Tab),
        "enter" | "return" => Some(VirtualKeyCode::Return),
        "space" | " " => Some(VirtualKeyCode::Space),
        "escape" | "esc" => Some(VirtualKeyCode::Escape),
        "backspace" | "back" => Some(VirtualKeyCode::Back),
        "delete" => Some(VirtualKeyCode::Delete),
        "insert" => Some(VirtualKeyCode::Insert),
        "home" => Some(VirtualKeyCode::Home),
        "end" => Some(VirtualKeyCode::End),
        "pageup" | "page_up" => Some(VirtualKeyCode::PageUp),
        "pagedown" | "page_down" => Some(VirtualKeyCode::PageDown),
        
        // Arrow keys
        "arrowup" | "up" => Some(VirtualKeyCode::Up),
        "arrowdown" | "down" => Some(VirtualKeyCode::Down),
        "arrowleft" | "left" => Some(VirtualKeyCode::Left),
        "arrowright" | "right" => Some(VirtualKeyCode::Right),
        
        // Function keys
        "f1" => Some(VirtualKeyCode::F1),
        "f2" => Some(VirtualKeyCode::F2),
        "f3" => Some(VirtualKeyCode::F3),
        "f4" => Some(VirtualKeyCode::F4),
        "f5" => Some(VirtualKeyCode::F5),
        "f6" => Some(VirtualKeyCode::F6),
        "f7" => Some(VirtualKeyCode::F7),
        "f8" => Some(VirtualKeyCode::F8),
        "f9" => Some(VirtualKeyCode::F9),
        "f10" => Some(VirtualKeyCode::F10),
        "f11" => Some(VirtualKeyCode::F11),
        "f12" => Some(VirtualKeyCode::F12),
        
        // Modifier keys (for explicit key presses)
        "shift" | "lshift" => Some(VirtualKeyCode::LShift),
        "rshift" => Some(VirtualKeyCode::RShift),
        "ctrl" | "control" | "lctrl" | "lcontrol" => Some(VirtualKeyCode::LControl),
        "rctrl" | "rcontrol" => Some(VirtualKeyCode::RControl),
        "alt" | "lalt" => Some(VirtualKeyCode::LAlt),
        "ralt" => Some(VirtualKeyCode::RAlt),
        "meta" | "super" | "lwin" | "lmeta" => Some(VirtualKeyCode::LWin),
        "rwin" | "rmeta" => Some(VirtualKeyCode::RWin),
        
        _ => None,
    }
}

/// Process a single debug event
#[cfg(feature = "std")]
fn process_debug_event(
    request: &DebugRequest,
    callback_info: &mut azul_layout::callbacks::CallbackInfo,
    app_data: &mut azul_core::refany::RefAny,
) -> bool {
    use azul_core::geom::{LogicalPosition, LogicalSize};

    let mut needs_update = false;

    match &request.event {
        DebugEvent::GetState => {
            let window_state = callback_info.get_current_window_state();
            let size = &window_state.size;
            let physical = size.get_physical_size();
            let hidpi = size.get_hidpi_factor();
            let window_id_str = window_state.window_id.as_str();
            
            // Get the focused node from the focus manager
            let focused_node_raw = callback_info.get_focused_node();
            let focused_node = focused_node_raw
                .and_then(|dom_node_id| dom_node_id.node.into_crate_internal())
                .map(|node_id| node_id.index() as u64);

            let snapshot = WindowStateSnapshot {
                window_id: window_id_str.to_string(),
                logical_width: size.dimensions.width,
                logical_height: size.dimensions.height,
                physical_width: physical.width,
                physical_height: physical.height,
                dpi: size.dpi,
                hidpi_factor: hidpi.inner.get(),
                focused: window_state.flags.has_focus,
                dom_node_count: 0,
                focused_node,
            };

            send_ok(request, Some(snapshot), None);
        }

        DebugEvent::Resize { width, height } => {
            log(
                LogLevel::Info,
                LogCategory::Window,
                format!("Resizing to {}x{}", width, height),
                None,
            );

            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.size.dimensions = LogicalSize::new(*width, *height);
            callback_info.modify_window_state(new_state);
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::MouseMove { x, y } => {
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Debug mouse move to ({}, {})", x, y),
                None,
            );

            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });
            callback_info.modify_window_state(new_state);
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::MouseDown { x, y, button } => {
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Debug mouse down at ({}, {}) button {:?}", x, y, button),
                None,
            );

            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });
            match button {
                MouseButton::Left => new_state.mouse_state.left_down = true,
                MouseButton::Right => new_state.mouse_state.right_down = true,
                MouseButton::Middle => new_state.mouse_state.middle_down = true,
            }
            callback_info.modify_window_state(new_state);
            needs_update = true;

            // Text selection is now handled automatically by the normal event pipeline.
            // When modify_window_state is called, it triggers process_callback_result_v2
            // which detects mouse_state_changed and calls process_window_events_recursive_v2.
            // This generates a TextClick internal event with the correct position from mouse_state.

            send_ok(request, None, None);
        }

        DebugEvent::MouseUp { x, y, button } => {
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Debug mouse up at ({}, {}) button {:?}", x, y, button),
                None,
            );

            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });
            match button {
                MouseButton::Left => new_state.mouse_state.left_down = false,
                MouseButton::Right => new_state.mouse_state.right_down = false,
                MouseButton::Middle => new_state.mouse_state.middle_down = false,
            }
            callback_info.modify_window_state(new_state);
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::Click {
            x,
            y,
            button,
            selector,
            node_id,
            text,
        } => {
            use azul_core::dom::{DomId, DomNodeId};
            use azul_core::id::NodeId;

            // Resolve the click target position
            let click_pos: Option<(f32, f32)> = if let (Some(x), Some(y)) = (x, y) {
                // Direct position provided
                Some((*x, *y))
            } else if let Some(nid) = node_id {
                // Click by node ID - use hit test bounds from display list
                let dom_id = DomId { inner: 0 };
                let dom_node_id = DomNodeId {
                    dom: dom_id,
                    node: Some(NodeId::new(*nid as usize)).into(),
                };
                if let Some(rect) = callback_info.get_node_hit_test_bounds(dom_node_id) {
                    Some((
                        rect.origin.x + rect.size.width / 2.0,
                        rect.origin.y + rect.size.height / 2.0,
                    ))
                } else {
                    None
                }
            } else if let Some(sel) = selector {
                // Click by CSS selector using matches_html_element
                use azul_core::style::matches_html_element;
                use azul_css::parser2::parse_css_path;

                let dom_id = DomId { inner: 0 };
                let layout_window = callback_info.get_layout_window();
                let mut found = None;

                if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                    // Parse the CSS selector string into a CssPath
                    if let Ok(css_path) = parse_css_path(sel.as_str()) {
                        let styled_dom = &layout_result.styled_dom;
                        let node_hierarchy = styled_dom.node_hierarchy.as_container();
                        let node_data = styled_dom.node_data.as_container();
                        let cascade_info = styled_dom.cascade_info.as_container();
                        let node_count = node_data.len();

                        // Iterate through all nodes and find the first match
                        for i in 0..node_count {
                            let node_id = NodeId::new(i);
                            if matches_html_element(
                                &css_path,
                                node_id,
                                &node_hierarchy,
                                &node_data,
                                &cascade_info,
                                None, // No expected pseudo-selector
                            ) {
                                let dom_node_id = DomNodeId {
                                    dom: dom_id.clone(),
                                    node: Some(NodeId::new(i)).into(),
                                };
                                // Use get_node_hit_test_bounds for reliable positions from display list
                                if let Some(rect) =
                                    callback_info.get_node_hit_test_bounds(dom_node_id)
                                {
                                    found = Some((
                                        rect.origin.x + rect.size.width / 2.0,
                                        rect.origin.y + rect.size.height / 2.0,
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                }
                found
            } else if let Some(txt) = text {
                // Click by text content
                let dom_id = DomId { inner: 0 };
                let layout_window = callback_info.get_layout_window();
                let mut found = None;

                if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                    let styled_dom = &layout_result.styled_dom;
                    let node_data = styled_dom.node_data.as_container();
                    let node_count = node_data.len();

                    for i in 0..node_count {
                        let data = &node_data[NodeId::new(i)];
                        if let azul_core::dom::NodeType::Text(t) = data.get_node_type() {
                            if t.as_str().contains(txt.as_str()) {
                                // For text nodes, get the parent's rect (the container)
                                let dom_node_id = DomNodeId {
                                    dom: dom_id.clone(),
                                    node: Some(NodeId::new(i)).into(),
                                };
                                // Try parent first (text nodes might not have rects)
                                let hierarchy = styled_dom.node_hierarchy.as_container();
                                let node_hier = &hierarchy[NodeId::new(i)];
                                let parent_idx = if node_hier.parent > 0 {
                                    node_hier.parent - 1
                                } else {
                                    i
                                };
                                let parent_dom_node_id = DomNodeId {
                                    dom: dom_id.clone(),
                                    node: Some(NodeId::new(parent_idx)).into(),
                                };
                                // Use get_node_hit_test_bounds for reliable positions from display list
                                if let Some(rect) =
                                    callback_info.get_node_hit_test_bounds(parent_dom_node_id)
                                {
                                    found = Some((
                                        rect.origin.x + rect.size.width / 2.0,
                                        rect.origin.y + rect.size.height / 2.0,
                                    ));
                                    break;
                                } else if let Some(rect) =
                                    callback_info.get_node_hit_test_bounds(dom_node_id)
                                {
                                    found = Some((
                                        rect.origin.x + rect.size.width / 2.0,
                                        rect.origin.y + rect.size.height / 2.0,
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                }
                found
            } else {
                None
            };

            match click_pos {
                Some((cx, cy)) => {
                    log(
                        LogLevel::Debug,
                        LogCategory::EventLoop,
                        format!("Debug click at ({}, {}) button {:?}", cx, cy, button),
                        None,
                    );

                    // Click = mouse move + mouse down + mouse up at same position
                    // We use queue_window_state_sequence to ensure each state change
                    // is processed separately, allowing the event system to detect
                    // the transitions (down→up) and trigger the appropriate callbacks.
                    let base_state = callback_info.get_current_window_state().clone();

                    // State 1: Move cursor to position
                    let mut move_state = base_state.clone();
                    move_state.mouse_state.cursor_position =
                        azul_core::window::CursorPosition::InWindow(LogicalPosition {
                            x: cx,
                            y: cy,
                        });

                    // State 2: Mouse button down
                    let mut down_state = move_state.clone();
                    match button {
                        MouseButton::Left => down_state.mouse_state.left_down = true,
                        MouseButton::Right => down_state.mouse_state.right_down = true,
                        MouseButton::Middle => down_state.mouse_state.middle_down = true,
                    }

                    // State 3: Mouse button up (this triggers MouseUp event)
                    let mut up_state = down_state.clone();
                    match button {
                        MouseButton::Left => up_state.mouse_state.left_down = false,
                        MouseButton::Right => up_state.mouse_state.right_down = false,
                        MouseButton::Middle => up_state.mouse_state.middle_down = false,
                    }

                    // Queue all states to be applied in sequence across frames
                    callback_info
                        .queue_window_state_sequence(vec![move_state, down_state, up_state]);
                    needs_update = true;

                    let response = ClickNodeResponse {
                        success: true,
                        message: format!("Clicked at ({:.1}, {:.1})", cx, cy),
                    };
                    send_ok(request, None, Some(ResponseData::ClickNode(response)));
                }
                None => {
                    let response = ClickNodeResponse {
                        success: false,
                        message: "Could not resolve click target (no matching node or position)"
                            .to_string(),
                    };
                    send_ok(request, None, Some(ResponseData::ClickNode(response)));
                }
            }
        }

        DebugEvent::DoubleClick { x, y, button } => {
            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!("Debug double click at ({}, {}) button {:?}", x, y, button),
                None,
            );

            // For double click, we set the position and rely on timing
            // In practice, we just do a click for now
            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });
            match button {
                MouseButton::Left => {
                    new_state.mouse_state.left_down = true;
                    callback_info.modify_window_state(new_state.clone());
                    new_state.mouse_state.left_down = false;
                }
                MouseButton::Right => {
                    new_state.mouse_state.right_down = true;
                    callback_info.modify_window_state(new_state.clone());
                    new_state.mouse_state.right_down = false;
                }
                MouseButton::Middle => {
                    new_state.mouse_state.middle_down = true;
                    callback_info.modify_window_state(new_state.clone());
                    new_state.mouse_state.middle_down = false;
                }
            }
            callback_info.modify_window_state(new_state);
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::Scroll {
            x,
            y,
            delta_x,
            delta_y,
        } => {
            use azul_core::dom::DomId;
            use azul_core::id::NodeId;
            use azul_core::styled_dom::NodeHierarchyItemId;

            log(
                LogLevel::Debug,
                LogCategory::EventLoop,
                format!(
                    "Debug scroll at ({}, {}) delta ({}, {})",
                    x, y, delta_x, delta_y
                ),
                None,
            );

            // Update cursor position
            let mut new_state = callback_info.get_current_window_state().clone();
            new_state.mouse_state.cursor_position =
                azul_core::window::CursorPosition::InWindow(LogicalPosition { x: *x, y: *y });
            callback_info.modify_window_state(new_state);

            // Find scrollable node that contains the point (x, y)
            // We iterate through scroll manager states and check if the point is inside
            let layout_window = callback_info.get_layout_window();
            let cursor_pos = LogicalPosition { x: *x, y: *y };
            
            let mut scroll_node: Option<(DomId, NodeId)> = None;
            for (dom_id, layout_result) in &layout_window.layout_results {
                for (scroll_id, &node_id) in &layout_result.scroll_id_to_node_id {
                    // Get node bounds from layout tree
                    if let Some(layout_indices) = layout_result.layout_tree.dom_to_layout.get(&node_id) {
                        if let Some(&layout_idx) = layout_indices.first() {
                            if let Some(layout_node) = layout_result.layout_tree.get(layout_idx) {
                                let node_pos = layout_result
                                    .calculated_positions
                                    .get(&layout_idx)
                                    .copied()
                                    .unwrap_or_default();
                                let node_size = layout_node.used_size.unwrap_or_default();
                                
                                // Check if cursor is inside this node
                                if cursor_pos.x >= node_pos.x
                                    && cursor_pos.x <= node_pos.x + node_size.width
                                    && cursor_pos.y >= node_pos.y
                                    && cursor_pos.y <= node_pos.y + node_size.height
                                {
                                    scroll_node = Some((*dom_id, node_id));
                                    break;
                                }
                            }
                        }
                    }
                }
                if scroll_node.is_some() {
                    break;
                }
            }

            if let Some((dom_id, node_id)) = scroll_node {
                let current = callback_info
                    .get_scroll_offset_for_node(dom_id, node_id)
                    .unwrap_or(LogicalPosition { x: 0.0, y: 0.0 });
                let new_pos = LogicalPosition {
                    x: current.x + *delta_x,
                    y: current.y + *delta_y,
                };
                let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                callback_info.scroll_to(dom_id, hierarchy_id, new_pos);
                log(
                    LogLevel::Debug,
                    LogCategory::EventLoop,
                    format!(
                        "Scrolled node {:?}/{:?} from ({:.1}, {:.1}) to ({:.1}, {:.1})",
                        dom_id, node_id, current.x, current.y, new_pos.x, new_pos.y
                    ),
                    None,
                );
            } else {
                log(
                    LogLevel::Debug,
                    LogCategory::EventLoop,
                    format!("No scrollable node found at ({}, {})", x, y),
                    None,
                );
            }
            needs_update = true;

            send_ok(request, None, None);
        }

        DebugEvent::Relayout => {
            log(
                LogLevel::Info,
                LogCategory::Layout,
                "Forcing relayout",
                None,
            );
            needs_update = true;
            send_ok(request, None, None);
        }

        DebugEvent::Redraw => {
            log(
                LogLevel::Info,
                LogCategory::Rendering,
                "Requesting redraw",
                None,
            );
            needs_update = true;
            send_ok(request, None, None);
        }

        DebugEvent::Close => {
            log(
                LogLevel::Info,
                LogCategory::EventLoop,
                "Close via close_window()",
                None,
            );
            callback_info.close_window();
            needs_update = true;
            send_ok(request, None, None);
        }

        DebugEvent::HitTest { x, y } => {
            let hit_test = callback_info.get_hit_test_frame(0);
            let response = HitTestResponse {
                x: *x,
                y: *y,
                node_id: None, // TODO: extract from hit_test
                node_tag: None,
            };
            send_ok(request, None, Some(ResponseData::HitTest(response)));
        }

        DebugEvent::GetLogs { .. } => {
            let logs = take_logs();
            send_ok(
                request,
                None,
                Some(ResponseData::Logs(LogsResponse { logs })),
            );
        }

        DebugEvent::WaitFrame => {
            send_ok(request, None, None);
        }

        DebugEvent::Wait { ms } => {
            std::thread::sleep(std::time::Duration::from_millis(*ms));
            send_ok(request, None, None);
        }

        DebugEvent::TakeScreenshot => {
            log(
                LogLevel::Info,
                LogCategory::Rendering,
                "Taking CPU screenshot via debug API",
                None,
            );
            // Use DomId(0) as default - first DOM in the window
            let dom_id = azul_core::dom::DomId { inner: 0 };
            match callback_info.take_screenshot_base64(dom_id) {
                Ok(data_uri) => {
                    let data = ScreenshotData {
                        data: data_uri.as_str().to_string(),
                    };
                    send_ok(request, None, Some(ResponseData::Screenshot(data)));
                }
                Err(e) => {
                    send_err(request, e.as_str().to_string());
                }
            }
        }

        DebugEvent::TakeNativeScreenshot => {
            log(
                LogLevel::Info,
                LogCategory::Rendering,
                "Taking native screenshot via debug API",
                None,
            );
            // Use the NativeScreenshotExt trait method explicitly (not the stubbed inherent method)
            match NativeScreenshotExt::take_native_screenshot_base64(callback_info) {
                Ok(data_uri) => {
                    let data = ScreenshotData {
                        data: data_uri.as_str().to_string(),
                    };
                    send_ok(request, None, Some(ResponseData::Screenshot(data)));
                }
                Err(e) => {
                    send_err(request, e.as_str().to_string());
                }
            }
        }

        DebugEvent::GetHtmlString => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting HTML string",
                None,
            );
            let dom_id = azul_core::dom::DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();
            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let html = layout_result.styled_dom.get_html_string("", "", true);
                send_ok(
                    request,
                    None,
                    Some(ResponseData::HtmlString(HtmlStringResponse { html })),
                );
            } else {
                send_err(request, "No layout result for DOM 0");
            }
        }

        DebugEvent::GetNodeCssProperties {
            node_id,
            selector,
            text,
        } => {
            use azul_core::dom::{DomId, DomNodeId, NodeId};
            use azul_css::props::property::CssPropertyType;
            use strum::IntoEnumIterator;

            let resolved_node_id = resolve_node_target(
                callback_info,
                selector.as_deref(),
                *node_id,
                text.as_deref(),
            );

            let nid = match resolved_node_id {
                Some(n) => n.index() as u64,
                None => {
                    send_err(request, "No node found matching the specified target");
                    return needs_update;
                }
            };

            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                format!("Getting CSS properties for node {}", nid),
                None,
            );

            let dom_node_id = DomNodeId {
                dom: DomId { inner: 0 },
                node: Some(NodeId::new(nid as usize)).into(),
            };

            // Collect all CSS properties that are set on this node
            let mut props = Vec::new();

            // Iterate over all CSS property types
            for prop_type in CssPropertyType::iter() {
                if let Some(prop) = callback_info.get_computed_css_property(dom_node_id, prop_type)
                {
                    props.push(format!("{}: {:?}", prop_type.to_str(), prop));
                }
            }

            let response = NodeCssPropertiesResponse {
                node_id: nid,
                property_count: props.len(),
                properties: props,
            };
            send_ok(
                request,
                None,
                Some(ResponseData::NodeCssProperties(response)),
            );
        }

        DebugEvent::GetNodeLayout {
            node_id,
            selector,
            text,
        } => {
            use azul_core::dom::{DomId, DomNodeId, NodeId};

            let resolved_node_id = resolve_node_target(
                callback_info,
                selector.as_deref(),
                *node_id,
                text.as_deref(),
            );

            let nid = match resolved_node_id {
                Some(n) => n.index() as u64,
                None => {
                    send_err(request, "No node found matching the specified target");
                    return needs_update;
                }
            };

            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                format!("Getting layout for node {}", nid),
                None,
            );

            let dom_node_id = DomNodeId {
                dom: DomId { inner: 0 },
                node: Some(NodeId::new(nid as usize)).into(),
            };

            let size = callback_info.get_node_size(dom_node_id);
            let pos = callback_info.get_node_position(dom_node_id);
            let rect = callback_info.get_node_rect(dom_node_id);

            let response = NodeLayoutResponse {
                node_id: nid,
                size: size.map(|s| LogicalSizeJson {
                    width: s.width,
                    height: s.height,
                }),
                position: pos.map(|p| LogicalPositionJson { x: p.x, y: p.y }),
                rect: rect.map(|r| LogicalRectJson {
                    x: r.origin.x,
                    y: r.origin.y,
                    width: r.size.width,
                    height: r.size.height,
                }),
            };
            send_ok(request, None, Some(ResponseData::NodeLayout(response)));
        }

        DebugEvent::GetAllNodesLayout => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting all nodes layout",
                None,
            );
            use azul_core::dom::{DomId, DomNodeId, NodeId};

            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();

            let mut nodes = Vec::new();
            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let node_count = layout_result.styled_dom.node_data.len();
                for i in 0..node_count {
                    let dom_node_id = DomNodeId {
                        dom: dom_id.clone(),
                        node: Some(NodeId::new(i)).into(),
                    };

                    let rect = callback_info.get_node_rect(dom_node_id);
                    let tag = callback_info.get_node_tag_name(dom_node_id);
                    let id_attr = callback_info.get_node_id(dom_node_id);
                    let classes = callback_info.get_node_classes(dom_node_id);

                    nodes.push(NodeLayoutInfo {
                        node_id: i,
                        tag: tag.map(|s| s.as_str().to_string()),
                        id: id_attr.map(|s| s.as_str().to_string()),
                        classes: classes
                            .as_ref()
                            .iter()
                            .map(|s| s.as_str().to_string())
                            .collect(),
                        rect: rect.map(|r| LogicalRectJson {
                            x: r.origin.x,
                            y: r.origin.y,
                            width: r.size.width,
                            height: r.size.height,
                        }),
                    });
                }
            }

            let response = AllNodesLayoutResponse {
                dom_id: 0,
                node_count: nodes.len(),
                nodes,
            };
            send_ok(request, None, Some(ResponseData::AllNodesLayout(response)));
        }

        DebugEvent::GetDomTree => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting DOM tree",
                None,
            );
            use azul_core::dom::DomId;

            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();

            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let styled_dom = &layout_result.styled_dom;
                let window_state = callback_info.get_current_window_state();

                let node_count = styled_dom.node_data.len();
                let dpi = window_state.size.dpi;
                let hidpi = window_state.size.get_hidpi_factor().inner.get();
                let logical_size = &window_state.size.dimensions;

                let response = DomTreeResponse {
                    dom_id: 0,
                    node_count,
                    dpi,
                    hidpi_factor: hidpi,
                    logical_width: logical_size.width,
                    logical_height: logical_size.height,
                };
                send_ok(request, None, Some(ResponseData::DomTree(response)));
            } else {
                send_err(request, "No layout result for DOM 0");
            }
        }

        DebugEvent::GetNodeHierarchy => {
            log(
                LogLevel::Debug,
                LogCategory::DebugServer,
                "Getting node hierarchy",
                None,
            );
            use azul_core::dom::DomId;
            use azul_core::id::NodeId;

            let dom_id = DomId { inner: 0 };
            let layout_window = callback_info.get_layout_window();

            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                let styled_dom = &layout_result.styled_dom;
                let hierarchy = styled_dom.node_hierarchy.as_container();
                let node_data = styled_dom.node_data.as_container();

                let root_decoded = styled_dom
                    .root
                    .into_crate_internal()
                    .map(|n| n.index() as i64)
                    .unwrap_or(-1);

                let mut nodes = Vec::new();
                for i in 0..hierarchy.len() {
                    let node_id = NodeId::new(i);
                    let hier = &hierarchy[node_id];
                    let data = &node_data[node_id];

                    let node_type = data.get_node_type().get_path().to_string();

                    let text_content = match data.get_node_type() {
                        azul_core::dom::NodeType::Text(t) => {
                            let s = t.as_str();
                            if s.len() > 50 {
                                Some(format!("{}...", &s[..47]))
                            } else {
                                Some(s.to_string())
                            }
                        }
                        _ => None,
                    };

                    let parent_decoded = if hier.parent == 0 {
                        -1i64
                    } else {
                        (hier.parent - 1) as i64
                    };
                    let prev_sib_decoded = if hier.previous_sibling == 0 {
                        -1i64
                    } else {
                        (hier.previous_sibling - 1) as i64
                    };
                    let next_sib_decoded = if hier.next_sibling == 0 {
                        -1i64
                    } else {
                        (hier.next_sibling - 1) as i64
                    };
                    let last_child_decoded = if hier.last_child == 0 {
                        -1i64
                    } else {
                        (hier.last_child - 1) as i64
     

... [FILE TRUNCATED - original size: 188711 bytes] ...
```

### layout/src/callbacks.rs

```rust
//! Callback handling for layout events
//!
//! This module provides the CallbackInfo struct and related types for handling
//! UI callbacks. Callbacks need access to layout information (node sizes, positions,
//! hierarchy), which is why this module lives in azul-layout instead of azul-core.

// Re-export callback macro from azul-core
use alloc::{
    boxed::Box,
    collections::{btree_map::BTreeMap, VecDeque},
    sync::Arc,
    vec::Vec,
};

#[cfg(feature = "std")]
use std::sync::Mutex;

use azul_core::{
    animation::UpdateImageType,
    callbacks::{CoreCallback, FocusTarget, FocusTargetPath, HidpiAdjustedBounds, Update},
    dom::{DomId, DomIdVec, DomNodeId, IdOrClass, NodeId, NodeType},
    events::CallbackResultRef,
    geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition},
    gl::OptionGlContextPtr,
    gpu::GpuValueCache,
    hit_test::ScrollPosition,
    id::NodeId as CoreNodeId,
    impl_callback,
    menu::Menu,
    refany::{OptionRefAny, RefAny},
    resources::{ImageCache, ImageMask, ImageRef, RendererResources},
    selection::{Selection, SelectionRange, SelectionRangeVec, SelectionState, TextCursor},
    styled_dom::{NodeHierarchyItemId, NodeIdVec, StyledDom},
    task::{self, GetSystemTimeCallback, Instant, ThreadId, ThreadIdVec, TimerId, TimerIdVec},
    window::{KeyboardState, MouseState, RawWindowHandle, WindowFlags, WindowSize},
    FastBTreeSet, FastHashMap,
};
use azul_css::{
    css::CssPath,
    props::{
        basic::FontRef,
        property::{CssProperty, CssPropertyType, CssPropertyVec},
    },
    system::SystemStyle,
    AzString, StringVec,
};
use rust_fontconfig::FcFontCache;

#[cfg(feature = "icu")]
use crate::icu::{
    FormatLength, IcuDate, IcuDateTime, IcuLocalizerHandle, IcuResult,
    IcuStringVec, IcuTime, ListType, PluralCategory,
};

use crate::{
    hit_test::FullHitTest,
    managers::{
        drag_drop::DragDropManager,
        file_drop::FileDropManager,
        focus_cursor::FocusManager,
        gesture::{GestureAndDragManager, InputSample, PenState},
        gpu_state::GpuStateManager,
        hover::{HoverManager, InputPointId},
        iframe::IFrameManager,
        scroll_state::{AnimatedScrollState, ScrollManager},
        selection::{ClipboardContent, SelectionManager},
        text_input::{PendingTextEdit, TextInputManager},
        undo_redo::{UndoRedoManager, UndoableOperation},
    },
    text3::cache::{LayoutCache as TextLayoutCache, UnifiedLayout},
    thread::{CreateThreadCallback, Thread},
    timer::Timer,
    window::{DomLayoutResult, LayoutWindow},
    window_state::{FullWindowState, WindowCreateOptions},
};

use azul_css::{impl_option, impl_option_inner};

// ============================================================================
// FFI-safe wrapper types for tuple returns
// ============================================================================

/// FFI-safe wrapper for pen tilt angles (x_tilt, y_tilt) in degrees
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
#[repr(C)]
pub struct PenTilt {
    /// X-axis tilt angle in degrees (-90 to 90)
    pub x_tilt: f32,
    /// Y-axis tilt angle in degrees (-90 to 90)
    pub y_tilt: f32,
}

impl From<(f32, f32)> for PenTilt {
    fn from((x, y): (f32, f32)) -> Self {
        Self {
            x_tilt: x,
            y_tilt: y,
        }
    }
}

impl_option!(
    PenTilt,
    OptionPenTilt,
    [Debug, Clone, Copy, PartialEq, PartialOrd]
);

/// FFI-safe wrapper for select-all result (full_text, selected_range)
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct SelectAllResult {
    /// The full text content of the node
    pub full_text: AzString,
    /// The range that would be selected
    pub selection_range: SelectionRange,
}

impl From<(alloc::string::String, SelectionRange)> for SelectAllResult {
    fn from((text, range): (alloc::string::String, SelectionRange)) -> Self {
        Self {
            full_text: text.into(),
            selection_range: range,
        }
    }
}

impl_option!(
    SelectAllResult,
    OptionSelectAllResult,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// FFI-safe wrapper for delete inspection result (range_to_delete, deleted_text)
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct DeleteResult {
    /// The range that would be deleted
    pub range_to_delete: SelectionRange,
    /// The text that would be deleted
    pub deleted_text: AzString,
}

impl From<(SelectionRange, alloc::string::String)> for DeleteResult {
    fn from((range, text): (SelectionRange, alloc::string::String)) -> Self {
        Self {
            range_to_delete: range,
            deleted_text: text.into(),
        }
    }
}

impl_option!(
    DeleteResult,
    OptionDeleteResult,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// Represents a change made by a callback that will be applied after the callback returns
///
/// This transaction-based system provides:
/// - Clear separation between read-only queries and modifications
/// - Atomic application of all changes
/// - Easy debugging and logging of callback actions
/// - Future extensibility for new change types
#[derive(Debug, Clone)]
pub enum CallbackChange {
    // Window State Changes
    /// Modify the window state (size, position, title, etc.)
    ModifyWindowState { state: FullWindowState },
    /// Queue multiple window state changes to be applied in sequence across frames.
    /// This is needed for simulating clicks (mouse down → wait → mouse up) where each
    /// state change needs to trigger separate event processing.
    QueueWindowStateSequence { states: Vec<FullWindowState> },
    /// Create a new window
    CreateNewWindow { options: WindowCreateOptions },
    /// Close the current window (via Update::CloseWindow return value, tracked here for logging)
    CloseWindow,

    // Focus Management
    /// Change keyboard focus to a specific node or clear focus
    SetFocusTarget { target: FocusTarget },

    // Event Propagation Control
    /// Stop event from propagating to parent nodes
    StopPropagation,
    /// Prevent default browser behavior (e.g., block text input from being applied)
    PreventDefault,

    // Timer Management
    /// Add a new timer to the window
    AddTimer { timer_id: TimerId, timer: Timer },
    /// Remove an existing timer
    RemoveTimer { timer_id: TimerId },

    // Thread Management
    /// Add a new background thread
    AddThread { thread_id: ThreadId, thread: Thread },
    /// Remove an existing thread
    RemoveThread { thread_id: ThreadId },

    // Content Modifications
    /// Change the text content of a node
    ChangeNodeText { node_id: DomNodeId, text: AzString },
    /// Change the image of a node
    ChangeNodeImage {
        dom_id: DomId,
        node_id: NodeId,
        image: ImageRef,
        update_type: UpdateImageType,
    },
    /// Re-render an image callback (for resize/animation)
    /// This triggers re-invocation of the RenderImageCallback
    UpdateImageCallback { dom_id: DomId, node_id: NodeId },
    /// Trigger re-rendering of an IFrame with a new DOM
    /// This forces the IFrame to call its callback and update the display list
    UpdateIFrame { dom_id: DomId, node_id: NodeId },
    /// Change the image mask of a node
    ChangeNodeImageMask {
        dom_id: DomId,
        node_id: NodeId,
        mask: ImageMask,
    },
    /// Change CSS properties of a node
    ChangeNodeCssProperties {
        dom_id: DomId,
        node_id: NodeId,
        properties: CssPropertyVec,
    },

    // Scroll Management
    /// Scroll a node to a specific position
    ScrollTo {
        dom_id: DomId,
        node_id: NodeHierarchyItemId,
        position: LogicalPosition,
    },
    /// Scroll a node into view (W3C scrollIntoView API)
    /// The scroll adjustments are calculated and applied when the change is processed
    ScrollIntoView {
        node_id: DomNodeId,
        options: crate::managers::scroll_into_view::ScrollIntoViewOptions,
    },

    // Image Cache Management
    /// Add an image to the image cache
    AddImageToCache { id: AzString, image: ImageRef },
    /// Remove an image from the image cache
    RemoveImageFromCache { id: AzString },

    // Font Cache Management
    /// Reload system fonts (expensive operation)
    ReloadSystemFonts,

    // Menu Management
    /// Open a context menu or dropdown menu
    /// Whether it's native or fallback depends on window.state.flags.use_native_context_menus
    OpenMenu {
        menu: Menu,
        /// Optional position override (if None, uses menu.position)
        position: Option<LogicalPosition>,
    },

    // Tooltip Management
    /// Show a tooltip at a specific position
    ///
    /// Platform-specific implementation:
    /// - Windows: Uses native tooltip window (TOOLTIPS_CLASS)
    /// - macOS: Uses NSPopover or custom NSWindow with tooltip styling
    /// - X11: Creates transient window with _NET_WM_WINDOW_TYPE_TOOLTIP
    /// - Wayland: Creates surface with zwlr_layer_shell_v1 (overlay layer)
    ShowTooltip {
        text: AzString,
        position: LogicalPosition,
    },
    /// Hide the currently displayed tooltip
    HideTooltip,

    // Text Editing
    /// Insert text at the current cursor position or replace selection
    InsertText {
        dom_id: DomId,
        node_id: NodeId,
        text: AzString,
    },
    /// Delete text backward (backspace) at cursor
    DeleteBackward { dom_id: DomId, node_id: NodeId },
    /// Delete text forward (delete key) at cursor
    DeleteForward { dom_id: DomId, node_id: NodeId },
    /// Move cursor to a specific position
    MoveCursor {
        dom_id: DomId,
        node_id: NodeId,
        cursor: TextCursor,
    },
    /// Set text selection range
    SetSelection {
        dom_id: DomId,
        node_id: NodeId,
        selection: Selection,
    },
    /// Set/override the text changeset for the current text input operation
    /// This allows callbacks to modify what text will be inserted during text input events
    SetTextChangeset { changeset: PendingTextEdit },

    // Cursor Movement Operations
    /// Move cursor left (arrow left)
    MoveCursorLeft {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor right (arrow right)
    MoveCursorRight {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor up (arrow up)
    MoveCursorUp {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor down (arrow down)
    MoveCursorDown {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor to line start (Home key)
    MoveCursorToLineStart {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor to line end (End key)
    MoveCursorToLineEnd {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor to document start (Ctrl+Home)
    MoveCursorToDocumentStart {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },
    /// Move cursor to document end (Ctrl+End)
    MoveCursorToDocumentEnd {
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
    },

    // Clipboard Operations (Override)
    /// Override clipboard content for copy operation
    SetCopyContent {
        target: DomNodeId,
        content: ClipboardContent,
    },
    /// Override clipboard content for cut operation
    SetCutContent {
        target: DomNodeId,
        content: ClipboardContent,
    },
    /// Override selection range for select-all operation
    SetSelectAllRange {
        target: DomNodeId,
        range: SelectionRange,
    },

    // Hit Test Request (for Debug API)
    /// Request a hit test update at a specific position
    ///
    /// This is used by the Debug API to update the hover manager's hit test
    /// data after modifying the mouse position, ensuring that callbacks
    /// can find the correct nodes under the cursor.
    RequestHitTestUpdate { position: LogicalPosition },

    // Text Selection (for Debug API)
    /// Process a text selection click at a specific position
    ///
    /// This is used by the Debug API to trigger text selection directly,
    /// bypassing the normal event pipeline. The handler will:
    /// 1. Hit-test IFC roots to find selectable text at the position
    /// 2. Create a text cursor at the clicked position
    /// 3. Update the selection manager with the new selection
    ProcessTextSelectionClick {
        position: LogicalPosition,
        time_ms: u64,
    },

    // Cursor Blinking (System Timer Control)
    /// Set the cursor visibility state (called by blink timer)
    SetCursorVisibility { visible: bool },
    /// Reset cursor blink state on user input (makes cursor visible, records time)
    ResetCursorBlink,
    /// Start the cursor blink timer for the focused contenteditable element
    StartCursorBlinkTimer,
    /// Stop the cursor blink timer (when focus leaves contenteditable)
    StopCursorBlinkTimer,
    
    // Scroll cursor/selection into view
    /// Scroll the active text cursor into view within its scrollable container
    /// This is automatically triggered after text input or cursor movement
    ScrollActiveCursorIntoView,
    
    // Create Text Input Event (for Debug API / Programmatic Text Input)
    /// Create a synthetic text input event
    ///
    /// This simulates receiving text input from the OS. The text input flow will:
    /// 1. Record the text in TextInputManager (creating a PendingTextEdit)
    /// 2. Generate synthetic TextInput events
    /// 3. Invoke user callbacks (which can intercept/reject via preventDefault)
    /// 4. Apply the changeset if not rejected
    /// 5. Mark dirty nodes for re-render
    CreateTextInput {
        /// The text to insert
        text: AzString,
    },
}

/// Main callback type for UI event handling
pub type CallbackType = extern "C" fn(RefAny, CallbackInfo) -> Update;

/// Stores a function pointer that is executed when the given UI element is hit
///
/// Must return an `Update` that denotes if the screen should be redrawn.
#[repr(C)]
pub struct Callback {
    pub cb: CallbackType,
    /// For FFI: stores the foreign callable (e.g., PyFunction)
    /// Native Rust code sets this to None
    pub ctx: OptionRefAny,
}

impl_callback!(Callback, CallbackType);

impl Callback {
    /// Create a new callback with just a function pointer (for native Rust code)
    pub fn create<C: Into<Callback>>(cb: C) -> Self {
        cb.into()
    }

    /// Convert from CoreCallback (stored as usize) to Callback (actual function pointer)
    ///
    /// # Safety
    /// The caller must ensure that the usize in CoreCallback.cb was originally a valid
    /// function pointer of type `CallbackType`. This is guaranteed when CoreCallback
    /// is created through standard APIs, but unsafe code could violate this.
    pub fn from_core(core: CoreCallback) -> Self {
        Self {
            cb: unsafe { core::mem::transmute(core.cb) },
            ctx: OptionRefAny::None,
        }
    }

    /// Convert to CoreCallback (function pointer stored as usize)
    ///
    /// This is always safe - we're just casting the function pointer to usize for storage.
    pub fn to_core(self) -> CoreCallback {
        CoreCallback {
            cb: self.cb as usize,
            ctx: self.ctx,
        }
    }
}

/// Allow Callback to be passed to functions expecting `C: Into<CoreCallback>`
impl From<Callback> for CoreCallback {
    fn from(callback: Callback) -> Self {
        callback.to_core()
    }
}

/// Convert a raw function pointer to CoreCallback
///
/// This is a helper function that wraps the function pointer cast.
/// Cannot use From trait due to orphan rules (extern "C" fn is not a local type).
#[inline]
pub fn callback_type_to_core(cb: CallbackType) -> CoreCallback {
    CoreCallback {
        cb: cb as usize,
        ctx: OptionRefAny::None,
    }
}

impl Callback {
    /// Safely invoke the callback with the given data and info
    ///
    /// This is a safe wrapper around calling the function pointer directly.
    pub fn invoke(&self, data: RefAny, info: CallbackInfo) -> Update {
        (self.cb)(data, info)
    }
}

/// Safe conversion from CoreCallback to function pointer
///
/// This provides a type-safe way to convert CoreCallback.cb (usize) to the actual
/// function pointer type without using transmute directly in application code.
///
/// # Safety
/// The caller must ensure the usize was originally a valid CallbackType function pointer.
pub unsafe fn core_callback_to_fn(core: CoreCallback) -> CallbackType {
    core::mem::transmute(core.cb)
}

/// FFI-safe Option<Callback> type for C interop.
///
/// This enum provides an ABI-stable alternative to `Option<Callback>`
/// that can be safely passed across FFI boundaries.
#[derive(Debug, Eq, Clone, PartialEq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum OptionCallback {
    /// No callback is present.
    None,
    /// A callback is present.
    Some(Callback),
}

impl OptionCallback {
    /// Converts this FFI-safe option into a standard Rust `Option<Callback>`.
    pub fn into_option(self) -> Option<Callback> {
        match self {
            OptionCallback::None => None,
            OptionCallback::Some(c) => Some(c),
        }
    }

    /// Returns `true` if a callback is present.
    pub fn is_some(&self) -> bool {
        matches!(self, OptionCallback::Some(_))
    }

    /// Returns `true` if no callback is present.
    pub fn is_none(&self) -> bool {
        matches!(self, OptionCallback::None)
    }
}

impl From<Option<Callback>> for OptionCallback {
    fn from(o: Option<Callback>) -> Self {
        match o {
            None => OptionCallback::None,
            Some(c) => OptionCallback::Some(c),
        }
    }
}

impl From<OptionCallback> for Option<Callback> {
    fn from(o: OptionCallback) -> Self {
        o.into_option()
    }
}

/// Information about the callback that is passed to the callback whenever a callback is invoked
///
/// # Architecture
///
/// CallbackInfo uses a transaction-based system:
/// - **Read-only pointers**: Access to layout data, window state, managers for queries
/// - **Change vector**: All modifications are recorded as CallbackChange items
/// - **Processing**: Changes are applied atomically after callback returns
///
/// This design provides clear separation between queries and modifications, makes debugging
/// easier, and allows for future extensibility.

/// Reference data container for CallbackInfo (all read-only fields)
///
/// This struct consolidates all readonly references that callbacks need to query window state.
/// By grouping these into a single struct, we reduce the number of parameters to
/// CallbackInfo::new() from 13 to 3, making the API more maintainable and easier to extend.
///
/// This is pure syntax sugar - the struct lives on the stack in the caller and is passed by
/// reference.
pub struct CallbackInfoRefData<'a> {
    /// Pointer to the LayoutWindow containing all layout results (READ-ONLY for queries)
    pub layout_window: &'a LayoutWindow,
    /// Necessary to query FontRefs from callbacks
    pub renderer_resources: &'a RendererResources,
    /// Previous window state (for detecting changes)
    pub previous_window_state: &'a Option<FullWindowState>,
    /// State of the current window that the callback was called on (read only!)
    pub current_window_state: &'a FullWindowState,
    /// An Rc to the OpenGL context, in order to be able to render to OpenGL textures
    pub gl_context: &'a OptionGlContextPtr,
    /// Immutable reference to where the nodes are currently scrolled (current position)
    pub current_scroll_manager: &'a BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>>,
    /// Handle of the current window
    pub current_window_handle: &'a RawWindowHandle,
    /// Callbacks for creating threads and getting the system time (since this crate uses no_std)
    pub system_callbacks: &'a ExternalSystemCallbacks,
    /// Platform-specific system style (colors, spacing, etc.)
    /// Arc allows safe cloning in callbacks without unsafe pointer manipulation
    pub system_style: Arc<SystemStyle>,
    /// ICU4X localizer cache for internationalized formatting (numbers, dates, lists, plurals)
    /// Caches localizers for multiple locales. Only available when the "icu" feature is enabled.
    #[cfg(feature = "icu")]
    pub icu_localizer: IcuLocalizerHandle,
    /// The callable for FFI language bindings (Python, etc.)
    /// Cloned from the Callback struct before invocation. Native Rust callbacks have this as None.
    pub ctx: OptionRefAny,
}

/// CallbackInfo is a lightweight wrapper around pointers to stack-local data.
/// It can be safely copied because it only contains pointers - the underlying
/// data lives on the stack and outlives the callback invocation.
/// This allows callbacks to "consume" CallbackInfo by value while the caller
/// retains access to the same underlying data.
///
/// The `changes` field uses a pointer to Arc<Mutex<...>> so that cloned CallbackInfo instances
/// (e.g., passed to timer callbacks) still push changes to the original collection,
/// while keeping CallbackInfo as Copy.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CallbackInfo {
    // Read-only Data (Query Access)
    /// Single reference to all readonly reference data
    /// This consolidates 8 individual parameters into 1, improving API ergonomics
    ref_data: *const CallbackInfoRefData<'static>,
    // Context Info (Immutable Event Data)
    /// The ID of the DOM + the node that was hit
    hit_dom_node: DomNodeId,
    /// The (x, y) position of the mouse cursor, **relative to top left of the element that was
    /// hit**
    cursor_relative_to_item: OptionLogicalPosition,
    /// The (x, y) position of the mouse cursor, **relative to top left of the window**
    cursor_in_viewport: OptionLogicalPosition,
    // Transaction Container (New System) - Uses pointer to Arc<Mutex> for shared access across clones
    /// All changes made by the callback, applied atomically after callback returns
    /// Stored as raw pointer so CallbackInfo remains Copy
    #[cfg(feature = "std")]
    changes: *const Arc<Mutex<Vec<CallbackChange>>>,
    #[cfg(not(feature = "std"))]
    changes: *mut Vec<CallbackChange>,
}

impl CallbackInfo {
    #[cfg(feature = "std")]
    pub fn new<'a>(
        ref_data: &'a CallbackInfoRefData<'a>,
        changes: &'a Arc<Mutex<Vec<CallbackChange>>>,
        hit_dom_node: DomNodeId,
        cursor_relative_to_item: OptionLogicalPosition,
        cursor_in_viewport: OptionLogicalPosition,
    ) -> Self {
        Self {
            // Read-only data (single reference to consolidated refs)
            // SAFETY: We cast away the lifetime 'a to 'static because CallbackInfo
            // only lives for the duration of the callback, which is shorter than 'a
            ref_data: unsafe { core::mem::transmute(ref_data) },

            // Context info (immutable event data)
            hit_dom_node,
            cursor_relative_to_item,
            cursor_in_viewport,

            // Transaction container - store pointer to Arc<Mutex> for shared access
            changes: changes as *const Arc<Mutex<Vec<CallbackChange>>>,
        }
    }

    #[cfg(not(feature = "std"))]
    pub fn new<'a>(
        ref_data: &'a CallbackInfoRefData<'a>,
        changes: &'a mut Vec<CallbackChange>,
        hit_dom_node: DomNodeId,
        cursor_relative_to_item: OptionLogicalPosition,
        cursor_in_viewport: OptionLogicalPosition,
    ) -> Self {
        Self {
            ref_data: unsafe { core::mem::transmute(ref_data) },
            hit_dom_node,
            cursor_relative_to_item,
            cursor_in_viewport,
            changes: changes as *mut Vec<CallbackChange>,
        }
    }

    /// Get the callable for FFI language bindings (Python, etc.)
    ///
    /// Returns the cloned OptionRefAny if a callable was set, or None if this
    /// is a native Rust callback.
    pub fn get_ctx(&self) -> OptionRefAny {
        unsafe { (*self.ref_data).ctx.clone() }
    }

    /// Returns the OpenGL context if available
    pub fn get_gl_context(&self) -> OptionGlContextPtr {
        unsafe { (*self.ref_data).gl_context.clone() }
    }

    // Helper methods for transaction system

    /// Push a change to be applied after the callback returns
    /// This is the primary method for modifying window state from callbacks
    #[cfg(feature = "std")]
    pub fn push_change(&mut self, change: CallbackChange) {
        // SAFETY: The pointer is valid for the lifetime of the callback
        unsafe {
            if let Ok(mut changes) = (*self.changes).lock() {
                changes.push(change);
            }
        }
    }

    #[cfg(not(feature = "std"))]
    pub fn push_change(&mut self, change: CallbackChange) {
        unsafe { (*self.changes).push(change) }
    }

    /// Debug helper to get the changes pointer for debugging
    #[cfg(feature = "std")]
    pub fn get_changes_ptr(&self) -> *const () {
        self.changes as *const ()
    }

    /// Get the collected changes (consumes them from the Arc<Mutex>)
    #[cfg(feature = "std")]
    pub fn take_changes(&self) -> Vec<CallbackChange> {
        // SAFETY: The pointer is valid for the lifetime of the callback
        unsafe {
            if let Ok(mut changes) = (*self.changes).lock() {
                core::mem::take(&mut *changes)
            } else {
                Vec::new()
            }
        }
    }

    #[cfg(not(feature = "std"))]
    pub fn take_changes(&self) -> Vec<CallbackChange> {
        unsafe { core::mem::take(&mut *self.changes) }
    }

    // Modern Api (using CallbackChange transactions)

    /// Add a timer to this window (applied after callback returns)
    pub fn add_timer(&mut self, timer_id: TimerId, timer: Timer) {
        self.push_change(CallbackChange::AddTimer { timer_id, timer });
    }

    /// Remove a timer from this window (applied after callback returns)
    pub fn remove_timer(&mut self, timer_id: TimerId) {
        self.push_change(CallbackChange::RemoveTimer { timer_id });
    }

    /// Add a thread to this window (applied after callback returns)
    pub fn add_thread(&mut self, thread_id: ThreadId, thread: Thread) {
        self.push_change(CallbackChange::AddThread { thread_id, thread });
    }

    /// Remove a thread from this window (applied after callback returns)
    pub fn remove_thread(&mut self, thread_id: ThreadId) {
        self.push_change(CallbackChange::RemoveThread { thread_id });
    }

    /// Stop event propagation (applied after callback returns)
    pub fn stop_propagation(&mut self) {
        self.push_change(CallbackChange::StopPropagation);
    }

    /// Set keyboard focus target (applied after callback returns)
    pub fn set_focus(&mut self, target: FocusTarget) {
        self.push_change(CallbackChange::SetFocusTarget { target });
    }

    /// Create a new window (applied after callback returns)
    pub fn create_window(&mut self, options: WindowCreateOptions) {
        self.push_change(CallbackChange::CreateNewWindow { options });
    }

    /// Close the current window (applied after callback returns)
    pub fn close_window(&mut self) {
        self.push_change(CallbackChange::CloseWindow);
    }

    /// Modify the window state (applied after callback returns)
    pub fn modify_window_state(&mut self, state: FullWindowState) {
        self.push_change(CallbackChange::ModifyWindowState { state });
    }

    /// Queue multiple window state changes to be applied in sequence.
    /// Each state triggers a separate event processing cycle, which is needed
    /// for simulating clicks where mouse down and mouse up must be separate events.
    pub fn queue_window_state_sequence(&mut self, states: Vec<FullWindowState>) {
        self.push_change(CallbackChange::QueueWindowStateSequence { states });
    }

    /// Change the text content of a node (applied after callback returns)
    ///
    /// This method was previously called `set_string_contents` in older API versions.
    ///
    /// # Arguments
    /// * `node_id` - The text node to modify (DomNodeId containing both DOM and node IDs)
    /// * `text` - The new text content
    pub fn change_node_text(&mut self, node_id: DomNodeId, text: AzString) {
        self.push_change(CallbackChange::ChangeNodeText { node_id, text });
    }

    /// Change the image of a node (applied after callback returns)
    pub fn change_node_image(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        image: ImageRef,
        update_type: UpdateImageType,
    ) {
        self.push_change(CallbackChange::ChangeNodeImage {
            dom_id,
            node_id,
            image,
            update_type,
        });
    }

    /// Re-render an image callback (for resize/animation updates)
    ///
    /// This triggers re-invocation of the RenderImageCallback associated with the node.
    /// Useful for:
    /// - Responding to window resize (image needs to match new size)
    /// - Animation frames (update OpenGL texture each frame)
    /// - Interactive content (user input changes rendering)
    pub fn update_image_callback(&mut self, dom_id: DomId, node_id: NodeId) {
        self.push_change(CallbackChange::UpdateImageCallback { dom_id, node_id });
    }

    /// Trigger re-rendering of an IFrame (applied after callback returns)
    ///
    /// This forces the IFrame to call its layout callback with reason `DomRecreated`
    /// and submit a new display list to WebRender. The IFrame's pipeline will be updated
    /// without affecting other parts of the window.
    ///
    /// Useful for:
    /// - Live preview panes (update when source code changes)
    /// - Dynamic content that needs manual refresh
    /// - Editor previews (re-parse and display new DOM)
    pub fn trigger_iframe_rerender(&mut self, dom_id: DomId, node_id: NodeId) {
        self.push_change(CallbackChange::UpdateIFrame { dom_id, node_id });
    }

    // Dom Tree Navigation

    /// Find a node by ID attribute in the layout tree
    ///
    /// Returns the NodeId of the first node with the given ID attribute, or None if not found.
    pub fn get_node_id_by_id_attribute(&self, dom_id: DomId, id: &str) -> Option<NodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.layout_results.get(&dom_id)?;
        let styled_dom = &layout_result.styled_dom;

        // Search through all nodes to find one with matching ID attribute
        for (node_idx, node_data) in styled_dom.node_data.as_ref().iter().enumerate() {
            for id_or_class in node_data.ids_and_classes.as_ref() {
                if let IdOrClass::Id(node_id_str) = id_or_class {
                    if node_id_str.as_str() == id {
                        return Some(NodeId::new(node_idx));
                    }
                }
            }
        }

        None
    }

    /// Get the parent node of the given node
    ///
    /// Returns None if the node has no parent (i.e., it's the root node)
    pub fn get_parent_node(&self, dom_id: DomId, node_id: NodeId) -> Option<NodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.layout_results.get(&dom_id)?;
        let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
        let node = node_hierarchy.as_ref().get(node_id.index())?;
        node.parent_id()
    }

    /// Get the next sibling of the given node
    ///
    /// Returns None if the node has no next sibling
    pub fn get_next_sibling_node(&self, dom_id: DomId, node_id: NodeId) -> Option<NodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.layout_results.get(&dom_id)?;
        let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
        let node = node_hierarchy.as_ref().get(node_id.index())?;
        node.next_sibling_id()
    }

    /// Get the previous sibling of the given node
    ///
    /// Returns None if the node has no previous sibling
    pub fn get_previous_sibling_node(&self, dom_id: DomId, node_id: NodeId) -> Option<NodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.layout_results.get(&dom_id)?;
        let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
        let node = node_hierarchy.as_ref().get(node_id.index())?;
        node.previous_sibling_id()
    }

    /// Get the first child of the given node
    ///
    /// Returns None if the node has no children
    pub fn get_first_child_node(&self, dom_id: DomId, node_id: NodeId) -> Option<NodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.layout_results.get(&dom_id)?;
        let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
        let node = node_hierarchy.as_ref().get(node_id.index())?;
        node.first_child_id(node_id)
    }

    /// Get the last child of the given node
    ///
    /// Returns None if the node has no children
    pub fn get_last_child_node(&self, dom_id: DomId, node_id: NodeId) -> Option<NodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.layout_results.get(&dom_id)?;
        let node_hierarchy = &layout_result.styled_dom.node_hierarchy;
        let node = node_hierarchy.as_ref().get(node_id.index())?;
        node.last_child_id()
    }

    /// Get all direct children of the given node
    ///
    /// Returns an empty vector if the node has no children.
    /// Uses the contiguous node layout for efficient iteration.
    pub fn get_all_children_nodes(&self, dom_id: DomId, node_id: NodeId) -> NodeIdVec {
        let layout_window = self.get_layout_window();
        let layout_result = match layout_window.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return NodeIdVec::from_const_slice(&[]),
        };
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = match node_hierarchy.get(node_id) {
            Some(h) => h,
            None => return NodeIdVec::from_const_slice(&[]),
        };

        // Get first child - if none, return empty
        let first_child = match hier_item.first_child_id(node_id) {
            Some(fc) => fc,
            None => return NodeIdVec::from_const_slice(&[]),
        };

        // Collect children by walking the sibling chain
        let mut children: Vec<NodeHierarchyItemId> = Vec::new();
        children.push(NodeHierarchyItemId::from_crate_internal(Some(first_child)));

        let mut current = first_child;
        while let Some(next_sibling) = node_hierarchy
            .get(current)
            .and_then(|h| h.next_sibling_id())
        {
            children.push(NodeHierarchyItemId::from_crate_internal(Some(next_sibling)));
            current = next_sibling;
        }

        NodeIdVec::from(children)
    }

    /// Get the number of direct children of the given node
    ///
    /// Uses the contiguous node layout for efficient counting.
    pub fn get_children_count(&self, dom_id: DomId, node_id: NodeId) -> usize {
        let layout_window = self.get_layout_window();
        let layout_result = match layout_window.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return 0,
        };
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = match node_hierarchy.get(node_id) {
            Some(h) => h,
            None => return 0,
        };

        // Get first child - if none, return 0
        let first_child = match hier_item.first_child_id(node_id) {
            Some(fc) => fc,
            None => return 0,
        };

        // Count children by walking the sibling chain
        let mut count = 1;
        let mut current = first_child;
        while let Some(next_sibling) = node_hierarchy
            .get(current)
            .and_then(|h| h.next_sibling_id())
        {
            count += 1;
            current = next_sibling;
        }

        count
    }

    /// Change the image mask of a node (applied after callback returns)
    pub fn change_node_image_mask(&mut self, dom_id: DomId, node_id: NodeId, mask: ImageMask) {
        self.push_change(CallbackChange::ChangeNodeImageMask {
            dom_id,
            node_id,
            mask,
        });
    }

    /// Change CSS properties of a node (applied after callback returns)
    pub fn change_node_css_properties(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        properties: CssPropertyVec,
    ) {
        self.push_change(CallbackChange::ChangeNodeCssProperties {
            dom_id,
            node_id,
            properties,
        });
    }

    /// Set a single CSS property on a node (convenience method for widgets)
    ///
    /// This is a helper method that wraps `change_node_css_properties` for the common case
    /// of setting a single property. It uses the hit node's DOM ID automatically.
    ///
    /// # Arguments
    /// * `node_id` - The node to set the property on (uses hit node's DOM ID)
    /// * `property` - The CSS property to set
    pub fn set_css_property(&mut self, node_id: DomNodeId, property: CssProperty) {
        let dom_id = node_id.dom;
        let internal_node_id = node_id
            .node
            .into_crate_internal()
            .expect("DomNodeId node should not be None");
        self.change_node_css_properties(dom_id, internal_node_id, vec![property].into());
    }

    /// Scroll a node to a specific position (applied after callback returns)
    pub fn scroll_to(
        &mut self,
        dom_id: DomId,
        node_id: NodeHierarchyItemId,
        position: LogicalPosition,
    ) {
        self.push_change(CallbackChange::ScrollTo {
            dom_id,
            node_id,
            position,
        });
    }

    /// Scroll a node into view (W3C scrollIntoView API)
    ///
    /// Scrolls the element into the visible area of its scroll container.
    /// This is the recommended way to programmatically scroll elements into view.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node to scroll into view
    /// * `options` - Scroll alignment and animation options
    ///
    /// # Note
    ///
    /// This uses the transactional change system - the scroll is queued and applied
    /// after the callback returns. The actual scroll adjustments are calculated
    /// during change processing.
    pub fn scroll_node_into_view(
        &mut self,
        node_id: DomNodeId,
        options: crate::managers::scroll_into_view::ScrollIntoViewOptions,
    ) {
        self.push_change(CallbackChange::ScrollIntoView {
            node_id,
            options,
        });
    }

    /// Add an image to the image cache (applied after callback returns)
    pub fn add_image_to_cache(&mut self, id: AzString, image: ImageRef) {
        self.push_change(CallbackChange::AddImageToCache { id, image });
    }

    /// Remove an image from the image cache (applied after callback returns)
    pub fn remove_image_from_cache(&mut self, id: AzString) {
        self.push_change(CallbackChange::RemoveImageFromCache { id });
    }

    /// Reload system fonts (applied after callback returns)
    ///
    /// Note: This is an expensive operation that rebuilds the entire font cache
    pub fn reload_system_fonts(&mut self) {
        self.push_change(CallbackChange::ReloadSystemFonts);
    }

    // Text Input / Changeset Api

    /// Get the current text changeset being processed (if any)
    ///
    /// This allows callbacks to inspect what text input is about to be applied.
    /// Returns None if no text input is currently being processed.
    ///
    /// Use `set_text_changeset()` to modify the text that will be inserted,
    /// and `prevent_default()` to block the text input entirely.
    pub fn get_text_changeset(&self) -> Option<&PendingTextEdit> {
        self.get_layout_window()
            .text_input_manager
            .get_pending_changeset()
    }

    /// Set/override the text changeset for the current text input operation
    ///
    /// This allows you to modify what text will be inserted during text input events.
    /// Typically used in combination with `prevent_default()` to transform user input.
    ///
    /// # Arguments
    /// * `changeset` - The modified text changeset to apply
    pub fn set_text_changeset(&mut self, changeset: PendingTextEdit) {
        self.push_change(CallbackChange::SetTextChangeset { changeset });
    }

    /// Create a synthetic text input event
    ///
    /// This simulates receiving text input from the OS. Use this to programmatically
    /// insert text into contenteditable elements, for example from the debug server
    /// or from accessibility APIs.
    ///
    /// The text input flow will:
    /// 1. Record the text in TextInputManager (creating a PendingTextEdit)
    /// 2. Generate synthetic TextInput events
    /// 3. Invoke user callbacks (which can intercept/reject via preventDefault)
    /// 4. Apply the changeset if not rejected
    /// 5. Mark dirty nodes for re-render
    ///
    /// # Arguments
    /// * `text` - The text to insert at the current cursor position
    pub fn create_text_input(&mut self, text: AzString) {
        println!("[CallbackInfo::create_text_input] Creating text input: '{}'", text.as_str());
        self.push_change(CallbackChange::CreateTextInput { text });
    }

    /// Prevent the default text input from being applied
    ///
    /// When called in a TextInput callback, prevents the typed text from being inserted.
    /// Useful for custom validation, filtering, or text transformation.
    pub fn prevent_default(&mut self) {
        self.push_change(CallbackChange::PreventDefault);
    }

    // Cursor Blinking Api (for system timer control)
    
    /// Set cursor visibility state
    ///
    /// This is primarily used internally by the cursor blink timer callback.
    /// User code typically doesn't need to call this directly.
    pub fn set_cursor_visibility(&mut self, visible: bool) {
        self.push_change(CallbackChange::SetCursorVisibility { visible });
    }
    
    /// Reset cursor blink state on user input
    ///
    /// This makes the cursor visible and records the current time, so the blink
    /// timer knows to keep the cursor solid for a while before blinking.
    /// Called automatically on keyboard input, but can be called manually.
    pub fn reset_cursor_blink(&mut self) {
        self.push_change(CallbackChange::ResetCursorBlink);
    }
    
    /// Start the cursor blink timer
    ///
    /// Called automatically when focus lands on a contenteditable element.
    /// The timer will toggle cursor visibility at ~530ms intervals.
    pub fn start_cursor_blink_timer(&mut self) {
        self.push_change(CallbackChange::StartCursorBlinkTimer);
    }
    
    /// Stop the cursor blink timer
    ///
    /// Called automatically when focus leaves a contenteditable element.
    pub fn stop_cursor_blink_timer(&mut self) {
        self.push_change(CallbackChange::StopCursorBlinkTimer);
    }
    
    /// Scroll the active cursor into view
    ///
    /// This scrolls the focused text element's cursor into the visible area
    /// of any scrollable ancestor. Called automatically after text input.
    pub fn scroll_active_cursor_into_view(&mut self) {
        self.push_change(CallbackChange::ScrollActiveCursorIntoView);
    }

    /// Open a menu (context menu or dropdown)
    ///
    /// The menu will be displayed either as a native menu or a fallback DOM-based menu
    /// depending on the window's `use_native_context_menus` flag.
    /// Uses the position specified in the menu itself.
    ///
    /// # Arguments
    /// * `menu` - The menu to display
    pub fn open_menu(&mut self, menu: Menu) {
        self.push_change(CallbackChange::OpenMenu {
            menu,
            position: None,
        });
    }

    /// Open a menu at a specific position
    ///
    /// # Arguments
    /// * `menu` - The menu to display
    /// * `position` - The position where the menu should appear (overrides menu's position)
    pub fn open_menu_at(&mut self, menu: Menu, position: LogicalPosition) {
        self.push_change(CallbackChange::OpenMenu {
            menu,
            position: Some(position),
        });
    }

    // Tooltip Api

    /// Show a tooltip at the current cursor position
    ///
    /// Displays a simple text tooltip near the mouse cursor.
    /// The tooltip will be shown using platform-specific native APIs where available.
    ///
    /// Platform implementations:
    /// - **Windows**: Uses `TOOLTIPS_CLASS` Win32 control
    /// - **macOS**: Uses `NSPopover` or custom `NSWindow` with tooltip styling
    /// - **X11**: Creates transient window with `_NET_WM_WINDOW_TYPE_TOOLTIP`
    /// - **Wayland**: Uses `zwlr_layer_shell_v1` with overlay layer
    ///
    /// # Arguments
    /// * `text` - The tooltip text to display
    pub fn show_tooltip(&mut self, text: AzString) {
        let position = self
            .get_cursor_relative_to_viewport()
            .into_option()
            .unwrap_or_else(LogicalPosition::zero);
        self.push_change(CallbackChange::ShowTooltip { text, position });
    }

    /// Show a tooltip at a specific position
    ///
    /// # Arguments
    /// * `text` - The tooltip text to display
    /// * `position` - The position where the tooltip should appear (in window coordinates)
    pub fn show_tooltip_at(&mut self, text: AzString, position: LogicalPosition) {
        self.push_change(CallbackChange::ShowTooltip { text, position });
    }

    /// Hide the currently displayed tooltip
    pub fn hide_tooltip(&mut self) {
        self.push_change(CallbackChange::HideTooltip);
    }

    // Text Editing Api (transactional)

    /// Insert text at the current cursor position in a text node
    ///
    /// This operation is transactional - the text will be inserted after the callback returns.
    /// If there's a selection, it will be replaced with the inserted text.
    ///
    /// # Arguments
    /// * `dom_id` - The DOM containing the text node
    /// * `node_id` - The node to insert text into
    /// * `text` - The text to insert
    pub fn insert_text(&mut self, dom_id: DomId, node_id: NodeId, text: AzString) {
        self.push_change(CallbackChange::InsertText {
            dom_id,
            node_id,
            text,
        });
    }

    /// Move the text cursor to a specific position
    ///
    /// # Arguments
    /// * `dom_id` - The DOM containing the text node
    /// * `node_id` - The node containing the cursor
    /// * `cursor` - The new cursor position
    pub fn move_cursor(&mut self, dom_id: DomId, node_id: NodeId, cursor: TextCursor) {
        self.push_change(CallbackChange::MoveCursor {
            dom_id,
            node_id,
            cursor,
        });
    }

    /// Set the text selection range
    ///
    /// # Arguments
    /// * `dom_id` - The DOM containing the text node
    /// * `node_id` - The node containing the selection
    /// * `selection` - The new selection (can be a cursor or range)
    pub fn set_selection(&mut self, dom_id: DomId, node_id: NodeId, selection: Selection) {
        self.push_change(CallbackChange::SetSelection {
            dom_id,
            node_id,
            selection,
        });
    }

    /// Open a menu positioned relative to a specific DOM node
    ///
    /// This is useful for dropdowns, combo boxes, and context menus that should appear
    /// near a specific UI element. The menu will be positioned below the node by default.
    ///
    /// # Arguments
    /// * `menu` - The menu to display
    /// * `node_id` - The DOM node to position the menu relative to
    ///
    /// # Returns
    /// * `true` if the menu was queued for opening
    /// * `false` if the node doesn't exist or has no layout information
    pub fn open_menu_for_node(&mut self, menu: Menu, node_id: DomNodeId) -> bool {
        // Get the node's bounding rectangle
        if let Some(rect) = self.get_node_rect(node_id) {
            // Position menu at bottom-left of the node
            let position = LogicalPosition::new(rect.origin.x, rect.origin.y + rect.size.height);
            self.push_change(CallbackChange::OpenMenu {
                menu,
                position: Some(position),
            });
            true
        } else {
            false
        }
    }

    /// Open a menu positioned relative to the currently hit node
    ///
    /// Convenience method for opening a menu at the element that triggered the callback.
    /// Equivalent to `open_menu_for_node(menu, info.get_hit_node())`.
    ///
    /// # Arguments
    /// * `menu` - The menu to display
    ///
    /// # Returns
    /// * `true` if the menu was queued for opening
    /// * `false` if no node is currently hit or it has no layout information
    pub fn open_menu_for_hit_node(&mut self, menu: Menu) -> bool {
        let hit_node = self.get_hit_node();
        self.open_menu_for_node(menu, hit_node)
    }

    // Internal accessors

    /// Get reference to the underlying LayoutWindow for queries
    ///
    /// This provides read-only access to layout data, node hierarchies, managers, etc.
    /// All modifications should go through CallbackChange transactions via push_change().
    pub fn get_layout_window(&self) -> &LayoutWindow {
        unsafe { (*self.ref_data).layout_window }
    }

    /// Internal helper: Get the inline text layout for a given node
    ///
    /// This efficiently looks up the text layout by following the chain:
    /// LayoutWindow → layout_results → LayoutTree → dom_to_layout → LayoutNode →
    /// inline_layout_result
    ///
    /// Returns None if:
    /// - The DOM doesn't exist in layout_results
    /// - The node doesn't have a layout node mapping
    /// - The layout node doesn't have inline text layout
    fn get_inline_layout_for_node(&self, node_id: &DomNodeId) -> Option<&Arc<UnifiedLayout>> {
        let layout_window = self.get_layout_window();

        // Get the layout result for this DOM
        let layout_result = layout_window.layout_results.get(&node_id.dom)?;

        // Convert NodeHierarchyItemId to NodeId
        let dom_node_id = node_id.node.into_crate_internal()?;

        // Look up the layout node index(es) for this DOM node
        let layout_indices = layout_result.layout_tree.dom_to_layout.get(&dom_node_id)?;

        // Get the first layout node (a DOM node can generate multiple layout nodes,
        // but for text we typically only care about the first one)
        let layout_index = *layout_indices.first()?;

        // Get the layout node and its inline layout result
        let layout_node = layout_result.layout_tree.nodes.get(layout_index)?;
        layout_node
            .inline_layout_result
            .as_ref()
            .map(|c| c.get_layout())
    }

    // Public query Api
    // All methods below delegate to LayoutWindow for read-only access
    pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
        self.get_layout_window().get_node_size(node_id)
    }

    pub fn get_node_position(&self, node_id: DomNodeId) -> Option<LogicalPosition> {
        self.get_layout_window().get_node_position(node_id)
    }

    /// Get the hit test bounds of a node from the display list
    ///
    /// This is more reliable than get_node_rect because the display list
    /// always contains the correct final rendered positions.
    pub fn get_node_hit_test_bounds(&self, node_id: DomNodeId) -> Option<LogicalRect> {
        self.get_layout_window().get_node_hit_test_bounds(node_id)
    }

    /// Get the bounding rectangle of a node (position + size)
    ///
    /// This is particularly useful for menu positioning, where you need
    /// to know where a UI element is to popup a menu relative to it.
    pub fn get_node_rect(&self, node_id: DomNodeId) -> Option<LogicalRect> {
        let position = self.get_node_position(node_id)?;
        let size = self.get_node_size(node_id)?;
        Some(LogicalRect::new(position, size))
    }

    /// Get the bounding rectangle of the hit node
    ///
    /// Convenience method that combines get_hit_node() and get_node_rect().
    /// Useful for menu positioning based on the clicked element.
    pub fn get_hit_node_rect(&self) -> Option<LogicalRect> {
        let hit_node = self.get_hit_node();
        self.get_node_rect(hit_node)
    }

    // Timer Management (Query APIs)

    /// Get a reference to a timer
    pub fn get_timer(&self, timer_id: &TimerId) -> Option<&Timer> {
        self.get_layout_window().get_timer(timer_id)
    }

    /// Get all timer IDs
    pub fn get_timer_ids(&self) -> TimerIdVec {
        self.get_layout_window().get_timer_ids()
    }

    // Thread Management (Query APIs)

    /// Get a reference to a thread
    pub fn get_thread(&self, thread_id: &ThreadId) -> Option<&Thread> {
        self.get_layout_window().get_thread(thread_id)
    }

    /// Get all thread IDs
    pub fn get_thread_ids(&self) -> ThreadIdVec {
        self.get_layout_window().get_thread_ids()
    }

    // Gpu Value Cache Management (Query APIs)

    /// Get the GPU value cache for a specific DOM
    pub fn get_gpu_cache(&self, dom_id: &DomId) -> Option<&GpuValueCache> {
        self.get_layout_window().get_gpu_cache(dom_id)
    }

    // Layout Result Access (Query APIs)

    /// Get a layout result for a specific DOM
    pub fn get_layout_result(&self, dom_id: &DomId) -> Option<&DomLayoutResult> {
        self.get_layout_window().get_layout_result(dom_id)
    }

    /// Get all DOM IDs that have layout results
    pub fn get_dom_ids(&self) -> DomIdVec {
        self.get_layout_window().get_dom_ids()
    }

    // Node Hierarchy Navigation

    pub fn get_hit_node(&self) -> DomNodeId {
        self.hit_dom_node
    }

    /// Check if a node is anonymous (generated for table layout)
    fn is_node_anonymous(&self, dom_id: &DomId, node_id: NodeId) -> bool {
        let layout_window = self.get_layout_window();
        let layout_result = match layout_window.get_layout_result(dom_id) {
            Some(lr) => lr,
            None => return false,
        };
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = match node_data_cont.get(node_id) {
            Some(nd) => nd,
            None => return false,
        };
        node_data.is_anonymous()
    }

    pub fn get_parent(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;

        // Skip anonymous parent nodes - walk up the tree until we find a non-anonymous node
        let mut current_parent_id = hier_item.parent_id()?;
        loop {
            if !self.is_node_anonymous(&node_id.dom, current_parent_id) {
                return Some(DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(current_parent_id)),
                });
            }

            // This parent is anonymous, try its parent
            let parent_hier_item = node_hierarchy.get(current_parent_id)?;
            current_parent_id = parent_hier_item.parent_id()?;
        }
    }

    pub fn get_previous_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;

        // Skip anonymous siblings - walk backwards until we find a non-anonymous node
        let mut current_sibling_id = hier_item.previous_sibling_id()?;
        loop {
            if !self.is_node_anonymous(&node_id.dom, current_sibling_id) {
                return Some(DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(current_sibling_id)),
                });
            }

            // This sibling is anonymous, try the previous one
            let sibling_hier_item = node_hierarchy.get(current_sibling_id)?;
            current_sibling_id = sibling_hier_item.previous_sibling_id()?;
        }
    }

    pub fn get_next_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;

        // Skip anonymous siblings - walk forwards until we find a non-anonymous node
        let mut current_sibling_id = hier_item.next_sibling_id()?;
        loop {
            if !self.is_node_anonymous(&node_id.dom, current_sibling_id) {
                return Some(DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(current_sibling_id)),
                });
            }

            // This sibling is anonymous, try the next one
            let sibling_hier_item = node_hierarchy.get(current_sibling_id)?;
            current_sibling_id = sibling_hier_item.next_sibling_id()?;
        }
    }

    pub fn get_first_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;

        // Get first child, then skip anonymous nodes
        let mut current_child_id = hier_item.first_child_id(node_id_internal)?;
        loop {
            if !self.is_node_anonymous(&node_id.dom, current_child_id) {
                return Some(DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(current_child_id)),
                });
            }

            // This child is anonymous, try the next sibling
            let child_hier_item = node_hierarchy.get(current_child_id)?;
            current_child_id = child_hier_item.next_sibling_id()?;
        }
    }

    pub fn get_last_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hier_item = node_hierarchy.get(node_id_internal)?;

        // Get last child, then skip anonymous nodes by walking backwards
        let mut current_child_id = hier_item.last_child_id()?;
        loop {
            if !self.is_node_anonymous(&node_id.dom, current_child_id) {
                return Some(DomNodeId {
                    dom: node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(current_child_id)),
                });
            }

            // This child is anonymous, try the previous sibling
            let child_hier_item = node_hierarchy.get(current_child_id)?;
            current_child_id = child_hier_item.previous_sibling_id()?;
        }
    }

    // Node Data and State

    pub fn get_dataset(&mut self, node_id: DomNodeId) -> Option<RefAny> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_cont.get(node_id_internal)?;
        node_data.get_dataset().clone().into_option()
    }

    pub fn get_node_id_of_root_dataset(&mut self, search_key: RefAny) -> Option<DomNodeId> {
        let mut found: Option<(u64, DomNodeId)> = None;
        let search_type_id = search_key.get_type_id();

        for dom_id in self.get_dom_ids().as_ref().iter().copied() {
            let layout_window = self.get_layout_window();
            let layout_result = match layout_window.get_layout_result(&dom_id) {
                Some(lr) => lr,
                None => continue,
            };

            let node_data_cont = layout_result.styled_dom.node_data.as_container();
            for (node_idx, node_data) in node_data_cont.iter().enumerate() {
                if let Some(dataset) = node_data.get_dataset().clone().into_option() {
                    if dataset.get_type_id() == search_type_id {
                        let node_id = DomNodeId {
                            dom: dom_id,
                            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(
                                node_idx,
                            ))),
                        };
                        let instance_id = dataset.instance_id;

                        match found {
                            None => found = Some((instance_id, node_id)),
                            Some((prev_instance, _)) => {
                                if instance_id < prev_instance {
                                    found = Some((instance_id, node_id));
                                }
                            }
                        }
                    }
                }
            }
        }

        found.map(|s| s.1)
    }

    pub fn get_string_contents(&self, node_id: DomNodeId) -> Option<AzString> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_cont.get(node_id_internal)?;

        if let NodeType::Text(ref text) = node_data.get_node_type() {
            Some(text.clone())
        } else {
            None
        }
    }

    /// Get the tag name of a node (e.g., "div", "p", "span")
    ///
    /// Returns the HTML tag name as a string for the given node.
    /// For text nodes, returns "text". For image nodes, returns "img".
    pub fn get_node_tag_name(&self, node_id: DomNodeId) -> Option<AzString> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_cont.get(node_id_internal)?;

        let tag = node_data.get_node_type().get_path();
        Some(tag.to_string().into())
    }

    /// Get an attribute value from a node by attribute name
    ///
    /// # Arguments
    /// * `node_id` - The node to query
    /// * `attr_name` - The attribute name (e.g., "id", "class", "href", "data-custom", "aria-label")
    ///
    /// Returns the attribute value if found, None otherwise.
    /// This searches the strongly-typed AttributeVec on the node.
    pub fn get_node_attribute(&self, node_id: DomNodeId, attr_name: &str) -> Option<AzString> {
        use azul_core::dom::AttributeType;

        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_cont.get(node_id_internal)?;

        // Check the strongly-typed attributes vec
        for attr in node_data.attributes.as_ref() {
            match (attr_name, attr) {
                ("id", AttributeType::Id(v)) => return Some(v.clone()),
                ("class", AttributeType::Class(v)) => return Some(v.clone()),
                ("aria-label", AttributeType::AriaLabel(v)) => return Some(v.clone()),
                ("aria-labelledby", AttributeType::AriaLabelledBy(v)) => return Some(v.clone()),
                ("aria-describedby", AttributeType::AriaDescribedBy(v)) => return Some(v.clone()),
                ("role", AttributeType::AriaRole(v)) => return Some(v.clone()),
                ("href", AttributeType::Href(v)) => return Some(v.clone()),
                ("rel", AttributeType::Rel(v)) => return Some(v.clone()),
                ("target", AttributeType::Target(v)) => return Some(v.clone()),
                ("src", AttributeType::Src(v)) => return Some(v.clone()),
                ("alt", AttributeType::Alt(v)) => return Some(v.clone()),
                ("title", AttributeType::Title(v)) => return Some(v.clone()),
                ("name", AttributeType::Name(v)) => return Some(v.clone()),
                ("value", AttributeType::Value(v)) => return Some(v.clone()),
                ("type", AttributeType::InputType(v)) => return Some(v.clone()),
                ("placeholder", AttributeType::Placeholder(v)) => return Some(v.clone()),
                ("max", AttributeType::Max(v)) => return Some(v.clone()),
                ("min", AttributeType::Min(v)) => return Some(v.clone()),
                ("step", AttributeType::Step(v)) => return Some(v.clone()),
                ("pattern", AttributeType::Pattern(v)) => return Some(v.clone()),
                ("autocomplete", AttributeType::Autocomplete(v)) => return Some(v.clone()),
                ("scope", AttributeType::Scope(v)) => return Some(v.clone()),
                ("lang", AttributeType::Lang(v)) => return Some(v.clone()),
                ("dir", AttributeType::Dir(v)) => return Some(v.clone()),
                ("required", AttributeType::Required) => return Some("true".into()),
                ("disabled", AttributeType::Disabled) => return Some("true".into()),
                ("readonly", AttributeType::Readonly) => return Some("true".into()),
                ("checked", AttributeType::Checked) => return Some("true".into()),
                ("selected", AttributeType::Selected) => return Some("true".into()),
                ("hidden", AttributeType::Hidden) => return Some("true".into()),
                ("focusable", AttributeType::Focusable) => return Some("true".into()),
                ("minlength", AttributeType::MinLength(v)) => return Some(v.to_string().into()),
                ("maxlength", AttributeType::MaxLength(v)) => return Some(v.to_string().into()),
                ("colspan", AttributeType::ColSpan(v)) => return Some(v.to_string().into()),
                ("rowspan", AttributeType::RowSpan(v)) => return Some(v.to_string().into()),
                ("tabindex", AttributeType::TabIndex(v)) => return Some(v.to_string().into()),
                ("contenteditable", AttributeType::ContentEditable(v)) => {
                    return Some(v.to_string().into())
                }
                ("draggable", AttributeType::Draggable(v)) => return Some(v.to_string().into()),
                // Handle data-* attributes
                (name, AttributeType::Data(nv))
                    if name.starts_with("data-") && nv.attr_name.as_str() == &name[5..] =>
                {
                    return Some(nv.value.clone());
                }
                // Handle aria-* state/property attributes
                (name, AttributeType::AriaState(nv))
                    if name == format!("aria-{}", nv.attr_name.as_str()) =>
                {
                    return Some(nv.value.clone());
                }
                (name, AttributeType::AriaProperty(nv))
                    if name == format!("aria-{}", nv.attr_name.as_str()) =>
                {
                    return Some(nv.value.clone());
                }
                // Handle custom attributes
                (name, AttributeType::Custom(nv)) if nv.attr_name.as_str() == name => {
                    return Some(nv.value.clone());
                }
                _ => continue,
            }
        }

        // Fallback: check ids_and_classes for "id" and "class"
        if attr_name == "id" {
            for id_or_class in node_data.ids_and_classes.as_ref() {
                if let IdOrClass::Id(id) = id_or_class {
                    return Some(id.clone());
                }
            }
        }

        if attr_name == "class" {
            let classes: Vec<&str> = node_data
                .ids_and_classes
                .as_ref()
                .iter()
                .filter_map(|ioc| {
                    if let IdOrClass::Class(class) = ioc {
                        Some(class.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            if !classes.is_empty() {
                return Some(classes.join(" ").into());
            }
        }

        None
    }

    /// Get all classes of a node as a vector of strings
    pub fn get_node_classes(&self, node_id: DomNodeId) -> StringVec {
        let layout_window = match self.get_layout_window().get_layout_result(&node_id.dom) {
            Some(lr) => lr,
            None => return StringVec::from_const_slice(&[]),
        };
        let node_id_internal = match node_id.node.into_crate_internal() {
            Some(n) => n,
            None => return StringVec::from_const_slice(&[]),
        };
        let node_data_cont = layout_window.styled_dom.node_data.as_container();
        let node_data = match node_data_cont.get(node_id_internal) {
            Some(n) => n,
            None => return StringVec::from_const_slice(&[]),
        };

        let classes: Vec<AzString> = node_data
            .ids_and_classes
            .as_ref()
            .iter()
            .filter_map(|ioc| {
                if let IdOrClass::Class(class) = ioc {
                    Some(class.clone())
                } else {
                    None
                }
            })
            .collect();

        StringVec::from(classes)
    }

    /// Get the ID attribute of a node (if it has one)
    pub fn get_node_id(&self, node_id: DomNodeId) -> Option<AzString> {
        let layout_window = self.get_layout_window();
        let layout_result = layout_window.get_layout_result(&node_id.dom)?;
        let node_id_internal = node_id.node.into_crate_internal()?;
        let node_data_cont = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_cont.get(node_id_internal)?;

        for id_or_class in node_data.ids_and_classes.as_ref() {
            if let IdOrClass::Id(id) = id_or_class {
                return Some(id.clone());
            }
        }
        None
    }

    // Text Selection Management

    /// Get the current selection state for a DOM
    pub fn get_selection(&self, dom_id: &DomId) -> Option<&SelectionState> {
        self.get_layout_window()
            .selection_manager
            .get_selection(dom_id)
    }

    /// Check if a DOM has any selection
    pub fn has_selection(&self, dom_id: &DomId) -> bool {
        self.get_layout_window()
            .selection_manager
            .has_selection(dom_id)
    }

    /// Get the primary cursor for a DOM (first in selection list)
    pub fn get_primary_cursor(&self, dom_id: &DomId) -> Option<TextCursor> {
        self.get_layout_window()
            .selection_manager
            .get_primary_cursor(dom_id)
    }

    /// Get all selection ranges (excludes plain cursors)
    pub fn get_selection_ranges(&self, dom_id: &DomId) -> SelectionRangeVec {
        self.get_layout_window()
            .selection_manager
            .get_ranges(dom_id)
            .into()
    }

    /// Get direct access to the text layout cache
    ///
    /// Note: This provides direct read-only access to the text layout cache, but you need
    /// to know the CacheId for the specific text node you want. Currently there's
    /// no direct mapping from NodeId to CacheId exposed in the public API.
    ///
    /// For text modifications, use CallbackChange transactions:
    /// - `change_node_text()` for changing text content
    /// - `set_selection()` for setting selections
    /// - `get_selection()`, `get_primary_cursor()` for reading selections
    ///
    /// Future: Add NodeId -> CacheId mapping to enable node-specific layout access
    pub fn get_text_cache(&self) -> &TextLayoutCache {
        &self.get_layout_window().text_cache
    }

    // Window State Access

    /// Get full current window state (immutable reference)
    pub fn get_current_window_state(&self) -> &FullWindowState {
        // SAFETY: current_window_state is a valid pointer for the lifetime of CallbackInfo
        unsafe { (*self.ref_data).current_window_state }
    }

    /// Get current window flags
    pub fn get_current_window_flags(&self) -> WindowFlags {
        self.get_current_window_state().flags.clone()
    }

    /// Get current keyboard state
    pub fn get_current_keyboard_state(&self) -> KeyboardState {
        self.get_current_window_state().keyboard_state.clone()
    }

    /// Get current mouse state
    pub fn get_current_mouse_state(&self) -> MouseState {
        self.get_current_window_state().mouse_state.clone()
    }

    /// Get full previous window state (immutable reference)
    pub fn get_previous_window_state(&self) -> &Option<FullWindowState> {
        unsafe { (*self.ref_data).previous_window_state }
    }

    /// Get previous window flags
    pub fn get_previous_window_flags(&self) -> Option<WindowFlags> {
        Some(self.get_previous_window_state().as_ref()?.flags.clone())
    }

    /// Get previous keyboard state
    pub fn get_previous_keyboard_state(&self) -> Option<KeyboardState> {
        Some(
            self.get_previous_window_state()
                .as_ref()?
                .keyboard_state
                .clone(),
        )
    }

    /// Get previous mouse state
    pub fn get_previous_mouse_state(&self) -> Option<MouseState> {
        Some(
            self.get_previous_window_state()
                .as_ref()?
                .mouse_state
                .clone(),
        )
    }

    // Cursor and Input

    pub fn get_cursor_relative_to_node(&self) -> OptionLogicalPosition {
        self.cursor_relative_to_item
    }

    pub fn get_cursor_relative_to_viewport(&self) -> OptionLogicalPosition {
        self.cursor_in_viewport
    }

    pub fn get_current_window_handle(&self) -> RawWindowHandle {
        unsafe { (*self.ref_data).current_window_handle.clone() }
    }

    /// Get the system style (for menu rendering, CSD, etc.)
    /// This is useful for creating custom menus or other system-styled UI.
    pub fn get_system_style(&self) -> Arc<SystemStyle> {
        unsafe { (*self.ref_data).system_style.clone() }
    }

    // ==================== ICU4X Internationalization API ====================
    //
    // All formatting functions take a locale string (BCP 47 format) as the first
    // parameter, allowing dynamic language switching per-call.
    //
    // For date/time construction, use the static methods on IcuDate, IcuTime, IcuDateTime:
    // - IcuDate::now(), IcuDate::now_utc(), IcuDate::new(year, month, day)
    // - IcuTime::now(), IcuTime::now_utc(), IcuTime::new(hour, minute, second)
    // - IcuDateTime::now(), IcuDateTime::now_utc(), IcuDateTime::from_timestamp(secs)

    /// Get the ICU localizer cache for internationalized formatting.
    ///
    /// The cache stores localizers for multiple locales. Each locale's formatter
    /// is lazily created on first use and cached for subsequent calls.
    #[cfg(feature = "icu")]
    pub fn get_icu_localizer(&self) -> &IcuLocalizerHandle {
        unsafe { &(*self.ref_data).icu_localizer }
    }

    /// Format an integer with locale-appropriate grouping separators.
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string (e.g., "en-US", "de-DE", "ja-JP")
    /// * `value` - The integer to format
    ///
    /// # Example
    /// ```rust,ignore
    /// info.format_integer("en-US", 1234567) // → "1,234,567"
    /// info.format_integer("de-DE", 1234567) // → "1.234.567"
    /// info.format_integer("fr-FR", 1234567) // → "1 234 567"
    /// ```
    #[cfg(feature = "icu")]
    pub fn format_integer(&self, locale: &str, value: i64) -> AzString {
        self.get_icu_localizer().format_integer(locale, value)
    }

    /// Format a decimal number with locale-appropriate separators.
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string
    /// * `integer_part` - The full integer value (e.g., 123456 for 1234.56)
    /// * `decimal_places` - Number of decimal places (e.g., 2 for 1234.56)
    ///
    /// # Example
    /// ```rust,ignore
    /// info.format_decimal("en-US", 123456, 2) // → "1,234.56"
    /// info.format_decimal("de-DE", 123456, 2) // → "1.234,56"
    /// ```
    #[cfg(feature = "icu")]
    pub fn format_decimal(&self, locale: &str, integer_part: i64, decimal_places: i16) -> AzString {
        self.get_icu_localizer().format_decimal(locale, integer_part, decimal_places)
    }

    /// Get the plural category for a number (cardinal: "1 item", "2 items").
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string
    /// * `value` - The number to get the plural category for
    ///
    /// # Example
    /// ```rust,ignore
    /// info.get_plural_category("en", 1)  // → PluralCategory::One
    /// info.get_plural_category("en", 2)  // → PluralCategory::Other
    /// info.get_plural_category("pl", 2)  // → PluralCategory::Few
    /// info.get_plural_category("pl", 5)  // → PluralCategory::Many
    /// ```
    #[cfg(feature = "icu")]
    pub fn get_plural_category(&self, locale: &str, value: i64) -> PluralCategory {
        self.get_icu_localizer().get_plural_category(locale, value)
    }

    /// Select the appropriate string based on plural rules.
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string
    /// * `value` - The number to pluralize
    /// * `zero`, `one`, `two`, `few`, `many`, `other` - Strings for each category
    ///
    /// # Example
    /// ```rust,ignore
    /// info.pluralize("en", count, "no items", "1 item", "2 items", "{} items", "{} items", "{} items")
    /// info.pluralize("pl", count, "brak", "1 element", "2 elementy", "{} elementy", "{} elementów", "{} elementów")
    /// ```
    #[cfg(feature = "icu")]
    pub fn pluralize(
        &self,
        locale: &str,
        value: i64,
        zero: &str,
        one: &str,
        two: &str,
        few: &str,
        many: &str,
        other: &str,
    ) -> AzString {
        self.get_icu_localizer().pluralize(locale, value, zero, one, two, few, many, other)
    }

    /// Format a list of items with locale-appropriate conjunctions.
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string
    /// * `items` - The items to format as a list
    /// * `list_type` - And, Or, or Unit list type
    ///
    /// # Example
    /// ```rust,ignore
    /// info.format_list("en-US", &items, ListType::And) // → "A, B, and C"
    /// info.format_list("es-ES", &items, ListType::And) // → "A, B y C"
    /// ```
    #[cfg(feature = "icu")]
    pub fn format_list(&self, locale: &str, items: &[AzString], list_type: ListType) -> AzString {
        self.get_icu_localizer().format_list(locale, items, list_type)
    }

    /// Format a date according to the specified locale.
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string
    /// * `date` - The date to format (use IcuDate::now() or IcuDate::new())
    /// * `length` - Short, Medium, or Long format
    ///
    /// # Example
    /// ```rust,ignore
    /// let today = IcuDate::now();
    /// info.format_date("en-US", today, FormatLength::Medium) // → "Jan 15, 2025"
    /// info.format_date("de-DE", today, FormatLength::Medium) // → "15.01.2025"
    /// ```
    #[cfg(feature = "icu")]
    pub fn format_date(&self, locale: &str, date: IcuDate, length: FormatLength) -> IcuResult {
        self.get_icu_localizer().format_date(locale, date, length)
    }

    /// Format a time according to the specified locale.
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string
    /// * `time` - The time to format (use IcuTime::now() or IcuTime::new())
    /// * `include_seconds` - Whether to include seconds in the output
    ///
    /// # Example
    /// ```rust,ignore
    /// let now = IcuTime::now();
    /// info.format_time("en-US", now, false) // → "4:30 PM"
    /// info.format_time("de-DE", now, false) // → "16:30"
    /// ```
    #[cfg(feature = "icu")]
    pub fn format_time(&self, locale: &str, time: IcuTime, include_seconds: bool) -> IcuResult {
        self.get_icu_localizer().format_time(locale, time, include_seconds)
    }

    /// Format a date and time according to the specified locale.
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string
    /// * `datetime` - The date and time to format (use IcuDateTime::now())
    /// * `length` - Short, Medium, or Long format
    #[cfg(feature = "icu")]
    pub fn format_datetime(&self, locale: &str, datetime: IcuDateTime, length: FormatLength) -> IcuResult {
        self.get_icu_localizer().format_datetime(locale, datetime, length)
    }

    /// Compare two strings according to locale-specific collation rules.
    ///
    /// Returns -1 if a < b, 0 if a == b, 1 if a > b.
    /// This is useful for locale-aware sorting where "Ä" should sort with "A" in German.
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string
    /// * `a` - First string to compare
    /// * `b` - Second string to compare
    ///
    /// # Example
    /// ```rust,ignore
    /// info.compare_strings("de-DE", "Äpfel", "Banane") // → -1 (Ä sorts with A)
    /// info.compare_strings("sv-SE", "Äpple", "Öl")     // → -1 (Swedish: Ä before Ö)
    /// ```
    #[cfg(feature = "icu")]
    pub fn compare_strings(&self, locale: &str, a: &str, b: &str) -> i32 {
        self.get_icu_localizer().compare_strings(locale, a, b)
    }

    /// Sort a list of strings using locale-aware collation.
    ///
    /// This properly handles accented characters, case sensitivity, and
    /// language-specific sorting rules.
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string
    /// * `strings` - The strings to sort
    ///
    /// # Example
    /// ```rust,ignore
    /// let sorted = info.sort_strings("de-DE", &["Österreich", "Andorra", "Ägypten"]);
    /// // Result: ["Ägypten", "Andorra", "Österreich"] (Ä sorts with A, Ö with O)
    /// ```
    #[cfg(feature = "icu")]
    pub fn sort_strings(&self, locale: &str, strings: &[AzString]) -> IcuStringVec {
        self.get_icu_localizer().sort_strings(locale, strings)
    }

    /// Check if two strings are equal according to locale collation rules.
    ///
    /// This may return `true` for strings that differ in case or accents,
    /// depending on the collation strength.
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string
    /// * `a` - First string to compare
    /// * `b` - Second string to compare
    #[cfg(feature = "icu")]
    pub fn strings_equal(&self, locale: &str, a: &str, b: &str) -> bool {
        self.get_icu_localizer().strings_equal(locale, a, b)
    }

    /// Get the current cursor position in logical coordinates relative to the window
    pub fn get_cursor_position(&self) -> Option<LogicalPosition> {
        self.cursor_in_viewport.into_option()
    }

    /// Get the layout rectangle of the currently hit node (in logical coordinates)
    pub fn get_hit_node_layout_rect(&self) -> Option<LogicalRect> {
        self.get_layout_window()
            .get_node_layout_rect(self.hit_dom_node)
    }

    // Css Property Access

    /// Get the computed CSS property for a specific DOM node
    ///
    /// This queries the CSS property cache and returns the resolved property value
    /// for the given node, taking into account:
    /// - User overrides (from callbacks)
    /// - Node state (:hover, :active, :focus)
    /// - CSS rules from stylesheets
    /// - Cascaded properties from parents
    /// - Inline styles
    ///
    /// # Arguments
    /// * `node_id` - The DOM node to query
    /// * `property_type` - The CSS property type to retrieve
    ///
    /// # Returns
    /// * `Some(CssProperty)` if the property is set on this node
    /// * `None` if the property is not set (will use default value)
    pub fn get_computed_css_property(
        &self,
        node_id: DomNodeId,
        property_type: CssPropertyType,
    ) -> Option<CssProperty> {
        let layout_window = self.get_layout_window();

        // Get the layout result for this DOM
        let layout_result = layout_window.layout_results.get(&node_id.dom)?;

        // Get the styled DOM
        let styled_dom = &layout_result.styled_dom;

        // Convert DomNodeId to NodeId using proper decoding
        let internal_node_id = node_id.node.into_crate_internal()?;

        // Get the node data
        let node_data_container = styled_dom.node_data.as_container();
        let node_data = node_data_container.get(internal_node_id)?;

        // Get the styled node state
        let styled_nodes_container = styled_dom.styled_nodes.as_container();
        let styled_node = styled_nodes_container.get(internal_node_id)?;
        let node_state = &styled_node.styled_node_state;

        // Query the CSS property cache
        let css_property_cache = &styled_dom.css_property_cache.ptr;
        css_property_cache
            .get_property(node_data, &internal_node_id, node_state, &property_type)
            .cloned()
    }

    /// Get the computed width of a node from CSS
    ///
    /// Convenience method for getting the CSS width property.
    pub fn get_computed_width(&self, node_id: DomNodeId) -> Option<CssProperty> {
        self.get_computed_css_property(node_id, CssPropertyType::Width)
    }

    /// Get the computed height of a node from CSS
    ///
    /// Convenience method for getting the CSS height property.
    pub fn get_computed_height(&self, node_id: DomNodeId) -> Option<CssProperty> {
        self.get_computed_css_property(node_id, CssPropertyType::Height)
    }

    // System Callbacks

    pub fn get_system_time_fn(&self) -> GetSystemTimeCallback {
        unsafe { (*self.ref_data).system_callbacks.get_system_time_fn }
    }

    pub fn get_current_time(&self) -> task::Instant {
        let cb = self.get_system_time_fn();
        (cb.cb)()
    }

    /// Get immutable reference to the renderer resources
    ///
    /// This provides access to fonts, images, and other rendering resources.
    /// Useful for custom rendering or screenshot functionality.
    pub fn get_renderer_resources(&self) -> &RendererResources {
        unsafe { (*self.ref_data).renderer_resources }
    }

    // Screenshot API

    /// Take a CPU-rendered screenshot of the current window content
    ///
    /// This renders the current display list to a PNG image using CPU rendering.
    /// The screenshot captures the window content as it would appear on screen,
    /// without window decorations.
    ///
    /// # Arguments
    /// * `dom_id` - The DOM to screenshot (use the main DOM ID for the full window)
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - PNG-encoded image data
    /// * `Err(String)` - Error message if rendering failed
    ///
    /// # Example
    /// ```ignore
    /// fn on_click(info: &mut CallbackInfo) -> Update {
    ///     let dom_id = info.get_hit_node().dom;
    ///     match info.take_screenshot(dom_id) {
    ///         Ok(png_data) => {
    ///             std::fs::write("screenshot.png", png_data).unwrap();
    ///         }
    ///         Err(e) => eprintln!("Screenshot failed: {}", e),
    ///     }
    ///     Update::DoNothing
    /// }
    /// ```
    #[cfg(feature = "cpurender")]
    pub fn take_screenshot(&self, dom_id: DomId) -> Result<alloc::vec::Vec<u8>, AzString> {
        use crate::cpurender::{render, RenderOptions};

        let layout_window = self.get_layout_window();
        let renderer_resources = self.get_renderer_resources();

        // Get the layout result for this DOM
        let layout_result = layout_window
            .layout_results
            .get(&dom_id)
            .ok_or_else(|| AzString::from("DOM not found in layout results"))?;

        // Get viewport dimensions
        let viewport = &layout_result.viewport;
        let width = viewport.size.width;
        let height = viewport.size.height;

        if width <= 0.0 || height <= 0.0 {
            return Err(AzString::from("Invalid viewport dimensions"));
        }

        // Get the display list
        let display_list = &layout_result.display_list;

        // Get DPI factor from window state
        let dpi_factor = self
            .get_current_window_state()
            .size
            .get_hidpi_factor()
            .inner
            .get();

        // Render to pixmap
        let opts = RenderOptions {
            width,
            height,
            dpi_factor,
        };

        let pixmap =
            render(display_list, renderer_resources, opts).map_err(|e| AzString::from(e))?;

        // Encode to PNG
        let png_data = pixmap
            .encode_png()
            .map_err(|e| AzString::from(alloc::format!("PNG encoding failed: {}", e)))?;

        Ok(png_data)
    }

    /// Take a screenshot and save it directly to a file
    ///
    /// Convenience method that combines `take_screenshot` with file writing.
    ///
    /// # Arguments
    /// * `dom_id` - The DOM to screenshot
    /// * `path` - The file path to save the PNG to
    ///
    /// # Returns
    /// * `Ok(())` - Screenshot saved successfully
    /// * `Err(String)` - Error message if rendering or saving failed
    #[cfg(all(feature = "std", feature = "cpurender"))]
    pub fn take_screenshot_to_file(&self, dom_id: DomId, path: &str) -> Result<(), AzString> {
        let png_data = self.take_screenshot(dom_id)?;
        std::fs::write(path, png_data)
            .map_err(|e| AzString::from(alloc::format!("Failed to write file: {}", e)))?;
        Ok(())
    }

    /// Take a native OS-level screenshot of the window including window decorations
    ///
    /// **NOTE**: This is a stub implementation. For full native screenshot support,
    /// use the `NativeScreenshotExt` trait from the `azul-dll` crate, which uses
    /// runtime dynamic loading (dlopen) to avoid static linking dependencies.
    ///
    /// # Returns
    /// * `Err(String)` - Always returns an error directing to use the extension trait
    #[cfg(feature = "std")]
    pub fn take_native_screenshot(&self, _path: &str) -> Result<(), AzString> {
        Err(AzString::from(
            "Native screenshot requires the NativeScreenshotExt trait from azul-dll crate. \
             Import it with: use azul::desktop::NativeScreenshotExt;",
        ))
    }

    /// Take a native OS-level screenshot and return the PNG data as bytes
    ///
    /// **NOTE**: This is a stub implementation. For full native screenshot support,
    /// use the `NativeScreenshotExt` trait from the `azul-dll` crate.
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - PNG-encoded image data
    /// * `Err(String)` - Error message if screenshot failed
    #[cfg(feature = "std")]
    pub fn take_native_screenshot_bytes(&self) -> Result<alloc::vec::Vec<u8>, AzString> {
        // Create a temporary file, take screenshot, read bytes, delete file
        let temp_path = std::env::temp_dir().join("azul_screenshot_temp.png");
        let temp_path_str = temp_path.to_string_lossy().to_string();

        self.take_native_screenshot(&temp_path_str)?;

        let bytes = std::fs::read(&temp_path)
            .map_err(|e| AzString::from(alloc::format!("Failed to read screenshot: {}", e)))?;

        let _ = std::fs::remove_file(&temp_path);

        Ok(bytes)
    }

    /// Take a native OS-level screenshot and return as a Base64 data URI
    ///
    /// Returns the screenshot as a "data:image/png;base64,..." string that can
    /// be directly used in HTML img tags or JSON responses.
    ///
    /// # Returns
    /// * `Ok(String)` - Base64 data URI string
    /// * `Err(String)` - Error message if screenshot failed
    ///
    #[cfg(feature = "std")]
    pub fn take_native_screenshot_base64(&self) -> Result<AzString, AzString> {
        let png_bytes = self.take_native_screenshot_bytes()?;
        let base64_str = base64_encode(&png_bytes);
        Ok(AzString::from(alloc::format!(
            "data:image/png;base64,{}",
            base64_str
        )))
    }

    /// Take a CPU-rendered screenshot and return as a Base64 data URI
    ///
    /// Returns the screenshot as a "data:image/png;base64,..." string.
    /// This is the software-rendered version without window decorations.
    ///
    /// # Returns
    /// * `Ok(String)` - Base64 data URI string
    /// * `Err(String)` - Error message if rendering failed
    #[cfg(feature = "cpurender")]
    pub fn take_screenshot_base64(&self, dom_id: DomId) -> Result<AzString, AzString> {
        let png_bytes = self.take_screenshot(dom_id)?;
        let base64_str = base64_encode(&png_bytes);
        Ok(AzString::from(alloc::format!(
            "data:image/png;base64,{}",
            base64_str
        )))
    }

    // Manager Access (Read-Only)

    /// Get immutable reference to the scroll manager
    ///
    /// Use this to query scroll state for nodes without modifying it.
    /// To request programmatic scrolling, use `nodes_scrolled_in_callback`.
    pub fn get_scroll_manager(&self) -> &ScrollManager {
        unsafe { &(*self.ref_data).layout_window.scroll_manager }
    }

    /// Get immutable reference to the gesture and drag manager
    ///
    /// Use this to query current gesture/drag state (e.g., "is this node being dragged?",
    /// "what files are being dropped?", "is a long-press active?").
    ///
    /// The manager is updated by the event loop and provides read-only query access
    /// to callbacks for gesture-aware UI behavior.
    pub fn get_gesture_drag_manager(&self) -> &GestureAndDragManager {
        unsafe { &(*self.ref_data).layout_window.gesture_drag_manager }
    }

    /// Get immutable reference to the focus manager
    ///
    /// Use this to query which node currently has focus and whether focus
    /// is being moved to another node.
    pub fn get_focus_manager(&self) -> &FocusManager {
        &self.get_layout_window().focus_manager
    }

    /// Get a reference to the undo/redo manager
    ///
    /// This allows user callbacks to query the undo/redo state and intercept
    /// undo/redo operations via preventDefault().
    pub fn get_undo_redo_manager(&self) -> &UndoRedoManager {
        &self.get_layout_window().undo_redo_manager
    }

    /// Get immutable reference to the hover manager
    ///
    /// Use this to query which nodes are currently hovered at various input points
    /// (mouse, touch points, pen).
    pub fn get_hover_manager(&self) -> &HoverManager {
        &self.get_layout_window().hover_manager
    }

    /// Get immutable reference to the text input manager
    ///
    /// Use this to query text selection state, cursor positions, and IME composition.
    pub fn get_text_input_manager(&self) -> &TextInputManager {
        &self.get_layout_window().text_input_manager
    }

    /// Get immutable reference to the selection manager
    ///
    /// Use this to query text selections across multiple nodes.
    pub fn get_selection_manager(&self) -> &SelectionManager {
        &self.get_layout_window().selection_manager
    }

    /// Check if a specific node is currently focused
    pub fn is_node_focused(&self, node_id: DomNodeId) -> bool {
        self.get_focus_manager().has_focus(&node_id)
    }

    /// Check if any node in a specific DOM is focused
    pub fn is_dom_focused(&self, dom_id: DomId) -> bool {
        self.get_focused_node()
            .map(|n| n.dom == dom_id)
            .unwrap_or(false)
    }

    // Pen/Stylus Query Methods

    /// Get current pen/stylus state if a pen is active
    pub fn get_pen_state(&self) -> Option<&PenState> {
        self.get_gesture_drag_manager().get_pen_state()
    }

    /// Get current pen pressure (0.0 to 1.0)
    /// Returns None if no pen is active, Some(0.5) for mouse
    pub fn get_pen_pressure(&self) -> Option<f32> {
        self.get_pen_state().map(|pen| pen.pressure)
    }

    /// Get current pen tilt angles (x_tilt, y_tilt) in degrees
    /// Returns None if no pen is active
    pub fn get_pen_tilt(&self) -> Option<PenTilt> {
        self.get_pen_state().map(|pen| pen.tilt)
    }

    /// Check if pen is currently in contact with surface
    pub fn is_pen_in_contact(&self) -> bool {
        self.get_pen_state()
            .map(|pen| pen.in_contact)
            .unwrap_or(false)
    }

    /// Check if pen is in eraser mode
    pub fn is_pen_eraser(&self) -> bool {
        self.get_pen_state()
            .map(|pen| pen.is_eraser)
            .unwrap_or(false)
    }

    /// Check if pen barrel button is pressed
    pub fn is_pen_barrel_button_pressed(&self) -> bool {
        self.get_pen_state()
            .map(|pen| pen.barrel_button_pressed)
            .unwrap_or(false)
    }

    /// Get the last recorded input sample (for event_id and detailed input data)
    pub fn get_last_input_sample(&self) -> Option<&InputSample> {
        let manager = self.get_gesture_drag_manager();
        manager
            .get_current_session()
            .and_then(|session| session.last_sample())
    }

    /// Get the event ID of the current event
    pub fn get_current_event_id(&self) -> Option<u64> {
        self.get_last_input_sample().map(|sample| sample.event_id)
    }

    // Focus Management Methods

    /// Set focus to a specific DOM node by ID
    pub fn set_focus_to_node(&mut self, dom_id: DomId, node_id: NodeId) {
        self.set_focus(FocusTarget::Id(DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
        }));
    }

    /// Set focus to a node matching a CSS path
    pub fn set_focus_to_path(&mut self, dom_id: DomId, css_path: CssPath) {
        self.set_focus(FocusTarget::Path(FocusTargetPath {
            dom: dom_id,
            css_path,
        }));
    }

    /// Move focus to next focusable element in tab order
    pub fn focus_next(&mut self) {
        self.set_focus(FocusTarget::Next);
    }

    /// Move focus to previous focusable element in tab order
    pub fn focus_previous(&mut self) {
        self.set_focus(FocusTarget::Previous);
    }

    /// Move focus to first focusable element
    pub fn focus_first(&mut self) {
        self.set_focus(FocusTarget::First);
    }

    /// Move focus to last focusable element
    pub fn focus_last(&mut self) {
        self.set_focus(FocusTarget::Last);
    }

    /// Remove focus from all elements
    pub fn clear_focus(&mut self) {
        self.set_focus(FocusTarget::NoFocus);
    }

    // Manager Access Methods

    /// Check if a drag gesture is currently active
    ///
    /// Convenience method that queries the gesture manager.
    pub fn is_dragging(&self) -> bool {
        self.get_gesture_drag_manager().is_dragging()
    }

    /// Get the currently focused node (if any)
    ///
    /// Returns None if no node has focus.
    pub fn get_focused_node(&self) -> Option<DomNodeId> {
        self.get_layout_window()
            .focus_manager
            .get_focused_node()
            .copied()
    }

    /// Check if a specific node has focus
    pub fn has_focus(&self, node_id: DomNodeId) -> bool {
        self.get_layout_window().focus_manager.has_focus(&node_id)
    }

    /// Get the currently hovered file (if drag-drop is in progress)
    ///
    /// Returns None if no file is being hovered over the window.
    pub fn get_hovered_file(&self) -> Option<&azul_css::AzString> {
        self.get_layout_window()
            .file_drop_manager
            .get_hovered_file()
    }

    /// Get the currently dropped file (if a file was just dropped)
    ///
    /// This is a one-shot value that is cleared after event processing.
    /// Returns None if no file was dropped this frame.
    pub fn get_dropped_file(&self) -> Option<&azul_css::AzString> {
        self.get_layout_window()
            .file_drop_manager
            .dropped_file
            .as_ref()
    }

    /// Check if a node or file drag is currently active
    ///
    /// Returns true if either a node drag or file drag is in progress.
    pub fn is_drag_active(&self) -> bool {
        self.get_layout_window().drag_drop_manager.is_dragging()
    }

    /// Check if a node drag is specifically active
    pub fn is_node_drag_active(&self) -> bool {
        self.get_layout_window()
            .drag_drop_manager
            .is_dragging_node()
    }

    /// Check if a file drag is specifically active
    pub fn is_file_drag_active(&self) -> bool {
        self.get_layout_window()
            .drag_drop_manager
            .is_dragging_file()
    }

    /// Get the current drag/drop state (if any)
    ///
    /// Returns None if no drag is active, or Some with drag state.
    /// Uses legacy DragSt

... [FILE TRUNCATED - original size: 147648 bytes] ...
```

### layout/src/window.rs

```rust
//! Window layout management for solver3/text3
//!
//! This module provides the high-level API for managing layout
//! state across frames, including caching, incremental updates,
//! and display list generation.
//!
//! The main entry point is `LayoutWindow`, which encapsulates all
//! the state needed to perform layout and maintain consistency
//! across window resizes and DOM updates.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use azul_core::{
    animation::UpdateImageType,
    callbacks::{FocusTarget, HidpiAdjustedBounds, IFrameCallbackReason, Update},
    dom::{
        AccessibilityAction, AttributeType, Dom, DomId, DomIdVec, DomNodeId, NodeId, NodeType, On,
    },
    events::{EasingFunction, EventFilter, FocusEventFilter, HoverEventFilter},
    geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition},
    gl::OptionGlContextPtr,
    gpu::{GpuScrollbarOpacityEvent, GpuValueCache},
    hit_test::{DocumentId, ScrollPosition, ScrollbarHitId},
    refany::{OptionRefAny, RefAny},
    resources::{
        Epoch, FontKey, GlTextureCache, IdNamespace, ImageCache, ImageMask, ImageRef, ImageRefHash,
        OpacityKey, RendererResources,
    },
    selection::{
        CursorAffinity, GraphemeClusterId, Selection, SelectionAnchor, SelectionFocus,
        SelectionRange, SelectionState, TextCursor, TextSelection,
    },
    styled_dom::{
        collect_nodes_in_document_order, is_before_in_document_order, NodeHierarchyItemId,
        StyledDom,
    },
    task::{
        Duration, Instant, SystemTickDiff, SystemTimeDiff, TerminateTimer, ThreadId, ThreadIdVec,
        ThreadSendMsg, TimerId, TimerIdVec,
    },
    window::{CursorPosition, RawWindowHandle, RendererType},
    FastBTreeSet, FastHashMap,
};
use azul_css::{
    css::Css,
    props::{
        basic::FontRef,
        property::{CssProperty, CssPropertyVec},
    },
    AzString, LayoutDebugMessage, OptionString,
};
use rust_fontconfig::FcFontCache;

#[cfg(feature = "icu")]
use crate::icu::IcuLocalizerHandle;
use crate::{
    callbacks::{
        CallCallbacksResult, Callback, ExternalSystemCallbacks, FocusUpdateRequest, MenuCallback,
    },
    managers::{
        gpu_state::GpuStateManager,
        iframe::IFrameManager,
        scroll_state::{ScrollManager, ScrollStates},
    },
    solver3::{
        self, cache::LayoutCache as Solver3LayoutCache, display_list::DisplayList,
        layout_tree::LayoutTree,
    },
    text3::{
        cache::{
            FontManager, FontSelector, FontStyle, InlineContent, LayoutCache as TextLayoutCache,
            LayoutError, ShapedItem, StyleProperties, StyledRun, TextBoundary, UnifiedConstraints,
            UnifiedLayout,
        },
        default::PathLoader,
    },
    thread::{OptionThreadReceiveMsg, Thread, ThreadReceiveMsg, ThreadWriteBackMsg},
    timer::Timer,
    window_state::{FullWindowState, WindowCreateOptions},
};

// Global atomic counters for generating unique IDs
static DOCUMENT_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
static ID_NAMESPACE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Helper function to create a unique DocumentId
fn new_document_id() -> DocumentId {
    let namespace_id = new_id_namespace();
    let id = DOCUMENT_ID_COUNTER.fetch_add(1, Ordering::Relaxed) as u32;
    DocumentId { namespace_id, id }
}

/// Direction for cursor navigation
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CursorNavigationDirection {
    /// Move cursor up one line
    Up,
    /// Move cursor down one line
    Down,
    /// Move cursor left one character
    Left,
    /// Move cursor right one character
    Right,
    /// Move cursor to start of current line
    LineStart,
    /// Move cursor to end of current line
    LineEnd,
    /// Move cursor to start of document
    DocumentStart,
    /// Move cursor to end of document
    DocumentEnd,
}

/// Result of a cursor movement operation
#[derive(Debug, Clone)]
pub enum CursorMovementResult {
    /// Cursor moved within the same text node
    MovedWithinNode(TextCursor),
    /// Cursor moved to a different text node
    MovedToNode {
        dom_id: DomId,
        node_id: NodeId,
        cursor: TextCursor,
    },
    /// Cursor is at a boundary and cannot move further
    AtBoundary {
        boundary: TextBoundary,
        cursor: TextCursor,
    },
}

/// Error when no cursor destination is available
#[derive(Debug, Clone)]
pub struct NoCursorDestination {
    pub reason: String,
}

/// Action to take for the cursor blink timer when focus changes
///
/// This enum is returned by `LayoutWindow::handle_focus_change_for_cursor_blink()`
/// to tell the platform layer what timer action to take.
#[derive(Debug, Clone)]
pub enum CursorBlinkTimerAction {
    /// Start the cursor blink timer with the given timer configuration
    Start(crate::timer::Timer),
    /// Stop the cursor blink timer
    Stop,
    /// No change needed (timer already in correct state)
    NoChange,
}

/// Helper function to create a unique IdNamespace
fn new_id_namespace() -> IdNamespace {
    let id = ID_NAMESPACE_COUNTER.fetch_add(1, Ordering::Relaxed) as u32;
    IdNamespace(id)
}

// ============================================================================
// Cursor Blink Timer Callback
// ============================================================================

/// Destructor for cursor blink timer RefAny (no-op since we use null pointer)
extern "C" fn cursor_blink_timer_destructor(_: RefAny) {
    // No cleanup needed - we use a null pointer RefAny
}

/// Callback for the cursor blink timer
///
/// This function is called every ~530ms to toggle cursor visibility.
/// It checks if enough time has passed since the last user input before blinking,
/// to avoid blinking while the user is actively typing.
///
/// The callback returns:
/// - `TerminateTimer::Continue` + `Update::RefreshDom` if cursor toggled
/// - `TerminateTimer::Terminate` if focus is no longer on a contenteditable element
pub extern "C" fn cursor_blink_timer_callback(
    _data: RefAny,
    mut info: crate::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    use azul_core::callbacks::{TimerCallbackReturn, Update};
    use azul_core::task::TerminateTimer;
    
    // Get current time
    let now = info.get_current_time();
    
    // We need to access the LayoutWindow through the info
    // The timer callback needs to:
    // 1. Check if focus is still on a contenteditable element
    // 2. Check time since last input
    // 3. Toggle visibility or keep solid
    
    // For now, we'll queue changes via the CallbackInfo system
    // The actual state modification happens in apply_callback_changes
    
    // Check if we should blink or stay solid
    // This is done by checking CursorManager.should_blink(now) in the layout window
    
    // Since we can't access LayoutWindow directly here (it's not passed to timer callbacks),
    // we use a different approach: the timer callback always toggles, and the visibility
    // check is done in display_list.rs based on CursorManager state.
    
    // Simply toggle cursor visibility
    info.set_cursor_visibility_toggle();
    
    // Continue the timer and request a redraw
    TimerCallbackReturn {
        should_update: Update::RefreshDom,
        should_terminate: TerminateTimer::Continue,
    }
}

/// Result of a layout pass for a single DOM, before display list generation
#[derive(Debug)]
pub struct DomLayoutResult {
    /// The styled DOM that was laid out
    pub styled_dom: StyledDom,
    /// The layout tree with computed sizes and positions
    pub layout_tree: LayoutTree,
    /// Absolute positions of all nodes
    pub calculated_positions: BTreeMap<usize, LogicalPosition>,
    /// The viewport used for this layout
    pub viewport: LogicalRect,
    /// The generated display list for this DOM.
    pub display_list: DisplayList,
    /// Stable scroll IDs computed from node_data_hash
    /// Maps layout node index -> external scroll ID
    pub scroll_ids: BTreeMap<usize, u64>,
    /// Mapping from scroll IDs to DOM NodeIds for hit testing
    /// This allows us to map WebRender scroll IDs back to DOM nodes
    pub scroll_id_to_node_id: BTreeMap<u64, NodeId>,
}

/// State for tracking scrollbar drag interaction
#[derive(Debug, Clone)]
pub struct ScrollbarDragState {
    pub hit_id: ScrollbarHitId,
    pub initial_mouse_pos: LogicalPosition,
    pub initial_scroll_offset: LogicalPosition,
}

/// Information about the last text edit operation
/// Allows callbacks to query what changed during text input
// Re-export PendingTextEdit from text_input manager
pub use crate::managers::text_input::PendingTextEdit;

/// Cached text layout constraints for a node
/// These are the layout parameters that were used to shape the text
#[derive(Debug, Clone)]
pub struct TextConstraintsCache {
    /// Map from (dom_id, node_id) to their layout constraints
    pub constraints: BTreeMap<(DomId, NodeId), UnifiedConstraints>,
}

impl Default for TextConstraintsCache {
    fn default() -> Self {
        Self {
            constraints: BTreeMap::new(),
        }
    }
}

/// A text node that has been edited since the last full layout.
/// This allows us to perform lightweight relayout without rebuilding the entire DOM.
#[derive(Debug, Clone)]
pub struct DirtyTextNode {
    /// The new inline content (text + images) after editing
    pub content: Vec<InlineContent>,
    /// The new cursor position after editing
    pub cursor: Option<TextCursor>,
    /// Whether this edit requires ancestor relayout (e.g., text grew taller)
    pub needs_ancestor_relayout: bool,
}

/// Result of applying callback changes
///
/// This struct consolidates all the outputs from `apply_callback_changes()`,
/// eliminating the need for 18+ mutable reference parameters.
#[derive(Debug, Default)]
pub struct CallbackChangeResult {
    /// Timers to add
    pub timers: FastHashMap<TimerId, crate::timer::Timer>,
    /// Threads to add  
    pub threads: FastHashMap<ThreadId, crate::thread::Thread>,
    /// Timers to remove
    pub timers_removed: FastBTreeSet<TimerId>,
    /// Threads to remove
    pub threads_removed: FastBTreeSet<ThreadId>,
    /// New windows to create
    pub windows_created: Vec<crate::window_state::WindowCreateOptions>,
    /// Menus to open
    pub menus_to_open: Vec<(azul_core::menu::Menu, Option<LogicalPosition>)>,
    /// Tooltips to show
    pub tooltips_to_show: Vec<(AzString, LogicalPosition)>,
    /// Whether to hide tooltip
    pub hide_tooltip: bool,
    /// Whether stopPropagation() was called
    pub stop_propagation: bool,
    /// Whether preventDefault() was called
    pub prevent_default: bool,
    /// Focus target change
    pub focus_target: Option<FocusTarget>,
    /// Text changes that don't require full relayout
    pub words_changed: BTreeMap<DomId, BTreeMap<NodeId, AzString>>,
    /// Image changes (for animated images/video)
    pub images_changed: BTreeMap<DomId, BTreeMap<NodeId, (ImageRef, UpdateImageType)>>,
    /// Image callback nodes that need to be re-rendered (for resize/animations)
    /// Unlike images_changed, this triggers a callback re-invocation
    pub image_callbacks_changed: BTreeMap<DomId, FastBTreeSet<NodeId>>,
    /// IFrame nodes that need to be re-rendered (for content updates)
    /// This triggers the IFrame callback to be called with DomRecreated reason
    pub iframes_to_update: BTreeMap<DomId, FastBTreeSet<NodeId>>,
    /// Clip mask changes (for vector animations)
    pub image_masks_changed: BTreeMap<DomId, BTreeMap<NodeId, ImageMask>>,
    /// CSS property changes from callbacks
    pub css_properties_changed: BTreeMap<DomId, BTreeMap<NodeId, CssPropertyVec>>,
    /// Scroll position changes from callbacks
    pub nodes_scrolled: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, LogicalPosition>>,
    /// Modified window state
    pub modified_window_state: FullWindowState,
    /// Queued window states to apply in sequence (for simulating clicks, etc.)
    /// Each state will trigger separate event processing to detect state changes.
    pub queued_window_states: Vec<FullWindowState>,
    /// Hit test update requested at this position (for Debug API)
    /// When set, the shell layer should perform a hit test update before processing events
    pub hit_test_update_requested: Option<LogicalPosition>,
    /// Text input events triggered by CreateTextInput
    /// These need to be processed by the recursive event loop to invoke user callbacks
    pub text_input_triggered: Vec<(azul_core::dom::DomNodeId, Vec<azul_core::events::EventFilter>)>,
}

/// A window-level layout manager that encapsulates all layout state and caching.
///
/// This struct owns the layout and text caches, and provides methods dir_to:
/// - Perform initial layout
/// - Incrementally update layout on DOM changes
/// - Generate display lists for rendering
/// - Handle window resizes efficiently
/// - Manage multiple DOMs (for IFrames)
pub struct LayoutWindow {
    /// Fragmentation context for this window (continuous for screen, paged for print)
    #[cfg(feature = "pdf")]
    pub fragmentation_context: crate::paged::FragmentationContext,
    /// Layout cache for solver3 (incremental layout tree) - for the root DOM
    pub layout_cache: Solver3LayoutCache,
    /// Text layout cache for text3 (shaped glyphs, line breaks, etc.)
    pub text_cache: TextLayoutCache,
    /// Font manager for loading and caching fonts
    pub font_manager: FontManager<FontRef>,
    /// Cache to store decoded images
    pub image_cache: ImageCache,
    /// Cached layout results for all DOMs (root + iframes)
    pub layout_results: BTreeMap<DomId, DomLayoutResult>,
    /// Scroll state manager for all nodes across all DOMs
    pub scroll_manager: ScrollManager,
    /// Gesture and drag manager for multi-frame interactions (moved from FullWindowState)
    pub gesture_drag_manager: crate::managers::gesture::GestureAndDragManager,
    /// Focus manager for keyboard focus and tab navigation
    pub focus_manager: crate::managers::focus_cursor::FocusManager,
    /// Cursor manager for text cursor position and rendering
    pub cursor_manager: crate::managers::cursor::CursorManager,
    /// File drop manager for cursor state and file drag-drop
    pub file_drop_manager: crate::managers::file_drop::FileDropManager,
    /// Selection manager for text selections across all DOMs
    pub selection_manager: crate::managers::selection::SelectionManager,
    /// Clipboard manager for system clipboard integration
    pub clipboard_manager: crate::managers::clipboard::ClipboardManager,
    /// Drag-drop manager for node and file dragging operations
    pub drag_drop_manager: crate::managers::drag_drop::DragDropManager,
    /// Hover manager for tracking hit test history over multiple frames
    pub hover_manager: crate::managers::hover::HoverManager,
    /// IFrame manager for all nodes across all DOMs
    pub iframe_manager: IFrameManager,
    /// GPU state manager for all nodes across all DOMs
    pub gpu_state_manager: GpuStateManager,
    /// Accessibility manager for screen reader support
    pub a11y_manager: crate::managers::a11y::A11yManager,
    /// Timers associated with this window
    pub timers: BTreeMap<TimerId, Timer>,
    /// Threads running in the background for this window
    pub threads: BTreeMap<ThreadId, Thread>,
    /// Currently loaded fonts and images present in this renderer (window)
    pub renderer_resources: RendererResources,
    /// Renderer type: Hardware-with-software-fallback, pure software or pure hardware renderer?
    pub renderer_type: Option<RendererType>,
    /// Windows state of the window of (current frame - 1): initialized to None on startup
    pub previous_window_state: Option<FullWindowState>,
    /// Window state of this current window (current frame): initialized to the state of
    /// WindowCreateOptions
    pub current_window_state: FullWindowState,
    /// A "document" in WebRender usually corresponds to one tab (i.e. in Azuls case, the whole
    /// window).
    pub document_id: DocumentId,
    /// ID namespace under which every font / image for this window is registered
    pub id_namespace: IdNamespace,
    /// The "epoch" is a frame counter, to remove outdated images, fonts and OpenGL textures when
    /// they're not in use anymore.
    pub epoch: Epoch,
    /// Currently GL textures inside the active CachedDisplayList
    pub gl_texture_cache: GlTextureCache,
    /// State for tracking scrollbar drag interaction
    currently_dragging_thumb: Option<ScrollbarDragState>,
    /// Text input manager - centralizes all text editing logic
    pub text_input_manager: crate::managers::text_input::TextInputManager,
    /// Undo/Redo manager for text editing operations
    pub undo_redo_manager: crate::managers::undo_redo::UndoRedoManager,
    /// Cached text layout constraints for each node
    /// This allows us to re-layout text with the same constraints after edits
    text_constraints_cache: TextConstraintsCache,
    /// Tracks which nodes have been edited since last full layout.
    /// Key: (DomId, NodeId of IFC root)
    /// Value: The edited inline content that should be used for relayout
    dirty_text_nodes: BTreeMap<(DomId, NodeId), DirtyTextNode>,
    /// Pending IFrame updates from callbacks (processed in next frame)
    /// Map of DomId -> Set of NodeIds that need re-rendering
    pub pending_iframe_updates: BTreeMap<DomId, FastBTreeSet<NodeId>>,
    /// ICU4X localizer handle for internationalized formatting (numbers, dates, lists, plurals)
    /// Initialized from system language at startup, can be overridden
    #[cfg(feature = "icu")]
    pub icu_localizer: IcuLocalizerHandle,
}

fn default_duration_500ms() -> Duration {
    Duration::System(SystemTimeDiff::from_millis(500))
}

fn default_duration_200ms() -> Duration {
    Duration::System(SystemTimeDiff::from_millis(200))
}

/// Helper function to convert Duration to milliseconds
///
/// Duration is an enum with System (std::time::Duration) and Tick variants.
/// We need to handle both cases for proper time calculations.
fn duration_to_millis(duration: Duration) -> u64 {
    match duration {
        #[cfg(feature = "std")]
        Duration::System(system_diff) => {
            let std_duration: std::time::Duration = system_diff.into();
            std_duration.as_millis() as u64
        }
        #[cfg(not(feature = "std"))]
        Duration::System(system_diff) => {
            // Manual calculation: secs * 1000 + nanos / 1_000_000
            system_diff.secs * 1000 + (system_diff.nanos / 1_000_000) as u64
        }
        Duration::Tick(tick_diff) => {
            // Assume tick = 1ms for simplicity (platform-specific)
            tick_diff.tick_diff
        }
    }
}

impl LayoutWindow {
    /// Create a new layout window with empty caches.
    ///
    /// For full initialization with WindowInternal compatibility, use `new_full()`.
    pub fn new(fc_cache: FcFontCache) -> Result<Self, crate::solver3::LayoutError> {
        Ok(Self {
            // Default width, will be updated on first layout
            #[cfg(feature = "pdf")]
            fragmentation_context: crate::paged::FragmentationContext::new_continuous(800.0),
            layout_cache: Solver3LayoutCache {
                tree: None,
                calculated_positions: BTreeMap::new(),
                viewport: None,
                scroll_ids: BTreeMap::new(),
                scroll_id_to_node_id: BTreeMap::new(),
                counters: BTreeMap::new(),
                float_cache: BTreeMap::new(),
            },
            text_cache: TextLayoutCache::new(),
            font_manager: FontManager::new(fc_cache)?,
            image_cache: ImageCache::default(),
            layout_results: BTreeMap::new(),
            scroll_manager: ScrollManager::new(),
            gesture_drag_manager: crate::managers::gesture::GestureAndDragManager::new(),
            focus_manager: crate::managers::focus_cursor::FocusManager::new(),
            cursor_manager: crate::managers::cursor::CursorManager::new(),
            file_drop_manager: crate::managers::file_drop::FileDropManager::new(),
            selection_manager: crate::managers::selection::SelectionManager::new(),
            clipboard_manager: crate::managers::clipboard::ClipboardManager::new(),
            drag_drop_manager: crate::managers::drag_drop::DragDropManager::new(),
            hover_manager: crate::managers::hover::HoverManager::new(),
            iframe_manager: IFrameManager::new(),
            gpu_state_manager: GpuStateManager::new(
                default_duration_500ms(),
                default_duration_200ms(),
            ),
            a11y_manager: crate::managers::a11y::A11yManager::new(),
            timers: BTreeMap::new(),
            threads: BTreeMap::new(),
            renderer_resources: RendererResources::default(),
            renderer_type: None,
            previous_window_state: None,
            current_window_state: FullWindowState::default(),
            document_id: new_document_id(),
            id_namespace: new_id_namespace(),
            epoch: Epoch::new(),
            gl_texture_cache: GlTextureCache::default(),
            currently_dragging_thumb: None,
            text_input_manager: crate::managers::text_input::TextInputManager::new(),
            undo_redo_manager: crate::managers::undo_redo::UndoRedoManager::new(),
            text_constraints_cache: TextConstraintsCache {
                constraints: BTreeMap::new(),
            },
            dirty_text_nodes: BTreeMap::new(),
            pending_iframe_updates: BTreeMap::new(),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
        })
    }

    /// Create a new layout window for paged media (PDF generation).
    ///
    /// This constructor initializes the layout window with a paged fragmentation context,
    /// which will cause content to flow across multiple pages instead of a single continuous
    /// scrollable container.
    ///
    /// # Arguments
    /// - `fc_cache`: Font configuration cache for font loading
    /// - `page_size`: The logical size of each page
    ///
    /// # Returns
    /// A new `LayoutWindow` configured for paged output, or an error if initialization fails.
    #[cfg(feature = "pdf")]
    pub fn new_paged(
        fc_cache: FcFontCache,
        page_size: LogicalSize,
    ) -> Result<Self, crate::solver3::LayoutError> {
        Ok(Self {
            fragmentation_context: crate::paged::FragmentationContext::new_paged(page_size),
            layout_cache: Solver3LayoutCache {
                tree: None,
                calculated_positions: BTreeMap::new(),
                viewport: None,
                scroll_ids: BTreeMap::new(),
                scroll_id_to_node_id: BTreeMap::new(),
                counters: BTreeMap::new(),
                float_cache: BTreeMap::new(),
            },
            text_cache: TextLayoutCache::new(),
            font_manager: FontManager::new(fc_cache)?,
            image_cache: ImageCache::default(),
            layout_results: BTreeMap::new(),
            scroll_manager: ScrollManager::new(),
            gesture_drag_manager: crate::managers::gesture::GestureAndDragManager::new(),
            focus_manager: crate::managers::focus_cursor::FocusManager::new(),
            cursor_manager: crate::managers::cursor::CursorManager::new(),
            file_drop_manager: crate::managers::file_drop::FileDropManager::new(),
            selection_manager: crate::managers::selection::SelectionManager::new(),
            clipboard_manager: crate::managers::clipboard::ClipboardManager::new(),
            drag_drop_manager: crate::managers::drag_drop::DragDropManager::new(),
            hover_manager: crate::managers::hover::HoverManager::new(),
            iframe_manager: IFrameManager::new(),
            gpu_state_manager: GpuStateManager::new(
                default_duration_500ms(),
                default_duration_200ms(),
            ),
            a11y_manager: crate::managers::a11y::A11yManager::new(),
            timers: BTreeMap::new(),
            threads: BTreeMap::new(),
            renderer_resources: RendererResources::default(),
            renderer_type: None,
            previous_window_state: None,
            current_window_state: FullWindowState::default(),
            document_id: new_document_id(),
            id_namespace: new_id_namespace(),
            epoch: Epoch::new(),
            gl_texture_cache: GlTextureCache::default(),
            currently_dragging_thumb: None,
            text_input_manager: crate::managers::text_input::TextInputManager::new(),
            undo_redo_manager: crate::managers::undo_redo::UndoRedoManager::new(),
            text_constraints_cache: TextConstraintsCache {
                constraints: BTreeMap::new(),
            },
            dirty_text_nodes: BTreeMap::new(),
            pending_iframe_updates: BTreeMap::new(),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
        })
    }

    /// Perform layout on a styled DOM and generate a display list.
    ///
    /// This is the main entry point for layout. It handles:
    /// - Incremental layout updates using the cached layout tree
    /// - Text shaping and line breaking
    /// - IFrame callback invocation and recursive layout
    /// - Display list generation for rendering
    /// - Accessibility tree synchronization
    ///
    /// # Arguments
    /// - `styled_dom`: The styled DOM to layout
    /// - `window_state`: Current window dimensions and state
    /// - `renderer_resources`: Resources for image sizing etc.
    /// - `debug_messages`: Optional vector to collect debug/warning messages
    ///
    /// # Returns
    /// The display list ready for rendering, or an error if layout fails.
    pub fn layout_and_generate_display_list(
        &mut self,
        root_dom: StyledDom,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<(), solver3::LayoutError> {
        // Clear previous results for a full relayout
        self.layout_results.clear();

        if let Some(msgs) = debug_messages.as_mut() {
            msgs.push(LayoutDebugMessage::info(format!(
                "[layout_and_generate_display_list] Starting layout for DOM with {} nodes",
                root_dom.node_data.len()
            )));
        }

        // Start recursive layout from the root DOM
        let result = self.layout_dom_recursive(
            root_dom,
            window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        );

        if let Err(ref e) = result {
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::error(format!(
                    "[layout_and_generate_display_list] Layout FAILED: {:?}",
                    e
                )));
            }
            eprintln!("[layout_and_generate_display_list] Layout FAILED: {:?}", e);
        } else {
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[layout_and_generate_display_list] Layout SUCCESS, layout_results count: {}",
                    self.layout_results.len()
                )));
            }
        }

        // After successful layout, update the accessibility tree
        // Note: This is wrapped in catch_unwind to prevent a11y issues from crashing the app
        #[cfg(feature = "a11y")]
        if result.is_ok() {
            // Use catch_unwind to prevent a11y panics from crashing the main application
            let a11y_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::managers::a11y::A11yManager::update_tree(
                    self.a11y_manager.root_id,
                    &self.layout_results,
                    &self.current_window_state.title,
                    self.current_window_state.size.dimensions,
                )
            }));

            match a11y_result {
                Ok(tree_update) => {
                    // Store the tree_update for platform adapter to consume
                    self.a11y_manager.last_tree_update = Some(tree_update);
                }
                Err(_) => {
                    // A11y update failed - log and continue without a11y
                }
            }
        }

        // After layout, automatically scroll cursor into view if there's a focused text input
        if result.is_ok() {
            self.scroll_focused_cursor_into_view();
        }

        result
    }

    fn layout_dom_recursive(
        &mut self,
        mut styled_dom: StyledDom,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<(), solver3::LayoutError> {
        if styled_dom.dom_id.inner == 0 {
            styled_dom.dom_id = DomId::ROOT_ID;
        }
        let dom_id = styled_dom.dom_id;

        let viewport = LogicalRect {
            origin: LogicalPosition::zero(),
            size: window_state.size.dimensions,
        };

        // Font Resolution And Loading
        // This must happen BEFORE layout_document() is called
        {
            use crate::{
                solver3::getters::{
                    collect_and_resolve_font_chains, collect_font_ids_from_chains,
                    compute_fonts_to_load, load_fonts_from_disk, register_embedded_fonts_from_styled_dom,
                },
                text3::default::PathLoader,
            };

            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(
                    "[FontLoading] Starting font resolution for DOM".to_string(),
                ));
            }

            // Step 0: Register embedded FontRefs (e.g. Material Icons)
            // These fonts bypass fontconfig and are used directly
            register_embedded_fonts_from_styled_dom(&styled_dom, &self.font_manager);

            // Step 1: Resolve font chains (cached by FontChainKey)
            let chains = collect_and_resolve_font_chains(&styled_dom, &self.font_manager.fc_cache);
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[FontLoading] Resolved {} font chains",
                    chains.len()
                )));
            }

            // Step 2: Get required font IDs from chains
            let required_fonts = collect_font_ids_from_chains(&chains);
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[FontLoading] Required fonts: {} unique fonts",
                    required_fonts.len()
                )));
            }

            // Step 3: Compute which fonts need to be loaded (diff with already loaded)
            let already_loaded = self.font_manager.get_loaded_font_ids();
            let fonts_to_load = compute_fonts_to_load(&required_fonts, &already_loaded);
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[FontLoading] Already loaded: {}, need to load: {}",
                    already_loaded.len(),
                    fonts_to_load.len()
                )));
            }

            // Step 4: Load missing fonts
            if !fonts_to_load.is_empty() {
                if let Some(msgs) = debug_messages.as_mut() {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[FontLoading] Loading {} fonts from disk...",
                        fonts_to_load.len()
                    )));
                }
                let loader = PathLoader::new();
                let load_result = load_fonts_from_disk(
                    &fonts_to_load,
                    &self.font_manager.fc_cache,
                    |bytes, index| loader.load_font(bytes, index),
                );

                if let Some(msgs) = debug_messages.as_mut() {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[FontLoading] Loaded {} fonts, {} failed",
                        load_result.loaded.len(),
                        load_result.failed.len()
                    )));
                }

                // Insert loaded fonts into the font manager
                self.font_manager.insert_fonts(load_result.loaded);

                // Log any failures
                for (font_id, error) in &load_result.failed {
                    if let Some(msgs) = debug_messages.as_mut() {
                        msgs.push(LayoutDebugMessage::warning(format!(
                            "[FontLoading] Failed to load font {:?}: {}",
                            font_id, error
                        )));
                    }
                }
            }

            // Step 5: Update font chain cache
            self.font_manager.set_font_chain_cache(chains.into_fontconfig_chains());
        }

        let scroll_offsets = self.scroll_manager.get_scroll_states_for_dom(dom_id);
        let styled_dom_clone = styled_dom.clone();
        let gpu_cache = self.gpu_state_manager.get_or_create_cache(dom_id).clone();
        
        // Get cursor visibility from cursor manager for display list generation
        let cursor_is_visible = self.cursor_manager.should_draw_cursor();
        
        // Get cursor location from cursor manager for independent cursor rendering
        let cursor_location = self.cursor_manager.get_cursor_location().and_then(|loc| {
            self.cursor_manager.get_cursor().map(|cursor| {
                (loc.dom_id, loc.node_id, cursor.clone())
            })
        });

        let mut display_list = solver3::layout_document(
            &mut self.layout_cache,
            &mut self.text_cache,
            styled_dom,
            viewport,
            &self.font_manager,
            &scroll_offsets,
            &self.selection_manager.selections,
            &self.selection_manager.text_selections,
            debug_messages,
            Some(&gpu_cache),
            &self.renderer_resources,
            self.id_namespace,
            dom_id,
            cursor_is_visible,
            cursor_location,
        )?;

        let tree = self
            .layout_cache
            .tree
            .clone()
            .ok_or(solver3::LayoutError::InvalidTree)?;

        // Get scroll IDs from cache (they were computed during layout_document)
        let scroll_ids = self.layout_cache.scroll_ids.clone();
        let scroll_id_to_node_id = self.layout_cache.scroll_id_to_node_id.clone();

        // Synchronize scrollbar transforms AFTER layout
        self.gpu_state_manager
            .update_scrollbar_transforms(dom_id, &self.scroll_manager, &tree);

        // Scan for IFrames *after* the initial layout pass
        let iframes = self.scan_for_iframes(dom_id, &tree, &self.layout_cache.calculated_positions);

        for (node_id, bounds) in iframes {
            if let Some(child_dom_id) = self.invoke_iframe_callback(
                dom_id,
                node_id,
                bounds,
                window_state,
                renderer_resources,
                system_callbacks,
                debug_messages,
            ) {
                // Insert an IFrame primitive that the renderer will use
                display_list
                    .items
                    .push(crate::solver3::display_list::DisplayListItem::IFrame {
                        child_dom_id,
                        bounds,
                        clip_rect: bounds,
                    });
            }
        }

        // Store the final layout result for this DOM
        self.layout_results.insert(
            dom_id,
            DomLayoutResult {
                styled_dom: styled_dom_clone,
                layout_tree: tree,
                calculated_positions: self.layout_cache.calculated_positions.clone(),
                viewport,
                display_list,
                scroll_ids,
                scroll_id_to_node_id,
            },
        );

        Ok(())
    }

    fn scan_for_iframes(
        &self,
        dom_id: DomId,
        layout_tree: &LayoutTree,
        calculated_positions: &BTreeMap<usize, LogicalPosition>,
    ) -> Vec<(NodeId, LogicalRect)> {
        layout_tree
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(idx, node)| {
                let node_dom_id = node.dom_node_id?;
                let layout_result = self.layout_results.get(&dom_id)?;
                let node_data = &layout_result.styled_dom.node_data.as_container()[node_dom_id];
                if matches!(node_data.get_node_type(), NodeType::IFrame(_)) {
                    let pos = calculated_positions.get(&idx).copied().unwrap_or_default();
                    let size = node.used_size.unwrap_or_default();
                    Some((node_dom_id, LogicalRect::new(pos, size)))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Handle a window resize by updating the cached layout.
    ///
    /// This method leverages solver3's incremental layout system to efficiently
    /// relayout only the affected parts of the tree when the window size changes.
    ///
    /// Returns the new display list after the resize.
    pub fn resize_window(
        &mut self,
        styled_dom: StyledDom,
        new_size: LogicalSize,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<DisplayList, crate::solver3::LayoutError> {
        // Create a temporary FullWindowState with the new size
        let mut window_state = FullWindowState::default();
        window_state.size.dimensions = new_size;

        let dom_id = styled_dom.dom_id;

        // Reuse the main layout method - solver3 will detect the viewport
        // change and invalidate only what's necessary
        self.layout_and_generate_display_list(
            styled_dom,
            &window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        )?;

        // Retrieve the display list from the layout result
        // We need to take ownership of the display list, so we replace it with an empty one
        self.layout_results
            .get_mut(&dom_id)
            .map(|result| std::mem::replace(&mut result.display_list, DisplayList::default()))
            .ok_or(solver3::LayoutError::InvalidTree)
    }

    /// Clear all caches (useful for testing or when switching documents).
    pub fn clear_caches(&mut self) {
        self.layout_cache = Solver3LayoutCache {
            tree: None,
            calculated_positions: BTreeMap::new(),
            viewport: None,
            scroll_ids: BTreeMap::new(),
            scroll_id_to_node_id: BTreeMap::new(),
            counters: BTreeMap::new(),
            float_cache: BTreeMap::new(),
        };
        self.text_cache = TextLayoutCache::new();
        self.layout_results.clear();
        self.scroll_manager = ScrollManager::new();
        self.selection_manager.clear_all();
    }

    /// Set scroll position for a node
    pub fn set_scroll_position(&mut self, dom_id: DomId, node_id: NodeId, scroll: ScrollPosition) {
        // Convert ScrollPosition to the internal representation
        #[cfg(feature = "std")]
        let now = Instant::System(std::time::Instant::now().into());
        #[cfg(not(feature = "std"))]
        let now = Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 });

        self.scroll_manager.update_node_bounds(
            dom_id,
            node_id,
            scroll.parent_rect,
            scroll.children_rect,
            now.clone(),
        );
        self.scroll_manager
            .set_scroll_position(dom_id, node_id, scroll.children_rect.origin, now);
    }

    /// Get scroll position for a node
    pub fn get_scroll_position(&self, dom_id: DomId, node_id: NodeId) -> Option<ScrollPosition> {
        let states = self.scroll_manager.get_scroll_states_for_dom(dom_id);
        states.get(&node_id).cloned()
    }

    /// Set selection state for a DOM
    pub fn set_selection(&mut self, dom_id: DomId, selection: SelectionState) {
        self.selection_manager.set_selection(dom_id, selection);
    }

    /// Get selection state for a DOM
    pub fn get_selection(&self, dom_id: DomId) -> Option<&SelectionState> {
        self.selection_manager.get_selection(&dom_id)
    }

    /// Invoke an IFrame callback and perform layout on the returned DOM.
    ///
    /// This is the entry point that looks up the necessary `IFrameNode` data before
    /// delegating to the core implementation logic.
    fn invoke_iframe_callback(
        &mut self,
        parent_dom_id: DomId,
        node_id: NodeId,
        bounds: LogicalRect,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Option<DomId> {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "invoke_iframe_callback called for node {:?}",
                node_id
            )));
        }

        // Get the layout result for the parent DOM to access its styled_dom
        let layout_result = self.layout_results.get(&parent_dom_id)?;
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "Got layout result for parent DOM {:?}",
                parent_dom_id
            )));
        }

        // Get the node data for the IFrame element
        let node_data_container = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_container.get(node_id)?;
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "Got node data at index {}",
                node_id.index()
            )));
        }

        // Extract the IFrame node, cloning it to avoid borrow checker issues
        let iframe_node = match node_data.get_node_type() {
            NodeType::IFrame(iframe) => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info("Node is IFrame type".to_string()));
                }
                iframe.clone()
            }
            other => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "Node is NOT IFrame, type = {:?}",
                        other
                    )));
                }
                return None;
            }
        };

        // Call the actual implementation with all necessary data
        self.invoke_iframe_callback_impl(
            parent_dom_id,
            node_id,
            &iframe_node,
            bounds,
            window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        )
    }

    /// Core implementation for invoking an IFrame callback and managing the recursive layout.
    ///
    /// This method implements the 5 conditional re-invocation rules by coordinating
    /// with the `IFrameManager` and `ScrollManager`.
    ///
    /// # Returns
    ///
    /// `Some(child_dom_id)` if the callback was invoked and the child DOM was laid out.
    /// The parent's display list generator will then use this ID to reference the child's
    /// display list. Returns `None` if the callback was not invoked.
    fn invoke_iframe_callback_impl(
        &mut self,
        parent_dom_id: DomId,
        node_id: NodeId,
        iframe_node: &azul_core::dom::IFrameNode,
        bounds: LogicalRect,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Option<DomId> {
        // Get current time from system callbacks for state updates
        let now = (system_callbacks.get_system_time_fn.cb)();

        // Update node bounds in the scroll manager. This is necessary for the IFrameManager
        // to correctly detect edge scroll conditions.
        self.scroll_manager.update_node_bounds(
            parent_dom_id,
            node_id,
            bounds,
            LogicalRect::new(LogicalPosition::zero(), bounds.size), // Initial content_rect
            now.clone(),
        );

        // Check with the IFrameManager to see if re-invocation is necessary.
        // It handles all 5 conditional rules.
        let reason = match self.iframe_manager.check_reinvoke(
            parent_dom_id,
            node_id,
            &self.scroll_manager,
            bounds,
        ) {
            Some(r) => r,
            None => {
                // No re-invocation needed, but we still need the child_dom_id for the display list.
                return self
                    .iframe_manager
                    .get_nested_dom_id(parent_dom_id, node_id);
            }
        };

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "IFrame ({:?}, {:?}) - Reason: {:?}",
                parent_dom_id, node_id, reason
            )));
        }

        let scroll_offset = self
            .scroll_manager
            .get_current_offset(parent_dom_id, node_id)
            .unwrap_or_default();
        let hidpi_factor = window_state.size.get_hidpi_factor();

        // Create IFrameCallbackInfo with the most up-to-date state
        let mut callback_info = azul_core::callbacks::IFrameCallbackInfo::new(
            reason,
            &*self.font_manager.fc_cache,
            &self.image_cache,
            window_state.theme,
            azul_core::callbacks::HidpiAdjustedBounds {
                logical_size: bounds.size,
                hidpi_factor,
            },
            bounds.size,
            scroll_offset,
            bounds.size,
            LogicalPosition::zero(),
        );

        // Clone the user data for the callback
        let callback_data = iframe_node.refany.clone();

        // Invoke the user's IFrame callback
        let callback_return = (iframe_node.callback.cb)(callback_data, callback_info);

        // Mark the IFrame as invoked to prevent duplicate InitialRender calls
        self.iframe_manager
            .mark_invoked(parent_dom_id, node_id, reason);

        // Get the child StyledDom from the callback's return value
        let mut child_styled_dom = match callback_return.dom {
            azul_core::styled_dom::OptionStyledDom::Some(dom) => dom,
            azul_core::styled_dom::OptionStyledDom::None => {
                // If the callback returns None, it's an optimization hint.
                if reason == IFrameCallbackReason::InitialRender {
                    // For the very first render, create an empty div as a fallback.
                    let mut empty_dom = Dom::create_div();
                    let empty_css = Css::empty();
                    empty_dom.style(empty_css)
                } else {
                    // For subsequent calls, returning None means "keep the old DOM".
                    // We just need to update the scroll info and return the existing child ID.
                    self.iframe_manager.update_iframe_info(
                        parent_dom_id,
                        node_id,
                        callback_return.scroll_size,
                        callback_return.virtual_scroll_size,
                    );
                    return self
                        .iframe_manager
                        .get_nested_dom_id(parent_dom_id, node_id);
                }
            }
        };

        // Get or create a unique DomId for the IFrame's content
        let child_dom_id = self
            .iframe_manager
            .get_or_create_nested_dom_id(parent_dom_id, node_id);
        child_styled_dom.dom_id = child_dom_id;

        // Update the IFrameManager with the new scroll sizes from the callback
        self.iframe_manager.update_iframe_info(
            parent_dom_id,
            node_id,
            callback_return.scroll_size,
            callback_return.virtual_scroll_size,
        );

        // **RECURSIVE LAYOUT STEP**
        // Perform a full layout pass on the child DOM. This will recursively handle
        // any IFrames within this IFrame.
        self.layout_dom_recursive(
            child_styled_dom,
            window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        )
        .ok()?;

        Some(child_dom_id)
    }

    // Query methods for callbacks

    /// Get the size of a laid-out node
    pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        // Use dom_to_layout mapping since layout tree indices differ from DOM indices
        let layout_indices = layout_result.layout_tree.dom_to_layout.get(&nid)?;
        let layout_index = *layout_indices.first()?;
        let layout_node = layout_result.layout_tree.get(layout_index)?;
        layout_node.used_size
    }

    /// Get the position of a laid-out node
    pub fn get_node_position(&self, node_id: DomNodeId) -> Option<LogicalPosition> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        // Use dom_to_layout mapping since layout tree indices differ from DOM indices
        let layout_indices = layout_result.layout_tree.dom_to_layout.get(&nid)?;
        let layout_index = *layout_indices.first()?;
        let position = layout_result.calculated_positions.get(&layout_index)?;
        Some(*position)
    }

    /// Get the hit test bounds of a node from the display list
    ///
    /// This is more reliable than get_node_position + get_node_size because
    /// the display list always contains the correct final rendered positions,
    /// including for nodes that may not have entries in calculated_positions.
    pub fn get_node_hit_test_bounds(&self, node_id: DomNodeId) -> Option<LogicalRect> {
        use crate::solver3::display_list::DisplayListItem;

        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;

        // Get the actual tag_id from styled_nodes (matches what get_tag_id in display_list.rs uses)
        let styled_nodes = layout_result.styled_dom.styled_nodes.as_container();
        let tag_id = styled_nodes.get(nid)?.tag_id.into_option()?.inner;

        // Search the display list for a HitTestArea with matching tag
        // Note: tag is now (u64, u16) tuple where tag.0 is the TagId.inner
        for item in &layout_result.display_list.items {
            if let DisplayListItem::HitTestArea { bounds, tag } = item {
                if tag.0 == tag_id && bounds.size.width > 0.0 && bounds.size.height > 0.0 {
                    return Some(*bounds);
                }
            }
        }
        None
    }

    /// Get the parent of a node
    pub fn get_parent(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let parent_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .parent_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(parent_id)),
        })
    }

    /// Get the first child of a node
    pub fn get_first_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hierarchy_item = node_hierarchy.get(nid)?;
        let first_child_id = hierarchy_item.first_child_id(nid)?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(first_child_id)),
        })
    }

    /// Get the next sibling of a node
    pub fn get_next_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let next_sibling_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .next_sibling_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(next_sibling_id)),
        })
    }

    /// Get the previous sibling of a node
    pub fn get_previous_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let prev_sibling_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .previous_sibling_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(prev_sibling_id)),
        })
    }

    /// Get the last child of a node
    pub fn get_last_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let last_child_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .last_child_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(last_child_id)),
        })
    }

    /// Scan all fonts used in this LayoutWindow (for resource GC)
    pub fn scan_used_fonts(&self) -> BTreeSet<FontKey> {
        let mut fonts = BTreeSet::new();
        for (_dom_id, layout_result) in &self.layout_results {
            // TODO: Scan styled_dom for font references
            // This requires accessing the CSS property cache and finding all font-family properties
        }
        fonts
    }

    /// Scan all images used in this LayoutWindow (for resource GC)
    pub fn scan_used_images(&self, _css_image_cache: &ImageCache) -> BTreeSet<ImageRefHash> {
        let mut images = BTreeSet::new();
        for (_dom_id, layout_result) in &self.layout_results {
            // TODO: Scan styled_dom for image references
            // This requires scanning background-image and content properties
        }
        images
    }

    /// Helper function to convert ScrollStates to nested format for CallbackInfo
    fn get_nested_scroll_states(
        &self,
        dom_id: DomId,
    ) -> BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> {
        let mut nested = BTreeMap::new();
        let scroll_states = self.scroll_manager.get_scroll_states_for_dom(dom_id);
        let mut inner = BTreeMap::new();
        for (node_id, scroll_pos) in scroll_states {
            inner.insert(
                NodeHierarchyItemId::from_crate_internal(Some(node_id)),
                scroll_pos,
            );
        }
        nested.insert(dom_id, inner);
        nested
    }

    // Scroll Into View
    
    /// Scroll a DOM node into view
    ///
    /// This is the main API for scrolling elements into view. It handles:
    /// - Finding scroll ancestors
    /// - Calculating scroll deltas
    /// - Applying scroll animations
    ///
    /// # Arguments
    ///
    /// * `node_id` - The DOM node to scroll into view
    /// * `options` - Scroll alignment and animation options
    /// * `now` - Current timestamp for animations
    ///
    /// # Returns
    ///
    /// A vector of scroll adjustments that were applied
    pub fn scroll_node_into_view(
        &mut self,
        node_id: DomNodeId,
        options: crate::managers::scroll_into_view::ScrollIntoViewOptions,
        now: azul_core::task::Instant,
    ) -> Vec<crate::managers::scroll_into_view::ScrollAdjustment> {
        crate::managers::scroll_into_view::scroll_node_into_view(
            node_id,
            &self.layout_results,
            &mut self.scroll_manager,
            options,
            now,
        )
    }
    
    /// Scroll a text cursor into view
    ///
    /// Used when the cursor moves within a contenteditable element.
    /// The cursor rect should be in node-local coordinates.
    pub fn scroll_cursor_into_view(
        &mut self,
        cursor_rect: LogicalRect,
        node_id: DomNodeId,
        options: crate::managers::scroll_into_view::ScrollIntoViewOptions,
        now: azul_core::task::Instant,
    ) -> Vec<crate::managers::scroll_into_view::ScrollAdjustment> {
        crate::managers::scroll_into_view::scroll_cursor_into_view(
            cursor_rect,
            node_id,
            &self.layout_results,
            &mut self.scroll_manager,
            options,
            now,
        )
    }

    // Timer Management

    /// Add a timer to this window
    pub fn add_timer(&mut self, timer_id: TimerId, timer: Timer) {
        self.timers.insert(timer_id, timer);
    }

    /// Remove a timer from this window
    pub fn remove_timer(&mut self, timer_id: &TimerId) -> Option<Timer> {
        self.timers.remove(timer_id)
    }

    /// Get a reference to a timer
    pub fn get_timer(&self, timer_id: &TimerId) -> Option<&Timer> {
        self.timers.get(timer_id)
    }

    /// Get a mutable reference to a timer
    pub fn get_timer_mut(&mut self, timer_id: &TimerId) -> Option<&mut Timer> {
        self.timers.get_mut(timer_id)
    }

    /// Get all timer IDs
    pub fn get_timer_ids(&self) -> TimerIdVec {
        self.timers.keys().copied().collect::<Vec<_>>().into()
    }

    /// Tick all timers (called once per frame)
    /// Returns a list of timer IDs that are ready to run
    pub fn tick_timers(&mut self, current_time: azul_core::task::Instant) -> Vec<TimerId> {
        let mut ready_timers = Vec::new();

        for (timer_id, timer) in &mut self.timers {
            // Check if timer is ready to run
            // This logic should match the timer's internal state
            // For now, we'll just collect all timer IDs
            // The actual readiness check will be done when invoking
            ready_timers.push(*timer_id);
        }

        ready_timers
    }

    /// Calculate milliseconds until the next timer needs to fire.
    ///
    /// Returns `None` if there are no timers, meaning the caller can block indefinitely.
    /// Returns `Some(0)` if a timer is already overdue.
    /// Otherwise returns the minimum time in milliseconds until any timer fires.
    ///
    /// This is used by Linux (X11/Wayland) to set an efficient poll/select timeout
    /// instead of always polling every 16ms.
    pub fn time_until_next_timer_ms(
        &self,
        get_system_time_fn: &azul_core::task::GetSystemTimeCallback,
    ) -> Option<u64> {
        if self.timers.is_empty() {
            return None; // No timers - can block indefinitely
        }

        let now = (get_system_time_fn.cb)();
        let mut min_ms: Option<u64> = None;

        for timer in self.timers.values() {
            let next_run = timer.instant_of_next_run();

            // Calculate time difference in milliseconds
            let ms_until = if next_run < now {
                0 // Timer is overdue
            } else {
                duration_to_millis(next_run.duration_since(&now))
            };

            min_ms = Some(match min_ms {
                Some(current_min) => current_min.min(ms_until),
                None => ms_until,
            });
        }

        min_ms
    }

    // Thread Management

    /// Add a thread to this window
    pub fn add_thread(&mut self, thread_id: ThreadId, thread: Thread) {
        self.threads.insert(thread_id, thread);
    }

    /// Remove a thread from this window
    pub fn remove_thread(&mut self, thread_id: &ThreadId) -> Option<Thread> {
        self.threads.remove(thread_id)
    }

    /// Get a reference to a thread
    pub fn get_thread(&self, thread_id: &ThreadId) -> Option<&Thread> {
        self.threads.get(thread_id)
    }

    /// Get a mutable reference to a thread
    pub fn get_thread_mut(&mut self, thread_id: &ThreadId) -> Option<&mut Thread> {
        self.threads.get_mut(thread_id)
    }

    /// Get all thread IDs
    pub fn get_thread_ids(&self) -> ThreadIdVec {
        self.threads.keys().copied().collect::<Vec<_>>().into()
    }
    
    // Cursor Blinking Timer
    
    /// Create the cursor blink timer
    ///
    /// This timer toggles cursor visibility at ~530ms intervals.
    /// It checks if enough time has passed since the last user input before blinking,
    /// to avoid blinking while the user is actively typing.
    pub fn create_cursor_blink_timer(&self, _window_state: &FullWindowState) -> crate::timer::Timer {
        use azul_core::task::{Duration, SystemTimeDiff};
        use crate::timer::{Timer, TimerCallback};
        use azul_core::refany::RefAny;
        
        let interval_ms = crate::managers::cursor::CURSOR_BLINK_INTERVAL_MS;
        
        // Create a RefAny with a unit type - the timer callback doesn't need any data
        // The actual cursor state is in LayoutWindow.cursor_manager
        let refany = RefAny::new(());
        
        Timer {
            refany,
            node_id: None.into(),
            created: azul_core::task::Instant::now(),
            run_count: 0,
            last_run: azul_core::task::OptionInstant::None,
            delay: azul_core::task::OptionDuration::None,
            interval: azul_core::task::OptionDuration::Some(Duration::System(SystemTimeDiff::from_millis(interval_ms))),
            timeout: azul_core::task::OptionDuration::None,
            callback: TimerCallback::create(cursor_blink_timer_callback),
        }
    }
    
    /// Scroll the active text cursor into view within its scrollable container
    ///
    /// This finds the focused contenteditable node, gets the cursor rectangle,
    /// and scrolls any scrollable ancestor to ensure the cursor is visible.
    pub fn scroll_active_cursor_into_view(&mut self, result: &mut CallbackChangeResult) {
        use crate::managers::scroll_into_view;
        
        // Get the focused node
        let focused_node = match self.focus_manager.get_focused_node() {
            Some(node) => *node,
            None => return,
        };
        
        let Some(node_id_internal) = focused_node.node.into_crate_internal() else {
            return;
        };
        
        // Check if node is contenteditable
        if !self.is_node_contenteditable_internal(focused_node.dom, node_id_internal) {
            return;
        }
        
        // Get the cursor location
        let cursor_location = match self.cursor_manager.get_cursor_location() {
            Some(loc) if loc.dom_id == focused_node.dom && loc.node_id == node_id_internal => loc,
            _ => return,
        };
        
        // Get the cursor position
        let cursor = match self.cursor_manager.get_cursor() {
            Some(c) => c.clone(),
            None => return,
        };
        
        // Get the inline layout to find the cursor rectangle
        let layout = match self.get_inline_layout_for_node(focused_node.dom, node_id_internal) {
            Some(l) => l,
            None => return,
        };
        
        // Get cursor rectangle (node-local coordinates)
        let cursor_rect = match layout.get_cursor_rect(&cursor) {
            Some(r) => r,
            None => return,
        };
        
        // Use scroll_into_view to scroll the cursor rect into view
        let now = azul_core::task::Instant::now();
        let options = scroll_into_view::ScrollIntoViewOptions::nearest();
        
        // Calculate scroll adjustments
        let adjustments = scroll_into_view::scroll_rect_into_view(
            cursor_rect,
            focused_node.dom,
            node_id_internal,
            &self.layout_results,
            &mut self.scroll_manager,
            options,
            now,
        );
        
        // Record the scroll changes
        for adj in adjustments {
            let current_pos = self.scroll_manager
                .get_current_offset(adj.scroll_container_dom_id, adj.scroll_container_node_id)
                .unwrap_or(LogicalPosition::zero());
            
            let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(adj.scroll_container_node_id));
            result
                .nodes_scrolled
                .entry(adj.scroll_container_dom_id)
                .or_insert_with(BTreeMap::new)
                .insert(hierarchy_id, current_pos);
        }
    }
    
    /// Check if a node is contenteditable (internal version using NodeId)
    fn is_node_contenteditable_internal(&self, dom_id: DomId, node_id: NodeId) -> bool {
        use crate::solver3::getters::is_node_contenteditable;
        
        let Some(layout_result) = self.layout_results.get(&dom_id) else {
            return false;
        };
        
        is_node_contenteditable(&layout_result.styled_dom, node_id)
    }
    
    /// Check if a node is contenteditable with W3C-conformant inheritance.
    ///
    /// This traverses up the DOM tree to check if the node or any ancestor
    /// has `contenteditable="true"` set, respecting `contenteditable="false"`
    /// to stop inheritance.
    fn is_node_contenteditable_inherited_internal(&self, dom_id: DomId, node_id: NodeId) -> bool {
        use crate::solver3::getters::is_node_contenteditable_inherited;
        
        let Some(layout_result) = self.layout_results.get(&dom_id) else {
            return false;
        };
        
        is_node_contenteditable_inherited(&layout_result.styled_dom, node_id)
    }
    
    /// Handle focus change for cursor blink timer management (W3C "flag and defer" pattern)
    ///
    /// This method implements the W3C focus/selection model:
    /// 1. Focus change is handled immediately (timer start/stop)
    /// 2. Cursor initialization is DEFERRED until after layout (via flag)
    ///
    /// The cursor is NOT initialized here because text layout may not be available
    /// during focus event handling. Instead, we set a flag that is consumed by
    /// `finalize_pending_focus_changes()` after the layout pass.
    ///
    /// # Parameters
    ///
    /// * `new_focus` - The newly focused node (None if focus is being cleared)
    /// * `current_window_state` - Current window state for timer creation
    ///
    /// # Returns
    ///
    /// A `CursorBlinkTimerAction` indicating what timer action the platform
    /// layer should take.
    pub fn handle_focus_change_for_cursor_blink(
        &mut self,
        new_focus: Option<azul_core::dom::DomNodeId>,
        current_window_state: &FullWindowState,
    ) -> CursorBlinkTimerAction {
        // Check if the new focus is on a contenteditable element
        // Use the inherited check for W3C conformance
        let contenteditable_info = match new_focus {
            Some(focus_node) => {
                if let Some(node_id) = focus_node.node.into_crate_internal() {
                    // Check if this node or any ancestor is contenteditable
                    if self.is_node_contenteditable_inherited_internal(focus_node.dom, node_id) {
                        // Find the text node where the cursor should be placed
                        let text_node_id = self.find_last_text_child(focus_node.dom, node_id)
                            .unwrap_or(node_id);
                        Some((focus_node.dom, node_id, text_node_id))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            None => None,
        };
        
        // Determine the action based on current state and new focus
        let timer_was_active = self.cursor_manager.is_blink_timer_active();
        
        if let Some((dom_id, container_node_id, text_node_id)) = contenteditable_info {
            
            // W3C "flag and defer" pattern:
            // Set flag for cursor initialization AFTER layout pass
            self.focus_manager.set_pending_contenteditable_focus(
                dom_id,
                container_node_id,
                text_node_id,
            );
            
            // Make cursor visible and record current time (even before actual initialization)
            let now = azul_core::task::Instant::now();
            self.cursor_manager.reset_blink_on_input(now);
            self.cursor_manager.set_blink_timer_active(true);
            
            if !timer_was_active {
                // Need to start the timer
                let timer = self.create_cursor_blink_timer(current_window_state);
                return CursorBlinkTimerAction::Start(timer);
            } else {
                // Timer already active, just continue
                return CursorBlinkTimerAction::NoChange;
            }
        } else {
            // Focus is moving away from contenteditable or being cleared
            
            // Clear the cursor AND the pending focus flag
            self.cursor_manager.clear();
            self.focus_manager.clear_pending_contenteditable_focus();
            
            if timer_was_active {
                // Need to stop the timer
                self.cursor_manager.set_blink_timer_active(false);
                return CursorBlinkTimerAction::Stop;
            } else {
                return CursorBlinkTimerAction::NoChange;
            }
        }
    }
    
    /// Finalize pending focus changes after layout pass (W3C "flag and defer" pattern)
    ///
    /// This method should be called AFTER the layout pass completes. It checks if
    /// there's a pending contenteditable focus and initializes the cursor now that
    /// text layout information is available.
    ///
    /// # W3C Conformance
    ///
    /// In the W3C model:
    /// 1. Focus event fires during event handling (layout may not be ready)
    /// 2. Selection/cursor placement happens after layout is computed
    /// 3. The cursor is drawn at the position specified by the Selection
    ///
    /// This function implements step 2+3 by:
    /// - Checking the `cursor_needs_initialization` flag
    /// - Getting the (now available) text layout
    /// - Initializing the cursor at the correct position
    ///
    /// # Returns
    ///
    /// `true` if cursor was initialized, `false` if no pending focus or initialization failed.
    pub fn finalize_pending_focus_changes(&mut self) -> bool {
        // Take the pending focus info (this clears the flag)
        let pending = match self.focus_manager.take_pending_contenteditable_focus() {
            Some(p) => p,
            None => return false,
        };
        
        // Now we can safely get the text layout (layout pass has completed)
        let text_layout = self.get_inline_layout_for_node(pending.dom_id, pending.text_node_id).cloned();
        
        // Initialize cursor at end of text
        self.cursor_manager.initialize_cursor_at_end(
            pending.dom_id,
            pending.text_node_id,
            text_layout.as_ref(),
        )
    }

    // CallbackChange Processing

    /// Apply callback changes that were collected during callback execution
    ///
    /// This method processes all changes accumulated in the CallbackChange vector
    /// and applies them to the appropriate state. This is called after a callback
    /// returns to ensure atomic application of all changes.
    ///
    /// Returns a `CallbackChangeResult` containing all the changes to be applied.
    pub fn apply_callback_changes(
        &mut self,
        changes: Vec<crate::callbacks::CallbackChange>,
        current_window_state: &FullWindowState,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
    ) -> CallbackChangeResult {
        use crate::callbacks::CallbackChange;

        let mut result = CallbackChangeResult {
            modified_window_state: current_window_state.clone(),
            ..Default::default()
        };
        for change in changes {
            match change {
                CallbackChange::ModifyWindowState { state } => {
                    result.modified_window_state = state;
                }
                CallbackChange::QueueWindowStateSequence { states } => {
                    // Queue the states to be processed in sequence.
                    // The first state is applied immediately, subsequent states
                    // are stored for processing in future frames.
                    result.queued_window_states.extend(states);
                }
                CallbackChange::CreateNewWindow { options } => {
                    result.windows_created.push(options);
                }
                CallbackChange::CloseWindow => {
                    // Set the close_requested flag to trigger window close
                    result.modified_window_state.flags.close_requested = true;
                }
                CallbackChange::SetFocusTarget { target } => {
                    result.focus_target = Some(target);
                }
                CallbackChange::StopPropagation => {
                    result.stop_propagation = true;
                }
                CallbackChange::PreventDefault => {
                    result.prevent_default = true;
                }
                CallbackChange::AddTimer { timer_id, timer } => {
                    result.timers.insert(timer_id, timer);
                }
                CallbackChange::RemoveTimer { timer_id } => {
                    result.timers_removed.insert(timer_id);
                }
                CallbackChange::AddThread { thread_id, thread } => {
                    result.threads.insert(thread_id, thread);
                }
                CallbackChange::RemoveThread { thread_id } => {
                    result.threads_removed.insert(thread_id);
                }
                CallbackChange::ChangeNodeText { node_id, text } => {
                    let dom_id = node_id.dom;
                    let internal_node_id = match node_id.node.into_crate_internal() {
                        Some(id) => id,
                        None => continue,
                    };
                    result
                        .words_changed
                        .entry(dom_id)
                        .or_insert_with(BTreeMap::new)
                        .insert(internal_node_id, text);
                }
                CallbackChange::ChangeNodeImage {
                    dom_id,
                    node_id,
                    image,
                    update_type,
                } => {
                    result
                        .images_changed
                        .entry(dom_id)
                        .or_insert_with(BTreeMap::new)
                        .insert(node_id, (image, update_type));
                }
                CallbackChange::UpdateImageCallback { dom_id, node_id } => {
                    result
                        .image_callbacks_changed
                        .entry(dom_id)
                        .or_insert_with(FastBTreeSet::new)
                        .insert(node_id);
                }
                CallbackChange::UpdateIFrame { dom_id, node_id } => {
                    result
                        .iframes_to_update
                        .entry(dom_id)
                        .or_insert_with(FastBTreeSet::new)
                        .insert(node_id);
                }
                CallbackChange::ChangeNodeImageMask {
                    dom_id,
                    node_id,
                    mask,
                } => {
                    result
                        .image_masks_changed
                        .entry(dom_id)
                        .or_insert_with(BTreeMap::new)
                        .insert(node_id, mask);
                }
                CallbackChange::ChangeNodeCssProperties {
                    dom_id,
                    node_id,
                    properties,
                } => {
                    result
                        .css_properties_changed
                        .entry(dom_id)
                        .or_insert_with(BTreeMap::new)
                        .insert(node_id, properties);
                }
                CallbackChange::ScrollTo {
                    dom_id,
                    node_id,
                    position,
                } => {
                    result
                        .nodes_scrolled
                        .entry(dom_id)
                        .or_insert_with(BTreeMap::new)
                        .insert(node_id, position);
                }
                CallbackChange::ScrollIntoView { node_id, options } => {
                    // Use the scroll_into_view module to calculate and apply scroll adjustments
                    use crate::managers::scroll_into_view;
                    let now = azul_core::task::Instant::now();
                    let adjustments = scroll_into_view::scroll_node_into_view(
                        node_id,
                        &self.layout_results,
                        &mut self.scroll_manager,
                        options,
                        now,
                    );
                    // Record the scroll changes in nodes_scrolled
                    // The scroll_manager was already updated by scroll_node_into_view,
                    // but we need to report the new absolute positions for event processing
                    for adj in adjustments {
                        // Get the current scroll position from scroll_manager (now updated)
                        let current_pos = self.scroll_manager
                            .get_current_offset(adj.scroll_container_dom_id, adj.scroll_container_node_id)
                            .unwrap_or(LogicalPosition::zero());
                        
                        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(adj.scroll_container_node_id));
                        result
                            .nodes_scrolled
                            .entry(adj.scroll_container_dom_id)
                            .or_insert_with(BTreeMap::new)
                            .insert(hierarchy_id, current_pos);
                    }
                }
                CallbackChange::AddImageToCache { id, image } => {
                    image_cache.add_css_image_id(id, image);
                }
                CallbackChange::RemoveImageFromCache { id } => {
                    image_cache.delete_css_image_id(&id);
                }
                CallbackChange::ReloadSystemFonts => {
                    *system_fonts = FcFontCache::build();
                }
                CallbackChange::OpenMenu { menu, position } => {
                    result.menus_to_open.push((menu, position));
                }
                CallbackChange::ShowTooltip { text, position } => {
                    result.tooltips_to_show.push((text, position));
                }
                CallbackChange::HideTooltip => {
                    result.hide_tooltip = true;
                }
                CallbackChange::InsertText {
                    dom_id,
                    node_id,
                    text,
                } => {
                    // Record text input for the node
                    let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                    let dom_node_id = DomNodeId {
                        dom: dom_id,
                        node: hierarchy_id,
                    };

                    // Get old text from node
                    let old_inline_content = self.get_text_before_textinput(dom_id, node_id);
                    let old_text = self.extract_text_from_inline_content(&old_inline_content);

                    // Record the text input
                    use crate::managers::text_input::TextInputSource;
                    self.text_input_manager.record_input(
                        dom_node_id,
                        text.to_string(),
                        old_text,
                        TextInputSource::Programmatic,
                    );
                }
                CallbackChange::DeleteBackward { dom_id, node_id } => {
                    // Get current cursor/selection
                    if let Some(cursor) = self.cursor_manager.get_cursor() {
                        // Get current content
                        let content = self.get_text_before_textinput(dom_id, node_id);

                        // Apply delete backward using text3::edit

                        use crate::text3::edit::{delete_backward, TextEdit};
                        let mut new_content = content.clone();
                        let (updated_content, new_cursor) =
                            delete_backward(&mut new_content, cursor);

                        // Update cursor position
                        self.cursor_manager
                            .move_cursor_to(new_cursor, dom_id, node_id);

                        // Update text cache
                        self.update_text_cache_after_edit(dom_id, node_id, updated_content);

                        // Mark node as dirty
                        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                        let dom_node_id = DomNodeId {
                            dom: dom_id,
                            node: hierarchy_id,
                        };
                        // Note: Dirty marking happens in the caller
                    }
                }
                CallbackChange::DeleteForward { dom_id, node_id } => {
                    // Get current cursor/selection
                    if let Some(cursor) = self.cursor_manager.get_cursor() {
                        // Get current content
                        let content = self.get_text_before_textinput(dom_id, node_id);

                        // Apply delete forward using text3::edit

                        use crate::text3::edit::{delete_forward, TextEdit};
                        let mut new_content = content.clone();
                        let (updated_content, new_cursor) =
                            delete_forward(&mut new_content, cursor);

                        // Update cursor position
                        self.cursor_manager
                            .move_cursor_to(new_cursor, dom_id, node_id);

                        // Update text cache
                        self.update_text_cache_after_edit(dom_id, node_id, updated_content);
                    }
                }
                CallbackChange::MoveCursor {
                    dom_id,
                    node_id,
                    cursor,
                } => {
                    // Update cursor position in CursorManager
                    self.cursor_manager.move_cursor_to(cursor, dom_id, node_id);
                }
                CallbackChange::SetSelection {
                    dom_id,
                    node_id,
                    selection,
                } => {
                    // Update selection in SelectionManager
                    let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                    let dom_node_id = DomNodeId {
                        dom: dom_id,
                        node: hierarchy_id,
                    };

                    match selection {
                        Selection::Cursor(cursor) => {
                            self.cursor_manager.move_cursor_to(cursor, dom_id, node_id);
                            self.selection_manager.clear_all();
                        }
                        Selection::Range(range) => {
                            self.cursor_manager
                                .move_cursor_to(range.start, dom_id, node_id);
                            // TODO: Set selection range in SelectionManager
                            // self.selection_manager.set_selection(dom_node_id, range);
                        }
                    }
                }
                CallbackChange::SetTextChangeset { changeset } => {
                    // Override the current text input changeset
                    // This allows user callbacks to modify what text will be inserted
                    self.text_input_manager.pending_changeset = Some(changeset);
                }
                // Cursor Movement Operations
                CallbackChange::MoveCursorLeft {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    if let Some(new_cursor) =
                        self.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                            layout.move_cursor_left(*cursor, &mut None)
                        })
                    {
                        self.handle_cursor_movement(dom_id, node_id, new_cursor, extend_selection);
                    }
                }
                CallbackChange::MoveCursorRight {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    if let Some(new_cursor) =
                        self.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                            layout.move_cursor_right(*cursor, &mut None)
                        })
                    {
                        self.handle_cursor_movement(dom_id, node_id, new_cursor, extend_selection);
                    }
                }
                CallbackChange::MoveCursorUp {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    if let Some(new_cursor) =
                        self.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                            layout.move_cursor_up(*cursor, &mut None, &mut None)
                        })
                    {
                        self.handle_cursor_movement(dom_id, node_id, new_cursor, extend_selection);
                    }
                }
                CallbackChange::MoveCursorDown {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    if let Some(new_cursor) =
                        self.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                            layout.move_cursor_down(*cursor, &mut None, &mut None)
                        })
                    {
                        self.handle_cursor_movement(dom_id, node_id, new_cursor, extend_selection);
                    }
                }
                CallbackChange::MoveCursorToLineStart {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    if let Some(new_cursor) =
                        self.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                            layout.move_cursor_to_line_start(*cursor, &mut None)
                        })
                    {
                        self.handle_cursor_movement(dom_id, node_id, new_cursor, extend_selection);
                    }
                }
                CallbackChange::MoveCursorToLineEnd {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    if let Some(new_cursor) =
                        self.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                            layout.move_cursor_to_line_end(*cursor, &mut None)
                        })
                    {
                        self.handle_cursor_movement(dom_id, node_id, new_cursor, extend_selection);
                    }
                }
                CallbackChange::MoveCursorToDocumentStart {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    // Document start is the first cluster in the layout
                    if let Some(new_cursor) = self.get_inline_layout_for_node(dom_id, node_id) {
                        if let Some(first_cluster) = new_cursor
                            .items
                            .first()
                            .and_then(|item| item.item.as_cluster())
                        {
                            let doc_start_cursor = TextCursor {
                                cluster_id: first_cluster.source_cluster_id,
                                affinity: CursorAffinity::Leading,
                            };
                            self.handle_cursor_movement(
                                dom_id,
                                node_id,
                                doc_start_cursor,
                                extend_selection,
                            );
                        }
                    }
                }
                CallbackChange::MoveCursorToDocumentEnd {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    // Document end is the last cluster in the layout
                    if let Some(layout) = self.get_inline_layout_for_node(dom_id, node_id) {
                        if let Some(last_cluster) =
                            layout.items.last().and_then(|item| item.item.as_cluster())
                        {
                            let doc_end_cursor = TextCursor {
                                cluster_id: last_cluster.source_cluster_id,
                                affinity: CursorAffinity::Trailing,
                            };
                            self.handle_cursor_movement(
                                dom_id,
                                node_id,
                                doc_end_cursor,
                                extend_selection,
                            );
                        }
                    }
                }
                // Clipboard Operations (Override)
                CallbackChange::SetCopyContent { target, content } => {
                    // Store clipboard content to be written to system clipboard
                    // This will be picked up by the platform's sync_clipboard() method
                    self.clipboard_manager.set_copy_content(content);
                }
                CallbackChange::SetCutContent { target, content } => {
                    // Same as copy, but the deletion is handled separately
                    self.clipboard_manager.set_copy_content(content);
                }
                CallbackChange::SetSelectAllRange { target, range } => {
                    // Override selection range for select-all operation
                    // Convert DomNodeId back to internal NodeId
                    if let Some(node_id_internal) = target.node.into_crate_internal() {
                        let dom_node_id = azul_core::dom::DomNodeId {
                            dom: target.dom,
                            node: target.node,
                        };
                        self.selection_manager
                            .set_range(target.dom, dom_node_id, range);
                    }
                }
                CallbackChange::RequestHitTestUpdate { position } => {
                    // Mark that a hit test update is requested
                    // This will be processed by the shell layer which has access to WebRender
                    result.hit_test_update_requested = Some(position);
                }
                CallbackChange::ProcessTextSelectionClick { position, time_ms } => {
                    // Process text selection click at position
                    // This is used by the Debug API to trigger text selection directly
                    // The selection update will cause the display list to be regenerated
                    let _ = self.process_mouse_click_for_selection(position, time_ms);
                }
                CallbackChange::SetCursorVisibility { visible: _ } => {
                    // Timer callback sets visibility - check if we should blink or stay solid
                    let now = azul_core::task::Instant::now();
                    if self.cursor_manager.should_blink(&now) {
                        // Enough time has passed since last input - toggle visibility
                        self.cursor_manager.toggle_visibility();
                    } else {
                        // User is actively typing - keep cursor visible
                        self.cursor_manager.set_visibility(true);
                    }
                }
                CallbackChange::ResetCursorBlink => {
                    // Reset cursor blink state on user input
                    let now = azul_core::task::Instant::now();
                    self.cursor_manager.reset_blink_on_input(now);
                }
                CallbackChange::StartCursorBlinkTimer => {
                    // Start the cursor blink timer if not already active
                    if !self.cursor_manager.is_blink_timer_active() {
                        let timer = self.create_cursor_blink_timer(current_window_state);
                        result.timers.insert(azul_core::task::CURSOR_BLINK_TIMER_ID, timer);
                        self.cursor_manager.set_blink_timer_active(true);
                    }
                }
                CallbackChange::StopCursorBlinkTimer => {
                    // Stop the cursor blink timer
                    if self.cursor_manager.is_blink_timer_active() {
                        result.timers_removed.insert(azul_core::task::CURSOR_BLINK_TIMER_ID);
                        self.cursor_manager.set_blink_timer_active(false);
                    }
                }
                CallbackChange::ScrollActiveCursorIntoView => {
                    // Scroll the active text cursor into view
                    self.scroll_active_cursor_into_view(&mut result);
                }
                CallbackChange::CreateTextInput { text } => {
                    // Create a synthetic text input event
                    // This simulates receiving text input from the OS
                    println!("[CreateTextInput] Processing text: '{}'", text.as_str());
                    
                    // Process the text input - this records the changeset in TextInputManager
                    let affected_nodes = self.process_text_input(text.as_str());
                    println!("[CreateTextInput] process_text_input returned {} affected nodes", affected_nodes.len());
                    
                    // Mark that we need to trigger text input callbacks
                    // The affected nodes and their events will be processed by the recursive event loop
                    for (node, (events, _)) in affected_nodes {
                        result.text_input_triggered.push((node, events));
                    }
                }
            }
        }

        // Sync cursor to selection manager for rendering
        // This must happen after all cursor updates
        self.sync_cursor_to_selection_manager();

        result
    }

    /// Helper: Get inline layout for a node
    /// 
    /// For text nodes that participate in an IFC, the inline layout is stored
    /// on the IFC root node (the block container), not on the text node itself.
    /// This method handles both cases:
    /// 1. The node has its own `inline_layout_result` (IFC root)
    /// 2. The node has `ifc_membership` pointing to the IFC root
    ///
    /// This is a thin wrapper around `LayoutTree::get_inline_layout_for_node`.
    fn get_inline_layout_for_node(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<&Arc<UnifiedLayout>> {
        let layout_result = self.layout_results.get(&dom_id)?;

        let layout_indices = layout_result.layout_tree.dom_to_layout.get(&node_id)?;
        let layout_index = *layout_indices.first()?;

        // Use the centralized LayoutTree method that handles IFC membership
        layout_result.layout_tree.get_inline_layout_for_node(layout_index)
    }

    /// Helper: Move cursor using a movement function and return the new cursor if it changed
    fn move_cursor_in_node<F>(
        &self,
        dom_id: DomId,
        node_id: NodeId,
        movement_fn: F,
    ) -> Option<TextCursor>
    where
        F: FnOnce(&UnifiedLayout, &TextCursor) -> TextCursor,
    {
        let current_cursor = self.cursor_manager.get_cursor()?;
        let layout = self.get_inline_layout_for_node(dom_id, node_id)?;

        let new_cursor = movement_fn(layout, current_cursor);

        // Only return if cursor actually moved
        if new_cursor != *current_cursor {
            Some(new_cursor)
        } else {
            None
        }
    }

    /// Helper: Handle cursor movement with optional selection extension
    fn handle_cursor_movement(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        new_cursor: TextCursor,
        extend_selection: bool,
    ) {
        if extend_selection {
            // Get the current cursor as the selection anchor
            if let Some(old_cursor) = self.cursor_manager.get_cursor() {
                // Create DomNodeId for the selection
                let dom_node_id = azul_core::dom::DomNodeId {
                    dom: dom_id,
                    node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
                };

                // Create a selection range from old cursor to new cursor
                let selection_range = if new_cursor.cluster_id.start_byte_in_run
                    < old_cursor.cluster_id.start_byte_in_run
                {
                    // Moving backwards
                    SelectionRange {
                        start: new_cursor,
                        end: *old_cursor,
                    }
                } else {
                    // Moving forwards
                    SelectionRange {
                        start: *old_cursor,
                        end: new_cursor,
                    }
                };

                // Set the selection range in SelectionManager
                self.selection_manager
                    .set_range(dom_id, dom_node_id, selection_range);
            }

            // Move cursor to new position
            self.cursor_manager
                .move_cursor_to(new_cursor, dom_id, node_id);
        } else {
            // Just move cursor without extending selection
            self.cursor_manager
                .move_cursor_to(new_cursor, dom_id, node_id);

            // Clear any existing selection
            self.selection_manager.clear_selection(&dom_id);
        }
    }

    // Gpu Value Cache Management

    /// Get the GPU value cache for a specific DOM
    pub fn get_gpu_cache(&self, dom_id: &DomId) -> Option<&GpuValueCache> {
        self.gpu_state_manager.caches.get(dom_id)
    }

    /// Get a mutable reference to the GPU value cache for a specific DOM
    pub fn get_gpu_cache_mut(&mut self, dom_id: &DomId) -> Option<&mut GpuValueCache> {
        self.gpu_state_manager.caches.get_mut(dom_id)
    }

    /// Get or create a GPU value cache for a specific DOM
    pub fn get_or_create_gpu_cache(&mut self, dom_id: DomId) -> &mut GpuValueCache {
        self.gpu_state_manager
            .caches
            .entry(dom_id)
            .or_insert_with(GpuValueCache::default)
    }

    // Layout Result Access

    /// Get a layout result for a specific DOM
    pub fn get_layout_result(&self, dom_id: &DomId) -> Option<&DomLayoutResult> {
        self.layout_results.get(dom_id)
    }

    /// Get a mutable layout result for a specific DOM
    pub fn get_layout_result_mut(&mut self, dom_id: &DomId) -> Option<&mut DomLayoutResult> {
        self.layout_results.get_mut(dom_id)
    }

    /// Get all DOM IDs that have layout results
    pub fn get_dom_ids(&self) -> DomIdVec {
        self.layout_results
            .keys()
            .copied()
            .collect::<Vec<_>>()
            .into()
    }

    // Hit-Test Computation

    /// Compute the cursor type hit-test from a full hit-test
    ///
    /// This determines which mouse cursor to display based on the CSS cursor
    /// properties of the hovered nodes.
    pub fn compute_cursor_type_hit_test(
        &self,
        hit_test: &crate::hit_test::FullHitTest,
    ) -> crate::hit_test::CursorTypeHitTest {
        crate::hit_test::CursorTypeHitTest::new(hit_test, self)
    }

    // TODO: Implement compute_hit_test() once we have the actual hit-testing logic
    // This would involve:
    // 1. Converting screen coordinates to layout coordinates
    // 2. Traversing the layout tree to find nodes under the cursor
    // 3. Hand

... [FILE TRUNCATED - original size: 294963 bytes] ...
```

### layout/src/text3/cache.rs

```rust
use std::{
    any::{Any, TypeId},
    cmp::Ordering,
    collections::{
        hash_map::{DefaultHasher, Entry, HashMap},
        BTreeSet,
    },
    hash::{Hash, Hasher},
    mem::discriminant,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

pub use azul_core::selection::{ContentIndex, GraphemeClusterId};
use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::ImageRef,
    selection::{CursorAffinity, SelectionRange, TextCursor},
    ui_solver::GlyphInstance,
};
use azul_css::{
    corety::LayoutDebugMessage, props::basic::ColorU, props::style::StyleBackgroundContent,
};
#[cfg(feature = "text_layout_hyphenation")]
use hyphenation::{Hyphenator, Language as HyphenationLanguage, Load, Standard};
use rust_fontconfig::{FcFontCache, FcPattern, FcWeight, FontId, PatternMatch, UnicodeRange};
use unicode_bidi::{BidiInfo, Level, TextSource};
use unicode_segmentation::UnicodeSegmentation;

// Stub type when hyphenation is disabled
#[cfg(not(feature = "text_layout_hyphenation"))]
pub struct Standard;

#[cfg(not(feature = "text_layout_hyphenation"))]
impl Standard {
    /// Stub hyphenate method that returns no breaks
    pub fn hyphenate<'a>(&'a self, _word: &'a str) -> StubHyphenationBreaks {
        StubHyphenationBreaks { breaks: Vec::new() }
    }
}

/// Result of hyphenation (stub when feature is disabled)
#[cfg(not(feature = "text_layout_hyphenation"))]
pub struct StubHyphenationBreaks {
    pub breaks: alloc::vec::Vec<usize>,
}

// Always import Language from script module
use crate::text3::script::{script_to_language, Language, Script};

/// Available space for layout, similar to Taffy's AvailableSpace.
///
/// This type explicitly represents the three possible states for available space:
///
/// - `Definite(f32)`: A specific pixel width is available
/// - `MinContent`: Layout should use minimum content width (shrink-wrap)
/// - `MaxContent`: Layout should use maximum content width (no line breaks unless necessary)
///
/// This is critical for proper handling of intrinsic sizing in Flexbox/Grid
/// where the available space may be indefinite during the measure phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AvailableSpace {
    /// A specific amount of space is available (in pixels)
    Definite(f32),
    /// The node should be laid out under a min-content constraint
    MinContent,
    /// The node should be laid out under a max-content constraint  
    MaxContent,
}

impl Default for AvailableSpace {
    fn default() -> Self {
        AvailableSpace::Definite(0.0)
    }
}

impl AvailableSpace {
    /// Returns true if this is a definite (finite, known) amount of space
    pub fn is_definite(&self) -> bool {
        matches!(self, AvailableSpace::Definite(_))
    }

    /// Returns true if this is an indefinite (min-content or max-content) constraint
    pub fn is_indefinite(&self) -> bool {
        !self.is_definite()
    }

    /// Returns the definite value if available, or a fallback for indefinite constraints
    pub fn unwrap_or(self, fallback: f32) -> f32 {
        match self {
            AvailableSpace::Definite(v) => v,
            _ => fallback,
        }
    }

    /// Returns the definite value, or 0.0 for min-content, or f32::MAX for max-content
    pub fn to_f32_for_layout(self) -> f32 {
        match self {
            AvailableSpace::Definite(v) => v,
            AvailableSpace::MinContent => 0.0,
            AvailableSpace::MaxContent => f32::MAX,
        }
    }

    /// Create from an f32 value, recognizing special sentinel values.
    ///
    /// This function provides backwards compatibility with code that uses f32 for constraints:
    /// - `f32::INFINITY` or `f32::MAX` → `MaxContent` (no line wrapping)
    /// - `0.0` → `MinContent` (maximum line wrapping, return longest word width)
    /// - Other values → `Definite(value)`
    ///
    /// Note: Using sentinel values like 0.0 for MinContent is fragile. Prefer using
    /// `AvailableSpace::MinContent` directly when possible.
    pub fn from_f32(value: f32) -> Self {
        if value.is_infinite() || value >= f32::MAX / 2.0 {
            // Treat very large values (including f32::MAX) as MaxContent
            AvailableSpace::MaxContent
        } else if value <= 0.0 {
            // Treat zero or negative as MinContent (shrink-wrap)
            AvailableSpace::MinContent
        } else {
            AvailableSpace::Definite(value)
        }
    }
}

impl Hash for AvailableSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        if let AvailableSpace::Definite(v) = self {
            (v.round() as usize).hash(state);
        }
    }
}

// Re-export traits for backwards compatibility
pub use crate::font_traits::{ParsedFontTrait, ShallowClone};

// --- Core Data Structures for the New Architecture ---

/// Key for caching font chains - based only on CSS properties, not text content
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontChainKey {
    pub font_families: Vec<String>,
    pub weight: FcWeight,
    pub italic: bool,
    pub oblique: bool,
}

/// Either a FontChainKey (resolved via fontconfig) or a direct FontRef hash.
/// 
/// This enum cleanly separates:
/// - `Chain`: Fonts resolved through fontconfig with fallback support
/// - `Ref`: Direct FontRef that bypasses fontconfig entirely (e.g., embedded icon fonts)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FontChainKeyOrRef {
    /// Regular font chain resolved via fontconfig
    Chain(FontChainKey),
    /// Direct FontRef identified by pointer address (covers entire Unicode range, no fallbacks)
    Ref(usize),
}

impl FontChainKeyOrRef {
    /// Create from a FontStack enum
    pub fn from_font_stack(font_stack: &FontStack) -> Self {
        match font_stack {
            FontStack::Stack(selectors) => FontChainKeyOrRef::Chain(FontChainKey::from_selectors(selectors)),
            FontStack::Ref(font_ref) => FontChainKeyOrRef::Ref(font_ref.parsed as usize),
        }
    }
    
    /// Returns true if this is a direct FontRef
    pub fn is_ref(&self) -> bool {
        matches!(self, FontChainKeyOrRef::Ref(_))
    }
    
    /// Returns the FontRef pointer if this is a Ref variant
    pub fn as_ref_ptr(&self) -> Option<usize> {
        match self {
            FontChainKeyOrRef::Ref(ptr) => Some(*ptr),
            _ => None,
        }
    }
    
    /// Returns the FontChainKey if this is a Chain variant
    pub fn as_chain(&self) -> Option<&FontChainKey> {
        match self {
            FontChainKeyOrRef::Chain(key) => Some(key),
            _ => None,
        }
    }
}

impl FontChainKey {
    /// Create a FontChainKey from a slice of font selectors
    pub fn from_selectors(font_stack: &[FontSelector]) -> Self {
        let font_families: Vec<String> = font_stack
            .iter()
            .map(|s| s.family.clone())
            .filter(|f| !f.is_empty())
            .collect();

        let font_families = if font_families.is_empty() {
            vec!["serif".to_string()]
        } else {
            font_families
        };

        let weight = font_stack
            .first()
            .map(|s| s.weight)
            .unwrap_or(FcWeight::Normal);
        let is_italic = font_stack
            .first()
            .map(|s| s.style == FontStyle::Italic)
            .unwrap_or(false);
        let is_oblique = font_stack
            .first()
            .map(|s| s.style == FontStyle::Oblique)
            .unwrap_or(false);

        FontChainKey {
            font_families,
            weight,
            italic: is_italic,
            oblique: is_oblique,
        }
    }
}

/// A map of pre-loaded fonts, keyed by FontId (from rust-fontconfig)
///
/// This is passed to the shaper - no font loading happens during shaping
/// The fonts are loaded BEFORE layout based on the font chains and text content.
///
/// Provides both FontId and hash-based lookup for efficient glyph operations.
#[derive(Debug, Clone)]
pub struct LoadedFonts<T> {
    /// Primary storage: FontId -> Font
    pub fonts: HashMap<FontId, T>,
    /// Reverse index: font_hash -> FontId for fast hash-based lookups
    hash_to_id: HashMap<u64, FontId>,
}

impl<T: ParsedFontTrait> LoadedFonts<T> {
    pub fn new() -> Self {
        Self {
            fonts: HashMap::new(),
            hash_to_id: HashMap::new(),
        }
    }

    /// Insert a font with its FontId
    pub fn insert(&mut self, font_id: FontId, font: T) {
        let hash = font.get_hash();
        self.hash_to_id.insert(hash, font_id.clone());
        self.fonts.insert(font_id, font);
    }

    /// Get a font by FontId
    pub fn get(&self, font_id: &FontId) -> Option<&T> {
        self.fonts.get(font_id)
    }

    /// Get a font by its hash
    pub fn get_by_hash(&self, hash: u64) -> Option<&T> {
        self.hash_to_id.get(&hash).and_then(|id| self.fonts.get(id))
    }

    /// Get the FontId for a hash
    pub fn get_font_id_by_hash(&self, hash: u64) -> Option<&FontId> {
        self.hash_to_id.get(&hash)
    }

    /// Check if a FontId is present
    pub fn contains_key(&self, font_id: &FontId) -> bool {
        self.fonts.contains_key(font_id)
    }

    /// Check if a hash is present
    pub fn contains_hash(&self, hash: u64) -> bool {
        self.hash_to_id.contains_key(&hash)
    }

    /// Iterate over all fonts
    pub fn iter(&self) -> impl Iterator<Item = (&FontId, &T)> {
        self.fonts.iter()
    }

    /// Get the number of loaded fonts
    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

impl<T: ParsedFontTrait> Default for LoadedFonts<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ParsedFontTrait> FromIterator<(FontId, T)> for LoadedFonts<T> {
    fn from_iter<I: IntoIterator<Item = (FontId, T)>>(iter: I) -> Self {
        let mut loaded = LoadedFonts::new();
        for (id, font) in iter {
            loaded.insert(id, font);
        }
        loaded
    }
}

/// Enum that wraps either a fontconfig-resolved font (T) or a direct FontRef.
///
/// This allows the shaping code to handle both fontconfig-resolved fonts
/// and embedded fonts (FontRef) uniformly through the ParsedFontTrait interface.
#[derive(Debug, Clone)]
pub enum FontOrRef<T> {
    /// A font loaded via fontconfig
    Font(T),
    /// A direct FontRef (embedded font, bypasses fontconfig)
    Ref(azul_css::props::basic::FontRef),
}

impl<T: ParsedFontTrait> ShallowClone for FontOrRef<T> {
    fn shallow_clone(&self) -> Self {
        match self {
            FontOrRef::Font(f) => FontOrRef::Font(f.shallow_clone()),
            FontOrRef::Ref(r) => FontOrRef::Ref(r.clone()),
        }
    }
}

impl<T: ParsedFontTrait> ParsedFontTrait for FontOrRef<T> {
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        language: Language,
        direction: BidiDirection,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph>, LayoutError> {
        match self {
            FontOrRef::Font(f) => f.shape_text(text, script, language, direction, style),
            FontOrRef::Ref(r) => r.shape_text(text, script, language, direction, style),
        }
    }

    fn get_hash(&self) -> u64 {
        match self {
            FontOrRef::Font(f) => f.get_hash(),
            FontOrRef::Ref(r) => r.get_hash(),
        }
    }

    fn get_glyph_size(&self, glyph_id: u16, font_size: f32) -> Option<LogicalSize> {
        match self {
            FontOrRef::Font(f) => f.get_glyph_size(glyph_id, font_size),
            FontOrRef::Ref(r) => r.get_glyph_size(glyph_id, font_size),
        }
    }

    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        match self {
            FontOrRef::Font(f) => f.get_hyphen_glyph_and_advance(font_size),
            FontOrRef::Ref(r) => r.get_hyphen_glyph_and_advance(font_size),
        }
    }

    fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        match self {
            FontOrRef::Font(f) => f.get_kashida_glyph_and_advance(font_size),
            FontOrRef::Ref(r) => r.get_kashida_glyph_and_advance(font_size),
        }
    }

    fn has_glyph(&self, codepoint: u32) -> bool {
        match self {
            FontOrRef::Font(f) => f.has_glyph(codepoint),
            FontOrRef::Ref(r) => r.has_glyph(codepoint),
        }
    }

    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics> {
        match self {
            FontOrRef::Font(f) => f.get_vertical_metrics(glyph_id),
            FontOrRef::Ref(r) => r.get_vertical_metrics(glyph_id),
        }
    }

    fn get_font_metrics(&self) -> LayoutFontMetrics {
        match self {
            FontOrRef::Font(f) => f.get_font_metrics(),
            FontOrRef::Ref(r) => r.get_font_metrics(),
        }
    }

    fn num_glyphs(&self) -> u16 {
        match self {
            FontOrRef::Font(f) => f.num_glyphs(),
            FontOrRef::Ref(r) => r.num_glyphs(),
        }
    }
}

#[derive(Debug)]
pub struct FontManager<T> {
    ///  Cache that holds the **file paths** of the fonts (not any font data itself)
    pub fc_cache: Arc<FcFontCache>,
    /// Holds the actual parsed font (usually with the font bytes attached)
    pub parsed_fonts: Mutex<HashMap<FontId, T>>,
    // Cache for font chains - populated by resolve_all_font_chains() before layout
    // This is read-only during layout - no locking needed for reads
    pub font_chain_cache: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    /// Cache for direct FontRefs (embedded fonts like Material Icons)
    /// These are fonts referenced via FontStack::Ref that bypass fontconfig
    pub embedded_fonts: Mutex<HashMap<u64, azul_css::props::basic::FontRef>>,
}

impl<T: ParsedFontTrait> FontManager<T> {
    pub fn new(fc_cache: FcFontCache) -> Result<Self, LayoutError> {
        Ok(Self {
            fc_cache: Arc::new(fc_cache),
            parsed_fonts: Mutex::new(HashMap::new()),
            font_chain_cache: HashMap::new(), // Populated via set_font_chain_cache()
            embedded_fonts: Mutex::new(HashMap::new()),
        })
    }

    /// Set the font chain cache from externally resolved chains
    ///
    /// This should be called with the result of `resolve_font_chains()` or
    /// `collect_and_resolve_font_chains()` from `solver3::getters`.
    pub fn set_font_chain_cache(
        &mut self,
        chains: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    ) {
        self.font_chain_cache = chains;
    }

    /// Merge additional font chains into the existing cache
    ///
    /// Useful when processing multiple DOMs that may have different font requirements.
    pub fn merge_font_chain_cache(
        &mut self,
        chains: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    ) {
        self.font_chain_cache.extend(chains);
    }

    /// Get a reference to the font chain cache
    pub fn get_font_chain_cache(
        &self,
    ) -> &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain> {
        &self.font_chain_cache
    }

    /// Get an embedded font by its hash (used for WebRender registration)
    /// Returns the FontRef if it exists in the embedded_fonts cache.
    pub fn get_embedded_font_by_hash(&self, font_hash: u64) -> Option<azul_css::props::basic::FontRef> {
        let embedded = self.embedded_fonts.lock().unwrap();
        embedded.get(&font_hash).cloned()
    }

    /// Get a parsed font by its hash (used for WebRender registration)
    /// Returns the parsed font if it exists in the parsed_fonts cache.
    pub fn get_font_by_hash(&self, font_hash: u64) -> Option<T> {
        let parsed = self.parsed_fonts.lock().unwrap();
        // Linear search through all cached fonts to find one with matching hash
        for (_, font) in parsed.iter() {
            if font.get_hash() == font_hash {
                return Some(font.clone());
            }
        }
        None
    }

    /// Register an embedded FontRef for later lookup by hash
    /// This is called when using FontStack::Ref during shaping
    pub fn register_embedded_font(&self, font_ref: &azul_css::props::basic::FontRef) {
        let hash = font_ref.get_hash();
        let mut embedded = self.embedded_fonts.lock().unwrap();
        embedded.insert(hash, font_ref.clone());
    }

    /// Get a snapshot of all currently loaded fonts
    ///
    /// This returns a copy of all parsed fonts, which can be passed to the shaper.
    /// No locking is required after this call - the returned HashMap is independent.
    ///
    /// NOTE: This should be called AFTER loading all required fonts for a layout pass.
    pub fn get_loaded_fonts(&self) -> LoadedFonts<T> {
        let parsed = self.parsed_fonts.lock().unwrap();
        parsed
            .iter()
            .map(|(id, font)| (id.clone(), font.shallow_clone()))
            .collect()
    }

    /// Get the set of FontIds that are currently loaded
    ///
    /// This is useful for computing which fonts need to be loaded
    /// (diff with required fonts).
    pub fn get_loaded_font_ids(&self) -> std::collections::HashSet<FontId> {
        let parsed = self.parsed_fonts.lock().unwrap();
        parsed.keys().cloned().collect()
    }

    /// Insert a loaded font into the cache
    ///
    /// Returns the old font if one was already present for this FontId.
    pub fn insert_font(&self, font_id: FontId, font: T) -> Option<T> {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        parsed.insert(font_id, font)
    }

    /// Insert multiple loaded fonts into the cache
    ///
    /// This is more efficient than calling `insert_font` multiple times
    /// because it only acquires the lock once.
    pub fn insert_fonts(&self, fonts: impl IntoIterator<Item = (FontId, T)>) {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        for (font_id, font) in fonts {
            parsed.insert(font_id, font);
        }
    }

    /// Remove a font from the cache
    ///
    /// Returns the removed font if it was present.
    pub fn remove_font(&self, font_id: &FontId) -> Option<T> {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        parsed.remove(font_id)
    }
}

// Error handling
#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("Bidi analysis failed: {0}")]
    BidiError(String),
    #[error("Shaping failed: {0}")]
    ShapingError(String),
    #[error("Font not found: {0:?}")]
    FontNotFound(FontSelector),
    #[error("Invalid text input: {0}")]
    InvalidText(String),
    #[error("Hyphenation failed: {0}")]
    HyphenationError(String),
}

/// Text boundary types for cursor movement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextBoundary {
    /// Reached top of text (first line)
    Top,
    /// Reached bottom of text (last line)
    Bottom,
    /// Reached start of text (first character)
    Start,
    /// Reached end of text (last character)
    End,
}

/// Error returned when cursor movement hits a boundary
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorBoundsError {
    /// The boundary that was hit
    pub boundary: TextBoundary,
    /// The cursor position (unchanged from input)
    pub cursor: TextCursor,
}

/// Unified constraints combining all layout features
///
/// # CSS Inline Layout Module Level 3: Constraint Mapping
///
/// This structure maps CSS properties to layout constraints:
///
/// ## \u00a7 2.1 Layout of Line Boxes
/// - `available_width`: \u26a0\ufe0f CRITICAL - Should equal containing block's inner width
///   * Currently defaults to 0.0 which causes immediate line breaking
///   * Per spec: "logical width of a line box is equal to the inner logical width of its containing
///     block"
/// - `available_height`: For block-axis constraints (max-height)
///
/// ## \u00a7 2.2 Layout Within Line Boxes
/// - `text_align`: \u2705 Horizontal alignment (start, end, center, justify)
/// - `vertical_align`: \u26a0\ufe0f PARTIAL - Only baseline supported, missing:
///   * top, bottom, middle, text-top, text-bottom
///   * <length>, <percentage> values
///   * sub, super positions
/// - `line_height`: \u2705 Distance between baselines
///
/// ## \u00a7 3 Baselines and Alignment Metrics
/// - `text_orientation`: \u2705 For vertical writing (sideways, upright)
/// - `writing_mode`: \u2705 horizontal-tb, vertical-rl, vertical-lr
/// - `direction`: \u2705 ltr, rtl for BiDi
///
/// ## \u00a7 4 Baseline Alignment (vertical-align property)
/// \u26a0\ufe0f INCOMPLETE: Only basic baseline alignment implemented
///
/// ## \u00a7 5 Line Spacing (line-height property)
/// - `line_height`: \u2705 Implemented
/// - \u274c MISSING: line-fit-edge for controlling which edges contribute to line height
///
/// ## \u00a7 6 Trimming Leading (text-box-trim)
/// - \u274c NOT IMPLEMENTED: text-box-trim property
/// - \u274c NOT IMPLEMENTED: text-box-edge property
///
/// ## CSS Text Module Level 3
/// - `text_indent`: \u2705 First line indentation
/// - `text_justify`: \u2705 Justification algorithm (auto, inter-word, inter-character)
/// - `hyphenation`: \u2705 Automatic hyphenation
/// - `hanging_punctuation`: \u2705 Hanging punctuation at line edges
///
/// ## CSS Text Level 4
/// - `text_wrap`: \u2705 balance, pretty, stable
/// - `line_clamp`: \u2705 Max number of lines
///
/// ## CSS Writing Modes Level 4
/// - `text_combine_upright`: \u2705 Tate-chu-yoko for vertical text
///
/// ## CSS Shapes Module
/// - `shape_boundaries`: \u2705 Custom line box shapes
/// - `shape_exclusions`: \u2705 Exclusion areas (float-like behavior)
/// - `exclusion_margin`: \u2705 Margin around exclusions
///
/// ## Multi-column Layout
/// - `columns`: \u2705 Number of columns
/// - `column_gap`: \u2705 Gap between columns
///
/// # Known Issues:
/// 1. [ISSUE] available_width defaults to Definite(0.0) instead of containing block width
/// 2. [ISSUE] vertical_align only supports baseline
/// 3. [TODO] initial-letter (drop caps) not implemented
#[derive(Debug, Clone)]
pub struct UnifiedConstraints {
    // Shape definition
    pub shape_boundaries: Vec<ShapeBoundary>,
    pub shape_exclusions: Vec<ShapeBoundary>,

    // Basic layout - using AvailableSpace for proper indefinite handling
    pub available_width: AvailableSpace,
    pub available_height: Option<f32>,

    // Text layout
    pub writing_mode: Option<WritingMode>,
    // Base direction from CSS, overrides auto-detection
    pub direction: Option<BidiDirection>,
    pub text_orientation: TextOrientation,
    pub text_align: TextAlign,
    pub text_justify: JustifyContent,
    pub line_height: f32,
    pub vertical_align: VerticalAlign,

    // Overflow handling
    pub overflow: OverflowBehavior,
    pub segment_alignment: SegmentAlignment,

    // Advanced features
    pub text_combine_upright: Option<TextCombineUpright>,
    pub exclusion_margin: f32,
    pub hyphenation: bool,
    pub hyphenation_language: Option<Language>,
    pub text_indent: f32,
    pub initial_letter: Option<InitialLetter>,
    pub line_clamp: Option<NonZeroUsize>,

    // text-wrap: balance
    pub text_wrap: TextWrap,
    pub columns: u32,
    pub column_gap: f32,
    pub hanging_punctuation: bool,
}

impl Default for UnifiedConstraints {
    fn default() -> Self {
        Self {
            shape_boundaries: Vec::new(),
            shape_exclusions: Vec::new(),

            // IMPORTANT: This should be set to the containing block's inner width
            // per CSS Inline-3 § 2.1, but defaults to Definite(0.0) which causes immediate line
            // breaking. This value should be passed from the box layout solver (fc.rs)
            // when creating UnifiedConstraints for text layout.
            available_width: AvailableSpace::Definite(0.0),
            available_height: None,
            writing_mode: None,
            direction: None, // Will default to LTR if not specified
            text_orientation: TextOrientation::default(),
            text_align: TextAlign::default(),
            text_justify: JustifyContent::default(),
            line_height: 16.0, // A more sensible default
            vertical_align: VerticalAlign::default(),
            overflow: OverflowBehavior::default(),
            segment_alignment: SegmentAlignment::default(),
            text_combine_upright: None,
            exclusion_margin: 0.0,
            hyphenation: false,
            hyphenation_language: None,
            columns: 1,
            column_gap: 0.0,
            hanging_punctuation: false,
            text_indent: 0.0,
            initial_letter: None,
            line_clamp: None,
            text_wrap: TextWrap::default(),
        }
    }
}

// UnifiedConstraints
impl Hash for UnifiedConstraints {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.shape_boundaries.hash(state);
        self.shape_exclusions.hash(state);
        self.available_width.hash(state);
        self.available_height
            .map(|h| h.round() as usize)
            .hash(state);
        self.writing_mode.hash(state);
        self.direction.hash(state);
        self.text_orientation.hash(state);
        self.text_align.hash(state);
        self.text_justify.hash(state);
        (self.line_height.round() as usize).hash(state);
        self.vertical_align.hash(state);
        self.overflow.hash(state);
        self.text_combine_upright.hash(state);
        (self.exclusion_margin.round() as usize).hash(state);
        self.hyphenation.hash(state);
        self.hyphenation_language.hash(state);
        self.columns.hash(state);
        (self.column_gap.round() as usize).hash(state);
        self.hanging_punctuation.hash(state);
    }
}

impl PartialEq for UnifiedConstraints {
    fn eq(&self, other: &Self) -> bool {
        self.shape_boundaries == other.shape_boundaries
            && self.shape_exclusions == other.shape_exclusions
            && self.available_width == other.available_width
            && match (self.available_height, other.available_height) {
                (None, None) => true,
                (Some(h1), Some(h2)) => round_eq(h1, h2),
                _ => false,
            }
            && self.writing_mode == other.writing_mode
            && self.direction == other.direction
            && self.text_orientation == other.text_orientation
            && self.text_align == other.text_align
            && self.text_justify == other.text_justify
            && round_eq(self.line_height, other.line_height)
            && self.vertical_align == other.vertical_align
            && self.overflow == other.overflow
            && self.text_combine_upright == other.text_combine_upright
            && round_eq(self.exclusion_margin, other.exclusion_margin)
            && self.hyphenation == other.hyphenation
            && self.hyphenation_language == other.hyphenation_language
            && self.columns == other.columns
            && round_eq(self.column_gap, other.column_gap)
            && self.hanging_punctuation == other.hanging_punctuation
    }
}

impl Eq for UnifiedConstraints {}

impl UnifiedConstraints {
    fn direction(&self, fallback: BidiDirection) -> BidiDirection {
        match self.writing_mode {
            Some(s) => s.get_direction().unwrap_or(fallback),
            None => fallback,
        }
    }
    fn is_vertical(&self) -> bool {
        matches!(
            self.writing_mode,
            Some(WritingMode::VerticalRl) | Some(WritingMode::VerticalLr)
        )
    }
}

/// Line constraints with multi-segment support
#[derive(Debug, Clone)]
pub struct LineConstraints {
    pub segments: Vec<LineSegment>,
    pub total_available: f32,
}

impl WritingMode {
    fn get_direction(&self) -> Option<BidiDirection> {
        match self {
            // determined by text content
            WritingMode::HorizontalTb => None,
            WritingMode::VerticalRl => Some(BidiDirection::Rtl),
            WritingMode::VerticalLr => Some(BidiDirection::Ltr),
            WritingMode::SidewaysRl => Some(BidiDirection::Rtl),
            WritingMode::SidewaysLr => Some(BidiDirection::Ltr),
        }
    }
}

// Stage 1: Collection - Styled runs from DOM traversal
#[derive(Debug, Clone, Hash)]
pub struct StyledRun {
    pub text: String,
    pub style: Arc<StyleProperties>,
    /// Byte index in the original logical paragraph text
    pub logical_start_byte: usize,
    /// The DOM NodeId of the Text node this run came from.
    /// None for generated content (e.g., list markers, ::before/::after).
    pub source_node_id: Option<NodeId>,
}

// Stage 2: Bidi Analysis - Visual runs in display order
#[derive(Debug, Clone)]
pub struct VisualRun<'a> {
    pub text_slice: &'a str,
    pub style: Arc<StyleProperties>,
    pub logical_start_byte: usize,
    pub bidi_level: BidiLevel,
    pub script: Script,
    pub language: Language,
}

// Font and styling types

/// A selector for loading fonts from the font cache.
/// Used by FontManager to query fontconfig and load font files.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontSelector {
    pub family: String,
    pub weight: FcWeight,
    pub style: FontStyle,
    pub unicode_ranges: Vec<UnicodeRange>,
}

impl Default for FontSelector {
    fn default() -> Self {
        Self {
            family: "serif".to_string(),
            weight: FcWeight::Normal,
            style: FontStyle::Normal,
            unicode_ranges: Vec::new(),
        }
    }
}

/// Font stack that can be either a list of font selectors (resolved via fontconfig)
/// or a direct FontRef (bypasses fontconfig entirely).
///
/// When a `FontRef` is used, it bypasses fontconfig resolution entirely
/// and uses the pre-parsed font data directly. This is used for embedded
/// fonts like Material Icons.
#[derive(Debug, Clone)]
pub enum FontStack {
    /// A stack of font selectors to be resolved via fontconfig
    /// First font is primary, rest are fallbacks
    Stack(Vec<FontSelector>),
    /// A direct reference to a pre-parsed font (e.g., embedded icon fonts)
    /// This font covers the entire Unicode range and has no fallbacks.
    Ref(azul_css::props::basic::font::FontRef),
}

impl Default for FontStack {
    fn default() -> Self {
        FontStack::Stack(vec![FontSelector::default()])
    }
}

impl FontStack {
    /// Returns true if this is a direct FontRef
    pub fn is_ref(&self) -> bool {
        matches!(self, FontStack::Ref(_))
    }

    /// Returns the FontRef if this is a Ref variant
    pub fn as_ref(&self) -> Option<&azul_css::props::basic::font::FontRef> {
        match self {
            FontStack::Ref(r) => Some(r),
            _ => None,
        }
    }

    /// Returns the font selectors if this is a Stack variant
    pub fn as_stack(&self) -> Option<&[FontSelector]> {
        match self {
            FontStack::Stack(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the first FontSelector if this is a Stack variant, None if Ref
    pub fn first_selector(&self) -> Option<&FontSelector> {
        match self {
            FontStack::Stack(s) => s.first(),
            FontStack::Ref(_) => None,
        }
    }

    /// Returns the first font family name (for Stack) or a placeholder (for Ref)
    pub fn first_family(&self) -> &str {
        match self {
            FontStack::Stack(s) => s.first().map(|f| f.family.as_str()).unwrap_or("serif"),
            FontStack::Ref(_) => "<embedded-font>",
        }
    }
}

impl PartialEq for FontStack {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (FontStack::Stack(a), FontStack::Stack(b)) => a == b,
            (FontStack::Ref(a), FontStack::Ref(b)) => a.parsed == b.parsed,
            _ => false,
        }
    }
}

impl Eq for FontStack {}

impl Hash for FontStack {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            FontStack::Stack(s) => s.hash(state),
            FontStack::Ref(r) => (r.parsed as usize).hash(state),
        }
    }
}

/// A reference to a font for rendering, identified by its hash.
/// This hash corresponds to ParsedFont::hash and is used to look up
/// the actual font data in the renderer's font cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontHash {
    /// The hash of the ParsedFont. 0 means invalid/unknown font.
    pub font_hash: u64,
}

impl FontHash {
    pub fn invalid() -> Self {
        Self { font_hash: 0 }
    }

    pub fn from_hash(font_hash: u64) -> Self {
        Self { font_hash }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

/// Defines how text should be aligned when a line contains multiple disjoint segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SegmentAlignment {
    /// Align text within the first available segment on the line.
    #[default]
    First,
    /// Align text relative to the total available width of all
    /// segments on the line combined.
    Total,
}

#[derive(Debug, Clone)]
pub struct VerticalMetrics {
    pub advance: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub origin_y: f32,
}

/// Layout-specific font metrics extracted from FontMetrics
/// Contains only the metrics needed for text layout and rendering
#[derive(Debug, Clone)]
pub struct LayoutFontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub units_per_em: u16,
}

impl LayoutFontMetrics {
    pub fn baseline_scaled(&self, font_size: f32) -> f32 {
        let scale = font_size / self.units_per_em as f32;
        self.ascent * scale
    }

    /// Convert from full FontMetrics to layout-specific metrics
    pub fn from_font_metrics(metrics: &azul_css::props::basic::FontMetrics) -> Self {
        Self {
            ascent: metrics.ascender as f32,
            descent: metrics.descender as f32,
            line_gap: metrics.line_gap as f32,
            units_per_em: metrics.units_per_em,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LineSegment {
    pub start_x: f32,
    pub width: f32,
    // For choosing best segment when multiple available
    pub priority: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum TextWrap {
    #[default]
    Wrap,
    Balance,
    NoWrap,
}

// initial-letter
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct InitialLetter {
    /// How many lines tall the initial letter should be.
    pub size: f32,
    /// How many lines the letter should sink into.
    pub sink: u32,
    /// How many characters to apply this styling to.
    pub count: NonZeroUsize,
}

// A type that implements `Hash` must also implement `Eq`.
// Since f32 does not implement `Eq`, we provide a manual implementation.
// This is a marker trait, indicating that `a == b` is a true equivalence
// relation. The derived `PartialEq` already satisfies this.
impl Eq for InitialLetter {}

impl Hash for InitialLetter {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Per the request, round the f32 to a usize for hashing.
        // This is a lossy conversion; values like 2.3 and 2.4 will produce
        // the same hash value for this field. This is acceptable as long as
        // the `PartialEq` implementation correctly distinguishes them.
        (self.size.round() as usize).hash(state);
        self.sink.hash(state);
        self.count.hash(state);
    }
}

// Path and shape definitions
#[derive(Debug, Clone, PartialOrd)]
pub enum PathSegment {
    MoveTo(Point),
    LineTo(Point),
    CurveTo {
        control1: Point,
        control2: Point,
        end: Point,
    },
    QuadTo {
        control: Point,
        end: Point,
    },
    Arc {
        center: Point,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
    },
    Close,
}

// PathSegment
impl Hash for PathSegment {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the enum variant's discriminant first to distinguish them
        discriminant(self).hash(state);

        match self {
            PathSegment::MoveTo(p) => p.hash(state),
            PathSegment::LineTo(p) => p.hash(state),
            PathSegment::CurveTo {
                control1,
                control2,
                end,
            } => {
                control1.hash(state);
                control2.hash(state);
                end.hash(state);
            }
            PathSegment::QuadTo { control, end } => {
                control.hash(state);
                end.hash(state);
            }
            PathSegment::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                center.hash(state);
                (radius.round() as usize).hash(state);
                (start_angle.round() as usize).hash(state);
                (end_angle.round() as usize).hash(state);
            }
            PathSegment::Close => {} // No data to hash
        }
    }
}

impl PartialEq for PathSegment {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (PathSegment::MoveTo(a), PathSegment::MoveTo(b)) => a == b,
            (PathSegment::LineTo(a), PathSegment::LineTo(b)) => a == b,
            (
                PathSegment::CurveTo {
                    control1: c1a,
                    control2: c2a,
                    end: ea,
                },
                PathSegment::CurveTo {
                    control1: c1b,
                    control2: c2b,
                    end: eb,
                },
            ) => c1a == c1b && c2a == c2b && ea == eb,
            (
                PathSegment::QuadTo {
                    control: ca,
                    end: ea,
                },
                PathSegment::QuadTo {
                    control: cb,
                    end: eb,
                },
            ) => ca == cb && ea == eb,
            (
                PathSegment::Arc {
                    center: ca,
                    radius: ra,
                    start_angle: sa_a,
                    end_angle: ea_a,
                },
                PathSegment::Arc {
                    center: cb,
                    radius: rb,
                    start_angle: sa_b,
                    end_angle: ea_b,
                },
            ) => ca == cb && round_eq(*ra, *rb) && round_eq(*sa_a, *sa_b) && round_eq(*ea_a, *ea_b),
            (PathSegment::Close, PathSegment::Close) => true,
            _ => false, // Variants are different
        }
    }
}

impl Eq for PathSegment {}

// Enhanced content model supporting mixed inline content
#[derive(Debug, Clone, Hash)]
pub enum InlineContent {
    Text(StyledRun),
    Image(InlineImage),
    Shape(InlineShape),
    Space(InlineSpace),
    LineBreak(InlineBreak),
    Tab,
    /// List marker (::marker pseudo-element)
    /// Markers with list-style-position: outside are positioned
    /// in the padding gutter of the list container
    Marker {
        run: StyledRun,
        /// Whether marker is positioned outside (in padding) or inside (inline)
        position_outside: bool,
    },
    // Ruby annotation
    Ruby {
        base: Vec<InlineContent>,
        text: Vec<InlineContent>,
        // Style for the ruby text itself
        style: Arc<StyleProperties>,
    },
}

#[derive(Debug, Clone)]
pub struct InlineImage {
    pub source: ImageSource,
    pub intrinsic_size: Size,
    pub display_size: Option<Size>,
    // How much to shift baseline
    pub baseline_offset: f32,
    pub alignment: VerticalAlign,
    pub object_fit: ObjectFit,
}

impl PartialEq for InlineImage {
    fn eq(&self, other: &Self) -> bool {
        self.baseline_offset.to_bits() == other.baseline_offset.to_bits()
            && self.source == other.source
            && self.intrinsic_size == other.intrinsic_size
            && self.display_size == other.display_size
            && self.alignment == other.alignment
            && self.object_fit == other.object_fit
    }
}

impl Eq for InlineImage {}

impl Hash for InlineImage {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.intrinsic_size.hash(state);
        self.display_size.hash(state);
        self.baseline_offset.to_bits().hash(state);
        self.alignment.hash(state);
        self.object_fit.hash(state);
    }
}

impl PartialOrd for InlineImage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InlineImage {
    fn cmp(&self, other: &Self) -> Ordering {
        self.source
            .cmp(&other.source)
            .then_with(|| self.intrinsic_size.cmp(&other.intrinsic_size))
            .then_with(|| self.display_size.cmp(&other.display_size))
            .then_with(|| self.baseline_offset.total_cmp(&other.baseline_offset))
            .then_with(|| self.alignment.cmp(&other.alignment))
            .then_with(|| self.object_fit.cmp(&other.object_fit))
    }
}

/// Enhanced glyph with all features
#[derive(Debug, Clone)]
pub struct Glyph {
    // Core glyph data
    pub glyph_id: u16,
    pub codepoint: char,
    /// Hash of the font - use LoadedFonts to look up the actual font when needed
    pub font_hash: u64,
    /// Cached font metrics to avoid font lookup for common operations
    pub font_metrics: LayoutFontMetrics,
    pub style: Arc<StyleProperties>,
    pub source: GlyphSource,

    // Text mapping
    pub logical_byte_index: usize,
    pub logical_byte_len: usize,
    pub content_index: usize,
    pub cluster: u32,

    // Metrics
    pub advance: f32,
    pub kerning: f32,
    pub offset: Point,

    // Vertical text support
    pub vertical_advance: f32,
    pub vertical_origin_y: f32, // from VORG
    pub vertical_bearing: Point,
    pub orientation: GlyphOrientation,

    // Layout properties
    pub script: Script,
    pub bidi_level: BidiLevel,
}

impl Glyph {
    #[inline]
    fn bounds(&self) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: self.advance,
            height: self.style.line_height,
        }
    }

    #[inline]
    fn character_class(&self) -> CharacterClass {
        classify_character(self.codepoint as u32)
    }

    #[inline]
    fn is_whitespace(&self) -> bool {
        self.character_class() == CharacterClass::Space
    }

    #[inline]
    fn can_justify(&self) -> bool {
        !self.codepoint.is_whitespace() && self.character_class() != CharacterClass::Combining
    }

    #[inline]
    fn justification_priority(&self) -> u8 {
        get_justification_priority(self.character_class())
    }

    #[inline]
    fn break_opportunity_after(&self) -> bool {
        let is_whitespace = self.codepoint.is_whitespace();
        let is_soft_hyphen = self.codepoint == '\u{00AD}';
        is_whitespace || is_soft_hyphen
    }
}

// Information about text runs after initial analysis
#[derive(Debug, Clone)]
pub struct TextRunInfo<'a> {
    pub text: &'a str,
    pub style: Arc<StyleProperties>,
    pub logical_start: usize,
    pub content_index: usize,
}

#[derive(Debug, Clone)]
pub enum ImageSource {
    /// Direct reference to decoded image (from DOM NodeType::Image)
    Ref(ImageRef),
    /// CSS url reference (from background-image, needs ImageCache lookup)
    Url(String),
    /// Raw image data
    Data(Arc<[u8]>),
    /// SVG source
    Svg(Arc<str>),
    /// Placeholder for layout without actual image
    Placeholder(Size),
}

impl PartialEq for ImageSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ImageSource::Ref(a), ImageSource::Ref(b)) => a.get_hash() == b.get_hash(),
            (ImageSource::Url(a), ImageSource::Url(b)) => a == b,
            (ImageSource::Data(a), ImageSource::Data(b)) => Arc::ptr_eq(a, b),
            (ImageSource::Svg(a), ImageSource::Svg(b)) => Arc::ptr_eq(a, b),
            (ImageSource::Placeholder(a), ImageSource::Placeholder(b)) => {
                a.width.to_bits() == b.width.to_bits() && a.height.to_bits() == b.height.to_bits()
            }
            _ => false,
        }
    }
}

impl Eq for ImageSource {}

impl std::hash::Hash for ImageSource {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            ImageSource::Ref(r) => r.get_hash().hash(state),
            ImageSource::Url(s) => s.hash(state),
            ImageSource::Data(d) => (Arc::as_ptr(d) as *const u8 as usize).hash(state),
            ImageSource::Svg(s) => (Arc::as_ptr(s) as *const u8 as usize).hash(state),
            ImageSource::Placeholder(sz) => {
                sz.width.to_bits().hash(state);
                sz.height.to_bits().hash(state);
            }
        }
    }
}

impl PartialOrd for ImageSource {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ImageSource {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn variant_index(s: &ImageSource) -> u8 {
            match s {
                ImageSource::Ref(_) => 0,
                ImageSource::Url(_) => 1,
                ImageSource::Data(_) => 2,
                ImageSource::Svg(_) => 3,
                ImageSource::Placeholder(_) => 4,
            }
        }
        match (self, other) {
            (ImageSource::Ref(a), ImageSource::Ref(b)) => a.get_hash().cmp(&b.get_hash()),
            (ImageSource::Url(a), ImageSource::Url(b)) => a.cmp(b),
            (ImageSource::Data(a), ImageSource::Data(b)) => {
                (Arc::as_ptr(a) as *const u8 as usize).cmp(&(Arc::as_ptr(b) as *const u8 as usize))
            }
            (ImageSource::Svg(a), ImageSource::Svg(b)) => {
                (Arc::as_ptr(a) as *const u8 as usize).cmp(&(Arc::as_ptr(b) as *const u8 as usize))
            }
            (ImageSource::Placeholder(a), ImageSource::Placeholder(b)) => {
                (a.width.to_bits(), a.height.to_bits())
                    .cmp(&(b.width.to_bits(), b.height.to_bits()))
            }
            // Different variants: compare by variant index
            _ => variant_index(self).cmp(&variant_index(other)),
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum VerticalAlign {
    // Align image baseline with text baseline
    #[default]
    Baseline,
    // Align image bottom with line bottom
    Bottom,
    // Align image top with line top
    Top,
    // Align image middle with text middle
    Middle,
    // Align with tallest text in line
    TextTop,
    // Align with lowest text in line
    TextBottom,
    // Subscript alignment
    Sub,
    // Superscript alignment
    Super,
    // Custom offset from baseline
    Offset(f32),
}

impl std::hash::Hash for VerticalAlign {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        if let VerticalAlign::Offset(f) = self {
            f.to_bits().hash(state);
        }
    }
}

impl Eq for VerticalAlign {}

impl Ord for VerticalAlign {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap_or(std::cmp::Ordering::Equal)
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectFit {
    // Stretch to fit display size
    Fill,
    // Scale to fit within display size
    Contain,
    // Scale to cover display size
    Cover,
    // Use intrinsic size
    None,
    // Like contain but never scale up
    ScaleDown,
}

/// Border information for inline elements (display: inline, inline-block)
///
/// This stores the resolved border properties needed for rendering inline element borders.
/// Unlike block elements which render borders via paint_node_background_and_border(),
/// inline element borders must be rendered per glyph-run to handle line breaks correctly.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineBorderInfo {
    /// Border widths in pixels for each side
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
    /// Border colors for each side
    pub top_color: ColorU,
    pub right_color: ColorU,
    pub bottom_color: ColorU,
    pub left_color: ColorU,
    /// Border radius (if any)
    pub radius: Option<f32>,
}

impl Default for InlineBorderInfo {
    fn default() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
            top_color: ColorU::TRANSPARENT,
            right_color: ColorU::TRANSPARENT,
            bottom_color: ColorU::TRANSPARENT,
            left_color: ColorU::TRANSPARENT,
            radius: None,
        }
    }
}

impl InlineBorderInfo {
    /// Returns true if any border has a non-zero width
    pub fn has_border(&self) -> bool {
        self.top > 0.0 || self.right > 0.0 || self.bottom > 0.0 || self.left > 0.0
    }
}

#[derive(Debug, Clone)]
pub struct InlineShape {
    pub shape_def: ShapeDefinition,
    pub fill: Option<ColorU>,
    pub stroke: Option<Stroke>,
    pub baseline_offset: f32,
    /// The NodeId of the element that created this shape
    /// (e.g., inline-block) - this allows us to look up
    /// styling information (background, border) when rendering
    pub source_node_id: Option<azul_core::dom::NodeId>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OverflowBehavior {
    // Content extends outside shape
    Visible,
    // Content is clipped to shape
    Hidden,
    // Scrollable overflow
    Scroll,
    // Browser/system decides
    #[default]
    Auto,
    // Break into next shape/page
    Break,
}

#[derive(Debug, Clone)]
pub struct MeasuredImage {
    pub source: ImageSource,
    pub size: Size,
    pub baseline_offset: f32,
    pub alignment: VerticalAlign,
    pub content_index: usize,
}

#[derive(Debug, Clone)]
pub struct MeasuredShape {
    pub shape_def: ShapeDefinition,
    pub size: Size,
    pub baseline_offset: f32,
    pub content_index: usize,
}

#[derive(Debug, Clone)]
pub struct InlineSpace {
    pub width: f32,
    pub is_breaking: bool, // Can line break here
    pub is_stretchy: bool, // Can be expanded for justification
}

impl PartialEq for InlineSpace {
    fn eq(&self, other: &Self) -> bool {
        self.width.to_bits() == other.width.to_bits()
            && self.is_breaking == other.is_breaking
            && self.is_stretchy == other.is_stretchy
    }
}

impl Eq for InlineSpace {}

impl Hash for InlineSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.width.to_bits().hash(state);
        self.is_breaking.hash(state);
        self.is_stretchy.hash(state);
    }
}

impl PartialOrd for InlineSpace {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InlineSpace {
    fn cmp(&self, other: &Self) -> Ordering {
        self.width
            .total_cmp(&other.width)
            .then_with(|| self.is_breaking.cmp(&other.is_breaking))
            .then_with(|| self.is_stretchy.cmp(&other.is_stretchy))
    }
}

impl PartialEq for InlineShape {
    fn eq(&self, other: &Self) -> bool {
        self.baseline_offset.to_bits() == other.baseline_offset.to_bits()
            && self.shape_def == other.shape_def
            && self.fill == other.fill
            && self.stroke == other.stroke
            && self.source_node_id == other.source_node_id
    }
}

impl Eq for InlineShape {}

impl Hash for InlineShape {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.shape_def.hash(state);
        self.fill.hash(state);
        self.stroke.hash(state);
        self.baseline_offset.to_bits().hash(state);
        self.source_node_id.hash(state);
    }
}

impl PartialOrd for InlineShape {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(
            self.shape_def
                .partial_cmp(&other.shape_def)?
                .then_with(|| self.fill.cmp(&other.fill))
                .then_with(|| {
                    self.stroke
                        .partial_cmp(&other.stroke)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| self.baseline_offset.total_cmp(&other.baseline_offset))
                .then_with(|| self.source_node_id.cmp(&other.source_node_id)),
        )
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PartialEq for Rect {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.x, other.x)
            && round_eq(self.y, other.y)
            && round_eq(self.width, other.width)
            && round_eq(self.height, other.height)
    }
}
impl Eq for Rect {}

impl Hash for Rect {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // The order in which you hash the fields matters.
        // A consistent order is crucial.
        (self.x.round() as usize).hash(state);
        (self.y.round() as usize).hash(state);
        (self.width.round() as usize).hash(state);
        (self.height.round() as usize).hash(state);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialOrd)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Ord for Size {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.width.round() as usize)
            .cmp(&(other.width.round() as usize))
            .then_with(|| (self.height.round() as usize).cmp(&(other.height.round() as usize)))
    }
}

// Size
impl Hash for Size {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.width.round() as usize).hash(state);
        (self.height.round() as usize).hash(state);
    }
}
impl PartialEq for Size {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.width, other.width) && round_eq(self.height, other.height)
    }
}
impl Eq for Size {}

impl Size {
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialOrd)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

// Point
impl Hash for Point {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.x.round() as usize).hash(state);
        (self.y.round() as usize).hash(state);
    }
}

impl PartialEq for Point {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.x, other.x) && round_eq(self.y, other.y)
    }
}

impl Eq for Point {}

#[derive(Debug, Clone, PartialOrd)]
pub enum ShapeDefinition {
    Rectangle {
        size: Size,
        corner_radius: Option<f32>,
    },
    Circle {
        radius: f32,
    },
    Ellipse {
        radii: Size,
    },
    Polygon {
        points: Vec<Point>,
    },
    Path {
        segments: Vec<PathSegment>,
    },
}

// ShapeDefinition
impl Hash for ShapeDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            ShapeDefinition::Rectangle {
                size,
                corner_radius,
            } => {
                size.hash(state);
                corner_radius.map(|r| r.round() as usize).hash(state);
            }
            ShapeDefinition::Circle { radius } => {
                (radius.round() as usize).hash(state);
            }
            ShapeDefinition::Ellipse { radii } => {
                radii.hash(state);
            }
            ShapeDefinition::Polygon { points } => {
                // Since Point implements Hash, we can hash the Vec directly.
                points.hash(state);
            }
            ShapeDefinition::Path { segments } => {
                // Same for Vec<PathSegment>
                segments.hash(state);
            }
        }
    }
}

impl PartialEq for ShapeDefinition {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                ShapeDefinition::Rectangle {
                    size: s1,
                    corner_radius: r1,
                },
                ShapeDefinition::Rectangle {
                    size: s2,
                    corner_radius: r2,
                },
            ) => {
                s1 == s2
                    && match (r1, r2) {
                        (None, None) => true,
                        (Some(v1), Some(v2)) => round_eq(*v1, *v2),
                        _ => false,
                    }
            }
            (ShapeDefinition::Circle { radius: r1 }, ShapeDefinition::Circle { radius: r2 }) => {
                round_eq(*r1, *r2)
            }
            (ShapeDefinition::Ellipse { radii: r1 }, ShapeDefinition::Ellipse { radii: r2 }) => {
                r1 == r2
            }
            (ShapeDefinition::Polygon { points: p1 }, ShapeDefinition::Polygon { points: p2 }) => {
                p1 == p2
            }
            (ShapeDefinition::Path { segments: s1 }, ShapeDefinition::Path { segments: s2 }) => {
                s1 == s2
            }
            _ => false,
        }
    }
}
impl Eq for ShapeDefinition {}

impl ShapeDefinition {
    /// Calculates the bounding box size for the shape.
    pub fn get_size(&self) -> Size {
        match self {
            // The size is explicitly defined.
            ShapeDefinition::Rectangle { size, .. } => *size,

            // The bounding box of a circle is a square with sides equal to the diameter.
            ShapeDefinition::Circle { radius } => {
                let diameter = radius * 2.0;
                Size::new(diameter, diameter)
            }

            // The bounding box of an ellipse has width and height equal to twice its radii.
            ShapeDefinition::Ellipse { radii } => Size::new(radii.width * 2.0, radii.height * 2.0),

            // For a polygon, we must find the min/max coordinates to get the bounds.
            ShapeDefinition::Polygon { points } => calculate_bounding_box_size(points),

            // For a path, we find the bounding box of all its anchor and control points.
            //
            // NOTE: This is a common and fast approximation. The true bounding box of
            // bezier curves can be slightly smaller than the box containing their control
            // points. For pixel-perfect results, one would need to calculate the
            // curve's extrema.
            ShapeDefinition::Path { segments } => {
                let mut points = Vec::new();
                let mut current_pos = Point { x: 0.0, y: 0.0 };

                for segment in segments {
                    match segment {
                        PathSegment::MoveTo(p) | PathSegment::LineTo(p) => {
                            points.push(*p);
                            current_pos = *p;
                        }
                        PathSegment::QuadTo { control, end } => {
                            points.push(current_pos);
                            points.push(*control);
                            points.push(*end);
                            current_pos = *end;
                        }
                        PathSegment::CurveTo {
                            control1,
                            control2,
                            end,
                        } => {
                            points.push(current_pos);
                            points.push(*control1);
                            points.push(*control2);
                            points.push(*end);
                            current_pos = *end;
                        }
                        PathSegment::Arc {
                            center,
                            radius,
                            start_angle,
                            end_angle,
                        } => {
                            // 1. Calculate and add the arc's start and end points to the list.
                            let start_point = Point {
                                x: center.x + radius * start_angle.cos(),
                                y: center.y + radius * start_angle.sin(),
                            };
                            let end_point = Point {
                                x: center.x + radius * end_angle.cos(),
                                y: center.y + radius * end_angle.sin(),
                            };
                            points.push(start_point);
                            points.push(end_point);

                            // 2. Normalize the angles to handle cases where the arc crosses the
                            //    0-radian line.
                            // This ensures we can iterate forward from a start to an end angle.
                            let mut normalized_end = *end_angle;
                            while normalized_end < *start_angle {
                                normalized_end += 2.0 * std::f32::consts::PI;
                            }

                            // 3. Find the first cardinal point (multiples of PI/2) at or after the
                            //    start angle.
                            let mut check_angle = (*start_angle / std::f32::consts::FRAC_PI_2)
                                .ceil()
                                * std::f32::consts::FRAC_PI_2;

                            // 4. Iterate through all cardinal points that fall within the arc's
                            //    sweep and add them.
                            // These points define the maximum extent of the arc's bounding box.
                            while check_angle < normalized_end {
                                points.push(Point {
                                    x: center.x + radius * check_angle.cos(),
                                    y: center.y + radius * check_angle.sin(),
                                });
                                check_angle += std::f32::consts::FRAC_PI_2;
                            }

                            // 5. The end of the arc is the new current position for subsequent path
                            //    segments.
                            current_pos = end_point;
                        }
                        PathSegment::Close => {
                            // No new points are added for closing the path
                        }
                    }
                }
                calculate_bounding_box_size(&points)
            }
        }
    }
}

/// Helper function to calculate the size of the bounding box enclosing a set of points.
fn calculate_bounding_box_size(points: &[Point]) -> Size {
    if points.is_empty() {
        return Size::zero();
    }

    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;

    for point in points {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }

    // Handle case where points might be collinear or a single point
    if min_x > max_x || min_y > max_y {
        return Size::zero();
    }

    Size::new(max_x - min_x, max_y - min_y)
}

#[derive(Debug, Clone, PartialOrd)]
pub struct Stroke {
    pub color: ColorU,
    pub width: f32,
    pub dash_pattern: Option<Vec<f32>>,
}

// Stroke
impl Hash for Stroke {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.color.hash(state);
        (self.width.round() as usize).hash(state);

        // Manual hashing for Option<Vec<f32>>
        match &self.dash_pattern {
            None => 0u8.hash(state), // Hash a discriminant for None
            Some(pattern) => {
                1u8.hash(state); // Hash a discriminant for Some
                pattern.len().hash(state); // Hash the length
                for &val in pattern {
                    (val.round() as usize).hash(state); // Hash each rounded value
                }
            }
        }
    }
}

impl PartialEq for Stroke {
    fn eq(&self, other: &Self) -> bool {
        if self.color != other.color || !round_eq(self.width, other.width) {
            return false;
        }
        match (&self.dash_pattern, &other.dash_pattern) {
            (None, None) => true,
            (Some(p1), Some(p2)) => {
                p1.len() == p2.len() && p1.iter().zip(p2.iter()).all(|(a, b)| round_eq(*a, *b))
            }
            _ => false,
        }
    }
}

impl Eq for Stroke {}

// Helper function to round f32 for comparison
fn round_eq(a: f32, b: f32) -> bool {
    (a.round() as isize) == (b.round() as isize)
}

#[derive(Debug, Clone)]
pub enum ShapeBoundary {
    Rectangle(Rect),
    Circle { center: Point, radius: f32 },
    Ellipse { center: Point, radii: Size },
    Polygon { points: Vec<Point> },
    Path { segments: Vec<PathSegment> },
}

impl ShapeBoundary {
    pub fn inflate(&self, margin: f32) -> Self {
        if margin == 0.0 {
            return self.clone();
        }
        match self {
            Self::Rectangle(rect) => Self::Rectangle(Rect {
                x: rect.x - margin,
                y: rect.y - margin,
                width: (rect.width + margin * 2.0).max(0.0),
                height: (rect.height + margin * 2.0).max(0.0),
            }),
            Self::Circle { center, radius } => Self::Circle {
                center: *center,
                radius: radius + margin,
            },
            // For simplicity, Polygon and Path inflation is not implemented here.
            // A full implementation would require a geometry library to offset the path.
            _ => self.clone(),
        }
    }
}

// ShapeBoundary
impl Hash for ShapeBoundary {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            ShapeBoundary::Rectangle(rect) => rect.hash(state),
            ShapeBoundary::Circle { center, radius } => {
                center.hash(state);
                (radius.round() as usize).hash(state);
            }
            ShapeBoundary::Ellipse { center, radii } => {
                center.hash(state);
                radii.hash(state);
            }
            ShapeBoundary::Polygon { points } => points.hash(state),
            ShapeBoundary::Path { segments } => segments.hash(state),
        }
    }
}
impl PartialEq for ShapeBoundary {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ShapeBoundary::Rectangle(r1), ShapeBoundary::Rectangle(r2)) => r1 == r2,
            (
                ShapeBoundary::Circle {
                    center: c1,
                    radius: r1,
                },
                ShapeBoundary::Circle {
                    center: c2,
                    radius: r2,
                },
            ) => c1 == c2 && round_eq(*r1, *r2),
            (
                ShapeBoundary::Ellipse {
                    center: c1,
                    radii: r1,
                },
                ShapeBoundary::Ellipse {
                    center: c2,
                    radii: r2,
                },
            ) => c1 == c2 && r1 == r2,
            (ShapeBoundary::Polygon { points: p1 }, ShapeBoundary::Polygon { points: p2 }) => {
                p1 == p2
            }
            (ShapeBoundary::Path { segments: s1 }, ShapeBoundary::Path { segments: s2 }) => {
                s1 == s2
            }
            _ => false,
        }
    }
}
impl Eq for ShapeBoundary {}

impl ShapeBoundary {
    /// Converts a CSS shape (from azul-css) to a layout engine ShapeBoundary
    ///
    /// # Arguments
    /// * `css_shape` - The parsed CSS shape from azul-css
    /// * `reference_box` - The containing box for resolving coordinates (from layout solver)
    ///
    /// # Returns
    /// A ShapeBoundary ready for use in the text layout engine
    pub fn from_css_shape(
        css_shape: &azul_css::shape::CssShape,
        reference_box: Rect,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Self {
        use azul_css::shape::CssShape;

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Input CSS shape: {:?}",
                css_shape
            )));
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Reference box: {:?}",
                reference_box
            )));
        }

        let result = match css_shape {
            CssShape::Circle(circle) => {
                let center = Point {
                    x: reference_box.x + circle.center.x,
                    y: reference_box.y + circle.center.y,
                };
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Circle - CSS center: ({}, {}), radius: {}",
                        circle.center.x, circle.center.y, circle.radius
                    )));
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Circle - Absolute center: ({}, {}), \
                         radius: {}",
                        center.x, center.y, circle.radius
                    )));
                }
                ShapeBoundary::Circle {
                    center,
                    radius: circle.radius,
                }
            }

            CssShape::Ellipse(ellipse) => {
                let center = Point {
                    x: reference_box.x + ellipse.center.x,
                    y: reference_box.y + ellipse.center.y,
                };
                let radii = Size {
                    width: ellipse.radius_x,
                    height: ellipse.radius_y,
                };
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Ellipse - center: ({}, {}), radii: ({}, \
                         {})",
                        center.x, center.y, radii.width, radii.height
                    )));
                }
                ShapeBoundary::Ellipse { center, radii }
            }

            CssShape::Polygon(polygon) => {
                let points = polygon
                    .points
                    .as_ref()
                    .iter()
                    .map(|pt| Point {
                        x: reference_box.x + pt.x,
                        y: reference_box.y + pt.y,
                    })
                    .collect();
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Polygon - {} points",
                        polygon.points.as_ref().len()
                    )));
                }
                ShapeBoundary::Polygon { points }
            }

            CssShape::Inset(inset) => {
                // Inset defines distances from reference box edges
                let x = reference_box.x + inset.inset_left;
                let y = reference_box.y + inset.inset_top;
                let width = reference_box.width - inset.inset_left - inset.inset_right;
                let height = reference_box.height - inset.inset_top - inset.inset_bottom;

                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Inset - insets: ({}, {}, {}, {})",
                        inset.inset_top, inset.inset_right, inset.inset_bottom, inset.inset_left
                    )));
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Inset - resulting rect: x={}, y={}, \
                         w={}, h={}",
                        x, y, width, height
                    )));
                }

                ShapeBoundary::Rectangle(Rect {
                    x,
                    y,
                    width: width.max(0.0),
                    height: height.max(0.0),
                })
            }

            CssShape::Path(path) => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "[ShapeBoundary::from_css_shape] Path - fallback to rectangle".to_string(),
                    ));
                }
                // TODO: Parse SVG path data into PathSegments
                // For now, fall back to rectangle
                ShapeBoundary::Rectangle(reference_box)
            }
        };

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Result: {:?}",
                result
            )));
        }
        result
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InlineBreak {
    pub break_type: BreakType,
    pub clear: ClearType,
    pub content_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BreakType {
    Soft,   // Preferred break (like <wbr>)
    Hard,   // Forced break (like <br>)
    Page,   // Page break
    Column, // Column break
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClearType {
    None,
    Left,
    Right,
    Both,
}

// Complex shape constraints for non-rectangular text flow
#[derive(Debug, Clone)]
pub struct ShapeConstraints {
    pub boundaries: Vec<ShapeBoundary>,
    pub exclusions: Vec<ShapeBoundary>,
    pub writing_mode: WritingMode,
    pub text_align: TextAlign,
    pub line_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub enum WritingMode {
    #[default]
    HorizontalTb, // horizontal-tb (normal horizontal)
    VerticalRl, // vertical-rl (vertical right-to-left)
    VerticalLr, // vertical-lr (vertical left-to-right)
    SidewaysRl, // sideways-rl (rotated horizontal in vertical context)
    SidewaysLr, // sideways-lr (rotated horizontal in vertical context)
}

impl WritingMode {
    /// Necessary to determine if the glyphs are advancing in a horizontal direction
    pub fn is_advance_horizontal(&self) -> bool {
        matches!(
            self,
            WritingMode::HorizontalTb | WritingMode::SidewaysRl | WritingMode::SidewaysLr
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub enum JustifyContent {
    #[default]
    None,
    InterWord,      // Expand spaces between words
    InterCharacter, // Expand spaces between all characters (for CJK)
    Distribute,     // Distribute space evenly including start/end
    Kashida,        // Stretch Arabic text using kashidas
}

// Enhanced text alignment with logical directions
#[derive(Debug, Clone, Copy, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub enum TextAlign {
    #[default]
    Left,
    Right,
    Center,
    Justify,
    Start,
    End,        // Logical start/end
    JustifyAll, // Justify including last line
}

// Vertical text orientation for individual characters
#[derive(Debug, Clone, Copy, PartialEq, Default, Eq, PartialOrd, Ord, Hash)]
pub enum TextOrientation {
    #[default]
    Mixed, // Default: upright for scripts, rotated for others
    Upright,  // All characters upright
    Sideways, // All characters rotated 90 degrees
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextDecoration {
    pub underline: bool,
    pub strikethrough: bool,
    pub overline: bool,
}

impl Default for TextDecoration {
    fn default() -> Self {
        TextDecoration {
            underline: false,
            overline: false,
            strikethrough: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum TextTransform {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

// Type alias for OpenType feature tags
pub type FourCc = [u8; 4];

// Enum for relative or absolute spacing
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Spacing {
    Px(i32), // Use integer pixels to simplify hashing and equality
    Em(f32),
}

// A type that implements `Hash` must also implement `Eq`.
// Since f32 does not implement `Eq`, we provide a manual implementation.
// The derived `PartialEq` is sufficient for this marker trait.
impl Eq for Spacing {}

impl Hash for Spacing {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // First, hash the enum variant to distinguish between Px and Em.
        discriminant(self).hash(state);
        match self {
            Spacing::Px(val) => val.hash(state),
            // For hashing floats, convert them to their raw bit representation.
            // This ensures that identical float values produce identical hashes.
            Spacing::Em(val) => val.to_bits().hash(state),
        }
    }
}

impl Default for Spacing {
    fn default() -> Self {
        Spacing::Px(0)
    }
}

impl Default for FontHash {
    fn default() -> Self {
        Self::invalid()
    }
}

/// Style properties with vertical text support
#[derive(Debug, Clone, PartialEq)]
pub struct StyleProperties {
    /// Font stack for fallback support (priority order)
    /// Can be either a list of FontSelectors (resolved via fontconfig)
    /// or a direct FontRef (bypasses fontconfig entirely).
    pub font_stack: FontStack,
    pub font_size_px: f32,
    pub color: ColorU,
    /// Background color for inline elements (e.g., `<span style="background-color: yellow">`)
    ///
    /// This is propagated from CSS through the style system and eventually used by
    /// the PDF renderer to draw filled rectangles behind text. The value is `None`
    /// for transparent backgrounds (the default).
    ///
    /// The propagation chain is:
    /// CSS -> `get_style_properties()` -> `StyleProperties` -> `ShapedGlyph` -> `PdfGlyphRun`
    ///
    /// See `PdfGlyphRun::background_color` for how this is used in PDF rendering.
    pub background_color: Option<ColorU>,
    /// Full background content layers (for gradients, images, etc.)
    /// This extends background_color to support CSS gradients on inline elements.
    pub background_content: Vec<StyleBackgroundContent>,
    /// Border information for inline elements
    pub border: Option<InlineBorderInfo>,
    pub letter_spacing: Spacing,
    pub word_spacing: Spacing,

    pub line_height: f32,
    pub text_decoration: TextDecoration,

    // Represents CSS font-feature-settings like `"liga"`, `"smcp=1"`.
    pub font_features: Vec<String>,

    // Variable fonts
    pub font_variations: Vec<(FourCc, f32)>,
    // Multiplier of the space width
    pub tab_size: f32,
    // text-transform
    pub text_transform: TextTransform,
    // Vertical text properties
    pub writing_mode: WritingMode,
    pub text_orientation: TextOrientation,
    // Tate-chu-yoko
    pub text_combine_upright: Option<TextCombineUpright>,

    // Variant handling
    pub font_variant_caps: FontVariantCaps,
    pub font_variant_numeric: FontVariantNumeric,
    pub font_variant_ligatures: FontVariantLigatures,
    pub font_variant_east_asian: FontVariantEastAsian,
}

impl Default for StyleProperties {
    fn default() -> Self {
        const FONT_SIZE: f32 = 16.0;
        const TAB_SIZE: f32 = 8.0;
        Self {
            font_stack: FontStack::default(),
            font_size_px: FONT_SIZE,
            color: ColorU::default(),
            background_color: None,
            background_content: Vec::new(),
            border: None,
            letter_spacing: Spacing::default(), // Px(0)
            word_spacing: Spacing::default(),   // Px(0)
            line_height: FONT_SIZE * 1.2,
            text_decoration: TextDecoration::default(),
            font_features: Vec::new(),
            font_variations: Vec::new(),
            tab_size: TAB_SIZE, // CSS default
            text_transform: TextTransform::default(),
            writing_mode: WritingMode::default(),
            text_orientation: TextOrientation::default(),
            text_combine_upright: None,
            font_variant_caps: FontVariantCaps::default(),
            font_variant_numeric: FontVariantNumeric::default(),
            font_variant_ligatures: FontVariantLigatures::default(),
            font_variant_east_asian: FontVariantEastAsian::default(),
        }
    }
}

impl Hash for StyleProperties {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_stack.hash(state);
        self.color.hash(state);
        self.background_color.hash(state);
        self.text_decoration.hash(state);
        self.font_features.hash(state);
        self.writing_mode.hash(state);
        self.text_orientation.hash(state);
        self.text_combine_upright.hash(state);
        self.letter_spacing.hash(state);
        self.word_spacing.hash(state);

        // For f32 fields, round and cast to usize before hashing.
        (self.font_size_px.round() as usize).hash(state);
        (self.line_height.round() as usize).hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
pub enum TextCombineUpright {
    None,
    All,        // Combine all characters in horizontal layout
    Digits(u8), // Combine up to N digits
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlyphSource {
    /// Glyph generated from a character in the source text.
    Char,
    /// Glyph inserted dynamically by the layout engine (e.g., a hyphen).
    Hyphen,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CharacterClass {
    Space,       // Regular spaces - highest justification priority
    Punctuation, // Can sometimes be adjusted
    Letter,      // Normal letters
    Ideograph,   // CJK characters - can be justified between
    Symbol,      // Symbols, emojis
    Combining,   // Combining marks - never justified
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphOrientation {
    Horizontal, // Keep horizontal (normal in horizontal text)
    Vertical,   // Rotate to vertical (normal in vertical text)
    Upright,    // Keep upright regardless of writing mode
    Mixed,      // Use script-specific default orientation
}

// Bidi and script detection
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BidiDirection {
    Ltr,
    Rtl,
}

impl BidiDirection {
    pub fn is_rtl(&self) -> bool {
        matches!(self, BidiDirection::Rtl)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantCaps {
    #[default]
    Normal,
    SmallCaps,
    AllSmallCaps,
    PetiteCaps,
    AllPetiteCaps,
    Unicase,
    TitlingCaps,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantNumeric {
    #[default]
    Normal,
    LiningNums,
    OldstyleNums,
    ProportionalNums,
    TabularNums,
    DiagonalFractions,
    StackedFractions,
    Ordinal,
    SlashedZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantLigatures {
    #[default]
    Normal,
    None,
    Common,
    NoCommon,
    Discretionary,
    NoDiscretionary,
    Historical,
    NoHistorical,
    Contextual,
    NoContextual,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantEastAsian {
    #[default]
    Normal,
    Jis78,
    Jis83,
    Jis90,
    Jis04,
    Simplified,
    Traditional,
    FullWidth,
    ProportionalWidth,
    Ruby,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BidiLevel(u8);

impl BidiLevel {
    pub fn new(level: u8) -> Self {
        Self(level)
    }
    pub fn is_rtl(&self) -> bool {
        self.0 % 2 == 1
    }
    pub fn level(&self) -> u8 {
        self.0
    }
}

// Add this new struct for style overrides
#[derive(Debug, Clone)]
pub struct StyleOverride {
    /// The specific character this override applies to.
    pub target: ContentIndex,
    /// The style properties to apply.
    /// Any `None` value means "inherit from the base style".
    pub style: PartialStyleProperties,
}

#[derive(Debug, Clone, Default)]
pub struct PartialStyleProperties {
    pub font_stack: Option<FontStack>,
    pub font_size_px: Option<f32>,
    pub color: Option<ColorU>,
    pub letter_spacing: Option<Spacing>,
    pub word_spacing: Option<Spacing>,
    pub line_height: Option<f32>,
    pub text_decoration: Option<TextDecoration>,
    pub font_features: Option<Vec<String>>,
    pub font_variations: Option<Vec<(FourCc, f32)>>,
    pub tab_size: Option<f32>,
    pub text_transform: Option<TextTransform>,
    pub writing_mode: Option<WritingMode>,
    pub text_orientation: Option<TextOrientation>,
    pub text_combine_upright: Option<Option<TextCombineUpright>>,
    pub font_variant_caps: Option<FontVariantCaps>,
    pub font_variant_numeric: Option<FontVariantNumeric>,
    pub font_variant_ligatures: Option<FontVariantLigatures>,
    pub font_variant_east_asian: Option<FontVariantEastAsian>,
}

impl Hash for PartialStyleProperties {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_stack.hash(state);
        self.font_size_px.map(|f| f.to_bits()).hash(state);
        self.color.hash(state);
        self.letter_spacing.hash(state);
        self.word_spacing.hash(state);
        self.line_height.map(|f| f.to_bits()).hash(state);
        self.text_decoration.hash(state);
        self.font_features.hash(state);

        // Manual hashing for Vec<(FourCc, f32)>
        self.font_variations.as_ref().map(|v| {
            for (tag, val) in v {
                tag.hash(state);
                val.to_bits().hash(state);
            }
        });

        self.tab_size.map(|f| f.to_bits()).hash(state);
        self.text_transform.hash(state);
        self.writing_mode.hash(state);
        self.text_orientation.hash(state);
        self.text_combine_upright.hash(state);
        self.font_variant_caps.hash(state);
        self.font_variant_numeric.hash(state);
        self.font_variant_ligatures.hash(state);
        self.font_variant_east_asian.hash(state);
    }
}

impl PartialEq for PartialStyleProperties {
    fn eq(&self, other: &Self) -> bool {
        self.font_stack == other.font_stack &&
        self.font_size_px.map(|f| f.to_bits()) == other.font_size_px.map(|f| f.to_bits()) &&
        self.color == other.color &&
        self.letter_spacing == other.letter_spacing &&
        self.word_spacing == other.word_spacing &&
        self.line_height.map(|f| f.to_bits()) == other.line_height.map(|f| f.to_bits()) &&
        self.text_decoration == other.text_decoration &&
        self.font_features == other.font_features &&
        self.font_variations == other.font_variations && // Vec<(FourCc, f32)> is PartialEq
        self.tab_size.map(|f| f.to_bits()) == other.tab_size.map(|f| f.to_bits()) &&
        self.text_transform == other.text_transform &&
        self.writing_mode == other.writing_mode &&
        self.text_orientation == other.text_orientation &&
        self.text_combine_upright == other.text_combine_upright &&
        self.font_variant_caps == other.font_variant_caps &&
        self.font_variant_numeric == other.font_variant_numeric &&
        self.font_variant_ligatures == other.font_variant_ligatures &&
        self.font_variant_east_asian == other.font_variant_east_asian
    }
}

impl Eq for PartialStyleProperties {}

impl StyleProperties {
    fn apply_override(&self, partial: &PartialStyleProperties) -> Self {
        let mut new_style = self.clone();
        if let Some(val) = &partial.font_stack {
            new_style.font_stack = val.clone();
        }
        if let Some(val) = partial.font_size_px {
            new_style.font_size_px = val;
        }
        if let Some(val) = &partial.color {
            new_style.color = val.clone();
        }
        if let Some(val) = partial.letter_spacing {
            new_style.letter_spacing = val;
        }
        if let Some(val) = partial.word_spacing {
            new_style.word_spacing = val;
        }
        if let Some(val) = partial.line_height {
            new_style.line_height = val;
        }
        if let Some(val) = &partial.text_decoration {
            new_style.text_decoration = val.clone();
        }
        if let Some(val) = &partial.font_features {
            new_style.font_features = val.clone();
        }
        if let Some(val) = &partial.font_variations {
            new_style.font_variations = val.clone();
        }
        if let Some(val) = partial.tab_size {
            new_style.tab_size = val;
        }
        if let Some(val) = partial.text_transform {
            new_style.text_transform = val;
        }
        if let Some(val) = partial.writing_mode {
            new_style.writing_mode = val;
        }
        if let Some(val) = partial.text_orientation {
            new_style.text_orientation = val;
        }
        if let Some(val) = &partial.text_combine_upright {
            new_style.text_combine_upright = val.clone();
        }
        if let Some(val) = partial.font_variant_caps {
            new_style.font_variant_caps = val;
        }
        if let Some(val) = partial.font_variant_numeric {
            new_style.font_variant_numeric = val;
        }
        if let Some(val) = partial.font_variant_ligatures {
            new_style.font_variant_ligatures = val;
        }
        if let Some(val) = partial.font_variant_east_asian {
            new_style.font_variant_east_asian = val;
        }
        new_style
    }
}

/// The kind of a glyph, used to distinguish characters from layout-inserted items.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphKind {
    /// A standard glyph representing one or more characters from the source text.
    Character,
    /// A hyphen glyph inserted by the line breaking algorithm.
    Hyphen,
    /// A `.notdef` glyph, indicating a character that could not be found in any font.
    NotDef,
    /// A Kashida justification glyph, inserted to stretch Arabic text.
    Kashida {
        /// The target width of the kashida.
        width: f32,
    },
}

// --- Stage 1: Logical Representation ---

#[derive(Debug, Clone)]
pub enum LogicalItem {
    Text {
        /// A stable ID pointing back to the original source character.
        source: ContentIndex,
        /// The text of this specific logical item (often a single grapheme cluster).
        text: String,
        style: Arc<StyleProperties>,
        /// If this text is a list marker: whether it should be positioned outside
        /// (in the padding gutter) or inside (inline with content).
        /// None for non-marker content.
        marker_position_outside: Option<bool>,
        /// The DOM NodeId of the Text node this item originated from.
        /// None for generated content (list markers, ::before/::after, etc.)
        source_node_id: Option<NodeId>,
    },
    /// Tate-chu-yoko: Run of text to be laid out horizontally within a vertical context.
    CombinedText {
        source: ContentIndex,
        text: String,
        style: Arc<StyleProperties>,
    },
    Ruby {
        source: ContentIndex,
        // For the stub, we simplify to strings. A full implementation
        // would need to handle Vec<LogicalItem> for both.
        base_text: String,
        ruby_text: String,
        style: Arc<StyleProperties>,
    },
    Object {
        /// A stable ID pointing back to the original source object.
        source: ContentIndex,
        /// The original non-text object.
        content: InlineContent,
    },
    Tab {
        source: ContentIndex,
        style: Arc<StyleProperties>,
    },
    Break {
        source: ContentIndex,
        break_info: InlineBreak,
    },
}

impl Hash for LogicalItem {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            LogicalItem::Text {
                source,
                text,
                style,
                marker_position_outside,
                source_node_id,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state); // Hash the content, not the Arc pointer
                marker_position_outside.hash(state);
                source_node_id.hash(state);
            }
            LogicalItem::CombinedText {
                source,
                text,
                style,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Ruby {
                source,
                base_text,
                ruby_text,
                style,
            } => {
                source.hash(state);
                base_text.hash(state);
                ruby_text.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Object { source, content } => {
                source.hash(state);
                content.hash(state);
            }
            LogicalItem::Tab { source, style } => {
                source.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Break { source, break_info } => {
                source.hash(state);
                break_info.hash(state);
            }
        }
    }
}

// --- Stage 2: Visual Representation ---

#[derive(Debug, Clone)]
pub struct VisualItem {
    /// A reference to the logical item this visual item originated from.
    /// A single LogicalItem can be split into multiple VisualItems.
    pub logical_source: LogicalItem,
    /// The Bidi embedding level for this item.
    pub bidi_level: BidiLevel,
    /// The script detected for this run, crucial for shaping.
    pub script: Script,
    /// The text content for this specific visual run.
    pub text: String,
}

// --- Stage 3: Shaped Representation ---

#[derive(Debug, Clone)]
pub enum ShapedItem {
    Cluster(ShapedCluster),
    /// A block of combined text (tate-chu-yoko) that is laid out
    // as a single unbreakable object.
    CombinedBlock {
        source: ContentIndex,
        /// The glyphs to be rendered horizontally within the vertical line.
        glyphs: Vec<ShapedGlyph>,
        bounds: Rect,
        baseline_offset: f32,
    },
    Object {
        source: ContentIndex,
        bounds: Rect,
        baseline_offset: f32,
        // Store original object for rendering
        content: InlineContent,
    },
    Tab {
        source: ContentIndex,
        bounds: Rect,
    },
    Break {
        source: ContentIndex,
        break_info: InlineBreak,
    },
}

impl ShapedItem {
    pub fn as_cluster(&self) -> Option<&ShapedCluster> {
        match self {
            ShapedItem::Cluster(c) => Some(c),
            _ => None,
        }
    }
    /// Returns the bounding box of the item, relative to its own origin.
    ///
    /// The origin of the returned `Rect` is `(0,0)`, representing the top-left corner
    /// of the item's layout space before final positioning. The size represents the
    /// item's total advance (width in horizontal mode) and its line height (ascent + descent).
    pub fn bounds(&self) -> Rect {
        match self {
            ShapedItem::Cluster(cluster) => {
                // The width of a text cluster is its total advance.
                let width = cluster.advance;

                // The height is the sum of its ascent and descent, which defines its line box.
                // We use the existing helper function which correctly calculates this from font
                // metrics.
                let (ascent, descent) = get_item_vertical_metrics(self);
                let height = ascent + descent;

                Rect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                }
            }
            // For atomic inline items like objects, combined blocks, and tabs,
            // their bounds have already been calculated during the shaping or measurement phase.
            ShapedItem::CombinedBlock { bounds, .. } => *bounds,
            ShapedItem::Object { bounds, .. } => *bounds,
            ShapedItem::Tab { bounds, .. } => *bounds,

            // Breaks are control characters and have no visual geometry.
            ShapedItem::Break { .. } => Rect::default(), // A zero-sized rectangle.
        }
    }
}

/// A group of glyphs that corresponds to one or more source characters (a cluster).
#[derive(Debug, Clone)]
pub struct ShapedCluster {
    /// The original text that this cluster was shaped from.
    /// This is crucial for correct hyphenation.
    pub text: String,
    /// The ID of the grapheme cluster this glyph cluster represents.
    pub source_cluster_id: GraphemeClusterId,
    /// The source `ContentIndex` for mapping back to logical items.
    pub source_content_index: ContentIndex,
    /// The DOM NodeId of the Text node this cluster originated from.
    /// None for generated content (list markers, ::before/::after, etc.)
    pub source_node_id: Option<NodeId>,
    /// The glyphs that make up this cluster.
    pub glyphs: Vec<ShapedGlyph>,
    /// The total advance width (horizontal) or height (vertical) of the cluster.
    pub advance: f32,
    /// The direction of this cluster, inherited from its `VisualItem`.
    pub direction: BidiDirection,
    /// Font style of this cluster
    pub style: Arc<StyleProperties>,
    /// If this cluster is a list marker: whether it should be positioned outside
    /// (in the padding gutter) or inside (inline with content).
    /// None for non-marker content.
    pub marker_position_outside: Option<bool>,
}

/// A single, shaped glyph with its essential metrics.
#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    /// The kind of glyph this is (character, hyphen, etc.).
    pub kind: GlyphKind,
    /// Glyph ID inside of the font
    pub glyph_id: u16,
    /// The byte offset of this glyph's source character(s) within its cluster text.
    pub cluster_offset: u32,
    /// The horizontal advance for this glyph (for horizontal text) - this is the BASE advance
    /// from the font metrics, WITHOUT kerning applied
    pub advance: f32,
    /// The kerning adjustment for this glyph (positive = more space, negative = less space)
    /// This is separate from advance so we can position glyphs absolutely
    pub kerning: f32,
    /// The horizontal offset/bearing for this glyph
    pub offset: Point,
    /// The vertical advance for this glyph (for vertical text).
    pub vertical_advance: f32,
    /// The vertical offset/bearing for this glyph.
    pub vertical_offset: Point,
    pub script: Script,
    pub style: Arc<StyleProperties>,
    /// Hash of the font - use LoadedFonts to look up the actual font when needed
    pub font_hash: u64,
    /// Cached font metrics to avoid font lookup for common operations
    pub font_metrics: LayoutFontMetrics,
}

impl ShapedGlyph {
    pub fn into_glyph_instance<T: ParsedFontTrait>(
        &self,
        writing_mode: WritingMode,
        loaded_fonts: &LoadedFonts<T>,
    ) -> GlyphInstance {
        let size = loaded_fonts
            .get_by_hash(self.font_hash)
            .and_then(|font| font.get_glyph_size(self.glyph_id, self.style.font_size_px))
            .unwrap_or_default();

        let position = if writing_mode.is_advance_horizontal() {
            LogicalPosition {
                x: self.offset.x,
                y: self.offset.y,
            }
        } else {
            LogicalPosition {
                x: self.vertical_offset.x,
                y: self.vertical_offset.y,
            }
        };

        GlyphInstance {
            index: self.glyph_id as u32,
            point: position,
            size,
        }
    }

    /// Convert this ShapedGlyph into a GlyphInstance with an absolute position.
    /// This is used for display list generation where glyphs need their final page coordinates.
    pub fn into_glyph_instance_at<T: ParsedFontTrait>(
        &self,
        writing_mode: WritingMode,
        absolute_position: LogicalPosition,
        loaded_fonts: &LoadedFonts<T>,
    ) -> GlyphInstance {
        let size = loaded_fonts
            .get_by_hash(self.font_hash)
            .and_then(|font| font.get_glyph_size(self.glyph_id, self.style.font_size_px))
            .unwrap_or_default();

        GlyphInstance {
            index: self.glyph_id as u32,
            point: absolute_position,
            size,
        }
    }

    /// Convert this ShapedGlyph into a GlyphInstance with an absolute position.
    /// This version doesn't require fonts - it uses a default size.
    /// Use this when you don't need precise glyph bounds (e.g., display list generation).
    pub fn into_glyph_instance_at_simple(
        &self,
        _writing_mode: WritingMode,
        absolute_position: LogicalPosition,
    ) -> GlyphInstance {
        // Use font metrics to estimate size, or default to zero
        // The actual rendering will use the font directly
        GlyphInstance {
            index: self.glyph_id as u32,
            point: absolute_position,
            size: LogicalSize::default(),
        }
    }
}

// --- Stage 4: Positioned Representation (Final Layout) ---

#[derive(Debug, Clone)]
pub struct PositionedItem {
    pub item: ShapedItem,
    pub position: Point,
    pub line_index: usize,
}

#[derive(Debug, Clone)]
pub struct UnifiedLayout {
    pub items: Vec<PositionedItem>,
    /// Information about content that did not fit.
    pub overflow: OverflowInfo,
}

impl UnifiedLayout {
    /// Calculate the bounding box of all positioned items.
    /// This is computed on-demand rather than cached.
    pub fn bounds(&self) -> Rect {
        if self.items.is_empty() {
            return Rect::default();
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32:

... [FILE TRUNCATED - original size: 264340 bytes] ...
```

### layout/src/text3/edit.rs

```rust
//! Pure functions for editing a `Vec<InlineContent>` based on selections.

use std::sync::Arc;

use azul_core::selection::{
    CursorAffinity, GraphemeClusterId, Selection, SelectionRange, TextCursor,
};

use crate::text3::cache::{ContentIndex, InlineContent, StyledRun};

/// An enum representing a single text editing action.
#[derive(Debug, Clone)]
pub enum TextEdit {
    Insert(String),
    DeleteBackward,
    DeleteForward,
}

/// The primary entry point for text modification. Takes the current content and selections,
/// applies an edit, and returns the new content and the resulting cursor positions.
pub fn edit_text(
    content: &[InlineContent],
    selections: &[Selection],
    edit: &TextEdit,
) -> (Vec<InlineContent>, Vec<Selection>) {
    if selections.is_empty() {
        return (content.to_vec(), Vec::new());
    }

    let mut new_content = content.to_vec();
    let mut new_selections = Vec::new();

    // To handle multiple cursors correctly, we must process edits
    // from the end of the document to the beginning. This ensures that
    // earlier edits do not invalidate the indices of later edits.
    let mut sorted_selections = selections.to_vec();
    sorted_selections.sort_by(|a, b| {
        let cursor_a = match a {
            Selection::Cursor(c) => c,
            Selection::Range(r) => &r.start,
        };
        let cursor_b = match b {
            Selection::Cursor(c) => c,
            Selection::Range(r) => &r.start,
        };
        cursor_b.cluster_id.cmp(&cursor_a.cluster_id) // Reverse sort
    });

    for selection in sorted_selections {
        let (mut temp_content, new_cursor) =
            apply_edit_to_selection(&new_content, &selection, edit);

        // When we insert/delete text, we need to adjust all previously-processed cursors
        // that come after this edit position in the same run
        let edit_run = match selection {
            Selection::Cursor(c) => c.cluster_id.source_run,
            Selection::Range(r) => r.start.cluster_id.source_run,
        };
        let edit_byte = match selection {
            Selection::Cursor(c) => c.cluster_id.start_byte_in_run,
            Selection::Range(r) => r.start.cluster_id.start_byte_in_run,
        };

        // Calculate the byte offset change
        let byte_offset_change: i32 = match edit {
            TextEdit::Insert(text) => text.len() as i32,
            TextEdit::DeleteBackward | TextEdit::DeleteForward => {
                // For simplicity, assume 1 grapheme deleted = some bytes
                // A full implementation would track actual bytes deleted
                -1
            }
        };

        // Adjust all previously-processed cursors in the same run that come after this position
        for prev_selection in new_selections.iter_mut() {
            if let Selection::Cursor(cursor) = prev_selection {
                if cursor.cluster_id.source_run == edit_run
                    && cursor.cluster_id.start_byte_in_run >= edit_byte
                {
                    cursor.cluster_id.start_byte_in_run =
                        (cursor.cluster_id.start_byte_in_run as i32 + byte_offset_change).max(0)
                            as u32;
                }
            }
        }

        new_content = temp_content;
        new_selections.push(Selection::Cursor(new_cursor));
    }

    // The new selections were added in reverse order, so we reverse them back.
    new_selections.reverse();

    (new_content, new_selections)
}

/// Applies a single edit to a single selection.
pub fn apply_edit_to_selection(
    content: &[InlineContent],
    selection: &Selection,
    edit: &TextEdit,
) -> (Vec<InlineContent>, TextCursor) {
    let mut new_content = content.to_vec();

    // First, if the selection is a range, we perform a deletion.
    // The result of a deletion is always a single cursor.
    let cursor_after_delete = match selection {
        Selection::Range(range) => {
            let (content_after_delete, cursor_pos) = delete_range(&new_content, range);
            new_content = content_after_delete;
            cursor_pos
        }
        Selection::Cursor(cursor) => *cursor,
    };

    // Now, apply the edit at the collapsed cursor position.
    match edit {
        TextEdit::Insert(text_to_insert) => {
            insert_text(&mut new_content, &cursor_after_delete, text_to_insert)
        }
        TextEdit::DeleteBackward => delete_backward(&mut new_content, &cursor_after_delete),
        TextEdit::DeleteForward => delete_forward(&mut new_content, &cursor_after_delete),
    }
}

/// Deletes the content within a given range.
pub fn delete_range(
    content: &[InlineContent],
    range: &SelectionRange,
) -> (Vec<InlineContent>, TextCursor) {
    // This is a highly complex function. A full implementation needs to handle:
    //
    // - Deletions within a single text run.
    // - Deletions that span across multiple text runs.
    // - Deletions that include non-text items like images.
    //
    // For now, we provide a simplified version that handles deletion within a
    // single run.

    let mut new_content = content.to_vec();
    let start_run_idx = range.start.cluster_id.source_run as usize;
    let end_run_idx = range.end.cluster_id.source_run as usize;

    if start_run_idx == end_run_idx {
        if let Some(InlineContent::Text(run)) = new_content.get_mut(start_run_idx) {
            let start_byte = range.start.cluster_id.start_byte_in_run as usize;
            let end_byte = range.end.cluster_id.start_byte_in_run as usize;
            if start_byte <= end_byte && end_byte <= run.text.len() {
                run.text.drain(start_byte..end_byte);
            }
        }
    } else {
        // TODO: Handle multi-run deletion
    }

    (new_content, range.start) // Return cursor at the start of the deleted range
}

/// Inserts text at a cursor position.
pub fn insert_text(
    content: &mut Vec<InlineContent>,
    cursor: &TextCursor,
    text_to_insert: &str,
) -> (Vec<InlineContent>, TextCursor) {
    let mut new_content = content.clone();
    let run_idx = cursor.cluster_id.source_run as usize;
    let byte_offset = cursor.cluster_id.start_byte_in_run as usize;

    if let Some(InlineContent::Text(run)) = new_content.get_mut(run_idx) {
        if byte_offset <= run.text.len() {
            run.text.insert_str(byte_offset, text_to_insert);

            let new_cursor = TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: run_idx as u32,
                    start_byte_in_run: (byte_offset + text_to_insert.len()) as u32,
                },
                affinity: CursorAffinity::Leading,
            };
            return (new_content, new_cursor);
        }
    }

    // If insertion failed, return original state
    (content.to_vec(), *cursor)
}

/// Deletes one grapheme cluster backward from the cursor.
pub fn delete_backward(
    content: &mut Vec<InlineContent>,
    cursor: &TextCursor,
) -> (Vec<InlineContent>, TextCursor) {
    use unicode_segmentation::UnicodeSegmentation;
    let mut new_content = content.clone();
    let run_idx = cursor.cluster_id.source_run as usize;
    let byte_offset = cursor.cluster_id.start_byte_in_run as usize;

    if let Some(InlineContent::Text(run)) = new_content.get_mut(run_idx) {
        if byte_offset > 0 {
            let prev_grapheme_start = run.text[..byte_offset]
                .grapheme_indices(true)
                .last()
                .map_or(0, |(i, _)| i);
            run.text.drain(prev_grapheme_start..byte_offset);

            let new_cursor = TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: run_idx as u32,
                    start_byte_in_run: prev_grapheme_start as u32,
                },
                affinity: CursorAffinity::Leading,
            };
            return (new_content, new_cursor);
        } else if run_idx > 0 {
            // Handle deleting across run boundaries (merge with previous run)
            if let Some(InlineContent::Text(prev_run)) = content.get(run_idx - 1).cloned() {
                let mut merged_text = prev_run.text;
                let new_cursor_byte_offset = merged_text.len();
                merged_text.push_str(&run.text);

                new_content[run_idx - 1] = InlineContent::Text(StyledRun {
                    text: merged_text,
                    style: prev_run.style,
                    logical_start_byte: prev_run.logical_start_byte,
                    source_node_id: prev_run.source_node_id,
                });
                new_content.remove(run_idx);

                let new_cursor = TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: (run_idx - 1) as u32,
                        start_byte_in_run: new_cursor_byte_offset as u32,
                    },
                    affinity: CursorAffinity::Leading,
                };
                return (new_content, new_cursor);
            }
        }
    }

    (content.to_vec(), *cursor)
}

/// Deletes one grapheme cluster forward from the cursor.
pub fn delete_forward(
    content: &mut Vec<InlineContent>,
    cursor: &TextCursor,
) -> (Vec<InlineContent>, TextCursor) {
    use unicode_segmentation::UnicodeSegmentation;
    let mut new_content = content.clone();
    let run_idx = cursor.cluster_id.source_run as usize;
    let byte_offset = cursor.cluster_id.start_byte_in_run as usize;

    if let Some(InlineContent::Text(run)) = new_content.get_mut(run_idx) {
        if byte_offset < run.text.len() {
            let next_grapheme_end = run.text[byte_offset..]
                .grapheme_indices(true)
                .nth(1)
                .map_or(run.text.len(), |(i, _)| byte_offset + i);
            run.text.drain(byte_offset..next_grapheme_end);

            // Cursor position doesn't change
            return (new_content, *cursor);
        } else if run_idx < content.len() - 1 {
            // Handle deleting across run boundaries (merge with next run)
            if let Some(InlineContent::Text(next_run)) = content.get(run_idx + 1).cloned() {
                let mut merged_text = run.text.clone();
                merged_text.push_str(&next_run.text);

                new_content[run_idx] = InlineContent::Text(StyledRun {
                    text: merged_text,
                    style: run.style.clone(),
                    logical_start_byte: run.logical_start_byte,
                    source_node_id: run.source_node_id,
                });
                new_content.remove(run_idx + 1);

                return (new_content, *cursor);
            }
        }
    }

    (content.to_vec(), *cursor)
}

/// Inspect what would be deleted by a delete operation without actually deleting
///
/// Returns (range_that_would_be_deleted, text_that_would_be_deleted).
/// This is useful for callbacks to inspect pending delete operations.
///
/// # Arguments
///
/// - `content` - The current text content
/// - `selection` - The current selection (cursor or range)
/// - `forward` - If true, delete forward (Delete key); if false, delete backward (Backspace key)
///
/// # Returns
///
/// - `Some((range, deleted_text))` - The range and text that would be deleted
/// - `None` - Nothing would be deleted (e.g., cursor at start/end of document)
pub fn inspect_delete(
    content: &[InlineContent],
    selection: &Selection,
    forward: bool,
) -> Option<(SelectionRange, String)> {
    match selection {
        Selection::Range(range) => {
            // If there's already a selection, that's what would be deleted
            let deleted_text = extract_text_in_range(content, range);
            Some((*range, deleted_text))
        }
        Selection::Cursor(cursor) => {
            // No selection - would delete one grapheme cluster
            if forward {
                inspect_delete_forward(content, cursor)
            } else {
                inspect_delete_backward(content, cursor)
            }
        }
    }
}

/// Inspect what would be deleted by delete-forward (Delete key)
fn inspect_delete_forward(
    content: &[InlineContent],
    cursor: &TextCursor,
) -> Option<(SelectionRange, String)> {
    use unicode_segmentation::UnicodeSegmentation;

    let run_idx = cursor.cluster_id.source_run as usize;
    let byte_offset = cursor.cluster_id.start_byte_in_run as usize;

    if let Some(InlineContent::Text(run)) = content.get(run_idx) {
        if byte_offset < run.text.len() {
            // Delete within same run
            let next_grapheme_end = run.text[byte_offset..]
                .grapheme_indices(true)
                .nth(1)
                .map_or(run.text.len(), |(i, _)| byte_offset + i);

            let deleted_text = run.text[byte_offset..next_grapheme_end].to_string();

            let range = SelectionRange {
                start: *cursor,
                end: TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: run_idx as u32,
                        start_byte_in_run: next_grapheme_end as u32,
                    },
                    affinity: CursorAffinity::Leading,
                },
            };

            return Some((range, deleted_text));
        } else if run_idx < content.len() - 1 {
            // Would delete across run boundary
            if let Some(InlineContent::Text(next_run)) = content.get(run_idx + 1) {
                let deleted_text = next_run.text.graphemes(true).next()?.to_string();

                let next_grapheme_end = next_run
                    .text
                    .grapheme_indices(true)
                    .nth(1)
                    .map_or(next_run.text.len(), |(i, _)| i);

                let range = SelectionRange {
                    start: *cursor,
                    end: TextCursor {
                        cluster_id: GraphemeClusterId {
                            source_run: (run_idx + 1) as u32,
                            start_byte_in_run: next_grapheme_end as u32,
                        },
                        affinity: CursorAffinity::Leading,
                    },
                };

                return Some((range, deleted_text));
            }
        }
    }

    None // At end of document, nothing to delete
}

/// Inspect what would be deleted by delete-backward (Backspace key)
fn inspect_delete_backward(
    content: &[InlineContent],
    cursor: &TextCursor,
) -> Option<(SelectionRange, String)> {
    use unicode_segmentation::UnicodeSegmentation;

    let run_idx = cursor.cluster_id.source_run as usize;
    let byte_offset = cursor.cluster_id.start_byte_in_run as usize;

    if let Some(InlineContent::Text(run)) = content.get(run_idx) {
        if byte_offset > 0 {
            // Delete within same run
            let prev_grapheme_start = run.text[..byte_offset]
                .grapheme_indices(true)
                .last()
                .map_or(0, |(i, _)| i);

            let deleted_text = run.text[prev_grapheme_start..byte_offset].to_string();

            let range = SelectionRange {
                start: TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: run_idx as u32,
                        start_byte_in_run: prev_grapheme_start as u32,
                    },
                    affinity: CursorAffinity::Leading,
                },
                end: *cursor,
            };

            return Some((range, deleted_text));
        } else if run_idx > 0 {
            // Would delete across run boundary
            if let Some(InlineContent::Text(prev_run)) = content.get(run_idx - 1) {
                let deleted_text = prev_run.text.graphemes(true).last()?.to_string();

                let prev_grapheme_start = prev_run.text[..]
                    .grapheme_indices(true)
                    .last()
                    .map_or(0, |(i, _)| i);

                let range = SelectionRange {
                    start: TextCursor {
                        cluster_id: GraphemeClusterId {
                            source_run: (run_idx - 1) as u32,
                            start_byte_in_run: prev_grapheme_start as u32,
                        },
                        affinity: CursorAffinity::Leading,
                    },
                    end: *cursor,
                };

                return Some((range, deleted_text));
            }
        }
    }

    None // At start of document, nothing to delete
}

/// Extract the text within a selection range
fn extract_text_in_range(content: &[InlineContent], range: &SelectionRange) -> String {
    let start_run = range.start.cluster_id.source_run as usize;
    let end_run = range.end.cluster_id.source_run as usize;
    let start_byte = range.start.cluster_id.start_byte_in_run as usize;
    let end_byte = range.end.cluster_id.start_byte_in_run as usize;

    if start_run == end_run {
        // Single run
        if let Some(InlineContent::Text(run)) = content.get(start_run) {
            if start_byte <= end_byte && end_byte <= run.text.len() {
                return run.text[start_byte..end_byte].to_string();
            }
        }
    } else {
        // Multi-run selection (simplified - full implementation would handle images, etc.)
        let mut result = String::new();

        for (idx, item) in content.iter().enumerate() {
            if let InlineContent::Text(run) = item {
                if idx == start_run {
                    // First run - from start_byte to end
                    if start_byte < run.text.len() {
                        result.push_str(&run.text[start_byte..]);
                    }
                } else if idx > start_run && idx < end_run {
                    // Middle runs - entire text
                    result.push_str(&run.text);
                } else if idx == end_run {
                    // Last run - from 0 to end_byte
                    if end_byte <= run.text.len() {
                        result.push_str(&run.text[..end_byte]);
                    }
                    break;
                }
            }
        }

        return result;
    }

    String::new()
}

```

### layout/src/text3/selection.rs

```rust
//! Text selection helper functions
//!
//! Provides word and paragraph selection algorithms.

use azul_core::selection::{CursorAffinity, GraphemeClusterId, SelectionRange, TextCursor};

use crate::text3::cache::{PositionedItem, ShapedCluster, ShapedItem, UnifiedLayout};

/// Select the word at the given cursor position
///
/// Uses Unicode word boundaries to determine word start/end.
/// Returns a SelectionRange covering the entire word.
pub fn select_word_at_cursor(
    cursor: &TextCursor,
    layout: &UnifiedLayout,
) -> Option<SelectionRange> {
    // Find the item containing this cursor
    let (item_idx, cluster) = find_cluster_at_cursor(cursor, layout)?;

    // Get the text from this cluster and surrounding clusters on the same line
    let line_text = extract_line_text_at_item(item_idx, layout);
    let cursor_byte_offset = cursor.cluster_id.start_byte_in_run as usize;

    // Find word boundaries
    let (word_start, word_end) = find_word_boundaries(&line_text, cursor_byte_offset);

    // Convert byte offsets to cursors
    let start_cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: cursor.cluster_id.source_run,
            start_byte_in_run: word_start as u32,
        },
        affinity: CursorAffinity::Leading,
    };

    let end_cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: cursor.cluster_id.source_run,
            start_byte_in_run: word_end as u32,
        },
        affinity: CursorAffinity::Trailing,
    };

    Some(SelectionRange {
        start: start_cursor,
        end: end_cursor,
    })
}

/// Select the paragraph/line at the given cursor position
///
/// Returns a SelectionRange covering the entire line from the first
/// to the last cluster on that line.
pub fn select_paragraph_at_cursor(
    cursor: &TextCursor,
    layout: &UnifiedLayout,
) -> Option<SelectionRange> {
    // Find the item containing this cursor
    let (item_idx, _) = find_cluster_at_cursor(cursor, layout)?;
    let item = &layout.items[item_idx];
    let line_index = item.line_index;

    // Find all items on this line
    let line_items: Vec<(usize, &PositionedItem)> = layout
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.line_index == line_index)
        .collect();

    if line_items.is_empty() {
        return None;
    }

    // Get first and last cluster on line
    let first_cluster = line_items
        .iter()
        .find_map(|(_, item)| item.item.as_cluster())?;

    let last_cluster = line_items
        .iter()
        .rev()
        .find_map(|(_, item)| item.item.as_cluster())?;

    // Create selection spanning entire line
    Some(SelectionRange {
        start: TextCursor {
            cluster_id: first_cluster.source_cluster_id,
            affinity: CursorAffinity::Leading,
        },
        end: TextCursor {
            cluster_id: last_cluster.source_cluster_id,
            affinity: CursorAffinity::Trailing,
        },
    })
}

// Helper Functions

/// Find the cluster containing the given cursor
fn find_cluster_at_cursor<'a>(
    cursor: &TextCursor,
    layout: &'a UnifiedLayout,
) -> Option<(usize, &'a ShapedCluster)> {
    layout.items.iter().enumerate().find_map(|(idx, item)| {
        if let ShapedItem::Cluster(cluster) = &item.item {
            if cluster.source_cluster_id == cursor.cluster_id {
                return Some((idx, cluster));
            }
        }
        None
    })
}

/// Extract text from all clusters on the same line as the given item
fn extract_line_text_at_item(item_idx: usize, layout: &UnifiedLayout) -> String {
    let line_index = layout.items[item_idx].line_index;

    let mut text = String::new();
    for item in &layout.items {
        if item.line_index != line_index {
            continue;
        }

        if let ShapedItem::Cluster(cluster) = &item.item {
            text.push_str(&cluster.text);
        }
    }

    text
}

/// Find word boundaries around the given byte offset
///
/// Uses a simple algorithm: word characters are alphanumeric or underscore,
/// everything else is a boundary.
fn find_word_boundaries(text: &str, cursor_offset: usize) -> (usize, usize) {
    // Clamp cursor offset to text length
    let cursor_offset = cursor_offset.min(text.len());

    // Find word start (scan backwards)
    let mut word_start = 0;
    let mut char_indices: Vec<(usize, char)> = text.char_indices().collect();

    for (i, (byte_idx, ch)) in char_indices.iter().enumerate().rev() {
        if *byte_idx >= cursor_offset {
            continue;
        }

        if !is_word_char(*ch) {
            // Found boundary, word starts after this char
            word_start = if i + 1 < char_indices.len() {
                char_indices[i + 1].0
            } else {
                text.len()
            };
            break;
        }
    }

    // Find word end (scan forwards)
    let mut word_end = text.len();
    for (byte_idx, ch) in char_indices.iter() {
        if *byte_idx <= cursor_offset {
            continue;
        }

        if !is_word_char(*ch) {
            // Found boundary, word ends before this char
            word_end = *byte_idx;
            break;
        }
    }

    // If cursor is on whitespace, select just that whitespace
    if let Some((_, ch)) = char_indices.iter().find(|(idx, _)| *idx == cursor_offset) {
        if !is_word_char(*ch) {
            // Find span of consecutive whitespace/punctuation
            let start = char_indices
                .iter()
                .rev()
                .find(|(idx, c)| *idx < cursor_offset && is_word_char(*c))
                .map(|(idx, c)| idx + c.len_utf8())
                .unwrap_or(0);

            let end = char_indices
                .iter()
                .find(|(idx, c)| *idx > cursor_offset && is_word_char(*c))
                .map(|(idx, _)| *idx)
                .unwrap_or(text.len());

            return (start, end);
        }
    }

    (word_start, word_end)
}

/// Check if a character is part of a word
#[inline]
fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

```

### layout/src/solver3/mod.rs

```rust
//! solver3/mod.rs
//!
//! Next-generation CSS layout engine with proper formatting context separation

pub mod cache;
pub mod counters;
pub mod display_list;
pub mod fc;
pub mod geometry;
pub mod getters;
pub mod layout_tree;
pub mod paged_layout;
pub mod pagination;
pub mod positioning;
pub mod scrollbar;
pub mod sizing;
pub mod taffy_bridge;

/// Lazy debug_info macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_info {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_info_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_warning macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_warning {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_warning_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_error macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_error {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_error_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_log macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_log {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_log_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_box_props macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_box_props {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_box_props_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_css_getter macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_css_getter {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_css_getter_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_bfc_layout macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_bfc_layout {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_bfc_layout_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_ifc_layout macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_ifc_layout {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_ifc_layout_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_table_layout macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_table_layout {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_table_layout_inner(format!($($arg)*));
        }
    };
}

/// Lazy debug_display_type macro - only evaluates format args when debug_messages is Some
#[macro_export]
macro_rules! debug_display_type {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_display_type_inner(format!($($arg)*));
        }
    };
}

// Test modules commented out until they are implemented
// #[cfg(test)]
// mod tests;
// #[cfg(test)]
// mod tests_arabic;

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::{DocumentId, ScrollPosition},
    resources::RendererResources,
    selection::{SelectionState, TextCursor, TextSelection},
    styled_dom::StyledDom,
};
use azul_css::{
    props::property::{CssProperty, CssPropertyCategory},
    LayoutDebugMessage, LayoutDebugMessageType,
};

use self::{
    display_list::generate_display_list,
    geometry::IntrinsicSizes,
    getters::get_writing_mode,
    layout_tree::{generate_layout_tree, LayoutTree},
    sizing::calculate_intrinsic_sizes,
};
#[cfg(feature = "text_layout")]
pub use crate::font_traits::TextLayoutCache;
use crate::{
    font_traits::ParsedFontTrait,
    solver3::{
        cache::LayoutCache,
        display_list::DisplayList,
        fc::{check_scrollbar_necessity, LayoutConstraints, LayoutResult},
        layout_tree::DirtyFlag,
    },
};

/// A map of hashes for each node to detect changes in content like text.
pub type NodeHashMap = BTreeMap<usize, u64>;

/// Central context for a single layout pass.
pub struct LayoutContext<'a, T: ParsedFontTrait> {
    pub styled_dom: &'a StyledDom,
    #[cfg(feature = "text_layout")]
    pub font_manager: &'a crate::font_traits::FontManager<T>,
    #[cfg(not(feature = "text_layout"))]
    pub font_manager: core::marker::PhantomData<&'a T>,
    /// Legacy per-node selection state (for backward compatibility)
    pub selections: &'a BTreeMap<DomId, SelectionState>,
    /// New multi-node text selection with anchor/focus model
    pub text_selections: &'a BTreeMap<DomId, TextSelection>,
    pub debug_messages: &'a mut Option<Vec<LayoutDebugMessage>>,
    pub counters: &'a mut BTreeMap<(usize, String), i32>,
    pub viewport_size: LogicalSize,
    /// Fragmentation context for CSS Paged Media (PDF generation)
    /// When Some, layout respects page boundaries and generates one DisplayList per page
    pub fragmentation_context: Option<&'a mut crate::paged::FragmentationContext>,
    /// Whether the text cursor should be drawn (managed by CursorManager blink timer)
    /// When false, the cursor is in the "off" phase of blinking and should not be rendered.
    /// When true (default), the cursor is visible.
    pub cursor_is_visible: bool,
    /// Current cursor location from CursorManager (dom_id, node_id, cursor)
    /// This is separate from selections - the cursor represents the text insertion point
    /// in a contenteditable element and should be painted independently.
    pub cursor_location: Option<(DomId, NodeId, TextCursor)>,
}

impl<'a, T: ParsedFontTrait> LayoutContext<'a, T> {
    /// Check if debug messages are enabled (for use with lazy macros)
    #[inline]
    pub fn has_debug(&self) -> bool {
        self.debug_messages.is_some()
    }

    /// Internal method - called by debug_log! macro after checking has_debug()
    #[inline]
    pub fn debug_log_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage {
                message: message.into(),
                location: "solver3".into(),
                message_type: Default::default(),
            });
        }
    }

    /// Internal method - called by debug_info! macro after checking has_debug()
    #[inline]
    pub fn debug_info_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::info(message));
        }
    }

    /// Internal method - called by debug_warning! macro after checking has_debug()
    #[inline]
    pub fn debug_warning_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::warning(message));
        }
    }

    /// Internal method - called by debug_error! macro after checking has_debug()
    #[inline]
    pub fn debug_error_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::error(message));
        }
    }

    /// Internal method - called by debug_box_props! macro after checking has_debug()
    #[inline]
    pub fn debug_box_props_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::box_props(message));
        }
    }

    /// Internal method - called by debug_css_getter! macro after checking has_debug()
    #[inline]
    pub fn debug_css_getter_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::css_getter(message));
        }
    }

    /// Internal method - called by debug_bfc_layout! macro after checking has_debug()
    #[inline]
    pub fn debug_bfc_layout_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::bfc_layout(message));
        }
    }

    /// Internal method - called by debug_ifc_layout! macro after checking has_debug()
    #[inline]
    pub fn debug_ifc_layout_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::ifc_layout(message));
        }
    }

    /// Internal method - called by debug_table_layout! macro after checking has_debug()
    #[inline]
    pub fn debug_table_layout_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::table_layout(message));
        }
    }

    /// Internal method - called by debug_display_type! macro after checking has_debug()
    #[inline]
    pub fn debug_display_type_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::display_type(message));
        }
    }

    // DEPRECATED: Use debug_*!() macros instead for lazy evaluation
    // These methods always evaluate format!() arguments even when debug is disabled

    #[inline]
    #[deprecated(note = "Use debug_info! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_info(&mut self, message: impl Into<String>) {
        self.debug_info_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_warning! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_warning(&mut self, message: impl Into<String>) {
        self.debug_warning_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_error! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_error(&mut self, message: impl Into<String>) {
        self.debug_error_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_log! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_log(&mut self, message: &str) {
        self.debug_log_inner(message.to_string());
    }

    #[inline]
    #[deprecated(note = "Use debug_box_props! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_box_props(&mut self, message: impl Into<String>) {
        self.debug_box_props_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_css_getter! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_css_getter(&mut self, message: impl Into<String>) {
        self.debug_css_getter_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_bfc_layout! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_bfc_layout(&mut self, message: impl Into<String>) {
        self.debug_bfc_layout_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_ifc_layout! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_ifc_layout(&mut self, message: impl Into<String>) {
        self.debug_ifc_layout_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_table_layout! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_table_layout(&mut self, message: impl Into<String>) {
        self.debug_table_layout_inner(message.into());
    }

    #[inline]
    #[deprecated(note = "Use debug_display_type! macro for lazy evaluation")]
    #[allow(deprecated)]
    pub fn debug_display_type(&mut self, message: impl Into<String>) {
        self.debug_display_type_inner(message.into());
    }
}

/// Main entry point for the incremental, cached layout engine
#[cfg(feature = "text_layout")]
pub fn layout_document<T: ParsedFontTrait + Sync + 'static>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    new_dom: StyledDom,
    viewport: LogicalRect,
    font_manager: &crate::font_traits::FontManager<T>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    selections: &BTreeMap<DomId, SelectionState>,
    text_selections: &BTreeMap<DomId, TextSelection>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    gpu_value_cache: Option<&azul_core::gpu::GpuValueCache>,
    renderer_resources: &azul_core::resources::RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: azul_core::dom::DomId,
    cursor_is_visible: bool,
    cursor_location: Option<(DomId, NodeId, TextCursor)>,
) -> Result<DisplayList> {
    // Reset IFC ID counter at the start of each layout pass
    // This ensures IFCs get consistent IDs across frames when the DOM structure is stable
    crate::solver3::layout_tree::IfcId::reset_counter();

    if let Some(msgs) = debug_messages.as_mut() {
        msgs.push(LayoutDebugMessage::info(format!(
            "[Layout] layout_document called - viewport: ({:.1}, {:.1}) size ({:.1}x{:.1})",
            viewport.origin.x, viewport.origin.y, viewport.size.width, viewport.size.height
        )));
        msgs.push(LayoutDebugMessage::info(format!(
            "[Layout] DOM has {} nodes",
            new_dom.node_data.len()
        )));
    }

    // Create temporary context without counters for tree generation
    let mut counter_values = BTreeMap::new();
    let mut ctx_temp = LayoutContext {
        styled_dom: &new_dom,
        font_manager,
        selections,
        text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: None,
        cursor_is_visible,
        cursor_location: cursor_location.clone(),
    };

    // --- Step 1: Reconciliation & Invalidation ---
    let (mut new_tree, mut recon_result) =
        cache::reconcile_and_invalidate(&mut ctx_temp, cache, viewport)?;

    // Step 1.2: Clear Taffy Caches for Dirty Nodes
    for &node_idx in &recon_result.intrinsic_dirty {
        if let Some(node) = new_tree.get_mut(node_idx) {
            node.taffy_cache.clear();
        }
    }

    // Step 1.3: Compute CSS Counters
    // This must be done after tree generation but before layout,
    // as list markers need counter values during formatting context layout
    cache::compute_counters(&new_dom, &new_tree, &mut counter_values);

    // Now create the real context with computed counters
    let mut ctx = LayoutContext {
        styled_dom: &new_dom,
        font_manager,
        selections,
        text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: None,
        cursor_is_visible,
        cursor_location,
    };

    // --- Step 1.5: Early Exit Optimization ---
    if recon_result.is_clean() {
        ctx.debug_log("No changes, returning existing display list");
        let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;

        // Use cached scroll IDs if available, otherwise compute them
        let scroll_ids = if cache.scroll_ids.is_empty() {
            use crate::window::LayoutWindow;
            let (scroll_ids, scroll_id_to_node_id) =
                LayoutWindow::compute_scroll_ids(tree, &new_dom);
            cache.scroll_ids = scroll_ids.clone();
            cache.scroll_id_to_node_id = scroll_id_to_node_id;
            scroll_ids
        } else {
            cache.scroll_ids.clone()
        };

        return generate_display_list(
            &mut ctx,
            tree,
            &cache.calculated_positions,
            scroll_offsets,
            &scroll_ids,
            gpu_value_cache,
            renderer_resources,
            id_namespace,
            dom_id,
        );
    }

    // --- Step 2: Incremental Layout Loop (handles scrollbar-induced reflows) ---
    let mut calculated_positions = cache.calculated_positions.clone();
    let mut loop_count = 0;
    loop {
        loop_count += 1;
        if loop_count > 10 {
            // Safety limit to prevent infinite loops
            break;
        }

        calculated_positions = cache.calculated_positions.clone();
        let mut reflow_needed_for_scrollbars = false;

        calculate_intrinsic_sizes(&mut ctx, &mut new_tree, &recon_result.intrinsic_dirty)?;

        for &root_idx in &recon_result.layout_roots {
            let (cb_pos, cb_size) = get_containing_block_for_node(
                &new_tree,
                &new_dom,
                root_idx,
                &calculated_positions,
                viewport,
            );

            // For ROOT nodes (no parent), we need to account for their margin.
            // The containing block position from viewport is (0, 0), but the root's
            // content starts at (margin + border + padding, margin + border + padding).
            // We pass margin-adjusted position so calculate_content_box_pos works correctly.
            let root_node = &new_tree.nodes[root_idx];
            let is_root_with_margin = root_node.parent.is_none()
                && (root_node.box_props.margin.left != 0.0 || root_node.box_props.margin.top != 0.0);

            let adjusted_cb_pos = if is_root_with_margin {
                LogicalPosition::new(
                    cb_pos.x + root_node.box_props.margin.left,
                    cb_pos.y + root_node.box_props.margin.top,
                )
            } else {
                cb_pos
            };

            // DEBUG: Log containing block info for this root
            if let Some(debug_msgs) = ctx.debug_messages.as_mut() {
                let dom_name = root_node
                    .dom_node_id
                    .and_then(|id| new_dom.node_data.as_container().internal.get(id.index()))
                    .map(|n| format!("{:?}", n.node_type))
                    .unwrap_or_else(|| "Unknown".to_string());

                debug_msgs.push(LayoutDebugMessage::new(
                    LayoutDebugMessageType::PositionCalculation,
                    format!(
                        "[LAYOUT ROOT {}] {} - CB pos=({:.2}, {:.2}), adjusted=({:.2}, {:.2}), \
                         CB size=({:.2}x{:.2}), viewport=({:.2}x{:.2}), margin=({:.2}, {:.2})",
                        root_idx,
                        dom_name,
                        cb_pos.x,
                        cb_pos.y,
                        adjusted_cb_pos.x,
                        adjusted_cb_pos.y,
                        cb_size.width,
                        cb_size.height,
                        viewport.size.width,
                        viewport.size.height,
                        root_node.box_props.margin.left,
                        root_node.box_props.margin.top
                    ),
                ));
            }

            cache::calculate_layout_for_subtree(
                &mut ctx,
                &mut new_tree,
                text_cache,
                root_idx,
                adjusted_cb_pos,
                cb_size,
                &mut calculated_positions,
                &mut reflow_needed_for_scrollbars,
                &mut cache.float_cache,
            )?;

            // CRITICAL: Insert the root node's own position into calculated_positions
            // This is necessary because calculate_layout_for_subtree only inserts
            // positions for children, not for the root itself.
            //
            // For root nodes, the position should be at (margin.left, margin.top) relative
            // to the viewport origin, because the margin creates space between the viewport
            // edge and the element's border-box.
            if !calculated_positions.contains_key(&root_idx) {
                let root_node = &new_tree.nodes[root_idx];

                // Calculate the root's border-box position by adding margins to viewport origin
                // This is different from non-root nodes which inherit their position from
                // their containing block.
                let root_position = LogicalPosition::new(
                    cb_pos.x + root_node.box_props.margin.left,
                    cb_pos.y + root_node.box_props.margin.top,
                );

                // DEBUG: Log root positioning
                if let Some(debug_msgs) = ctx.debug_messages.as_mut() {
                    let dom_name = root_node
                        .dom_node_id
                        .and_then(|id| new_dom.node_data.as_container().internal.get(id.index()))
                        .map(|n| format!("{:?}", n.node_type))
                        .unwrap_or_else(|| "Unknown".to_string());

                    debug_msgs.push(LayoutDebugMessage::new(
                        LayoutDebugMessageType::PositionCalculation,
                        format!(
                            "[ROOT POSITION {}] {} - Inserting position=({:.2}, {:.2}) (viewport origin + margin), \
                             margin=({:.2}, {:.2}, {:.2}, {:.2})",
                            root_idx,
                            dom_name,
                            root_position.x,
                            root_position.y,
                            root_node.box_props.margin.top,
                            root_node.box_props.margin.right,
                            root_node.box_props.margin.bottom,
                            root_node.box_props.margin.left
                        ),
                    ));
                }

                calculated_positions.insert(root_idx, root_position);
            }
        }

        cache::reposition_clean_subtrees(
            &new_dom,
            &new_tree,
            &recon_result.layout_roots,
            &mut calculated_positions,
        );

        if reflow_needed_for_scrollbars {
            ctx.debug_log(&format!(
                "Scrollbars changed container size, starting full reflow (loop {})",
                loop_count
            ));
            recon_result.layout_roots.clear();
            recon_result.layout_roots.insert(new_tree.root);
            recon_result.intrinsic_dirty = (0..new_tree.nodes.len()).collect();
            continue;
        }

        break;
    }

    // --- Step 3: Adjust Relatively Positioned Elements ---
    // This must be done BEFORE positioning out-of-flow elements, because
    // relatively positioned elements establish containing blocks for their
    // absolutely positioned descendants. If we adjust relative positions after
    // positioning absolute elements, the absolute elements will be positioned
    // relative to the wrong (pre-adjustment) position of their containing block.
    // Pass the viewport to correctly resolve percentage offsets for the root element.
    positioning::adjust_relative_positions(
        &mut ctx,
        &new_tree,
        &mut calculated_positions,
        viewport,
    )?;

    // --- Step 3.5: Position Out-of-Flow Elements ---
    // This must be done AFTER adjusting relative positions, so that absolutely
    // positioned elements are positioned relative to the final (post-adjustment)
    // position of their relatively positioned containing blocks.
    positioning::position_out_of_flow_elements(
        &mut ctx,
        &mut new_tree,
        &mut calculated_positions,
        viewport,
    )?;

    // --- Step 3.75: Compute Stable Scroll IDs ---
    // This must be done AFTER layout but BEFORE display list generation
    use crate::window::LayoutWindow;
    let (scroll_ids, scroll_id_to_node_id) = LayoutWindow::compute_scroll_ids(&new_tree, &new_dom);

    // --- Step 4: Generate Display List & Update Cache ---
    let display_list = generate_display_list(
        &mut ctx,
        &new_tree,
        &calculated_positions,
        scroll_offsets,
        &scroll_ids,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
    )?;

    cache.tree = Some(new_tree);
    cache.calculated_positions = calculated_positions;
    cache.viewport = Some(viewport);
    cache.scroll_ids = scroll_ids;
    cache.scroll_id_to_node_id = scroll_id_to_node_id;
    cache.counters = counter_values;

    Ok(display_list)
}

// STUB: This helper is required by the main loop
fn get_containing_block_for_node(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    node_idx: usize,
    calculated_positions: &BTreeMap<usize, LogicalPosition>,
    viewport: LogicalRect,
) -> (LogicalPosition, LogicalSize) {
    if let Some(parent_idx) = tree.get(node_idx).and_then(|n| n.parent) {
        if let Some(parent_node) = tree.get(parent_idx) {
            let pos = calculated_positions
                .get(&parent_idx)
                .copied()
                .unwrap_or_default();
            let size = parent_node.used_size.unwrap_or_default();
            // Position in calculated_positions is the margin-box position
            // To get content-box, add: border + padding (NOT margin, that's already in pos)
            let content_pos = LogicalPosition::new(
                pos.x + parent_node.box_props.border.left + parent_node.box_props.padding.left,
                pos.y + parent_node.box_props.border.top + parent_node.box_props.padding.top,
            );

            if let Some(dom_id) = parent_node.dom_node_id {
                let styled_node_state = &styled_dom
                    .styled_nodes
                    .as_container()
                    .get(dom_id)
                    .map(|n| &n.styled_node_state)
                    .cloned()
                    .unwrap_or_default();
                let writing_mode =
                    get_writing_mode(styled_dom, dom_id, styled_node_state).unwrap_or_default();
                let content_size = parent_node.box_props.inner_size(size, writing_mode);
                return (content_pos, content_size);
            }

            return (content_pos, size);
        }
    }
    (viewport.origin, viewport.size)
}

#[derive(Debug)]
pub enum LayoutError {
    InvalidTree,
    SizingFailed,
    PositioningFailed,
    DisplayListFailed,
    Text(crate::font_traits::LayoutError),
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::InvalidTree => write!(f, "Invalid layout tree"),
            LayoutError::SizingFailed => write!(f, "Sizing calculation failed"),
            LayoutError::PositioningFailed => write!(f, "Position calculation failed"),
            LayoutError::DisplayListFailed => write!(f, "Display list generation failed"),
            LayoutError::Text(e) => write!(f, "Text layout error: {:?}", e),
        }
    }
}

impl From<crate::font_traits::LayoutError> for LayoutError {
    fn from(err: crate::font_traits::LayoutError) -> Self {
        LayoutError::Text(err)
    }
}

impl std::error::Error for LayoutError {}

pub type Result<T> = std::result::Result<T, LayoutError>;

```

### layout/src/solver3/fc.rs

```rust
//! solver3/fc.rs - Formatting Context Layout
//!
//! This module implements the CSS Visual Formatting Model's formatting contexts:
//!
//! - **Block Formatting Context (BFC)**: CSS 2.2 § 9.4.1 Block-level boxes in normal flow, with
//!   margin collapsing and float positioning.
//!
//! - **Inline Formatting Context (IFC)**: CSS 2.2 § 9.4.2 Inline-level content (text,
//!   inline-blocks) laid out in line boxes.
//!
//! - **Table Formatting Context**: CSS 2.2 § 17 Table layout with column width calculation and cell
//!   positioning.
//!
//! - **Flex/Grid Formatting Contexts**: CSS Flexbox/Grid via Taffy Delegated to the Taffy layout
//!   engine for modern layout modes.
//!
//! # Module Organization
//!
//! 1. **Constants & Types** - Magic numbers as named constants, core types
//! 2. **Entry Point** - `layout_formatting_context` dispatcher
//! 3. **BFC Layout** - Block formatting context implementation
//! 4. **IFC Layout** - Inline formatting context implementation
//! 5. **Table Layout** - Table formatting context implementation
//! 6. **Flex/Grid Layout** - Taffy bridge wrappers
//! 7. **Helper Functions** - Property getters, margin collapsing, utilities

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use azul_core::{
    dom::{FormattingContext, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{
            font::{StyleFontStyle, StyleFontWeight},
            pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
            ColorU, PhysicalSize, PropertyContext, ResolutionContext, SizeMetric,
        },
        layout::{
            ColumnCount, LayoutBorderSpacing, LayoutClear, LayoutDisplay, LayoutFloat,
            LayoutHeight, LayoutJustifyContent, LayoutOverflow, LayoutPosition, LayoutTableLayout,
            LayoutTextJustify, LayoutWidth, LayoutWritingMode, ShapeInside, ShapeOutside,
            StyleBorderCollapse, StyleCaptionSide,
        },
        property::CssProperty,
        style::{
            BorderStyle, StyleDirection, StyleHyphens, StyleListStylePosition, StyleListStyleType,
            StyleTextAlign, StyleTextCombineUpright, StyleVerticalAlign, StyleVisibility,
            StyleWhiteSpace,
        },
    },
};
use rust_fontconfig::FcWeight;
use taffy::{AvailableSpace, LayoutInput, Line, Size as TaffySize};

#[cfg(feature = "text_layout")]
use crate::text3;
use crate::{
    debug_ifc_layout, debug_info, debug_log, debug_table_layout, debug_warning,
    font_traits::{
        ContentIndex, FontLoaderTrait, ImageSource, InlineContent, InlineImage, InlineShape,
        LayoutFragment, ObjectFit, ParsedFontTrait, SegmentAlignment, ShapeBoundary,
        ShapeDefinition, ShapedItem, Size, StyleProperties, StyledRun, TextLayoutCache,
        UnifiedConstraints,
    },
    solver3::{
        geometry::{BoxProps, EdgeSizes, IntrinsicSizes},
        getters::{
            get_css_height, get_css_width, get_display_property, get_element_font_size, get_float,
            get_list_style_position, get_list_style_type, get_overflow_x, get_overflow_y,
            get_parent_font_size, get_root_font_size, get_style_properties, get_writing_mode,
            MultiValue,
        },
        layout_tree::{
            AnonymousBoxType, CachedInlineLayout, LayoutNode, LayoutTree, PseudoElement,
        },
        positioning::get_position_type,
        scrollbar::ScrollbarRequirements,
        sizing::extract_text_from_node,
        taffy_bridge, LayoutContext, LayoutDebugMessage, LayoutError, Result,
    },
    text3::cache::{AvailableSpace as Text3AvailableSpace, TextAlign as Text3TextAlign},
};

/// Default scrollbar width in pixels (CSS Overflow Module Level 3).
/// Used when `overflow: scroll` or `overflow: auto` triggers scrollbar display.
pub const SCROLLBAR_WIDTH_PX: f32 = 16.0;

// Note: DEFAULT_FONT_SIZE and PT_TO_PX are imported from pixel

/// Result of BFC layout with margin escape information
#[derive(Debug, Clone)]
pub(crate) struct BfcLayoutResult {
    /// Standard layout output (positions, overflow size, baseline)
    pub output: LayoutOutput,
    /// Top margin that escaped the BFC (for parent-child collapse)
    /// If Some, this margin should be used by parent instead of positioning this BFC
    pub escaped_top_margin: Option<f32>,
    /// Bottom margin that escaped the BFC (for parent-child collapse)
    /// If Some, this margin should collapse with next sibling
    pub escaped_bottom_margin: Option<f32>,
}

impl BfcLayoutResult {
    pub fn from_output(output: LayoutOutput) -> Self {
        Self {
            output,
            escaped_top_margin: None,
            escaped_bottom_margin: None,
        }
    }
}

/// The CSS `overflow` property behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowBehavior {
    Visible,
    Hidden,
    Clip,
    Scroll,
    Auto,
}

impl OverflowBehavior {
    pub fn is_clipped(&self) -> bool {
        matches!(self, Self::Hidden | Self::Clip | Self::Scroll | Self::Auto)
    }

    pub fn is_scroll(&self) -> bool {
        matches!(self, Self::Scroll | Self::Auto)
    }
}

/// Input constraints for a layout function.
#[derive(Debug)]
pub struct LayoutConstraints<'a> {
    /// The available space for the content, excluding padding and borders.
    pub available_size: LogicalSize,
    /// The CSS writing-mode of the context.
    pub writing_mode: LayoutWritingMode,
    /// The state of the parent Block Formatting Context, if applicable.
    /// This is how state (like floats) is passed down.
    pub bfc_state: Option<&'a mut BfcState>,
    // Other properties like text-align would go here.
    pub text_align: TextAlign,
    /// The size of the containing block (parent's content box).
    /// This is used for resolving percentage-based sizes and as parent_size for Taffy.
    pub containing_block_size: LogicalSize,
    /// The semantic type of the available width constraint.
    ///
    /// This field is crucial for correct inline layout caching:
    /// - `Definite(w)`: Normal layout with a specific available width
    /// - `MinContent`: Intrinsic minimum width measurement (maximum wrapping)
    /// - `MaxContent`: Intrinsic maximum width measurement (no wrapping)
    ///
    /// When caching inline layouts, we must track which constraint type was used
    /// to compute the cached result. A layout computed with `MinContent` (width=0)
    /// must not be reused when the actual available width is known.
    pub available_width_type: Text3AvailableSpace,
}

/// Manages all layout state for a single Block Formatting Context.
/// This struct is created by the BFC root and lives for the duration of its layout.
#[derive(Debug, Clone)]
pub struct BfcState {
    /// The current position for the next in-flow block element.
    pub pen: LogicalPosition,
    /// The state of all floated elements within this BFC.
    pub floats: FloatingContext,
    /// The state of margin collapsing within this BFC.
    pub margins: MarginCollapseContext,
}

impl BfcState {
    pub fn new() -> Self {
        Self {
            pen: LogicalPosition::zero(),
            floats: FloatingContext::default(),
            margins: MarginCollapseContext::default(),
        }
    }
}

/// Manages vertical margin collapsing within a BFC.
#[derive(Debug, Default, Clone)]
pub struct MarginCollapseContext {
    /// The bottom margin of the last in-flow, block-level element.
    /// Can be positive or negative.
    pub last_in_flow_margin_bottom: f32,
}

/// The result of laying out a formatting context.
#[derive(Debug, Default, Clone)]
pub struct LayoutOutput {
    /// The final positions of child nodes, relative to the container's content-box origin.
    pub positions: BTreeMap<usize, LogicalPosition>,
    /// The total size occupied by the content, which may exceed `available_size`.
    pub overflow_size: LogicalSize,
    /// The baseline of the context, if applicable, measured from the top of its content box.
    pub baseline: Option<f32>,
}

/// Text alignment options
#[derive(Debug, Clone, Copy, Default)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Center,
    Justify,
}

/// Represents a single floated element within a BFC.
#[derive(Debug, Clone, Copy)]
struct FloatBox {
    /// The type of float (Left or Right).
    kind: LayoutFloat,
    /// The rectangle of the float's content box (origin includes top/left margin offset).
    rect: LogicalRect,
    /// The margin sizes (needed to calculate true margin-box bounds).
    margin: EdgeSizes,
}

/// Manages the state of all floated elements within a Block Formatting Context.
#[derive(Debug, Default, Clone)]
pub struct FloatingContext {
    /// All currently positioned floats within the BFC.
    pub floats: Vec<FloatBox>,
}

impl FloatingContext {
    /// Add a newly positioned float to the context
    pub fn add_float(&mut self, kind: LayoutFloat, rect: LogicalRect, margin: EdgeSizes) {
        self.floats.push(FloatBox { kind, rect, margin });
    }

    /// Finds the available space on the cross-axis for a line box at a given main-axis range.
    ///
    /// Returns a tuple of (`cross_start_offset`, `cross_end_offset`) relative to the
    /// BFC content box, defining the available space for an in-flow element.
    pub fn available_line_box_space(
        &self,
        main_start: f32,
        main_end: f32,
        bfc_cross_size: f32,
        wm: LayoutWritingMode,
    ) -> (f32, f32) {
        let mut available_cross_start = 0.0_f32;
        let mut available_cross_end = bfc_cross_size;

        for float in &self.floats {
            // Get the logical main-axis span of the existing float.
            let float_main_start = float.rect.origin.main(wm);
            let float_main_end = float_main_start + float.rect.size.main(wm);

            // Check for overlap on the main axis.
            if main_end > float_main_start && main_start < float_main_end {
                // The float overlaps with the main-axis range of the element we're placing.
                let float_cross_start = float.rect.origin.cross(wm);
                let float_cross_end = float_cross_start + float.rect.size.cross(wm);

                if float.kind == LayoutFloat::Left {
                    // "line-left", i.e., cross-start
                    available_cross_start = available_cross_start.max(float_cross_end);
                } else {
                    // Float::Right, i.e., cross-end
                    available_cross_end = available_cross_end.min(float_cross_start);
                }
            }
        }
        (available_cross_start, available_cross_end)
    }

    /// Returns the main-axis offset needed to be clear of floats of the given type.
    pub fn clearance_offset(
        &self,
        clear: LayoutClear,
        current_main_offset: f32,
        wm: LayoutWritingMode,
    ) -> f32 {
        let mut max_end_offset = 0.0_f32;

        let check_left = clear == LayoutClear::Left || clear == LayoutClear::Both;
        let check_right = clear == LayoutClear::Right || clear == LayoutClear::Both;

        for float in &self.floats {
            let should_clear_this_float = (check_left && float.kind == LayoutFloat::Left)
                || (check_right && float.kind == LayoutFloat::Right);

            if should_clear_this_float {
                // CSS 2.2 § 9.5.2: "the top border edge of the box be below the bottom outer edge"
                // Outer edge = margin-box boundary (content + padding + border + margin)
                let float_margin_box_end = float.rect.origin.main(wm)
                    + float.rect.size.main(wm)
                    + float.margin.main_end(wm);
                max_end_offset = max_end_offset.max(float_margin_box_end);
            }
        }

        if max_end_offset > current_main_offset {
            max_end_offset
        } else {
            current_main_offset
        }
    }
}

/// Encapsulates all state needed to lay out a single Block Formatting Context.
struct BfcLayoutState {
    /// The current position for the next in-flow block element.
    pen: LogicalPosition,
    floats: FloatingContext,
    margins: MarginCollapseContext,
    /// The writing mode of the BFC root.
    writing_mode: LayoutWritingMode,
}

/// Result of a formatting context layout operation
#[derive(Debug, Default)]
pub struct LayoutResult {
    pub positions: Vec<(usize, LogicalPosition)>,
    pub overflow_size: Option<LogicalSize>,
    pub baseline_offset: f32,
}

// Entry Point & Dispatcher

/// Main dispatcher for formatting context layout.
///
/// Routes layout to the appropriate formatting context handler based on the node's
/// `formatting_context` property. This is the main entry point for all layout operations.
///
/// # CSS Spec References
/// - CSS 2.2 § 9.4: Formatting contexts
/// - CSS Flexbox § 3: Flex formatting contexts
/// - CSS Grid § 5: Grid formatting contexts
pub fn layout_formatting_context<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints,
    float_cache: &mut std::collections::BTreeMap<usize, FloatingContext>,
) -> Result<BfcLayoutResult> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    debug_info!(
        ctx,
        "[layout_formatting_context] node_index={}, fc={:?}, available_size={:?}",
        node_index,
        node.formatting_context,
        constraints.available_size
    );

    match node.formatting_context {
        FormattingContext::Block { .. } => {
            layout_bfc(ctx, tree, text_cache, node_index, constraints, float_cache)
        }
        FormattingContext::Inline => layout_ifc(ctx, text_cache, tree, node_index, constraints)
            .map(BfcLayoutResult::from_output),
        FormattingContext::Table => layout_table_fc(ctx, tree, text_cache, node_index, constraints)
            .map(BfcLayoutResult::from_output),
        FormattingContext::Flex | FormattingContext::Grid => {
            layout_flex_grid(ctx, tree, text_cache, node_index, constraints)
        }
        _ => {
            // Unknown formatting context - fall back to BFC
            let mut temp_float_cache = std::collections::BTreeMap::new();
            layout_bfc(
                ctx,
                tree,
                text_cache,
                node_index,
                constraints,
                &mut temp_float_cache,
            )
        }
    }
}

// Flex / grid layout (taffy Bridge)

/// Lays out a Flex or Grid formatting context using the Taffy layout engine.
///
/// # CSS Spec References
///
/// - CSS Flexbox § 9: Flex Layout Algorithm
/// - CSS Grid § 12: Grid Layout Algorithm
///
/// # Implementation Notes
///
/// - Resolves explicit CSS dimensions to pixel values for `known_dimensions`
/// - Uses `InherentSize` mode when explicit dimensions are set
/// - Uses `ContentSize` mode for auto-sizing (shrink-to-fit)
fn layout_flex_grid<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<BfcLayoutResult> {
    let available_space = TaffySize {
        width: AvailableSpace::Definite(constraints.available_size.width),
        height: AvailableSpace::Definite(constraints.available_size.height),
    };

    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    // Resolve explicit CSS dimensions to pixel values.
    // This is CRITICAL for align-items: stretch to work correctly!
    // Taffy uses known_dimensions to calculate cross_axis_available_space for children.
    let (explicit_width, has_explicit_width) =
        resolve_explicit_dimension_width(ctx, node, constraints);
    let (explicit_height, has_explicit_height) =
        resolve_explicit_dimension_height(ctx, node, constraints);

    // FIX: Taffy interprets known_dimensions as Border Box size.
    // CSS width/height properties define Content Box size (by default, box-sizing: content-box).
    // We must add border and padding to the explicit dimensions to get the correct Border
    // Box size for Taffy.
    let width_adjustment = node.box_props.border.left
        + node.box_props.border.right
        + node.box_props.padding.left
        + node.box_props.padding.right;
    let height_adjustment = node.box_props.border.top
        + node.box_props.border.bottom
        + node.box_props.padding.top
        + node.box_props.padding.bottom;

    // Apply adjustment only if dimensions are explicit (convert content-box to border-box)
    let adjusted_width = explicit_width.map(|w| w + width_adjustment);
    let adjusted_height = explicit_height.map(|h| h + height_adjustment);

    // CSS Flexbox § 9.2: Use InherentSize when explicit dimensions are set,
    // ContentSize for auto-sizing (shrink-to-fit behavior).
    let sizing_mode = if has_explicit_width || has_explicit_height {
        taffy::SizingMode::InherentSize
    } else {
        taffy::SizingMode::ContentSize
    };

    let known_dimensions = TaffySize {
        width: adjusted_width,
        height: adjusted_height,
    };

    let taffy_inputs = LayoutInput {
        known_dimensions,
        parent_size: translate_taffy_size(constraints.containing_block_size),
        available_space,
        run_mode: taffy::RunMode::PerformLayout,
        sizing_mode,
        axis: taffy::RequestedAxis::Both,
        // Flex and Grid containers establish a new BFC, preventing margin collapse.
        vertical_margins_are_collapsible: Line::FALSE,
    };

    debug_info!(
        ctx,
        "CALLING LAYOUT_TAFFY FOR FLEX/GRID FC node_index={:?}",
        node_index
    );

    let taffy_output =
        taffy_bridge::layout_taffy_subtree(ctx, tree, text_cache, node_index, taffy_inputs);

    // Collect child positions from the tree (Taffy stores results directly on nodes).
    let mut output = LayoutOutput::default();
    // Use content_size for overflow detection, not container size.
    // content_size represents the actual size of all children, which may exceed the container.
    output.overflow_size = translate_taffy_size_back(taffy_output.content_size);

    let children: Vec<usize> = tree.get(node_index).unwrap().children.clone();
    for &child_idx in &children {
        if let Some(child_node) = tree.get(child_idx) {
            if let Some(pos) = child_node.relative_position {
                output.positions.insert(child_idx, pos);
            }
        }
    }

    Ok(BfcLayoutResult::from_output(output))
}

/// Resolves explicit CSS width to pixel value for Taffy layout.
fn resolve_explicit_dimension_width<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
    constraints: &LayoutConstraints,
) -> (Option<f32>, bool) {
    node.dom_node_id
        .map(|id| {
            let width = get_css_width(
                ctx.styled_dom,
                id,
                &ctx.styled_dom.styled_nodes.as_container()[id].styled_node_state,
            );
            match width.unwrap_or_default() {
                LayoutWidth::Auto => (None, false),
                LayoutWidth::Px(px) => {
                    let pixels = resolve_size_metric(
                        px.metric,
                        px.number.get(),
                        constraints.available_size.width,
                    );
                    (Some(pixels), true)
                }
                LayoutWidth::MinContent | LayoutWidth::MaxContent => (None, false),
            }
        })
        .unwrap_or((None, false))
}

/// Resolves explicit CSS height to pixel value for Taffy layout.
fn resolve_explicit_dimension_height<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
    constraints: &LayoutConstraints,
) -> (Option<f32>, bool) {
    node.dom_node_id
        .map(|id| {
            let height = get_css_height(
                ctx.styled_dom,
                id,
                &ctx.styled_dom.styled_nodes.as_container()[id].styled_node_state,
            );
            match height.unwrap_or_default() {
                LayoutHeight::Auto => (None, false),
                LayoutHeight::Px(px) => {
                    let pixels = resolve_size_metric(
                        px.metric,
                        px.number.get(),
                        constraints.available_size.height,
                    );
                    (Some(pixels), true)
                }
                LayoutHeight::MinContent | LayoutHeight::MaxContent => (None, false),
            }
        })
        .unwrap_or((None, false))
}

/// Position a float within a BFC, considering existing floats.
/// Returns the LogicalRect (margin box) for the float.
fn position_float(
    float_ctx: &FloatingContext,
    float_type: LayoutFloat,
    size: LogicalSize,
    margin: &EdgeSizes,
    current_main_offset: f32,
    bfc_cross_size: f32,
    wm: LayoutWritingMode,
) -> LogicalRect {
    // Start at the current main-axis position (Y in horizontal-tb)
    let mut main_start = current_main_offset;

    // Calculate total size including margins
    let total_main = size.main(wm) + margin.main_start(wm) + margin.main_end(wm);
    let total_cross = size.cross(wm) + margin.cross_start(wm) + margin.cross_end(wm);

    // Find a position where the float fits
    let cross_start = loop {
        let (avail_start, avail_end) = float_ctx.available_line_box_space(
            main_start,
            main_start + total_main,
            bfc_cross_size,
            wm,
        );

        let available_width = avail_end - avail_start;

        if available_width >= total_cross {
            // Found space that fits
            if float_type == LayoutFloat::Left {
                // Position at line-left (avail_start)
                break avail_start + margin.cross_start(wm);
            } else {
                // Position at line-right (avail_end - size)
                break avail_end - total_cross + margin.cross_start(wm);
            }
        }

        // Not enough space at this Y, move down past the lowest overlapping float
        let next_main = float_ctx
            .floats
            .iter()
            .filter(|f| {
                let f_main_start = f.rect.origin.main(wm);
                let f_main_end = f_main_start + f.rect.size.main(wm);
                f_main_end > main_start && f_main_start < main_start + total_main
            })
            .map(|f| f.rect.origin.main(wm) + f.rect.size.main(wm))
            .max_by(|a, b| a.partial_cmp(b).unwrap());

        if let Some(next) = next_main {
            main_start = next;
        } else {
            // No overlapping floats found, use current position anyway
            if float_type == LayoutFloat::Left {
                break avail_start + margin.cross_start(wm);
            } else {
                break avail_end - total_cross + margin.cross_start(wm);
            }
        }
    };

    LogicalRect {
        origin: LogicalPosition::from_main_cross(
            main_start + margin.main_start(wm),
            cross_start,
            wm,
        ),
        size,
    }
}

// Block Formatting Context (CSS 2.2 § 9.4.1)

/// Lays out a Block Formatting Context (BFC).
///
/// This is the corrected, architecturally-sound implementation. It solves the
/// "chicken-and-egg" problem by performing its own two-pass layout:
///
/// 1. **Sizing Pass:** It first iterates through its children and triggers their layout recursively
///    by calling `calculate_layout_for_subtree`. This ensures that the `used_size` property of each
///    child is correctly populated.
///
/// 2. **Positioning Pass:** It then iterates through the children again. Now that each child has a
///    valid size, it can apply the standard block-flow logic: stacking them vertically and
///    advancing a "pen" by each child's outer height.
///
/// # Margin Collapsing Architecture
///
/// CSS 2.1 Section 8.3.1 compliant margin collapsing:
///
/// ```text
/// layout_bfc()
///   ├─ Check parent border/padding blockers
///   ├─ For each child:
///   │   ├─ Check child border/padding blockers
///   │   ├─ is_first_child?
///   │   │   └─ Check parent-child top collapse
///   │   ├─ Sibling collapse?
///   │   │   └─ advance_pen_with_margin_collapse()
///   │   │       └─ collapse_margins(prev_bottom, curr_top)
///   │   ├─ Position child
///   │   ├─ is_empty_block()?
///   │   │   └─ Collapse own top+bottom margins (collapse through)
///   │   └─ Save bottom margin for next sibling
///   └─ Check parent-child bottom collapse
/// ```
///
/// **Collapsing Rules:**
///
/// - Sibling margins: Adjacent vertical margins collapse to max (or sum if mixed signs)
/// - Parent-child: First child's top margin can escape parent (if no border/padding)
/// - Parent-child: Last child's bottom margin can escape parent (if no border/padding/height)
/// - Empty blocks: Top+bottom margins collapse with each other, then with siblings
/// - Blockers: Border, padding, inline content, or new BFC prevents collapsing
///
/// This approach is compliant with the CSS visual formatting model and works within
/// the constraints of the existing layout engine architecture.
fn layout_bfc<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints,
    float_cache: &mut std::collections::BTreeMap<usize, FloatingContext>,
) -> Result<BfcLayoutResult> {
    let node = tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?
        .clone();
    let writing_mode = constraints.writing_mode;
    let mut output = LayoutOutput::default();

    debug_info!(
        ctx,
        "\n[layout_bfc] ENTERED for node_index={}, children.len()={}, incoming_bfc_state={}",
        node_index,
        node.children.len(),
        constraints.bfc_state.is_some()
    );

    // Initialize FloatingContext for this BFC
    //
    // We always recalculate float positions in this pass, but we'll store them in the cache
    // so that subsequent layout passes (for auto-sizing) have access to the positioned floats
    let mut float_context = FloatingContext::default();

    // Calculate this node's content-box size for use as containing block for children
    // CSS 2.2 § 10.1: The containing block for in-flow children is formed by the
    // content edge of the parent's content box.
    //
    // We use constraints.available_size directly as this already represents the
    // content-box available to this node (set by parent). For nodes with explicit
    // sizes, used_size contains the border-box which we convert to content-box.
    let mut children_containing_block_size = if let Some(used_size) = node.used_size {
        // Node has explicit used_size (border-box) - convert to content-box
        node.box_props.inner_size(used_size, writing_mode)
    } else {
        // No used_size yet - use available_size directly (this is already content-box
        // when coming from parent's layout constraints)
        constraints.available_size
    };

    // Proactively reserve space for vertical scrollbar if overflow-y is auto/scroll.
    // This ensures children are laid out with the correct available width from the start,
    // preventing the "children overlap scrollbar" layout issue.
    let scrollbar_reservation = node
        .dom_node_id
        .map(|dom_id| {
            let styled_node_state = ctx
                .styled_dom
                .styled_nodes
                .as_container()
                .get(dom_id)
                .map(|s| s.styled_node_state.clone())
                .unwrap_or_default();
            let overflow_y =
                crate::solver3::getters::get_overflow_y(ctx.styled_dom, dom_id, &styled_node_state);
            use azul_css::props::layout::LayoutOverflow;
            match overflow_y.unwrap_or_default() {
                LayoutOverflow::Scroll | LayoutOverflow::Auto => SCROLLBAR_WIDTH_PX,
                _ => 0.0,
            }
        })
        .unwrap_or(0.0);

    if scrollbar_reservation > 0.0 {
        children_containing_block_size.width =
            (children_containing_block_size.width - scrollbar_reservation).max(0.0);
    }

    // Pass 1: Size all children (floats and normal flow)
    for &child_index in &node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_dom_id = child_node.dom_node_id;

        // Skip out-of-flow children (absolute/fixed)
        let position_type = get_position_type(ctx.styled_dom, child_dom_id);
        if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
            continue;
        }

        // Size all children (floats and normal flow) - floats will be positioned later in Pass 2
        let mut temp_positions = BTreeMap::new();
        crate::solver3::cache::calculate_layout_for_subtree(
            ctx,
            tree,
            text_cache,
            child_index,
            LogicalPosition::zero(),
            children_containing_block_size, // Use this node's content-box as containing block
            &mut temp_positions,
            &mut bool::default(),
            float_cache,
        )?;
    }

    // Pass 2: Single-pass interleaved layout (position floats and normal flow in DOM order)

    let mut main_pen = 0.0f32;
    let mut max_cross_size = 0.0f32;

    // Track escaped margins separately from content-box height
    // CSS 2.2 § 8.3.1: Escaped margins don't contribute to parent's content-box height,
    // but DO affect sibling positioning within the parent
    let mut total_escaped_top_margin = 0.0f32;
    // Track all inter-sibling margins (collapsed) - these are also not part of content height
    let mut total_sibling_margins = 0.0f32;

    // Margin collapsing state
    let mut last_margin_bottom = 0.0f32;
    let mut is_first_child = true;
    let mut first_child_index: Option<usize> = None;
    let mut last_child_index: Option<usize> = None;

    // Parent's own margins (for escape calculation)
    let parent_margin_top = node.box_props.margin.main_start(writing_mode);
    let parent_margin_bottom = node.box_props.margin.main_end(writing_mode);

    // Check if parent (this BFC root) has border/padding that prevents parent-child collapse
    let parent_has_top_blocker = has_margin_collapse_blocker(&node.box_props, writing_mode, true);
    let parent_has_bottom_blocker =
        has_margin_collapse_blocker(&node.box_props, writing_mode, false);

    // Track accumulated top margin for first-child escape
    let mut accumulated_top_margin = 0.0f32;
    let mut top_margin_resolved = false;
    // Track if first child's margin escaped (for return value)
    let mut top_margin_escaped = false;

    // Track if we have any actual content (non-empty blocks)
    let mut has_content = false;

    for &child_index in &node.children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_dom_id = child_node.dom_node_id;

        let position_type = get_position_type(ctx.styled_dom, child_dom_id);
        if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
            continue;
        }

        // Check if this child is a float - if so, position it at current main_pen
        let is_float = if let Some(node_id) = child_dom_id {
            let float_type = get_float_property(ctx.styled_dom, Some(node_id));

            if float_type != LayoutFloat::None {
                let float_size = child_node.used_size.unwrap_or_default();
                let float_margin = &child_node.box_props.margin;

                // CSS 2.2 § 9.5: Float margins don't collapse with any other margins.
                // If there's a previous in-flow element with a bottom margin, we must
                // include it in the Y position calculation for this float.
                let float_y = main_pen + last_margin_bottom;

                debug_info!(
                    ctx,
                    "[layout_bfc] Positioning float: index={}, type={:?}, size={:?}, at Y={} \
                     (main_pen={} + last_margin={})",
                    child_index,
                    float_type,
                    float_size,
                    float_y,
                    main_pen,
                    last_margin_bottom
                );

                // Position the float at the CURRENT main_pen + last margin (respects DOM order!)
                let float_rect = position_float(
                    &float_context,
                    float_type,
                    float_size,
                    float_margin,
                    // Include last_margin_bottom since float margins don't collapse!
                    float_y,
                    constraints.available_size.cross(writing_mode),
                    writing_mode,
                );

                debug_info!(ctx, "[layout_bfc] Float positioned at: {:?}", float_rect);

                // Add to float context BEFORE positioning next element
                float_context.add_float(float_type, float_rect, *float_margin);

                // Store position in output
                output.positions.insert(child_index, float_rect.origin);

                debug_info!(
                    ctx,
                    "[layout_bfc] *** FLOAT POSITIONED: child={}, main_pen={} (unchanged - floats \
                     don't advance pen)",
                    child_index,
                    main_pen
                );

                // Floats are taken out of normal flow - DON'T advance main_pen
                // Continue to next child
                continue;
            }
            false
        } else {
            false
        };

        // Early exit for floats (already handled above)
        if is_float {
            continue;
        }

        // From here: normal flow (non-float) children only

        // Track first and last in-flow children for parent-child collapse
        if first_child_index.is_none() {
            first_child_index = Some(child_index);
        }
        last_child_index = Some(child_index);

        let child_size = child_node.used_size.unwrap_or_default();
        let child_margin = &child_node.box_props.margin;

        debug_info!(
            ctx,
            "[layout_bfc] Child {} margin from box_props: top={}, right={}, bottom={}, left={}",
            child_index,
            child_margin.top,
            child_margin.right,
            child_margin.bottom,
            child_margin.left
        );

        // IMPORTANT: Use the ACTUAL margins from box_props, NOT escaped margins!
        //
        // Escaped margins are only relevant for the parent-child relationship WITHIN a node's
        // own BFC layout. When positioning this child in ITS parent's BFC, we use its actual
        // margins. CSS 2.2 § 8.3.1: Margin collapsing happens between ADJACENT margins,
        // which means:
        //
        // - Parent's top and first child's top (if no blocker)
        // - Sibling's bottom and next sibling's top
        // - Parent's bottom and last child's bottom (if no blocker)
        //
        // The escaped_top_margin stored in the child node is for its OWN children, not for itself!
        let child_margin_top = child_margin.main_start(writing_mode);
        let child_margin_bottom = child_margin.main_end(writing_mode);

        debug_info!(
            ctx,
            "[layout_bfc] Child {} final margins: margin_top={}, margin_bottom={}",
            child_index,
            child_margin_top,
            child_margin_bottom
        );

        // Check if this child has border/padding that prevents margin collapsing
        let child_has_top_blocker =
            has_margin_collapse_blocker(&child_node.box_props, writing_mode, true);
        let child_has_bottom_blocker =
            has_margin_collapse_blocker(&child_node.box_props, writing_mode, false);

        // Check for clear property FIRST - clearance affects whether element is considered empty
        // CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing"
        // An element with clearance is NOT empty even if it has no content
        let child_clear = if let Some(node_id) = child_dom_id {
            get_clear_property(ctx.styled_dom, Some(node_id))
        } else {
            LayoutClear::None
        };
        debug_info!(
            ctx,
            "[layout_bfc] Child {} clear property: {:?}",
            child_index,
            child_clear
        );

        // PHASE 1: Empty Block Detection & Self-Collapse
        let is_empty = is_empty_block(child_node);

        // Handle empty blocks FIRST (they collapse through and don't participate in layout)
        // EXCEPTION: Elements with clear property are NOT skipped even if empty!
        // CSS 2.2 § 9.5.2: Clear property affects positioning even for empty elements
        if is_empty
            && !child_has_top_blocker
            && !child_has_bottom_blocker
            && child_clear == LayoutClear::None
        {
            // Empty block: collapse its own top and bottom margins FIRST
            let self_collapsed = collapse_margins(child_margin_top, child_margin_bottom);

            // Then collapse with previous margin (sibling or parent)
            if is_first_child {
                is_first_child = false;
                // Empty first child: its collapsed margin can escape with parent's
                if !parent_has_top_blocker {
                    accumulated_top_margin = collapse_margins(parent_margin_top, self_collapsed);
                } else {
                    // Parent has blocker: add margins
                    if accumulated_top_margin == 0.0 {
                        accumulated_top_margin = parent_margin_top;
                    }
                    main_pen += accumulated_top_margin + self_collapsed;
                    top_margin_resolved = true;
                    accumulated_top_margin = 0.0;
                }
                last_margin_bottom = self_collapsed;
            } else {
                // Empty sibling: collapse with previous sibling's bottom margin
                last_margin_bottom = collapse_margins(last_margin_bottom, self_collapsed);
            }

            // Skip positioning and pen advance (empty has no visual presence)
            continue;
        }

        // From here on: non-empty blocks only (or empty blocks with clear property)

        // Apply clearance if needed
        // CSS 2.2 § 9.5.2: Clearance inhibits margin collapsing
        let clearance_applied = if child_clear != LayoutClear::None {
            let cleared_offset =
                float_context.clearance_offset(child_clear, main_pen, writing_mode);
            debug_info!(
                ctx,
                "[layout_bfc] Child {} clearance check: cleared_offset={}, main_pen={}",
                child_index,
                cleared_offset,
                main_pen
            );
            if cleared_offset > main_pen {
                debug_info!(
                    ctx,
                    "[layout_bfc] Applying clearance: child={}, clear={:?}, old_pen={}, new_pen={}",
                    child_index,
                    child_clear,
                    main_pen,
                    cleared_offset
                );
                main_pen = cleared_offset;
                true // Signal that clearance was applied
            } else {
                false
            }
        } else {
            false
        };

        // PHASE 2: Parent-Child Top Margin Escape (First Child)
        //
        // CSS 2.2 § 8.3.1: "The top margin of a box is adjacent to the top margin of its first
        // in-flow child if the box has no top border, no top padding, and the child has no
        // clearance." CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing"

        if is_first_child {
            is_first_child = false;

            // Clearance prevents collapse (acts as invisible blocker)
            if clearance_applied {
                // Clearance inhibits all margin collapsing for this element
                // The clearance has already positioned main_pen past floats
                //
                // CSS 2.2 § 8.3.1: Parent's margin was already handled by parent's parent BFC
                // We only add child's margin in our content-box coordinate space
                main_pen += child_margin_top;
                debug_info!(
                    ctx,
                    "[layout_bfc] First child {} with CLEARANCE: no collapse, child_margin={}, \
                     main_pen={}",
                    child_index,
                    child_margin_top,
                    main_pen
                );
            } else if !parent_has_top_blocker {
                // Margin Escape Case
                //
                // CSS 2.2 § 8.3.1: "The top margin of an in-flow block element collapses with
                // its first in-flow block-level child's top margin if the element has no top
                // border, no top padding, and the child has no clearance."
                //
                // When margins collapse, they "escape" upward through the parent to be resolved
                // in the grandparent's coordinate space. This is critical for understanding the
                // coordinate system separation:
                //
                // Example:
                // <body padding=20>
                //  <div margin=0>
                //      <div margin=30></div>
                //  </div>
                // </body>
                //
                //   - Middle div (our parent) has no padding → margins can escape
                //   - Inner div's 30px margin collapses with middle div's 0px margin = 30px
                //   - This 30px margin "escapes" to be handled by body's BFC
                //   - Body positions middle div at Y=30 (relative to body's content-box)
                //   - Middle div's content-box height does NOT include the escaped 30px
                //   - Inner div is positioned at Y=0 in middle div's content-box
                //
                // **NOTE**: This is a subtle but critical distinction in coordinate systems:
                //
                //   - Parent's margin belongs to grandparent's coordinate space
                //   - Child's margin (when escaped) also belongs to grandparent's coordinate space
                //   - They collapse BEFORE entering this BFC's coordinate space
                //   - We return the collapsed margin so grandparent can position parent correctly
                //
                // **NOTE**: Child's own blocker status (padding/border) is IRRELEVANT for
                // parent-child  collapse. The child may have padding that prevents
                // collapse with ITS OWN  children, but this doesn't prevent its
                // margin from escaping  through its parent.
                //
                // **NOTE**: Previously, we incorrectly added parent_margin_top to main_pen in
                //  the blocked case, which double-counted the margin by mixing
                //  coordinate systems. The parent's margin is NEVER in our (the
                //  parent's content-box) coordinate system!

                accumulated_top_margin = collapse_margins(parent_margin_top, child_margin_top);
                top_margin_resolved = true;
                top_margin_escaped = true;

                // Track escaped margin so it gets subtracted from content-box height
                // The escaped margin is NOT part of our content-box - it belongs to our
                // parent's parent
                total_escaped_top_margin = accumulated_top_margin;

                // Position child at pen (no margin applied - it escaped!)
                debug_info!(
                    ctx,
                    "[layout_bfc] First child {} margin ESCAPES: parent_margin={}, \
                     child_margin={}, collapsed={}, total_escaped={}",
                    child_index,
                    parent_margin_top,
                    child_margin_top,
                    accumulated_top_margin,
                    total_escaped_top_margin
                );
            } else {
                // Margin Blocked Case
                //
                // CSS 2.2 § 8.3.1: "no top padding and no top border" required for collapse.
                // When padding or border exists, margins do NOT collapse and exist in different
                // coordinate spaces.
                //
                // CRITICAL COORDINATE SYSTEM SEPARATION:
                //
                //   This is where the architecture becomes subtle. When layout_bfc() is called:
                //   1. We are INSIDE the parent's content-box coordinate space (main_pen starts at
                //      0)
                //   2. The parent's own margin was ALREADY RESOLVED by the grandparent's BFC
                //   3. The parent's margin is in the grandparent's coordinate space, not ours
                //   4. We NEVER reference the parent's margin in this BFC - it's outside our scope
                //
                // Example:
                //
                // <body padding=20>
                //   <div margin=30 padding=20>
                //      <div margin=30></div>
                //   </div>
                // </body>
                //
                //   - Middle div has padding=20 → blocker exists, margins don't collapse
                //   - Body's BFC positions middle div at Y=30 (middle div's margin, in body's
                //     space)
                //   - Middle div's BFC starts at its content-box (after the padding)
                //   - main_pen=0 at the top of middle div's content-box
                //   - Inner div has margin=30 → we add 30 to main_pen (in OUR coordinate space)
                //   - Inner div positioned at Y=30 (relative to middle div's content-box)
                //   - Absolute position: 20 (body padding) + 30 (middle margin) + 20 (middle
                //     padding) + 30 (inner margin) = 100px
                //
                // **NOTE**: Previous code incorrectly added parent_margin_top to main_pen here:
                //
                //     - main_pen += parent_margin_top;  // WRONG! Mixes coordinate systems
                //     - main_pen += child_margin_top;
                //
                //   This caused the "double margin" bug where margins were applied twice:
                //
                //   - Once by grandparent positioning parent (correct)
                //   - Again inside parent's BFC (INCORRECT - wrong coordinate system)
                //
                //   The parent's margin belongs to GRANDPARENT's coordinate space and was already
                //   used to position the parent. Adding it again here is like adding feet to
                //   meters.
                //
                //   We ONLY add the child's margin in our (parent's content-box) coordinate space.
                //   The parent's margin is irrelevant to us - it's outside our scope.

                main_pen += child_margin_top;
                debug_info!(
                    ctx,
                    "[layout_bfc] First child {} BLOCKED: parent_has_blocker={}, advanced by \
                     child_margin={}, main_pen={}",
                    child_index,
                    parent_has_top_blocker,
                    child_margin_top,
                    main_pen
                );
            }
        } else {
            // Not first child: handle sibling collapse
            // CSS 2.2 § 8.3.1 Rule 1: "Vertical margins of adjacent block boxes in the normal flow
            // collapse" CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing"

            // Resolve accumulated top margin if not yet done (for parent's first in-flow child)
            if !top_margin_resolved {
                main_pen += accumulated_top_margin;
                top_margin_resolved = true;
                debug_info!(
                    ctx,
                    "[layout_bfc] RESOLVED top margin for node {} at sibling {}: accumulated={}, \
                     main_pen={}",
                    node_index,
                    child_index,
                    accumulated_top_margin,
                    main_pen
                );
            }

            if clearance_applied {
                // Clearance inhibits collapsing - add full margin
                main_pen += child_margin_top;
                debug_info!(
                    ctx,
                    "[layout_bfc] Child {} with CLEARANCE: no collapse with sibling, \
                     child_margin_top={}, main_pen={}",
                    child_index,
                    child_margin_top,
                    main_pen
                );
            } else {
                // Sibling Margin Collapse
                //
                // CSS 2.2 § 8.3.1: "Vertical margins of adjacent block boxes in the normal
                // flow collapse." The collapsed margin is the maximum of the two margins.
                //
                // IMPORTANT: Sibling margins ARE part of the parent's content-box height!
                //
                // Unlike escaped margins (which belong to grandparent's space), sibling margins
                // are the space BETWEEN children within our content-box.
                //
                // Example:
                //
                // <div>
                //  <div margin-bottom=30></div>
                //  <div margin-top=40></div>
                // </div>
                //
                //   - First child ends at Y=100 (including its content + margins)
                //   - Collapsed margin = max(30, 40) = 40px
                //   - Second child starts at Y=140 (100 + 40)
                //   - Parent's content-box height includes this 40px gap
                //
                // We track total_sibling_margins for debugging, but NOTE: we do **not**
                // subtract these from content-box height! They are part of the layout space.
                //
                // Previously we subtracted total_sibling_margins from content-box height:
                //
                //   content_box_height = main_pen - total_escaped_top_margin -
                // total_sibling_margins;
                //
                // This was wrong because sibling margins are between boxes (part of content),
                // not outside boxes (like escaped margins).

                let collapsed = collapse_margins(last_margin_bottom, child_margin_top);
                main_pen += collapsed;
                total_sibling_margins += collapsed;
                debug_info!(
                    ctx,
                    "[layout_bfc] Sibling collapse for child {}: last_margin_bottom={}, \
                     child_margin_top={}, collapsed={}, main_pen={}, total_sibling_margins={}",
                    child_index,
                    last_margin_bottom,
                    child_margin_top,
                    collapsed,
                    main_pen,
                    total_sibling_margins
                );
            }
        }

        // Position child (non-empty blocks only reach here)
        //
        // CSS 2.2 § 9.4.1: "In a block formatting context, each box's left outer edge touches
        // the left edge of the containing block (for right-to-left formatting, right edges touch).
        // This is true even in the presence of floats (although a box's line boxes may shrink
        // due to the floats), unless the box establishes a new block formatting context
        // (in which case the box itself may become narrower due to the floats)."
        //
        // CSS 2.2 § 9.5: "The border box of a table, a block-level replaced element, or an element
        // in the normal flow that establishes a new block formatting context (such as an element
        // with 'overflow' other than 'visible') must not overlap any floats in the same block
        // formatting context as the element itself."

        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let establishes_bfc = establishes_new_bfc(ctx, child_node);

        // Query available space considering floats ONLY if child establishes new BFC
        let (cross_start, cross_end, available_cross) = if establishes_bfc {
            // New BFC: Must shrink or move down to avoid overlapping floats
            let (start, end) = float_context.available_line_box_space(
                main_pen,
                main_pen + child_size.main(writing_mode),
                constraints.available_size.cross(writing_mode),
                writing_mode,
            );
            let available = end - start;

            debug_info!(
                ctx,
                "[layout_bfc] Child {} establishes BFC: shrinking to avoid floats, \
                 cross_range={}..{}, available_cross={}",
                child_index,
                start,
                end,
                available
            );

            (start, end, available)
        } else {
            // Normal flow: Overlaps floats, positioned at full width
            // Only the child's INLINE CONTENT (if any) wraps around floats
            let start = 0.0;
            let end = constraints.available_size.cross(writing_mode);
            let available = end - start;

            debug_info!(
                ctx,
                "[layout_bfc] Child {} is normal flow: overlapping floats at full width, \
                 available_cross={}",
                child_index,
                available
            );

            (start, end, available)
        };

        // Get child's margin and formatting context
        let (child_margin_cloned, is_inline_fc) = {
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            (
                child_node.box_props.margin.clone(),
                child_node.formatting_context == FormattingContext::Inline,
            )
        };
        let child_margin = &child_margin_cloned;

        // Position child
        // For normal flow blocks (including IFCs): position at full width (cross_start = 0)
        // For BFC-establishing blocks: position in available space between floats
        let (child_cross_pos, mut child_main_pos) = if establishes_bfc {
            // BFC: Position in space between floats
            (
                cross_start + child_margin.cross_start(writing_mode),
                main_pen,
            )
        } else {
            // Normal flow: Position at full width (floats don't affect box position)
            (child_margin.cross_start(writing_mode), main_pen)
        };

        // CSS 2.2 § 8.3.1: If child's top margin escaped through parent, adjust position
        // "If the top margin of a box collapses with its first child's top margin,
        // the top border edge of the box is defined to coincide with the top border edge of the
        // child." This means the child's margin appears ABOVE the parent, so we offset the
        // child down. IMPORTANT: This only applies to the FIRST child! For siblings, normal
        // margin collapse applies.
        let child_escaped_margin = child_node.escaped_top_margin.unwrap_or(0.0);
        let is_first_in_flow_child = Some(child_index) == first_child_index;

        if child_escaped_margin > 0.0 && is_first_in_flow_child {
            child_main_pos += child_escaped_margin;
            total_escaped_top_margin += child_escaped_margin;
            debug_info!(
                ctx,
                "[layout_bfc] FIRST child {} has escaped_top_margin={}, adjusting position from \
                 {} to {}, total_escaped={}",
                child_index,
                child_escaped_margin,
                main_pen,
                child_main_pos,
                total_escaped_top_margin
            );
        } else if child_escaped_margin > 0.0 {
            debug_info!(
                ctx,
                "[layout_bfc] NON-FIRST child {} has escaped_top_margin={} but NOT adjusting \
                 position (sibling margin collapse handles this)",
                child_index,
                child_escaped_margin
            );
        }

        let final_pos =
            LogicalPosition::from_main_cross(child_main_pos, child_cross_pos, writing_mode);

        debug_info!(
            ctx,
            "[layout_bfc] *** NORMAL FLOW BLOCK POSITIONED: child={}, final_pos={:?}, \
             main_pen={}, establishes_bfc={}",
            child_index,
            final_pos,
            main_pen,
            establishes_bfc
        );

        // Re-layout IFC children with float context for correct text wrapping
        // Normal flow blocks WITH inline content need float context propagated
        if is_inline_fc && !establishes_bfc {
            // Use cached floats if available (from previous layout passes),
            // otherwise use the floats positioned in this pass
            let floats_for_ifc = float_cache.get(&node_index).unwrap_or(&float_context);

            debug_info!(
                ctx,
                "[layout_bfc] Re-layouting IFC child {} (normal flow) with parent's float context \
                 at Y={}, child_cross_pos={}",
                child_index,
                main_pen,
                child_cross_pos
            );
            debug_info!(
                ctx,
                "[layout_bfc]   Using {} floats (from cache: {})",
                floats_for_ifc.floats.len(),
                float_cache.contains_key(&node_index)
            );

            // Translate float coordinates from BFC-relative to IFC-relative
            // The IFC child is positioned at (child_cross_pos, main_pen) in BFC coordinates
            // Floats need to be relative to the IFC's CONTENT-BOX origin (inside padding/border)
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let padding_border_cross = child_node.box_props.padding.cross_start(writing_mode)
                + child_node.box_props.border.cross_start(writing_mode);
            let padding_border_main = child_node.box_props.padding.main_start(writing_mode)
                + child_node.box_props.border.main_start(writing_mode);

            // Content-box origin in BFC coordinates
            let content_box_cross = child_cross_pos + padding_border_cross;
            let content_box_main = main_pen + padding_border_main;

            debug_info!(
                ctx,
                "[layout_bfc]   Border-box at ({}, {}), Content-box at ({}, {}), \
                 padding+border=({}, {})",
                child_cross_pos,
                main_pen,
                content_box_cross,
                content_box_main,
                padding_border_cross,
                padding_border_main
            );

            let mut ifc_floats = FloatingContext::default();
            for float_box in &floats_for_ifc.floats {
                // Convert float position from BFC coords to IFC CONTENT-BOX relative coords
                let float_rel_to_ifc = LogicalRect {
                    origin: LogicalPosition {
                        x: float_box.rect.origin.x - content_box_cross,
                        y: float_box.rect.origin.y - content_box_main,
                    },
                    size: float_box.rect.size,
                };

                debug_info!(
                    ctx,
                    "[layout_bfc] Float {:?}: BFC coords = {:?}, IFC-content-relative = {:?}",
                    float_box.kind,
                    float_box.rect,
                    float_rel_to_ifc
                );

                ifc_floats.add_float(float_box.kind, float_rel_to_ifc, float_box.margin);
            }

            // Create a BfcState with IFC-relative float coordinates
            let mut bfc_state = BfcState {
                pen: LogicalPosition::zero(), // IFC starts at its own origin
                floats: ifc_floats.clone(),
                margins: MarginCollapseContext::default(),
            };

            debug_info!(
                ctx,
                "[layout_bfc]   Created IFC-relative FloatingContext with {} floats",
                ifc_floats.floats.len()
            );

            // Get the IFC child's content-box size (after padding/border)
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let child_dom_id = child_node.dom_node_id;

            // For inline elements (display: inline), use containing block width as available
            // width. Inline elements flow within the containing block and wrap at its width.
            // CSS 2.2 § 10.3.1: For inline elements, available width = containing block width.
            let display = get_display_property(ctx.styled_dom, child_dom_id).unwrap_or_default();
            let child_content_size = if display == LayoutDisplay::Inline {
                // Inline elements use the containing block's content-box width
                LogicalSize::new(
                    children_containing_block_size.width,
                    children_containing_block_size.height,
                )
            } else {
                // Block-level elements use their own content-box
                child_node.box_props.inner_size(child_size, writing_mode)
            };

            debug_info!(
                ctx,
                "[layout_bfc]   IFC child size: border-box={:?}, content-box={:?}",
                child_size,
                child_content_size
            );

            // Create new constraints with float context
            // IMPORTANT: Use the child's CONTENT-BOX width, not the BFC width!
            let ifc_constraints = LayoutConstraints {
                available_size: child_content_size,
                bfc_state: Some(&mut bfc_state),
                writing_mode,
                text_align: constraints.text_align,
                containing_block_size: constraints.containing_block_size,
                available_width_type: Text3AvailableSpace::Definite(child_content_size.width),
            };

            // Re-layout the IFC with float awareness
            // This will pass floats as exclusion zones to text3 for line wrapping
            let ifc_result = layout_formatting_context(
                ctx,
                tree,
                text_cache,
                child_index,
                &ifc_constraints,
                float_cache,
            )?;

            // DON'T update used_size - the box keeps its full width!
            // Only the text layout inside changes to wrap around floats

            debug_info!(
                ctx,
                "[layout_bfc] IFC child {} re-layouted with float context (text will wrap, box \
                 stays full width)",
                child_index
            );

            // Merge positions from IFC output (inline-block children, etc.)
            for (idx, pos) in ifc_result.output.positions {
                output.positions.insert(idx, pos);
            }
        }

        output.positions.insert(child_index, final_pos);

        // Advance the pen past the child's content size
        // For FIRST child with escaped margin: the escaped margin was added to position,
        // so we need to add it to main_pen too for correct sibling positioning
        // For NON-FIRST children: escaped margins are internal to that child, don't affect our pen
        if is_first_in_flow_child && child_escaped_margin > 0.0 {
            main_pen += child_size.main(writing_mode) + child_escaped_margin;
            debug_info!(
                ctx,
                "[layout_bfc] Advanced main_pen by child_size={} + escaped={} = {} total",
                child_size.main(writing_mode),
                child_escaped_margin,
                main_pen
            );
        } else {
            main_pen += child_size.main(writing_mode);
        }
        has_content = true;

        // Update last margin for next sibling
        // CSS 2.2 § 8.3.1: The bottom margin of this box will collapse with the top margin
        // of the next sibling (if no clearance or blockers intervene)
        // CSS 2.2 § 9.5.2: If clearance was applied, margin collapsing is inhibited
        if clearance_applied {
            // Clearance inhibits collapse - next sibling starts fresh
            last_margin_bottom = 0.0;
        } else {
            last_margin_bottom = child_margin_bottom;
        }

        debug_info!(
            ctx,
            "[layout_bfc] Child {} positioned at final_pos={:?}, size={:?}, advanced main_pen to \
             {}, last_margin_bottom={}, clearance_applied={}",
            child_index,
            final_pos,
            child_size,
            main_pen,
            last_margin_bottom,
            clearance_applied
        );

        // Track the maximum cross-axis size to determine the BFC's overflow size.
        let child_cross_extent =
            child_cross_pos + child_size.cross(writing_mode) + child_margin.cross_end(writing_mode);
        max_cross_size = max_cross_size.max(child_cross_extent);
    }

    // Store the float context in cache for future layout passes
    // This happens after ALL children (floats and normal) have been positioned
    debug_info!(
        ctx,
        "[layout_bfc] Storing {} floats in cache for node {}",
        float_context.floats.len(),
        node_index
    );
    float_cache.insert(node_index, float_context.clone());

    // PHASE 3: Parent-Child Bottom Margin Escape
    let mut escaped_top_margin = None;
    let mut escaped_bottom_margin = None;

    // Handle top margin escape
    if top_margin_escaped {
        // First child's margin escaped through parent
        escaped_top_margin = Some(accumulated_top_margin);
        debug_info!(
            ctx,
            "[layout_bfc] Returning escaped top margin: accumulated={}, node={}",
            accumulated_top_margin,
            node_index
        );
    } else if !top_margin_resolved && accumulated_top_margin > 0.0 {
        // No content was positioned, all margins accumulated (empty blocks)
        escaped_top_margin = Some(accumulated_top_margin);
        debug_info!(
            ctx,
            "[layout_bfc] Escaping top margin (no content): accumulated={}, node={}",
            accumulated_top_margin,
            node_index
        );
    } else if !top_margin_resolved {
        // Unusual case: no content, zero margin
        escaped_top_margin = Some(accumulated_top_margin);
        debug_info!(
            ctx,
            "[layout_bfc] Escaping top margin (zero, no content): accumulated={}, node={}",
            accumulated_top_margin,
            node_index
        );
    } else {
        debug_info!(
            ctx,
            "[layout_bfc] NOT escaping top margin: top_margin_resolved={}, escaped={}, \
             accumulated={}, node={}",
            top_margin_resolved,
            top_margin_escaped,
            accumulated_top_margin,
            node_index
        );
    }

    // Handle bottom margin escape
    if let Some(last_idx) = last_child_index {
        let last_child = tree.get(last_idx).ok_or(LayoutError::InvalidTree)?;
        let last_has_bottom_blocker =
            has_margin_collapse_blocker(&last_child.box_props, writing_mode, false);

        debug_info!(
            ctx,
            "[layout_bfc] Bottom margin for node {}: parent_has_bottom_blocker={}, \
             last_has_bottom_blocker={}, last_margin_bottom={}, main_pen_before={}",
            node_index,
            parent_has_bottom_blocker,
            last_has_bottom_blocker,
            last_margin_bottom,
            main_pen
        );

        if !parent_has_bottom_blocker && !last_has_bottom_blocker && has_content {
            // Last child's bottom margin can escape
            let collapsed_bottom = collapse_margins(parent_margin_bottom, last_margin_bottom);
            escaped_bottom_margin = Some(collapsed_bottom);
            debug_info!(
                ctx,
                "[layout_bfc] Bottom margin ESCAPED for node {}: collapsed={}",
                node_index,
                collapsed_bottom
            );
            // Don't add last_margin_bottom to pen (it escaped)
        } else {
            // Can't escape: add to pen
            main_pen += last_margin_bottom;
            // NOTE: We do NOT add parent_margin_bottom to main_pen here!
            // parent_margin_bottom is added OUTSIDE the content-box (in the margin-box)
            // The content-box height should only include children's content and margins
            debug_info!(
                ctx,
                "[layout_bfc] Bottom margin BLOCKED for node {}: added last_margin_bottom={}, \
                 main_pen_after={}",
                node_index,
                last_margin_bottom,
                main_pen
            );
        }
    } else {
        // No children: just use parent's margins
        if !top_margin_resolved {
            main_pen += parent_margin_top;
        }
        main_pen += parent_margin_bottom;
    }

    // CRITICAL: If this is a root node (no parent), apply escaped margins directly
    // instead of propagating them upward (since there's no parent to receive them)
    let is_root_node = node.parent.is_none();
    if is_root_node {
        if let Some(top) = escaped_top_margin {
            // Adjust all child positions downward by the escaped top margin
            for (_, pos) in output.positions.iter_mut() {
                let current_main = pos.main(writing_mode);
                *pos = LogicalPosition::from_main_cross(
                    current_main + top,
                    pos.cross(writing_mode),
                    writing_mode,
                );
            }
            main_pen += top;
        }
        if let Some(bottom) = escaped_bottom_margin {
            main_pen += bottom;
        }
        // For root nodes, don't propagate margins further
        escaped_top_margin = None;
        escaped_bottom_margin = None;
    }

    // CSS 2.2 § 9.5: Floats don't contribute to container height with overflow:visible
    //
    // However, browsers DO expand containers to contain floats in specific cases:
    //
    // 1. If there's NO in-flow content (main_pen == 0), floats determine height
    // 2. If container establishes a BFC (overflow != visible)
    //
    // In this case, we have in-flow content (main_pen > 0) and overflow:visible,
    // so floats should NOT expand the container. Their margins can "bleed" beyond
    // the container boundaries into the parent.
    //
    // This matches Chrome/Firefox behavior where float margins escape through
    // the container's padding when there's existing in-flow content.

    // Content-box Height Calculation
    //
    // CSS 2.2 § 8.3.1: "The top border edge of the box is defined to coincide with
    // the top border edge of the [first] child" when margins collapse/escape.
    //
    // This means escaped margins do NOT contribute to the parent's content-box height.
    //
    // Calculation:
    //
    //   main_pen = total vertical space used by all children and margins
    //
    //   Components of main_pen:
    //
    //   1. Children's border-boxes (always included)
    //   2. Sibling collapsed margins (space BETWEEN children - part of content)
    //   3. First child's position (0 if margin escaped, margin_top if blocked)
    //
    //   What to subtract:
    //
    //   - total_escaped_top_margin: First child's margin that went to grandparent's space This
    //     margin is OUTSIDE our content-box, so we must subtract it.
    //
    //   What NOT to subtract:
    //
    //   - total_sibling_margins: These are the gaps BETWEEN children, which are
    //    legitimately part of our content area's layout space.
    //
    // Example with escaped margin:
    //   <div class="parent" padding=0>              <!-- Node 2 -->
    //     <div class="child1" margin=30></div>      <!-- Node 3, margin escapes -->
    //     <div class="child2" margin=40></div>      <!-- Node 5 -->
    //   </div>
    //
    //   Layout process:
    //
    //   - Node 3 positioned at main_pen=0 (margin escaped)
    //   - Node 3 size=140px → main_pen advances to 140
    //   - Sibling collapse: max(30 child1 bottom, 40 child2 top) = 40px
    //   - main_pen advances to 180
    //   - Node 5 size=130px → main_pen advances to 310
    //   - total_escaped_top_margin = 30
    //   - total_sibling_margins = 40 (tracked but NOT subtracted)
    //   - content_box_height = 310 - 30 = 280px ✓
    //
    // Previously, we calculated:
    //
    //   content_box_height = main_pen - total_escaped_top_margin - total_sibling_margins
    //
    // This incorrectly subtracted sibling margins, making parent too small.
    // Sibling margins are *between* boxes (part of layout), not *outside* boxes
    // (like escaped margins).

    let content_box_height = main_pen - total_escaped_top_margin;
    output.overflow_size =
        LogicalSize::from_main_cross(content_box_height, max_cross_size, writing_mode);

    debug_info!(
        ctx,
        "[layout_bfc] FINAL for node {}: main_pen={}, total_escaped_top={}, \
         total_sibling_margins={}, content_box_height={}",
        node_index,
        main_pen,
        total_escaped_top_margin,
        total_sibling_margins,
        content_box_height
    );

    // Baseline calculation would happen here in a full implementation.
    output.baseline = None;

    // Store escaped margins in the LayoutNode for use by parent
    if let Some(node_mut) = tree.get_mut(node_index) {
        node_mut.escaped_top_margin = escaped_top_margin;
        node_mut.escaped_bottom_margin = escaped_bottom_margin;
    }

    if let Some(node_mut) = tree.get_mut(node_index) {
        node_mut.baseline = output.baseline;
    }

    Ok(BfcLayoutResult {
        output,
        escaped_top_margin,
        escaped_bottom_margin,
    })
}

// Inline Formatting Context (CSS 2.2 § 9.4.2)

/// Lays out an Inline Formatting Context (IFC) by delegating to the `text3` engine.
///
/// This function acts as a bridge between the box-tree world of `solver3` and the
/// rich text layout world of `text3`. Its responsibilities are:
///
/// 1. **Collect Content**: Traverse the direct children of the IFC root and convert them into a
///    `Vec<InlineContent>`, the input format for `text3`. This involves:
///
///     - Recursively laying out `inline-block` children to determine their final size and baseline,
///       which are then passed to `text3` as opaque objects.
///     - Extracting raw text runs from inline text nodes.
///
/// 2. **Translate Constraints**: Convert the `LayoutConstraints` (available space, floats) from
///    `solver3` into the more detailed `UnifiedConstraints` that `text3` requires.
///
/// 3. **Invoke Text Layout**: Call the `text3` cache's `layout_flow` method to perform the complex
///    tasks of BIDI analysis, shaping, line breaking, justification, and vertical alignment.
///
/// 4. **Integrate Results**: Process the `UnifiedLayout` returned by `text3`:
///
///     - Store the rich layout result on the IFC root `LayoutNode` for the display list generation
///       pass.
///     - Update the `positions` map for all `inline-block` children based on the positions
///       calculated by `text3`.
///     - Extract the final overflow size and baseline for the IFC root itself
fn layout_ifc<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    tree: &mut LayoutTree,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
    let float_count = constraints
        .bfc_state
        .as_ref()
        .map(|s| s.floats.floats.len())
        .unwrap_or(0);
    debug_info!(
        ctx,
        "[layout_ifc] ENTRY: node_index={}, has_bfc_state={}, float_count={}",
        node_index,
        constraints.bfc_state.is_some(),
        float_count
    );
    debug_ifc_layout!(ctx, "CALLED for node_index={}", node_index);

    // For anonymous boxes, we need to find the DOM ID from a parent or child
    // CSS 2.2 § 9.2.1.1: Anonymous boxes inherit properties from their enclosing box
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
    let ifc_root_dom_id = match node.dom_node_id {
        Some(id) => id,
        None => {
            // Anonymous box - get DOM ID from parent or first child with DOM ID
            let parent_dom_id = node
                .parent
                .and_then(|p| tree.get(p))
                .and_then(|n| n.dom_node_id);

            if let Some(id) = parent_dom_id {
                id
            } else {
                // Try to find DOM ID from first child
                node.children
                    .iter()
                    .filter_map(|&child_idx| tree.get(child_idx))
                    .filter_map(|n| n.dom_node_id)
                    .next()
                    .ok_or(LayoutError::InvalidTree)?
            }
        }
    };

    debug_ifc_layout!(ctx, "ifc_root_dom_id={:?}", ifc_root_dom_id);

    // Phase 1: Collect and measure all inline-level children.
    let (inline_content, child_map) =
        collect_and_measure_inline_content(ctx, text_cache, tree, node_index, constraints)?;

    debug_info!(
        ctx,
        "[layout_ifc] Collected {} inline content items for node {}",
        inline_content.len(),
        node_index
    );
    for (i, item) in inline_content.iter().enumerate() {
        match item {
            InlineContent::Text(run) => debug_info!(ctx, "  [{}] Text: '{}'", i, run.text),
            InlineContent::Marker {
                run,
                position_outside,
            } => debug_info!(
                ctx,
                "  [{}] Marker: '{}' (outside={})",
                i,
                run.text,
                position_outside
            ),
            InlineContent::Shape(_) => debug_info!(ctx, "  [{}] Shape", i),
            InlineContent::Image(_) => debug_info!(ctx, "  [{}] Image", i),
            _ => debug_info!(ctx, "  [{}] Other", i),
        }
    }

    debug_ifc_layout!(
        ctx,
        "Collected {} inline content items",
        inline_content.len()
    );

    if inline_content.is_empty() {
        debug_warning!(ctx, "inline_content is empty, returning default output!");
        return Ok(LayoutOutput::default());
    }

    // Phase 2: Translate constraints and define a single layout fragment for text3.
    let text3_constraints =
        translate_to_text3_constraints(ctx, constraints, ctx.styled_dom, ifc_root_dom_id);

    // Clone constraints for caching (before they're moved into fragments)
    let cached_constraints = text3_constraints.clone();

    debug_info!(
        ctx,
        "[layout_ifc] CALLING text_cache.layout_flow for node {} with {} exclusions",
        node_index,
        text3_constraints.shape_exclusions.len()
    );

    let fragments = vec![LayoutFragment {
        id: "main".to_string(),
        constraints: text3_constraints,
    }];

    // Phase 3: Invoke the text layout engine.
    // Get pre-loaded fonts from font manager (fonts should be loaded before layout)
    let loaded_fonts = ctx.font_manager.get_loaded_fonts();
    let text_layout_result = match text_cache.layout_flow(
        &inline_content,
        &[],
        &fragments,
        &ctx.font_manager.font_chain_cache,
        &ctx.font_manager.fc_cache,
        &loaded_fonts,
        ctx.debug_messages,
    ) {
        Ok(result) => result,
        Err(e) => {
            // Font errors should not stop layout of other elements.
            // Log the error and return a zero-sized layout.
            debug_warning!(ctx, "Text layout failed: {:?}", e);
            debug_warning!(
                ctx,
                "Continuing with zero-sized layout for node {}",
                node_index
            );

            let mut output = LayoutOutput::default();
            output.overflow_size = LogicalSize::new(0.0, 0.0);
            return Ok(output);
        }
    };

    // Phase 4: Integrate results back into the solver3 layout tree.
    let mut output = LayoutOutput::default();
    let node = tree.get_mut(node_index).ok_or(LayoutError::InvalidTree)?;

    debug_ifc_layout!(
        ctx,
        "text_layout_result has {} fragment_layouts",
        text_layout_result.fragment_layouts.len()
    );

    if let Some(main_frag) = text_layout_result.fragment_layouts.get("main") {
        let frag_bounds = main_frag.bounds();
        debug_ifc_layout!(
            ctx,
            "Found 'main' fragment with {} items, bounds={}x{}",
            main_frag.items.len(),
            frag_bounds.width,
            frag_bounds.height
        );
        debug_ifc_layout!(ctx, "Storing inline_layout_result on node {}", node_index);

        // Determine if we should store this layout result using the new
        // CachedInlineLayout system. The key insight is that inline layouts
        // depend on available width:
        //
        // - Min-content measurement uses width ≈ 0 (maximum line wrapping)
        // - Max-content measurement uses width = ∞ (no line wrapping)
        // - Final layout uses the actual column/container width
        //
        // We must track which constraint type was used, otherwise a min-content
        // measurement would incorrectly be reused for final rendering.
        let has_floats = constraints
            .bfc_state
            .as_ref()
            .map(|s| !s.floats.floats.is_empty())
            .unwrap_or(false);
        let current_width_type = constraints.available_width_type;

        let should_store = match &node.inline_layout_result {
            None => {
                // No cached result - always store
                debug_info!(
                    ctx,
                    "[layout_ifc] Storing NEW inline_layout_result for node {} (width_type={:?}, \
                     has_floats={})",
                    node_index,
                    current_width_type,
                    has_floats
                );
                true
            }
            Some(cached) => {
                // Check if the new result should replace the cached one
                if cached.should_replace_with(current_width_type, has_floats) {
                    debug_info!(
                        ctx,
                        "[layout_ifc] REPLACING inline_layout_result for node {} (old: \
                         width={:?}, floats={}) with (new: width={:?}, floats={})",
                        node_index,
                        cached.available_width,
                        cached.has_floats,
                        current_width_type,
                        has_floats
                    );
                    true
                } else {
                    debug_info!(
                        ctx,
                        "[layout_ifc] KEEPING cached inline_layout_result for node {} (cached: \
                         width={:?}, floats={}, new: width={:?}, floats={})",
                        node_index,
                        cached.available_width,
                        cached.has_floats,
                        current_width_type,
                        has_floats
                    );
                    false
                }
            }
        };

        if should_store {
            node.inline_layout_result = Some(CachedInlineLayout::new_with_constraints(
                main_frag.clone(),
                current_width_type,
                has_floats,
                cached_constraints,
            ));
        }

        // Extract the overall size and baseline for the IFC root.
        output.overflow_size = LogicalSize::new(frag_bounds.width, frag_bounds.height);
        output.baseline = main_frag.last_baseline();
        node.baseline = output.baseline;

        // Position all the inline-block children based on text3's calculations.
        for positioned_item in &main_frag.items {
            if let ShapedItem::Object { source, content, .. } = &positioned_item.item {
                if let Some(&child_node_index) = child_map.get(source) {
                    let new_relative_pos = LogicalPosition {
                        x: positioned_item.position.x,
                        y: positioned_item.position.y,
                    };
                    output.positions.insert(child_node_index, new_relative_pos);
                }
            }
        }
    }

    Ok(output)
}

fn translate_taffy_size(size: LogicalSize) -> TaffySize<Option<f32>> {
    TaffySize {
        width: Some(size.width),
        height: Some(size.height),
    }
}

/// Helper: Convert StyleFontStyle to text3::cache::FontStyle
pub(crate) fn convert_font_style(style: StyleFontStyle) -> crate::font_traits::FontStyle {
    match style {
        StyleFontStyle::Normal => crate::font_traits::FontStyle::Normal,
        StyleFontStyle::Italic => crate::font_traits::FontStyle::Italic,
        StyleFontStyle::Oblique => crate::font_traits::FontStyle::Oblique,
    }
}

/// Helper: Convert StyleFontWeight to FcWeight
pub(crate) fn convert_font_weight(weight: StyleFontWeight) -> FcWeight {
    match weight {
        StyleFontWeight::W100 => FcWeight::Thin,
        StyleFontWeight::W200 => FcWeight::ExtraLight,
        StyleFontWeight::W300 | StyleFontWeight::Lighter => FcWeight::Light,
        StyleFontWeight::Normal => FcWeight::Normal,
        StyleFontWeight::W500 => FcWeight::Medium,
        StyleFontWeight::W600 => FcWeight::SemiBold,
        StyleFontWeight::Bold => FcWeight::Bold,
        StyleFontWeight::W800 => FcWeight::ExtraBold,
        StyleFontWeight::W900 | StyleFontWeight::Bolder => FcWeight::Black,
    }
}

/// Resolves a CSS size metric to pixels.
#[inline]
fn resolve_size_metric(metric: SizeMetric, value: f32, containing_block_size: f32) -> f32 {
    match metric {
        SizeMetric::Px => value,
        SizeMetric::Pt => value * PT_TO_PX,
        SizeMetric::Percent => value / 100.0 * containing_block_size,
        SizeMetric::Em | SizeMetric::Rem => value * DEFAULT_FONT_SIZE,
        _ => value, // Fallback
    }
}

pub fn translate_taffy_size_back(size: TaffySize<f32>) -> LogicalSize {
    LogicalSize {
        width: size.width,
        height: size.height,
    }
}

pub fn translate_taffy_point_back(point: taffy::Point<f32>) -> LogicalPosition {
    LogicalPosition {
        x: point.x,
        y: point.y,
    }
}

/// Checks if a node establishes a new Block Formatting Context (BFC).
///
/// Per CSS 2.2 § 9.4.1, a BFC is established by:
/// - Floats (elements with float other than 'none')
/// - Absolutely positioned elements (position: absolute or fixed)
/// - Block containers that are not block boxes (e.g., inline-blocks, table-cells)
/// - Block boxes with 'overflow' other than 'visible' and 'clip'
/// - Elements with 'display: flow-root'
/// - Table cells, table captions, and inline-blocks
///
/// Normal flow block-level boxes do NOT establish a new BFC.
///
/// This is critical for correct float interaction: normal blocks should overlap floats
/// (not shrink around them), while their inline content wraps around floats.
fn establishes_new_bfc<T: ParsedFontTrait>(ctx: &LayoutContext<'_, T>, node: &LayoutNode) -> bool {
    let Some(dom_id) = node.dom_node_id else {
        return false;
    };

    let node_state = &ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

    // 1. Floats establish BFC
    let float_val = get_float(ctx.styled_dom, dom_id, node_state);
    if matches!(
        float_val,
        MultiValue::Exact(LayoutFloat::Left | LayoutFloat::Right)
    ) {
        return true;
    }

    // 2. Absolutely positioned elements establish BFC
    let position = crate::solver3::positioning::get_position_type(ctx.styled_dom, Some(dom_id));
    if matches!(position, LayoutPosition::Absolute | LayoutPosition::Fixed) {
        return true;
    }

    // 3. Inline-blocks, table-cells, table-captions establish BFC
    let display = get_display_property(ctx.styled_dom, Some(dom_id));
    if matches!(
        display,
        MultiValue::Exact(
            LayoutDisplay::InlineBlock | LayoutDisplay::TableCell | LayoutDisplay::TableCaption
        )
    ) {
        return true;
    }

    // 4. display: flow-root establishes BFC
    if matches!(display, MultiValue::Exact(LayoutDisplay::FlowRoot)) {
        return true;
    }

    // 5. Block boxes with overflow other than 'visible' or 'clip' establish BFC
    // Note: 'clip' does NOT establish BFC per CSS Overflow Module Level 3
    let overflow_x = get_overflow_x(ctx.styled_dom, dom_id, node_state);
    let overflow_y = get_overflow_y(ctx.styled_dom, dom_id, node_state);

    let creates_bfc_via_overflow = |ov: &MultiValue<LayoutOverflow>| {
        matches!(
            ov,
            &MultiValue::Exact(
                LayoutOverflow::Hidden | LayoutOverflow::Scroll | LayoutOverflow::Auto
            )
        )
    };

    if creates_bfc_via_overflow(&overflow_x) || creates_bfc_via_overflow(&overflow_y) {
        return true;
    }

    // 6. Table, Flex, and Grid containers establish BFC (via FormattingContext)
    if matches!(
        node.formatting_context,
        FormattingContext::Table | FormattingContext::Flex | FormattingContext::Grid
    ) {
        return true;
    }

    // Normal flow block boxes do NOT establish BFC
    false
}

/// Translates solver3 layout constraints into the text3 engine's unified constraints.
fn translate_to_text3_constraints<'a, T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    constraints: &'a LayoutConstraints<'a>,
    styled_dom: &StyledDom,
    dom_id: NodeId,
) -> UnifiedConstraints {
    // Convert floats into exclusion zones for text3 to flow around.
    let mut shape_exclusions = if let Some(ref bfc_state) = constraints.bfc_state {
        debug_info!(
            ctx,
            "[translate_to_text3] dom_id={:?}, converting {} floats to exclusions",
            dom_id,
            bfc_state.floats.floats.len()
        );
        bfc_state
            .floats
            .floats
            .iter()
            .enumerate()
            .map(|(i, float_box)| {
                let rect = crate::text3::cache::Rect {
                    x: float_box.rect.origin.x,
                    y: float_box.rect.origin.y,
                    width: float_box.rect.size.width,
                    height: float_box.rect.size.height,
                };
                debug_info!(
                    ctx,
                    "[translate_to_text3]   Exclusion #{}: {:?} at ({}, {}) size {}x{}",
                    i,
                    float_box.kind,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height
                );
                ShapeBoundary::Rectangle(rect)
            })
            .collect()
    } else {
        debug_info!(
            ctx,
            "[translate_to_text3] dom_id={:?}, NO bfc_state - no float exclusions",
            dom_id
        );
        Vec::new()
    };

    debug_info!(
        ctx,
        "[translate_to_text3] dom_id={:?}, available_size={}x{}, shape_exclusions.len()={}",
        dom_id,
        constraints.available_size.width,
        constraints.available_size.height,
        shape_exclusions.len()
    );

    // Map text-align and justify-content from CSS to text3 enums.
    let id = dom_id;
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;

    // Read CSS Shapes properties
    // For reference box, use the element's CSS height if available, otherwise available_size
    // This is important because available_size.height might be infinite during auto height
    // calculation
    let ref_box_height = if constraints.available_size.height.is_finite() {
        constraints.available_size.height
    } else {
        // Try to get explicit CSS height
        // NOTE: If height is infinite, we can't properly resolve % heights
        // This is a limitation - shape-inside with % heights requires finite containing block
        styled_dom
            .css_property_cache
            .ptr
            .get_height(node_data, &id, node_state)
            .and_then(|v| v.get_property())
            .and_then(|h| match h {
                LayoutHeight::Px(v) => {
                    // Only accept absolute units (px, pt, in, cm, mm) - no %, em, rem
                    // since we can't resolve relative units without proper context
                    match v.metric {
                        SizeMetric::Px => Some(v.number.get()),
                        SizeMetric::Pt => Some(v.number.get() * PT_TO_PX),
                        SizeMetric::In => Some(v.number.get() * 96.0),
                        SizeMetric::Cm => Some(v.number.get() * 96.0 / 2.54),
                        SizeMetric::Mm => Some(v.number.get() * 96.0 / 25.4),
                        _ => None, // Ignore %, em, rem
                    }
                }
                _ => None,
            })
            .unwrap_or(constraints.available_size.width) // Fallback: use width as height (square)
    };

    let reference_box = crate::text3::cache::Rect {
        x: 0.0,
        y: 0.0,
        width: constraints.available_size.width,
        height: ref_box_height,
    };

    // shape-inside: Text flows within the shape boundary
    debug_info!(ctx, "Checking shape-inside for node {:?}", id);
    debug_info!(
        ctx,
        "Reference box: {:?} (available_size height was: {})",
        reference_box,
        constraints.available_size.height
    );

    let shape_boundaries = styled_dom
        .css_property_cache
        .ptr
        .get_shape_inside(node_data, &id, node_state)
        .and_then(|v| {
            debug_info!(ctx, "Got shape-inside value: {:?}", v);
            v.get_property()
        })
        .and_then(|shape_inside| {
            debug_info!(ctx, "shape-inside property: {:?}", shape_inside);
            if let ShapeInside::Shape(css_shape) = shape_inside {
                debug_info!(
                    ctx,
                    "Converting CSS shape to ShapeBoundary: {:?}",
                    css_shape
                );
                let boundary =
                    ShapeBoundary::from_css_shape(css_shape, reference_box, ctx.debug_messages);
                debug_info!(ctx, "Created ShapeBoundary: {:?}", boundary);
                Some(vec![boundary])
            } else {
                debug_info!(ctx, "shape-inside is None");
                None
            }
        })
        .unwrap_or_default();

    debug_info!(
        ctx,
        "Final shape_boundaries count: {}",
        shape_boundaries.len()
    );

    // shape-outside: Text wraps around the shape (adds to exclusions)
    debug_info!(ctx, "Checking shape-outside for node {:?}", id);
    if let Some(shape_outside_value) = styled_dom
        .css_property_cache
        .ptr
        .get_shape_outside(node_data, &id, node_state)
    {
        debug_info!(ctx, "Got shape-outside value: {:?}", shape_outside_value);
        if let Some(shape_outside) = shape_outside_value.get_property() {
            debug_info!(ctx, "shape-outside property: {:?}", shape_outside);
            if let ShapeOutside::Shape(css_shape) = shape_outside {
                debug_info!(
                    ctx,
                    "Converting CSS shape-outside to ShapeBoundary: {:?}",
                    css_shape
                );
                let boundary =
                    ShapeBoundary::from_css_shape(css_shape, reference_box, ctx.debug_messages);
                debug_info!(ctx, "Created ShapeBoundary (exclusion): {:?}", boundary);
                shape_exclusions.push(boundary);
            }
        }
    } else {
        debug_info!(ctx, "No shape-outside value found");
    }

    // TODO: clip-path will be used for rendering clipping (not text layout)

    let writing_mode = styled_dom
        .css_property_cache
        .ptr
        .get_writing_mode(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let text_align = styled_dom
        .css_property_cache
        .ptr
        .get_text_align(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let text_justify = styled_dom
        .css_property_cache
        .ptr
        .get_text_justify(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    // Get font-size for resolving line-height
    // Use helper function which checks dependency chain first
    let font_size = get_element_font_size(styled_dom, id, node_state);

    let line_height_value = styled_dom
        .css_property_cache
        .ptr
        .get_line_height(node_data, &id, node_state)
        .and_then(|s| s.get_property().cloned())
        .unwrap_or_default();

    let hyphenation = styled_dom
        .css_property_cache
        .ptr
        .get_hyphens(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let overflow_behaviour = styled_dom
        .css_property_cache
        .ptr
        .get_overflow_x(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    // Get vertical-align from CSS property cache (defaults to Baseline per CSS spec)
    let vertical_align = styled_dom
        .css_property_cache
        .ptr
        .get_vertical_align(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let vertical_align = match vertical_align {
        StyleVerticalAlign::Baseline => text3::cache::VerticalAlign::Baseline,
        StyleVerticalAlign::Top => text3::cache::VerticalAlign::Top,
        StyleVerticalAlign::Middle => text3::cache::VerticalAlign::Middle,
        StyleVerticalAlign::Bottom => text3::cache::VerticalAlign::Bottom,
        StyleVerticalAlign::Sub => text3::cache::VerticalAlign::Sub,
        StyleVerticalAlign::Superscript => text3::cache::VerticalAlign::Super,
        StyleVerticalAlign::TextTop => text3::cache::VerticalAlign::TextTop,
        StyleVerticalAlign::TextBottom => text3::cache::VerticalAlign::TextBottom,
    };
    let text_orientation = text3::cache::TextOrientation::default();

    // Get the direction property from the CSS cache (defaults to LTR if not set)
    let direction = styled_dom
        .css_property_cache
        .ptr
        .get_direction(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .map(|d| match d {
            StyleDirection::Ltr => text3::cache::BidiDirection::Ltr,
            StyleDirection::Rtl => text3::cache::BidiDirection::Rtl,
        });

    debug_info!(
        ctx,
        "dom_id={:?}, available_size={}x{}, setting available_width={}",
        dom_id,
        constraints.available_size.width,
        constraints.available_size.height,
        constraints.available_size.width
    );

    // Get text-indent
    let text_indent = styled_dom
        .css_property_cache
        .ptr
        .get_text_indent(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|ti| {
            let context = ResolutionContext {
                element_font_size: get_element_font_size(styled_dom, id, node_state),
                parent_font_size: get_parent_font_size(styled_dom, id, node_state),
                root_font_size: get_root_font_size(styled_dom, node_state),
                containing_block_size: PhysicalSize::new(constraints.available_size.width, 0.0),
                element_size: None,
                viewport_size: PhysicalSize::new(0.0, 0.0),
            };
            ti.inner
                .resolve_with_context(&context, PropertyContext::Other)
        })
        .unwrap_or(0.0);

    // Get column-count for multi-column layout (default: 1 = no columns)
    let columns = styled_dom
        .css_property_cache
        .ptr
        .get_column_count(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|cc| match cc {
            ColumnCount::Integer(n) => *n,
            ColumnCount::Auto => 1,
        })
        .unwrap_or(1);

    // Get column-gap for multi-column layout (default: normal = 1em)
    let column_gap = styled_dom
        .css_property_cache
        .ptr
        .get_column_gap(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|cg| {
            let context = ResolutionContext {
                element_font_size: get_element_font_size(styled_dom, id, node_state),
                parent_font_size: get_parent_font_size(styled_dom, id, node_state),
                root_font_size: get_root_font_size(styled_dom, node_state),
                containing_block_size: PhysicalSize::new(0.0, 0.0),
                element_size: None,
                viewport_size: PhysicalSize::new(0.0, 0.0),
            };
            cg.inner
                .resolve_with_context(&context, PropertyContext::Other)
        })
        .unwrap_or_else(|| {
            // Default: 1em
            get_element_font_size(styled_dom, id, node_state)
        });

    // Map white-space CSS property to TextWrap
    let text_wrap = styled_dom
        .css_property_cache
        .ptr
        .get_white_space(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|ws|

... [FILE TRUNCATED - original size: 244930 bytes] ...
```

### tests/e2e/contenteditable.c

```rust
/**
 * ContentEditable E2E Test with Large Font
 * 
 * Tests contenteditable text input, cursor movement, selection, and scroll-auto-follow:
 * 1. Single-line contenteditable input
 * 2. Multi-line contenteditable textarea
 * 3. Cursor movement (arrow keys)
 * 4. Text selection (Shift+Arrow, Ctrl+A)
 * 5. Text input (typing characters)
 * 6. Scroll-into-view when cursor moves off-screen
 * 7. Backspace/Delete key handling
 * 
 * Uses LARGE FONT (48px) for easy visual debugging
 * 
 * Compile:
 *   cd tests/e2e && cc contenteditable.c -I../../examples/c -L../../target/release/ -lazul -o contenteditable_test -Wl,-rpath,../../target/release
 * 
 * Run with: AZUL_DEBUG=8765 ./contenteditable_test
 * Test with: ./test_contenteditable.sh
 */

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))
#define MAX_TEXT_LEN 4096

typedef struct {
    char single_line_text[256];
    char multi_line_text[MAX_TEXT_LEN];
    int cursor_line;
    int cursor_column;
    int selection_start;
    int selection_end;
    int key_press_count;
    int text_change_count;
} ContentEditableData;

void ContentEditableData_destructor(void* data) {}

AZ_REFLECT(ContentEditableData, ContentEditableData_destructor)

// ============================================================================
// Callbacks
// ============================================================================

// Callback for tracking text input events
AzUpdate on_text_input(AzRefAny data, AzCallbackInfo info) {
    ContentEditableDataRefMut ref = ContentEditableDataRefMut_create(&data);
    if (!ContentEditableData_downcastMut(&data, &ref)) {
        return AzUpdate_DoNothing;
    }
    
    // Get the text changeset from the callback info
    AzOptionPendingTextEdit changeset = AzCallbackInfo_getTextChangeset(&info);
    
    if (changeset.Some.tag == AzOptionPendingTextEdit_Tag_Some) {
        AzPendingTextEdit* edit = &changeset.Some.payload;
        
        // Print the changeset for debugging
        printf("[TextInput] Changeset received:\n");
        printf("  inserted_text: '%.*s'\n", 
               (int)edit->inserted_text.vec.len, 
               (const char*)edit->inserted_text.vec.ptr);
        printf("  old_text: '%.*s' (len=%zu)\n", 
               (int)(edit->old_text.vec.len > 50 ? 50 : edit->old_text.vec.len),
               (const char*)edit->old_text.vec.ptr,
               edit->old_text.vec.len);
        
        // Append the inserted text to our data model
        // For single-line, we just append to the existing text
        size_t current_len = strlen(ref.ptr->single_line_text);
        size_t insert_len = edit->inserted_text.vec.len;
        
        if (current_len + insert_len < sizeof(ref.ptr->single_line_text) - 1) {
            memcpy(ref.ptr->single_line_text + current_len, 
                   edit->inserted_text.vec.ptr, 
                   insert_len);
            ref.ptr->single_line_text[current_len + insert_len] = '\0';
            printf("  Updated single_line_text: '%s'\n", ref.ptr->single_line_text);
        }
        
        ref.ptr->text_change_count++;
    } else {
        printf("[TextInput] No changeset available\n");
    }
    
    ContentEditableDataRefMut_delete(&ref);
    
    // Return DoNothing - the text input system handles the visual update internally
    // RefreshDom would override the internal edit with the old data model state
    return AzUpdate_DoNothing;
}

// Callback for key press events
AzUpdate on_key_down(AzRefAny data, AzCallbackInfo info) {
    ContentEditableDataRefMut ref = ContentEditableDataRefMut_create(&data);
    if (!ContentEditableData_downcastMut(&data, &ref)) {
        return AzUpdate_DoNothing;
    }
    
    ref.ptr->key_press_count++;
    ContentEditableDataRefMut_delete(&ref);
    return AzUpdate_RefreshDom;
}

// ============================================================================
// CSS Styling (Large Font for Debugging)
// ============================================================================

const char* CSS_STYLE = 
    "body { \n"
    "    display: flex; \n"
    "    flex-direction: column; \n"
    "    padding: 20px; \n"
    "    background-color: #1e1e1e; \n"
    "    font-family: 'Cascadia Code', 'Consolas', monospace; \n"
    "}\n"
    "\n"
    ".label {\n"
    "    font-size: 32px;\n"
    "    color: #cccccc;\n"
    "    margin-bottom: 10px;\n"
    "    margin-top: 20px;\n"
    "}\n"
    "\n"
    ".single-line-input {\n"
    "    font-size: 48px;\n"
    "    padding: 20px;\n"
    "    background-color: #2d2d2d;\n"
    "    color: #ffffff;\n"
    "    border: 3px solid #555555;\n"
    "    min-height: 80px;\n"
    "    cursor: text;\n"
    "}\n"
    "\n"
    ".single-line-input:focus {\n"
    "    border-color: #0078d4;\n"
    "    outline: none;\n"
    "}\n"
    "\n"
    ".multi-line-textarea {\n"
    "    font-size: 48px;\n"
    "    padding: 20px;\n"
    "    background-color: #2d2d2d;\n"
    "    color: #ffffff;\n"
    "    border: 3px solid #555555;\n"
    "    min-height: 300px;\n"
    "    max-height: 400px;\n"
    "    overflow-y: scroll;\n"
    "    cursor: text;\n"
    "    white-space: pre-wrap;\n"
    "    line-height: 1.4;\n"
    "}\n"
    "\n"
    ".multi-line-textarea:focus {\n"
    "    border-color: #0078d4;\n"
    "    outline: none;\n"
    "}\n"
    "\n"
    ".status-bar {\n"
    "    font-size: 24px;\n"
    "    color: #888888;\n"
    "    margin-top: 20px;\n"
    "    padding: 10px;\n"
    "    background-color: #252525;\n"
    "}\n"
    "\n"
    "/* Cursor styling - use caret-color for text cursor */\n"
    ".single-line-input, .multi-line-textarea {\n"
    "    caret-color: #ffffff;\n"
    "}\n"
    "\n"
    "/* Selection styling */\n"
    "::selection {\n"
    "    background-color: #264f78;\n"
    "}\n";

// ============================================================================
// DOM Layout
// ============================================================================

AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    ContentEditableDataRef ref = ContentEditableDataRef_create(&data);
    if (!ContentEditableData_downcastRef(&data, &ref)) {
        return AzStyledDom_default();
    }
    
    // Build DOM
    AzDom root = AzDom_createBody();
    
    // Label 1: Single Line Input
    AzDom label1 = AzDom_createText(AZ_STR("Single Line Input (48px font):"));
    AzDom_addClass(&label1, AZ_STR("label"));
    AzDom_addChild(&root, label1);
    
    // Single-line contenteditable input
    // Create a div with text child, set contenteditable on the div
    AzDom single_input = AzDom_createDiv();
    AzDom_addClass(&single_input, AZ_STR("single-line-input"));
    AzDom_setContenteditable(&single_input, true);
    AzTabIndex tab_auto = { .Auto = { .tag = AzTabIndex_Tag_Auto } };
    AzDom_setTabIndex(&single_input, tab_auto);
    
    // Add text as child
    AzDom single_text = AzDom_createText(AZ_STR(ref.ptr->single_line_text));
    AzDom_addChild(&single_input, single_text);
    
    // Add text input callback - use Focus filter for text input
    AzEventFilter text_filter = AzEventFilter_focus(AzFocusEventFilter_TextInput);
    AzDom_addCallback(&single_input, text_filter, AzRefAny_clone(&data), on_text_input);
    
    AzDom_addChild(&root, single_input);
    
    // Label 2: Multi Line Text Area
    AzDom label2 = AzDom_createText(AZ_STR("Multi Line Text Area (scroll test):"));
    AzDom_addClass(&label2, AZ_STR("label"));
    AzDom_addChild(&root, label2);
    
    // Multi-line contenteditable textarea
    // Create a div with text child, set contenteditable on the div
    AzDom multi_input = AzDom_createDiv();
    AzDom_addClass(&multi_input, AZ_STR("multi-line-textarea"));
    AzDom_setContenteditable(&multi_input, true);
    AzDom_setTabIndex(&multi_input, tab_auto);
    
    // Add text as child
    AzDom multi_text = AzDom_createText(AZ_STR(ref.ptr->multi_line_text));
    AzDom_addChild(&multi_input, multi_text);
    
    // Add callbacks
    AzDom_addCallback(&multi_input, text_filter, AzRefAny_clone(&data), on_text_input);
    
    AzEventFilter key_filter = AzEventFilter_focus(AzFocusEventFilter_VirtualKeyDown);
    AzDom_addCallback(&multi_input, key_filter, AzRefAny_clone(&data), on_key_down);
    
    AzDom_addChild(&root, multi_input);
    
    // Status bar
    char status[256];
    snprintf(status, sizeof(status), 
             "Cursor: Line %d, Col %d | Selection: %d-%d | Keys: %d | Changes: %d",
             ref.ptr->cursor_line, ref.ptr->cursor_column,
             ref.ptr->selection_start, ref.ptr->selection_end,
             ref.ptr->key_press_count, ref.ptr->text_change_count);
    
    AzDom status_bar = AzDom_createText(AZ_STR(status));
    AzDom_addClass(&status_bar, AZ_STR("status-bar"));
    AzDom_addChild(&root, status_bar);
    
    ContentEditableDataRef_delete(&ref);
    
    // Parse and apply CSS
    AzCss css = AzCss_fromString(AZ_STR(CSS_STYLE));
    return AzDom_style(&root, css);
}

// ============================================================================
// Main
// ============================================================================

int main(int argc, char** argv) {
    printf("ContentEditable E2E Test\n");
    printf("========================\n");
    printf("Features tested:\n");
    printf("  - Large font (48px) for easy visual debugging\n");
    printf("  - Single-line contenteditable input\n");
    printf("  - Multi-line contenteditable textarea with scroll\n");
    printf("  - Tab navigation between inputs\n");
    printf("  - Text input, cursor movement, selection\n");
    printf("\n");
    printf("Debug API: AZUL_DEBUG=8765\n");
    printf("Test commands:\n");
    printf("  curl -X POST http://localhost:8765/ -d '{\"op\": \"get_state\"}'\n");
    printf("  curl -X POST http://localhost:8765/ -d '{\"op\": \"key_down\", \"key\": \"Tab\"}'\n");
    printf("  curl -X POST http://localhost:8765/ -d '{\"op\": \"text_input\", \"text\": \"Hello\"}'\n");
    printf("\n");
    
    // Initialize app data
    ContentEditableData initial = {
        .single_line_text = "Hello World - Click here and type!",
        .multi_line_text = 
           "Line 1: This is a multi-line text area.\n"
           "Line 2: Use arrow keys to move cursor.\n"
           "Line 3: Use Shift+Arrow to select text.\n"
           "Line 4: Use Ctrl+A to select all.\n"
           "Line 5: Type to insert text at cursor.\n"
           "Line 6: Backspace/Delete to remove text.\n"
           "Line 7: This tests scroll-into-view.\n"
           "Line 8: When cursor goes off-screen...\n"
           "Line 9: The view should scroll automatically.\n"
           "Line 10: End of test content.",
        .cursor_line = 1,
        .cursor_column = 0,
        .selection_start = 0,
        .selection_end = 0,
        .key_press_count = 0,
        .text_change_count = 0
    };
    
    AzRefAny data = ContentEditableData_upcast(initial);
    
    // Create window
    AzWindowCreateOptions win_opts = AzWindowCreateOptions_create(layout);
    win_opts.window_state.title = AZ_STR("ContentEditable Test - 48px Font");
    win_opts.window_state.size.dimensions.width = 1200.0;
    win_opts.window_state.size.dimensions.height = 800.0;
    
    // Create app
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);
    AzApp_run(&app, win_opts);
    AzApp_delete(&app);
    
    return 0;
}

```

## Bug Analysis Document

# ContentEditable Bugs Analysis - January 28, 2026

## Executive Summary

After implementing the V3 Text Input Plan with `CallbackChange::CreateTextInput` and removing the `PENDING_DEBUG_TEXT_INPUT` hack, the contenteditable test reveals multiple interconnected bugs. The debug output shows the text input flow is partially working (text does update on screen), but there are serious issues with cursor positioning, duplicate input, layout, and event handling.

---

## Bug 1: Cursor Not Appearing on Click

### Symptoms
- Clicking on the text input focuses it (blue outline appears)
- But NO cursor appears (neither at click position nor at end of text)
- Debug output shows:
  ```
  [DEBUG] process_mouse_click_for_selection: position=(1090.9,132.6), time_ms=0
  [DEBUG] HoverManager has hit test with 1 doms
  [DEBUG] Setting selection on dom_id=DomId { inner: 0 }, node_id=NodeId(2)
  ```

### Expected Behavior
- Cursor should appear at the clicked position within the text
- Cursor blink timer should start

### Likely Causes
1. `hit_test_text_at_point()` may not be correctly mapping click position to cursor position
2. `CursorManager.set_cursor_with_time()` may not be called after focus
3. Cursor blink timer not starting on focus
4. `finalize_pending_focus_changes()` not being called or not working

### Files to Investigate
- `layout/src/managers/cursor.rs` - CursorManager
- `layout/src/managers/focus_cursor.rs` - FocusManager  
- `layout/src/text3/selection.rs` - hit_test_text_at_point
- `dll/src/desktop/shell2/common/event_v2.rs` - Mouse click handling

---

## Bug 2: Double Input (Pressing 'j' inserts 'jj')

### Symptoms
- Pressing 'j' once inserts 'jj' (two characters)
- Debug output shows `get_pending_changeset` being called multiple times for single input
- From debugoutput.txt:
  ```
  [record_text_input] Called with text: 'j'
  ...
  Updated single_line_text: 'Hello World - Click here and type!j'
  ...
  Updated single_line_text: 'Hello World - Click here and type!jj'
  ```

### Expected Behavior
- Single keypress should insert exactly one character

### Likely Causes
1. `process_text_input()` being called twice
2. Timer callback AND event processing both triggering text input
3. `text_input_triggered` events being processed multiple times
4. `CreateTextInput` being pushed twice in callback changes

### Files to Investigate
- `layout/src/window.rs` - `process_text_input()`, `apply_text_changeset()`
- `layout/src/managers/text_input.rs` - TextInputManager
- `dll/src/desktop/shell2/common/event_v2.rs` - `process_callback_result_v2()`

---

## Bug 3: Wrong Text Input Affected (Second input shifts when typing in first)

### Symptoms
- Typing in the first (single-line) text input
- The SECOND (multi-line) text input shifts/moves
- Suggests layout recalculation is affecting wrong nodes

### Expected Behavior
- Only the focused text input should be affected by typing

### Likely Causes
1. `dirty_text_nodes` marking wrong node as dirty
2. Relayout affecting sibling/parent nodes incorrectly
3. `DomNodeId` confusion between DOM nodes
4. Scroll states being incorrectly updated

### Files to Investigate
- `layout/src/window.rs` - `relayout_dirty_nodes()`, `update_text_cache_after_edit()`
- `layout/src/solver3/mod.rs` - Layout tree building

---

## Bug 4: Mouse Move Triggers Horrible Resize

### Symptoms
- After typing, moving the mouse causes the first text input to resize horribly
- Many repeated debug outputs in debugoutput.txt showing the same character being processed
- Text grows exponentially

### Expected Behavior
- Mouse movement should not affect text content or trigger text input

### Likely Causes
1. Mouse move event incorrectly triggering text input processing
2. `text_input_affected_nodes` not being cleared after processing
3. Hover events incorrectly invoking text input callbacks
4. State machine in event_v2.rs not correctly separating mouse from keyboard events

### Files to Investigate
- `dll/src/desktop/shell2/common/event_v2.rs` - Event filtering
- `layout/src/managers/text_input.rs` - Changeset clearing

---

## Bug 5: Single-Line Input Breaking onto Multiple Lines

### Symptoms
- Single-line text input (should have `white-space: nowrap`) breaks onto multiple lines
- Screenshot shows text wrapping character-by-character
- `overflow: hidden` also seems ignored

### Expected Behavior
- Single-line input should:
  - Not wrap text (white-space: nowrap)
  - Clip overflow (overflow: hidden)
  - Scroll horizontally if needed

### Likely Causes
1. CSS `white-space: nowrap` not being applied during relayout
2. Text constraints cache not including white-space property
3. `update_text_cache_after_edit()` not respecting original constraints
4. Container width being ignored during text layout

### Files to Investigate
- `layout/src/text3/cache.rs` - UnifiedConstraints, word breaking
- `layout/src/window.rs` - `text_constraints_cache`
- `layout/src/solver3/fc.rs` - IFC layout

---

## Bug 6: No Scroll Into View

### Symptoms
- When text extends beyond visible area, view doesn't scroll to cursor
- User cannot see what they're typing

### Expected Behavior
- Cursor should always be visible
- Container should scroll to keep cursor in view

### Likely Causes
1. `scroll_active_cursor_into_view()` not being called
2. `PostCallbackSystemEvent::ScrollIntoView` not being generated
3. Scroll manager not receiving scroll commands

### Files to Investigate
- `layout/src/window.rs` - `scroll_active_cursor_into_view()`
- `dll/src/desktop/shell2/common/event_v2.rs` - Post-callback scroll handling

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            TEXT INPUT FLOW (V3)                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  1. User clicks on contenteditable                                          │
│     └─► Mouse event → Hit test → Focus node → Start cursor blink timer     │
│                                         ▲                                    │
│                                         │ BUG 1: Cursor not appearing        │
│                                                                              │
│  2. User types 'j'                                                           │
│     └─► Keyboard event → Timer callback → create_text_input()               │
│                              │                                               │
│                              ▼                                               │
│     └─► CallbackChange::CreateTextInput { text: "j" }                       │
│                              │                                               │
│                              ▼                                               │
│     └─► apply_callback_changes() → process_text_input()                    │
│                              │         ▲                                     │
│                              │         │ BUG 2: Called twice?                │
│                              ▼                                               │
│     └─► text_input_triggered populated                                      │
│                              │                                               │
│                              ▼                                               │
│     └─► process_callback_result_v2() invokes user callbacks                │
│                              │                                               │
│                              ▼                                               │
│     └─► apply_text_changeset() → update_text_cache_after_edit()            │
│                              │         ▲                                     │
│                              │         │ BUG 5: Constraints not preserved    │
│                              ▼                                               │
│     └─► relayout_dirty_nodes() → mark for repaint                          │
│                              │    ▲                                          │
│                              │    │ BUG 3: Wrong nodes affected              │
│                              │    │ BUG 4: Mouse move re-triggers            │
│                              ▼                                               │
│     └─► Display list updated → Text appears on screen                      │
│                                    ▲                                         │
│                                    │ BUG 6: No scroll into view              │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Debug Information Needed

1. **Full event trace**: Every event from click to text appearing
2. **CallbackChange log**: What changes are pushed and when
3. **TextInputManager state**: Pending changeset lifecycle
4. **Cursor position**: Where cursor is set (or not set)
5. **Dirty nodes list**: Which nodes marked as dirty
6. **Text constraints**: What constraints are used for relayout
7. **Layout tree before/after**: How layout changes with each character

---

## Files Relevant to Analysis

### Core Event Handling
- `dll/src/desktop/shell2/common/event_v2.rs`
- `dll/src/desktop/shell2/common/debug_server.rs`

### Callback System
- `layout/src/callbacks.rs`
- `layout/src/window.rs`

### Text Input Management
- `layout/src/managers/text_input.rs`
- `layout/src/managers/cursor.rs`
- `layout/src/managers/focus_cursor.rs`
- `layout/src/managers/selection.rs`

### Text Layout
- `layout/src/text3/cache.rs`
- `layout/src/text3/edit.rs`
- `layout/src/text3/selection.rs`

### Layout Solver
- `layout/src/solver3/mod.rs`
- `layout/src/solver3/fc.rs`
- `layout/src/solver3/display_list.rs`

---

## Recent Changes (Last 10 Commits to Review)

Need to check if recent changes introduced regressions:
1. Adding `text_input_triggered` to `CallCallbacksResult`
2. Removing `PENDING_DEBUG_TEXT_INPUT` hack
3. Changes to `process_callback_result_v2()`
4. Changes to `apply_callback_changes()`
5. Changes to `CreateTextInput` handling

---

## Test Case

```c
// tests/e2e/contenteditable.c
// Creates:
// 1. Single-line text input with "Hello World - Click here and type!"
// 2. Multi-line text area
// Both are contenteditable divs
```

Expected behavior:
1. Click on text → cursor appears at click position
2. Type 'j' → 'j' appears at cursor position
3. Cursor moves right one position
4. Text scrolls into view if needed
5. Single-line input doesn't wrap

Actual behavior:
1. Click → focus but NO cursor
2. Type 'j' → 'jj' appears
3. Second input shifts
4. Mouse move → text explodes
5. Text wraps onto many lines


## Recent Git Diffs (Last 10 Commits)

```diff
commit f5ddf8b3b0f2dd0c9c858eccbdc9fb0c3af65d1d
Author: Felix Schütt <felix.schuett@maps4print.com>
Date:   Wed Jan 28 00:41:05 2026 +0100

    Add plan for full text input implementation and architecture
---
 scripts/TEXT_INPUT_IMPLEMENTATION_PLAN_V3.md | 920 +++++++++++++++++++++++++++
 1 file changed, 920 insertions(+)

diff --git a/scripts/TEXT_INPUT_IMPLEMENTATION_PLAN_V3.md b/scripts/TEXT_INPUT_IMPLEMENTATION_PLAN_V3.md
new file mode 100644
index 00000000..f7116344
--- /dev/null
+++ b/scripts/TEXT_INPUT_IMPLEMENTATION_PLAN_V3.md
@@ -0,0 +1,920 @@
+# Text Input Implementation Plan V3
+
+## Executive Summary
+
+This document provides the definitive implementation plan for Azul's text input system. The key architectural insight is the **dual-path layout system**:
+
+1. **Initial Layout** → Runs on `StyledDom` (committed state from `layout()` callback)
+2. **Relayout** → Runs on `LayoutCache` (respects quick edits, handles text node resizing)
+
+This separation enables:
+- Instant visual feedback during typing (no callback latency)
+- Proper layout shift handling when text causes reflow
+- Clean separation between "optimistic" and "committed" state
+- Support for complex multi-node editing
+
+---
+
+## Part 1: Architectural Overview
+
+### 1.1 The Two Layout Paths
+
+```
+┌─────────────────────────────────────────────────────────────────────────┐
+│                         INITIAL LAYOUT PATH                              │
+│                                                                          │
+│   User Data Model ──► layout() callback ──► StyledDom ──► LayoutCache   │
+│        (RefAny)           (pure fn)         (committed)    (visual)      │
+│                                                                          │
+│   Triggered by: Update::RefreshDom, window resize, first render          │
+└─────────────────────────────────────────────────────────────────────────┘
+
+┌─────────────────────────────────────────────────────────────────────────┐
+│                          RELAYOUT PATH                                   │
+│                                                                          │
+│   LayoutCache ──► detect dirty nodes ──► partial relayout ──► repaint   │
+│   (with edits)      (text changed)        (text only)         (fast)     │
+│                                                                          │
+│   Triggered by: Text input, cursor movement, selection change            │
+└─────────────────────────────────────────────────────────────────────────┘
+```
+
+### 1.2 Key Data Flow
+
+```
+User types 'a' 
+    │
+    ▼
+┌──────────────────────────────────────────────────────────────────┐
+│ 1. Platform layer receives keypress (macOS: NSTextInputClient)  │
+└──────────────────────────────────────────────────────────────────┘
+    │
+    ▼
+┌──────────────────────────────────────────────────────────────────┐
+│ 2. TextInputManager.record_input() creates PendingTextEdit      │
+│    - Records: inserted_text="a", old_text, node                  │
+│    - Does NOT modify any caches yet                              │
+└──────────────────────────────────────────────────────────────────┘
+    │
+    ▼
+┌──────────────────────────────────────────────────────────────────┐
+│ 3. Synthetic 'Input' event generated for contenteditable node   │
+└──────────────────────────────────────────────────────────────────┘
+    │
+    ▼
+┌──────────────────────────────────────────────────────────────────┐
+│ 4. User's On::TextInput callback fires                          │
+│    - Can call info.get_text_changeset() to inspect              │
+│    - Can call info.prevent_default() to cancel                  │
+│    - Updates their data model (RefAny)                          │
+│    - Returns Update::DoNothing (fast) or Update::RefreshDom     │
+└──────────────────────────────────────────────────────────────────┘
+    │
+    ▼
+┌──────────────────────────────────────────────────────────────────┐
+│ 5. If NOT prevented: apply_text_changeset()                     │
+│    - Calls text3::edit::edit_text() to compute new content      │
+│    - Calls update_text_cache_after_edit() for visual update     │
+│    - Updates cursor position                                     │
+│    - Marks node as dirty for relayout                           │
+└──────────────────────────────────────────────────────────────────┘
+    │
+    ▼
+┌──────────────────────────────────────────────────────────────────┐
+│ 6. Relayout runs on dirty nodes                                 │
+│    - Reads from LayoutCache (with edits), NOT StyledDom         │
+│    - Handles text node resizing                                  │
+│    - Propagates layout shifts to ancestors if needed            │
+└──────────────────────────────────────────────────────────────────┘
+    │
+    ▼
+┌──────────────────────────────────────────────────────────────────┐
+│ 7. Display list regenerated, repaint triggered                  │
+└──────────────────────────────────────────────────────────────────┘
+```
+
+---
+
+## Part 2: Data Structures
+
+### 2.1 New Fields in LayoutWindow
+
+```rust
+// In layout/src/window.rs
+
+pub struct LayoutWindow {
+    // ... existing fields ...
+    
+    /// Cache of text layout constraints for each IFC root node.
+    /// Used to perform consistent optimistic updates.
+    pub text_constraints_cache: TextConstraintsCache,
+    
+    /// Tracks which nodes have been edited since last full layout.
+    /// Key: (DomId, NodeId of IFC root)
+    /// Value: The edited Vec<InlineContent> that should be used for relayout
+    pub dirty_text_nodes: BTreeMap<(DomId, NodeId), DirtyTextNode>,
+}
+
+#[derive(Debug, Clone)]
+pub struct TextConstraintsCache {
+    /// Constraints used for each IFC during initial layout
+    pub constraints: BTreeMap<(DomId, NodeId), UnifiedConstraints>,
+}
+
+#[derive(Debug, Clone)]
+pub struct DirtyTextNode {
+    /// The new inline content (text + images) after editing
+    pub content: Vec<InlineContent>,
+    /// The new cursor position after editing
+    pub cursor: Option<TextCursor>,
+    /// Whether this edit requires ancestor relayout (e.g., text grew taller)
+    pub needs_ancestor_relayout: bool,
+}
+```
+
+### 2.2 UnifiedConstraints (Already Exists, Needs Caching)
+
+```rust
+// In layout/src/text3/cache.rs
+
+#[derive(Debug, Clone)]
+pub struct UnifiedConstraints {
+    pub available_width: AvailableSpace,
+    pub text_align: StyleTextAlign,
+    pub direction: Option<BidiDirection>,
+    pub writing_mode: WritingMode,
+    pub line_height: LineHeight,
+    pub word_break: WordBreak,
+    pub overflow_wrap: OverflowWrap,
+    pub white_space: WhiteSpace,
+    pub text_indent: f32,
+    pub letter_spacing: f32,
+    pub word_spacing: f32,
+    // ... etc
+}
+```
+
+### 2.3 Enhanced PendingTextEdit
+
+```rust
+// In layout/src/managers/text_input.rs
+
+#[derive(Debug, Clone)]
+pub struct PendingTextEdit {
+    /// The IFC root node being edited
+    pub node: DomNodeId,
+    /// The text that was inserted (can be empty for deletions)
+    pub inserted_text: String,
+    /// The old Vec<InlineContent> before the edit
+    pub old_content: Vec<InlineContent>,
+    /// The new Vec<InlineContent> after the edit (computed by text3::edit)
+    pub new_content: Option<Vec<InlineContent>>,
+    /// The new cursor position after the edit
+    pub new_cursor: Option<TextCursor>,
+    /// Source of the edit
+    pub source: TextInputSource,
+}
+```
+
+---
+
+## Part 3: Event Handling Flow
+
+### 3.1 Platform Layer → TextInputManager
+
+**File:** `dll/src/desktop/shell2/macos/text_input.rs` (and Windows/Linux equivalents)
+
+```rust
+// When user types a character:
+
+fn insert_text(&mut self, string: &str) {
+    let layout_window = self.get_layout_window_mut();
+    
+    // 1. Find the focused contenteditable node
+    let focused_node = layout_window.focus_manager.get_focused_node();
+    let Some(node_id) = focused_node else { return };
+    
+    // 2. Get current content from cache (NOT StyledDom!)
+    let old_content = layout_window.get_current_inline_content(node_id);
+    
+    // 3. Record the input (Phase 1 - just record, don't apply)
+    layout_window.text_input_manager.record_input(
+        node_id,
+        string.to_string(),
+        old_content,
+        TextInputSource::Keyboard,
+    );
+    
+    // 4. Generate synthetic Input event
+    layout_window.pending_events.push(SyntheticEvent::new(
+        EventType::Input,
+        EventSource::User,
+        node_id,
+        Instant::now(),
+        EventData::None,
+    ));
+}
+```
+
+### 3.2 Event Dispatch → User Callback
+
+**File:** `dll/src/desktop/shell2/common/event_v2.rs`
+
+```rust
+fn process_input_event(
+    &mut self,
+    event: &SyntheticEvent,
+    layout_window: &mut LayoutWindow,
+) -> ProcessedCallbackResult {
+    // 1. Find callbacks registered for On::TextInput on this node
+    let callbacks = layout_window.get_callbacks_for_event(event);
+    
+    // 2. Invoke each callback
+    let mut result = ProcessedCallbackResult::default();
+    for callback_data in callbacks {
+        let callback_info = CallbackInfo::new(layout_window, event);
+        
+        // User callback can:
+        // - info.get_text_changeset() to see what's being inserted
+        // - info.prevent_default() to cancel the edit
+        // - Modify their RefAny data model
+        let update = (callback_data.callback.cb)(
+            callback_data.refany.clone(),
+            callback_info,
+        );
+        
+        result.update.max_self(update);
+    }
+    
+    // 3. Check if prevented
+    if result.changes.contains(&CallbackChange::PreventDefault) {
+        layout_window.text_input_manager.clear_changeset();
+        return result;
+    }
+    
+    // 4. Apply the changeset (Phase 2)
+    layout_window.apply_text_changeset();
+    
+    result
+}
+```
+
+### 3.3 Applying the Changeset
+
+**File:** `layout/src/window.rs`
+
+```rust
+pub fn apply_text_changeset(&mut self) {
+    let Some(changeset) = self.text_input_manager.pending_changeset.take() else {
+        return;
+    };
+    
+    let dom_id = changeset.node.dom;
+    let node_id = changeset.node.node.into_crate_internal().unwrap();
+    
+    // 1. Get current cursor position
+    let current_cursor = self.cursor_manager.get_cursor();
+    
+    // 2. Get the old content (from cache if dirty, else from original layout)
+    let old_content = self.get_current_inline_content_internal(dom_id, node_id);
+    
+    // 3. Compute the edit using text3::edit
+    let edit_result = crate::text3::edit::edit_text(
+        &old_content,
+        &changeset.inserted_text,
+        current_cursor,
+        self.selection_manager.get_selection(dom_id),
+    );
+    
+    let Some((new_content, new_selections)) = edit_result else {
+        return;
+    };
+    
+    // 4. Update the visual cache (optimistic update)
+    self.update_text_cache_after_edit(dom_id, node_id, new_content.clone());
+    
+    // 5. Update cursor position
+    if let Some(new_cursor) = new_selections.cursor {
+        let now = Instant::now();
+        self.cursor_manager.set_cursor_with_time(
+            Some(new_cursor),
+            Some(CursorLocation { dom_id, node_id }),
+            now,
+        );
+    }
+    
+    // 6. Mark node as dirty for relayout
+    self.dirty_text_nodes.insert((dom_id, node_id), DirtyTextNode {
+        content: new_content,
+        cursor: new_selections.cursor,
+        needs_ancestor_relayout: false, // Will be determined during relayout
+    });
+    
+    // 7. Schedule relayout
+    self.needs_relayout = true;
+}
+```
+
+---
+
+## Part 4: The Dual Layout System
+
+### 4.1 Initial Layout (StyledDom Path)
+
+**File:** `layout/src/solver3/mod.rs`
+
+```rust
+/// Full layout pass - reads from StyledDom
+/// Called on: first render, Update::RefreshDom, window resize
+pub fn layout_document(
+    styled_dom: &StyledDom,
+    constraints: &LayoutConstraints,
+    font_manager: &mut FontManager,
+    // ... other params
+) -> LayoutTree {
+    // Clear dirty nodes - we're rebuilding from committed state
+    // The dirty_text_nodes map should be cleared by caller
+    
+    // Traverse StyledDom and build layout tree
+    for node in styled_dom.nodes() {
+        match node.node_type {
+            NodeType::Text(ref text) => {
+                // Convert text to InlineContent
+                let inline_content = text_to_inline_content(text, node.style);
+                // Layout the text
+                let layout = layout_inline_formatting_context(inline_content, ...);
+                // Cache the constraints for later relayout
+                ctx.text_constraints_cache.insert((dom_id, node_id), constraints);
+            }
+            // ... other node types
+        }
+    }
+}
+```
+
+### 4.2 Relayout (LayoutCache Path)
+
+**File:** `layout/src/window.rs`
+
+```rust
+/// Partial relayout - respects dirty text nodes
+/// Called on: text input, when needs_relayout is true
+pub fn relayout_dirty_nodes(&mut self) {
+    if !self.needs_relayout || self.dirty_text_nodes.is_empty() {
+        return;
+    }
+    
+    for ((dom_id, node_id), dirty_node) in self.dirty_text_nodes.iter() {
+        // 1. Get cached constraints
+        let Some(constraints) = self.text_constraints_cache.constraints.get(&(*dom_id, *node_id)) else {
+            continue;
+        };
+        
+        // 2. Re-run lightweight text layout
+        let new_layout = self.relayout_text_node(*dom_id, *node_id, &dirty_node.content, constraints);
+        
+        let Some(new_layout) = new_layout else {
+            continue;
+        };
+        
+        // 3. Check if size changed (needs ancestor relayout)
+        let old_size = self.get_node_size(*dom_id, *node_id);
+        let new_size = new_layout.bounds().size();
+        
+        if old_size.height != new_size.height || old_size.width != new_size.width {
+            // Text node changed size - need to propagate layout shift
+            self.propagate_layout_shift(*dom_id, *node_id, old_size, new_size);
+        }
+        
+        // 4. Update the cache
+        self.update_layout_cache(*dom_id, *node_id, new_layout);
+    }
+    
+    self.needs_relayout = false;
+    self.needs_display_list_update = true;
+}
+
+fn relayout_text_node(
+    &self,
+    dom_id: DomId,
+    node_id: NodeId,
+    content: &[InlineContent],
+    constraints: &UnifiedConstraints,
+) -> Option<UnifiedLayout> {
+    use crate::text3::cache::{
+        create_logical_items, reorder_logical_items, 
+        shape_visual_items, perform_fragment_layout, BreakCursor
+    };
+    
+    // Stage 1: Create logical items from InlineContent
+    let logical_items = create_logical_items(content, &[], &mut None);
+    
+    // Stage 2: Bidi reordering
+    let base_direction = constraints.direction.unwrap_or(BidiDirection::Ltr);
+    let visual_items = reorder_logical_items(&logical_items, base_direction, &mut None)?;
+    
+    // Stage 3: Shape text (resolve fonts, create glyphs)
+    let loaded_fonts = self.font_manager.get_loaded_fonts();
+    let shaped_items = shape_visual_items(
+        &visual_items,
+        self.font_manager.get_font_chain_cache(),
+        &self.font_manager.fc_cache,
+        &loaded_fonts,
+        &mut None,
+    )?;
+    
+    // Stage 4: Fragment layout (line breaking, positioning)
+    let mut cursor = BreakCursor::new(&shaped_items);
+    perform_fragment_layout(&mut cursor, &logical_items, constraints, &mut None, &loaded_fonts).ok()
+}
+```
+
+### 4.3 Layout Shift Propagation
+
+```rust
+fn propagate_layout_shift(
+    &mut self,
+    dom_id: DomId,
+    node_id: NodeId,
+    old_size: LogicalSize,
+    new_size: LogicalSize,
+) {
+    // When a text node changes size, ancestors may need relayout
+    // This is the "layout shift" that can cascade up the tree
+    
+    let height_delta = new_size.height - old_size.height;
+    let width_delta = new_size.width - old_size.width;
+    
+    if height_delta.abs() < 0.001 && width_delta.abs() < 0.001 {
+        return; // No significant change
+    }
+    
+    // For now: mark that we need full relayout for this DOM
+    // Future optimization: incremental ancestor relayout
+    self.needs_full_relayout.insert(dom_id);
+}
+```
+
+---
+
+## Part 5: Cursor and Selection
+
+### 5.1 Cursor Click Positioning
+
+**File:** `layout/src/text3/selection.rs`
+
+```rust
+use azul_core::geom::LogicalPosition;
+use azul_core::selection::{TextCursor, CursorAffinity, GraphemeClusterId};
+use crate::text3::cache::{UnifiedLayout, PositionedItem, ShapedItem};
+use std::collections::BTreeMap;
+
+/// Maps a click position to a TextCursor within a UnifiedLayout.
+/// The `point` must be relative to the layout's container origin.
+pub fn hit_test_text_at_point(
+    layout: &UnifiedLayout,
+    point: LogicalPosition,
+) -> Option<TextCursor> {
+    if layout.items.is_empty() {
+        // Empty contenteditable - cursor at beginning
+        return Some(TextCursor {
+            cluster_id: GraphemeClusterId::default(),
+            affinity: CursorAffinity::Leading,
+        });
+    }
+    
+    // Step 1: Find the line closest to the Y coordinate
+    let mut line_bounds: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
+    for item in &layout.items {
+        let bounds = item.item.bounds();
+        let entry = line_bounds.entry(item.line_index).or_insert((f32::MAX, f32::MIN));
+        entry.0 = entry.0.min(item.position.y);
+        entry.1 = entry.1.max(item.position.y + bounds.height);
+    }
+    
+    let closest_line = line_bounds.iter()
+        .min_by(|(_, (a_min, a_max)), (_, (b_min, b_max))| {
+            let a_center = (a_min + a_max) / 2.0;
+            let b_center = (b_min + b_max) / 2.0;
+            (point.y - a_center).abs().partial_cmp(&(point.y - b_center).abs()).unwrap()
+        })
+        .map(|(idx, _)| *idx)
+        .unwrap_or(0);
+    
+    // Step 2: Find the closest cluster on that line
+    let clusters_on_line: Vec<_> = layout.items.iter()
+        .filter(|item| item.line_index == closest_line)
+        .filter(|item| item.item.as_cluster().is_some())
+        .collect();
+    
+    if clusters_on_line.is_empty() {
+        // Empty line - find previous line's last cluster
+        return layout.items.iter().rev()
+            .filter(|item| item.line_index < closest_line)
+            .find_map(|item| item.item.as_cluster().map(|c| TextCursor {
+                cluster_id: c.source_cluster_id,
+                affinity: CursorAffinity::Trailing,
+            }));
+    }
+    
+    let closest_cluster = clusters_on_line.iter()
+        .min_by(|a, b| {
+            let a_dist = horizontal_distance(point.x, a);
+            let b_dist = horizontal_distance(point.x, b);
+            a_dist.partial_cmp(&b_dist).unwrap()
+        })?;
+    
+    let cluster = closest_cluster.item.as_cluster()?;
+    
+    // Step 3: Determine affinity (leading vs trailing half)
+    let cluster_mid_x = closest_cluster.position.x + cluster.advance / 2.0;
+    let affinity = if point.x < cluster_mid_x {
+        CursorAffinity::Leading
+    } else {
+        CursorAffinity::Trailing
+    };
+    
+    Some(TextCursor {
+        cluster_id: cluster.source_cluster_id,
+        affinity,
+    })
+}
+
+fn horizontal_distance(x: f32, item: &PositionedItem) -> f32 {
+    let bounds = item.item.bounds();
+    let left = item.position.x;
+    let right = left + bounds.width;
+    
+    if x < left {
+        left - x
+    } else if x > right {
+        x - right
+    } else {
+        0.0
+    }
+}
+```
+
+### 5.2 Focus Transfer
+
+**File:** `layout/src/window.rs`
+
+```rust
+/// Handles focus change for cursor blinking
+/// Returns the action the platform should take for the blink timer
+pub fn handle_focus_change_for_cursor_blink(
+    &mut self,
+    old_focus: Option<DomNodeId>,
+    new_focus: Option<DomNodeId>,
+) -> CursorBlinkTimerAction {
+    // Clear old cursor if focus was on a contenteditable
+    if let Some(old_node) = old_focus {
+        if self.is_node_contenteditable(old_node) {
+            self.cursor_manager.clear();
+        }
+    }
+    
+    // Initialize new cursor if focus is on a contenteditable
+    if let Some(new_node) = new_focus {
+        if self.is_node_contenteditable(new_node) {
+            // Set flag for deferred initialization (will be overridden by click)
+            self.focus_manager.set_pending_contenteditable_focus(
+                new_node.dom,
+                new_node.node.into_crate_internal().unwrap(),
+            );
+            return CursorBlinkTimerAction::Start;
+        }
+    }
+    
+    CursorBlinkTimerAction::Stop
+}
+
+/// Called after layout pass to finalize deferred focus changes
+pub fn finalize_pending_focus_changes(&mut self) {
+    if let Some((dom_id, node_id)) = self.focus_manager.take_pending_contenteditable_focus() {
+        // Get the layout for this node
+        if let Some(layout) = self.get_inline_layout_for_node(dom_id, node_id) {
+            // Place cursor at end of text
+            let cursor = get_cursor_at_end(&layout);
+            let now = Instant::now();
+            self.cursor_manager.set_cursor_with_time(
+                Some(cursor),
+                Some(CursorLocation { dom_id, node_id }),
+                now,
+            );
+        }
+    }
+}
+```
+
+---
+
+## Part 6: Callback Info API
+
+### 6.1 Text Changeset Access
+
+**File:** `layout/src/callbacks.rs`
+
+```rust
+impl CallbackInfo {
+    /// Get the pending text changeset for the current Input event.
+    /// Returns None if this is not a text input event.
+    pub fn get_text_changeset(&self) -> Option<&PendingTextEdit> {
+        self.get_layout_window()
+            .text_input_manager
+            .get_pending_changeset()
+    }
+    
+    /// Prevent the default text input behavior.
+    /// The typed character will not be inserted.
+    pub fn prevent_default(&mut self) {
+        self.push_change(CallbackChange::PreventDefault);
+    }
+    
+    /// Override the text that will be inserted.
+    /// Useful for input filtering or transformation.
+    pub fn set_text_changeset(&mut self, new_text: String) {
+        self.push_change(CallbackChange::SetInsertedText { text: new_text });
+    }
+    
+    /// Change the text of a node (for TextInput widget pattern)
+    pub fn change_node_text(&mut self, node_id: DomNodeId, new_text: AzString) {
+        self.push_change(CallbackChange::ChangeNodeText { node_id, text: new_text });
+    }
+}
+```
+
+### 6.2 TextInput Widget Pattern (Reference)
+
+**File:** `layout/src/widgets/text_input.rs`
+
+```rust
+/// The TextInput widget demonstrates the "controlled component" pattern:
+/// 1. Widget has internal state (TextInputStateWrapper)
+/// 2. On text input, callback fires BEFORE visual update
+/// 3. Callback can validate/transform input
+/// 4. If valid, callback updates its internal state
+/// 5. Callback calls info.change_node_text() for visual update
+
+extern "C" fn default_on_text_input(text_input: RefAny, info: CallbackInfo) -> Update {
+    let mut text_input = text_input.downcast_mut::<TextInputStateWrapper>()?;
+    
+    // 1. Get the changeset
+    let changeset = info.get_text_changeset()?;
+    let inserted_text = changeset.inserted_text.clone();
+    
+    if inserted_text.is_empty() {
+        return Update::DoNothing;
+    }
+    
+    // 2. Call user's validation callback if set
+    let validation_result = if let Some(on_text_input) = &text_input.on_text_input {
+        let new_state = compute_new_state(&text_input.inner, &inserted_text);
+        (on_text_input.callback.cb)(on_text_input.refany.clone(), info.clone(), new_state)
+    } else {
+        OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes }
+    };
+    
+    // 3. If valid, apply the change
+    if validation_result.valid == TextInputValid::Yes {
+        // Update internal state
+        text_input.inner.text.extend(inserted_text.chars().map(|c| c as u32));
+        text_input.inner.cursor_pos += inserted_text.len();
+        
+        // Update visual (for custom TextInput widget)
+        let label_node_id = get_label_node_id(&info);
+        info.change_node_text(label_node_id, text_input.inner.get_text().into());
+    } else {
+        // Prevent the edit
+        info.prevent_default();
+    }
+    
+    validation_result.update
+}
+```
+
+---
+
+## Part 7: Implementation Steps
+
+### Step 1: Add TextConstraintsCache (Day 1)
+
+**Files to modify:**
+- `layout/src/window.rs` - Add `text_constraints_cache` field to `LayoutWindow`
+- `layout/src/solver3/fc.rs` - Cache constraints during IFC layout
+
+```rust
+// In layout/src/window.rs
+impl LayoutWindow {
+    pub fn new(...) -> Self {
+        Self {
+            // ... existing fields
+            text_constraints_cache: TextConstraintsCache::default(),
+            dirty_text_nodes: BTreeMap::new(),
+            needs_relayout: false,
+        }
+    }
+}
+
+// In layout/src/solver3/fc.rs, in layout_inline_formatting_context()
+// After creating constraints:
+if let Some(cache) = ctx.text_constraints_cache.as_mut() {
+    cache.constraints.insert((ctx.dom_id, ifc_root_node_id), constraints.clone());
+}
+```
+
+### Step 2: Implement update_text_cache_after_edit (Day 1-2)
+
+**File:** `layout/src/window.rs`
+
+Replace the TODO stub with the full implementation from Part 4.2.
+
+### Step 3: Add hit_test_text_at_point (Day 2)
+
+**File:** `layout/src/text3/selection.rs`
+
+Add the function from Part 5.1.
+
+### Step 4: Integrate with MouseDown Handler (Day 2-3)
+
+**File:** `dll/src/desktop/shell2/common/event_v2.rs`
+
+```rust
+// In handle_mouse_down or process_mouse_event:
+
+fn handle_mouse_down_for_text(
+    &mut self,
+    position: LogicalPosition,
+    layout_window: &mut LayoutWindow,
+) {
+    // 1. Hit test to find node under cursor
+    let hit_result = layout_window.hit_test_point(position);
+    
+    // 2. Check if it's a contenteditable
+    if let Some(hit_node) = hit_result.deepest_contenteditable() {
+        let dom_id = hit_node.dom;
+        let node_id = hit_node.node.into_crate_internal().unwrap();
+        
+        // 3. Get the inline layout for hit testing
+        if let Some(inline_layout) = layout_window.get_inline_layout_for_node(dom_id, node_id) {
+            // 4. Calculate local position relative to node
+            let node_pos = layout_window.get_node_position(hit_node).unwrap_or_default();
+            let local_pos = LogicalPosition {
+                x: position.x - node_pos.x,
+                y: position.y - node_pos.y,
+            };
+            
+            // 5. Hit test for cursor position
+            if let Some(cursor) = hit_test_text_at_point(&inline_layout, local_pos) {
+                // 6. Set focus
+                let old_focus = layout_window.focus_manager.get_focused_node();
+                layout_window.focus_manager.set_focused_node(Some(hit_node));
+                
+                // 7. Handle focus change (stops old timer, starts new)
+                layout_window.handle_focus_change_for_cursor_blink(old_focus, Some(hit_node));
+                
+                // 8. Set cursor position (overrides deferred init)
+                let now = Instant::now();
+                layout_window.cursor_manager.set_cursor_with_time(
+                    Some(cursor),
+                    Some(CursorLocation { dom_id, node_id }),
+                    now,
+                );
+                
+                // 9. Clear any selection
+                layout_window.selection_manager.clear_text_selection(&dom_id);
+            }
+        }
+    }
+}
+```
+
+### Step 5: Implement relayout_dirty_nodes (Day 3-4)
+
+**File:** `layout/src/window.rs`
+
+Add the function from Part 4.2 and integrate it into the render loop.
+
+### Step 6: Add Event Processing Integration (Day 4)
+
+**File:** `dll/src/desktop/shell2/common/event_v2.rs`
+
+Ensure the Input event triggers the correct flow from Part 3.2.
+
+### Step 7: Testing (Day 5)
+
+Create test cases for:
+1. Single character insertion
+2. Backspace deletion
+3. Multi-character paste
+4. Cursor positioning on click
+5. Focus transfer between inputs
+6. Text that causes layout shift (line wrap)
+
+---
+
+## Part 8: Test Cases
+
+### 8.1 Basic Text Input
+
+```c
+// tests/e2e/contenteditable_basic.c
+
+void test_single_char_input() {
+    // 1. Focus contenteditable
+    // 2. Type 'a'
+    // Expected: 'a' appears, cursor moves right
+    // StyledDom: unchanged
+    // LayoutCache: updated
+}
+
+void test_backspace() {
+    // 1. Focus contenteditable with "hello"
+    // 2. Press Backspace
+    // Expected: 'hell' remains, cursor at end
+}
+
+void test_paste() {
+    // 1. Focus empty contenteditable
+    // 2. Paste "hello world"
+    // Expected: full text appears, cursor at end
+}
+```
+
+### 8.2 Cursor Positioning
+
+```c
+void test_click_positioning() {
+    // 1. Create contenteditable with "hello world"
+    // 2. Click in middle of "world"
+    // Expected: cursor appears between 'o' and 'r'
+}
+
+void test_click_empty_line() {
+    // 1. Create contenteditable with "line1\n\nline3"
+    // 2. Click on empty line 2
+    // Expected: cursor at start of line 2
+}
+```
+
+### 8.3 Focus Transfer
+
+```c
+void test_focus_transfer_click() {
+    // 1. Two contenteditables
+    // 2. Focus first, type "hello"
+    // 3. Click on second
+    // Expected: first cursor gone, second cursor at click position
+}
+
+void test_focus_transfer_tab() {
+    // 1. Two contenteditables
+    // 2. Focus first
+    // 3. Press Tab
+    // Expected: second gets focus, cursor at end of its text
+}
+```
+
+### 8.4 Layout Shift
+
+```c
+void test_line_wrap() {
+    // 1. Create narrow contenteditable (100px wide)
+    // 2. Type long text that wraps to second line
+    // Expected: text wraps, container height increases
+}
+```
+
+---
+
+## Part 9: Known Limitations & Future Work
+
+### Current Scope (V3)
+- Single text node editing
+- Basic cursor positioning
+- Focus transfer
+- Text-only content
+
+### Future Work (V4+)
+- Multi-node selections (bold/italic spans)
+- Inline images from clipboard
+- Undo/redo integration
+- IME composition support
+- Right-to-left text
+- Vertical writing modes
+
+---
+
+## Appendix: Key File Locations
+
+| Component | File | Key Functions |
+|-----------|------|---------------|
+| TextInputManager | `layout/src/managers/text_input.rs` | `record_input()`, `get_pending_changeset()` |
+| CursorManager | `layout/src/managers/cursor.rs` | `set_cursor_with_time()`, `clear()` |
+| FocusManager | `layout/src/managers/focus_cursor.rs` | `set_focused_node()`, `set_pending_contenteditable_focus()` |
+| Window Coordination | `layout/src/window.rs` | `apply_text_changeset()`, `update_text_cache_after_edit()`, `relayout_dirty_nodes()` |
+| Text Editing | `layout/src/text3/edit.rs` | `edit_text()`, `insert_text()`, `delete_range()` |
+| Text Layout | `layout/src/text3/cache.rs` | `perform_fragment_layout()`, `shape_visual_items()` |
+| Event Handling | `dll/src/desktop/shell2/common/event_v2.rs` | `process_input_event()`, `handle_mouse_down_for_text()` |
+| CallbackInfo | `layout/src/callbacks.rs` | `get_text_changeset()`, `prevent_default()` |
+| Display List | `layout/src/solver3/display_list.rs` | Cursor/selection rendering |

commit 90f50a9ef5df8421f97bc4debc99ceb8693e3f91
Author: Felix Schütt <felix.schuett@maps4print.com>
Date:   Wed Jan 28 00:40:32 2026 +0100

    Debug contenteditable E2E test with _getTextChangeset
---
 tests/e2e/.gitignore                 |   1 +
 tests/e2e/contenteditable.c          |  63 ++++-
 tests/e2e/test_contenteditable_v2.sh | 451 +++++++++++++++++++++++++++++++++++
 3 files changed, 507 insertions(+), 8 deletions(-)

diff --git a/tests/e2e/.gitignore b/tests/e2e/.gitignore
index 761c5b87..d8acac91 100644
--- a/tests/e2e/.gitignore
+++ b/tests/e2e/.gitignore
@@ -14,6 +14,7 @@ focus
 refany_test
 focus_scroll
 focus_scroll_test
+text_area_test
 
 # New test binaries
 text_input
diff --git a/tests/e2e/contenteditable.c b/tests/e2e/contenteditable.c
index e5e2cbe4..c76d5ba4 100644
--- a/tests/e2e/contenteditable.c
+++ b/tests/e2e/contenteditable.c
@@ -53,9 +53,45 @@ AzUpdate on_text_input(AzRefAny data, AzCallbackInfo info) {
         return AzUpdate_DoNothing;
     }
     
-    ref.ptr->text_change_count++;
+    // Get the text changeset from the callback info
+    AzOptionPendingTextEdit changeset = AzCallbackInfo_getTextChangeset(&info);
+    
+    if (changeset.Some.tag == AzOptionPendingTextEdit_Tag_Some) {
+        AzPendingTextEdit* edit = &changeset.Some.payload;
+        
+        // Print the changeset for debugging
+        printf("[TextInput] Changeset received:\n");
+        printf("  inserted_text: '%.*s'\n", 
+               (int)edit->inserted_text.vec.len, 
+               (const char*)edit->inserted_text.vec.ptr);
+        printf("  old_text: '%.*s' (len=%zu)\n", 
+               (int)(edit->old_text.vec.len > 50 ? 50 : edit->old_text.vec.len),
+               (const char*)edit->old_text.vec.ptr,
+               edit->old_text.vec.len);
+        
+        // Append the inserted text to our data model
+        // For single-line, we just append to the existing text
+        size_t current_len = strlen(ref.ptr->single_line_text);
+        size_t insert_len = edit->inserted_text.vec.len;
+        
+        if (current_len + insert_len < sizeof(ref.ptr->single_line_text) - 1) {
+            memcpy(ref.ptr->single_line_text + current_len, 
+                   edit->inserted_text.vec.ptr, 
+                   insert_len);
+            ref.ptr->single_line_text[current_len + insert_len] = '\0';
+            printf("  Updated single_line_text: '%s'\n", ref.ptr->single_line_text);
+        }
+        
+        ref.ptr->text_change_count++;
+    } else {
+        printf("[TextInput] No changeset available\n");
+    }
+    
     ContentEditableDataRefMut_delete(&ref);
-    return AzUpdate_RefreshDom;
+    
+    // Return DoNothing - the text input system handles the visual update internally
+    // RefreshDom would override the internal edit with the old data model state
+    return AzUpdate_DoNothing;
 }
 
 // Callback for key press events
@@ -132,10 +168,9 @@ const char* CSS_STYLE =
     "    background-color: #252525;\n"
     "}\n"
     "\n"
-    "/* Cursor styling */\n"
-    "::cursor {\n"
-    "    width: 3px;\n"
-    "    background-color: #ffffff;\n"
+    "/* Cursor styling - use caret-color for text cursor */\n"
+    ".single-line-input, .multi-line-textarea {\n"
+    "    caret-color: #ffffff;\n"
     "}\n"
     "\n"
     "/* Selection styling */\n"
@@ -162,11 +197,17 @@ AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
     AzDom_addChild(&root, label1);
     
     // Single-line contenteditable input
-    AzDom single_input = AzDom_createText(AZ_STR(ref.ptr->single_line_text));
+    // Create a div with text child, set contenteditable on the div
+    AzDom single_input = AzDom_createDiv();
     AzDom_addClass(&single_input, AZ_STR("single-line-input"));
+    AzDom_setContenteditable(&single_input, true);
     AzTabIndex tab_auto = { .Auto = { .tag = AzTabIndex_Tag_Auto } };
     AzDom_setTabIndex(&single_input, tab_auto);
     
+    // Add text as child
+    AzDom single_text = AzDom_createText(AZ_STR(ref.ptr->single_line_text));
+    AzDom_addChild(&single_input, single_text);
+    
     // Add text input callback - use Focus filter for text input
     AzEventFilter text_filter = AzEventFilter_focus(AzFocusEventFilter_TextInput);
     AzDom_addCallback(&single_input, text_filter, AzRefAny_clone(&data), on_text_input);
@@ -179,10 +220,16 @@ AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
     AzDom_addChild(&root, label2);
     
     // Multi-line contenteditable textarea
-    AzDom multi_input = AzDom_createText(AZ_STR(ref.ptr->multi_line_text));
+    // Create a div with text child, set contenteditable on the div
+    AzDom multi_input = AzDom_createDiv();
     AzDom_addClass(&multi_input, AZ_STR("multi-line-textarea"));
+    AzDom_setContenteditable(&multi_input, true);
     AzDom_setTabIndex(&multi_input, tab_auto);
     
+    // Add text as child
+    AzDom multi_text = AzDom_createText(AZ_STR(ref.ptr->multi_line_text));
+    AzDom_addChild(&multi_input, multi_text);
+    
     // Add callbacks
     AzDom_addCallback(&multi_input, text_filter, AzRefAny_clone(&data), on_text_input);
     
diff --git a/tests/e2e/test_contenteditable_v2.sh b/tests/e2e/test_contenteditable_v2.sh
new file mode 100755
index 00000000..7a88c3e4
--- /dev/null
+++ b/tests/e2e/test_contenteditable_v2.sh
@@ -0,0 +1,451 @@
+#!/bin/bash
+# ContentEditable E2E Test Suite v2
+#
+# Tests contenteditable text input, cursor blinking, focus, cursor state, and selection
+#
+# Usage: ./test_contenteditable_v2.sh
+#
+# Prerequisites:
+#   1. Build the test: cc contenteditable.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o contenteditable_test -Wl,-rpath,../../target/release
+#   2. Have jq installed for JSON parsing
+
+set -e
+
+SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
+cd "$SCRIPT_DIR"
+
+DEBUG_PORT=8766
+DEBUG_URL="http://localhost:$DEBUG_PORT/"
+
+# Colors for output
+RED='\033[0;31m'
+GREEN='\033[0;32m'
+YELLOW='\033[0;33m'
+BLUE='\033[0;34m'
+NC='\033[0m' # No Color
+
+PASSED=0
+FAILED=0
+WARNINGS=0
+
+echo "================================================"
+echo "ContentEditable E2E Test Suite v2"
+echo "Focus, Cursor, and Blink Timer Tests"
+echo "================================================"
+
+# Build the test executable
+echo -e "${YELLOW}Building contenteditable_test...${NC}"
+cc contenteditable.c -I../../target/codegen/v2/ -L../../target/release/ -lazul -o contenteditable_test -Wl,-rpath,../../target/release 2>&1
+if [ $? -ne 0 ]; then
+    echo -e "${RED}Build failed${NC}"
+    exit 1
+fi
+echo -e "${GREEN}Build successful${NC}"
+
+# Start the test app in background
+echo -e "${YELLOW}Starting contenteditable_test with debug server on port $DEBUG_PORT...${NC}"
+AZUL_DEBUG=$DEBUG_PORT ./contenteditable_test &
+APP_PID=$!
+
+# Give the app time to start and render first frame
+sleep 3
+
+# Function to send debug command
+send_cmd() {
+    curl -s --connect-timeout 2 -X POST "$DEBUG_URL" -d "$1" 2>/dev/null
+}
+
+# Function to check if app is running
+check_app() {
+    if ! kill -0 $APP_PID 2>/dev/null; then
+        echo -e "${RED}App crashed!${NC}"
+        exit 1
+    fi
+}
+
+# Assert function
+assert_eq() {
+    local actual="$1"
+    local expected="$2"
+    local msg="$3"
+    if [ "$actual" = "$expected" ]; then
+        echo -e "${GREEN}PASS: $msg${NC}"
+        ((PASSED++))
+        return 0
+    else
+        echo -e "${RED}FAIL: $msg${NC}"
+        echo -e "${RED}  Expected: $expected${NC}"
+        echo -e "${RED}  Actual: $actual${NC}"
+        ((FAILED++))
+        return 1
+    fi
+}
+
+# Assert not empty
+assert_not_empty() {
+    local value="$1"
+    local msg="$2"
+    if [ -n "$value" ] && [ "$value" != "null" ] && [ "$value" != "none" ]; then
+        echo -e "${GREEN}PASS: $msg (value=$value)${NC}"
+        ((PASSED++))
+        return 0
+    else
+        echo -e "${RED}FAIL: $msg (empty or null)${NC}"
+        ((FAILED++))
+        return 1
+    fi
+}
+
+# Assert boolean true
+assert_true() {
+    local value="$1"
+    local msg="$2"
+    if [ "$value" = "true" ]; then
+        echo -e "${GREEN}PASS: $msg${NC}"
+        ((PASSED++))
+        return 0
+    else
+        echo -e "${RED}FAIL: $msg (got $value, expected true)${NC}"
+        ((FAILED++))
+        return 1
+    fi
+}
+
+# Assert boolean false
+assert_false() {
+    local value="$1"
+    local msg="$2"
+    if [ "$value" = "false" ]; then
+        echo -e "${GREEN}PASS: $msg${NC}"
+        ((PASSED++))
+        return 0
+    else
+        echo -e "${RED}FAIL: $msg (got $value, expected false)${NC}"
+        ((FAILED++))
+        return 1
+    fi
+}
+
+# Warn
+warn() {
+    local msg="$1"
+    echo -e "${YELLOW}WARN: $msg${NC}"
+    ((WARNINGS++))
+}
+
+# Cleanup on exit
+cleanup() {
+    echo -e "\n${YELLOW}Cleaning up...${NC}"
+    kill $APP_PID 2>/dev/null || true
+}
+trap cleanup EXIT
+
+# Wait for debug server to be ready
+echo "Waiting for debug server..."
+for i in {1..20}; do
+    RESULT=$(curl -s --connect-timeout 1 -X POST "$DEBUG_URL" -d '{"op": "get_state"}' 2>/dev/null)
+    if [ -n "$RESULT" ] && echo "$RESULT" | jq -e '.status == "ok"' > /dev/null 2>&1; then
+        echo -e "${GREEN}Debug server ready${NC}"
+        break
+    fi
+    sleep 0.5
+done
+
+# Wait for initial layout and render
+sleep 1
+send_cmd '{"op": "wait_frame"}'
+sleep 0.5
+
+# ============================================================================
+# Test Group 1: Initial State - No Focus
+# ============================================================================
+echo ""
+echo -e "${BLUE}=== Test Group 1: Initial State ===${NC}"
+
+sleep 0.5
+FOCUS_STATE=$(send_cmd '{"op": "get_focus_state"}')
+echo "DEBUG: Focus state response: $FOCUS_STATE"
+HAS_FOCUS=$(echo "$FOCUS_STATE" | jq -r '.data.value.has_focus // false')
+assert_false "$HAS_FOCUS" "Initial state: no focus"
+
+sleep 0.3
+CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
+echo "DEBUG: Cursor state response: $CURSOR_STATE"
+HAS_CURSOR=$(echo "$CURSOR_STATE" | jq -r '.data.value.has_cursor // false')
+assert_false "$HAS_CURSOR" "Initial state: no cursor"
+
+# ============================================================================
+# Test Group 2: Focus on Contenteditable via Tab
+# ============================================================================
+echo ""
+echo -e "${BLUE}=== Test Group 2: Focus via Tab ===${NC}"
+
+# Tab to first contenteditable
+echo "Sending Tab key..."
+send_cmd '{"op": "key_down", "key": "Tab"}'
+sleep 0.5
+send_cmd '{"op": "wait_frame"}'
+sleep 0.5
+send_cmd '{"op": "wait_frame"}'
+sleep 0.3
+
+# Check focus state
+FOCUS_STATE=$(send_cmd '{"op": "get_focus_state"}')
+echo "Focus state: $FOCUS_STATE"
+
+HAS_FOCUS=$(echo "$FOCUS_STATE" | jq -r '.data.value.has_focus // false')
+assert_true "$HAS_FOCUS" "After Tab: has focus" || true
+
+IS_CONTENTEDITABLE=$(echo "$FOCUS_STATE" | jq -r '.data.value.focused_node.is_contenteditable // false')
+assert_true "$IS_CONTENTEDITABLE" "Focused node is contenteditable" || true
+
+sleep 0.3
+
+# Check cursor state
+CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
+echo "Cursor state: $CURSOR_STATE"
+
+HAS_CURSOR=$(echo "$CURSOR_STATE" | jq -r '.data.value.has_cursor // false')
+assert_true "$HAS_CURSOR" "After Tab: cursor exists" || true
+
+# Check blink timer is active
+BLINK_TIMER_ACTIVE=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.blink_timer_active')
+if [ "$BLINK_TIMER_ACTIVE" = "true" ]; then
+    echo -e "${GREEN}PASS: Blink timer is active${NC}"
+    ((PASSED++))
+else
+    warn "Blink timer not active (may not be implemented yet)"
+fi
+
+# Check cursor is visible initially
+IS_VISIBLE=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.is_visible')
+assert_true "$IS_VISIBLE" "Cursor is initially visible"
+
+check_app
+
+# ============================================================================
+# Test Group 3: Cursor Position
+# ============================================================================
+echo ""
+echo -e "${BLUE}=== Test Group 3: Cursor Position ===${NC}"
+
+CURSOR_POS=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.position')
+CURSOR_AFFINITY=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.affinity')
+
+echo "Cursor position: $CURSOR_POS, affinity: $CURSOR_AFFINITY"
+assert_eq "$CURSOR_POS" "0" "Initial cursor at position 0"
+assert_not_empty "$CURSOR_AFFINITY" "Cursor has affinity"
+
+check_app
+
+# ============================================================================
+# Test Group 4: Text Input
+# ============================================================================
+echo ""
+echo -e "${BLUE}=== Test Group 4: Text Input ===${NC}"
+
+echo "Sending text input 'Hello'..."
+send_cmd '{"op": "text_input", "text": "Hello"}'
+sleep 0.5
+send_cmd '{"op": "wait_frame"}'
+sleep 0.3
+
+CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
+CURSOR_POS=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.position // -1')
+echo "After typing 'Hello', cursor position: $CURSOR_POS"
+assert_eq "$CURSOR_POS" "5" "Cursor moved to position 5 after typing 'Hello'"
+
+# Cursor should be visible after typing (input resets blink)
+IS_VISIBLE=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.is_visible')
+assert_true "$IS_VISIBLE" "Cursor visible after typing"
+
+check_app
+
+# ============================================================================
+# Test Group 5: Arrow Key Navigation
+# ============================================================================
+echo ""
+echo -e "${BLUE}=== Test Group 5: Arrow Key Navigation ===${NC}"
+
+# Move cursor left
+echo "Pressing Left arrow..."
+send_cmd '{"op": "key_down", "key": "Left"}'
+sleep 0.3
+send_cmd '{"op": "wait_frame"}'
+sleep 0.2
+
+CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
+CURSOR_POS=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.position // -1')
+echo "After Left arrow, cursor position: $CURSOR_POS"
+assert_eq "$CURSOR_POS" "4" "Cursor moved left to position 4"
+
+# Move to beginning (Home)
+echo "Pressing Home key..."
+send_cmd '{"op": "key_down", "key": "Home"}'
+sleep 0.3
+send_cmd '{"op": "wait_frame"}'
+sleep 0.2
+
+CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
+CURSOR_POS=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.position // -1')
+echo "After Home key, cursor position: $CURSOR_POS"
+assert_eq "$CURSOR_POS" "0" "Cursor moved to beginning (position 0)"
+
+check_app
+
+# ============================================================================
+# Test Group 6: Selection State
+# ============================================================================
+echo ""
+echo -e "${BLUE}=== Test Group 6: Selection State ===${NC}"
+
+# Select All (Ctrl+A)
+echo "Pressing Ctrl+A..."
+send_cmd '{"op": "key_down", "key": "A", "modifiers": {"ctrl": true}}'
+sleep 0.3
+send_cmd '{"op": "wait_frame"}'
+sleep 0.2
+
+SELECTION_STATE=$(send_cmd '{"op": "get_selection_state"}')
+echo "Selection state: $SELECTION_STATE"
+
+HAS_SELECTION=$(echo "$SELECTION_STATE" | jq -r '.data.value.has_selection')
+assert_true "$HAS_SELECTION" "Has selection after Ctrl+A"
+
+# Check selection range
+SELECTION_TYPE=$(echo "$SELECTION_STATE" | jq -r '.data.value.selections[0].ranges[0].selection_type')
+if [ "$SELECTION_TYPE" = "range" ]; then
+    SELECTION_START=$(echo "$SELECTION_STATE" | jq -r '.data.value.selections[0].ranges[0].start')
+    SELECTION_END=$(echo "$SELECTION_STATE" | jq -r '.data.value.selections[0].ranges[0].end')
+    echo "Selection range: $SELECTION_START to $SELECTION_END"
+    assert_eq "$SELECTION_START" "0" "Selection starts at 0"
+    assert_eq "$SELECTION_END" "5" "Selection ends at 5 (Hello = 5 chars)"
+else
+    warn "Expected selection type 'range', got '$SELECTION_TYPE'"
+fi
+
+check_app
+
+# ============================================================================
+# Test Group 7: Cursor Blink Timer (530ms intervals)
+# ============================================================================
+echo ""
+echo -e "${BLUE}=== Test Group 7: Cursor Blink Timer ===${NC}"
+
+# First, clear selection by pressing Right (or any key)
+send_cmd '{"op": "key_down", "key": "Right"}'
+sleep 0.1
+
+# Wait 600ms (should toggle cursor visibility at ~530ms)
+echo "Waiting 600ms for blink toggle..."
+send_cmd '{"op": "wait", "ms": 600}'
+
+CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
+IS_VISIBLE_AFTER_WAIT=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.is_visible')
+echo "Cursor visibility after 600ms wait: $IS_VISIBLE_AFTER_WAIT"
+
+# We can't strictly assert this because timing may vary, but log it
+if [ "$IS_VISIBLE_AFTER_WAIT" = "false" ]; then
+    echo -e "${GREEN}PASS: Cursor toggled to invisible after blink interval${NC}"
+    ((PASSED++))
+else
+    warn "Cursor still visible after 600ms (blink timer may not be working)"
+fi
+
+# Type to reset blink
+send_cmd '{"op": "text_input", "text": " "}'
+sleep 0.1
+
+CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
+IS_VISIBLE=$(echo "$CURSOR_STATE" | jq -r '.data.value.cursor.is_visible')
+assert_true "$IS_VISIBLE" "Cursor visible after input (blink reset)"
+
+check_app
+
+# ============================================================================
+# Test Group 8: Focus Loss Clears Cursor
+# ============================================================================
+echo ""
+echo -e "${BLUE}=== Test Group 8: Focus Loss ===${NC}"
+
+# Send Escape to blur (or Tab away)
+send_cmd '{"op": "key_down", "key": "Escape"}'
+sleep 0.2
+send_cmd '{"op": "wait_frame"}'
+
+FOCUS_STATE=$(send_cmd '{"op": "get_focus_state"}')
+HAS_FOCUS=$(echo "$FOCUS_STATE" | jq -r '.data.value.has_focus')
+
+CURSOR_STATE=$(send_cmd '{"op": "get_cursor_state"}')
+HAS_CURSOR=$(echo "$CURSOR_STATE" | jq -r '.data.value.has_cursor')
+
+# Note: Escape may or may not clear focus depending on implementation
+echo "After Escape: has_focus=$HAS_FOCUS, has_cursor=$HAS_CURSOR"
+
+if [ "$HAS_FOCUS" = "false" ]; then
+    assert_false "$HAS_CURSOR" "Cursor cleared when focus lost"
+else
+    warn "Escape did not clear focus (may need different blur mechanism)"
+fi
+
+check_app
+
+# ============================================================================
+# Test Group 9: Click to Focus
+# ============================================================================
+echo ""
+echo -e "${BLUE}=== Test Group 9: Click to Focus ===${NC}"
+
+# Click on the single-line input (get its layo

... [DIFFS TRUNCATED] ...
```

## Analysis Request

Please analyze the code and help me identify:

1. **Root Cause of Double Input**: Why is each keypress inserting the character twice?
   Look at how `text_input_triggered` flows through the system and where callbacks
   might be invoked twice.

2. **Root Cause of Wrong Input Affected**: Why does typing in input 1 modify input 2?
   Look at how `dom_node_id` is tracked and whether the focus system is correct.

3. **Root Cause of State Not Updating**: Why is `old_text` always the original text?
   Look at how `TextInputState` is stored and updated between frames.

4. **Root Cause of Cursor Not Appearing**: Why doesn't the cursor blink after click?
   Look at how focus events trigger cursor initialization.

5. **Root Cause of Layout Issues**: Why does the single-line input wrap to multiple lines?
   Look at how `white-space: nowrap` is handled in the layout solver.

For each bug, please:
- Identify the specific file(s) and function(s) involved
- Explain the flow of data/control that leads to the bug
- Suggest a concrete fix with code changes

Focus especially on the interaction between:
- `process_text_input()` in window.rs
- `text_input_triggered` propagation
- `invoke_callbacks_v2()` and `process_callback_result_v2()` in event_v2.rs
- How `CallbackChange::CreateTextInput` is processed
