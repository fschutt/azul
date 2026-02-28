//! Cross-platform event processing system
//!
//! This module contains the **complete unified event processing logic** that is shared across all
//! platforms (macOS, Windows, X11, Wayland). The system uses state-diffing between frames to
//! detect events, eliminating platform-specific event handling differences.
//!
//! ## Architecture
//!
//! The `PlatformWindow` trait provides **default implementations** for all complex logic:
//! - Event processing (state diffing via `process_window_events()`)
//! - Callback invocation (`dispatch_events_propagated()`)
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
//! When migrating a platform to use `PlatformWindow`.

use alloc::sync::Arc;
use core::cell::RefCell;
use std::collections::BTreeMap;

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::{DomId, NodeId},
    events::{
        EventFilter, FocusEventFilter, PreCallbackFilterResult,
        ProcessEventResult, SyntheticEvent, SystemChange,
    },
    geom::LogicalPosition,
    gl::*,
    hit_test::{DocumentId, PipelineId},
    id::NodeId as CoreNodeId,
    refany::RefAny,
    resources::{IdNamespace, ImageCache, RendererResources},
    styled_dom::NodeHierarchyItemId,
    window::RawWindowHandle,
    FastBTreeSet,
};
use azul_layout::{
    callbacks::{
        Callback as LayoutCallback, CallbackInfo, ExternalSystemCallbacks,
    },
    event_determination::determine_all_events,
    hit_test::FullHitTest,
    window::{LayoutWindow, ScrollbarDragState},
    window_state::{self, FullWindowState},
};
use rust_fontconfig::FcFontCache;

use crate::desktop::wr_translate2::{self, AsyncHitTester, WrRenderApi};
use crate::{log_debug, log_warn};

/// Parse a node type string into a NodeType.
/// Supports tag names ("div", "p", "span", "button", etc.)
/// and text content ("text:Hello World").
fn parse_node_type_from_str(s: &str) -> azul_core::dom::NodeType {
    use azul_core::dom::NodeType;
    if let Some(text) = s.strip_prefix("text:") {
        return NodeType::Text(text.to_string().into());
    }
    match s.to_lowercase().as_str() {
        "html" => NodeType::Html,
        "head" => NodeType::Head,
        "body" => NodeType::Body,
        "div" => NodeType::Div,
        "p" => NodeType::P,
        "article" => NodeType::Article,
        "section" => NodeType::Section,
        "nav" => NodeType::Nav,
        "aside" => NodeType::Aside,
        "header" => NodeType::Header,
        "footer" => NodeType::Footer,
        "main" => NodeType::Main,
        "h1" => NodeType::H1,
        "h2" => NodeType::H2,
        "h3" => NodeType::H3,
        "h4" => NodeType::H4,
        "h5" => NodeType::H5,
        "h6" => NodeType::H6,
        "br" => NodeType::Br,
        "hr" => NodeType::Hr,
        "pre" => NodeType::Pre,
        "blockquote" => NodeType::BlockQuote,
        "ul" => NodeType::Ul,
        "ol" => NodeType::Ol,
        "li" => NodeType::Li,
        "table" => NodeType::Table,
        "thead" => NodeType::THead,
        "tbody" => NodeType::TBody,
        "tr" => NodeType::Tr,
        "th" => NodeType::Th,
        "td" => NodeType::Td,
        "form" => NodeType::Form,
        "label" => NodeType::Label,
        "input" => NodeType::Input,
        "button" => NodeType::Button,
        "select" => NodeType::Select,
        "textarea" => NodeType::TextArea,
        "span" => NodeType::Span,
        "a" => NodeType::A,
        "em" => NodeType::Em,
        "strong" => NodeType::Strong,
        "b" => NodeType::B,
        "i" => NodeType::I,
        "u" => NodeType::U,
        "code" => NodeType::Code,
        "img" | "image" => NodeType::Div, // image needs ImageRef, fallback to div
        "canvas" => NodeType::Canvas,
        "svg" => NodeType::Svg,
        "details" => NodeType::Details,
        "summary" => NodeType::Summary,
        "figure" => NodeType::Figure,
        "figcaption" => NodeType::FigCaption,
        _ => NodeType::Div, // default to div for unknown tags
    }
}

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
        "[Event] Focus restyle: needs_layout={}, needs_display_list={}, changed_nodes={}, max_scope={:?}",
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

/// Common window state shared across all platform window implementations.
///
/// Contains the 17 fields that are accessed via the 28 PlatformWindow getter/setter methods.
/// Each platform window struct should contain this as `pub common: CommonWindowState` and use
/// `impl_platform_window_getters!(common)` to generate all 28 trivial getter implementations.
///
/// Fields that are `Option<T>` here may be non-Option on some platforms (macOS, Win32)
/// but are wrapped in Option for a common representation. The getters use `.expect()`
/// for these fields — they should always be `Some(...)` by the time they're accessed.
pub struct CommonWindowState {
    /// LayoutWindow integration (for UI callbacks and display list)
    pub layout_window: Option<LayoutWindow>,
    /// Current window state
    pub current_window_state: FullWindowState,
    /// Window state from previous frame (for diff detection)
    pub previous_window_state: Option<FullWindowState>,
    /// Image cache for texture management
    pub image_cache: ImageCache,
    /// Renderer resources (GPU textures, etc.)
    pub renderer_resources: RendererResources,
    /// Shared font cache (shared across windows)
    pub fc_cache: Arc<FcFontCache>,
    /// OpenGL context pointer with compiled SVG and FXAA shaders
    pub gl_context_ptr: OptionGlContextPtr,
    /// System style (shared across windows)
    pub system_style: Arc<azul_css::system::SystemStyle>,
    /// Shared application data (used by callbacks, shared across windows)
    pub app_data: Arc<RefCell<RefAny>>,
    /// Current scrollbar drag state (if dragging a scrollbar thumb)
    pub scrollbar_drag_state: Option<ScrollbarDragState>,
    /// Hit-tester for fast asynchronous hit-testing (updated on layout changes).
    /// `None` only during initialization on X11/Wayland before WebRender is set up.
    pub hit_tester: Option<AsyncHitTester>,
    /// Last hovered node (for hover state tracking)
    pub last_hovered_node: Option<HitTestNode>,
    /// WebRender document ID. `None` only during X11/Wayland initialization.
    pub document_id: Option<DocumentId>,
    /// WebRender ID namespace. `None` only during X11/Wayland initialization.
    pub id_namespace: Option<IdNamespace>,
    /// Main render API for registering fonts, images, display lists.
    /// `None` only during X11/Wayland initialization.
    pub render_api: Option<WrRenderApi>,
    /// WebRender renderer (software or hardware depending on backend)
    pub renderer: Option<webrender::Renderer>,
    /// Track if frame needs regeneration (to avoid multiple generate_frame calls)
    pub frame_needs_regeneration: bool,
    /// Whether a WebRender display list has ever been sent for this window.
    /// Used to force a full display list build on the very first frame, even if
    /// regenerate_layout() returns LayoutUnchanged (because create_window already
    /// ran regenerate_layout for accessibility/font init).
    pub display_list_initialized: bool,
}

/// Generates all 28 PlatformWindow getter/setter implementations
/// by delegating to `self.$field` (a `CommonWindowState` field).
///
/// Usage: `impl_platform_window_getters!(common);`
/// where `common` is the field name on the platform struct.
///
/// Each getter borrows only its own field via `self.$field.xxx`, so the compiler
/// sees independent borrows and split borrows work naturally.
#[macro_export]
macro_rules! impl_platform_window_getters {
    ($field:ident) => {
        fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow> {
            self.$field.layout_window.as_mut()
        }
        fn get_layout_window(&self) -> Option<&LayoutWindow> {
            self.$field.layout_window.as_ref()
        }
        fn get_current_window_state(&self) -> &FullWindowState {
            &self.$field.current_window_state
        }
        fn get_current_window_state_mut(&mut self) -> &mut FullWindowState {
            &mut self.$field.current_window_state
        }
        fn get_previous_window_state(&self) -> &Option<FullWindowState> {
            &self.$field.previous_window_state
        }
        fn set_previous_window_state(&mut self, state: FullWindowState) {
            self.$field.previous_window_state = Some(state);
        }
        fn get_image_cache_mut(&mut self) -> &mut ImageCache {
            &mut self.$field.image_cache
        }
        fn get_renderer_resources_mut(&mut self) -> &mut RendererResources {
            &mut self.$field.renderer_resources
        }
        fn get_fc_cache(&self) -> &Arc<FcFontCache> {
            &self.$field.fc_cache
        }
        fn get_gl_context_ptr(&self) -> &OptionGlContextPtr {
            &self.$field.gl_context_ptr
        }
        fn get_system_style(&self) -> &Arc<azul_css::system::SystemStyle> {
            &self.$field.system_style
        }
        fn get_app_data(&self) -> &Arc<RefCell<RefAny>> {
            &self.$field.app_data
        }
        fn get_scrollbar_drag_state(&self) -> Option<&ScrollbarDragState> {
            self.$field.scrollbar_drag_state.as_ref()
        }
        fn get_scrollbar_drag_state_mut(&mut self) -> &mut Option<ScrollbarDragState> {
            &mut self.$field.scrollbar_drag_state
        }
        fn set_scrollbar_drag_state(&mut self, state: Option<ScrollbarDragState>) {
            self.$field.scrollbar_drag_state = state;
        }
        fn get_hit_tester(&self) -> &AsyncHitTester {
            self.$field.hit_tester.as_ref().expect("hit_tester not initialized")
        }
        fn get_hit_tester_mut(&mut self) -> &mut AsyncHitTester {
            self.$field.hit_tester.as_mut().expect("hit_tester not initialized")
        }
        fn get_last_hovered_node(&self) -> Option<&HitTestNode> {
            self.$field.last_hovered_node.as_ref()
        }
        fn set_last_hovered_node(&mut self, node: Option<HitTestNode>) {
            self.$field.last_hovered_node = node;
        }
        fn get_document_id(&self) -> DocumentId {
            self.$field.document_id.expect("document_id not initialized")
        }
        fn get_id_namespace(&self) -> IdNamespace {
            self.$field.id_namespace.expect("id_namespace not initialized")
        }
        fn get_render_api(&self) -> &WrRenderApi {
            self.$field.render_api.as_ref().expect("render_api not initialized")
        }
        fn get_render_api_mut(&mut self) -> &mut WrRenderApi {
            self.$field.render_api.as_mut().expect("render_api not initialized")
        }
        fn get_renderer(&self) -> Option<&webrender::Renderer> {
            self.$field.renderer.as_ref()
        }
        fn get_renderer_mut(&mut self) -> Option<&mut webrender::Renderer> {
            self.$field.renderer.as_mut()
        }
        fn needs_frame_regeneration(&self) -> bool {
            self.$field.frame_needs_regeneration
        }
        fn mark_frame_needs_regeneration(&mut self) {
            self.$field.frame_needs_regeneration = true;
        }
        fn clear_frame_regeneration_flag(&mut self) {
            self.$field.frame_needs_regeneration = false;
        }
    };
}

/// Trait that platform-specific window types must implement to use the unified event system.
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
/// - `process_window_events()` - Main event processing with recursion
/// - `apply_user_change()` - Apply individual callback changes
/// - `perform_scrollbar_hit_test()` - Scrollbar interaction
/// - `handle_scrollbar_click()` - Scrollbar click handling
/// - `handle_scrollbar_drag()` - Scrollbar drag handling
/// - `gpu_scroll()` - GPU-accelerated smooth scrolling
///
/// ## Platform Implementation Checklist
///
/// To integrate a new platform:
/// 1. Implement the 26 required getter methods
/// 2. Import the trait: `use crate::desktop::shell2::common::event::PlatformWindow;`
/// 3. Call `self.process_window_events(0)` after updating window state
/// 4. Done! All event processing is now unified.
pub trait PlatformWindow {
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
    /// * `threads` - Threads to add to the pool
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

    // REQUIRED: Window Creation (Platform-Specific Implementation)

    /// Queue a new window to be created by the event loop.
    ///
    /// Pushes the `WindowCreateOptions` onto `self.pending_window_creates`.
    /// The event loop (in `run.rs`) pops from this queue after each event
    /// iteration and creates the platform window.
    ///
    /// ## Parameters
    /// * `options` - Configuration for the new window
    fn queue_window_create(&mut self, options: azul_layout::window_state::WindowCreateOptions);

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

    // PROVIDED: Exhaustive Callback Change Processing (Cross-Platform)

    /// Process a single user-initiated callback change.
    ///
    /// This is the SINGLE place where all `CallbackChange` variants are handled.
    /// Adding a new variant causes a compile error here — no silent bugs.
    ///
    /// Single exhaustive match over all `CallbackChange` variants.
    fn apply_user_change(
        &mut self,
        change: &azul_layout::callbacks::CallbackChange,
    ) -> ProcessEventResult {
        use azul_layout::callbacks::CallbackChange;
        use azul_core::callbacks::Update;

        match change {
            // === Window State ===

            CallbackChange::ModifyWindowState { state } => {
                let old_mouse_state = self.get_current_window_state().mouse_state.clone();
                let old_keyboard_state = self.get_current_window_state().keyboard_state.clone();
                let mouse_state_changed = old_mouse_state != state.mouse_state;
                let keyboard_state_changed = old_keyboard_state != state.keyboard_state;

                // Save previous state BEFORE modifying (for synthetic event detection)
                if mouse_state_changed || keyboard_state_changed {
                    let old_state = self.get_current_window_state().clone();
                    self.set_previous_window_state(old_state);
                }

                // Apply state changes
                {
                    let current = self.get_current_window_state_mut();
                    current.title = state.title.clone();
                    current.size = state.size;
                    current.position = state.position;
                    current.flags = state.flags;
                    current.background_color = state.background_color;
                    current.mouse_state = state.mouse_state.clone();
                    current.keyboard_state = state.keyboard_state.clone();
                }

                if state.flags.close_requested {
                    return ProcessEventResult::DoNothing;
                }

                let mut result = ProcessEventResult::ShouldReRenderCurrentWindow;

                // Mouse state changed → update hit test and re-process events
                if mouse_state_changed {
                    let mouse_pos = self.get_current_window_state()
                        .mouse_state.cursor_position.get_position();
                    if let Some(pos) = mouse_pos {
                        self.update_hit_test_at(pos);
                    }
                    let nested = self.process_window_events(0);
                    result = result.max(nested);
                }

                // Keyboard state changed → re-process events
                if keyboard_state_changed && !mouse_state_changed {
                    let nested = self.process_window_events(0);
                    result = result.max(nested);
                }

                result
            }

            CallbackChange::QueueWindowStateSequence { states } => {
                let mut result = ProcessEventResult::DoNothing;
                for queued_state in states {
                    let old_state = self.get_current_window_state().clone();
                    self.set_previous_window_state(old_state);

                    {
                        let current = self.get_current_window_state_mut();
                        current.mouse_state = queued_state.mouse_state.clone();
                        current.keyboard_state = queued_state.keyboard_state.clone();
                        current.title = queued_state.title.clone();
                        current.size = queued_state.size;
                        current.position = queued_state.position;
                        current.flags = queued_state.flags;
                    }

                    let mouse_pos = queued_state.mouse_state.cursor_position.get_position();
                    if let Some(pos) = mouse_pos {
                        self.update_hit_test_at(pos);
                    }

                    let nested = self.process_window_events(0);
                    result = result.max(nested);
                }
                result
            }

            CallbackChange::CreateNewWindow { options } => {
                self.queue_window_create(options.clone());
                ProcessEventResult::DoNothing
            }

            CallbackChange::CloseWindow => {
                self.get_current_window_state_mut().flags.close_requested = true;
                ProcessEventResult::DoNothing
            }

            // === Focus ===

            CallbackChange::SetFocusTarget { target } => {
                // Resolve focus target to actual node
                let resolved = if let Some(lw) = self.get_layout_window() {
                    let current_focus = lw.focus_manager.get_focused_node().copied();
                    azul_layout::managers::focus_cursor::resolve_focus_target(
                        target,
                        &lw.layout_results,
                        current_focus,
                    ).ok().flatten()
                } else {
                    None
                };

                // Clear focus case: target resolves to None
                let is_clear = if let Some(lw) = self.get_layout_window() {
                    let current_focus = lw.focus_manager.get_focused_node().copied();
                    matches!(
                        azul_layout::managers::focus_cursor::resolve_focus_target(
                            target, &lw.layout_results, current_focus,
                        ),
                        Ok(None)
                    )
                } else {
                    false
                };

                if let Some(new_focus) = resolved {
                    // Focus a specific node
                    let timer_action = if let Some(lw) = self.get_layout_window_mut() {
                        lw.focus_manager.set_focused_node(Some(new_focus));

                        use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;
                        let now = azul_core::task::Instant::now();
                        lw.scroll_node_into_view(new_focus, ScrollIntoViewOptions::nearest(), now);

                        let ws = lw.current_window_state.clone();
                        Some(lw.handle_focus_change_for_cursor_blink(Some(new_focus), &ws))
                    } else {
                        None
                    };

                    if let Some(action) = timer_action {
                        match action {
                            azul_layout::CursorBlinkTimerAction::Start(timer) => {
                                self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                            }
                            azul_layout::CursorBlinkTimerAction::Stop => {
                                self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                            }
                            azul_layout::CursorBlinkTimerAction::NoChange => {}
                        }
                    }
                    ProcessEventResult::ShouldReRenderCurrentWindow
                } else if is_clear {
                    // Clear focus
                    let timer_action = if let Some(lw) = self.get_layout_window_mut() {
                        lw.focus_manager.set_focused_node(None);
                        let ws = lw.current_window_state.clone();
                        Some(lw.handle_focus_change_for_cursor_blink(None, &ws))
                    } else {
                        None
                    };

                    if let Some(action) = timer_action {
                        match action {
                            azul_layout::CursorBlinkTimerAction::Start(_) => {}
                            azul_layout::CursorBlinkTimerAction::Stop => {
                                self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                            }
                            azul_layout::CursorBlinkTimerAction::NoChange => {}
                        }
                    }
                    ProcessEventResult::ShouldReRenderCurrentWindow
                } else {
                    ProcessEventResult::DoNothing
                }
            }

            // === Propagation Control (consumed by dispatch loop, no-op here) ===

            CallbackChange::StopPropagation
            | CallbackChange::StopImmediatePropagation
            | CallbackChange::PreventDefault => ProcessEventResult::DoNothing,

            // === Timer Management ===

            CallbackChange::AddTimer { timer_id, timer } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.timers.insert(*timer_id, timer.clone());
                }
                self.start_timer(timer_id.id, timer.clone());
                ProcessEventResult::DoNothing
            }

            CallbackChange::RemoveTimer { timer_id } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.timers.remove(timer_id);
                }
                self.stop_timer(timer_id.id);
                ProcessEventResult::DoNothing
            }

            // === Thread Management ===

            CallbackChange::AddThread { thread_id, thread } => {
                let had_threads = self.get_layout_window()
                    .map(|lw| !lw.threads.is_empty()).unwrap_or(false);

                if let Some(lw) = self.get_layout_window_mut() {
                    lw.threads.insert(*thread_id, thread.clone());
                }

                if !had_threads {
                    self.start_thread_poll_timer();
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::RemoveThread { thread_id } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.threads.remove(thread_id);
                }

                let has_threads = self.get_layout_window()
                    .map(|lw| !lw.threads.is_empty()).unwrap_or(false);

                if !has_threads {
                    self.stop_thread_poll_timer();
                }
                ProcessEventResult::DoNothing
            }

            // === Content Modifications ===

            CallbackChange::ChangeNodeText { node_id, text } => {
                let dom_id = node_id.dom;
                let internal_node_id = match node_id.node.into_crate_internal() {
                    Some(id) => id,
                    None => return ProcessEventResult::DoNothing,
                };

                // Update StyledDom text content
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(layout_result) = lw.layout_results.get_mut(&dom_id) {
                        let idx = internal_node_id.index();
                        if idx < layout_result.styled_dom.node_data.as_ref().len() {
                            layout_result.styled_dom.node_data.as_container_mut()[internal_node_id]
                                .set_node_type(azul_core::dom::NodeType::Text(text.clone()));
                        }
                    }
                }
                ProcessEventResult::ShouldIncrementalRelayout
            }

            CallbackChange::ChangeNodeImage { dom_id, node_id, image, update_type: _ } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(layout_result) = lw.layout_results.get_mut(dom_id) {
                        let idx = node_id.index();
                        if idx < layout_result.styled_dom.node_data.as_ref().len() {
                            layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                                .set_node_type(azul_core::dom::NodeType::Image(image.clone()));
                        }
                    }
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::UpdateImageCallback { dom_id: _, node_id: _ } => {
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::UpdateAllImageCallbacks => {
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::UpdateVirtualizedView { dom_id, node_id } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    let mut updates = BTreeMap::new();
                    let mut set = FastBTreeSet::new();
                    set.insert(*node_id);
                    updates.insert(*dom_id, set);
                    lw.queue_virtualized_view_updates(updates);
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::ChangeNodeImageMask { dom_id, node_id, mask } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(layout_result) = lw.layout_results.get_mut(dom_id) {
                        let idx = node_id.index();
                        if idx < layout_result.styled_dom.node_data.as_ref().len() {
                            layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                                .set_clip_mask(mask.clone());
                        }
                    }
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::ChangeNodeCssProperties { dom_id, node_id, properties } => {
                // Update StyledDom CSS properties
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(layout_result) = lw.layout_results.get_mut(dom_id) {
                        let idx = node_id.index();
                        if idx < layout_result.styled_dom.node_data.as_ref().len() {
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
                ProcessEventResult::ShouldIncrementalRelayout
            }

            // === Scroll ===

            CallbackChange::ScrollTo { dom_id, node_id, position } => {
                let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
                let now = (external.get_system_time_fn.cb)();

                let mut needs_virtualized_view_update = false;

                if let Some(internal_node_id) = node_id.into_crate_internal() {
                    if let Some(lw) = self.get_layout_window_mut() {
                        lw.scroll_manager.scroll_to(
                            *dom_id, internal_node_id, *position,
                            std::time::Duration::from_millis(0).into(),
                            azul_core::events::EasingFunction::Linear,
                            now.clone().into(),
                        );

                        // Check if this scroll node is a VirtualizedView that needs
                        // re-invocation (e.g. user scrolled near edge for lazy loading).
                        // If so, queue it for processing in the next render pass.
                        needs_virtualized_view_update = lw.check_and_queue_virtualized_view_reinvoke(
                            *dom_id, internal_node_id,
                        );
                    }
                }

                if needs_virtualized_view_update {
                    // VirtualizedView needs new content — force display list rebuild
                    ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
                } else {
                    // Normal scroll — lightweight repaint (scroll offsets only)
                    ProcessEventResult::ShouldReRenderCurrentWindow
                }
            }

            CallbackChange::ScrollIntoView { node_id, options } => {
                let now = azul_core::task::Instant::now();
                if let Some(lw) = self.get_layout_window_mut() {
                    azul_layout::managers::scroll_into_view::scroll_node_into_view(
                        *node_id, &lw.layout_results, &mut lw.scroll_manager,
                        options.clone(), now,
                    );
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::ScrollActiveCursorIntoView => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.scroll_selection_into_view(
                        azul_layout::window::SelectionScrollType::Cursor,
                        azul_layout::window::ScrollMode::Instant,
                    );
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            // === Image/Font Cache ===

            CallbackChange::AddImageToCache { id, image } => {
                self.get_image_cache_mut().add_css_image_id(id.clone(), image.clone());
                ProcessEventResult::DoNothing
            }

            CallbackChange::RemoveImageFromCache { id } => {
                self.get_image_cache_mut().delete_css_image_id(id);
                ProcessEventResult::DoNothing
            }

            CallbackChange::ReloadSystemFonts => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.font_manager.fc_cache = FcFontCache::build().into();
                }
                ProcessEventResult::DoNothing
            }

            // === Menu / Tooltip ===

            CallbackChange::OpenMenu { menu, position } => {
                let pos = position.unwrap_or(LogicalPosition::new(0.0, 0.0));
                self.show_menu_from_callback(menu, pos);
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::ShowTooltip { text, position } => {
                self.show_tooltip_from_callback(text.as_str(), *position);
                ProcessEventResult::DoNothing
            }

            CallbackChange::HideTooltip => {
                self.hide_tooltip_from_callback();
                ProcessEventResult::DoNothing
            }

            // === Text Editing ===

            CallbackChange::InsertText { dom_id, node_id, text } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(*node_id));
                    let dom_node_id = azul_core::dom::DomNodeId { dom: *dom_id, node: hierarchy_id };
                    let old_inline_content = lw.get_text_before_textinput(*dom_id, *node_id);
                    let old_text = lw.extract_text_from_inline_content(&old_inline_content);
                    use azul_layout::managers::text_input::TextInputSource;
                    lw.text_input_manager.record_input(
                        dom_node_id, text.to_string(), old_text, TextInputSource::Programmatic,
                    );
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::DeleteBackward { dom_id, node_id } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(cursor) = lw.cursor_manager.get_cursor().cloned() {
                        let content = lw.get_text_before_textinput(*dom_id, *node_id);
                        use azul_layout::text3::edit::delete_backward;
                        let mut new_content = content.clone();
                        let (updated_content, new_cursor) = delete_backward(&mut new_content, &cursor);
                        lw.cursor_manager.move_cursor_to(new_cursor, *dom_id, *node_id);
                        lw.update_text_cache_after_edit(*dom_id, *node_id, updated_content);
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::DeleteForward { dom_id, node_id } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(cursor) = lw.cursor_manager.get_cursor().cloned() {
                        let content = lw.get_text_before_textinput(*dom_id, *node_id);
                        use azul_layout::text3::edit::delete_forward;
                        let mut new_content = content.clone();
                        let (updated_content, new_cursor) = delete_forward(&mut new_content, &cursor);
                        lw.cursor_manager.move_cursor_to(new_cursor, *dom_id, *node_id);
                        lw.update_text_cache_after_edit(*dom_id, *node_id, updated_content);
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::MoveCursor { dom_id, node_id, cursor } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.cursor_manager.move_cursor_to(*cursor, *dom_id, *node_id);
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::SetSelection { dom_id, node_id, selection } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    match selection {
                        azul_core::selection::Selection::Cursor(cursor) => {
                            lw.cursor_manager.move_cursor_to(*cursor, *dom_id, *node_id);
                            lw.selection_manager.clear_all();
                        }
                        azul_core::selection::Selection::Range(range) => {
                            lw.cursor_manager.move_cursor_to(range.start, *dom_id, *node_id);
                            let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(*node_id));
                            let dom_node_id = azul_core::dom::DomNodeId { dom: *dom_id, node: hierarchy_id };
                            lw.selection_manager.add_selection(*dom_id, dom_node_id, selection.clone());
                        }
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::SetTextChangeset { changeset } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.text_input_manager.pending_changeset = Some(changeset.clone());
                }
                ProcessEventResult::DoNothing
            }

            // === Cursor Movement ===

            CallbackChange::MoveCursorLeft { dom_id, node_id, extend_selection } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(new_cursor) = lw.move_cursor_in_node(*dom_id, *node_id, |layout, cursor| {
                        layout.move_cursor_left(*cursor, &mut None)
                    }) {
                        lw.handle_cursor_movement(*dom_id, *node_id, new_cursor, *extend_selection);
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::MoveCursorRight { dom_id, node_id, extend_selection } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(new_cursor) = lw.move_cursor_in_node(*dom_id, *node_id, |layout, cursor| {
                        layout.move_cursor_right(*cursor, &mut None)
                    }) {
                        lw.handle_cursor_movement(*dom_id, *node_id, new_cursor, *extend_selection);
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::MoveCursorUp { dom_id, node_id, extend_selection } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(new_cursor) = lw.move_cursor_in_node(*dom_id, *node_id, |layout, cursor| {
                        layout.move_cursor_up(*cursor, &mut None, &mut None)
                    }) {
                        lw.handle_cursor_movement(*dom_id, *node_id, new_cursor, *extend_selection);
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::MoveCursorDown { dom_id, node_id, extend_selection } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(new_cursor) = lw.move_cursor_in_node(*dom_id, *node_id, |layout, cursor| {
                        layout.move_cursor_down(*cursor, &mut None, &mut None)
                    }) {
                        lw.handle_cursor_movement(*dom_id, *node_id, new_cursor, *extend_selection);
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::MoveCursorToLineStart { dom_id, node_id, extend_selection } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(new_cursor) = lw.move_cursor_in_node(*dom_id, *node_id, |layout, cursor| {
                        layout.move_cursor_to_line_start(*cursor, &mut None)
                    }) {
                        lw.handle_cursor_movement(*dom_id, *node_id, new_cursor, *extend_selection);
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::MoveCursorToLineEnd { dom_id, node_id, extend_selection } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(new_cursor) = lw.move_cursor_in_node(*dom_id, *node_id, |layout, cursor| {
                        layout.move_cursor_to_line_end(*cursor, &mut None)
                    }) {
                        lw.handle_cursor_movement(*dom_id, *node_id, new_cursor, *extend_selection);
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::MoveCursorToDocumentStart { dom_id, node_id, extend_selection } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(layout) = lw.get_inline_layout_for_node(*dom_id, *node_id) {
                        if let Some(first_cluster) = layout.items.first().and_then(|item| item.item.as_cluster()) {
                            let doc_start = azul_core::selection::TextCursor {
                                cluster_id: first_cluster.source_cluster_id,
                                affinity: azul_core::selection::CursorAffinity::Leading,
                            };
                            lw.handle_cursor_movement(*dom_id, *node_id, doc_start, *extend_selection);
                        }
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::MoveCursorToDocumentEnd { dom_id, node_id, extend_selection } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(layout) = lw.get_inline_layout_for_node(*dom_id, *node_id) {
                        if let Some(last_cluster) = layout.items.last().and_then(|item| item.item.as_cluster()) {
                            let doc_end = azul_core::selection::TextCursor {
                                cluster_id: last_cluster.source_cluster_id,
                                affinity: azul_core::selection::CursorAffinity::Trailing,
                            };
                            lw.handle_cursor_movement(*dom_id, *node_id, doc_end, *extend_selection);
                        }
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            // === Clipboard ===

            CallbackChange::SetCopyContent { target: _, content } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.clipboard_manager.set_copy_content(content.clone());
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::SetCutContent { target: _, content } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.clipboard_manager.set_copy_content(content.clone());
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::SetSelectAllRange { target, range } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.selection_manager.set_range(target.dom, *target, range.clone());
                }
                ProcessEventResult::DoNothing
            }

            // === Debug / Hit Test ===

            CallbackChange::RequestHitTestUpdate { position } => {
                self.update_hit_test_at(*position);
                ProcessEventResult::DoNothing
            }

            CallbackChange::ProcessTextSelectionClick { position, time_ms } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.process_mouse_click_for_selection(*position, *time_ms);
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            // === Cursor Blink ===

            CallbackChange::SetCursorVisibility { visible: _ } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    let now = azul_core::task::Instant::now();
                    if lw.cursor_manager.should_blink(&now) {
                        lw.cursor_manager.toggle_visibility();
                    } else {
                        lw.cursor_manager.set_visibility(true);
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::ResetCursorBlink => {
                if let Some(lw) = self.get_layout_window_mut() {
                    let now = azul_core::task::Instant::now();
                    lw.cursor_manager.reset_blink_on_input(now);
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::StartCursorBlinkTimer => {
                let timer = if let Some(lw) = self.get_layout_window_mut() {
                    if lw.cursor_manager.is_blink_timer_active() {
                        None
                    } else {
                        lw.cursor_manager.set_blink_timer_active(true);
                        let ws = lw.current_window_state.clone();
                        Some(lw.create_cursor_blink_timer(&ws))
                    }
                } else {
                    None
                };

                if let Some(timer) = timer {
                    if let Some(lw) = self.get_layout_window_mut() {
                        lw.timers.insert(azul_core::task::CURSOR_BLINK_TIMER_ID, timer.clone());
                    }
                    self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::StopCursorBlinkTimer => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if lw.cursor_manager.is_blink_timer_active() {
                        lw.cursor_manager.set_blink_timer_active(false);
                        lw.timers.remove(&azul_core::task::CURSOR_BLINK_TIMER_ID);
                    }
                }
                self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                ProcessEventResult::DoNothing
            }

            // === Text Input ===

            CallbackChange::CreateTextInput { text } => {
                // Process text input
                let affected_nodes = if let Some(lw) = self.get_layout_window_mut() {
                    lw.process_text_input(text.as_str())
                } else {
                    BTreeMap::new()
                };

                if affected_nodes.is_empty() {
                    return ProcessEventResult::DoNothing;
                }

                // Build and dispatch synthetic text events
                let now = {
                    #[cfg(feature = "std")]
                    { azul_core::task::Instant::from(std::time::Instant::now()) }
                    #[cfg(not(feature = "std"))]
                    { azul_core::task::Instant::Tick(azul_core::task::SystemTick::new(0)) }
                };

                let text_events: Vec<_> = affected_nodes.keys().map(|dom_node_id| {
                    azul_core::events::SyntheticEvent::new(
                        azul_core::events::EventType::Input,
                        azul_core::events::EventSource::User,
                        *dom_node_id,
                        now.clone(),
                        azul_core::events::EventData::None,
                    )
                }).collect();

                let mut result = ProcessEventResult::DoNothing;

                if !text_events.is_empty() {
                    let (text_changes_result, text_update, _) = self.dispatch_events_propagated(&text_events);
                    result = result.max(text_changes_result);
                    if matches!(text_update, Update::RefreshDom | Update::RefreshDomAllWindows) {
                        result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
                    }
                }

                // Apply text changeset
                if let Some(lw) = self.get_layout_window_mut() {
                    let dirty_nodes = lw.apply_text_changeset();
                    if !dirty_nodes.is_empty() {
                        result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                        lw.scroll_selection_into_view(
                            azul_layout::window::SelectionScrollType::Cursor,
                            azul_layout::window::ScrollMode::Instant,
                        );
                    }
                }

                result
            }

            // === Window Move ===

            CallbackChange::BeginInteractiveMove => {
                self.handle_begin_interactive_move();
                ProcessEventResult::DoNothing
            }

            // === Drag & Drop ===

            CallbackChange::SetDragData { mime_type, data } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(ctx) = lw.gesture_drag_manager.get_drag_context_mut() {
                        if let Some(node_drag) = ctx.as_node_drag_mut() {
                            node_drag.drag_data.set_data(mime_type.clone(), data.clone());
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::AcceptDrop => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(ctx) = lw.gesture_drag_manager.get_drag_context_mut() {
                        if let Some(node_drag) = ctx.as_node_drag_mut() {
                            node_drag.drop_accepted = true;
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::SetDropEffect { effect } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(ctx) = lw.gesture_drag_manager.get_drag_context_mut() {
                        if let Some(node_drag) = ctx.as_node_drag_mut() {
                            node_drag.drop_effect = *effect;
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }

            // === DOM Mutation (Debug API) ===

            CallbackChange::InsertChildNode {
                dom_id, parent_node_id, node_type_str, position, classes, id,
            } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(layout_result) = lw.layout_results.get_mut(dom_id) {
                        let parent_idx = parent_node_id.index();
                        if parent_idx < layout_result.styled_dom.node_data.as_ref().len() {
                            // Parse node_type_str into a NodeType
                            let node_type = parse_node_type_from_str(node_type_str.as_str());

                            // Build a Dom with the correct node type
                            let mut dom = azul_core::dom::Dom::create_node(node_type);

                            // Set classes and ID on the root
                            if let Some(id_str) = id {
                                dom = dom.with_id(id_str.clone());
                            }
                            for class in classes.iter() {
                                dom = dom.with_class(class.clone());
                            }

                            // Style it (empty CSS = no styles, just creates StyledDom)
                            let css = azul_css::css::Css::empty();
                            let styled = azul_core::styled_dom::StyledDom::create(&mut dom, css);

                            // Append to the parent
                            match position {
                                Some(pos) => layout_result.styled_dom.append_child_with_index(styled, *pos),
                                None => layout_result.styled_dom.append_child(styled),
                            }
                        }
                    }
                }
                ProcessEventResult::ShouldIncrementalRelayout
            }

            CallbackChange::DeleteNode { dom_id, node_id } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(layout_result) = lw.layout_results.get_mut(dom_id) {
                        let idx = node_id.index();
                        let node_count = layout_result.styled_dom.node_data.as_ref().len();
                        if idx < node_count && idx != 0 {
                            // Tombstone: set node to empty Div and unlink from hierarchy
                            layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                                .set_node_type(azul_core::dom::NodeType::Div);
                            layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                                .set_ids_and_classes(Vec::new().into());
                            layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                                .set_callbacks(Vec::new().into());

                            // Unlink from hierarchy: connect prev sibling to next sibling
                            let hierarchy = &mut layout_result.styled_dom.node_hierarchy;
                            let prev_sib = hierarchy.as_container()[*node_id].previous_sibling_id();
                            let next_sib = hierarchy.as_container()[*node_id].next_sibling_id();
                            let parent = hierarchy.as_container()[*node_id].parent_id();

                            // Connect prev → next
                            if let Some(prev) = prev_sib {
                                hierarchy.as_container_mut()[prev].next_sibling =
                                    azul_core::id::NodeId::into_raw(&next_sib);
                            } else if let Some(p) = parent {
                                // This node was the first child — update parent's first-child
                                // (last_child field actually stores first_child pointer in
                                //  sibling-based encoding... we need to handle this via
                                //  just tombstoning the hierarchy entry)
                            }
                            if let Some(next) = next_sib {
                                hierarchy.as_container_mut()[next].previous_sibling =
                                    azul_core::id::NodeId::into_raw(&prev_sib);
                            } else if let Some(p) = parent {
                                // This node was the last child — update parent's last_child
                                hierarchy.as_container_mut()[p].last_child =
                                    azul_core::id::NodeId::into_raw(&prev_sib);
                            }

                            // Zero out the deleted node's hierarchy pointers
                            hierarchy.as_container_mut()[*node_id].parent = 0;
                            hierarchy.as_container_mut()[*node_id].previous_sibling = 0;
                            hierarchy.as_container_mut()[*node_id].next_sibling = 0;
                            hierarchy.as_container_mut()[*node_id].last_child = 0;
                        }
                    }
                }
                ProcessEventResult::ShouldIncrementalRelayout
            }

            CallbackChange::SetNodeIdsAndClasses { dom_id, node_id, ids_and_classes } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(layout_result) = lw.layout_results.get_mut(dom_id) {
                        let idx = node_id.index();
                        if idx < layout_result.styled_dom.node_data.as_ref().len() {
                            layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                                .set_ids_and_classes(ids_and_classes.clone());
                        }
                    }
                }
                ProcessEventResult::ShouldIncrementalRelayout
            }
        }
    }

    // PROVIDED: Exhaustive System Change Processing (Cross-Platform)

    /// Process a single framework-determined system change.
    ///
    /// This is the SINGLE place where all `SystemChange` variants are handled.
    /// Adding a new variant causes a compile error here — no silent bugs.
    ///
    /// Returns the `ProcessEventResult` indicating what level of re-render is needed.
    fn apply_system_change(
        &mut self,
        change: &SystemChange,
    ) -> ProcessEventResult {

        match change {
            // === Text Selection ===

            SystemChange::TextSelectionClick { position, timestamp } => {
                let external = ExternalSystemCallbacks::rust_internal();
                let current_instant = (external.get_system_time_fn.cb)();
                let duration_since_event = current_instant.duration_since(timestamp);
                let current_time_ms = match duration_since_event {
                    azul_core::task::Duration::System(d) => {
                        #[cfg(feature = "std")]
                        { let std_duration: std::time::Duration = d.into(); std_duration.as_millis() as u64 }
                        #[cfg(not(feature = "std"))]
                        { 0u64 }
                    }
                    azul_core::task::Duration::Tick(t) => t.tick_diff as u64,
                };
                if let Some(layout_window) = self.get_layout_window_mut() {
                    if layout_window.process_mouse_click_for_selection(*position, current_time_ms).is_some() {
                        return ProcessEventResult::ShouldReRenderCurrentWindow;
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::TextSelectionDrag { start_position, current_position } => {
                // Suppress text selection if a node drag is active
                let node_dragging = self.get_layout_window()
                    .map(|lw| lw.gesture_drag_manager.is_node_dragging_any())
                    .unwrap_or(false);
                if node_dragging {
                    return ProcessEventResult::DoNothing;
                }
                if let Some(layout_window) = self.get_layout_window_mut() {
                    if layout_window.process_mouse_drag_for_selection(*start_position, *current_position).is_some() {
                        return ProcessEventResult::ShouldReRenderCurrentWindow;
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::DeleteTextSelection { target, forward } => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    if layout_window.delete_selection(*target, *forward).is_some() {
                        return ProcessEventResult::ShouldReRenderCurrentWindow;
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::ArrowKeyNavigation { target, direction, extend_selection, word_jump } => {
                use azul_core::events::ArrowDirection;

                let node_id = match target.node.into_crate_internal() {
                    Some(id) => id,
                    None => return ProcessEventResult::DoNothing,
                };
                let dom_id = target.dom;

                if let Some(layout_window) = self.get_layout_window_mut() {
                    let new_cursor = match direction {
                        ArrowDirection::Left if *word_jump => {
                            layout_window.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                                layout.move_cursor_to_prev_word(*cursor, &mut None)
                            })
                        }
                        ArrowDirection::Right if *word_jump => {
                            layout_window.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                                layout.move_cursor_to_next_word(*cursor, &mut None)
                            })
                        }
                        ArrowDirection::Left => {
                            layout_window.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                                layout.move_cursor_left(*cursor, &mut None)
                            })
                        }
                        ArrowDirection::Right => {
                            layout_window.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                                layout.move_cursor_right(*cursor, &mut None)
                            })
                        }
                        ArrowDirection::Up => {
                            layout_window.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                                layout.move_cursor_up(*cursor, &mut None, &mut None)
                            })
                        }
                        ArrowDirection::Down => {
                            layout_window.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                                layout.move_cursor_down(*cursor, &mut None, &mut None)
                            })
                        }
                        ArrowDirection::LineStart => {
                            layout_window.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                                layout.move_cursor_to_line_start(*cursor, &mut None)
                            })
                        }
                        ArrowDirection::LineEnd => {
                            layout_window.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                                layout.move_cursor_to_line_end(*cursor, &mut None)
                            })
                        }
                        ArrowDirection::DocumentStart => {
                            layout_window.get_inline_layout_for_node(dom_id, node_id)
                                .and_then(|layout| layout.get_first_cluster_cursor())
                        }
                        ArrowDirection::DocumentEnd => {
                            layout_window.get_inline_layout_for_node(dom_id, node_id)
                                .and_then(|layout| layout.get_last_cluster_cursor())
                        }
                    };

                    if let Some(new_cursor) = new_cursor {
                        layout_window.handle_cursor_movement(dom_id, node_id, new_cursor, *extend_selection);
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            // === Keyboard Shortcuts ===

            SystemChange::CopyToClipboard => {
                if let Some(layout_window) = self.get_layout_window() {
                    let dom_id = azul_core::dom::DomId { inner: 0 };
                    if let Some(clipboard_content) = layout_window.get_selected_content_for_clipboard(&dom_id) {
                        set_system_clipboard(clipboard_content.plain_text.as_str().to_string());
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::CutToClipboard { target } => {
                let mut affected = false;
                if let Some(layout_window) = self.get_layout_window_mut() {
                    let dom_id = azul_core::dom::DomId { inner: 0 };
                    if let Some(clipboard_content) = layout_window.get_selected_content_for_clipboard(&dom_id) {
                        if set_system_clipboard(clipboard_content.plain_text.as_str().to_string()) {
                            if layout_window.delete_selection(*target, false).is_some() {
                                affected = true;
                            }
                        }
                    }
                }
                if affected { ProcessEventResult::ShouldReRenderCurrentWindow } else { ProcessEventResult::DoNothing }
            }

            SystemChange::PasteFromClipboard => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    if let Some(clipboard_text) = get_system_clipboard() {
                        let affected = layout_window.process_text_input(&clipboard_text);
                        if !affected.is_empty() {
                            return ProcessEventResult::ShouldReRenderCurrentWindow;
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::SelectAllText => {
                // Select all text in the focused contenteditable node
                if let Some(layout_window) = self.get_layout_window_mut() {
                    if let Some(focused_node) = layout_window.focus_manager.focused_node {
                        let dom_id = focused_node.dom;
                        if let Some(node_id) = focused_node.node.into_crate_internal() {
                            let range = layout_window.get_inline_layout_for_node(dom_id, node_id)
                                .and_then(|layout| {
                                    let start = layout.get_first_cluster_cursor()?;
                                    let end = layout.get_last_cluster_cursor()?;
                                    Some(azul_core::selection::SelectionRange { start, end })
                                });

                            if let Some(range) = range {
                                layout_window.selection_manager.set_range(dom_id, focused_node, range);
                                // Move cursor to end of selection
                                if let Some(layout) = layout_window.get_inline_layout_for_node(dom_id, node_id) {
                                    if let Some(end_cursor) = layout.get_last_cluster_cursor() {
                                        layout_window.cursor_manager.move_cursor_to(end_cursor, dom_id, node_id);
                                    }
                                }
                                return ProcessEventResult::ShouldReRenderCurrentWindow;
                            }
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::UndoTextEdit { target } => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    let node_id = match target.node.into_crate_internal() {
                        Some(id) => id,
                        None => return ProcessEventResult::DoNothing,
                    };
                    let external = ExternalSystemCallbacks::rust_internal();
                    let timestamp = (external.get_system_time_fn.cb)().into();

                    if let Some(operation) = layout_window.undo_redo_manager.pop_undo(node_id) {
                        use azul_layout::managers::undo_redo::create_revert_changeset;
                        let _revert_changeset = create_revert_changeset(&operation, timestamp);

                        let node_id_internal = target.node.into_crate_internal();
                        if let Some(node_id_internal) = node_id_internal {
                            use std::sync::Arc;
                            use azul_layout::text3::cache::{InlineContent, StyleProperties, StyledRun};

                            let new_content = vec![InlineContent::Text(StyledRun {
                                text: operation.pre_state.text_content.as_str().to_string(),
                                style: Arc::new(StyleProperties::default()),
                                logical_start_byte: 0,
                                source_node_id: None,
                            })];

                            layout_window.update_text_cache_after_edit(
                                target.dom, node_id_internal, new_content,
                            );

                            if let Some(cursor) = operation.pre_state.cursor_position.into_option() {
                                layout_window.cursor_manager.move_cursor_to(
                                    cursor, target.dom, node_id_internal,
                                );
                            }
                        }

                        layout_window.undo_redo_manager.push_redo(operation);
                        return ProcessEventResult::ShouldReRenderCurrentWindow;
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::RedoTextEdit { target } => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    let node_id = match target.node.into_crate_internal() {
                        Some(id) => id,
                        None => return ProcessEventResult::DoNothing,
                    };

                    if let Some(operation) = layout_window.undo_redo_manager.pop_redo(node_id) {
                        let node_id_internal = target.node.into_crate_internal();
                        if let Some(_node_id_internal) = node_id_internal {
                            use azul_layout::managers::changeset::TextOperation;
                            match &operation.changeset.operation {
                                TextOperation::InsertText(op) => {
                                    let _affected = layout_window.process_text_input(&op.text);
                                }
                                _ => {}
                            }
                        }
                        layout_window.undo_redo_manager.push_undo(operation);
                        return ProcessEventResult::ShouldReRenderCurrentWindow;
                    }
                }
                ProcessEventResult::DoNothing
            }

            // === Text Input ===

            SystemChange::ApplyPendingTextInput => {
                // Text input was already applied during pre-callback processing.
                // This variant exists for the post-callback filter to signal
                // that text changeset should be applied if not prevented.
                ProcessEventResult::DoNothing
            }

            SystemChange::ApplyTextChangeset => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    let dirty_nodes = layout_window.apply_text_changeset();
                    if !dirty_nodes.is_empty() {
                        return ProcessEventResult::ShouldReRenderCurrentWindow;
                    }
                }
                ProcessEventResult::DoNothing
            }

            // === Drag & Drop ===

            SystemChange::ActivateNodeDrag { dom_id, node_id } => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    let drag_data = azul_core::drag::DragData::new();
                    layout_window.gesture_drag_manager.activate_node_drag(
                        *dom_id, *node_id, drag_data, None,
                    );
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            SystemChange::ActivateWindowDrag => {
                let win_pos = self.get_current_window_state().position.clone();
                if let Some(layout_window) = self.get_layout_window_mut() {
                    layout_window.gesture_drag_manager.activate_window_drag(win_pos, None);
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::InitDragVisualState => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    // Sync DragDropManager from GestureAndDragManager
                    if let Some(ctx) = layout_window.gesture_drag_manager.get_drag_context() {
                        layout_window.drag_drop_manager.active_drag = Some(ctx.clone());
                    }

                    // Set :dragging pseudo-state and add GPU transform key
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
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            SystemChange::SetDragOverState { target, active } => {
                if let Some(target_node_id) = target.node.into_crate_internal() {
                    if let Some(layout_window) = self.get_layout_window_mut() {
                        if let Some(layout_result) = layout_window.layout_results.get_mut(&target.dom) {
                            let mut styled_nodes = layout_result.styled_dom.styled_nodes.as_container_mut();
                            if let Some(styled_node) = styled_nodes.get_mut(target_node_id) {
                                styled_node.styled_node_state.drag_over = *active;
                                return ProcessEventResult::ShouldReRenderCurrentWindow;
                            }
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::UpdateDropTarget { target } => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    if let Some(ctx) = layout_window.gesture_drag_manager.get_drag_context_mut() {
                        if let Some(node_drag) = ctx.as_node_drag_mut() {
                            node_drag.previous_drop_target = node_drag.current_drop_target.clone();
                            node_drag.current_drop_target = azul_core::dom::OptionDomNodeId::Some(target.clone());
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::UpdateDragGpuTransform => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    if let Some(ctx) = layout_window.gesture_drag_manager.get_drag_context() {
                        if let Some(node_drag) = ctx.as_node_drag() {
                            let dom_id = node_drag.dom_id;
                            let node_id = node_drag.node_id;
                            let delta_x = ctx.current_position().x - ctx.start_position().x;
                            let delta_y = ctx.current_position().y - ctx.start_position().y;
                            let gpu_cache = layout_window.gpu_state_manager.get_or_create_cache(dom_id);
                            let new_transform = azul_core::transform::ComputedTransform3D::new_translation(
                                delta_x, delta_y, 0.0,
                            );
                            gpu_cache.current_transform_values.insert(node_id, new_transform);
                        }
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            SystemChange::DeactivateDrag => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    // Clear :dragging pseudo-state
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

                    // Clear :drag-over on current drop target
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

                    // Remove GPU transform key
                    if let Some(ctx) = layout_window.gesture_drag_manager.get_drag_context() {
                        if let Some(node_drag) = ctx.as_node_drag() {
                            let dom_id = node_drag.dom_id;
                            let node_id = node_drag.node_id;
                            let gpu_cache = layout_window.gpu_state_manager.get_or_create_cache(dom_id);
                            gpu_cache.transform_keys.remove(&node_id);
                            gpu_cache.current_transform_values.remove(&node_id);
                        }
                    }

                    // End drag session
                    if layout_window.gesture_drag_manager.is_dragging() {
                        layout_window.gesture_drag_manager.end_drag();
                    }
                    layout_window.drag_drop_manager.active_drag = None;
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            // === Focus ===

            SystemChange::SetFocus { new_focus, old_focus } => {
                let old_focus_node_id = old_focus.and_then(|f| f.node.into_crate_internal());
                let new_focus_node_id = new_focus.and_then(|f| f.node.into_crate_internal());

                let timer_action = if let Some(layout_window) = self.get_layout_window_mut() {
                    layout_window.focus_manager.set_focused_node(*new_focus);

                    // Scroll newly focused node into view
                    if let Some(focus_node) = new_focus {
                        use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;
                        let now = azul_core::task::Instant::now();
                        layout_window.scroll_node_into_view(
                            *focus_node, ScrollIntoViewOptions::nearest(), now,
                        );
                    }

                    // Handle cursor blink timer
                    let window_state = layout_window.current_window_state.clone();
                    let timer_action = layout_window.handle_focus_change_for_cursor_blink(
                        *new_focus, &window_state,
                    );

                    // Apply CSS restyle (:focus pseudo-class)
                    if old_focus_node_id != new_focus_node_id {
                        let _restyle_result = apply_focus_restyle(
                            layout_window, old_focus_node_id, new_focus_node_id,
                        );
                    }

                    Some(timer_action)
                } else {
                    None
                };

                // Apply timer action outside layout_window borrow
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

                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            SystemChange::ClearAllSelections => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    layout_window.selection_manager.clear_all();
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::FinalizePendingFocusChanges => {
                let timer_creation_needed = if let Some(layout_window) = self.get_layout_window_mut() {
                    let needs_init = layout_window.focus_manager.needs_cursor_initialization();
                    if needs_init {
                        let cursor_initialized = layout_window.finalize_pending_focus_changes();
                        if cursor_initialized {
                            if !layout_window.cursor_manager.is_blink_timer_active() {
                                layout_window.cursor_manager.set_blink_timer_active(true);
                                true
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

                if timer_creation_needed {
                    let timer = if let Some(layout_window) = self.get_layout_window() {
                        let current_window_state = self.get_current_window_state();
                        Some(layout_window.create_cursor_blink_timer(current_window_state))
                    } else {
                        None
                    };
                    if let Some(timer) = timer {
                        self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                    }
                    return ProcessEventResult::ShouldReRenderCurrentWindow;
                }
                ProcessEventResult::DoNothing
            }

            // === Scroll ===

            SystemChange::ScrollSelectionIntoView => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    use azul_layout::window::{ScrollMode, SelectionScrollType};

                    let scroll_type = if let Some(focused_node) = layout_window.focus_manager.focused_node {
                        if layout_window.selection_manager.get_selection(&focused_node.dom).is_some() {
                            SelectionScrollType::Selection
                        } else {
                            SelectionScrollType::Cursor
                        }
                    } else {
                        return ProcessEventResult::DoNothing;
                    };

                    layout_window.scroll_selection_into_view(scroll_type, ScrollMode::Instant);
                    return ProcessEventResult::ShouldReRenderCurrentWindow;
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::ScrollNodeIntoView { target } => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;
                    let now = azul_core::task::Instant::now();
                    layout_window.scroll_node_into_view(*target, ScrollIntoViewOptions::nearest(), now);
                    return ProcessEventResult::ShouldReRenderCurrentWindow;
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::ScrollCursorIntoViewAfterTextInput => {
                if let Some(layout_window) = self.get_layout_window() {
                    if let Some(cursor_rect) = layout_window.get_focused_cursor_rect() {
                        if let Some(focused_node_id) = layout_window.focus_manager.focused_node {
                            if let Some(scroll_container) = layout_window.find_scrollable_ancestor(focused_node_id) {
                                let scroll_node_id = scroll_container.node.into_crate_internal();
                                if let Some(scroll_node_id) = scroll_node_id {
                                    if let Some(scroll_state) = layout_window.scroll_manager
                                        .get_scroll_state(scroll_container.dom, scroll_node_id) {
                                        if let Some(container_rect) = layout_window.get_node_layout_rect(scroll_container) {
                                            let visible_area = azul_core::geom::LogicalRect::new(
                                                azul_core::geom::LogicalPosition::new(
                                                    container_rect.origin.x + scroll_state.current_offset.x,
                                                    container_rect.origin.y + scroll_state.current_offset.y,
                                                ),
                                                container_rect.size,
                                            );

                                            const SCROLL_PADDING: f32 = 5.0;
                                            let mut scroll_delta = azul_core::geom::LogicalPosition::zero();

                                            if cursor_rect.origin.x < visible_area.origin.x + SCROLL_PADDING {
                                                scroll_delta.x = cursor_rect.origin.x - (visible_area.origin.x + SCROLL_PADDING);
                                            } else if cursor_rect.origin.x + cursor_rect.size.width > visible_area.origin.x + visible_area.size.width - SCROLL_PADDING {
                                                scroll_delta.x = (cursor_rect.origin.x + cursor_rect.size.width) - (visible_area.origin.x + visible_area.size.width - SCROLL_PADDING);
                                            }

                                            if cursor_rect.origin.y < visible_area.origin.y + SCROLL_PADDING {
                                                scroll_delta.y = cursor_rect.origin.y - (visible_area.origin.y + SCROLL_PADDING);
                                            } else if cursor_rect.origin.y + cursor_rect.size.height > visible_area.origin.y + visible_area.size.height - SCROLL_PADDING {
                                                scroll_delta.y = (cursor_rect.origin.y + cursor_rect.size.height) - (visible_area.origin.y + visible_area.size.height - SCROLL_PADDING);
                                            }

                                            if scroll_delta.x != 0.0 || scroll_delta.y != 0.0 {
                                                let external = ExternalSystemCallbacks::rust_internal();
                                                let now = (external.get_system_time_fn.cb)();

                                                if let Some(layout_window_mut) = self.get_layout_window_mut() {
                                                    layout_window_mut.scroll_manager.scroll_by(
                                                        scroll_container.dom, scroll_node_id, scroll_delta,
                                                        std::time::Duration::from_millis(0).into(),
                                                        azul_core::events::EasingFunction::Linear,
                                                        now.into(),
                                                    );
                                                    return ProcessEventResult::ShouldReRenderCurrentWindow;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }

            // === Auto-Scroll Timer ===

            SystemChange::StartAutoScrollTimer => {
                if let Some(layout_window) = self.get_layout_window() {
                    let timer_id = azul_core::task::DRAG_AUTOSCROLL_TIMER_ID;
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
                            RefAny::new(()),
                            auto_scroll_timer_callback as TimerCallbackType,
                            external.get_system_time_fn,
                        ).with_interval(AzulDuration::System(SystemTimeDiff {
                            secs: 0, nanos: frame_time_nanos,
                        }));

                        if let Some(layout_window) = self.get_layout_window_mut() {
                            layout_window.add_timer(timer_id, timer.clone());
                            self.start_timer(azul_core::task::DRAG_AUTOSCROLL_TIMER_ID.id, timer);
                            return ProcessEventResult::ShouldReRenderCurrentWindow;
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::StopAutoScrollTimer => {
                let timer_id = azul_core::task::DRAG_AUTOSCROLL_TIMER_ID;
                if let Some(layout_window) = self.get_layout_window_mut() {
                    if layout_window.timers.contains_key(&timer_id) {
                        layout_window.remove_timer(&timer_id);
                        self.stop_timer(azul_core::task::DRAG_AUTOSCROLL_TIMER_ID.id);
                    }
                }
                ProcessEventResult::DoNothing
            }
        }
    }

    // PROVIDED: Hit Testing (Cross-Platform Implementation)

    /// Update hit test at given position and store in hover manager.
    ///
    /// This method performs WebRender hit testing at the given logical position
    /// and updates the HoverManager with the results. This is needed for:
    /// - Normal mouse movement events (platform calls this)
    /// - Synthetic mouse events from debug API
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
    /// This replaces the old `invoke_callbacks()` method with proper W3C event propagation:
    /// - **HoverEventFilter**: Capture→Target→Bubble through DOM tree via `propagate_event()`
    /// - **FocusEventFilter**: Fires on focused node only (no propagation)
    /// - **WindowEventFilter**: Fires on ALL nodes with matching callback (brute-force)
    ///
    /// ## Arguments
    /// * `events` - SyntheticEvents to dispatch (already filtered to user events)
    ///
    /// ## Returns
    /// * `ProcessEventResult` - The maximum framework-determined processing level from applied changes
    /// * `Update` - The maximum update level requested by all invoked callbacks
    /// * `bool` - Whether any callback called preventDefault()
    fn dispatch_events_propagated(
        &mut self,
        events: &[azul_core::events::SyntheticEvent],
    ) -> (ProcessEventResult, azul_core::callbacks::Update, bool) {
        use azul_core::{
            callbacks::{CoreCallbackData, Update},
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
                None => return (ProcessEventResult::DoNothing, Update::DoNothing, false),
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
            return (ProcessEventResult::DoNothing, Update::DoNothing, false);
        }

        let mut borrows = self.prepare_callback_invocation();
        let mut all_updates: Vec<Update> = Vec::new();
        let mut all_changes: Vec<azul_layout::callbacks::CallbackChange> = Vec::new();
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
            let (changes, update) = borrows.layout_window.invoke_single_callback(
                &mut callback,
                &mut planned.callback_data.refany.clone(),
                &borrows.window_handle,
                borrows.gl_context_ptr,
                borrows.system_style.clone(),
                &ExternalSystemCallbacks::rust_internal(),
                borrows.previous_window_state,
                borrows.current_window_state,
                borrows.renderer_resources,
            );

            all_updates.push(update);

            // Check propagation control in the changes
            let mut should_stop_immediate = false;
            let mut should_stop_propagation = false;
            for change in &changes {
                use azul_layout::callbacks::CallbackChange;
                match change {
                    CallbackChange::PreventDefault => {
                        any_prevent_default = true;
                    }
                    CallbackChange::StopImmediatePropagation => {
                        should_stop_immediate = true;
                    }
                    CallbackChange::StopPropagation => {
                        should_stop_propagation = true;
                    }
                    _ => {}
                }
            }

            // Accumulate changes for later application
            all_changes.extend(changes);

            // stopPropagation: record that we should stop after remaining same-node handlers
            if should_stop_propagation && !propagation_stopped {
                propagation_stopped = true;
                propagation_stopped_node = Some((planned.dom_id, planned.node_id));
            }

            // stopImmediatePropagation: break immediately
            if should_stop_immediate {
                break;
            }
        }

        // Drop borrows before calling apply_user_change on self
        drop(borrows);

        // Apply all accumulated user changes, tracking max ProcessEventResult
        let mut changes_result = ProcessEventResult::DoNothing;
        for change in &all_changes {
            let r = self.apply_user_change(change);
            changes_result = changes_result.max(r);
        }

        // Compute the maximum update level across all callbacks
        let merged_update = all_updates.iter().copied().fold(
            Update::DoNothing,
            |acc, u| acc.max(u),
        );

        (changes_result, merged_update, any_prevent_default)
    }

    // PROVIDED: Complete Logic (Default Implementations)

    /// GPU-accelerated smooth scrolling.
    ///
    /// Updates the ScrollManager state with the scroll delta. Does NOT set
    /// `frame_needs_regeneration` — scrolling only requires a lightweight
    /// WebRender transaction (scroll offsets + GPU values), not a full layout
    /// regeneration or display list rebuild.
    ///
    /// Callers (`handle_scrollbar_click`, `handle_scrollbar_drag`) return
    /// `ShouldReRenderCurrentWindow` which triggers `request_redraw()`. The
    /// platform render function then sends a lightweight transaction via
    /// `build_image_only_transaction` (which includes `scroll_all_nodes`).
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

        // Apply scroll delta to ScrollManager
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

        // Recalculate scrollbar thumb positions after offset change
        layout_window.scroll_manager.calculate_scrollbar_states();

        // NOTE: We intentionally do NOT call mark_frame_needs_regeneration() here.
        // Scroll offset changes are frame-level operations in WebRender
        // (FrameMsg::SetScrollOffsets), not scene-level changes. The platform
        // render function will send scroll offsets via build_image_only_transaction
        // which calls scroll_all_nodes() + synchronize_gpu_values() +
        // txn.skip_scene_builder() + txn.generate_frame().
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

    /// Record accessibility action and return affected nodes.
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

    /// Process all window events using the state-diffing system.
    ///
    /// Main entry point for processing window events.
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
    fn process_window_events(&mut self, depth: usize) -> ProcessEventResult {

        if depth >= MAX_EVENT_RECURSION_DEPTH {
            log_warn!(
                super::debug_server::LogCategory::EventLoop,
                "[PlatformWindow] Max event recursion depth {} reached",
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
        // [ OK ] Text: Handled via CallbackChange::CreateTextInput / SystemChange::PasteFromClipboard
        // [ OK ] A11y: Tree updated after layout (rebuild_accessibility_tree); actions via record_accessibility_action()

        // NOTE: Text input is handled via:
        // - CallbackChange::CreateTextInput (debug server / user callbacks → apply_user_change)
        // - SystemChange::PasteFromClipboard (Ctrl+V → apply_system_change)
        // Platform IME text input (macOS NSTextInputClient, Windows WM_CHAR, etc.)
        // arrives as keyboard events and is processed through the above paths.
        //
        // Accessibility tree updates happen after layout in LayoutWindow::rebuild_accessibility_tree().
        // Screen reader actions are handled by PlatformWindow::record_accessibility_action().

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
                system_changes: Vec::new(),
                user_events: synthetic_events.clone(),
            }
        };

        // Track overall processing result
        let mut result = ProcessEventResult::DoNothing;

        // NOTE: VirtualizedView re-invocation for scroll edge detection is handled
        // transparently in the ScrollTo processing path (apply_user_change).

        // Get external callbacks for system time
        let external = ExternalSystemCallbacks::rust_internal();

        // Process pre-callback system changes (text selection, shortcuts) via apply_system_change
        for system_change in &pre_filter.system_changes {
            let r = self.apply_system_change(system_change);
            result = result.max(r);
        }

        // EVENT FILTERING AND CALLBACK DISPATCH (W3C Propagation Model)

        // Capture focus state before callbacks for post-callback filtering
        let old_focus = self
            .get_layout_window()
            .and_then(|lw| lw.focus_manager.get_focused_node().copied());

        // Dispatch user events using W3C Capture→Target→Bubble propagation
        // dispatch_events_propagated applies all CallbackChanges internally
        // via apply_user_change(), and returns the merged Update level.
        let (changes_result, callback_update, prevent_default) =
            self.dispatch_events_propagated(&pre_filter.user_events);
        result = result.max(changes_result);

        let mut should_recurse = false;

        use azul_core::callbacks::Update;
        match callback_update {
            Update::RefreshDom => {
                self.mark_frame_needs_regeneration();
                result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
                should_recurse = true;
            }
            Update::RefreshDomAllWindows => {
                self.mark_frame_needs_regeneration();
                result = result.max(ProcessEventResult::ShouldRegenerateDomAllWindows);
                should_recurse = true;
            }
            Update::DoNothing => {}
        }

        // POST-CALLBACK SYSTEM CHANGES
        // Detect drag, focus, and other post-callback changes, then process via apply_system_change

        let mut post_system_changes: Vec<SystemChange> = Vec::new();

        // AUTO-ACTIVATE NODE DRAG
        let had_drag_start = pre_filter.user_events.iter().any(|e| {
            matches!(e.event_type, azul_core::events::EventType::DragStart)
        });

        if had_drag_start {
            // Detect which drag activation to perform (pure analysis, no mutation)
            let drag_activation = if let Some(layout_window) = self.get_layout_window() {
                use azul_layout::managers::hover::InputPointId;
                let hit_test = layout_window.hover_manager
                    .get_current(&InputPointId::Mouse)
                    .cloned();

                if let Some(hit_test) = hit_test {
                    let mut found = None;
                    'outer: for (dom_id, hit_test_data) in &hit_test.hovered_nodes {
                        if let Some(layout_result) = layout_window.layout_results.get(dom_id) {
                            let node_data_container = layout_result.styled_dom.node_data.as_container();
                            let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();

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
                                let mut current = Some(*target_node_id);
                                while let Some(node_id) = current {
                                    if let Some(node_data) = node_data_container.get(node_id) {
                                        let is_draggable = node_data.attributes.as_ref().iter().any(|attr| {
                                            matches!(attr, azul_core::dom::AttributeType::Draggable(true))
                                        });
                                        if is_draggable {
                                            found = Some(SystemChange::ActivateNodeDrag {
                                                dom_id: *dom_id,
                                                node_id,
                                            });
                                            break 'outer;
                                        }
                                    }
                                    current = node_hierarchy.get(node_id).and_then(|h| h.parent_id());
                                }
                            }
                        }
                    }
                    found
                } else {
                    None
                }
            } else {
                None
            };

            match drag_activation {
                Some(change) => {
                    post_system_changes.push(change);
                    post_system_changes.push(SystemChange::InitDragVisualState);
                }
                None => {
                    post_system_changes.push(SystemChange::ActivateWindowDrag);
                }
            }
        }

        // SET :drag-over PSEUDO-STATE ON DragEnter / DragLeave TARGETS
        for event in &pre_filter.user_events {
            match event.event_type {
                azul_core::events::EventType::DragEnter => {
                    post_system_changes.push(SystemChange::SetDragOverState {
                        target: event.target.clone(), active: true,
                    });
                    post_system_changes.push(SystemChange::UpdateDropTarget {
                        target: event.target.clone(),
                    });
                }
                azul_core::events::EventType::DragLeave => {
                    post_system_changes.push(SystemChange::SetDragOverState {
                        target: event.target.clone(), active: false,
                    });
                }
                _ => {}
            }
        }

        // FORCE RE-RENDER DURING ACTIVE DRAG
        let is_node_dragging = self.get_layout_window()
            .map(|lw| lw.gesture_drag_manager.is_node_dragging_any())
            .unwrap_or(false);
        if is_node_dragging {
            post_system_changes.push(SystemChange::UpdateDragGpuTransform);
        }

        // AUTO-DEACTIVATE DRAG ON DRAG END
        let had_drag_end = pre_filter.user_events.iter().any(|e| {
            matches!(e.event_type, azul_core::events::EventType::DragEnd)
        });
        if had_drag_end {
            post_system_changes.push(SystemChange::DeactivateDrag);
        }

        // POST-CALLBACK INTERNAL EVENT FILTERING

        let new_focus = self
            .get_layout_window()
            .and_then(|lw| lw.focus_manager.get_focused_node().copied());

        let post_filter_changes = azul_core::events::post_callback_filter_system_changes(
            prevent_default,
            &pre_filter.system_changes,
            old_focus,
            new_focus,
        );
        post_system_changes.extend(post_filter_changes);

        // Detect if focus changed (for focus event dispatch later)
        let mut focus_changed = post_system_changes.iter().any(|c| matches!(c, SystemChange::SetFocus { .. }));

        // Apply all post-callback system changes via apply_system_change
        for system_change in &post_system_changes {
            let r = self.apply_system_change(system_change);
            result = result.max(r);
        }

        // POST-CALLBACK TEXT INPUT PROCESSING
        // ApplyPendingTextInput signals that text was entered (keyboard/IME).
        // When present, apply the text changeset and scroll cursor into view.
        let should_apply_text_input = post_system_changes.iter().any(|c| matches!(c, SystemChange::ApplyPendingTextInput));

        if should_apply_text_input {
            let r = self.apply_system_change(&SystemChange::ApplyTextChangeset);
            result = result.max(r);

            let r = self.apply_system_change(&SystemChange::ScrollCursorIntoViewAfterTextInput);
            result = result.max(r);
            if r >= ProcessEventResult::ShouldReRenderCurrentWindow {
                should_recurse = true;
            }
        }

        // MOUSE CLICK-TO-FOCUS (W3C default behavior)
        // Detect deepest focusable node under click, then set focus via SystemChange
        let mut mouse_click_focus_changed = false;
        if !prevent_default {
            let has_mouse_down = synthetic_events.iter().any(|e| {
                matches!(e.event_type, azul_core::events::EventType::MouseDown)
            });

            if has_mouse_down {
                // Pure detection: find deepest focusable node
                let clicked_focusable_node = if let Some(ref hit_test) = hit_test_for_dispatch {
                    let mut found: Option<azul_core::dom::DomNodeId> = None;
                    for (dom_id, hit_test_data) in &hit_test.hovered_nodes {
                        let deepest = hit_test_data.regular_hit_test_nodes
                            .iter()
                            .max_by_key(|(_, hit_item)| std::cmp::Reverse(hit_item.hit_depth));

                        if let Some((node_id, _)) = deepest {
                            if let Some(layout_window) = self.get_layout_window() {
                                if let Some(layout_result) = layout_window.layout_results.get(dom_id) {
                                    let node_data = layout_result.styled_dom.node_data.as_container();
                                    let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
                                    let mut current = Some(*node_id);
                                    while let Some(nid) = current {
                                        if let Some(nd) = node_data.get(nid) {
                                            if nd.is_focusable() {
                                                found = Some(azul_core::dom::DomNodeId {
                                                    dom: *dom_id,
                                                    node: NodeHierarchyItemId::from_crate_internal(Some(nid)),
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
                    found
                } else {
                    None
                };

                if let Some(new_focus_target) = clicked_focusable_node {
                    let old_focus_node_id = old_focus.and_then(|f| f.node.into_crate_internal());
                    let new_focus_node_id = new_focus_target.node.into_crate_internal();
                    if old_focus_node_id != new_focus_node_id {
                        let r = self.apply_system_change(&SystemChange::SetFocus {
                            new_focus: Some(new_focus_target),
                            old_focus,
                        });
                        result = result.max(r);
                        mouse_click_focus_changed = true;
                    }
                }
            }
        }

        // KEYBOARD DEFAULT ACTIONS (Tab navigation, Enter/Space activation, Escape)
        let mut default_action_focus_changed = false;
        let mut synthetic_click_target: Option<azul_core::dom::DomNodeId> = None;

        if !prevent_default {
            let has_key_event = pre_filter.user_events.iter().any(|e| {
                matches!(e.event_type, azul_core::events::EventType::KeyDown)
            });

            if has_key_event {
                let keyboard_state = &self.get_current_window_state().keyboard_state;
                let focused_node = old_focus;
                let layout_results = self.get_layout_window().map(|lw| &lw.layout_results);

                if let Some(layout_results) = layout_results {
                    let default_action_result = azul_layout::default_actions::determine_keyboard_default_action(
                        keyboard_state, focused_node, layout_results, prevent_default,
                    );

                    if default_action_result.has_action() {
                        use azul_core::events::DefaultAction;
                        use azul_layout::managers::focus_cursor::resolve_focus_target;

                        match &default_action_result.action {
                            DefaultAction::FocusNext | DefaultAction::FocusPrevious |
                            DefaultAction::FocusFirst | DefaultAction::FocusLast => {
                                let focus_target = azul_layout::default_actions::default_action_to_focus_target(&default_action_result.action);
                                if let Some(focus_target) = focus_target {
                                    let resolve_result = resolve_focus_target(&focus_target, layout_results, focused_node);
                                    if let Ok(new_focus_node) = resolve_result {
                                        let r = self.apply_system_change(&SystemChange::SetFocus {
                                            new_focus: new_focus_node,
                                            old_focus: focused_node,
                                        });
                                        result = result.max(r);
                                        default_action_focus_changed = true;
                                    }
                                }
                            }

                            DefaultAction::ClearFocus => {
                                let r = self.apply_system_change(&SystemChange::SetFocus {
                                    new_focus: None,
                                    old_focus,
                                });
                                result = result.max(r);
                                default_action_focus_changed = true;
                            }

                            DefaultAction::ActivateFocusedElement { target } => {
                                synthetic_click_target = Some(target.clone());
                            }

                            DefaultAction::ScrollFocusedContainer { .. } => {
                                // TODO: Implement keyboard scrolling
                            }

                            DefaultAction::None => {}

                            DefaultAction::SubmitForm { .. } |
                            DefaultAction::CloseModal { .. } |
                            DefaultAction::SelectAllText => {
                                // Placeholder for future implementation
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

                let (click_changes_result, click_update, _) = self.dispatch_events_propagated(&[click_event]);
                result = result.max(click_changes_result);

                if matches!(click_update, Update::RefreshDom | Update::RefreshDomAllWindows) {
                    self.mark_frame_needs_regeneration();
                    result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
                    should_recurse = true;
                }

                log_debug!(
                    super::debug_server::LogCategory::Input,
                    "[Event] Dispatched synthetic click for element activation: {:?}",
                    click_target
                );
            }
        }

        // Handle focus changes: generate synthetic FocusIn/FocusOut events
        log_debug!(
            super::debug_server::LogCategory::Input,
            "[Event] Focus check: focus_changed={}, default_action_focus_changed={}, mouse_click_focus_changed={}, depth={}, old_focus={:?}",
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
                "[Event] Focus changed! old_focus={:?}, new_focus={:?}",
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
                        "[Event] Dispatching FocusLost to node {:?}",
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
                        "[Event] Dispatching FocusReceived to node {:?}",
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
                    let (focus_changes_result, focus_update, _) = self.dispatch_events_propagated(&focus_events);
                    result = result.max(focus_changes_result);
                    if matches!(focus_update, Update::RefreshDom | Update::RefreshDomAllWindows) {
                        self.mark_frame_needs_regeneration();
                        result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
                    }
                }
            }

            // CRITICAL: Update previous_state BEFORE recursing to prevent the same
            // keyboard events from being detected again. Without this, a Tab key
            // would trigger FocusNext on every recursion level.
            let current = self.get_current_window_state().clone();
            self.set_previous_window_state(current);

            // Recurse to process any further events that may have been triggered
            let focus_result = self.process_window_events(depth + 1);
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

            let recursive_result = self.process_window_events(depth + 1);
            result = result.max(recursive_result);
        }

        // NOTE: Window drag is handled entirely by titlebar callbacks.
        // The DragStart/Drag callbacks on the csd-title node read the
        // gesture manager's drag delta and window_position_at_session_start
        // to compute the new window position via modify_window_state().

        // Finalize pending focus changes (cursor init + blink timer)
        let r = self.apply_system_change(&SystemChange::FinalizePendingFocusChanges);
        result = result.max(r);

        result
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
        use azul_core::callbacks::Update;

        let (timer_changes_result, timer_results) = self.invoke_expired_timers();
        let mut needs_redraw = timer_changes_result != ProcessEventResult::DoNothing;
        let mut needs_layout_regeneration = false;

        for update in &timer_results {
            // apply_user_change was already called inside invoke_expired_timers
            // We just check if the callback requested a visual update
            match update {
                Update::RefreshDom | Update::RefreshDomAllWindows => {
                    needs_redraw = true;
                    needs_layout_regeneration = true;
                }
                _ => {}
            }
        }

        if let Some((thread_changes_result, thread_update)) = self.invoke_thread_callbacks() {
            // apply_user_change was already called inside invoke_thread_callbacks
            if thread_changes_result != ProcessEventResult::DoNothing {
                needs_redraw = true;
            }
            match thread_update {
                Update::RefreshDom | Update::RefreshDomAllWindows => {
                    needs_redraw = true;
                    needs_layout_regeneration = true;
                }
                _ => {}
            }
        }

        // Also sync window state after all changes
        self.sync_window_state();

        // IMPORTANT: Only regenerate layout when DOM actually changed
        // (RefreshDom / RefreshDomAllWindows). A mere ShouldReRenderCurrentWindow
        // from image-callback updates does NOT require a full layout pass —
        // it only needs to re-invoke image callbacks and repaint.
        if needs_layout_regeneration {
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
    /// * `Vec<Update>` - Update level from each invoked timer callback
    ///
    /// ## Platform Usage
    /// Call this from platform event loops when:
    /// - **Windows**: In `WM_TIMER` handler
    /// - **macOS**: In `performSelector:withObject:afterDelay:` callback
    /// - **X11**: After `select()` timeout
    /// - **Wayland**: After `timerfd` read
    fn invoke_expired_timers(&mut self) -> (ProcessEventResult, Vec<azul_core::callbacks::Update>) {
        use azul_core::callbacks::Update;
        use azul_core::task::TimerId;
        use azul_layout::callbacks::ExternalSystemCallbacks;

        // Get current system time
        let system_callbacks = ExternalSystemCallbacks::rust_internal();
        let current_time = (system_callbacks.get_system_time_fn.cb)();
        let frame_start: azul_core::task::Instant = current_time.clone().into();

        // First, get expired timer IDs without borrowing self
        let expired_timer_ids: Vec<TimerId> = {
            let layout_window = match self.get_layout_window_mut() {
                Some(lw) => lw,
                None => return (ProcessEventResult::DoNothing, Vec::new()),
            };
            layout_window.tick_timers(current_time)
        };

        if expired_timer_ids.is_empty() {
            return (ProcessEventResult::DoNothing, Vec::new());
        }

        let mut all_results = Vec::new();
        let mut changes_result = ProcessEventResult::DoNothing;

        // Process each expired timer
        for timer_id in expired_timer_ids {
            // Prepare borrows fresh for each timer invocation
            let mut borrows = self.prepare_callback_invocation();

            let (changes, update) = borrows.layout_window.run_single_timer(
                timer_id.id,
                frame_start.clone(),
                &borrows.window_handle,
                borrows.gl_context_ptr,
                borrows.system_style.clone(),
                &ExternalSystemCallbacks::rust_internal(),
                borrows.previous_window_state,
                borrows.current_window_state,
                borrows.renderer_resources,
            );

            // Apply changes immediately so inter-timer visibility works
            // (e.g., timer A removes timer B → B shouldn't fire)
            drop(borrows);

            for change in &changes {
                let r = self.apply_user_change(change);
                changes_result = changes_result.max(r);
            }

            // Mark frame for redraw if callback requested it
            if update == Update::RefreshDom
                || update == Update::RefreshDomAllWindows
            {
                self.mark_frame_needs_regeneration();
            }

            all_results.push(update);
        }

        (changes_result, all_results)
    }

    // PROVIDED: Thread Callback Invocation (Cross-Platform Implementation)

    /// Invoke all pending thread callbacks (writeback messages).
    ///
    /// This method polls all active threads for completed work and invokes
    /// the writeback callbacks for any threads that have finished.
    ///
    /// ## Returns
    /// * `Option<Update>` - Update level from thread writeback callbacks, or None if no threads processed
    ///
    /// ## Platform Usage
    /// Call this from platform event loops when:
    /// - **Windows**: In `WM_TIMER` handler with thread timer ID (0xFFFF)
    /// - **macOS**: In thread poll timer callback (NSTimer every 16ms)
    /// - **X11**: After `select()` timeout when threads exist
    /// - **Wayland**: After thread timerfd read
    fn invoke_thread_callbacks(&mut self) -> Option<(ProcessEventResult, azul_core::callbacks::Update)> {
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
        let (changes, update) = borrows.layout_window.run_all_threads(
            &mut *app_data,
            &borrows.window_handle,
            borrows.gl_context_ptr,
            borrows.system_style.clone(),
            &ExternalSystemCallbacks::rust_internal(),
            borrows.previous_window_state,
            borrows.current_window_state,
            borrows.renderer_resources,
        );

        drop(app_data);
        drop(borrows);

        let mut changes_result = ProcessEventResult::DoNothing;
        for change in &changes {
            let r = self.apply_user_change(change);
            changes_result = changes_result.max(r);
        }

        Some((changes_result, update))
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
