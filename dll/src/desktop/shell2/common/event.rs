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
    window::{RawWindowHandle, VirtualKeyCode},
    FastBTreeSet,
};
use azul_layout::{
    callbacks::{
        Callback as LayoutCallback, CallbackInfo, ExternalSystemCallbacks,
    },
    event_determination::determine_all_events,
    hit_test::FullHitTest,
    managers::selection::{ClipboardContent, StyledTextRunVec},
    window::{LayoutWindow, ScrollbarDragState},
    window_state::{self, FullWindowState},
};
use rust_fontconfig::FcFontCache;

use crate::desktop::wr_translate2::{self, AsyncHitTester, WrRenderApi};
use crate::{log_debug, log_error, log_trace, log_warn};

const AUTO_SCROLL_EDGE_THRESHOLD: f32 = 30.0;
const AUTO_SCROLL_MAX_SPEED: f32 = 15.0;
/// One wheel detent / one scroll "line", in logical pixels — the engine's
/// canonical unit for DISCRETE scroll input. Every backend converts its
/// native tick to this (X11 button-4/5 ticks, Win32 WHEEL_DELTA notches
/// scaled by the wheel-lines setting, macOS non-precise line deltas, Wayland
/// axis_discrete detents, keyboard arrow scrolling). Trackpad/precise deltas
/// stay raw. Tune HERE, never per-backend — four independent `20.0`s
/// drifting apart is exactly how macOS wheels ended up ~20x slower than X11.
pub const WHEEL_SCROLL_PIXELS_PER_LINE: f32 = 20.0;
const KEYBOARD_SCROLL_LINE_PX: f32 = WHEEL_SCROLL_PIXELS_PER_LINE;
const KEYBOARD_SCROLL_DOCUMENT_MAX: f32 = 100_000.0;
const DEFAULT_VIEWPORT_HEIGHT: f32 = 600.0;

#[repr(C)]
struct EmptyRefAnyData(u8);

/// Parse a node type string into a NodeType.
/// Supports tag names ("div", "p", "span", "button", etc.)
/// and text content ("text:Hello World").
fn parse_node_type_from_str(s: &str) -> azul_core::dom::NodeType {
    use azul_core::dom::NodeType;
    if let Some(text) = s.strip_prefix("text:") {
        return NodeType::Text(azul_css::css::BoxOrStatic::heap(text.to_string().into()));
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
///
/// Defined in `azul-layout` so that the headless E2E runner — which ports this
/// event loop (`azul_layout::e2e::runner`) — caps at the SAME depth and reports
/// the same `relayout_iterations` / `hit_depth_cap` as the real shell.
const MAX_EVENT_RECURSION_DEPTH: usize = azul_layout::window::MAX_EVENT_RECURSION_DEPTH;

// Platform-specific Clipboard Helpers
//
// The seam moved to `super::clipboard`: the OS clipboard now carries typed
// multi-flavor payloads (`ClipboardPayload`) rather than bare strings, and
// that module owns both the per-OS dispatch and the conversions to and from
// azul's `ClipboardContent`.
use super::clipboard::{
    clipboard_content_to_payload, get_system_clipboard, payload_to_clipboard_content,
    set_system_clipboard,
};

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

    // MWA-B8: the timer stays alive for ANY live drag — text selection
    // (button held), node drag-and-drop, or an OS file hover (no button
    // state exists from our side during an XDND/OLE drag).
    let dragging = full_window_state.mouse_state.left_down
        || callback_info.is_drag_active()
        || callback_info.get_hovered_file().is_some();
    if !dragging {
        return azul_core::callbacks::TimerCallbackReturn::terminate_unchanged();
    }

    // MWA-B8: OutOfWindow coordinates are VALID input for auto-scroll —
    // scrolling while the pointer is past the window edge is the entire
    // point. The old get_position() (None for OutOfWindow) self-terminated
    // the timer the moment X11 delivered LeaveNotify during the implicit
    // grab (holding still past the edge stopped scrolling), and likewise
    // under Windows' WM_MOUSELEAVE-during-SetCapture.
    let mouse_position = match &full_window_state.mouse_state.cursor_position {
        azul_core::window::CursorPosition::InWindow(pos)
        | azul_core::window::CursorPosition::OutOfWindow(pos) => *pos,
        azul_core::window::CursorPosition::Uninitialized => {
            return azul_core::callbacks::TimerCallbackReturn::terminate_unchanged();
        }
    };

    // MWA-B8: anchor node — the focused node for text-selection drags,
    // else the node under the pointer (node DnD / OS file hover, where
    // nothing is focused). Previously focused-only, so non-text drags
    // never found a scroll container.
    let anchor_node = callback_info
        .get_focused_node()
        .or_else(|| callback_info.get_deepest_hovered_node());
    let focused_node = match anchor_node {
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

    // The scroll box the anchor node LIVES IN — itself included.
    //
    // This used to be `find_scroll_parent`, whose walk was hardcoded to skip
    // the node itself. The caret in a `TextInput` sits on the value `<p>`,
    // which IS the horizontal scroll box (see `TEXT_INPUT_LABEL_PROPS`), so
    // the strict-ancestor search walked straight past the field and returned
    // the page: dragging a selection past the right edge of an overflowing
    // text field scrolled the PAGE instead of the field, and the field's own
    // content never moved. Same shape for any editable that is its own
    // scroller (TextArea, code editors).
    let scroll_parent = match callback_info.find_scroll_target(dom_id, node_id) {
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

    // `container_rect` is the SCROLLPORT (the padding box) in STATIC layout
    // coordinates — ONE box for origin and size, published by
    // `managers::scroll_registration` and read identically by
    // `scroll_selection_into_view` and `scroll_into_view`. It used to be a
    // padding-box size at a border-box origin, so the edge tests below ran
    // against a rectangle that is not any CSS box and triggered a border-width
    // early on the top and left edges.
    //
    // `mouse_position` arrives in WINDOW space, and the two agree only while no
    // ancestor of this container is itself scrolled — so for a NESTED scroller
    // the edge tests ran against a box that is not where the container appears,
    // and it autoscrolled from the wrong edges. Subtract the scroll of every
    // ancestor ABOVE it; its own offset does not move its box.
    // `find_scroll_parent` (STRICT ancestors) is the right walker here, unlike
    // the target lookup above.
    let mut ancestor_scroll = azul_core::geom::LogicalPosition::zero();
    let mut walk = scroll_parent;
    for _ in 0..AUTO_SCROLL_ANCESTOR_WALK_LIMIT {
        let Some(parent) = callback_info.find_scroll_parent(dom_id, walk) else {
            break;
        };
        if let Some(info) = callback_info.get_scroll_node_info(dom_id, parent) {
            ancestor_scroll.x += info.current_offset.x;
            ancestor_scroll.y += info.current_offset.y;
        }
        walk = parent;
    }

    // Calculate scroll delta based on mouse distance from container edges
    let container = azul_core::geom::LogicalRect {
        origin: azul_core::geom::LogicalPosition::new(
            scroll_info.container_rect.origin.x - ancestor_scroll.x,
            scroll_info.container_rect.origin.y - ancestor_scroll.y,
        ),
        size: scroll_info.container_rect.size,
    };
    let edge_threshold = AUTO_SCROLL_EDGE_THRESHOLD;
    let max_speed = AUTO_SCROLL_MAX_SPEED;

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

    // DoNothing, not RefreshDom: `scroll_to` already yields
    // ShouldReRenderCurrentWindow through the CallbackChange::ScrollTo arm.
    // Returning RefreshDom re-invoked the APP's layout() callback every 16ms
    // for the entire duration of a drag-autoscroll.
    azul_core::callbacks::TimerCallbackReturn {
        should_update: azul_core::callbacks::Update::DoNothing,
        should_terminate: TerminateTimer::Continue,
    }
}

/// Record ONE undoable entry for a multi-cursor edit that neither recording
/// site covers.
///
/// `apply_text_changeset` records typing (changeset ids counting UP from 0)
/// and `delete_selection` records deletions (ids counting DOWN from
/// `usize::MAX`). A smart paste distributes N clipboard lines over N cursors
/// through `edit_text_multi` and reaches neither, so Ctrl+Z after one used to
/// undo whatever edit came before it.
///
/// The restore itself runs off the styled pre/post content snapshots keyed by
/// changeset id, so the operation kind and range recorded here are
/// informational — they are what the C-API `inspect_*` fns read. Ids come
/// from `LayoutWindow::record_text_edit_undo`'s single monotonic counter
/// (shared with typing and deletion).
fn record_multi_edit_undo(
    lw: &mut LayoutWindow,
    target: azul_core::dom::DomNodeId,
    node_id: NodeId,
    pre_content: &[azul_layout::text3::cache::InlineContent],
    post_content: &[azul_layout::text3::cache::InlineContent],
    pre_selections: &[azul_core::selection::Selection],
) {
    use azul_core::{selection::Selection, window::CursorPosition};
    use azul_layout::managers::{
        changeset::{TextOpPaste, TextOperation},
        undo_redo::NodeStateSnapshot,
    };

    let pre_text = lw.extract_text_from_inline_content(pre_content);
    let old_cursor = pre_selections.first().and_then(|sel| match sel {
        Selection::Cursor(c) => Some(*c),
        Selection::Range(_) => None,
    });
    let old_range = pre_selections.first().and_then(|sel| match sel {
        Selection::Range(r) => Some(*r),
        Selection::Cursor(_) => None,
    });
    let timestamp = azul_core::task::Instant::now();

    let pre_state = NodeStateSnapshot {
        node_id,
        text_content: pre_text.into(),
        cursor_position: old_cursor.into(),
        selection_range: old_range.into(),
        timestamp,
    };
    // A smart paste bypasses the text-input record pipeline, so the commit
    // queues the host's Input notification.
    lw.record_text_edit_undo(
        target,
        pre_state,
        pre_content.to_vec(),
        post_content.to_vec(),
        TextOperation::Paste(TextOpPaste {
            content: ClipboardContent {
                plain_text: lw.extract_text_from_inline_content(post_content).into(),
                styled_runs: StyledTextRunVec::from_const_slice(&[]),
            },
            position: CursorPosition::Uninitialized,
            new_cursor: CursorPosition::Uninitialized,
        }),
        azul_layout::window::TextEditNotify::QueueInput,
    );
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
        // A focus change with no detected resolved-style delta STILL needs the
        // display list REBUILT, not merely re-presented. Two focus visuals are
        // not captured in `changed_nodes`: (1) the caret, painted from focus /
        // editing state at build time (not a restyle property), and (2)
        // `:focus`-CONDITIONAL properties like the text input's focus border,
        // re-evaluated against the node's focused flag when the display list is
        // built. Returning `ShouldReRenderCurrentWindow` re-presented the STALE
        // list, so after a blur the caret and the blue focus border stayed on
        // screen ("focus doesn't get unset"). Rebuild so both re-resolve.
        return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
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

/// Apply an incremental `:hover` restyle for this pass's MouseEnter /
/// MouseLeave targets and classify the result (MWA-A3c).
///
/// Before this existed, enter/leave events dispatched to callbacks but the
/// styled DOM's `:hover` flags were only recomputed by a FULL DOM
/// regeneration — pure-CSS hover styling did nothing on any backend until
/// something else rebuilt the DOM (`restyle_nodes_hover` was dead code).
fn apply_hover_restyle(
    layout_window: &mut LayoutWindow,
    changes_per_dom: std::collections::BTreeMap<
        azul_core::dom::DomId,
        azul_core::styled_dom::HoverChange,
    >,
) -> ProcessEventResult {
    use azul_core::diff::ChangeAccumulator;

    let mut result = ProcessEventResult::DoNothing;
    for (dom_id, hover_change) in changes_per_dom {
        let Some(layout_result) = layout_window.layout_results.get_mut(&dom_id) else {
            continue;
        };
        let restyle_result = layout_result.styled_dom.restyle_on_state_change(
            None, // focus
            Some(hover_change),
            None, // active
        );
        if restyle_result.changed_nodes.is_empty() {
            continue;
        }
        // Same granular classification as apply_focus_restyle: paint-only
        // changes avoid relayout, layout-affecting ones take the
        // incremental path (no DOM rebuild — states are already updated).
        let r = if restyle_result.gpu_only_changes {
            ProcessEventResult::ShouldReRenderCurrentWindow
        } else {
            let mut accumulator = ChangeAccumulator::new();
            accumulator.merge_restyle_result(&restyle_result);
            if accumulator.needs_layout() {
                ProcessEventResult::ShouldIncrementalRelayout
            } else if accumulator.needs_paint_only() {
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            } else {
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
        };
        result = result.max(r);
    }
    result
}

// MWA-B11: CSD resize edges

/// Which frameless-window edge a pointer press falls on (within
/// [`CSD_RESIZE_BAND_PX`]). Each backend maps this to its native
/// interactive-resize primitive (xdg_toplevel.resize / _NET_WM_MOVERESIZE /
/// WM_NCLBUTTONDOWN HT*; macOS resizes natively via the Resizable
/// styleMask on borderless windows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsdResizeEdge {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Width of the invisible resize band along frameless-window borders.
pub const CSD_RESIZE_BAND_PX: f32 = 8.0;

/// MWA-B11: CSD resize-edge hit test. Returns which edge (if any) `pos`
/// falls on for a window of `size`; callers gate on
/// `flags.decorations == WindowDecorations::None` (server-decorated windows
/// get real WM edges).
#[must_use]
pub fn csd_resize_edge_at(
    pos: azul_core::geom::LogicalPosition,
    size: azul_core::geom::LogicalSize,
    band: f32,
) -> Option<CsdResizeEdge> {
    let l = pos.x <= band;
    let r = pos.x >= size.width - band;
    let t = pos.y <= band;
    let b = pos.y >= size.height - band;
    Some(match (l, r, t, b) {
        (true, _, true, _) => CsdResizeEdge::TopLeft,
        (_, true, true, _) => CsdResizeEdge::TopRight,
        (true, _, _, true) => CsdResizeEdge::BottomLeft,
        (_, true, _, true) => CsdResizeEdge::BottomRight,
        (true, ..) => CsdResizeEdge::Left,
        (_, true, ..) => CsdResizeEdge::Right,
        (_, _, true, _) => CsdResizeEdge::Top,
        (_, _, _, true) => CsdResizeEdge::Bottom,
        _ => return None,
    })
}

#[cfg(test)]
mod csd_resize_edge_tests {
    use super::*;
    use azul_core::geom::{LogicalPosition, LogicalSize};

    fn size() -> LogicalSize {
        LogicalSize { width: 800.0, height: 600.0 }
    }

    #[test]
    fn corners_edges_and_center() {
        let p = |x, y| LogicalPosition { x, y };
        assert_eq!(csd_resize_edge_at(p(2.0, 2.0), size(), 8.0), Some(CsdResizeEdge::TopLeft));
        assert_eq!(csd_resize_edge_at(p(797.0, 3.0), size(), 8.0), Some(CsdResizeEdge::TopRight));
        assert_eq!(csd_resize_edge_at(p(1.0, 599.0), size(), 8.0), Some(CsdResizeEdge::BottomLeft));
        assert_eq!(csd_resize_edge_at(p(799.0, 598.0), size(), 8.0), Some(CsdResizeEdge::BottomRight));
        assert_eq!(csd_resize_edge_at(p(400.0, 4.0), size(), 8.0), Some(CsdResizeEdge::Top));
        assert_eq!(csd_resize_edge_at(p(400.0, 597.0), size(), 8.0), Some(CsdResizeEdge::Bottom));
        assert_eq!(csd_resize_edge_at(p(3.0, 300.0), size(), 8.0), Some(CsdResizeEdge::Left));
        assert_eq!(csd_resize_edge_at(p(796.0, 300.0), size(), 8.0), Some(CsdResizeEdge::Right));
        assert_eq!(csd_resize_edge_at(p(400.0, 300.0), size(), 8.0), None);
        // just inside the band boundary
        assert_eq!(csd_resize_edge_at(p(8.0, 300.0), size(), 8.0), Some(CsdResizeEdge::Left));
        assert_eq!(csd_resize_edge_at(p(8.1, 300.0), size(), 8.0), None);
    }
}

// Button state bitfield constants for `record_input_sample`.
pub const BUTTON_STATE_NONE: u8 = 0x00;
pub const BUTTON_STATE_LEFT: u8 = 0x01;
pub const BUTTON_STATE_RIGHT: u8 = 0x02;
pub const BUTTON_STATE_MIDDLE: u8 = 0x04;

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

/// The `CommonWindowState` fields one layout pass needs, borrowed together.
///
/// Same trick as [`InvokeSingleCallbackBorrows`], and needed for the same
/// reason one level down: `current_window_state` is private, so a call site
/// cannot take a field-level borrow of it — [`CommonWindowState::current_window_state`]
/// borrows the whole struct, which collides with the `&mut layout_window` /
/// `&mut renderer_resources` the very same call needs. Handing all of them out
/// of ONE `&mut self` keeps the borrows disjoint.
///
/// `layout_window` is an `Option` because X11 and Wayland create the window
/// before the layout window exists.
pub struct LayoutPassBorrows<'a> {
    /// The layout window the pass drives
    pub layout_window: Option<&'a mut LayoutWindow>,
    /// The state the pass lays out against
    pub current_window_state: &'a FullWindowState,
    /// The event-diff baseline (callback passes report transitions from it)
    pub previous_window_state: &'a Option<FullWindowState>,
    /// The resource pool the pass fills
    pub renderer_resources: &'a mut RendererResources,
    /// OpenGL context pointer
    pub gl_context_ptr: &'a OptionGlContextPtr,
    /// Shared font cache
    pub fc_cache: &'a Arc<FcFontCache>,
    /// System style
    pub system_style: &'a Arc<azul_css::system::SystemStyle>,
    /// Shared application data
    pub app_data: &'a Arc<RefCell<RefAny>>,
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
/// Where a window-state mutation originated. This is the event-source tracking
/// the window-state sync relies on (see [`CommonWindowState::update_window_state`]).
///
/// `sync_window_state()` pushes the diff between `os_synced_state` (the
/// baseline = "what the OS already has") and `current_window_state` (what we
/// want) to the OS via `XMoveWindow`/`XResizeWindow`/`SetWindowPos`/…. Tagging
/// the source decides whether a change is echoed:
///   * [`App`](WindowStateSource::App) — the application/API asked for it, so it
///     must be applied to the OS (it isn't there yet).
///   * [`Os`](WindowStateSource::Os) — the OS *reported* it (already applied
///     outside), so it must NOT be echoed; doing so causes feedback loops — e.g.
///     a reparenting WM reports frame-relative coords and the echo walks the
///     window across the screen, spamming configure events (F4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowStateSource {
    /// Application/API requested the change → `sync_window_state()` applies it.
    App,
    /// OS reported the change (already applied) → never echoed back.
    Os,
}

// Input-delta validation (R2)

/// Is the window-state validation gate on?
///
/// Diagnostics here are runtime env / atomics, never cargo features (the same
/// ruling `AZ_LOG` / `AZ_PROFILE` / `AZ_BACKEND` / `AZ_E2E_TEST` follow), read
/// once. A debug build validates unconditionally; a release build — which is
/// what the battery and every shipped binary are — has to opt in with
/// `AZ_VALIDATE=1`.
#[must_use]
/// How many nodes the Ctrl+A block scan will visit under the editing host.
/// Editable subtrees are small; the bound only stops a malformed hierarchy
/// from spinning.
const SELECT_ALL_BLOCK_SCAN_LIMIT: usize = 4096;

/// How far the autoscroll edge test walks up summing ancestor scroll offsets.
const AUTO_SCROLL_ANCESTOR_WALK_LIMIT: usize = 64;

pub fn validation_enabled() -> bool {
    if cfg!(debug_assertions) {
        return true;
    }
    #[cfg(feature = "std")]
    {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        return *ON.get_or_init(|| {
            std::env::var("AZ_VALIDATE")
                .map(|v| {
                    let v = v.trim().to_ascii_lowercase();
                    !matches!(v.as_str(), "" | "0" | "off" | "false" | "no" | "none")
                })
                .unwrap_or(false)
        });
    }
    #[cfg(not(feature = "std"))]
    false
}

/// The first event-bearing field on which two window states disagree, for the
/// validation message.
///
/// STRICT ALLOW-LIST: it names exactly the fields `determine_all_events` diffs
/// to derive an event, and nothing else. It used to end in a catch-all —
/// `if a == b { None } else { Some("(a field event determination does not
/// read)") }` — which quietly turned it into a DENY-list over
/// `FullWindowState: PartialEq`, so every field the *shells* own was reported
/// as a lost event. `ime_position` is the one that fires in practice: all four
/// backends write it after `process_window_events` has consumed the delta,
/// because it needs the post-layout caret rect. A debug build (where
/// [`validation_enabled`] is unconditionally true) therefore panicked on the
/// next poll as soon as the user typed into a contenteditable.
///
/// A field no event is derived from cannot encode a LOST event, which is the
/// only failure [`check_input_delta_consumed`] exists to catch. The omissions
/// are deliberate, not oversights: `title`, the rest of `flags`
/// (decorations, visibility, …), `size.min_dimensions`/`max_dimensions` and
/// `renderer_options` are pushed to the OS by each backend's
/// `sync_window_state()`, which diffs against `os_synced_state` — a different
/// baseline this check never looks at — and `ime_position`, `debug_state`,
/// `window_id`, `active_route` and the callback handles are pure bookkeeping.
///
/// Never add a catch-all back. The destructuring below is exhaustive on
/// purpose (no `..`): a field added to `FullWindowState` breaks THIS function
/// at compile time and has to be classified — bound and compared if an event
/// is derived from it, bound to `_` if not — instead of silently re-arming the
/// panic for every shell-owned write.
fn first_differing_state_field(
    a: &FullWindowState,
    b: &FullWindowState,
) -> Option<&'static str> {
    let FullWindowState {
        // Event-bearing: `determine_all_events` turns a change in one of these
        // into a callback, so an unconsumed delta here IS a lost event.
        size,
        position,
        flags,
        window_focused,
        theme,
        monitor_id,
        mouse_state,
        keyboard_state,
        touch_state,
        // Not event-bearing — see the doc above. Never compared.
        platform_specific_options: _,
        window_id: _,
        title: _,
        close_callback: _,
        layout_callback: _,
        ime_position: _,
        renderer_options: _,
        debug_state: _,
        background_color: _,
        active_route: _,
    } = a;

    if size.dimensions != b.size.dimensions {
        return Some("size.dimensions");
    }
    if size.dpi != b.size.dpi {
        return Some("size.dpi");
    }
    if *position != b.position {
        return Some("position");
    }
    if flags.frame != b.flags.frame {
        return Some("flags.frame");
    }
    if flags.close_requested != b.flags.close_requested {
        return Some("flags.close_requested");
    }
    if *window_focused != b.window_focused {
        return Some("window_focused");
    }
    if *theme != b.theme {
        return Some("theme");
    }
    if *monitor_id != b.monitor_id {
        return Some("monitor_id");
    }
    if *mouse_state != b.mouse_state {
        return Some("mouse_state");
    }
    if *keyboard_state != b.keyboard_state {
        return Some("keyboard_state");
    }
    if *touch_state != b.touch_state {
        return Some("touch_state");
    }
    None
}

/// Catch an UNCONSUMED INPUT DELTA before it is silently overwritten.
///
/// The invariant: whenever a platform handler is about to snapshot the
/// event-diff baseline, `previous_window_state` already equals
/// `current_window_state` — every completed `process_window_events` pass
/// leaves it that way (it consumes its own delta), and every injection site
/// snapshots immediately before mutating and runs its pass immediately after.
///
/// A non-zero diff here means an earlier handler mutated
/// `current_window_state` and returned WITHOUT running a pass. The snapshot
/// about to happen overwrites the baseline with the already-mutated state, and
/// the event that delta encoded — a resize, a DPI change, a maximize — is gone
/// for good. Nothing crashes and nothing logs; the callback simply never
/// fires, which is exactly how it stayed unnoticed on four backends.
///
/// Gated on [`validation_enabled`]; panics rather than logs, because a
/// diagnostic that only logs in a loop of 60 frames per second is a diagnostic
/// nobody reads.
/// What [`PlatformWindow::request_window_close`] decided.
#[derive(Debug, Clone, Copy)]
pub struct WindowCloseOutcome {
    /// The flag survived the pass — no callback vetoed, so the close proceeds.
    pub confirmed: bool,
    /// The pass result, so a close callback that restyles (an "unsaved
    /// changes" prompt) still gets its relayout or repaint.
    pub result: ProcessEventResult,
}

/// The fields every backend's `sync_window_state()` diffs against
/// `os_synced_state` — the ones a write has to make the App-vs-Os decision for.
/// Used by [`CommonWindowState::update_unsynced_state`] to prove it did not
/// touch any of them.
fn os_synced_fields(
    state: &FullWindowState,
) -> (
    azul_css::AzString,
    azul_core::window::WindowSize,
    azul_core::window::WindowPosition,
    azul_core::window::WindowFlags,
) {
    (
        state.title.clone(),
        state.size,
        state.position,
        state.flags,
    )
}

/// How a menu item's callback came to run — see
/// `PlatformWindow::invoke_menu_callback` for what each owns afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuInvocation {
    /// The OS activated the item (menu-bar click, Win32 command id, GNOME
    /// action); `site` names the backend for the `AZ_VALIDATE` report.
    Native { site: &'static str },
    /// The shared accelerator dispatch, inside an input pass.
    Accelerator,
}

pub fn check_input_delta_consumed(
    previous: Option<&FullWindowState>,
    current: &FullWindowState,
    site: &str,
) {
    if !validation_enabled() {
        return;
    }
    let Some(previous) = previous else {
        return; // no baseline yet: the first frame has nothing to consume
    };
    if let Some(field) = first_differing_state_field(previous, current) {
        log_error!(
            super::debug_server::LogCategory::Window,
            "[AZ_VALIDATE] unconsumed input delta at {}: previous_window_state.{} != \
             current_window_state.{} — a handler mutated the current state without running \
             process_window_events(), and this snapshot is about to delete that event",
            site,
            field,
            field
        );
        panic!(
            "[AZ_VALIDATE] unconsumed input delta at {site}: previous_window_state.{field} != \
             current_window_state.{field}"
        );
    }
}

// Platform input translation
//
// Pure functions the `#[cfg(target_os = ...)]` backends delegate to. They live
// HERE, not in `windows/` or `macos/`, for one reason: nothing in CI builds
// those modules — the only `azul-dll` unit-test run is `cd dll && cargo test`
// on ubuntu — so a test placed next to the code it pins never executes, and a
// table or a unit conversion that silently regresses stays silent until a user
// on that OS notices. Everything below compiles and is tested on every host.

/// Decode one UTF-16 code unit of a Win32 character stream (`WM_CHAR` /
/// `WM_IME_CHAR`), pairing surrogates across messages.
///
/// Win32 delivers text as UTF-16, so anything outside the BMP — a
/// supplementary-plane CJK ideograph, an emoji committed by an IME — arrives as
/// TWO messages carrying the halves of a surrogate pair. `char::from_u32`
/// answers `None` for a lone half, so passing `wparam` straight to it dropped
/// both halves and the character never appeared at all. `high_surrogate` is the
/// caller's one-slot carry (`Win32Window::high_surrogate`); `WM_CHAR` and
/// `WM_IME_CHAR` never interleave for one commit, so a single slot serves both.
///
/// `None` means "nothing to insert yet": a pair still waiting for its low half,
/// an unpaired low half, or a control character (never text input).
#[must_use]
pub fn win32_utf16_stream_char(high_surrogate: &mut Option<u16>, code_unit: u32) -> Option<char> {
    if (0xD800..=0xDBFF).contains(&code_unit) {
        *high_surrogate = Some(code_unit as u16);
        return None;
    }
    if (0xDC00..=0xDFFF).contains(&code_unit) {
        let high = high_surrogate.take()?;
        let pair = [high, code_unit as u16];
        return match char::decode_utf16(pair.iter().copied()).next() {
            Some(Ok(chr)) => Some(chr),
            _ => None,
        };
    }
    *high_surrogate = None;
    char::from_u32(code_unit).filter(|chr| !chr.is_control())
}

/// Logical pixels one wheel notch scrolls on Win32, from the user's
/// `SPI_GETWHEELSCROLLLINES` setting (`SystemStyle::input::wheel_scroll_lines`).
///
/// The setting was captured and then read by nobody — the notch was hardcoded —
/// so the Control Panel / mouse-driver scroll speed did nothing on Windows.
/// Applied as a RATIO against the Windows default of 3 lines rather than as an
/// absolute line height, which keeps the default bit-identical to
/// [`WHEEL_SCROLL_PIXELS_PER_LINE`] (and therefore to X11 and macOS).
///
/// `viewport_extent` is the scroll axis' viewport size, used only by the
/// "one screen at a time" sentinel. `0` lines is a legal setting meaning
/// "wheel scrolling off" and yields `0.0`, which the caller must treat as a
/// gate — recording the scroll AND arming the physics timer.
#[must_use]
pub fn win32_wheel_pixels_per_notch(wheel_scroll_lines: u32, viewport_extent: f32) -> f32 {
    // The SPI_GETWHEELSCROLLLINES sentinel for "scroll one screen at a time".
    const WHEEL_PAGESCROLL: u32 = u32::MAX;
    const DEFAULT_WHEEL_SCROLL_LINES: f32 = 3.0;

    if wheel_scroll_lines == WHEEL_PAGESCROLL {
        viewport_extent
    } else {
        WHEEL_SCROLL_PIXELS_PER_LINE * (wheel_scroll_lines as f32 / DEFAULT_WHEEL_SCROLL_LINES)
    }
}

/// Convert one axis of a scroll delta to the engine's raw-delta unit, PIXELS.
///
/// A precise device (trackpad, `hasPreciseScrollingDeltas()`) already reports
/// pixels and passes through untouched. A ratcheting wheel reports LINES — about
/// ±1 per notch — and has to be scaled, or the same physical notch scrolls ~20x
/// less than it does on X11/Win32, which is exactly how macOS wheels ended up
/// crawling.
#[must_use]
pub fn discrete_scroll_delta_to_pixels(raw_delta: f64, has_precise_deltas: bool) -> f64 {
    if has_precise_deltas {
        raw_delta
    } else {
        raw_delta * f64::from(WHEEL_SCROLL_PIXELS_PER_LINE)
    }
}

/// Translate a macOS hardware keycode (`NSEvent::keyCode`) to a
/// [`VirtualKeyCode`].
///
/// `None` means the engine emits NOTHING for that key: `update_keyboard_state`
/// returns early, so an unmapped key is not merely unlabelled, it is dead. The
/// navigation cluster, the function row, the keypad and the right-hand modifiers
/// were all missing for exactly that reason — the nav keys emit Private-Use-Area
/// characters (U+F700..U+F7FF) that the text-input filter correctly refuses to
/// insert, so a missing entry produced no engine event whatsoever.
///
/// Keycode list: <https://eastmanreference.com/complete-list-of-applescript-key-codes>
#[must_use]
pub fn macos_keycode_to_virtual_key(keycode: u16) -> Option<VirtualKeyCode> {
    match keycode {
        0x00 => Some(VirtualKeyCode::A),
        0x01 => Some(VirtualKeyCode::S),
        0x02 => Some(VirtualKeyCode::D),
        0x03 => Some(VirtualKeyCode::F),
        0x04 => Some(VirtualKeyCode::H),
        0x05 => Some(VirtualKeyCode::G),
        0x06 => Some(VirtualKeyCode::Z),
        0x07 => Some(VirtualKeyCode::X),
        0x08 => Some(VirtualKeyCode::C),
        0x09 => Some(VirtualKeyCode::V),
        0x0B => Some(VirtualKeyCode::B),
        0x0C => Some(VirtualKeyCode::Q),
        0x0D => Some(VirtualKeyCode::W),
        0x0E => Some(VirtualKeyCode::E),
        0x0F => Some(VirtualKeyCode::R),
        0x10 => Some(VirtualKeyCode::Y),
        0x11 => Some(VirtualKeyCode::T),
        0x12 => Some(VirtualKeyCode::Key1),
        0x13 => Some(VirtualKeyCode::Key2),
        0x14 => Some(VirtualKeyCode::Key3),
        0x15 => Some(VirtualKeyCode::Key4),
        0x16 => Some(VirtualKeyCode::Key6),
        0x17 => Some(VirtualKeyCode::Key5),
        0x18 => Some(VirtualKeyCode::Equals),
        0x19 => Some(VirtualKeyCode::Key9),
        0x1A => Some(VirtualKeyCode::Key7),
        0x1B => Some(VirtualKeyCode::Minus),
        0x1C => Some(VirtualKeyCode::Key8),
        0x1D => Some(VirtualKeyCode::Key0),
        0x1E => Some(VirtualKeyCode::RBracket),
        0x1F => Some(VirtualKeyCode::O),
        0x20 => Some(VirtualKeyCode::U),
        0x21 => Some(VirtualKeyCode::LBracket),
        0x22 => Some(VirtualKeyCode::I),
        0x23 => Some(VirtualKeyCode::P),
        0x24 => Some(VirtualKeyCode::Return),
        0x25 => Some(VirtualKeyCode::L),
        0x26 => Some(VirtualKeyCode::J),
        0x27 => Some(VirtualKeyCode::Apostrophe),
        0x28 => Some(VirtualKeyCode::K),
        0x29 => Some(VirtualKeyCode::Semicolon),
        0x2A => Some(VirtualKeyCode::Backslash),
        0x2B => Some(VirtualKeyCode::Comma),
        0x2C => Some(VirtualKeyCode::Slash),
        0x2D => Some(VirtualKeyCode::N),
        0x2E => Some(VirtualKeyCode::M),
        0x2F => Some(VirtualKeyCode::Period),
        0x30 => Some(VirtualKeyCode::Tab),
        0x31 => Some(VirtualKeyCode::Space),
        0x32 => Some(VirtualKeyCode::Grave),
        0x33 => Some(VirtualKeyCode::Back),
        0x35 => Some(VirtualKeyCode::Escape),
        0x37 => Some(VirtualKeyCode::LWin), // Command
        0x38 => Some(VirtualKeyCode::LShift),
        0x39 => Some(VirtualKeyCode::Capital), // Caps Lock
        0x3A => Some(VirtualKeyCode::LAlt),    // Option
        0x3B => Some(VirtualKeyCode::LControl),
        0x36 => Some(VirtualKeyCode::RWin), // Right Command
        0x3C => Some(VirtualKeyCode::RShift),
        0x3D => Some(VirtualKeyCode::RAlt),
        0x3E => Some(VirtualKeyCode::RControl),
        // Keypad. Its digits/operators also produce ordinary characters, so the
        // text-insert path in handle_key_down keeps working; these entries are
        // what gives them a VirtualKeyDown as well.
        0x41 => Some(VirtualKeyCode::NumpadDecimal),
        0x43 => Some(VirtualKeyCode::NumpadMultiply),
        0x45 => Some(VirtualKeyCode::NumpadAdd),
        0x47 => Some(VirtualKeyCode::Numlock), // Keypad Clear sits in the NumLock position
        0x4B => Some(VirtualKeyCode::NumpadDivide),
        0x4C => Some(VirtualKeyCode::NumpadEnter),
        0x4E => Some(VirtualKeyCode::NumpadSubtract),
        0x51 => Some(VirtualKeyCode::NumpadEquals),
        0x52 => Some(VirtualKeyCode::Numpad0),
        0x53 => Some(VirtualKeyCode::Numpad1),
        0x54 => Some(VirtualKeyCode::Numpad2),
        0x55 => Some(VirtualKeyCode::Numpad3),
        0x56 => Some(VirtualKeyCode::Numpad4),
        0x57 => Some(VirtualKeyCode::Numpad5),
        0x58 => Some(VirtualKeyCode::Numpad6),
        0x59 => Some(VirtualKeyCode::Numpad7),
        0x5B => Some(VirtualKeyCode::Numpad8),
        0x5C => Some(VirtualKeyCode::Numpad9),
        // Function row. macOS orders these by hardware position, not by number.
        0x60 => Some(VirtualKeyCode::F5),
        0x61 => Some(VirtualKeyCode::F6),
        0x62 => Some(VirtualKeyCode::F7),
        0x63 => Some(VirtualKeyCode::F3),
        0x64 => Some(VirtualKeyCode::F8),
        0x65 => Some(VirtualKeyCode::F9),
        0x67 => Some(VirtualKeyCode::F11),
        0x6D => Some(VirtualKeyCode::F10),
        0x6E => Some(VirtualKeyCode::Apps), // PC "Menu" / contextual-menu key
        0x6F => Some(VirtualKeyCode::F12),
        0x76 => Some(VirtualKeyCode::F4),
        0x7A => Some(VirtualKeyCode::F1),
        0x78 => Some(VirtualKeyCode::F2),
        // Navigation cluster. These emit Private-Use-Area characters
        // (U+F700..U+F7FF), which handle_key_down correctly refuses to insert as
        // text — so without an entry here they produced no engine event AT ALL.
        0x73 => Some(VirtualKeyCode::Home),
        0x74 => Some(VirtualKeyCode::PageUp),
        0x75 => Some(VirtualKeyCode::Delete), // ForwardDelete (Back = 0x33 is Backspace)
        0x77 => Some(VirtualKeyCode::End),
        0x79 => Some(VirtualKeyCode::PageDown),
        0x7B => Some(VirtualKeyCode::Left),
        0x7C => Some(VirtualKeyCode::Right),
        0x7D => Some(VirtualKeyCode::Down),
        0x7E => Some(VirtualKeyCode::Up),
        _ => None,
    }
}

/// The Win32 `VK_*` codes [`win32_vkey_to_virtual_key`] matches on.
///
/// Transcribed from `winuser.h` because that table is compiled on every
/// platform and `winapi` is a Windows-only dependency. `win_event.rs` asserts
/// this whole list against `winapi::um::winuser` at compile time, so a mistyped
/// digit here is a Windows BUILD ERROR and not a dead key at runtime.
#[allow(missing_docs)]
pub mod win32_vk {
    pub const VK_BACK: i32 = 0x08;
    pub const VK_TAB: i32 = 0x09;
    pub const VK_RETURN: i32 = 0x0D;
    pub const VK_SHIFT: i32 = 0x10;
    pub const VK_CONTROL: i32 = 0x11;
    pub const VK_MENU: i32 = 0x12;
    pub const VK_PAUSE: i32 = 0x13;
    pub const VK_CAPITAL: i32 = 0x14;
    pub const VK_KANA: i32 = 0x15;
    pub const VK_KANJI: i32 = 0x19;
    pub const VK_ESCAPE: i32 = 0x1B;
    pub const VK_CONVERT: i32 = 0x1C;
    pub const VK_NONCONVERT: i32 = 0x1D;
    pub const VK_SPACE: i32 = 0x20;
    pub const VK_PRIOR: i32 = 0x21;
    pub const VK_NEXT: i32 = 0x22;
    pub const VK_END: i32 = 0x23;
    pub const VK_HOME: i32 = 0x24;
    pub const VK_LEFT: i32 = 0x25;
    pub const VK_UP: i32 = 0x26;
    pub const VK_RIGHT: i32 = 0x27;
    pub const VK_DOWN: i32 = 0x28;
    pub const VK_SNAPSHOT: i32 = 0x2C;
    pub const VK_INSERT: i32 = 0x2D;
    pub const VK_DELETE: i32 = 0x2E;
    pub const VK_LWIN: i32 = 0x5B;
    pub const VK_RWIN: i32 = 0x5C;
    pub const VK_APPS: i32 = 0x5D;
    pub const VK_SLEEP: i32 = 0x5F;
    pub const VK_NUMPAD0: i32 = 0x60;
    pub const VK_NUMPAD1: i32 = 0x61;
    pub const VK_NUMPAD2: i32 = 0x62;
    pub const VK_NUMPAD3: i32 = 0x63;
    pub const VK_NUMPAD4: i32 = 0x64;
    pub const VK_NUMPAD5: i32 = 0x65;
    pub const VK_NUMPAD6: i32 = 0x66;
    pub const VK_NUMPAD7: i32 = 0x67;
    pub const VK_NUMPAD8: i32 = 0x68;
    pub const VK_NUMPAD9: i32 = 0x69;
    pub const VK_MULTIPLY: i32 = 0x6A;
    pub const VK_ADD: i32 = 0x6B;
    pub const VK_SUBTRACT: i32 = 0x6D;
    pub const VK_DECIMAL: i32 = 0x6E;
    pub const VK_DIVIDE: i32 = 0x6F;
    pub const VK_F1: i32 = 0x70;
    pub const VK_F2: i32 = 0x71;
    pub const VK_F3: i32 = 0x72;
    pub const VK_F4: i32 = 0x73;
    pub const VK_F5: i32 = 0x74;
    pub const VK_F6: i32 = 0x75;
    pub const VK_F7: i32 = 0x76;
    pub const VK_F8: i32 = 0x77;
    pub const VK_F9: i32 = 0x78;
    pub const VK_F10: i32 = 0x79;
    pub const VK_F11: i32 = 0x7A;
    pub const VK_F12: i32 = 0x7B;
    pub const VK_F13: i32 = 0x7C;
    pub const VK_F14: i32 = 0x7D;
    pub const VK_F15: i32 = 0x7E;
    pub const VK_F16: i32 = 0x7F;
    pub const VK_F17: i32 = 0x80;
    pub const VK_F18: i32 = 0x81;
    pub const VK_F19: i32 = 0x82;
    pub const VK_F20: i32 = 0x83;
    pub const VK_F21: i32 = 0x84;
    pub const VK_F22: i32 = 0x85;
    pub const VK_F23: i32 = 0x86;
    pub const VK_F24: i32 = 0x87;
    pub const VK_NUMLOCK: i32 = 0x90;
    pub const VK_SCROLL: i32 = 0x91;
    pub const VK_LSHIFT: i32 = 0xA0;
    pub const VK_RSHIFT: i32 = 0xA1;
    pub const VK_LCONTROL: i32 = 0xA2;
    pub const VK_RCONTROL: i32 = 0xA3;
    pub const VK_LMENU: i32 = 0xA4;
    pub const VK_RMENU: i32 = 0xA5;
    pub const VK_BROWSER_BACK: i32 = 0xA6;
    pub const VK_BROWSER_FORWARD: i32 = 0xA7;
    pub const VK_BROWSER_REFRESH: i32 = 0xA8;
    pub const VK_BROWSER_STOP: i32 = 0xA9;
    pub const VK_BROWSER_SEARCH: i32 = 0xAA;
    pub const VK_BROWSER_FAVORITES: i32 = 0xAB;
    pub const VK_BROWSER_HOME: i32 = 0xAC;
    pub const VK_VOLUME_MUTE: i32 = 0xAD;
    pub const VK_VOLUME_DOWN: i32 = 0xAE;
    pub const VK_VOLUME_UP: i32 = 0xAF;
    pub const VK_MEDIA_NEXT_TRACK: i32 = 0xB0;
    pub const VK_MEDIA_PREV_TRACK: i32 = 0xB1;
    pub const VK_MEDIA_STOP: i32 = 0xB2;
    pub const VK_MEDIA_PLAY_PAUSE: i32 = 0xB3;
    pub const VK_LAUNCH_MAIL: i32 = 0xB4;
    pub const VK_LAUNCH_MEDIA_SELECT: i32 = 0xB5;
    pub const VK_OEM_1: i32 = 0xBA;
    pub const VK_OEM_PLUS: i32 = 0xBB;
    pub const VK_OEM_COMMA: i32 = 0xBC;
    pub const VK_OEM_MINUS: i32 = 0xBD;
    pub const VK_OEM_PERIOD: i32 = 0xBE;
    pub const VK_OEM_2: i32 = 0xBF;
    pub const VK_OEM_3: i32 = 0xC0;
    pub const VK_OEM_4: i32 = 0xDB;
    pub const VK_OEM_5: i32 = 0xDC;
    pub const VK_OEM_6: i32 = 0xDD;
    pub const VK_OEM_7: i32 = 0xDE;
    pub const VK_OEM_102: i32 = 0xE2;
}

/// Translate a Win32 virtual-key code (`VK_*`) to a [`VirtualKeyCode`].
///
/// `None` means the engine has no virtual key for that code. The SCANCODE is
/// recorded regardless (see [`apply_win32_key_transition`]), so an unmapped key
/// is unlabelled rather than dead — but every shortcut and every key-filtered
/// callback is keyed on the `VirtualKeyCode`, so a missing entry is still a
/// feature that silently does not exist.
///
/// `oem_char` is the character the ACTIVE keyboard layout produces for `vkey`,
/// i.e. `MapVirtualKeyA(vkey, MAPVK_VK_TO_CHAR)`. It is the only way to tell
/// the seven layout-dependent `VK_OEM_1..VK_OEM_7` codes apart — `VK_OEM_1` is
/// `;` on a US layout and `ü` on a German one — and it is ignored for every
/// other code. `None` resolves those seven to nothing, which is what a
/// non-Windows caller (and this crate's own tests) get.
///
/// VK code list: <https://learn.microsoft.com/en-us/windows/win32/inputdev/virtual-key-codes>
///
/// Derived from winit's `platform_impl::windows::event`; the Apache-2.0 notice
/// it is distributed under is reproduced in full at the head of
/// `shell2/windows/win_event.rs`, where this table used to live.
#[must_use]
#[allow(clippy::too_many_lines)] // exhaustive keycode match table
// The VK code is the documentation here: merging the arms that answer the same
// VirtualKeyCode would hide WHICH codes reach it (the generic modifier codes vs
// the sided ones), which is the distinction MWA-A2 was about.
#[allow(clippy::match_same_arms)]
#[allow(clippy::wildcard_imports)] // the VK_* constant block, same as the X11 table's defines::*
pub fn win32_vkey_to_virtual_key(vkey: i32, oem_char: Option<char>) -> Option<VirtualKeyCode> {
    use win32_vk::*;

    match vkey {
        VK_BACK => Some(VirtualKeyCode::Back),
        VK_TAB => Some(VirtualKeyCode::Tab),
        VK_RETURN => Some(VirtualKeyCode::Return),
        VK_LSHIFT => Some(VirtualKeyCode::LShift),
        VK_RSHIFT => Some(VirtualKeyCode::RShift),
        VK_LCONTROL => Some(VirtualKeyCode::LControl),
        VK_RCONTROL => Some(VirtualKeyCode::RControl),
        VK_LMENU => Some(VirtualKeyCode::LAlt),
        VK_RMENU => Some(VirtualKeyCode::RAlt),
        // MWA-A2: WM_KEYDOWN/WM_KEYUP deliver the GENERIC modifier codes
        // (VK_SHIFT/VK_CONTROL/VK_MENU) unless the caller runs MapVirtualKey
        // on the scancode — dropping them meant ctrl_down() was NEVER true
        // on Windows and every Ctrl shortcut was dead. Map generic → left
        // variant (side doesn't matter for shortcut state).
        VK_SHIFT => Some(VirtualKeyCode::LShift),
        VK_CONTROL => Some(VirtualKeyCode::LControl),
        VK_MENU => Some(VirtualKeyCode::LAlt),
        VK_PAUSE => Some(VirtualKeyCode::Pause),
        VK_CAPITAL => Some(VirtualKeyCode::Capital),
        VK_KANA => Some(VirtualKeyCode::Kana),
        VK_KANJI => Some(VirtualKeyCode::Kanji),
        VK_ESCAPE => Some(VirtualKeyCode::Escape),
        VK_CONVERT => Some(VirtualKeyCode::Convert),
        VK_NONCONVERT => Some(VirtualKeyCode::NoConvert),
        VK_SPACE => Some(VirtualKeyCode::Space),
        VK_PRIOR => Some(VirtualKeyCode::PageUp),
        VK_NEXT => Some(VirtualKeyCode::PageDown),
        VK_END => Some(VirtualKeyCode::End),
        VK_HOME => Some(VirtualKeyCode::Home),
        VK_LEFT => Some(VirtualKeyCode::Left),
        VK_UP => Some(VirtualKeyCode::Up),
        VK_RIGHT => Some(VirtualKeyCode::Right),
        VK_DOWN => Some(VirtualKeyCode::Down),
        VK_SNAPSHOT => Some(VirtualKeyCode::Snapshot),
        VK_INSERT => Some(VirtualKeyCode::Insert),
        VK_DELETE => Some(VirtualKeyCode::Delete),
        0x30 => Some(VirtualKeyCode::Key0),
        0x31 => Some(VirtualKeyCode::Key1),
        0x32 => Some(VirtualKeyCode::Key2),
        0x33 => Some(VirtualKeyCode::Key3),
        0x34 => Some(VirtualKeyCode::Key4),
        0x35 => Some(VirtualKeyCode::Key5),
        0x36 => Some(VirtualKeyCode::Key6),
        0x37 => Some(VirtualKeyCode::Key7),
        0x38 => Some(VirtualKeyCode::Key8),
        0x39 => Some(VirtualKeyCode::Key9),
        0x41 => Some(VirtualKeyCode::A),
        0x42 => Some(VirtualKeyCode::B),
        0x43 => Some(VirtualKeyCode::C),
        0x44 => Some(VirtualKeyCode::D),
        0x45 => Some(VirtualKeyCode::E),
        0x46 => Some(VirtualKeyCode::F),
        0x47 => Some(VirtualKeyCode::G),
        0x48 => Some(VirtualKeyCode::H),
        0x49 => Some(VirtualKeyCode::I),
        0x4A => Some(VirtualKeyCode::J),
        0x4B => Some(VirtualKeyCode::K),
        0x4C => Some(VirtualKeyCode::L),
        0x4D => Some(VirtualKeyCode::M),
        0x4E => Some(VirtualKeyCode::N),
        0x4F => Some(VirtualKeyCode::O),
        0x50 => Some(VirtualKeyCode::P),
        0x51 => Some(VirtualKeyCode::Q),
        0x52 => Some(VirtualKeyCode::R),
        0x53 => Some(VirtualKeyCode::S),
        0x54 => Some(VirtualKeyCode::T),
        0x55 => Some(VirtualKeyCode::U),
        0x56 => Some(VirtualKeyCode::V),
        0x57 => Some(VirtualKeyCode::W),
        0x58 => Some(VirtualKeyCode::X),
        0x59 => Some(VirtualKeyCode::Y),
        0x5A => Some(VirtualKeyCode::Z),
        VK_LWIN => Some(VirtualKeyCode::LWin),
        VK_RWIN => Some(VirtualKeyCode::RWin),
        VK_APPS => Some(VirtualKeyCode::Apps),
        VK_SLEEP => Some(VirtualKeyCode::Sleep),
        VK_NUMPAD0 => Some(VirtualKeyCode::Numpad0),
        VK_NUMPAD1 => Some(VirtualKeyCode::Numpad1),
        VK_NUMPAD2 => Some(VirtualKeyCode::Numpad2),
        VK_NUMPAD3 => Some(VirtualKeyCode::Numpad3),
        VK_NUMPAD4 => Some(VirtualKeyCode::Numpad4),
        VK_NUMPAD5 => Some(VirtualKeyCode::Numpad5),
        VK_NUMPAD6 => Some(VirtualKeyCode::Numpad6),
        VK_NUMPAD7 => Some(VirtualKeyCode::Numpad7),
        VK_NUMPAD8 => Some(VirtualKeyCode::Numpad8),
        VK_NUMPAD9 => Some(VirtualKeyCode::Numpad9),
        VK_MULTIPLY => Some(VirtualKeyCode::NumpadMultiply),
        VK_ADD => Some(VirtualKeyCode::NumpadAdd),
        VK_SUBTRACT => Some(VirtualKeyCode::NumpadSubtract),
        VK_DECIMAL => Some(VirtualKeyCode::NumpadDecimal),
        VK_DIVIDE => Some(VirtualKeyCode::NumpadDivide),
        VK_F1 => Some(VirtualKeyCode::F1),
        VK_F2 => Some(VirtualKeyCode::F2),
        VK_F3 => Some(VirtualKeyCode::F3),
        VK_F4 => Some(VirtualKeyCode::F4),
        VK_F5 => Some(VirtualKeyCode::F5),
        VK_F6 => Some(VirtualKeyCode::F6),
        VK_F7 => Some(VirtualKeyCode::F7),
        VK_F8 => Some(VirtualKeyCode::F8),
        VK_F9 => Some(VirtualKeyCode::F9),
        VK_F10 => Some(VirtualKeyCode::F10),
        VK_F11 => Some(VirtualKeyCode::F11),
        VK_F12 => Some(VirtualKeyCode::F12),
        VK_F13 => Some(VirtualKeyCode::F13),
        VK_F14 => Some(VirtualKeyCode::F14),
        VK_F15 => Some(VirtualKeyCode::F15),
        VK_F16 => Some(VirtualKeyCode::F16),
        VK_F17 => Some(VirtualKeyCode::F17),
        VK_F18 => Some(VirtualKeyCode::F18),
        VK_F19 => Some(VirtualKeyCode::F19),
        VK_F20 => Some(VirtualKeyCode::F20),
        VK_F21 => Some(VirtualKeyCode::F21),
        VK_F22 => Some(VirtualKeyCode::F22),
        VK_F23 => Some(VirtualKeyCode::F23),
        VK_F24 => Some(VirtualKeyCode::F24),
        VK_NUMLOCK => Some(VirtualKeyCode::Numlock),
        VK_SCROLL => Some(VirtualKeyCode::Scroll),
        VK_BROWSER_BACK => Some(VirtualKeyCode::NavigateBackward),
        VK_BROWSER_FORWARD => Some(VirtualKeyCode::NavigateForward),
        VK_BROWSER_REFRESH => Some(VirtualKeyCode::WebRefresh),
        VK_BROWSER_STOP => Some(VirtualKeyCode::WebStop),
        VK_BROWSER_SEARCH => Some(VirtualKeyCode::WebSearch),
        VK_BROWSER_FAVORITES => Some(VirtualKeyCode::WebFavorites),
        VK_BROWSER_HOME => Some(VirtualKeyCode::WebHome),
        VK_VOLUME_MUTE => Some(VirtualKeyCode::Mute),
        VK_VOLUME_DOWN => Some(VirtualKeyCode::VolumeDown),
        VK_VOLUME_UP => Some(VirtualKeyCode::VolumeUp),
        VK_MEDIA_NEXT_TRACK => Some(VirtualKeyCode::NextTrack),
        VK_MEDIA_PREV_TRACK => Some(VirtualKeyCode::PrevTrack),
        VK_MEDIA_STOP => Some(VirtualKeyCode::MediaStop),
        VK_MEDIA_PLAY_PAUSE => Some(VirtualKeyCode::PlayPause),
        VK_LAUNCH_MAIL => Some(VirtualKeyCode::Mail),
        VK_LAUNCH_MEDIA_SELECT => Some(VirtualKeyCode::MediaSelect),
        VK_OEM_PLUS => Some(VirtualKeyCode::Equals),
        VK_OEM_COMMA => Some(VirtualKeyCode::Comma),
        VK_OEM_MINUS => Some(VirtualKeyCode::Minus),
        VK_OEM_PERIOD => Some(VirtualKeyCode::Period),
        // Windows does not distinguish these seven per layout: the code names
        // a POSITION on the keyboard and only the active layout says which
        // character sits there.
        VK_OEM_1 | VK_OEM_2 | VK_OEM_3 | VK_OEM_4 | VK_OEM_5 | VK_OEM_6 | VK_OEM_7 => {
            win32_oem_char_to_virtual_key(oem_char?)
        }
        VK_OEM_102 => Some(VirtualKeyCode::OEM102),
        _ => None,
    }
}

/// The [`VirtualKeyCode`] for the character an active Windows layout puts on one
/// of the seven `VK_OEM_1..VK_OEM_7` positions.
///
/// A character with no punctuation key of its own (`ü`, `ö`, `#` on a German
/// layout) answers `None`: the engine has no code for it, and the text still
/// reaches the app through `WM_CHAR`, which never consults this table.
#[must_use]
pub fn win32_oem_char_to_virtual_key(oem_char: char) -> Option<VirtualKeyCode> {
    match oem_char {
        ';' => Some(VirtualKeyCode::Semicolon),
        '/' => Some(VirtualKeyCode::Slash),
        '`' => Some(VirtualKeyCode::Grave),
        '[' => Some(VirtualKeyCode::LBracket),
        ']' => Some(VirtualKeyCode::RBracket),
        '\'' => Some(VirtualKeyCode::Apostrophe),
        '\\' => Some(VirtualKeyCode::Backslash),
        _ => None,
    }
}

/// Write ONE pointer button transition into a [`MouseState`], together with the
/// position it happened at.
///
/// Only the button that actually changed is touched: assigning all three from a
/// `button == …` comparison clears the others, and pressing Right while Left is
/// held then synthesizes a phantom `LeftMouseUp` that kills drags and text
/// selections mid-gesture — the Wayland finding, which is why this is a shared
/// helper rather than three hand-written assignments per backend.
///
/// `MouseButton::Other` (thumb back/forward, buttons 4/5) records only the
/// position: [`MouseState`] has no field for them, so no backend derives a
/// MouseDown/MouseUp from those buttons.
///
/// [`MouseState`]: azul_core::window::MouseState
pub fn apply_pointer_button_state(
    mouse_state: &mut azul_core::window::MouseState,
    position: LogicalPosition,
    button: azul_core::events::MouseButton,
    is_down: bool,
) {
    use azul_core::{events::MouseButton, window::CursorPosition};

    mouse_state.cursor_position = CursorPosition::InWindow(position);
    match button {
        MouseButton::Left => mouse_state.left_down = is_down,
        MouseButton::Right => mouse_state.right_down = is_down,
        MouseButton::Middle => mouse_state.middle_down = is_down,
        _ => {}
    }
}

/// The `MouseButton` a `WM_XBUTTONDOWN` / `WM_XBUTTONUP` names, from its WPARAM.
///
/// Unlike every other `WM_*BUTTON*` message, the X-button messages carry the
/// button in the HIGH word of `wParam` (`XBUTTON1` = 1 = thumb "back",
/// `XBUTTON2` = 2 = thumb "forward") and the low word holds the modifier keys.
/// Reading `wParam` whole therefore names the wrong button as soon as Shift or
/// Ctrl is held.
///
/// Numbering follows AppKit's `buttonNumber` — 0 left, 1 right, 2 middle,
/// 3 back, 4 forward — so the same physical thumb button reports the same
/// `Other(n)` on macOS and Win32. (X11 reports its own button numbers, 8 and 9,
/// and is not aligned with either.) `None` is a button no Windows version
/// defines: the high word is documented as `XBUTTON1` or `XBUTTON2` and
/// nothing else.
#[must_use]
pub fn win32_xbutton_to_mouse_button(wparam: usize) -> Option<azul_core::events::MouseButton> {
    use azul_core::events::MouseButton;

    const XBUTTON1: u16 = 0x0001;
    const XBUTTON2: u16 = 0x0002;

    match ((wparam >> 16) & 0xFFFF) as u16 {
        XBUTTON1 => Some(MouseButton::Other(3)),
        XBUTTON2 => Some(MouseButton::Other(4)),
        _ => None,
    }
}

/// Apply one Win32 key transition to a [`KeyboardState`].
///
/// `virtual_key` is `None` for a key `vkey_to_winit_vkey` has no entry for — a
/// media key, the browser cluster, an OEM key on a non-US layout. The SCANCODE
/// is written either way, which is the whole point: it names the PHYSICAL key
/// and needs no translation table to be true. Gating its write on the
/// translation (what the Win32 backend did) left every unmapped key missing
/// from `pressed_scancodes` — and, because the release was gated away by the
/// same test, a key that DID make it in could never come back out.
///
/// `current_virtual_keycode` is deliberately left ALONE for an unmapped key.
/// `determine_all_events` derives `KeyUp` from `previous.is_some() &&
/// current.is_none()`, so writing `None` there would fire a KeyUp for whatever
/// key is still physically HELD the moment the user touches a key the table
/// does not know — a spurious release in the middle of a two-key rollover.
/// Nothing is derived from `pressed_scancodes`, so recording the physical key
/// is purely additive: the pass sees a keyboard delta, consumes it, and emits
/// no event.
///
/// [`KeyboardState`]: azul_core::window::KeyboardState
pub fn apply_win32_key_state_change(
    keyboard_state: &mut azul_core::window::KeyboardState,
    virtual_key: Option<VirtualKeyCode>,
    scan_code: u32,
    is_down: bool,
) {
    use azul_core::window::OptionVirtualKeyCode;

    if is_down {
        if let Some(vk) = virtual_key {
            keyboard_state.current_virtual_keycode = OptionVirtualKeyCode::Some(vk);
            keyboard_state.pressed_virtual_keycodes.insert_hm_item(vk);
        }
        keyboard_state.pressed_scancodes.insert_hm_item(scan_code);
    } else {
        if let Some(vk) = virtual_key {
            keyboard_state.current_virtual_keycode = OptionVirtualKeyCode::None;
            keyboard_state.pressed_virtual_keycodes.remove_hm_item(&vk);
        }
        keyboard_state.pressed_scancodes.remove_hm_item(&scan_code);
    }
}

/// Everything the OS-side IME state is a function of: whether the window has
/// the focus, WHICH node is being edited, and where the caret sits inside it.
///
/// A backend keeps the last one it pushed and re-pushes only on a change, so
/// the sync can be called from every event pass and every frame without the
/// caret-rect walk running 60 times a second for nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImeSyncKey {
    /// The IME only engages for the focused window.
    pub window_focused: bool,
    /// `(dom index, node index)` of the editing session, if any.
    pub editing_node: Option<(usize, usize)>,
    /// Where the over-the-spot candidate window belongs.
    pub cursor: Option<azul_core::selection::TextCursor>,
}

/// Read the live [`ImeSyncKey`] off a window.
///
/// Kept next to the key so every backend derives it from the same three
/// sources. Focus has to be one of them: without it a blur would leave the key
/// unchanged and the IME engaged for a window that no longer has the keyboard.
#[must_use]
pub fn ime_sync_key(window_focused: bool, layout_window: Option<&LayoutWindow>) -> ImeSyncKey {
    ImeSyncKey {
        window_focused,
        editing_node: layout_window.and_then(|lw| {
            let dom = lw.text_edit_manager.get_editing_dom_id()?;
            let node = lw.text_edit_manager.get_editing_node_id()?;
            Some((dom.inner, node.index()))
        }),
        cursor: layout_window.and_then(|lw| lw.text_edit_manager.get_primary_cursor()),
    }
}

/// App-global undo/redo manager handle, shared (Arc) between the App and every
/// window. The actual mini-git manager only exists under the `json` feature;
/// without it every method is a no-op so the type can be threaded unconditionally.
#[derive(Clone, Debug)]
pub struct SharedUndoManager {
    #[cfg(feature = "json")]
    inner: std::sync::Arc<std::sync::Mutex<azul_layout::json::RefAnyUndoManager>>,
}

impl SharedUndoManager {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "json")]
            inner: std::sync::Arc::new(std::sync::Mutex::new(
                azul_layout::json::RefAnyUndoManager::new(0),
            )),
        }
    }

    /// Snapshot current app state into history. No-op (returns false) without json.
    pub fn commit(&self, _state: &azul_core::refany::RefAny) -> bool {
        #[cfg(feature = "json")]
        {
            self.inner.lock().map(|mut m| m.commit(_state)).unwrap_or(false)
        }
        #[cfg(not(feature = "json"))]
        {
            false
        }
    }

    /// Undo the last committed app-state change. No-op (returns false) without json.
    pub fn undo(&self, _state: &mut azul_core::refany::RefAny) -> bool {
        #[cfg(feature = "json")]
        {
            self.inner.lock().map(|mut m| m.undo(_state)).unwrap_or(false)
        }
        #[cfg(not(feature = "json"))]
        {
            false
        }
    }

    /// Redo a previously undone app-state change. No-op (returns false) without json.
    pub fn redo(&self, _state: &mut azul_core::refany::RefAny) -> bool {
        #[cfg(feature = "json")]
        {
            self.inner.lock().map(|mut m| m.redo(_state)).unwrap_or(false)
        }
        #[cfg(not(feature = "json"))]
        {
            false
        }
    }
}

impl Default for SharedUndoManager {
    fn default() -> Self {
        Self::new()
    }
}

/// A one-bit "somebody asked for this" request that survives being serviced.
///
/// A bare `bool` cannot distinguish "nobody asked" from "somebody asked while
/// we were busy answering the last ask". That distinction is the whole problem
/// here: every frame path in this shell ends by clearing the request it was
/// serving, and the work in between runs USER CALLBACKS that can raise the very
/// same request again. A bare `= false` at the end silently eats those.
///
/// So the bit travels with a monotonic generation counter:
///
/// ```ignore
/// let seen = req.epoch();      // BEFORE the work
/// do_the_work();               // may call req.raise()
/// req.retire_unless_reraised(seen);  // clears ONLY what `seen` covered
/// ```
///
/// The counter also means the only way to clear the bit is to name an epoch,
/// which is much harder to do by accident than typing `= false`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LatchedRequest {
    set: bool,
    generation: u64,
}

impl LatchedRequest {
    /// A request that is already outstanding (a window that owes its first frame).
    #[must_use]
    pub fn raised() -> Self {
        Self {
            set: true,
            generation: 0,
        }
    }

    /// Ask. Always bumps the generation, even if the bit was already set — the
    /// counter's job is to record that an ask HAPPENED, not that the state
    /// changed.
    pub fn raise(&mut self) {
        self.set = true;
        self.generation = self.generation.wrapping_add(1);
    }

    /// Is a request outstanding? Read-only on purpose: a gate that only wants
    /// to know must not be able to consume what it is asking about.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.set
    }

    /// The epoch to capture BEFORE doing the work this request asked for.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.generation
    }

    /// Retire the request — but only the one identified by `seen`. If the
    /// counter moved while the work ran, somebody asked again and the bit
    /// STAYS SET so the next turn of the loop services it.
    pub fn retire_unless_reraised(&mut self, seen: u64) {
        if self.generation == seen {
            self.set = false;
        }
    }

    /// Take the request outright: returns whether one was outstanding and
    /// clears it unconditionally.
    ///
    /// ONLY for callers where no new request can arrive between the take and
    /// the work — i.e. nothing in between runs a user callback. Anything that
    /// goes through `regenerate_layout()` must use `epoch()` +
    /// `retire_unless_reraised()` instead, because that path runs lifecycle
    /// callbacks and they DO raise new requests.
    #[must_use]
    pub fn take(&mut self) -> bool {
        std::mem::replace(&mut self.set, false)
    }
}

/// The pending "rebuild this window's DOM before the next frame" request.
///
/// The three pieces live together and are PRIVATE on purpose. A census of the
/// previous shape — a bare `pub frame_needs_regeneration: bool` plus a bare
/// `pub next_relayout_reason` beside it — found **91 places that set the bool
/// against 18 that cleared it**, three of those clears unreachable and two
/// doing no work at all. At that ratio a flag stops being a signal and becomes
/// a hazard: every producer had to remember to also tag the reason, and every
/// consumer had to remember not to erase a request raised while it rendered.
///
/// Now there is exactly one way in — [`CommonWindowState::request_regeneration`],
/// which cannot be called without saying WHY — and the ways out all go through
/// [`LatchedRequest`], so a `= false` that eats a mid-flight request is not
/// expressible.
#[derive(Debug, Clone, Copy)]
pub struct RegenerationState {
    /// "The DOM must be rebuilt", latched so a render cannot eat a request
    /// that arrived while it was running.
    request: LatchedRequest,
    /// Why, forwarded to the user's `LayoutCallback` via
    /// `LayoutCallbackInfo::relayout_reason()`. Consumed (and reset to the
    /// implicit `RefreshDom`) by the `regenerate_layout()` call it describes.
    reason: azul_core::callbacks::RelayoutReason,
    /// When `true`, layout is ALREADY up to date — an `incremental_relayout()`
    /// re-ran layout on the existing `StyledDom` (restyle / runtime edit) in the
    /// event arm, so the frame-generation path must SKIP the full
    /// `regenerate_layout()` (which would re-invoke the user's `layout_callback`
    /// and rebuild the `StyledDom`) and only rebuild + send the WebRender
    /// transaction. All four desktop frame paths honor it — windows
    /// (`WM_PAINT`), wayland (`generate_frame_if_needed`), macOS
    /// (`build_atomic_txn`) and x11 (`render_and_present`) check it BEFORE the
    /// plain regeneration request and take the transaction-only path.
    relayout_only: bool,
    /// "The window size changed and layout must re-run on the EXISTING
    /// `StyledDom` at the new size" — the resize fast path.
    ///
    /// Distinct from [`Self::relayout_only`] in both directions of time:
    /// `relayout_only` means the incremental relayout ALREADY ran (the scroll /
    /// restyle arms run it inline, then set the flag so the frame path only
    /// sends the transaction); `resize_relayout` means it has NOT run yet —
    /// the frame path runs it exactly once when it consumes the flag. That
    /// deferral is the COALESCING: a mouse drag delivers one configure per
    /// pixel (373 in a measured 5 s drag), and running the relayout inline per
    /// configure would relayout at every intermediate size. Latching and
    /// consuming per-frame means any number of configures between two frames
    /// cost ONE relayout, at the latest size.
    ///
    /// Deliberately NOT raised through [`LatchedRequest`]/`request`: the
    /// regeneration request means "rebuild the DOM", and the entire point of
    /// this flag is that the DOM does NOT need rebuilding (no CSS breakpoint
    /// crossed, no recorded window-size query answer flipped). A concurrent
    /// REAL regeneration request (app callback returned `RefreshDom`, a
    /// breakpoint-crossing resize) takes precedence in every frame path: the
    /// full rebuild lays out at the current (new) size anyway, so this flag is
    /// consumed and dropped alongside it.
    resize_relayout: bool,
}

impl RegenerationState {
    /// A window that has not laid out yet: the first frame is already owed and
    /// the user's `layout()` must see `RelayoutReason::Initial`.
    #[must_use]
    pub fn pending_initial() -> Self {
        Self {
            request: LatchedRequest::raised(),
            reason: azul_core::callbacks::RelayoutReason::Initial,
            relayout_only: false,
            resize_relayout: false,
        }
    }

    /// A window whose first frame is driven by a platform paint message
    /// (`WM_PAINT`, `drawRect:`, the first xdg `configure`) rather than by this
    /// request. The reason stays `Initial` — whoever asks first is still asking
    /// for the first layout.
    #[must_use]
    pub fn idle_initial() -> Self {
        Self {
            request: LatchedRequest::default(),
            reason: azul_core::callbacks::RelayoutReason::Initial,
            relayout_only: false,
            resize_relayout: false,
        }
    }
}

/// Which incremental relayout a backend asks
/// [`CommonWindowState::incremental_relayout`] for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncrementalRelayout {
    /// A restyle / runtime edit / scroll-driven pass on the existing
    /// StyledDom: solver3's reconcile diffs the node fingerprints, which is
    /// what classifies paint-dirt for the partial present.
    Restyle,
    /// The coalesced window-resize pass: the StyledDom is by construction the
    /// same object with zero DOM/style dirt, so solver3 may keep its retained
    /// tree as-is (`resize_only_hint`) instead of re-walking every node to
    /// rediscover full reuse. ONLY for the `take_resize_relayout()` branches.
    Resize,
}

pub struct CommonWindowState {
    /// LayoutWindow integration (for UI callbacks and display list)
    pub layout_window: Option<LayoutWindow>,
    /// The live window state. PRIVATE, and the reason is the pair of invariants
    /// nothing else can enforce:
    ///
    ///   * every write has to decide whether `sync_window_state()` should push
    ///     it at the window system, and
    ///   * every write to a field `determine_all_events` diffs owes the
    ///     snapshot → mutate → pass shape.
    ///
    /// While it was `pub`, 166 sites wrote it directly (or through the
    /// `&mut FullWindowState` the `PlatformWindow` trait used to hand out)
    /// against 25 that went through [`Self::update_window_state`], so both
    /// invariants were advisory.
    /// Read it with [`Self::current_window_state`]; write it with
    /// [`Self::update_window_state`] (the fields the sync diffs — `title`,
    /// `size`, `position`, `flags`), [`Self::update_unsynced_state`] (the ones
    /// it never looks at), or the three input accessors
    /// ([`Self::mouse_state_mut`] and friends).
    current_window_state: FullWindowState,
    /// The EVENT-DIFF baseline: the state the last completed
    /// [`PlatformWindow::process_window_events`] pass consumed.
    ///
    /// `determine_all_events` derives WindowResize / WindowMove / DpiChanged /
    /// every flag transition purely from `previous` vs `current`, so this field
    /// has exactly ONE writer — the end of a pass (plus the pre-mutation
    /// snapshot every injection site takes). It is NOT the OS sync baseline;
    /// see [`Self::os_synced_state`].
    pub previous_window_state: Option<FullWindowState>,
    /// The OS-SYNC baseline: what the window system last confirmed it has.
    ///
    /// `sync_window_state()` pushes the diff between this and
    /// `current_window_state` to the OS (`XMoveWindow` / `SetWindowPos` /
    /// `setFrameTopLeftPoint` / …) and then advances it — see
    /// [`Self::take_os_sync_diff`].
    ///
    /// It exists because `previous_window_state` cannot serve both roles:
    /// suppressing an OS echo means "make the diff zero", and event
    /// determination reads the same diff to decide whether a `Resized` /
    /// `DpiChanged` / maximize transition happened. One field could satisfy
    /// only one of the two, and the echo-suppression writer won — which is
    /// why OS-reported resizes, DPI changes and frame-flag changes never
    /// reached a single user callback on any backend.
    ///
    /// `None` until the window has been shown and the baseline seeded
    /// (`mark_os_synced()`), which is also what makes the first frame a no-op.
    pub os_synced_state: Option<FullWindowState>,
    // NOTE: there is deliberately NO `image_cache` here. The css-id image map
    // has a single owner — `LayoutWindow::image_cache` — and a single writer,
    // `LayoutWindow::apply_content_change` (the shell copy used to be mirrored
    // at two mutation points and read by a different relayout path than the
    // one the layout used: the two could disagree).
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
    /// App-global undo/redo manager, shared across all windows (owned by the App).
    /// A callback's `commit_undo_snapshot` / `undo_app_state` / `redo_app_state`
    /// drives this; undo/redo relayouts all windows.
    pub undo_manager: SharedUndoManager,
    /// Current scrollbar drag state (if dragging a scrollbar thumb)
    pub scrollbar_drag_state: Option<ScrollbarDragState>,
    /// Hit-tester for fast asynchronous hit-testing (updated on layout changes).
    /// `None` only during initialization on X11/Wayland before WebRender is set up.
    /// Not used in CPU mode — see `cpu_hit_tester` instead.
    pub hit_tester: Option<AsyncHitTester>,
    /// CPU-based hit tester for AZ_BACKEND=cpu mode.
    /// Rebuilt from layout results after each layout pass. Works without WebRender.
    pub cpu_hit_tester: Option<azul_layout::headless::CpuHitTester>,
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
    /// The pending "rebuild this window's DOM" request — the flag, the reason
    /// it carries, and the generation counter that keeps a mid-render request
    /// alive. Its fields are PRIVATE; see [`RegenerationState`] for why, and go
    /// through [`CommonWindowState::request_regeneration`] /
    /// [`CommonWindowState::take_regeneration`] / the epoch pair.
    pub regen: RegenerationState,
    /// Whether a WebRender display list has ever been sent for this window.
    /// Used to force a full display list build on the very first frame, even if
    /// regenerate_layout() returns LayoutUnchanged (because create_window already
    /// ran regenerate_layout for accessibility/font init).
    pub display_list_initialized: bool,
    /// Whether the display list was updated internally (e.g. by text editing)
    /// and needs to be sent to WebRender without a full DOM rebuild.
    pub display_list_dirty: bool,
    /// Whether the accessibility tree needs to be rebuilt and sent to the OS.
    /// Set on focus change, DOM rebuild, text edit — NOT on every mouse move.
    pub a11y_dirty: bool,
}

impl CommonWindowState {
    /// Ask for this window's DOM to be rebuilt before the next frame, and say
    /// WHY.
    ///
    /// This is the ONLY way to raise the request. The reason is not optional
    /// because it used to be a second, separate field that producers kept
    /// forgetting: a resize handler that set the flag but not the tag left the
    /// user's `layout()` callback seeing a phantom `RefreshDom` and unable to
    /// tell a breakpoint change from an app-state change.
    ///
    /// `RefreshDom` is the IMPLICIT tag — "something changed, rebuild" — and the
    /// tag most call sites carry. It deliberately does NOT overwrite a more
    /// specific reason (`Initial`, `Resize`, `ThemeChange`, `RouteChange`) that
    /// another producer already queued for the same, not-yet-serviced
    /// regeneration. That is exactly what the old two-field code did by simply
    /// not touching `next_relayout_reason`, and losing it would mean a plain
    /// redraw request arriving after a resize could downgrade the resize.
    /// Release the WebRender renderer the way WebRender requires.
    ///
    /// `Renderer::deinit(self)` is NOT optional and is NOT a `Drop`. Its own
    /// comment says why:
    ///
    /// ```text
    /// //Note: this is a fake frame, only needed because texture deletion is
    /// // require to happen inside a frame
    /// self.device.begin_frame();
    /// ```
    ///
    /// azul never called it — `grep -rn '\.deinit()'` over dll/, layout/ and
    /// core/ returned nothing — so every window dropped its `Renderer`
    /// implicitly and released textures and depth targets outside a frame. In a
    /// DEBUG build that trips `SharedDepthTarget::drop`'s
    /// `debug_assert!(thread::panicking() || self.refcount == 0)`
    /// (webrender/core/src/device/gl.rs:1019) and the process dies; that
    /// assertion is `#[cfg(debug_assertions)]`, so a release build stays quiet
    /// and leaks instead.
    ///
    /// Idempotent: takes the renderer out, so a second call is a no-op. Safe to
    /// call from both `close()` and `Drop`.
    pub fn deinit_renderer(&mut self) {
        if let Some(renderer) = self.renderer.take() {
            renderer.deinit();
        }
    }

    pub fn request_regeneration(&mut self, reason: azul_core::callbacks::RelayoutReason) {
        self.regen.request.raise();
        if reason != azul_core::callbacks::RelayoutReason::RefreshDom {
            self.regen.reason = reason;
        }
    }

    /// Is a DOM rebuild outstanding? Read-only: a gate cannot consume the
    /// request it is only asking about.
    #[must_use]
    pub fn regeneration_pending(&self) -> bool {
        self.regen.request.pending()
    }

    /// The reason a pending regeneration carries, without consuming it.
    #[must_use]
    pub fn regeneration_reason(&self) -> azul_core::callbacks::RelayoutReason {
        self.regen.reason
    }

    /// Consume the reason for the `regenerate_layout()` call about to run, and
    /// reset it to the implicit `RefreshDom` so a later untagged regeneration
    /// does not inherit a stale `Resize`/`RouteChange`.
    pub fn take_relayout_reason(&mut self) -> azul_core::callbacks::RelayoutReason {
        std::mem::replace(
            &mut self.regen.reason,
            azul_core::callbacks::RelayoutReason::RefreshDom,
        )
    }

    /// The regeneration epoch to capture BEFORE rendering.
    #[must_use]
    pub fn regen_epoch(&self) -> u64 {
        self.regen.request.epoch()
    }

    /// Retire a regeneration request — but ONLY the one that was observed.
    ///
    /// Every backend used to end its frame with a bare
    /// `frame_needs_regeneration = false`, which erased anything raised while
    /// that frame was being produced. And things ARE raised then:
    /// `regenerate_layout` runs user lifecycle callbacks, and a callback
    /// returning `Update::RefreshDom` routes through `process_window_events` into
    /// `request_regeneration`. The symptom is a widget that seeds derived
    /// state on mount showing its pre-seed DOM until some unrelated later event
    /// happens to force another pass.
    ///
    /// Pass the epoch captured before the work. If the counter moved, somebody
    /// asked again mid-flight and the request STAYS SET.
    pub fn clear_regeneration_unless_reraised(&mut self, seen: u64) {
        self.regen.request.retire_unless_reraised(seen);
    }

    /// Take the regeneration request outright — returns whether one was
    /// pending and clears it.
    ///
    /// ONLY for consumers with no mid-flight window: nothing between the take
    /// and the work may run a user callback. A path that calls
    /// `regenerate_layout()` MUST use [`Self::regen_epoch`] +
    /// [`Self::clear_regeneration_unless_reraised`] instead — that path runs
    /// lifecycle callbacks and they raise new requests.
    #[must_use]
    pub fn take_regeneration(&mut self) -> bool {
        self.regen.request.take()
    }

    /// Ask for the "layout is already up to date on the EXISTING StyledDom"
    /// fast path: re-run layout on that DOM, do NOT rebuild it.
    ///
    /// This ALSO raises the ordinary regeneration request, and that is not
    /// redundant — it is the fix for a real stall. The frame gates
    /// (`X11::poll_event`, wayland's `frame_done_callback`) ask "is a frame
    /// owed?" by testing the regeneration request, so a lone `relayout_only`
    /// was invisible to them: the work sat queued until some unrelated event
    /// happened to redraw. Every frame path checks `relayout_only_pending()`
    /// FIRST, so raising both cannot cause a spurious DOM rebuild.
    pub fn request_relayout_only(&mut self) {
        self.regen.relayout_only = true;
        self.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
    }

    /// Is the relayout-only fast path requested? Read-only.
    #[must_use]
    pub fn relayout_only_pending(&self) -> bool {
        self.regen.relayout_only
    }

    /// Ask for the RESIZE fast path: the window size changed, no CSS
    /// breakpoint was crossed and no recorded window-size query answer flipped,
    /// so `layout()` provably cannot produce a different DOM — re-run layout
    /// on the EXISTING `StyledDom` at the new size instead.
    ///
    /// Latched, consumed once per frame by [`Self::take_resize_relayout`] —
    /// see `RegenerationState::resize_relayout` for why that deferral (the
    /// coalescing of one-configure-per-pixel drags into one relayout per
    /// frame) is the point. Callers must ALSO schedule a frame through their
    /// backend's own mechanism (`request_redraw` / needs_redraw / invalidate);
    /// this flag deliberately does not raise the DOM-rebuild request.
    pub fn request_resize_relayout(&mut self) {
        self.regen.resize_relayout = true;
    }

    /// Is the resize fast path requested? Read-only, for frame gates.
    #[must_use]
    pub fn resize_relayout_pending(&self) -> bool {
        self.regen.resize_relayout
    }

    /// Consume the resize fast-path request.
    #[must_use]
    pub fn take_resize_relayout(&mut self) -> bool {
        core::mem::take(&mut self.regen.resize_relayout)
    }

    /// Decide how a size change regenerates, and request it.
    ///
    /// THE resize policy (user ruling, 2026-08-08): a resize NEVER re-invokes
    /// the app's `layout()` — except when it could observably change what that
    /// callback returns, which is exactly:
    ///
    ///   1. a recorded window-size query answer flips (`window_width_less_than` & co.)
    ///      (the sanctioned imperative channel — see
    ///      `LayoutCallbackInfo::window_width_less_than` & co.),
    ///   2. a `CSS_BREAKPOINTS` threshold or the orientation is crossed
    ///      (the declarative `@media` channel), or
    ///   3. there is nothing to reuse (no previous layout).
    ///
    /// Everything else takes `resize -> relayout (existing StyledDom, warm
    /// solver3 caches) -> new display list -> repaint`, coalesced to one
    /// relayout per frame.
    ///
    /// Returns `true` when the FULL path was requested, so callers that log or
    /// branch further can tell which way it went.
    pub fn request_regeneration_for_resize(
        &mut self,
        old_logical: azul_core::geom::LogicalSize,
        new_logical: azul_core::geom::LogicalSize,
    ) -> bool {
        let full = self.resize_needs_full_regeneration(old_logical, new_logical);
        if full {
            self.request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
        } else {
            self.request_resize_relayout();
        }
        full
    }

    /// The decision behind [`Self::request_regeneration_for_resize`], without
    /// the side effect.
    #[must_use]
    pub fn resize_needs_full_regeneration(
        &self,
        old_logical: azul_core::geom::LogicalSize,
        new_logical: azul_core::geom::LogicalSize,
    ) -> bool {
        // Nothing to reuse: the first layout has not happened.
        let Some(layout_window) = self.layout_window.as_ref() else {
            return true;
        };
        // The shared engine policy — the same fn the headless E2E runner
        // calls, so the corpus tests exactly what the shells run.
        layout_window.resize_needs_full_regeneration(old_logical, new_logical)
    }

    /// Consume the relayout-only request.
    #[must_use]
    pub fn take_relayout_only(&mut self) -> bool {
        std::mem::replace(&mut self.regen.relayout_only, false)
    }
}

impl CommonWindowState {
    /// Seed a window's common state from the `FullWindowState` it is created
    /// with. Everything a backend cannot know yet — the renderer, the hit
    /// tester, the document/namespace ids — starts empty and is assigned
    /// afterwards through the `pub` fields; `layout_window` and `regen` are the
    /// two the backends always override.
    ///
    /// This exists because `current_window_state` is private, so the 22-field
    /// struct literal each backend used to write is no longer expressible
    /// outside this module. Both baselines start `None`: `previous` because no
    /// pass has run, `os_synced` because nothing has been shown yet — which is
    /// what makes the first `sync_window_state()` a no-op instead of a burst of
    /// redundant geometry calls (see [`Self::mark_os_synced`]).
    #[must_use]
    pub fn new(
        current_window_state: FullWindowState,
        fc_cache: Arc<FcFontCache>,
        system_style: Arc<azul_css::system::SystemStyle>,
        app_data: Arc<RefCell<RefAny>>,
        undo_manager: SharedUndoManager,
    ) -> Self {
        Self {
            layout_window: None,
            current_window_state,
            previous_window_state: None,
            os_synced_state: None,
            renderer_resources: RendererResources::default(),
            fc_cache,
            gl_context_ptr: OptionGlContextPtr::None,
            system_style,
            app_data,
            undo_manager,
            scrollbar_drag_state: None,
            hit_tester: None,
            cpu_hit_tester: None,
            last_hovered_node: None,
            document_id: None,
            id_namespace: None,
            render_api: None,
            renderer: None,
            regen: RegenerationState::pending_initial(),
            display_list_initialized: false,
            display_list_dirty: false,
            a11y_dirty: true,
        }
    }

    /// Everything a layout pass needs from here, as disjoint borrows — see
    /// [`LayoutPassBorrows`].
    pub fn layout_borrows(&mut self) -> LayoutPassBorrows<'_> {
        LayoutPassBorrows {
            layout_window: self.layout_window.as_mut(),
            current_window_state: &self.current_window_state,
            previous_window_state: &self.previous_window_state,
            renderer_resources: &mut self.renderer_resources,
            gl_context_ptr: &self.gl_context_ptr,
            fc_cache: &self.fc_cache,
            system_style: &self.system_style,
            app_data: &self.app_data,
        }
    }

    /// Re-run layout on the EXISTING StyledDom — no DOM rebuild, the user's
    /// layout callback is NOT invoked — and then run the finalize tail that
    /// no relayout may skip.
    ///
    /// THE TAIL IS THE POINT. Layout results the hit-tester does not know
    /// about are worse than no relayout: the window *looks* right, but every
    /// click and wheel over a node that moved goes to whatever used to be
    /// there. That was the "scroll area is dead after a resize until I click
    /// a widget" and the AzMap "+" bug: the coalesced resize fast path on
    /// macOS and X11 called the bare layout function and nothing rebuilt the
    /// CPU hit-tester until the next FULL `regenerate_layout()`. Windows,
    /// Wayland and headless happened to rebuild it themselves — so the bug
    /// only existed on the platforms the tests do not run on.
    ///
    /// The bare functions (`common::layout::incremental_relayout` and
    /// `_for_resize`) are now `pub(super)`: a backend cannot reach them, so
    /// an incremental relayout without this tail does not compile.
    ///
    /// A window without a layout window (still initialising) is a no-op,
    /// like the call sites this replaced.
    ///
    /// Private to `common`: backends call
    /// [`PlatformWindow::incremental_relayout_dispatching`], which adds the
    /// lifecycle-event delivery (`NodeResized`) on top.
    pub(in crate::desktop::shell2::common) fn incremental_relayout(
        &mut self,
        kind: IncrementalRelayout,
        debug_messages: &mut Option<Vec<azul_css::LayoutDebugMessage>>,
    ) -> Result<(), String> {
        {
            let borrows = self.layout_borrows();
            let Some(layout_window) = borrows.layout_window else {
                return Ok(());
            };
            match kind {
                IncrementalRelayout::Restyle => super::layout::incremental_relayout(
                    layout_window,
                    borrows.current_window_state,
                    borrows.renderer_resources,
                    debug_messages,
                )?,
                IncrementalRelayout::Resize => super::layout::incremental_relayout_for_resize(
                    layout_window,
                    borrows.current_window_state,
                    borrows.renderer_resources,
                    debug_messages,
                )?,
            }
        }
        self.rebuild_cpu_hit_tester();
        Ok(())
    }

    /// Drain the queued `VirtualView` re-invocations (see
    /// [`super::layout::drain_virtual_view_updates`]) — re-invoke each view in
    /// place AND rebuild the CPU hit-tester when any child DOM was rebuilt.
    /// Returns whether any view was rebuilt. Every CPU frame path calls this
    /// before it paints; the GPU paths drain inside `generate_frame`.
    pub fn drain_virtual_view_updates(&mut self) -> bool {
        match self.layout_window.as_mut() {
            Some(lw) => super::layout::drain_virtual_view_updates(lw, self.cpu_hit_tester.as_mut()),
            None => false,
        }
    }

    /// Rebuild the CPU hit-tester from the current layout results.
    ///
    /// CPU backend only: under WebRender the field is `None` and the
    /// WebRender hit-tester is refreshed by the next display-list
    /// transaction. Called by [`Self::incremental_relayout`] and by every
    /// backend's full `regenerate_layout()` tail — the hit-tester is a CACHE
    /// of `layout_results`, and a cache that outlives the layout it was
    /// built from sends input to nodes that are no longer there.
    pub fn rebuild_cpu_hit_tester(&mut self) {
        if let (Some(cpu_ht), Some(lw)) =
            (self.cpu_hit_tester.as_mut(), self.layout_window.as_ref())
        {
            cpu_ht.rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager));
        }
    }

    /// Read the live window state.
    ///
    /// The field itself is private: every write goes through
    /// [`Self::update_window_state`] (the fields `sync_window_state()` diffs)
    /// or [`Self::update_unsynced_state`] (the fields it never looks at).
    #[must_use]
    pub fn current_window_state(&self) -> &FullWindowState {
        &self.current_window_state
    }

    /// Apply a window-state mutation, tagged with its [`WindowStateSource`].
    ///
    /// This is the single entry point for changing `current_window_state` in a
    /// way `sync_window_state()` is aware of:
    ///   * [`App`](WindowStateSource::App) — mutates `current` only, so the
    ///     `current` vs `os_synced_state` diff makes `sync_window_state()` push
    ///     it to the OS (`XMoveWindow`/`SetWindowPos`/…).
    ///   * [`Os`](WindowStateSource::Os) — the change is *already applied* by the
    ///     OS, so this mutates `current` AND advances the OS-sync baseline
    ///     (`os_synced_state`) in lockstep, leaving a zero diff so it is never
    ///     echoed. Echoing OS-reported geometry is what drifted the window on
    ///     reparenting WMs (F4).
    ///
    /// It NEVER writes `previous_window_state`. It used to, back when one field
    /// was both baselines, and that is precisely what killed the events: the
    /// OS reports a resize, the echo suppression zeroes the previous→current
    /// diff, and `determine_all_events` — which derives WindowResize /
    /// WindowMove / DpiChanged / the frame-flag transitions from exactly that
    /// diff — sees nothing to report. `Resized` was dead on all four backends,
    /// `DpiChanged` on Windows and Wayland, maximize/fullscreen/miniaturize on
    /// Wayland and macOS. Advancing the event baseline is the sole business of
    /// `process_window_events`.
    ///
    /// `apply` may run against both states, so pass a pure field assignment with
    /// no side effects.
    pub fn update_window_state(
        &mut self,
        source: WindowStateSource,
        apply: impl Fn(&mut FullWindowState),
    ) {
        apply(&mut self.current_window_state);
        if source == WindowStateSource::Os {
            if let Some(synced) = self.os_synced_state.as_mut() {
                apply(synced);
            }
        }
    }

    /// The live pointer state, for a handler translating a platform mouse
    /// event into it.
    ///
    /// One of the three narrow doors that replace the old
    /// `get_current_window_state_mut()`: a platform handler holds this across
    /// its whole body (and often hands it to a shared `apply_*` helper), which
    /// a closure cannot do while the handler still needs the window. Narrow
    /// because it reaches ONLY an input sub-state — `sync_window_state()` never
    /// diffs one, so no baseline decision is being skipped. The EVENT diff is a
    /// different matter and still owed: see [`Self::update_unsynced_state`].
    pub fn mouse_state_mut(&mut self) -> &mut azul_core::window::MouseState {
        &mut self.current_window_state.mouse_state
    }

    /// The live keyboard state — see [`Self::mouse_state_mut`].
    pub fn keyboard_state_mut(&mut self) -> &mut azul_core::window::KeyboardState {
        &mut self.current_window_state.keyboard_state
    }

    /// The live touch state — see [`Self::mouse_state_mut`].
    pub fn touch_state_mut(&mut self) -> &mut azul_core::window::TouchState {
        &mut self.current_window_state.touch_state
    }

    /// Apply a change to the parts of the window state `sync_window_state()`
    /// never diffs — `window_focused`, `theme`, `monitor_id`, `ime_position`,
    /// `layout_callback`, `active_route`, and multi-field input updates.
    ///
    /// Deliberately NOT [`update_window_state`](Self::update_window_state) with
    /// [`Os`](WindowStateSource::Os): that advances `os_synced_state` too, and
    /// doing so here would claim these fields are part of the OS-sync contract
    /// when nothing ever pushes them at the window system. There is no echo to
    /// suppress, so there is no baseline to advance.
    ///
    /// What it does NOT excuse is the EVENT diff: these fields are exactly what
    /// `determine_all_events` reads, so a caller still owes the
    /// snapshot → mutate → pass shape (or a `discard_input_delta` if it
    /// consumed the input itself). [`check_input_delta_consumed`] enforces that
    /// separately.
    ///
    /// Writing an OS-synced field through here would silently skip the baseline
    /// decision, so under the validation gate that is an assertion, not a
    /// convention.
    pub fn update_unsynced_state<R>(&mut self, apply: impl FnOnce(&mut FullWindowState) -> R) -> R {
        let before = validation_enabled().then(|| os_synced_fields(&self.current_window_state));
        let out = apply(&mut self.current_window_state);
        if let Some(before) = before {
            assert!(
                before == os_synced_fields(&self.current_window_state),
                "update_unsynced_state changed an OS-SYNCED field (title/size/position/flags); \
                 it has to go through update_window_state so the baseline decision is made"
            );
        }
        out
    }

    /// Snapshot the EVENT-DIFF baseline from a plain `CommonWindowState`.
    ///
    /// Same contract and same check as
    /// [`PlatformWindow::snapshot_window_state_baseline`], for the handful of
    /// ingress points that only ever get handed the common state (the
    /// system-theme adopters, which take `&mut CommonWindowState` so one body
    /// serves every window on the backend).
    pub fn snapshot_window_state_baseline(&mut self, site: &str) {
        check_input_delta_consumed(
            self.previous_window_state.as_ref(),
            &self.current_window_state,
            site,
        );
        self.previous_window_state = Some(self.current_window_state.clone());
    }

    /// Seed / advance the OS-sync baseline to the current state.
    ///
    /// Call it once the window exists and its state is known to match what the
    /// window system has (right after creation, or after a sync that pushed
    /// everything). Until it has been called at least once,
    /// [`Self::take_os_sync_diff`] answers `None` and no geometry is pushed —
    /// which is what makes the first frame a no-op instead of a burst of
    /// redundant `SetWindowPos` calls.
    pub fn mark_os_synced(&mut self) {
        self.os_synced_state = Some(self.current_window_state.clone());
    }

    /// The OS-sync baseline, without advancing it.
    #[must_use]
    pub fn os_sync_baseline(&self) -> Option<&FullWindowState> {
        self.os_synced_state.as_ref()
    }

    /// THE accessor every `sync_window_state()` opens with: `(baseline,
    /// current)` to diff, with the baseline advanced to `current` in the same
    /// call.
    ///
    /// Advancing here rather than at the end of each backend's sync is
    /// deliberate — a baseline that some backend forgets to advance re-pushes
    /// the same geometry every single frame, and that failure is invisible
    /// until a WM answers the redundant push with a configure event. Returns
    /// `None` before the baseline is seeded (see [`Self::mark_os_synced`]), so
    /// the classic `None => return, // first frame, nothing to sync` arm still
    /// reads the same.
    pub fn take_os_sync_diff(&mut self) -> Option<(FullWindowState, FullWindowState)> {
        let baseline = self.os_synced_state.take()?;
        let current = self.current_window_state.clone();
        self.os_synced_state = Some(current.clone());
        Some((baseline, current))
    }

    /// Perform a hit test using whichever backend is available (GPU or CPU).
    ///
    /// Encapsulates the GPU vs CPU dispatch so callers don't need if/else chains.
    pub fn perform_hit_test(
        &mut self,
        position: azul_core::geom::LogicalPosition,
    ) -> azul_core::hit_test::FullHitTest {
        use azul_core::window::CursorPosition;

        let focused_node = self.layout_window
            .as_ref()
            .and_then(|lw| lw.focus_manager.get_focused_node().copied());

        let layout_results_ptr = match self.layout_window.as_ref() {
            Some(lw) => lw as *const azul_layout::window::LayoutWindow,
            None => return azul_core::hit_test::FullHitTest::empty(focused_node),
        };

        // GPU path: WebRender hit tester
        if let Some(ref mut ht) = self.hit_tester {
            if let (Some(doc_id), Some(_)) = (self.document_id, self.id_namespace) {
                let resolved = ht.resolve();
                let hidpi = self.current_window_state.size.get_hidpi_factor();
                // SAFETY: layout_results is not modified by hit testing
                let layout_results = unsafe { &(*layout_results_ptr).layout_results };
                return crate::desktop::wr_translate2::fullhittest_new_webrender(
                    &*resolved, doc_id, focused_node, layout_results,
                    &CursorPosition::InWindow(position), hidpi,
                );
            }
        }

        // CPU path: layout-based hit tester
        if let Some(ref cpu_ht) = self.cpu_hit_tester {
            // SAFETY: neither layout_results nor the managers are modified by
            // hit testing
            let lw = unsafe { &*layout_results_ptr };
            let resolve = |d: azul_core::dom::DomId, n: azul_core::dom::NodeId| {
                lw.scroll_manager.get_current_offset(d, n)
            };
            // Same map the CPU raster paints reference frames from.
            let resolve_tf = |d: azul_core::dom::DomId, n: azul_core::dom::NodeId| {
                lw.gpu_state_manager
                    .caches
                    .get(&d)
                    .and_then(|c| c.css_current_transform_values.get(&n))
                    .copied()
            };
            let nodes = cpu_ht.hit_test_scrolled(position, &resolve, &resolve_tf);
            return crate::desktop::wr_translate2::convert_cpu_hit_test_to_full(
                cpu_ht,
                &nodes,
                focused_node,
                &lw.layout_results,
                position,
                &resolve,
                &resolve_tf,
            );
        }

        azul_core::hit_test::FullHitTest::empty(focused_node)
    }
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
            self.$field.current_window_state()
        }
        fn get_previous_window_state(&self) -> &Option<FullWindowState> {
            &self.$field.previous_window_state
        }
        fn set_previous_window_state(&mut self, state: FullWindowState) {
            self.$field.previous_window_state = Some(state);
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
        fn get_undo_manager(&self) -> &$crate::desktop::shell2::common::event::SharedUndoManager {
            &self.$field.undo_manager
        }
        fn get_common_mut(&mut self) -> &mut $crate::desktop::shell2::common::event::CommonWindowState {
            &mut self.$field
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
        fn get_hit_tester(&self) -> Option<&AsyncHitTester> {
            self.$field.hit_tester.as_ref()
        }
        fn get_cpu_hit_tester(&self) -> Option<&azul_layout::headless::CpuHitTester> {
            self.$field.cpu_hit_tester.as_ref()
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
        fn mark_display_list_dirty(&mut self) {
            self.$field.display_list_dirty = true;
        }
        fn take_display_list_dirty(&mut self) -> bool {
            let v = self.$field.display_list_dirty;
            self.$field.display_list_dirty = false;
            v
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
/// - Frame regeneration — provided, not required: `request_regeneration(reason)`
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

    /// Get the current window state.
    ///
    /// There is deliberately no `_mut` counterpart: a `&mut FullWindowState`
    /// handed to a backend is a door around both the OS-sync baseline and the
    /// event-diff guard. Writes go through
    /// [`CommonWindowState::update_window_state`] (fields `sync_window_state()`
    /// diffs) or [`CommonWindowState::update_unsynced_state`] (fields it never
    /// looks at), reached via [`Self::get_common_mut`].
    fn get_current_window_state(&self) -> &FullWindowState;

    /// Get the previous window state (if available)
    fn get_previous_window_state(&self) -> &Option<FullWindowState>;

    /// Set the previous window state
    fn set_previous_window_state(&mut self, state: FullWindowState);

    /// THE window-close protocol. Every backend routes its close through this.
    ///
    /// A close from the window manager is a REQUEST, not an order: flip
    /// `close_requested` false -> true, run a pass so `EventType::WindowClose`
    /// fires, and report whether the flag survived. A callback that cleared it
    /// has VETOED the close, and the window must stay open.
    ///
    /// This exists because all four backends got it wrong in four different
    /// ways: X11 had no protocol at all (a bare `is_open = false`, so the
    /// title-bar X discarded unsaved work with no callback and no veto),
    /// Wayland skipped it on compositor loss, Win32 relied on the previous
    /// pass having left the baselines equal instead of snapshotting, and macOS
    /// ran its copy TWICE for one user close. Backends now decide only what to
    /// DO with the verdict, not how to reach it.
    fn request_window_close(&mut self, site: &str) -> WindowCloseOutcome {
        self.snapshot_window_state_baseline(site);
        self.get_common_mut()
            .update_window_state(WindowStateSource::App, |ws| {
                ws.flags.close_requested = true;
            });
        let result = self.process_window_events(0);
        let confirmed = self.get_current_window_state().flags.close_requested;
        WindowCloseOutcome { confirmed, result }
    }

    /// Snapshot the EVENT-DIFF baseline, then mutate `current_window_state`,
    /// then run `process_window_events` — in that order. THE call every
    /// platform event handler opens with.
    ///
    /// It exists so the three-step protocol has one name instead of a
    /// copy-pasted `let s = self.get_current_window_state().clone();
    /// self.set_previous_window_state(s);` in every handler on four backends,
    /// and so the R2 check below has one place to live.
    ///
    /// Only for TOP-LEVEL handlers (a platform event just arrived). Do not
    /// call it from inside a pass: mid-pass the previous→current delta is the
    /// live input being consumed, and the check would rightly object.
    fn snapshot_window_state_baseline(&mut self, site: &str) {
        check_input_delta_consumed(
            self.get_previous_window_state().as_ref(),
            self.get_current_window_state(),
            site,
        );
        let current = self.get_current_window_state().clone();
        self.set_previous_window_state(current);
    }

    /// CREATION-TIME seed of the event-diff baseline, for
    /// `apply_initial_window_state()` and the geometry read-back that follows
    /// it — the points where the window is being BUILT and no pass has ever
    /// run.
    ///
    /// Unchecked on purpose. Everything creation writes into
    /// `current_window_state` (the frame flags it just applied, the position
    /// the window manager answered with) is state, not input: there is no
    /// callback yet to lose it, so [`check_input_delta_consumed`] has nothing
    /// true to say and would only report the construction itself. Every OTHER
    /// baseline advance must go through
    /// [`Self::snapshot_window_state_baseline`] or
    /// [`Self::discard_input_delta`].
    fn seed_window_state_baseline(&mut self, _site: &str) {
        let current = self.get_current_window_state().clone();
        self.set_previous_window_state(current);
    }

    /// SANCTIONED SWALLOW: a handler consumed an input itself and the delta
    /// must NOT become an event — the scrollbar-thumb drag (routed around the
    /// event system by design) and a key eaten by an open popup are the two
    /// legitimate cases. Advances the event-diff baseline so
    /// [`check_input_delta_consumed`] does not read the mutation as a lost
    /// event. Greppable by name; every call site is an explicit, audited
    /// exception to the snapshot→mutate→pass contract.
    fn discard_input_delta(&mut self, _site: &str) {
        let current = self.get_current_window_state().clone();
        self.set_previous_window_state(current);
    }

    // Resource Access

    /// Get mutable access to renderer resources
    fn get_renderer_resources_mut(&mut self) -> &mut RendererResources;

    /// Get the font cache
    fn get_fc_cache(&self) -> &Arc<FcFontCache>;

    /// Get the OpenGL context pointer
    fn get_gl_context_ptr(&self) -> &OptionGlContextPtr;

    /// Get the system style
    fn get_system_style(&self) -> &Arc<azul_css::system::SystemStyle>;

    /// Get mutable access to the underlying CommonWindowState
    fn get_common_mut(&mut self) -> &mut CommonWindowState;

    /// Get the shared application data
    fn get_app_data(&self) -> &Arc<RefCell<RefAny>>;

    /// Get the app-global undo/redo manager (shared across all windows)
    fn get_undo_manager(&self) -> &SharedUndoManager;

    // Scrollbar State

    /// Get the current scrollbar drag state
    fn get_scrollbar_drag_state(&self) -> Option<&ScrollbarDragState>;

    /// Get mutable access to scrollbar drag state
    fn get_scrollbar_drag_state_mut(&mut self) -> &mut Option<ScrollbarDragState>;

    /// Set scrollbar drag state
    fn set_scrollbar_drag_state(&mut self, state: Option<ScrollbarDragState>);

    // Hit Testing

    /// Get the async hit tester (None in CPU mode)
    fn get_hit_tester(&self) -> Option<&AsyncHitTester>;

    /// Get CPU-based hit tester (None in GPU mode)
    fn get_cpu_hit_tester(&self) -> Option<&azul_layout::headless::CpuHitTester>;

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

    /// Rebuild the DOM and lay it out ONCE. Implemented per backend.
    ///
    /// Do NOT call this directly from a frame path — call
    /// [`PlatformWindow::regenerate_layout`], which additionally runs the
    /// lifecycle callbacks this pass queued and honours their refresh requests.
    fn regenerate_layout_once(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String>;

    /// Rebuild the DOM, then keep rebuilding while lifecycle callbacks ask for it.
    ///
    /// `dispatch_pending_lifecycle_events` returns `true` when a callback
    /// returned something other than `Update::DoNothing`, and its contract is
    /// that the caller regenerates again. Every backend used to call it at the
    /// end of its own `regenerate_layout` and throw the answer away with
    /// `let _ =`, so a callback that seeds derived state on mount and asks for a
    /// refresh — a common pattern — had that request dropped ON EVERY PLATFORM.
    /// The UI only caught up when an unrelated later event forced another pass.
    ///
    /// The loop lives HERE, not at the seven call sites, because the obvious
    /// local fix does not work: those calls were made from INSIDE each backend's
    /// `regenerate_layout`, so `if dispatch() { self.regenerate_layout() }` would
    /// recurse. Splitting the single pass out as `regenerate_layout_once` makes
    /// the nesting structurally impossible — the once-variant no longer dispatches
    /// anything — and gives all seven backends the fix from one place.
    ///
    /// Bounded by [`MAX_LIFECYCLE_REGEN_PASSES`]: a callback that refreshes
    /// unconditionally would otherwise spin forever. Exhaustion is a bug in the
    /// callback, so it is logged and recorded in `FrameReport::hit_depth_cap`
    /// rather than passed over in silence.
    fn regenerate_layout(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        use azul_layout::window::MAX_LIFECYCLE_REGEN_PASSES;

        // Timed at WINDOW category, deliberately NOT Layout.
        //
        // Everything inside `regenerate_layout` logs under LogCategory::Layout,
        // and Layout is the category you are forced to silence (`AZ_LOG=debug,
        // -layout`) because it emitted 482 547 lines on a three-resize run. So
        // the single most important number for diagnosing a slow resize — how
        // long a relayout takes and how many passes it needed — was only
        // available in the one configuration too noisy to run. This span is
        // outside that category on purpose.
        let _span = crate::log_span!(
            crate::desktop::shell2::common::debug_server::LogCategory::Window,
            "regenerate_layout",
        );
        let started = std::time::Instant::now();

        self.poll_transient_mailbox();
        let mut result = self.regenerate_layout_once()?;
        let mut passes = 1usize;

        for _pass in 1..MAX_LIFECYCLE_REGEN_PASSES {
            // Popups first: a dismissal queues its `Dismissed` lifecycle event,
            // which the drain below then delivers in this same call.
            self.sync_transient_windows();
            // An inline-docked panel dropped onto another zone (or torn off /
            // docked back) re-grafts the layout tree: that is a layout change
            // of its own, whether or not its `Docked` handler asked for one.
            let docks_changed = self
                .get_layout_window()
                .is_some_and(|lw| lw.transient_docks_changed());
            let lifecycle_wants_pass = self.dispatch_pending_lifecycle_events();
            if !lifecycle_wants_pass && !docks_changed {
                self.refill_a11y_tree_after_regeneration();
                self.flush_a11y_tree_update();
                _span.note(format_args!(
                    "{passes} pass(es) in {:.2}ms",
                    started.elapsed().as_secs_f64() * 1000.0
                ));
                return Ok(result);
            }
            result = self.regenerate_layout_once()?;
            passes += 1;
        }
        self.sync_transient_windows();

        // One last drain, so callbacks still RUN on the final pass even though we
        // will not lay out again for them.
        if self.dispatch_pending_lifecycle_events() {
            if let Some(lw) = self.get_layout_window_mut() {
                lw.frame_report.hit_depth_cap = true;
            }
        }
        self.refill_a11y_tree_after_regeneration();
        self.flush_a11y_tree_update();
        // Hitting the cap means the lifecycle loop never converged — every one
        // of these passes is a FULL relayout, so this is the difference between
        // a resize costing one layout and costing MAX_LIFECYCLE_REGEN_PASSES of
        // them. Warn rather than note: it is a defect, not a statistic.
        crate::log_warn!(
            crate::desktop::shell2::common::debug_server::LogCategory::Window,
            "[regenerate_layout] hit the lifecycle cap: {passes} FULL relayouts in {:.2}ms \
             without converging",
            started.elapsed().as_secs_f64() * 1000.0
        );
        Ok(result)
    }

    /// Rebuild the accessibility tree into `a11y_manager.last_tree_update`
    /// after a DOM regeneration, so the per-backend push paths have something
    /// to push.
    ///
    /// The push side has existed on every desktop backend for a while: the
    /// x11/wayland/windows post-layout tails and all four
    /// `flush_a11y_tree_update` overrides `take()` the slot and hand it to
    /// their accesskit adapter. But the FILL side —
    /// [`azul_layout::window::LayoutWindow::update_a11y_tree`], whose own doc
    /// says "called after full layout AND after display-list-only
    /// regeneration" — was only ever called from macOS's `a11y_dirty`-gated
    /// tick. On the other backends the slot stayed `None` after the initial
    /// adapter activation, every `take()` came back empty, and the tree the
    /// screen reader saw stayed frozen at the FIRST state forever, no matter
    /// how often the DOM was rebuilt. Filling it here — in the one shared
    /// regeneration path every backend's frame code calls — is what makes the
    /// existing drains actually carry updates.
    ///
    /// Cost: one pass over the exposed nodes per DOM rebuild (the same cost
    /// macOS already paid per rebuild via its dirty-gated tick).
    ///
    /// The caller follows this with [`Self::flush_a11y_tree_update`] so a
    /// TIMER-driven rebuild reaches the adapter immediately too — without
    /// that, a regeneration with no subsequent input event left the update
    /// parked until the next `process_window_events` pass happened to flush.
    #[cfg(feature = "a11y")]
    fn refill_a11y_tree_after_regeneration(&mut self) {
        if let Some(lw) = self.get_layout_window_mut() {
            lw.update_a11y_tree();
        }
    }

    /// No-op without the `a11y` feature (there is no tree to rebuild).
    #[cfg(not(feature = "a11y"))]
    fn refill_a11y_tree_after_regeneration(&mut self) {}

    /// Resize the PLATFORM surface (swapchain / shm buffers / GL drawable) to
    /// match `current_window_state.size`.
    ///
    /// Compositor-driven resizes already do this from their own configure
    /// handler. This exists for the other direction: a resize the APPLICATION
    /// initiates, via `CallbackChange::ModifyWindowState` — an E2E scenario, a
    /// plugin, or any app calling `modify_window_state` with new dimensions.
    ///
    /// Without it the engine relayouts at the new size while the platform
    /// surface keeps the old one, and the compositor is handed undersized
    /// content for an oversized window: blank where the buffer does not reach.
    /// That was a real bug, and it only
    /// showed up on the programmatic path, because the mouse path never comes
    /// through here.
    ///
    /// Default is a NO-OP, which is correct for any backend where the window
    /// system owns the surface size and reports it back through a configure
    /// event. A backend that owns its own surface MUST override this — and
    /// override it rather than leaving the default, because an empty impl and
    /// an unimplemented one are indistinguishable at the call site.
    ///
    /// # Per-backend audit (2026-08-07) — the default is DELIBERATE, not unchecked
    ///
    /// The commit that introduced this said X11/Windows/macOS "have not been
    /// checked and may have the same bug". They have now been checked, by
    /// reading each backend's `sync_window_state()`:
    ///
    /// | backend | programmatic resize reaches the OS by | override needed |
    /// |---|---|---|
    /// | Wayland | nothing — `sync_window_state` has NO size branch | **yes, overridden** |
    /// | X11 | `sync_window_state` → `XResizeWindow` (x11/mod.rs) | no |
    /// | Windows | `sync_window_state` → `SetWindowPos` (windows/mod.rs) | no |
    /// | macOS | `sync_window_state` → `setContentSize` (macos/mod.rs) | no |
    /// | Android / iOS | the system owns the surface; `sync_window_state` is `{}` | no |
    /// | headless | no platform surface exists | no |
    ///
    /// The asymmetry is real rather than an oversight: on X11/Windows/macOS the
    /// platform call resizes the WINDOW, and the drawable is then resized by the
    /// resulting `ConfigureNotify` / `WM_SIZE` / `windowDidResize`. Wayland has
    /// no such round trip — the client allocates its own `wl_shm` buffers — so
    /// it is the one backend where the application-initiated path has to do the
    /// work itself.
    ///
    /// If you add a backend that allocates its own swapchain, override this.
    fn resize_platform_surface(&mut self, _width: i32, _height: i32) {}

    /// Ask for the DOM to be rebuilt before the next frame, saying WHY.
    ///
    /// Delegates to [`CommonWindowState::request_regeneration`] — the single
    /// entry point. There is deliberately no counterpart that just *clears* the
    /// request: consumers either take it with a stated epoch
    /// (`regen_epoch()` + `clear_regeneration_unless_reraised()`) or, when no
    /// user callback can run in between, with
    /// [`CommonWindowState::take_regeneration`].
    fn request_regeneration(&mut self, reason: azul_core::callbacks::RelayoutReason) {
        self.get_common_mut().request_regeneration(reason);
    }

    /// Rebuild the DOM NOW for `reason`, retiring only the request this call
    /// satisfies.
    ///
    /// For the handful of event arms that relayout synchronously instead of
    /// queueing a frame (the X11 ConfigureNotify / DPI paths). Doing it by hand
    /// meant either forgetting the reason tag or leaving the request set for a
    /// second, redundant regeneration on the next frame; a lifecycle callback
    /// that asks for another rebuild during this one still survives, because
    /// the epoch is captured before the work.
    fn regenerate_now(
        &mut self,
        reason: azul_core::callbacks::RelayoutReason,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        self.get_common_mut().request_regeneration(reason);
        let seen = self.get_common_mut().regen_epoch();
        let result = self.regenerate_layout();
        self.get_common_mut()
            .clear_regeneration_unless_reraised(seen);
        result
    }

    /// Mark that the display list was updated internally and needs sending to WebRender
    fn mark_display_list_dirty(&mut self);

    /// Check and clear the display_list_dirty flag
    fn take_display_list_dirty(&mut self) -> bool;

    /// Ask for the next frame to take the "layout is already up to date on the
    /// EXISTING StyledDom" path — re-run layout on it, do NOT rebuild the DOM.
    ///
    /// See [`CommonWindowState::request_relayout_only`]: this also raises the
    /// ordinary regeneration request so the frame gates can see that work is
    /// owed.
    fn request_relayout_only(&mut self) {
        self.get_common_mut().request_relayout_only();
    }

    /// Consume the relayout-only request.
    #[must_use]
    fn take_relayout_only(&mut self) -> bool {
        self.get_common_mut().take_relayout_only()
    }

    // Callback Invocation Preparation

    /// Borrow all resources needed for `invoke_single_callback` in one call.
    ///
    /// This method returns a struct with individual field borrows, allowing the borrow
    /// checker to see that we're borrowing distinct fields rather than `&mut self` multiple times.
    ///
    /// ## Returns
    /// * `InvokeSingleCallbackBorrows` - All borrowed resources needed for callback invocation
    fn prepare_callback_invocation(&mut self) -> InvokeSingleCallbackBorrows<'_>;

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

    /// This window's key in the backend's window registry — what a child
    /// passes as `WindowCreateOptions::parent_window_id`. Every backend keys
    /// its registry by the native handle the raw window handle carries
    /// (`NSWindow*`, `HWND`, the X11 window id, the `wl_surface*`), so the
    /// default derives it; headless has no registry and reports 0.
    fn registry_window_id(&self) -> u64 {
        use azul_core::window::RawWindowHandle;
        match self.get_raw_window_handle() {
            RawWindowHandle::MacOS(h) => h.ns_window as usize as u64,
            RawWindowHandle::Windows(h) => h.hwnd as usize as u64,
            RawWindowHandle::Xlib(h) => h.window,
            RawWindowHandle::Wayland(h) => h.surface as usize as u64,
            _ => 0,
        }
    }

    /// Ask every OTHER window of this app to regenerate its layout and
    /// redraw. Backends implement it by walking their registry — the same
    /// fan-out they do for `Update::RefreshDomAllWindows`. It is how a parent
    /// wakes the popups it wrote a mailbox for, and how a popup wakes its
    /// parent after dismissing itself. Headless has nobody to wake.
    fn request_regeneration_all_windows(&mut self) {}

    /// Make this window pass mouse events straight through to whatever is
    /// behind it (macOS `setIgnoresMouseEvents:`). Used for a live drag proxy
    /// — a torn panel following the parent's cursor — so the parent keeps the
    /// gesture. Default: no-op (headless, or a backend without click-through).
    fn set_window_mouse_transparent(&mut self, _transparent: bool) {}

    /// `<transient-window>`, parent side: after a layout pass, turn the
    /// engine's popup diff into child windows / mailbox writes, and act on
    /// popups that dismissed themselves. See `common::transient`.
    fn sync_transient_windows(&mut self) {
        let parent_id = self.registry_window_id();
        let parent_state = self.get_current_window_state().clone();
        let Some(lw) = self.get_layout_window_mut() else {
            return;
        };
        let outcome = super::transient::sync_parent(parent_id, &parent_state, lw);
        for options in outcome.create {
            self.queue_window_create(options);
        }
        if outcome.wake_all {
            self.request_regeneration_all_windows();
        }
    }

    /// Give the OS the window's shape: `rects` (physical pixels of the
    /// frame just presented) cover every pixel that IS the window; outside
    /// them clicks fall through and, where the OS draws shapes (X11 /
    /// Windows), nothing is drawn. An empty `rects` means "the whole frame
    /// is transparent": backends keep the previous shape rather than vanish.
    /// Called after a CPU present while `WindowFlags::shape_from_alpha` is
    /// set, only when the shape changed. macOS needs no call - a non-opaque
    /// window hit-tests by its alpha - so the default is a no-op.
    fn apply_window_shape(&mut self, _rects: &[azul_layout::cpurender::ShapeRect]) {}

    /// Start the OS's own eyedropper for `request_id` (macOS: the system
    /// sampler). `true` if one started - the answer arrives through
    /// `azul_layout::managers::eyedropper::push_result`. The default has none.
    fn start_native_eyedropper(&mut self, _request_id: u64) -> bool {
        false
    }

    /// Read the screen for the eyedropper's loupe window. `None` when the
    /// platform cannot (or the user declined - Wayland asks through the
    /// portal). The default reads nothing.
    fn capture_screen_for_eyedropper(&mut self) -> Option<crate::desktop::eyedropper::Screenshot> {
        None
    }

    /// Run every queued `pick_screen_color`: the native sampler where there
    /// is one, else a screenshot in a fullscreen loupe window; a request
    /// neither can serve is answered "cancelled" so the asking callback is
    /// not left waiting.
    fn dispatch_eyedropper_requests(&mut self) {
        for req in azul_layout::managers::eyedropper::drain_requests() {
            if self.start_native_eyedropper(req.request_id) {
                continue;
            }
            let dpi = self.get_current_window_state().size.dpi;
            match self
                .capture_screen_for_eyedropper()
                .and_then(|shot| crate::desktop::eyedropper::loupe_window(shot, req.request_id, dpi))
            {
                Some(options) => {
                    log_debug!(
                        super::debug_server::LogCategory::Window,
                        "[eyedropper] opening the loupe window for request {}",
                        req.request_id
                    );
                    self.queue_window_create(options);
                }
                None => crate::desktop::eyedropper::finish(req.request_id, None),
            }
        }
    }

    /// Does applying a new `position` through `sync_window_state` actually
    /// move this window? `true` everywhere a window can be placed; a Wayland
    /// popup (no `xdg_popup.reposition` at the bound protocol version) says
    /// `false`, and the tear-off drag then tracks the pointer arithmetically.
    fn window_follows_position_changes(&self) -> bool {
        true
    }

    /// `<transient-window>`, popup side: close if the parent said so; move
    /// and resize to the placement the parent last wrote (the anchor moved,
    /// the content re-measured) - a torn-off toplevel only resizes.
    fn poll_transient_mailbox(&mut self) {
        use super::transient::{poll_popup, proxy_is_following, relative_position, PopupAction};
        // A drag proxy (a torn panel the parent is dragging) must let the mouse
        // through to the parent that owns the gesture; a settled window takes
        // its own clicks again.
        let following = proxy_is_following(self.get_current_window_state());
        self.set_window_mouse_transparent(following);
        match poll_popup(self.get_current_window_state()) {
            PopupAction::Close => {
                let _ = self.request_window_close("transient.closed_by_parent");
            }
            PopupAction::Place { origin, size } => {
                self.get_common_mut().update_window_state(WindowStateSource::App, |ws| {
                    if let Some(o) = origin {
                        ws.position = relative_position(o);
                    }
                    ws.size.dimensions = size;
                });
                self.sync_window_state();
            }
            PopupAction::Nothing => {}
        }
    }

    /// `<transient-window>`, both sides, on every input transition:
    /// - a popup dismisses itself on Escape / focus loss (per its policy),
    /// - a parent dismisses its `outside`-dismissable popups on a fresh press.
    fn process_transient_dismissal(&mut self) {
        use super::transient::{dismiss_outside_on_press, popup_dismiss_cause, post_dismissed};
        let Some(previous) = self.get_previous_window_state().clone() else {
            return;
        };
        let current = self.get_current_window_state().clone();

        if let Some(cause) = popup_dismiss_cause(&previous, &current) {
            if post_dismissed(&current) {
                log_debug!(
                    super::debug_server::LogCategory::Window,
                    "[transient] popup dismissing itself: {cause:?}"
                );
                // The dismissing input (the Escape press / the focus-loss
                // transition) is CONSUMED by the popup — the documented "key
                // eaten by an open popup" swallow. Without this,
                // `request_window_close`'s baseline snapshot reads the very
                // delta that caused the dismissal as an unconsumed input and
                // the debug validator aborts (`transient.dismissed:
                // keyboard_state`). The parent still hears the key through its
                // OWN window state; only this closing popup's copy is spent.
                self.discard_input_delta("transient.dismissed");
                let _ = self.request_window_close("transient.dismissed");
                self.request_regeneration_all_windows();
            }
            return;
        }
        // The window is closing for a reason of its own (a torn-off
        // toplevel's close button): the parent's node closes with it.
        if super::transient::post_dismissed_on_close(&previous, &current) {
            log_debug!(
                super::debug_server::LogCategory::Window,
                "[transient] closing window reports itself dismissed"
            );
            self.request_regeneration_all_windows();
            return;
        }

        let dismissed_any = match self.get_layout_window_mut() {
            Some(lw) => {
                dismiss_outside_on_press(&previous, &current, lw)
                    | super::transient::dismiss_on_escape(&previous, &current, lw)
            }
            None => false,
        };
        if dismissed_any {
            log_debug!(
                super::debug_server::LogCategory::Window,
                "[transient] press in the parent dismissed its popups"
            );
            // The Dismissed lifecycle event is queued; a regeneration drains it.
            self.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
            self.request_regeneration_all_windows();
        }
    }

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

    /// The C-API `DeleteBackward` / `DeleteForward` arms, routed onto the SAME
    /// path Backspace and Delete take.
    ///
    /// They used to call `text3::edit::delete_backward` / `delete_forward`
    /// against the primary CURSOR only. A Range selection was therefore
    /// invisible to them — the C API could not delete a selection at all, it
    /// deleted one grapheme next to the selection's cursor — nothing was
    /// recorded for undo (the keyboard path records a `DeleteText` operation
    /// with styled pre/post snapshots), and the caret kept blinking through
    /// the edit. `delete_selection` is that keyboard path.
    fn apply_capi_delete(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        forward: bool,
    ) -> ProcessEventResult {
        let target = azul_core::dom::DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
        };
        let now = azul_core::task::Instant::now();
        let Some(lw) = self.get_layout_window_mut() else {
            return ProcessEventResult::DoNothing;
        };
        if lw.delete_selection(target, forward).is_none() {
            return ProcessEventResult::DoNothing;
        }
        // Editing keeps the caret solid while it happens, same as typing.
        lw.text_edit_manager.blink.reset_blink_on_input(now);
        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
    }

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
            CallbackChange::SetAnimationMomentum {
                node,
                velocity_x,
                velocity_y,
            } => {
                if let Some(n) = node.node.into_crate_internal() {
                    if let Some(layout_window) = self.get_layout_window_mut() {
                        layout_window.apply_animation_momentum(n, *velocity_x, *velocity_y);
                    }
                }
                return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
            }

            CallbackChange::TickAnimations { dt_micros, steps } => {
                let dt = *dt_micros as f32 / 1_000_000.0;
                if let Some(layout_window) = self.get_layout_window_mut() {
                    for _ in 0..(*steps).max(1) {
                        // `tick_animations`, NOT `tick_animations_now`:
                        // bypassing the wall clock is the entire purpose.
                        layout_window.tick_animations(dt);
                    }
                }
                // Sample the tracks for THIS tick — may invoke COMPONENT
                // animation functions; their changes apply like timer
                // changes. Idempotent per tick (guarded), so the present
                // path's own sampling cannot double-invoke.
                let track_changes = {
                    let borrows = self.prepare_callback_invocation();
                    let system_callbacks = ExternalSystemCallbacks::rust_internal();
                    let frame_start = (system_callbacks.get_system_time_fn.cb)();
                    borrows.layout_window.run_track_frames(
                        dt,
                        frame_start,
                        &borrows.window_handle,
                        borrows.gl_context_ptr,
                        borrows.system_style.clone(),
                        &system_callbacks,
                        borrows.previous_window_state,
                        borrows.current_window_state,
                        borrows.renderer_resources,
                    )
                };
                let mut extra = ProcessEventResult::DoNothing;
                for change in &track_changes {
                    extra = extra.max(self.apply_user_change(change));
                }
                // A settled step still owes one frame, so the final (identity)
                // transform actually reaches the screen. A layout-affecting
                // `animation` transition escalates to a real relayout.
                let (needs_relayout, patched) = match self.get_layout_window_mut() {
                    Some(lw) => (lw.take_transition_relayout(), lw.take_transition_patched()),
                    None => (false, false),
                };
                return extra.max(if needs_relayout {
                    ProcessEventResult::ShouldIncrementalRelayout
                } else if patched {
                    // The DL was patched in place — re-render only.
                    ProcessEventResult::ShouldReRenderCurrentWindow
                } else {
                    ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
                });
            }

            // NOT YET EXECUTED ON THE DESKTOP SHELL.
            //
            // `E2eSession` — which owns the continuation slot a script has to
            // be resumed through — is not reachable from here, so there is
            // nowhere to hand this yet. Logged at ERROR and dropped, NOT
            // swallowed: a scripting API that accepts a script, returns
            // success and runs nothing is indistinguishable from one that
            // works, which is the whole reason this arm is loud.
            // No executor yet, so nothing can be running to cancel. Silent
            // because `stop_e2e_json` is documented as a no-op for unknown
            // handles — unlike Execute, this arm is not hiding a dropped
            // request.
            CallbackChange::StopE2eJson { .. } => ProcessEventResult::DoNothing,

            CallbackChange::ExecuteE2eJson { .. } => {
                // eprintln, not the e2e logger: `azul_layout::e2e` is itself
                // behind `e2e-server`, so the message would vanish in exactly
                // the builds most likely to hit this.
                eprintln!(
                    "[azul] execute_e2e_json: NOT IMPLEMENTED on the desktop shell \
                     — the script was DROPPED. The E2eSession continuation slot is \
                     not reachable from apply_user_change yet. Both Async (queue + \
                     driver timer) and Sync (block the caller) are still to be wired."
                );
                ProcessEventResult::DoNothing
            }

            // === Window State ===

            CallbackChange::ModifyWindowState { state } => {
                let old_state = self.get_current_window_state().clone();

                let mouse_state_changed = old_state.mouse_state != state.mouse_state;
                let keyboard_state_changed = old_state.keyboard_state != state.keyboard_state;
                // WINDOW-LEVEL transitions. `event_determination` derives
                // WindowFocusIn / WindowFocusOut / WindowMove / WindowResize
                // purely from current-vs-previous FullWindowState — which is
                // exactly what the platform handlers rely on (WM_SETFOCUS,
                // X11 FocusIn, ConfigureNotify, WM_DPICHANGED all just mutate
                // the state and run the diff pass). Until now this handler
                // neither COPIED `window_focused` nor ran the diff pass for a
                // window-level change, so `modify_window_state()` could not
                // focus, blur, move or rescale a window at all.
                let focus_changed = old_state.window_focused != state.window_focused
                    || old_state.flags.has_focus != state.flags.has_focus;
                let position_changed = old_state.position != state.position;
                let size_changed = old_state.size.dimensions != state.size.dimensions;
                // Copied out now: `old_state` is MOVED into
                // set_previous_window_state below, and the resize decision
                // further down needs the pre-change dimensions.
                let old_dims = old_state.size.dimensions;
                let dpi_changed = old_state.size.dpi != state.size.dpi;
                // Touch was missing from BOTH the copy below and this gate, so
                // a callback that pushed a modified `touch_state` through
                // `modify_window_state` changed nothing and ran no pass. The
                // headless E2E port of this handler had the same hole, which is
                // what made every touch op inert.
                let touch_state_changed = old_state.touch_state != state.touch_state;

                let anything_changed = mouse_state_changed
                    || keyboard_state_changed
                    || focus_changed
                    || position_changed
                    || size_changed
                    || dpi_changed
                    || touch_state_changed;

                // Save previous state BEFORE modifying (for synthetic event detection)
                if anything_changed {
                    self.set_previous_window_state(old_state);
                }

                // Apply state changes
                self.get_common_mut()
                    .update_window_state(WindowStateSource::App, |current| {
                        current.title = state.title.clone();
                        current.size = state.size;
                        current.position = state.position;
                        current.flags = state.flags;
                        current.background_color = state.background_color;
                        current.mouse_state = state.mouse_state;
                        current.keyboard_state = state.keyboard_state.clone();
                        current.touch_state = state.touch_state.clone();
                        current.window_focused = state.window_focused;
                    });

                if state.flags.close_requested {
                    return ProcessEventResult::DoNothing;
                }

                let mut result = ProcessEventResult::ShouldReRenderCurrentWindow;

                // A size OR scale change invalidates every rasterised pixel:
                // same thing WM_DPICHANGED / the X11 DPI path do
                // (a regeneration request tagged RelayoutReason::Resize).
                if size_changed || dpi_changed {
                    // The engine relayouts because of the line below; the
                    // PLATFORM surface has to be told separately. A
                    // compositor-driven resize does this from its configure
                    // handler and never reaches here, so this call is what
                    // covers application-initiated resizes.
                    let (w, h) = (
                        state.size.dimensions.width as i32,
                        state.size.dimensions.height as i32,
                    );
                    crate::log_debug!(
                        crate::desktop::shell2::common::debug_server::LogCategory::Window,
                        "[resize] APP-initiated {}x{} (size_changed={} dpi_changed={}) \
                         — calling resize_platform_surface + regenerate",
                        w,
                        h,
                        size_changed,
                        dpi_changed
                    );
                    if w > 0 && h > 0 {
                        let _span = crate::log_span!(
                            crate::desktop::shell2::common::debug_server::LogCategory::Window,
                            "resize_platform_surface",
                        );
                        self.resize_platform_surface(w, h);
                    } else {
                        // A zero/negative size reaching here means the caller
                        // built a window state the platform cannot represent.
                        // Silently skipping it is how "the window never
                        // resized" turns into an unexplained blank region.
                        crate::log_warn!(
                            crate::desktop::shell2::common::debug_server::LogCategory::Window,
                            "[resize] REFUSED a {w}x{h} platform resize — the engine will \
                             still relayout, so the surface and the layout are now out of step"
                        );
                    }
                    if dpi_changed {
                        // A DPI change rescales every rasterised pixel AND
                        // changes text metrics — never the fast path.
                        self.request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
                    } else {
                        // Resize policy: layout() is NOT re-invoked unless a
                        // recorded window-size query answer flips or a CSS
                        // breakpoint / orientation is crossed. See
                        // request_regeneration_for_resize.
                        let full = self.get_common_mut().request_regeneration_for_resize(
                            old_dims,
                            state.size.dimensions,
                        );
                        crate::log_debug!(
                            crate::desktop::shell2::common::debug_server::LogCategory::Window,
                            "[resize] APP-initiated path chose {} ({}x{} -> {}x{})",
                            if full { "FULL regeneration (boundary crossed)" }
                            else { "fast relayout (no boundary crossed)" },
                            old_dims.width,
                            old_dims.height,
                            state.size.dimensions.width,
                            state.size.dimensions.height,
                        );
                    }
                }

                // Mouse state changed → update hit test before the event pass
                if mouse_state_changed {
                    let mouse_pos = self.get_current_window_state()
                        .mouse_state.cursor_position.get_position();
                    if let Some(pos) = mouse_pos {
                        self.update_hit_test_at(pos);
                    }
                }

                // Run the state-diff pass ONCE for whatever changed. This is the
                // single path that turns a state delta into synthetic events
                // (MouseDown/Up, KeyDown/Up, WindowFocusIn/Out, WindowMove,
                // WindowResize) and dispatches the user callbacks bound to them.
                if anything_changed {
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

                    self.get_common_mut()
                        .update_window_state(WindowStateSource::App, |current| {
                            current.mouse_state = queued_state.mouse_state;
                            current.keyboard_state = queued_state.keyboard_state.clone();
                            current.title = queued_state.title.clone();
                            current.size = queued_state.size;
                            current.position = queued_state.position;
                            current.flags = queued_state.flags;
                        });

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
                self.get_common_mut()
                    .update_window_state(WindowStateSource::App, |ws| {
                        ws.flags.close_requested = true;
                    });
                ProcessEventResult::DoNothing
            }

            // === Focus ===

            CallbackChange::SetFocusTarget { target } => {
                // Resolve ONCE, and distinguish "matched nothing" from "there
                // is no layout to match against yet". A failed resolution must
                // also be AUDIBLE: `.ok().flatten()` used to swallow both the
                // Err(warning) and the matched-nothing case, and a dropped
                // programmatic focus is indistinguishable from a working no-op
                // (the 2026-08-11 caret hunt burned a day on exactly this
                // silence).
                //
                // The two-call version this replaces asked the same question
                // twice and read `Ok(None)` as "clear focus" — which, before
                // the first layout, is what silently swallowed every
                // `set_focus` issued from a create callback.
                use azul_layout::managers::focus_cursor::FocusResolution;

                let resolution = if let Some(lw) = self.get_layout_window_mut() {
                    azul_layout::managers::focus_cursor::resolve_focus_target_or_defer(
                        &mut lw.focus_manager,
                        target,
                        &lw.layout_results,
                    )
                } else {
                    return ProcessEventResult::DoNothing;
                };

                let new_focus = match resolution {
                    Ok(FocusResolution::Resolved(Some(n))) => {
                        crate::log_debug!(
                            crate::desktop::shell2::common::debug_server::LogCategory::Window,
                            "[SetFocusTarget] resolved {:?} -> node {:?}",
                            target, n
                        );
                        Some(n)
                    }
                    Ok(FocusResolution::Resolved(None)) => {
                        crate::log_debug!(
                            crate::desktop::shell2::common::debug_server::LogCategory::Window,
                            "[SetFocusTarget] target resolved to NO node — clearing focus: {:?}",
                            target
                        );
                        None
                    }
                    Ok(FocusResolution::Deferred) => {
                        // No layout yet: the target is queued on the focus
                        // manager and re-resolved by the next
                        // finalize_pending_focus_changes(). Touching focus here
                        // — in either direction — is what made a create-callback
                        // set_focus vanish.
                        crate::log_debug!(
                            crate::desktop::shell2::common::debug_server::LogCategory::Window,
                            "[SetFocusTarget] focus target queued until first layout: {:?}",
                            target
                        );
                        return ProcessEventResult::DoNothing;
                    }
                    Err(w) => {
                        crate::log_warn!(
                            crate::desktop::shell2::common::debug_server::LogCategory::Window,
                            "[SetFocusTarget] resolution FAILED: {:?} (target {:?})",
                            w, target
                        );
                        return ProcessEventResult::DoNothing;
                    }
                };

                if let Some(new_focus) = new_focus {
                    // Focus a specific node
                    let timer_action = if let Some(lw) = self.get_layout_window_mut() {
                        lw.focus_manager.set_focused_node(Some(new_focus));

                        use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;
                        let now = azul_core::task::Instant::now();
                        lw.scroll_node_into_view(new_focus, ScrollIntoViewOptions::nearest(), now);

                        let ws = lw.current_window_state.clone();
                        let action = lw.handle_focus_change_for_cursor_blink(Some(new_focus), &ws);
                        // Finalize the W3C flag-and-defer contenteditable
                        // focus NOW, exactly as the e2e runner does — the
                        // handler only sets the PENDING flag, and nothing
                        // else in the shell ever finalized it: the blink
                        // timer ticked forever with no cursor and no caret
                        // was ever painted for programmatic focus
                        // (2026-08-11 caret hunt).
                        lw.finalize_pending_focus_changes();
                        Some(action)
                    } else {
                        None
                    };

                    if let Some(action) = timer_action {
                        match action {
                            azul_layout::CursorBlinkTimerAction::Start(timer) => {
                                self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                            }
                            azul_layout::CursorBlinkTimerAction::Restart(timer) => {
                                self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                                self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                            }
                            azul_layout::CursorBlinkTimerAction::Stop => {
                                self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                            }
                            azul_layout::CursorBlinkTimerAction::NoChange => {}
                        }
                    }
                    ProcessEventResult::ShouldReRenderCurrentWindow
                } else {
                    // Clear focus
                    let timer_action = if let Some(lw) = self.get_layout_window_mut() {
                        lw.focus_manager.set_focused_node(None);
                        let ws = lw.current_window_state.clone();
                        let action = lw.handle_focus_change_for_cursor_blink(None, &ws);
                        // Symmetric with the focus branch above (runner parity).
                        lw.finalize_pending_focus_changes();
                        Some(action)
                    } else {
                        None
                    };

                    if let Some(action) = timer_action {
                        match action {
                            azul_layout::CursorBlinkTimerAction::Start(_)
                            | azul_layout::CursorBlinkTimerAction::Restart(_) => {}
                            azul_layout::CursorBlinkTimerAction::Stop => {
                                self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                            }
                            azul_layout::CursorBlinkTimerAction::NoChange => {}
                        }
                    }
                    ProcessEventResult::ShouldReRenderCurrentWindow
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

            CallbackChange::ChangeNodeAccessibilityState { node_id, states } => {
                // Widgets publish role and state when they BUILD. That is only
                // correct if every state change rebuilds, and many do not — the
                // accordion toggles with set_css_property and Update::DoNothing,
                // so a build-time `Expanded` would keep announcing "expanded"
                // after the section closed, with no way for the user to notice.
                let dom_id = node_id.dom;
                let Some(nid) = node_id.node.into_crate_internal() else {
                    return ProcessEventResult::DoNothing;
                };
                let mut changed = false;
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(lr) = lw.layout_results.get_mut(&dom_id) {
                        let mut nodes = lr.styled_dom.node_data.as_container_mut();
                        if let Some(node) = nodes.get_mut(nid) {
                            let mut info = node
                                .accessibility
                                .as_ref()
                                .map_or_else(Default::default, |b| (**b).clone());
                            if info.states != *states {
                                info.states = states.clone();
                                node.set_accessibility_info(info);
                                changed = true;
                            }
                        }
                    }
                }
                if changed {
                    // Tell the platform adapter to re-read: an update nobody is
                    // notified of is the same as no update at all.
                    self.get_common_mut().a11y_dirty = true;
                }
                ProcessEventResult::DoNothing
            }
            CallbackChange::ChangeNodeAccessibilityValue { node_id, value } => {
                let dom_id = node_id.dom;
                let Some(nid) = node_id.node.into_crate_internal() else {
                    return ProcessEventResult::DoNothing;
                };
                let mut changed = false;
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(lr) = lw.layout_results.get_mut(&dom_id) {
                        let mut nodes = lr.styled_dom.node_data.as_container_mut();
                        if let Some(node) = nodes.get_mut(nid) {
                            let mut info = node
                                .accessibility
                                .as_ref()
                                .map_or_else(Default::default, |b| (**b).clone());
                            let new_val = azul_css::OptionString::Some(value.clone());
                            if info.accessibility_value != new_val {
                                info.accessibility_value = new_val;
                                node.set_accessibility_info(info);
                                changed = true;
                            }
                        }
                    }
                }
                if changed {
                    self.get_common_mut().a11y_dirty = true;
                }
                ProcessEventResult::DoNothing
            }
            CallbackChange::ChangeNodeText { node_id, text } => {
                let dom_id = node_id.dom;
                let internal_node_id = match node_id.node.into_crate_internal() {
                    Some(id) => id,
                    None => return ProcessEventResult::DoNothing,
                };

                // NO-OP SHORT CIRCUIT (mirrors the headless E2E runner):
                // re-setting the byte-identical string used to drop the ENTIRE
                // incremental shaped-text cache and re-shape every run in the
                // DOM for a write that changed nothing — and produced no damage,
                // so nothing could ever observe the waste.
                let unchanged = self.get_layout_window().is_some_and(|lw| {
                    lw.layout_results
                        .get(&dom_id)
                        .is_some_and(|lr| {
                            let nodes = lr.styled_dom.node_data.as_container();
                            nodes.get(internal_node_id).is_some_and(|node| {
                                matches!(
                                    node.get_node_type(),
                                    azul_core::dom::NodeType::Text(existing)
                                        if existing.as_str() == text.as_str()
                                )
                            })
                        })
                });
                if unchanged {
                    return ProcessEventResult::DoNothing;
                }

                // Update StyledDom text content
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(layout_result) = lw.layout_results.get_mut(&dom_id) {
                        let idx = internal_node_id.index();
                        if idx < layout_result.styled_dom.node_data.as_ref().len() {
                            layout_result.styled_dom.node_data.as_container_mut()[internal_node_id]
                                .set_node_type(azul_core::dom::NodeType::Text(azul_css::css::BoxOrStatic::heap(text.clone())));
                        }
                    }
                    // The incremental layout cache keys its shaped-text runs on
                    // the DOM pointer, which a text mutation does not change — so
                    // the next relayout happily reused the OLD glyph runs and the
                    // screen kept showing the previous text (damage was reported,
                    // yet not one pixel differed). Drop the incremental cache so
                    // the text is re-shaped…
                    lw.layout_cache.reset_incremental();
                    // …and rebuild the display list, which otherwise still carries
                    // the old glyph run (same reasoning as ChangeNodeImage below).
                    lw.regenerate_display_list_for_dom(dom_id);
                }
                ProcessEventResult::ShouldIncrementalRelayout
            }

            CallbackChange::RecordDocumentEdit { changeset } => {
                // RECORD phase only — nothing is mutated; the app reads the
                // changeset (get_document_edit_clone), applies it to its own
                // model and regenerates.
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.record_document_edit(changeset.clone());
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::MarkDocumentEditApplied { id } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    let _ = lw.mark_document_edit_applied(*id);
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::MarkDocumentEditAppliedWithInverse { id, inverse } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    let _ = lw.mark_document_edit_applied_with_inverse(*id, inverse.clone());
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::UndoStructuralEdit => {
                if let Some(lw) = self.get_layout_window_mut() {
                    let _ = lw.undo_structural_edit();
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::RedoStructuralEdit => {
                if let Some(lw) = self.get_layout_window_mut() {
                    let _ = lw.redo_structural_edit();
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::ChangeNodeImage { dom_id, node_id, image, update_type: _ } => {
                // The ONE content chokepoint: overlay write + journal + in-place
                // display-list patch (paint tier — the DL diff sees the ImageRef
                // identity change) or incremental-cache reset (relayout tier,
                // when the intrinsic size changed). The StyledDom is NEVER
                // mutated and the DL is NEVER rebuilt for a same-size swap.
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.apply_content_change(azul_layout::overlay::ContentChange::Image {
                        dom_id: *dom_id,
                        node_id: *node_id,
                        image: image.clone(),
                    })
                    .tier
                    .to_process_event_result()
                } else {
                    ProcessEventResult::DoNothing
                }
            }

            CallbackChange::UpdateImageCallback { dom_id: _, node_id: _ } => {
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::UpdateAllImageCallbacks => {
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::UpdateVirtualView { dom_id, node_id } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    let mut updates = BTreeMap::new();
                    let mut set = FastBTreeSet::new();
                    set.insert(*node_id);
                    updates.insert(*dom_id, set);
                    lw.queue_virtual_view_updates(updates);
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::UpdateAllVirtualViews => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.queue_all_virtual_view_reinvoke();
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::ChangeNodeImageMask { dom_id, node_id, mask } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.apply_content_change(azul_layout::overlay::ContentChange::ImageMask {
                        dom_id: *dom_id,
                        node_id: *node_id,
                        mask: mask.clone(),
                    })
                    .tier
                    .to_process_event_result()
                } else {
                    ProcessEventResult::DoNothing
                }
            }

            CallbackChange::ChangeNodeCssProperties { dom_id, node_id, properties } => {
                // The content chokepoint (one impl for this host AND the e2e
                // runner): inline-vec sync + retained-cascade restyle + DL
                // rebuild + the shared paint-vs-relayout tier.
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.apply_content_change(azul_layout::overlay::ContentChange::NodeCss {
                        dom_id: *dom_id,
                        node_id: *node_id,
                        props: properties.as_ref().to_vec(),
                        override_only: false,
                    })
                    .tier
                    .to_process_event_result()
                } else {
                    ProcessEventResult::DoNothing
                }
            }

            CallbackChange::OverrideNodeCssProperties { dom_id, node_id, properties } => {
                // Fast-path override channel (animation frames): cascade-only
                // write, no inline-vec sync — same chokepoint, override_only.
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.apply_content_change(azul_layout::overlay::ContentChange::NodeCss {
                        dom_id: *dom_id,
                        node_id: *node_id,
                        props: properties.as_ref().to_vec(),
                        override_only: true,
                    })
                    .tier
                    .to_process_event_result()
                } else {
                    ProcessEventResult::DoNothing
                }
            }

            CallbackChange::ScrollTo { dom_id, node_id, position, unclamped } => {
                log_debug!(super::debug_server::LogCategory::EventLoop, "[SCROLL] ScrollTo dom={:?} node={:?} pos=({:.1},{:.1}) unclamped={}", dom_id, node_id, position.x, position.y, unclamped);
                let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
                let now = (external.get_system_time_fn.cb)();

                let mut needs_virtual_view_update = false;

                if let Some(internal_node_id) = node_id.into_crate_internal() {
                    if let Some(lw) = self.get_layout_window_mut() {
                        if *unclamped {
                            // Physics timer provides pre-clamped rubber-band positions
                            lw.scroll_manager.set_scroll_position_unclamped(
                                *dom_id, internal_node_id, *position,
                                now.clone(),
                            );
                        } else {
                            lw.scroll_manager.scroll_to(
                                *dom_id, internal_node_id, *position,
                                std::time::Duration::from_millis(0).into(),
                                azul_core::events::EasingFunction::Linear,
                                now.clone(),
                            );
                        }

                        // Recalculate scrollbar geometry so CPU-side hit testing
                        // (perform_scrollbar_hit_test) has up-to-date thumb positions.
                        lw.scroll_manager.calculate_scrollbar_states();

                        // Check if this scroll node is a VirtualView that needs
                        // re-invocation (e.g. user scrolled near edge for lazy loading).
                        // If so, queue it for processing in the next render pass.
                        needs_virtual_view_update = lw.check_and_queue_virtual_view_reinvoke(
                            *dom_id, internal_node_id,
                        );
                    }
                }

                if needs_virtual_view_update {
                    // VirtualView needs new content — force display list rebuild
                    ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
                } else {
                    // Normal scroll — lightweight repaint (scroll offsets only)
                    ProcessEventResult::ShouldReRenderCurrentWindow
                }
            }
            CallbackChange::SetVirtualViewGeometry {
                dom_id,
                node_id,
                materialized,
                virtual_rect,
            } => {
                // #28 (a): reconfigure a VirtualView's geometry (USER design;
                // guide/en/dom/virtual-views.md) WITHOUT re-invoking its
                // callback — exactly the two stores a normal invoke writes
                // (VirtualViewManager + ScrollManager), minus the child
                // relayout. Each rect: Some = set, None = keep.
                //
                // The streaming case this exists for — a background
                // exact-pagination pass correcting the document extent — sets
                // `virtual_rect` only. Content placement reads the
                // materialized window and the live scroll offset, never the
                // virtual extent, so the scrollbar re-scales and not one
                // pixel moves.
                if let Some(internal_node_id) = node_id.into_crate_internal() {
                    if let Some(lw) = self.get_layout_window_mut() {
                        let (kept_scroll, kept_virtual) = lw
                            .virtual_view_manager
                            .get_declared_sizes(*dom_id, internal_node_id);
                        let kept_origin = lw
                            .virtual_view_manager
                            .materialized_window_origin(*dom_id, internal_node_id);
                        let new_mat: Option<azul_core::geom::LogicalRect> = (*materialized).into();
                        let new_virt: Option<azul_core::geom::LogicalRect> = (*virtual_rect).into();
                        let eff_virtual = new_virt.map(|r| r.size).or(kept_virtual);
                        let eff_scroll = new_mat.map(|r| r.size).or(kept_scroll).or(eff_virtual);
                        let eff_origin = new_mat
                            .map(|r| r.origin)
                            .or(kept_origin)
                            .unwrap_or_else(azul_core::geom::LogicalPosition::zero);
                        if let (Some(s), Some(v)) = (eff_scroll, eff_virtual) {
                            let _ = lw.virtual_view_manager.update_virtual_view_info(
                                *dom_id,
                                internal_node_id,
                                eff_origin,
                                s,
                                v,
                            );
                            lw.scroll_manager.update_virtual_scroll_bounds(
                                *dom_id,
                                internal_node_id,
                                v,
                                Some(eff_origin),
                            );
                            lw.scroll_manager.calculate_scrollbar_states();
                        }
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::ScrollIntoView { node_id, options } => {
                let now = azul_core::task::Instant::now();
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.scroll_node_into_view(*node_id, *options, now);
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
                // Single authority: the LayoutWindow's ImageCache (the shell
                // copy and its mirroring are deleted). The chokepoint returns
                // the DL-rebuild tier — the old handler returned `DoNothing`,
                // so a css-id registration only became visible on the next
                // UNRELATED relayout.
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.apply_content_change(azul_layout::overlay::ContentChange::ImageById {
                        id: id.clone(),
                        image: Some(image.clone()),
                    })
                    .tier
                    .to_process_event_result()
                } else {
                    ProcessEventResult::DoNothing
                }
            }

            CallbackChange::RemoveImageFromCache { id } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.apply_content_change(azul_layout::overlay::ContentChange::ImageById {
                        id: id.clone(),
                        image: None,
                    })
                    .tier
                    .to_process_event_result()
                } else {
                    ProcessEventResult::DoNothing
                }
            }

            CallbackChange::ReloadSystemFonts => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.font_manager.replace_fc_cache(FcFontCache::build().into());
                }
                ProcessEventResult::DoNothing
            }

            // === Menu / Tooltip ===

            CallbackChange::OpenMenu { menu, position } => {
                let pos = position.unwrap_or(LogicalPosition::new(0.0, 0.0));
                self.show_menu_from_callback(menu, pos);
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::CapturePointer { node } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.pointer_capture = Some(*node);
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::ReleasePointerCapture => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.pointer_capture = None;
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::SetTransientWindowOpen { node, open } => {
                // Only this window's root dom hangs popups; a node id from a
                // popup's own dom (or any child dom) means nothing here.
                let Some(node_id) = node.node.into_crate_internal() else {
                    return ProcessEventResult::DoNothing;
                };
                if node.dom != azul_core::dom::DomId::ROOT_ID {
                    log_warn!(
                        super::debug_server::LogCategory::Window,
                        "[transient] set_transient_window_open on a non-root dom {:?}: ignored",
                        node.dom
                    );
                    return ProcessEventResult::DoNothing;
                }
                let changed = self
                    .get_layout_window_mut()
                    .is_some_and(|lw| lw.transient_windows.set_forced_open(node_id, *open));
                if changed {
                    // The popup set is reconciled after a layout pass.
                    self.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                    ProcessEventResult::ShouldRegenerateDomCurrentWindow
                } else {
                    ProcessEventResult::DoNothing
                }
            }

            CallbackChange::SetTransientWindowTorn { node, torn } => {
                let Some(node_id) = node.node.into_crate_internal() else {
                    return ProcessEventResult::DoNothing;
                };
                if node.dom != azul_core::dom::DomId::ROOT_ID {
                    log_warn!(
                        super::debug_server::LogCategory::Window,
                        "[transient] set_transient_window_torn on a non-root dom {:?}: ignored",
                        node.dom
                    );
                    return ProcessEventResult::DoNothing;
                }
                let changed = self
                    .get_layout_window_mut()
                    .is_some_and(|lw| lw.set_transient_window_torn(node_id, *torn));
                if changed {
                    // The surface change (popup <-> toplevel) is synced after
                    // a layout pass, which also drains the TornOff/Docked event.
                    self.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                    ProcessEventResult::ShouldRegenerateDomCurrentWindow
                } else {
                    ProcessEventResult::DoNothing
                }
            }

            CallbackChange::PickScreenColor => {
                // Issued on THIS window's manager so the answer routes back
                // here; dispatched right away (the loupe is queued, or the
                // system sampler starts) rather than on the next pump.
                if let Some(lw) = self.get_layout_window_mut() {
                    let id = lw.eyedropper_manager.begin_request();
                    azul_layout::managers::eyedropper::push_request(
                        azul_layout::managers::eyedropper::EyedropperRequest { request_id: id },
                    );
                }
                self.dispatch_eyedropper_requests();
                ProcessEventResult::DoNothing
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
                self.apply_capi_delete(*dom_id, *node_id, false)
            }

            CallbackChange::DeleteForward { dom_id, node_id } => {
                self.apply_capi_delete(*dom_id, *node_id, true)
            }

            CallbackChange::MoveCursor { dom_id, node_id, cursor } => {
                // Same route as every MoveCursor{Left,Right,…} arm. Setting the
                // cursor straight on the multi-cursor state skipped the display
                // list rebuild `handle_cursor_movement` does, so a programmatic
                // move repainted the OLD caret position. `extend_selection` is
                // false because this variant carries an absolute cursor, not a
                // movement.
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.handle_cursor_movement(*dom_id, *node_id, *cursor, false);
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::SetSelection { dom_id: _, node_id: _, selection } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    match selection {
                        azul_core::selection::Selection::Cursor(cursor) => {
                            if let Some(ref mut mc) = lw.text_edit_manager.multi_cursor { mc.set_single_cursor(*cursor); }
                        }
                        azul_core::selection::Selection::Range(range) => {
                            if let Some(ref mut mc) = lw.text_edit_manager.multi_cursor { mc.set_single_range(*range); }
                        }
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::SetTextChangeset { changeset } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.text_input_manager.set_changeset(changeset.clone());
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
                // A user On::Copy callback overrode the clipboard content. Commit it
                // to the OS clipboard now: the old `sync_clipboard` flush path was
                // never called by any run loop, so this is the only place the
                // override reaches the system pasteboard.
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.clipboard_manager.set_copy_content(content.clone());
                }
                if let Some(payload) = clipboard_content_to_payload(content) {
                    set_system_clipboard(&payload);
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::SetCutContent { target: _, content } => {
                // A user On::Cut callback overrode the clipboard content. Commit it
                // to the OS clipboard (deletion of the selected text is handled by
                // the CutToClipboard shortcut / user code, as before).
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.clipboard_manager.set_copy_content(content.clone());
                }
                if let Some(payload) = clipboard_content_to_payload(content) {
                    set_system_clipboard(&payload);
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::SetSelectAllRange { target, range } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(ref mut mc) = lw.text_edit_manager.multi_cursor {
                        mc.set_single_range(*range);
                    }
                }
                ProcessEventResult::DoNothing
            }

            // === Multi-Cursor ===

            CallbackChange::AddCursor { dom_id, node_id, cursor } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(ref mut mc) = lw.text_edit_manager.multi_cursor {
                        let _ = mc.add_cursor(*cursor);
                    } else {
                        // Create new MultiCursorState with the cursor
                        let dom_node_id = azul_core::dom::DomNodeId {
                            dom: *dom_id,
                            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                        };
                        lw.text_edit_manager.multi_cursor = Some(
                            azul_core::selection::MultiCursorState::new_with_cursor(*cursor, dom_node_id, 0)
                        );
                    }
                    lw.text_edit_manager.mark_dirty();
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::AddSelectionRange { dom_id, node_id, range } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(ref mut mc) = lw.text_edit_manager.multi_cursor {
                        let _ = mc.add_selection(*range);
                    } else {
                        let dom_node_id = azul_core::dom::DomNodeId {
                            dom: *dom_id,
                            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                        };
                        let mut mc = azul_core::selection::MultiCursorState::new_with_cursor(range.start, dom_node_id, 0);
                        mc.set_single_range(*range);
                        lw.text_edit_manager.multi_cursor = Some(mc);
                    }
                    lw.text_edit_manager.mark_dirty();
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::RemoveSelectionById { selection_id } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    if let Some(ref mut mc) = lw.text_edit_manager.multi_cursor {
                        let _ = mc.remove_selection(*selection_id);
                        lw.text_edit_manager.mark_dirty();
                    }
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
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

            CallbackChange::SetCursorVisibility { visible } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.text_edit_manager.blink.set_visibility(*visible);
                    // The tween driver rides this change: a FOCUS-RING glide
                    // runs without an editing session, so fall back to the
                    // tween's own dom (else the ring tween would stall after
                    // the first frame).
                    let dom = lw
                        .text_edit_manager
                        .get_editing_dom_id()
                        .or(lw.text_edit_manager.tween.dom_id);
                    if let Some(dom_id) = dom {
                        lw.regenerate_display_list_for_dom(dom_id);
                    }
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::ToggleCursorVisibility => {
                if let Some(lw) = self.get_layout_window_mut() {
                    let now = azul_core::task::Instant::now();
                    if lw.text_edit_manager.blink.should_blink(&now) {
                        lw.text_edit_manager.blink.toggle_visibility();
                    } else {
                        lw.text_edit_manager.blink.set_visibility(true);
                    }
                    // Regenerate display list with cursor rect toggled.
                    // Future: use GPU opacity animation instead of display list rebuild.
                    if let Some(dom_id) = lw.text_edit_manager.get_editing_dom_id() {
                        lw.regenerate_display_list_for_dom(dom_id);
                    }
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::ResetCursorBlink => {
                if let Some(lw) = self.get_layout_window_mut() {
                    let now = azul_core::task::Instant::now();
                    lw.text_edit_manager.blink.reset_blink_on_input(now);
                }
                ProcessEventResult::DoNothing
            }

            CallbackChange::StartCursorBlinkTimer => {
                let timer = if let Some(lw) = self.get_layout_window_mut() {
                    if lw.text_edit_manager.blink.is_blink_timer_active() {
                        None
                    } else {
                        lw.text_edit_manager.blink.set_blink_timer_active(true);
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
                    if lw.text_edit_manager.blink.is_blink_timer_active() {
                        lw.text_edit_manager.blink.set_blink_timer_active(false);
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
                let mut text_prevented = false;

                if !text_events.is_empty() {
                    let (text_changes_result, text_update, text_prevent_default) =
                        self.dispatch_events_propagated(&text_events);
                    text_prevented = text_prevent_default;
                    result = result.max_self(text_changes_result);
                    if matches!(text_update, Update::RefreshDom | Update::RefreshDomAllWindows) {
                        result = result.max_self(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
                    }
                }

                // A callback veto (preventDefault / TextInputValid::No) kills
                // the recorded edit — clearing it also stops the NEXT pass's
                // unconditional apply from landing it late.
                if text_prevented {
                    if let Some(lw) = self.get_layout_window_mut() {
                        lw.text_input_manager.clear_changeset();
                    }
                    return result;
                }

                // Apply text changeset
                if let Some(lw) = self.get_layout_window_mut() {
                    let changeset_result = lw.apply_text_changeset();
                    if !changeset_result.dirty_nodes.is_empty() {
                        if changeset_result.needs_relayout {
                            // Text size changed — need full re-layout for scroll container update
                            result = result.max(ProcessEventResult::ShouldIncrementalRelayout);
                        } else {
                            result = result.max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
                        }
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

                            // `append_child` always attaches to the DOM's ROOT — the
                            // requested `parent_node_id` was accepted, validated and
                            // then IGNORED, so every inserted node landed as a last
                            // child of <html> (outside <body>, inheriting nothing,
                            // painting nothing). Append first, then RE-PARENT.
                            let sd = &mut layout_result.styled_dom;
                            let new_id = azul_core::id::NodeId::new(
                                sd.node_data.as_ref().len(),
                            );
                            let root_id = sd
                                .root
                                .into_crate_internal()
                                .unwrap_or(azul_core::id::NodeId::ZERO);
                            let root_last_before =
                                sd.node_hierarchy.as_container()[root_id].last_child_id();
                            sd.append_child(styled);

                            if *parent_node_id != root_id {
                                // The hierarchy is a FLAT DFS array whose
                                // `first_child_id(n)` is *derived* as `n + 1`. A node
                                // appended at the end of the array can therefore only
                                // ever be a LAST child, and only of a parent that
                                // already has children (otherwise the derived first
                                // child would point at an unrelated node). Anything
                                // else needs a full re-index of the DOM (and of every
                                // node-keyed manager) — out of scope here, so it is
                                // rejected loudly instead of silently corrupting the
                                // tree.
                                let parent_last = sd.node_hierarchy.as_container()
                                    [*parent_node_id]
                                    .last_child_id();
                                match parent_last {
                                    Some(parent_last) => {
                                        // 1. unlink the new node from the root chain
                                        {
                                            let h = &mut sd.node_hierarchy;
                                            h.as_container_mut()[root_id].last_child =
                                                azul_core::id::NodeId::into_raw(&root_last_before);
                                            if let Some(rl) = root_last_before {
                                                h.as_container_mut()[rl].next_sibling =
                                                    azul_core::id::NodeId::into_raw(&None);
                                            }
                                            // 2. link it as the parent's new last child
                                            h.as_container_mut()[parent_last].next_sibling =
                                                azul_core::id::NodeId::into_raw(&Some(new_id));
                                            h.as_container_mut()[new_id].previous_sibling =
                                                azul_core::id::NodeId::into_raw(&Some(parent_last));
                                            h.as_container_mut()[new_id].next_sibling =
                                                azul_core::id::NodeId::into_raw(&None);
                                            h.as_container_mut()[new_id].parent =
                                                azul_core::id::NodeId::into_raw(&Some(
                                                    *parent_node_id,
                                                ));
                                            h.as_container_mut()[*parent_node_id].last_child =
                                                azul_core::id::NodeId::into_raw(&Some(new_id));
                                        }
                                        // 3. keep the cascade bookkeeping consistent
                                        let sibling_index = {
                                            let h = sd.node_hierarchy.as_container();
                                            let mut n = parent_node_id
                                                .az_children(&h)
                                                .count();
                                            n = n.saturating_sub(1);
                                            n
                                        };
                                        let ci = sd.cascade_info.as_mut();
                                        ci[parent_last.index()].is_last_child = false;
                                        ci[new_id.index()].index_in_parent =
                                            u32::try_from(sibling_index).unwrap_or(u32::MAX);
                                        ci[new_id.index()].is_last_child = true;
                                        sd.finalize_non_leaf_nodes();
                                    }
                                    None => {
                                        log_warn!(
                                            super::debug_server::LogCategory::EventLoop,
                                            "[InsertChildNode] parent {:?} has no children — the \
                                             flat DFS hierarchy cannot represent a first child \
                                             appended at the end of the array; the node stayed at \
                                             the DOM root",
                                            parent_node_id
                                        );
                                    }
                                }
                            }
                            let _ = position; // only append-as-last-child is representable

                            // Re-run the author cascade from the retained stylesheet:
                            // the node was styled with an EMPTY css above (the author
                            // rules are unavailable here), so without this it would
                            // never match rules like `.hot { width: 80px }` — the
                            // "inserted node never gets the author cascade" bug.
                            // Grow the retained author-CSS scopes so the just-appended
                            // node is inside them, then re-cascade so it picks up rules
                            // like `.hot { width: 80px }`.
                            sd.extend_author_scopes_for_appended(new_id, *parent_node_id);
                            sd.restyle_retained();
                            // `append_child` composes the trees but does NOT re-run
                            // inheritance or rebuild the compact cache (see the doc
                            // comment on this very method): the appended node keeps
                            // its isolated cascade — no inherited font-size, no
                            // inherited color, no UA defaults, no compact-cache
                            // entry — so it measured 0×0 and painted nothing even
                            // once it was correctly parented.
                            sd.recompute_inheritance_and_compact_cache();
                        }
                    }
                    // The tree changed shape: the incremental layout cache (keyed
                    // on the DOM pointer) would otherwise reuse the old tree.
                    lw.layout_cache.reset_incremental();
                    // The stored display list still describes the OLD tree.
                    lw.regenerate_display_list_for_dom(*dom_id);
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

            CallbackChange::RemountDom { xml } => {
                // The E2E `mount` / `unmount` document is per-window state, not
                // a process-global sink: store it on the window and let
                // `regenerate_layout` read it back on the next pass.
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.e2e_mount.set(xml.as_ref().map(|s| s.as_str().to_string()));
                }
                ProcessEventResult::ShouldRegenerateDomCurrentWindow
            }

            // === Routing ===

            CallbackChange::SwitchRoute { pattern, params } => {
                // Look up the route in LayoutWindow.routes and swap the layout callback
                let found_cb = self.get_layout_window().and_then(|lw| {
                    lw.routes.as_ref().iter().find_map(|route| {
                        if route.pattern.as_str() == pattern.as_str() {
                            Some(route.layout_callback.clone())
                        } else {
                            None
                        }
                    })
                });

                if let Some(new_cb) = found_cb {
                    self.get_common_mut().update_unsynced_state(|ws| {
                        // Swap layout callback
                        ws.layout_callback = new_cb;
                        // Store the active route match (pattern + params)
                        ws.active_route = azul_core::resources::OptionRouteMatch::Some(
                            azul_core::resources::RouteMatch {
                                pattern: pattern.clone(),
                                params: params.clone(),
                            },
                        );
                    });
                    ProcessEventResult::ShouldRegenerateDomCurrentWindow
                } else {
                    log_warn!(
                        super::debug_server::LogCategory::EventLoop,
                        "[azul] SwitchRoute: no route found for pattern '{}'",
                        pattern.as_str(),
                    );
                    ProcessEventResult::DoNothing
                }
            }

            // === Native Gesture Injection ===

            CallbackChange::InjectNativeGesture { gesture } => {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.gesture_drag_manager.inject_native_gesture(*gesture);
                }
                // The latched gesture only becomes a SyntheticEvent inside an
                // event pass (`determine_all_events` consults the gesture
                // manager: an injected DoubleClick raises `E::DoubleClick` at
                // the hovered node), and the same pass clears the latch. Run
                // that pass NOW — the contract every state-raising change
                // follows (ModifyWindowState, QueueWindowStateSequence): a
                // change delivers its events before the next change applies.
                // This used to return ShouldRegenerateDomCurrentWindow and
                // rely on "some later pass" reading the latch — which, until
                // pass-end delta consumption was enforced, happened to be the
                // STALE re-detection pass (the same defect that double-fired
                // MouseUp). With that fixed, nothing guaranteed delivery at
                // all: an injected DoubleClick sat latched until an unrelated
                // event pass fired it at whatever was hovered much later.
                self.process_window_events(0)
            }

            // === Accessibility action (screen-reader request) ===
            //
            // The out-of-band twin of the per-backend
            // `process_accessibility_actions()` pump: same
            // `LayoutWindow::process_accessibility_action`, same synthetic-event
            // dispatch. Routed through the shared
            // `dispatch_accessibility_actions` so this arm and the seven frame
            // pumps cannot disagree about what an action does.
            CallbackChange::PerformAccessibilityAction {
                dom_id,
                node_id,
                action,
            } => {
                #[cfg(feature = "a11y")]
                {
                    self.dispatch_accessibility_actions(vec![(
                        *dom_id,
                        *node_id,
                        action.clone(),
                    )]);
                    // UNCONDITIONAL, matching every backend's unconditional
                    // `request_redraw()` after a batch: Focus / Blur / the
                    // Scroll* family / SetTextSelection change manager state and
                    // map to NO callback, so their affected-node map is empty
                    // while the screen has genuinely gone stale.
                    ProcessEventResult::ShouldReRenderCurrentWindow
                }
                #[cfg(not(feature = "a11y"))]
                {
                    // Not "nothing happened" — the build genuinely cannot
                    // perform the action, and a caller that asked for one must
                    // hear about it rather than read a silent DoNothing as
                    // success.
                    log_warn!(
                        super::debug_server::LogCategory::EventLoop,
                        "[azul] PerformAccessibilityAction on dom {} node {} ignored: this build \
                         has the `a11y` feature disabled, so no accessibility action can be \
                         applied",
                        dom_id.inner,
                        node_id.index(),
                    );
                    let _ = action;
                    ProcessEventResult::DoNothing
                }
            }

            // === App-global Undo / Redo (mini-git over the app state) ===

            CallbackChange::CommitUndoSnapshot => {
                // Clone the Arc first so the `&self` borrow ends before we
                // borrow `&self` again via get_undo_manager().
                let app_data = self.get_app_data().clone();
                let snap = app_data.borrow();
                self.get_undo_manager().commit(&snap);
                ProcessEventResult::DoNothing // committing a snapshot doesn't change the UI
            }

            CallbackChange::UndoAppState => {
                let app_data = self.get_app_data().clone();
                let ok = self.get_undo_manager().undo(&mut app_data.borrow_mut());
                if ok {
                    ProcessEventResult::ShouldRegenerateDomAllWindows
                } else {
                    ProcessEventResult::DoNothing
                }
            }

            CallbackChange::RedoAppState => {
                let app_data = self.get_app_data().clone();
                let ok = self.get_undo_manager().redo(&mut app_data.borrow_mut());
                if ok {
                    ProcessEventResult::ShouldRegenerateDomAllWindows
                } else {
                    ProcessEventResult::DoNothing
                }
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
                // `as_millis_u64` converts a Tick span at the nominal frame rate.
                // The hand-rolled match this replaces returned the raw tick count
                // for the Tick arm — a FRAME count handed to a routine that reads
                // it as milliseconds — and answered 0 for every System span on
                // no_std.
                let current_time_ms = duration_since_event.as_millis_u64();
                if let Some(layout_window) = self.get_layout_window_mut() {
                    if layout_window.process_mouse_click_for_selection(*position, current_time_ms).is_some() {
                        return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::TextSelectionDrag { start_position, current_position } => {
                log_debug!(super::debug_server::LogCategory::Input, "[DRAG] TextSelectionDrag start=({:.1},{:.1}) current=({:.1},{:.1})", start_position.x, start_position.y, current_position.x, current_position.y);
                // Suppress text selection if a node drag is active
                let node_dragging = self.get_layout_window()
                    .map(|lw| lw.gesture_drag_manager.is_node_drag_active())
                    .unwrap_or(false);
                if node_dragging {
                    return ProcessEventResult::DoNothing;
                }
                if let Some(layout_window) = self.get_layout_window_mut() {
                    if layout_window.process_mouse_drag_for_selection(*start_position, *current_position).is_some() {
                        return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::ApplySelectionOp { target, op } => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    if layout_window.apply_selection_op(*target, op) {
                        return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
                    }
                }
                ProcessEventResult::DoNothing
            }

            // === Keyboard Shortcuts ===

            SystemChange::CopyToClipboard => {
                if let Some(layout_window) = self.get_layout_window() {
                    // MWA-C-text_edit: use the DOM that actually holds the
                    // editing session — the DomId-0 hardcode copied nothing
                    // when the contenteditable lived in a VirtualView /
                    // iframe child DOM.
                    let dom_id = layout_window
                        .text_edit_manager
                        .get_editing_dom_id()
                        .unwrap_or(azul_core::dom::DomId { inner: 0 });
                    if let Some(clipboard_content) = layout_window.get_selected_content_for_clipboard(&dom_id) {
                        if let Some(payload) = clipboard_content_to_payload(&clipboard_content) {
                            set_system_clipboard(&payload);
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::CutToClipboard { target } => {
                let mut affected = false;
                if let Some(layout_window) = self.get_layout_window_mut() {
                    // MWA-C-text_edit: editing DOM, not hardcoded DomId 0
                    // (see CopyToClipboard above).
                    let dom_id = layout_window
                        .text_edit_manager
                        .get_editing_dom_id()
                        .unwrap_or(azul_core::dom::DomId { inner: 0 });
                    if let Some(clipboard_content) = layout_window.get_selected_content_for_clipboard(&dom_id) {
                        let committed = clipboard_content_to_payload(&clipboard_content)
                            .is_some_and(|payload| set_system_clipboard(&payload));
                        if committed {
                            // Cross-block cut: the copy above already joined the
                            // multi-paragraph text; the delete is the atomic
                            // replace-merge changeset.
                            let deleted = if layout_window
                                .text_edit_manager
                                .get_cross_block_selection()
                                .is_some()
                            {
                                layout_window.delete_cross_block_selection().is_some()
                            } else {
                                layout_window.delete_selection(*target, false).is_some()
                            };
                            if deleted {
                                affected = true;
                            }
                        }
                    }
                }
                if affected { ProcessEventResult::ShouldUpdateDisplayListCurrentWindow } else { ProcessEventResult::DoNothing }
            }

            SystemChange::PasteFromClipboard => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    let pasted = get_system_clipboard()
                        .as_ref()
                        .and_then(payload_to_clipboard_content);
                    if let Some(clipboard_content) = pasted {
                        let clipboard_text = clipboard_content.plain_text.as_str().to_string();
                        // Paste over a cross-block selection: one atomic
                        // replace-merge changeset with the pasted text at the
                        // join (caret resumes after it).
                        if layout_window
                            .text_edit_manager
                            .get_cross_block_selection()
                            .is_some()
                        {
                            if layout_window
                                .replace_cross_block_selection(&clipboard_text)
                                .is_some()
                            {
                                return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
                            }
                        }
                        // `styled_runs` is populated whenever the OS payload
                        // carried a rich flavor (RTF/HTML) the decode policy
                        // could read; the text-editing pipeline below pastes
                        // the plain text.
                        layout_window.clipboard_manager.set_paste_content(clipboard_content);
                        // Smart paste: if N lines == N cursors, paste one line per cursor
                        let cursor_count = layout_window.text_edit_manager.multi_cursor
                            .as_ref().map(|mc| mc.len()).unwrap_or(0);
                        let lines: Vec<&str> = clipboard_text.lines().collect();

                        if cursor_count > 1 && lines.len() == cursor_count {
                            // N lines → N cursors: use edit_text_multi
                            if let Some(ref mc) = layout_window.text_edit_manager.multi_cursor {
                                let dom_id = mc.node_id.dom;
                                let target = mc.node_id;
                                if let Some(node_id) = mc.node_id.node.into_crate_internal() {
                                    let content = layout_window.get_text_before_textinput(dom_id, node_id);
                                    let selections = mc.to_selections();
                                    let (new_content, new_sels) = azul_layout::text3::edit::edit_text_multi(
                                        &content, &selections, &lines,
                                    );
                                    // Smart paste is an EDIT and has to be
                                    // undoable: it bypasses both recording
                                    // sites (apply_text_changeset for typing,
                                    // delete_selection for deletions), so
                                    // Ctrl+Z after a multi-cursor paste used
                                    // to undo whatever came before it instead.
                                    record_multi_edit_undo(
                                        layout_window, target, node_id,
                                        &content, &new_content, &selections,
                                    );
                                    if let Some(ref mut mc) = layout_window.text_edit_manager.multi_cursor {
                                        mc.update_from_edit_result(&new_sels);
                                    }
                                    layout_window.update_text_cache_after_edit(dom_id, node_id, new_content);
                                    layout_window.text_edit_manager.mark_dirty();
                                    return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
                                }
                            }
                        }

                        // Default: broadcast paste text to all cursors
                        let affected = layout_window.process_text_input(&clipboard_text);
                        if !affected.is_empty() {
                            return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::SelectAllText => {
                // Ctrl+A over a MULTI-BLOCK editable, not just the one IFC the
                // focus happens to sit on. The old arm built its range from
                // `get_inline_layout_for_node(focused_node)` alone, so a
                // container whose children are paragraphs has no inline layout
                // of its own and select-all did NOTHING at all — while the
                // engine has carried cross-block selection (the same machinery
                // drag-select and Cut/Copy use) the whole time.
                let Some(focused_node) = self
                    .get_layout_window()
                    .and_then(|lw| lw.focus_manager.focused_node)
                else {
                    return ProcessEventResult::DoNothing;
                };
                let dom_id = focused_node.dom;
                let Some(node_id) = focused_node.node.into_crate_internal() else {
                    return ProcessEventResult::DoNothing;
                };

                // The blocks to cover, rooted at the editing HOST: with focus
                // on one paragraph INSIDE a multi-paragraph contenteditable,
                // Ctrl+A must still cover the whole editable — so walk up to
                // the outermost node whose contenteditable-ness the focus
                // inherits, and select from there. A non-editable focus keeps
                // the focused node as its root (unchanged behavior).
                //
                // Root's own IFC (a text input / a single paragraph) → that
                // one block; otherwise its ELEMENT children that are IFC
                // roots — the paragraph chain of a contenteditable container.
                // Text children are skipped: XML pretty-printing whitespace
                // is not a block, and a raw text run cannot anchor a
                // cross-block selection.
                let blocks: Vec<NodeId> = {
                    let Some(lw) = self.get_layout_window() else {
                        return ProcessEventResult::DoNothing;
                    };
                    let sel_root = lw
                        .find_contenteditable_host(dom_id, node_id)
                        .unwrap_or(node_id);
                    if lw.get_inline_layout_for_node(dom_id, sel_root).is_some() {
                        vec![sel_root]
                    } else {
                        let Some(lr) = lw.layout_results.get(&dom_id) else {
                            return ProcessEventResult::DoNothing;
                        };
                        let hierarchy = lr.styled_dom.node_hierarchy.as_container();
                        let node_data = lr.styled_dom.node_data.as_container();
                        // DESCEND. A child that owns no inline layout is a
                        // WRAPPER, not a leaf — scanning only direct children
                        // made Ctrl+A a no-op for the ordinary shape
                        // `div[contenteditable] > section > p`, because
                        // `section` has no IFC of its own and was skipped
                        // without ever looking inside it.
                        let mut out = Vec::new();
                        let siblings_of = |parent: NodeId| {
                            let mut kids = Vec::new();
                            let mut child = hierarchy[parent].first_child_id(parent);
                            while let Some(c) = child {
                                kids.push(c);
                                child = hierarchy[c].next_sibling_id();
                            }
                            kids
                        };
                        // Reversed, so `pop()` yields document order.
                        let mut stack: Vec<NodeId> =
                            siblings_of(sel_root).into_iter().rev().collect();
                        let mut visited = 0usize;
                        while let Some(c) = stack.pop() {
                            visited += 1;
                            if visited > SELECT_ALL_BLOCK_SCAN_LIMIT {
                                break;
                            }
                            let is_text = matches!(
                                node_data[c].get_node_type(),
                                azul_core::dom::NodeType::Text(_)
                            );
                            if is_text {
                                continue;
                            }
                            if lw.get_inline_layout_for_node(dom_id, c).is_some() {
                                // Owns inline content: it IS a block to select,
                                // and its inline runs are not blocks.
                                out.push(c);
                                continue;
                            }
                            stack.extend(siblings_of(c).into_iter().rev());
                        }
                        out
                    }
                };

                let (Some(&first), Some(&last)) = (blocks.first(), blocks.last()) else {
                    return ProcessEventResult::DoNothing;
                };

                // Endpoint cursors come from the LAYOUT (real grapheme
                // clusters); a byte-length synthetic end cursor resolves to
                // nothing in get_selection_rects and the block silently loses
                // its highlight.
                let cursors = self.get_layout_window().and_then(|lw| {
                    let start = lw
                        .get_inline_layout_for_node(dom_id, first)?
                        .get_first_cluster_cursor()?;
                    let end = lw
                        .get_inline_layout_for_node(dom_id, last)?
                        .get_last_cluster_cursor()?;
                    Some((start, end))
                });
                let Some((start_cursor, end_cursor)) = cursors else {
                    return ProcessEventResult::DoNothing;
                };

                if first != last {
                    // Cross-block: the engine precomputes the per-IFC ranges
                    // (anchor tail, full middles, focus head) and stores them
                    // render-ready, where the display-list pass picks them up
                    // through build_text_selections_map.
                    let set = self
                        .get_layout_window_mut()
                        .is_some_and(|lw| {
                            lw.set_cross_block_selection(
                                dom_id, first, start_cursor, last, end_cursor,
                            )
                        });
                    if set {
                        if let Some(lw) = self.get_layout_window_mut() {
                            lw.regenerate_display_list_for_dom(dom_id);
                        }
                        return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
                    }
                    // Not a sibling chain (nested / mixed containers): fall
                    // through to selecting the first block rather than nothing.
                }

                let range = azul_core::selection::SelectionRange {
                    start: start_cursor,
                    end: if first == last {
                        end_cursor
                    } else {
                        // Single-block fallback: end at the FIRST block's end.
                        match self
                            .get_layout_window()
                            .and_then(|lw| lw.get_inline_layout_for_node(dom_id, first))
                            .and_then(|l| l.get_last_cluster_cursor())
                        {
                            Some(c) => c,
                            None => return ProcessEventResult::DoNothing,
                        }
                    },
                };

                if let Some(lw) = self.get_layout_window_mut() {
                    lw.text_edit_manager.clear_cross_block_selection();
                    let did_set = if let Some(ref mut mc) = lw.text_edit_manager.multi_cursor {
                        // Select the whole content as ONE range. The caret
                        // sits at range.end (last cluster) implicitly. Do NOT
                        // follow with set_single_cursor — that collapsed the
                        // selection, turning Ctrl+A into a no-op "move caret to
                        // end" instead of select-all.
                        mc.set_single_range(range);
                        true
                    } else {
                        false
                    };
                    if did_set {
                        // Rebuild the display list so the selection HIGHLIGHT is
                        // actually drawn (build_text_selections_map runs inside).
                        // Without this, ShouldUpdateDisplayListCurrentWindow only
                        // re-renders the stale display list, so the range is set
                        // functionally but stays invisible. Mirrors
                        // apply_selection_op (Shift+Arrow), which regenerates for
                        // exactly this reason.
                        lw.regenerate_display_list_for_dom(dom_id);
                        return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
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

                    if let Some(operation) = layout_window.undo_redo_manager.pop_undo(node_id) {
                        let node_id_internal = target.node.into_crate_internal();
                        if let Some(node_id_internal) = node_id_internal {
                            use std::sync::Arc;
                            use azul_layout::text3::cache::{InlineContent, StyleProperties, StyledRun};

                            // MWA-C-undo_redo: restore the STYLED pre-content
                            // snapshot when available; the plain-text rebuild
                            // (StyleProperties::default()) is only the
                            // fallback for evicted snapshots — it used to be
                            // the only path and stripped all styling.
                            let new_content = layout_window
                                .undo_redo_manager
                                .get_content_snapshot(operation.changeset.id)
                                .map(|snap| snap.pre.clone())
                                .unwrap_or_else(|| {
                                    vec![InlineContent::Text(StyledRun {
                                        text: std::sync::Arc::from(operation.pre_state.text_content.as_str()),
                                        style: Arc::new(StyleProperties::default()),
                                        logical_start_byte: 0,
                                        source_node_id: None,
                                    })]
                                });

                            layout_window.update_text_cache_after_edit(
                                target.dom, node_id_internal, new_content,
                            );

                            // MWA-C-undo_redo: restore the pre-edit selection
                            // too (a range beats the collapsed cursor);
                            // pre_state.selection_range previously had no
                            // consumer at all.
                            if let Some(ref mut mc) = layout_window.text_edit_manager.multi_cursor {
                                if let Some(range) = operation.pre_state.selection_range.into_option() {
                                    mc.set_single_range(range);
                                } else if let Some(cursor) = operation.pre_state.cursor_position.into_option() {
                                    mc.set_single_cursor(cursor);
                                }
                            }
                        }

                        layout_window.undo_redo_manager.push_redo(operation);
                        return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::RedoTextEdit { target } => {
                // MWA-C-undo_redo: redo now RE-APPLIES the post-state
                // directly. The old path pushed InsertText redos back
                // through process_text_input, which re-entered the recording
                // pipeline: apply_text_changeset recorded a SECOND undo
                // entry whose push_undo cleared the redo stack (one redo
                // destroyed the rest), and non-InsertText ops were silently
                // skipped while still being moved to the undo stack.
                if let Some(layout_window) = self.get_layout_window_mut() {
                    let node_id = match target.node.into_crate_internal() {
                        Some(id) => id,
                        None => return ProcessEventResult::DoNothing,
                    };

                    if let Some(operation) = layout_window.undo_redo_manager.pop_redo(node_id) {
                        use std::sync::Arc;
                        use azul_layout::managers::changeset::TextOperation;
                        use azul_layout::text3::cache::{InlineContent, StyleProperties, StyledRun};

                        // Styled post-content snapshot; plain-text fallback
                        // reconstructs pre_state + inserted text for evicted
                        // InsertText snapshots.
                        let new_content = layout_window
                            .undo_redo_manager
                            .get_content_snapshot(operation.changeset.id)
                            .map(|snap| snap.post.clone())
                            .or_else(|| {
                                if let TextOperation::InsertText(op) = &operation.changeset.operation {
                                    let mut text =
                                        operation.pre_state.text_content.as_str().to_string();
                                    text.push_str(op.text.as_str());
                                    Some(vec![InlineContent::Text(StyledRun {
                                        text: std::sync::Arc::from(text.as_str()),
                                        style: Arc::new(StyleProperties::default()),
                                        logical_start_byte: 0,
                                        source_node_id: None,
                                    })])
                                } else {
                                    None
                                }
                            });

                        if let Some(new_content) = new_content {
                            layout_window.update_text_cache_after_edit(
                                target.dom, node_id, new_content,
                            );
                            layout_window
                                .undo_redo_manager
                                .reinstate_undo(operation);
                            return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
                        }
                        // No snapshot and no reconstructable content: put the
                        // operation back on the redo stack unchanged instead
                        // of losing it.
                        layout_window.undo_redo_manager.push_redo(operation);
                    }
                }
                ProcessEventResult::DoNothing
            }

            // === Multi-Cursor ===

            SystemChange::AddCursorAtClick { position } => {
                // Ctrl+Click: add a cursor at the clicked position.
                // Delegates to process_mouse_click_for_selection which will
                // set multi_cursor to a single cursor. We then convert it to
                // "add" mode by saving the old cursors and re-adding them.
                if let Some(layout_window) = self.get_layout_window_mut() {
                    // Save existing multi-cursor selections
                    let old_selections = layout_window.text_edit_manager.multi_cursor
                        .as_ref()
                        .map(|mc| mc.selections.clone())
                        .unwrap_or_default();
                    let old_node_id = layout_window.text_edit_manager.multi_cursor
                        .as_ref()
                        .map(|mc| mc.node_id);

                    // This will reset multi_cursor to a single cursor at click position.
                    // `time_ms` is vestigial: multi-click detection moved into
                    // `gesture_drag_manager.detect_click_count()` and the callee
                    // no longer reads the parameter at all, so no clock is sampled
                    // here (the timestamp this arm used to compute was dead).
                    layout_window.process_mouse_click_for_selection(*position, 0);

                    // Now add back the old cursors
                    if let Some(ref mut mc) = layout_window.text_edit_manager.multi_cursor {
                        // Only merge if the node_id matches (same contenteditable)
                        if old_node_id.map(|n| n == mc.node_id).unwrap_or(false) {
                            // The new cursor is already in mc.selections[0].
                            // Prepend the old selections.
                            let new_cursor = mc.selections.clone();
                            mc.selections = old_selections;
                            mc.selections.extend(new_cursor);
                            mc.merge_overlapping();
                        }
                    }

                    return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::SelectNextOccurrence { target } => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    if layout_window.select_next_occurrence() {
                        return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
                    }
                }
                ProcessEventResult::DoNothing
            }

            // === Text Input ===

            SystemChange::ApplyPendingTextInput => {
                ProcessEventResult::DoNothing
            }

            SystemChange::ApplyTextChangeset => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    let changeset_result = layout_window.apply_text_changeset();
                    if !changeset_result.dirty_nodes.is_empty() {
                        if changeset_result.needs_relayout {
                            return ProcessEventResult::ShouldIncrementalRelayout;
                        }
                        return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
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
                let win_pos = self.get_current_window_state().position;
                if let Some(layout_window) = self.get_layout_window_mut() {
                    layout_window.gesture_drag_manager.activate_window_drag(win_pos, None);
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::InitDragVisualState => {
                if let Some(layout_window) = self.get_layout_window_mut() {
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

                            // MWA-C-gpu_state: register the drag transform in
                            // the CSS transform maps — the display list builds
                            // the dragged node's reference frame EXCLUSIVELY
                            // from css_transform_keys (display_list.rs child
                            // ref-frame lookup); the old write went into
                            // transform_keys, which is the vertical-scrollbar-
                            // thumb map, so the per-pixel drag offset never
                            // reached the screen (and could corrupt a thumb
                            // key). A pre-existing CSS transform is replaced
                            // for the drag's duration and restored by the CSS
                            // sync on the post-drag relayout.
                            let gpu_cache = layout_window.gpu_state_manager.get_or_create_cache(dom_id);
                            if let std::collections::hash_map::Entry::Vacant(e) = gpu_cache.css_transform_keys.entry(node_id) {
                                let transform_key = azul_core::resources::TransformKey::unique();
                                let identity = azul_core::transform::ComputedTransform3D::IDENTITY;
                                e.insert(transform_key);
                                gpu_cache.css_current_transform_values.insert(node_id, identity);
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
                            node_drag.previous_drop_target = node_drag.current_drop_target;
                            node_drag.current_drop_target = azul_core::dom::OptionDomNodeId::Some(*target);
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
                            // MWA-C-gpu_state: css map — see InitDragVisualState.
                            gpu_cache.css_current_transform_values.insert(node_id, new_transform);
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
                            // MWA-C-gpu_state: css maps — see InitDragVisualState.
                            // A genuine CSS transform on the node is restored
                            // by the CSS sync on the post-drag relayout (the
                            // :dragging restyle triggers one).
                            let gpu_cache = layout_window.gpu_state_manager.get_or_create_cache(dom_id);
                            gpu_cache.css_transform_keys.remove(&node_id);
                            gpu_cache.css_current_transform_values.remove(&node_id);
                        }
                    }

                    // End drag session
                    if layout_window.gesture_drag_manager.is_dragging() {
                        layout_window.gesture_drag_manager.end_drag();
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            // === Focus ===

            SystemChange::SetFocus { new_focus, old_focus } => {
                let old_focus_node_id = old_focus.and_then(|f| f.node.into_crate_internal());
                let new_focus_node_id = new_focus.and_then(|f| f.node.into_crate_internal());

                let mut result = ProcessEventResult::ShouldReRenderCurrentWindow;

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

                    // Bug A fix: Use apply_focus_restyle return value so :focus
                    // styling is applied immediately (not just on next resize)
                    if old_focus_node_id != new_focus_node_id {
                        let restyle_result = apply_focus_restyle(
                            layout_window, old_focus_node_id, new_focus_node_id,
                        );
                        result = result.max(restyle_result);
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
                        azul_layout::CursorBlinkTimerAction::Restart(timer) => {
                            self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                            self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                        }
                        azul_layout::CursorBlinkTimerAction::Stop => {
                            self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                        }
                        azul_layout::CursorBlinkTimerAction::NoChange => {}
                    }
                }

                result
            }

            SystemChange::ClearAllSelections => {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    // Clear all selections by collapsing ranges to cursors
                    if let Some(ref mut mc) = layout_window.text_edit_manager.multi_cursor {
                        if let Some(cursor) = mc.get_primary_cursor() {
                            mc.set_single_cursor(cursor);
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }

            SystemChange::FinalizePendingFocusChanges => {
                // A `set_focus` issued before the first layout (a create
                // callback, most commonly) is parked on the focus manager and
                // recovered here. It is drained BEFORE the gate below, not
                // inside `finalize_pending_focus_changes`, for two reasons:
                // that call only happens when `needs_cursor_initialization()`
                // is already set — which a deferred target does not set — and
                // only the shell can arm a real OS blink timer. The engine can
                // put the `Timer` in its own map, which is all a headless / E2E
                // host needs and nothing a desktop shell does.
                let deferred_blink = self
                    .get_layout_window_mut()
                    .and_then(azul_layout::window::LayoutWindow::drain_deferred_focus_target);
                if let Some(timer) = deferred_blink {
                    if let Some(lw) = self.get_layout_window_mut() {
                        lw.timers
                            .insert(azul_core::task::CURSOR_BLINK_TIMER_ID, timer.clone());
                    }
                    self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                }

                let timer_creation_needed = if let Some(layout_window) = self.get_layout_window_mut() {
                    let needs_init = layout_window.focus_manager.needs_cursor_initialization();
                    if needs_init {
                        let cursor_initialized = layout_window.finalize_pending_focus_changes();
                        if cursor_initialized {
                            if !layout_window.text_edit_manager.blink.is_blink_timer_active() {
                                layout_window.text_edit_manager.blink.set_blink_timer_active(true);
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

                    let scroll_type = if let Some(_focused_node) = layout_window.focus_manager.focused_node {
                        let has_range = layout_window.text_edit_manager.multi_cursor.as_ref()
                            .map(|mc| mc.selections.iter().any(|s| matches!(&s.selection, azul_core::selection::Selection::Range(_))))
                            .unwrap_or(false);
                        if has_range {
                            SelectionScrollType::Selection
                        } else {
                            SelectionScrollType::Cursor
                        }
                    } else {
                        return ProcessEventResult::DoNothing;
                    };

                    layout_window.scroll_selection_into_view(scroll_type, ScrollMode::Instant);
                    return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
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
                // The canonical reveal, not a second one. This arm used to
                // carry its own inline copy — 5px padding, an INSTANT
                // `scroll_manager.scroll_by`, blind to `caret_scroll_glide` —
                // while `CreateTextInput` already called
                // `scroll_selection_into_view` for the same keystroke. Typing
                // therefore issued TWO reveals per pass with different
                // semantics, and the one the user saw was whichever ran last.
                if let Some(layout_window) = self.get_layout_window_mut() {
                    if layout_window.scroll_selection_into_view(
                        azul_layout::window::SelectionScrollType::Cursor,
                        azul_layout::window::ScrollMode::Instant,
                    ) {
                        return ProcessEventResult::ShouldReRenderCurrentWindow;
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
        use azul_layout::managers::hover::InputPointId;

        // Single dispatch via CommonWindowState — handles GPU vs CPU internally
        let hit_test = self.get_common_mut().perform_hit_test(position);

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
    /// The deepest node under the point where the current drag gesture was
    /// PRESSED - the drag's source element. `None` when no gesture session
    /// is running.
    fn drag_source_node(&mut self) -> Option<azul_core::dom::DomNodeId> {
        let start = self.drag_press_position()?;
        let hit = self.get_common_mut().perform_hit_test(start);
        // The front-most hit (by hit depth, not by NodeId: a grafted
        // inline-docked panel sits under a zone with a higher id).
        azul_layout::managers::hover::deepest_node_across_doms(&hit)
    }

    /// Where the current drag gesture was pressed (window-local).
    fn drag_press_position(&self) -> Option<azul_core::geom::LogicalPosition> {
        Some(
            self.get_layout_window()?
                .gesture_drag_manager
                .get_current_session()?
                .first_sample()?
                .position,
        )
    }

    /// Does this node — or an ancestor — declare `-azul-app-region: drag`?
    ///
    /// The property does NOT cascade, deliberately: a drag region names the
    /// element that is draggable, and inheriting it would make every label and
    /// icon inside a title bar drag the window. But a press usually lands on a
    /// CHILD of the bar (the title text), so the lookup walks up until it finds
    /// a declaration. A child that declares `no-drag` — a close button — stops
    /// the walk at itself and is never treated as draggable, which is exactly
    /// the escape hatch Electron's `no-drag` provides.
    fn node_is_window_drag_region(&self, target: azul_core::dom::DomNodeId) -> bool {
        use azul_css::props::style::transform::StyleAppRegion;

        let Some(lw) = self.get_layout_window() else {
            return false;
        };
        let Some(lr) = lw.layout_results.get(&target.dom) else {
            return false;
        };
        let Some(mut node) = target.node.into_crate_internal() else {
            return false;
        };

        let hierarchy = lr.styled_dom.node_hierarchy.as_container();
        let states = lr.styled_dom.styled_nodes.as_container();
        for _ in 0..32 {
            // Bounded: a title bar is never 32 levels deep, and an unbounded
            // walk on a malformed tree would hang the event loop.
            let Some(sn) = states.get(node) else { break };
            match azul_layout::solver3::getters::get_app_region(
                &lr.styled_dom,
                node,
                &sn.styled_node_state,
            ) {
                azul_layout::solver3::getters::MultiValue::Exact(StyleAppRegion::Drag) => {
                    return true
                }
                azul_layout::solver3::getters::MultiValue::Exact(StyleAppRegion::NoDrag) => {
                    return false
                }
                _ => {}
            }
            match hierarchy.get(node).and_then(|h| h.parent_id()) {
                Some(parent) => node = parent,
                None => break,
            }
        }
        false
    }

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

        // `-azul-app-region: drag` — Electron's rule, enforced by the FRAMEWORK
        // rather than by a callback the app has to remember to attach.
        //
        // A DragStart landing on a node that declares `drag` hands the gesture
        // to the WINDOW MANAGER (X11 _NET_WM_MOVERESIZE, Wayland
        // xdg_toplevel.move, Windows WM_NCLBUTTONDOWN/HTCAPTION, macOS
        // performWindowDragWithEvent:) and a DoubleClick on one toggles
        // maximize/restore, exactly as a native title bar does.
        //
        // Checked on the node the event TARGETS, walking up to its ancestors:
        // the property does not cascade, but a press usually lands on a child
        // of the bar — the text inside it — and the bar is what declared the
        // region. Walking up is how a click on the title text still drags,
        // while a button that sets `no-drag` stops the walk at itself.
        //
        // In a `<transient-window>` the drag is the TEAR-OFF drag instead:
        // the window's own pipeline moves it with the pointer and reports
        // the drop to the parent (see `common::transient`). A popup has no
        // window manager to hand the gesture to.
        for ev in events {
            use azul_core::events::EventType;
            // An INLINE-docked panel's grip being dragged in this (parent)
            // window: the panel has no window to move, so the drag only
            // decides on release - same container: nothing; another zone:
            // grafted there; the open: torn off into a toplevel there.
            let inline_tear_active = self.get_layout_window().is_some_and(|lw| lw.inline_tear.is_some());
            if inline_tear_active {
                match ev.event_type {
                    EventType::Drag => {
                        // Slide the frameless proxy so the grip stays under the
                        // cursor. The parent owns the gesture (the mouse went
                        // down here), so the parent writes the child window's
                        // origin every frame; the child just honours it.
                        if let Some(cursor) = self
                            .get_current_window_state()
                            .mouse_state
                            .cursor_position
                            .get_position()
                        {
                            let drive = self.get_layout_window().and_then(|lw| {
                                let (node, origin) = lw.inline_tear_origin_at(cursor)?;
                                let m = super::transient::inline_tear_mailbox(lw, node)?;
                                Some((m, origin))
                            });
                            if let Some((mailbox, origin)) = drive {
                                super::transient::drive_proxy(&mailbox, origin);
                                self.request_regeneration_all_windows();
                            }
                        }
                        continue;
                    }
                    EventType::DragEnd => {
                        let cursor = self
                            .get_current_window_state()
                            .mouse_state
                            .cursor_position
                            .get_position()
                            .unwrap_or(azul_core::geom::LogicalPosition::zero());
                        // Stop driving the proxy, then let the engine decide
                        // what the drop meant (dock back inline, re-dock onto
                        // another zone, or stay floating where it was dropped).
                        if let Some(mailbox) = self.get_layout_window().and_then(|lw| {
                            let node = lw.inline_tear?.node;
                            super::transient::inline_tear_mailbox(lw, node)
                        }) {
                            super::transient::release_proxy(&mailbox);
                        }
                        let changed = self
                            .get_layout_window_mut()
                            .is_some_and(|lw| lw.end_inline_tear(cursor));
                        if changed {
                            log_debug!(
                                super::debug_server::LogCategory::Window,
                                "[transient] inline panel dropped at {cursor:?}: re-laying out"
                            );
                        }
                        self.request_regeneration_all_windows();
                        continue;
                    }
                    _ => {}
                }
            }
            match ev.event_type {
                EventType::Drag if super::transient::tear_drag_active(self.get_current_window_state()) => {
                    let follows = self.window_follows_position_changes();
                    if let Some(position) = super::transient::tear_drag_move(self.get_current_window_state(), follows) {
                        self.get_common_mut()
                            .update_window_state(WindowStateSource::App, |ws| ws.position = position);
                    }
                    continue;
                }
                EventType::DragEnd if super::transient::tear_drag_active(self.get_current_window_state()) => {
                    let follows = self.window_follows_position_changes();
                    if super::transient::tear_drag_end(self.get_current_window_state(), follows) {
                        log_debug!(
                            super::debug_server::LogCategory::Window,
                            "[transient] tear-off drag ended; reporting the drop to the parent"
                        );
                        self.request_regeneration_all_windows();
                    }
                    continue;
                }
                _ => {}
            }
            let is_drag_start = matches!(ev.event_type, EventType::DragStart);
            let is_double = matches!(ev.event_type, EventType::DoubleClick);
            if !(is_drag_start || is_double) {
                continue;
            }
            // A DragStart targets the node under the pointer NOW - after the
            // drag threshold, by which time a fast press on a thin title strip
            // has already left it. The region is the one that was PRESSED
            // (W3C: `dragstart` fires on the source element), so hit-test the
            // gesture's start position; the current target is the fallback.
            let target = if is_drag_start {
                self.drag_source_node().unwrap_or(ev.target)
            } else {
                ev.target
            };
            let is_region = self.node_is_window_drag_region(target);
            log_debug!(
                super::debug_server::LogCategory::Input,
                "[app-region] {:?} on {:?} (drag region: {})",
                ev.event_type,
                target,
                is_region
            );
            if !is_region {
                continue;
            }
            if is_drag_start {
                let press = self.drag_press_position();
                if super::transient::tear_drag_begin(self.get_current_window_state(), press) {
                    log_debug!(
                        super::debug_server::LogCategory::Window,
                        "[transient] tear-off drag begins"
                    );
                    continue;
                }
                if super::transient::mailbox_of(self.get_current_window_state()).is_some() {
                    continue; // a popup without tearoff: the strip does nothing
                }
                // A grip inside an inline-docked panel: tear THAT off, not
                // the window it is docked in.
                let panel = target
                    .node
                    .into_crate_internal()
                    .filter(|_| target.dom == azul_core::dom::DomId::ROOT_ID)
                    .and_then(|n| self.get_layout_window().and_then(|lw| lw.inline_docked_panel_of(n)));
                if let (Some(panel), Some(press)) = (panel, press) {
                    let began = self
                        .get_layout_window_mut()
                        .is_some_and(|lw| lw.begin_inline_tear(panel, press));
                    if began {
                        log_debug!(
                            super::debug_server::LogCategory::Window,
                            "[transient] inline panel {panel:?} tear-off drag begins"
                        );
                        // The panel tore off immediately: create its frameless
                        // proxy window now so it follows the cursor from the
                        // first move, not only once the pointer leaves us.
                        self.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                        continue;
                    }
                }
                self.handle_begin_interactive_move();
            } else {
                // Double-click on a drag region toggles the frame, the way
                // double-clicking a native title bar does.
                let cur = self.get_common_mut().current_window_state().flags.frame;
                let next = if cur == azul_core::window::WindowFrame::Maximized {
                    azul_core::window::WindowFrame::Normal
                } else {
                    azul_core::window::WindowFrame::Maximized
                };
                self.get_common_mut().update_unsynced_state(|ws| ws.flags.frame = next);
            }
        }

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
                        EventFilter::Component(_) => {
                            // Lifecycle events (Mount/Unmount/Update/Resize) carry the
                            // target node in `event.target`; fire the callback on that
                            // node only. No propagation, no bubbling — this mirrors how
                            // the diff emits one SyntheticEvent per affected node.
                            let dom_id = event.target.dom;
                            let Some(node_id) = event.target.node.into_crate_internal() else {
                                continue;
                            };
                            let Some(lr) = layout_window.layout_results.get(&dom_id) else {
                                continue;
                            };
                            let ndc = lr.styled_dom.node_data.as_container();
                            let Some(nd) = ndc.get(node_id) else {
                                continue;
                            };
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

            planned
        };

        // ===================================================================
        // Phase 2: Invoke planned callbacks (mutable access)
        // ===================================================================
        if planned_callbacks.is_empty() {
            return (ProcessEventResult::DoNothing, Update::DoNothing, false);
        }

        let borrows = self.prepare_callback_invocation();
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
            if propagation_stopped && propagation_stopped_node.is_none_or(|(dom, nid)| {
                dom != planned.dom_id || nid != planned.node_id
            }) {
                // We crossed to a different node and stop_propagation was called → skip
                break;
            }

            let mut callback = LayoutCallback::from_core(planned.callback_data.callback);
            // Set the event target so `info.get_hit_node()` — and thus
            // `open_menu_for_hit_node()` / `get_hit_node_rect()` — resolves to the
            // node the event was dispatched to (was a null node before).
            let hit_node = azul_core::dom::DomNodeId {
                dom: planned.dom_id,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                    planned.node_id,
                )),
            };
            let (changes, update) = borrows.layout_window.invoke_single_callback_at(
                hit_node,
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
    /// a regeneration request — scrolling only requires a lightweight
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

        // NOTE: We intentionally do NOT call request_regeneration() here.
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
    /// - `button_state`: Button state bitfield (BUTTON_STATE_LEFT / RIGHT / MIDDLE)
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
                // No reliable absolute origin → fall back to window-local coords.
                azul_core::window::WindowPosition::Uninitialized
                | azul_core::window::WindowPosition::RelativeToParentWindow(_) => position,
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

        // The injectable clock, not the wall clock: a11y-driven scroll and caret
        // timing must freeze with the rest of the engine under `freeze_test_clock`,
        // or no e2e scenario can assert on them.
        let now = azul_core::task::Instant::now();

        // Delegate to LayoutWindow's process_accessibility_action
        // This has direct mutable access to all managers and returns affected nodes
        layout_window.process_accessibility_action(dom_id, node_id, action, now)
    }

    /// Dispatch the synthetic events an accessibility action produced
    /// (`process_accessibility_action`'s affected-nodes map) through the
    /// normal propagated-callback machinery. Every backend used to drop this
    /// map on the floor — a screen reader's activation (AT-SPI `do_action`,
    /// UIA Invoke, NSAccessibility press) was accepted on the bus, decoded to
    /// the correct node, and then never invoked any callback.
    #[cfg(feature = "a11y")]
    fn dispatch_accessibility_events(
        &mut self,
        affected: &BTreeMap<azul_core::dom::DomNodeId, (Vec<EventFilter>, bool)>,
    ) -> azul_core::callbacks::Update {
        use azul_core::events::{
            EventData, EventSource, EventType, FocusEventFilter, HoverEventFilter,
            KeyModifiers, MouseButton, MouseEventData, SyntheticEvent,
        };

        let timestamp = azul_core::task::Instant::System(std::time::Instant::now().into());
        let mut events = Vec::new();
        for (node, (filters, _needs_relayout)) in affected {
            // Synthetic pointer events carry the node's centre as the cursor
            // position so callbacks that read it (get_cursor_relative_to_node)
            // see a sane in-bounds point.
            let centre = self
                .get_layout_window()
                .and_then(|lw| lw.get_node_layout_rect(*node))
                .map(|r| azul_core::geom::LogicalPosition {
                    x: r.origin.x + r.size.width / 2.0,
                    y: r.origin.y + r.size.height / 2.0,
                })
                .unwrap_or(azul_core::geom::LogicalPosition { x: 0.0, y: 0.0 });
            let mouse_data = || {
                EventData::Mouse(MouseEventData {
                    position: centre,
                    button: MouseButton::Left,
                    buttons: 0,
                    modifiers: KeyModifiers::default(),
                })
            };
            for f in filters {
                let (event_type, data) = match f {
                    EventFilter::Hover(HoverEventFilter::MouseUp)
                    | EventFilter::Focus(FocusEventFilter::MouseUp) => {
                        (EventType::MouseUp, mouse_data())
                    }
                    EventFilter::Hover(HoverEventFilter::MouseDown)
                    | EventFilter::Focus(FocusEventFilter::MouseDown) => {
                        (EventType::MouseDown, mouse_data())
                    }
                    _ => continue,
                };
                events.push(SyntheticEvent::new(
                    event_type,
                    EventSource::Synthetic,
                    *node,
                    timestamp.clone(),
                    data,
                ));
            }
        }
        if events.is_empty() {
            return azul_core::callbacks::Update::DoNothing;
        }
        let (_, update, _) = self.dispatch_events_propagated(&events);
        update
    }

    /// Apply a batch of already-decoded accessibility actions and dispatch the
    /// callbacks they map to.
    ///
    /// This is the body of every backend's inherent `process_accessibility_actions`
    /// once that backend has polled its own action source. It was copy-pasted
    /// four times (windows / macos / x11 / wayland) before iOS, Android and
    /// headless needed a fifth, sixth and seventh copy — and a copy that drifts
    /// is how "the callback map was dropped and screen-reader activation did
    /// nothing" shipped in the first place. One body, seven callers.
    ///
    /// Returns `true` when at least one action produced affected nodes.
    ///
    /// That is NOT "the caller owes a redraw" — every caller redraws
    /// unconditionally after a non-empty batch, because Focus, Blur, the
    /// `Scroll*` family, `ScrollIntoView` and `SetTextSelection` all change
    /// manager state and map to no callback, so they return an EMPTY affected
    /// set while the screen has genuinely gone stale. The flag is for callers
    /// that want to know whether any callback path was reachable at all.
    #[cfg(feature = "a11y")]
    fn dispatch_accessibility_actions(
        &mut self,
        actions: Vec<(
            azul_core::dom::DomId,
            azul_core::dom::NodeId,
            azul_core::dom::AccessibilityAction,
        )>,
    ) -> bool {
        if actions.is_empty() {
            return false;
        }

        // The injectable clock, not the wall clock: a11y-driven scroll and caret
        // timing must freeze with the rest of the engine under `freeze_test_clock`,
        // or no e2e scenario can assert on them.
        let now = azul_core::task::Instant::now();
        let mut anything_changed = false;

        for (dom_id, node_id, action) in actions {
            let affected = match self.get_layout_window_mut() {
                Some(lw) => lw.process_accessibility_action(dom_id, node_id, action, now.clone()),
                None => continue,
            };
            if affected.is_empty() {
                continue;
            }
            anything_changed = true;
            self.get_common_mut().display_list_dirty = true;
            // Invoke the callbacks the action mapped to (synthetic MouseUp for
            // the Default/click action, etc.) — dropping this map is exactly
            // the bug that made screen-reader activation a no-op.
            let update = self.dispatch_accessibility_events(&affected);
            if !matches!(update, azul_core::callbacks::Update::DoNothing) {
                // The callback asked for a refresh (e.g. RefreshDom from a zoom
                // button) — regenerate on the next frame, exactly like
                // pointer-event dispatch does.
                self.get_common_mut()
                    .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
            }
        }

        self.get_common_mut().a11y_dirty = true;
        anything_changed
    }

    /// Run an incremental relayout (see [`CommonWindowState::incremental_relayout`]:
    /// layout on the existing StyledDom + the CPU hit-tester rebuild) AND
    /// deliver the lifecycle events that pass produced — today that is
    /// `Resize` (`NodeResized`) for every subscribed node whose box changed.
    ///
    /// Does this backend's MENU BAR fire its accelerators natively? AppKit
    /// turns a menu item's chord into a key equivalent and runs the item
    /// before the key ever reaches the view, so the shared dispatch must
    /// leave the menu bar alone there (it would fire twice). Context menus
    /// are never native-accelerated anywhere and always go through
    /// [`Self::dispatch_menu_accelerators`]. Default: no.
    fn native_menu_bar_accelerators(&self) -> bool {
        false
    }

    /// SHARED MENU-ACCELERATOR DISPATCH. Called once per event pass: if a key
    /// just went down (the keyboard state's `current_virtual_keycode` changed
    /// from the previous pass), look for a menu item whose accelerator
    /// matches the chord — the root DOM's menu bar (unless the backend's
    /// menu bar is natively accelerated), then the context menus of the
    /// focused node and its ancestors — and run its callback like a click
    /// on it would. Returns what the callback asked for, so the pass result
    /// carries it; `DoNothing` when nothing matched.
    ///
    /// Before this, a menu item's chord was display-only everywhere but the
    /// macOS menu bar: AzPaint's Ctrl+O / Ctrl+S did nothing on Windows and
    /// Linux. The chord rule itself is `azul_core::menu::accelerator_matches`
    /// (`[LWin, S]` = Cmd+S on a Mac, Ctrl+S elsewhere, exact modifiers).
    fn dispatch_menu_accelerators(&mut self) -> ProcessEventResult {
        use azul_core::dom::{DomId, NodeId};

        let pressed = {
            let now = self
                .get_current_window_state()
                .keyboard_state
                .current_virtual_keycode
                .into_option();
            let before = self
                .get_previous_window_state()
                .as_ref()
                .and_then(|p| p.keyboard_state.current_virtual_keycode.into_option());
            match now {
                Some(key) if before != Some(key) => key,
                _ => return ProcessEventResult::DoNothing,
            }
        };
        let keyboard = self.get_current_window_state().keyboard_state.clone();
        let native_bar = self.native_menu_bar_accelerators();

        let callback = self.get_layout_window().and_then(|lw| {
            let lr = lw.layout_results.get(&DomId::ROOT_ID)?;
            let nodes = lr.styled_dom.node_data.as_container();
            let from_bar = if native_bar {
                None
            } else {
                // SCAN for the node carrying the menu bar — it is NOT always
                // the root: on Linux `inject_software_menubar` wraps the
                // user's DOM in `Html [menubar widget, user body]`, so the
                // body that owns `.get_menu_bar()` sits several nodes deep.
                // A `NodeId::ZERO` lookup found nothing there and every
                // menu-bar accelerator was dead exactly on the platforms the
                // shared dispatch exists for (the headless test caught it on
                // ubuntu CI while passing on macOS, where nothing wraps).
                nodes
                    .internal
                    .iter()
                    .find_map(azul_core::dom::NodeData::get_menu_bar)
                    .and_then(|menu| menu.find_accelerated_item(&keyboard, pressed))
            };
            let from_context = if from_bar.is_some() {
                None
            } else {
                // The context menus a right-click on the focused node would
                // open: the node's own, then its ancestors'.
                let hierarchy = lr.styled_dom.node_hierarchy.as_container();
                let mut current = lw
                    .focus_manager
                    .get_focused_node()
                    .filter(|f| f.dom == DomId::ROOT_ID)
                    .and_then(|f| f.node.into_crate_internal());
                let mut found = None;
                while let Some(n) = current {
                    found = nodes
                        .get(n)
                        .and_then(azul_core::dom::NodeData::get_context_menu)
                        .and_then(|menu| menu.find_accelerated_item(&keyboard, pressed));
                    if found.is_some() {
                        break;
                    }
                    current = hierarchy.get(n).and_then(|h| h.parent_id());
                }
                found
            };
            from_bar
                .or(from_context)
                .and_then(|item| item.callback.as_ref().cloned())
        });

        match callback {
            Some(cb) => self.invoke_menu_callback(cb, MenuInvocation::Accelerator),
            None => ProcessEventResult::DoNothing,
        }
    }

    /// Run a menu item's callback with a full `CallbackInfo` — exactly what
    /// a click on the item does on every backend — and apply what it asked
    /// for (window-state changes, a DOM rebuild). One implementation for the
    /// native menu handlers (macOS tags, Win32 command ids, GNOME actions)
    /// and the shared accelerator dispatch.
    ///
    /// `how` decides who owns the window-state baseline afterwards. A NATIVE
    /// activation arrives outside any input pass — there is no pending input
    /// delta — so the baseline is advanced here, like after a click. An
    /// ACCELERATOR runs INSIDE `process_window_events`, before the DOM
    /// dispatch, and the very key that fired it is the delta that pass is
    /// about to consume: snapshotting here tripped `AZ_VALIDATE`
    /// ("unconsumed input delta at menu.accelerator") and, without the
    /// validator, would have deleted the key-down for the DOM.
    fn invoke_menu_callback(
        &mut self,
        callback: azul_core::menu::CoreMenuCallback,
        how: MenuInvocation,
    ) -> ProcessEventResult {
        use azul_core::callbacks::Update;
        use azul_layout::callbacks::{Callback, MenuCallback};

        let raw_handle = self.get_raw_window_handle();
        let mut menu_callback = MenuCallback {
            callback: Callback::from_core(callback.callback),
            refany: callback.refany,
        };
        let (changes, update) = {
            let common = self.get_common_mut();
            let borrows = common.layout_borrows();
            let Some(layout_window) = borrows.layout_window else {
                return ProcessEventResult::DoNothing;
            };
            layout_window.invoke_single_callback(
                &mut menu_callback.callback,
                &mut menu_callback.refany,
                &raw_handle,
                borrows.gl_context_ptr,
                borrows.system_style.clone(),
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                borrows.previous_window_state,
                borrows.current_window_state,
                borrows.renderer_resources,
            )
        };

        if let MenuInvocation::Native { site } = how {
            self.snapshot_window_state_baseline(site);
        }
        let mut result = ProcessEventResult::DoNothing;
        for change in &changes {
            result = result.max(self.apply_user_change(change));
        }
        if matches!(update, Update::RefreshDom | Update::RefreshDomAllWindows) {
            result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
        }
        if update == Update::RefreshDomAllWindows
            || result == ProcessEventResult::ShouldRegenerateDomAllWindows
        {
            // A menu item's action mutates SHARED app state: every other
            // window (a popup's parent, say) must re-layout too, not only
            // the one the menu belonged to.
            result = result.max(ProcessEventResult::ShouldRegenerateDomAllWindows);
            self.request_regeneration_all_windows();
        }
        // Window-state changes the callback made (title, size, close) reach
        // the OS now, as after a native click. The surrounding input pass
        // syncs after an accelerator.
        if matches!(how, MenuInvocation::Native { .. }) {
            self.sync_window_state();
        }
        if matches!(
            result,
            ProcessEventResult::ShouldRegenerateDomCurrentWindow
                | ProcessEventResult::ShouldRegenerateDomAllWindows
                | ProcessEventResult::ShouldIncrementalRelayout
                | ProcessEventResult::UpdateHitTesterAndProcessAgain
        ) {
            self.get_common_mut()
                .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        }
        result
    }

    /// The relayout-only paths (restyle, runtime edit, the coalesced window
    /// resize) never reconciled, so nothing drained the lifecycle queue after
    /// them; a `NodeResized` callback would have run at the NEXT full
    /// rebuild, against stale geometry. A callback that asks for a rebuild
    /// gets it right here, through the bounded lifecycle loop of
    /// [`Self::regenerate_layout`]: a request merely latched for later would
    /// be retired by the relayout-only frame branch that follows every one
    /// of these call sites.
    ///
    /// `CommonWindowState::incremental_relayout` is private to `common` so a
    /// backend cannot take the relayout without this delivery.
    fn incremental_relayout_dispatching(
        &mut self,
        kind: IncrementalRelayout,
        debug_messages: &mut Option<Vec<azul_css::LayoutDebugMessage>>,
    ) -> Result<(), String> {
        self.get_common_mut().incremental_relayout(kind, debug_messages)?;
        if self.dispatch_pending_lifecycle_events() {
            self.regenerate_layout()?;
        }
        Ok(())
    }

    /// Drain `LayoutWindow.pending_lifecycle_events` and dispatch each event.
    ///
    /// Reconciliation (see `common::layout::regenerate_layout`) queues
    /// Mount / Unmount / Update / Resize `SyntheticEvent`s for affected nodes.
    /// This method pops the queue and routes them through `dispatch_events_propagated`,
    /// which handles `EventFilter::Component(_)` by invoking callbacks on the
    /// event's target node (no capture/bubble phases — lifecycle events are
    /// single-target).
    ///
    /// Call this after `regenerate_layout` completes so callbacks observe a
    /// consistent post-layout DOM. Returning `true` means at least one callback
    /// reported `Update::Refresh(Dom)` and the caller should regenerate again.
    fn dispatch_pending_lifecycle_events(&mut self) -> bool {
        // Snapshot both queues up front so we hold no borrow on the layout
        // window when invoking callbacks (callbacks may mutate it).
        let (events, unmount_invocations) = match self.get_layout_window_mut() {
            Some(lw) => (
                core::mem::take(&mut lw.pending_lifecycle_events),
                core::mem::take(&mut lw.pending_unmount_invocations),
            ),
            None => return false,
        };

        let mut any_refresh = false;

        if !events.is_empty() {
            let (_, update, _) = self.dispatch_events_propagated(&events);
            if !matches!(update, azul_core::callbacks::Update::DoNothing) {
                any_refresh = true;
            }
        }

        // BeforeUnmount callbacks were resolved against the OLD node data at
        // diff time, so we already have a `(CoreCallbackData, SyntheticEvent)`
        // pair for each one. Invoke directly via `invoke_single_callback` —
        // there is no DOM lookup to perform (the OLD NodeId is stale by now).
        if !unmount_invocations.is_empty() {
            let borrows = self.prepare_callback_invocation();
            let mut all_changes: Vec<azul_layout::callbacks::CallbackChange> = Vec::new();
            for (callback_data, _event) in &unmount_invocations {
                let mut callback =
                    azul_layout::callbacks::Callback::from_core(callback_data.callback.clone());
                let (changes, update) = borrows.layout_window.invoke_single_callback(
                    &mut callback,
                    &mut callback_data.refany.clone(),
                    &borrows.window_handle,
                    borrows.gl_context_ptr,
                    borrows.system_style.clone(),
                    &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                    borrows.previous_window_state,
                    borrows.current_window_state,
                    borrows.renderer_resources,
                );
                if !matches!(update, azul_core::callbacks::Update::DoNothing) {
                    any_refresh = true;
                }
                all_changes.extend(changes);
            }
            drop(borrows);
            for change in &all_changes {
                let _ = self.apply_user_change(change);
            }
        }

        any_refresh
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
        // Observability wrapper (E2E): record how much work this event took, so
        // that an invalidation loop TRIPS AN ASSERTION instead of being silently
        // capped at MAX_EVENT_RECURSION_DEPTH and log_warn'd. The counters are
        // sticky until `reset_frame_counters`.
        #[allow(clippy::cast_possible_truncation)]
        let depth_u32 = depth as u32;
        if let Some(lw) = self.get_layout_window_mut() {
            lw.sync_frame_report();
            let r = &mut lw.frame_report;
            r.relayout_iterations = r.relayout_iterations.max(depth_u32 + 1);
        }

        let mut result = self.process_window_events_inner(depth);

        // A callback that ran INSIDE a transient popup and asked for a
        // refresh must refresh the PARENT: the popup only mirrors the
        // parent's subtree (see `common::transient`), so re-laying the popup
        // out alone would show the old content. The parent's pass then
        // pushes the new subtree back into the popup's mailbox.
        if result == ProcessEventResult::ShouldRegenerateDomCurrentWindow
            && super::transient::mailbox_of(self.get_current_window_state()).is_some()
        {
            result = ProcessEventResult::ShouldRegenerateDomAllWindows;
        }

        // CONSUME the state delta this pass just processed. The
        // previous→current diff is a one-shot: without this, the delta
        // survives the pass, and the NEXT pass — a redraw tick, a
        // wait_frame pump, the regeneration pass after a callback returned
        // RefreshDom, anything that calls process_window_events() without
        // changing state first — re-detects the SAME transition and
        // re-dispatches its events. One physical mouse release invoked a
        // Hover(MouseUp) callback twice that way (every toggle callback
        // self-cancelled); a held KeyDown or a WindowResize could repeat
        // likewise. It runs AFTER the pass (not at determination time)
        // because callbacks legitimately read the pre-pass state through
        // CallbackInfo::get_previous_window_state() during dispatch.
        //
        // Every event-injection site (the platform handlers, the headless
        // arms, ModifyWindowState, QueueWindowStateSequence) advances
        // previous BEFORE mutating current and immediately runs its own
        // pass, so consuming here never eats an unprocessed delta — it
        // makes "a completed pass leaves no live delta behind" an invariant
        // of this function instead of a convention every caller must
        // remember. The depth-cap early-out is exempt: that pass never ran
        // determination, so its delta must stay for the next regular pass.
        if depth < MAX_EVENT_RECURSION_DEPTH {
            let consumed = self.get_current_window_state().clone();
            self.set_previous_window_state(consumed);
        }

        if let Some(lw) = self.get_layout_window_mut() {
            lw.frame_report.terminal_result = result as u8;
        }

        // Arm the caret / selection tween driver if the display-list pass this
        // event triggered left a tween in flight (LayoutWindow::apply_text_tweens
        // publishes the flag). One shared site for every backend — the timer
        // self-terminates via its RefAny'd flag when the tween finishes, so
        // there is no matching stop call to keep in sync.
        {
            use azul_core::task::CARET_TWEEN_TIMER_ID;
            let needs_tween_timer = self
                .get_layout_window()
                .map(|lw| {
                    lw.text_edit_manager.tween.is_active()
                        && !lw.timers.contains_key(&CARET_TWEEN_TIMER_ID)
                })
                .unwrap_or(false);
            if needs_tween_timer {
                let timer = self
                    .get_layout_window()
                    .map(|lw| lw.create_caret_tween_timer());
                if let Some(timer) = timer {
                    if let Some(lw) = self.get_layout_window_mut() {
                        lw.timers.insert(CARET_TWEEN_TIMER_ID, timer.clone());
                    }
                    self.start_timer(CARET_TWEEN_TIMER_ID.id, timer);
                }
            }

            // Arm the scroll-physics timer for WINDOW-LEVEL queue pushes:
            // caret-into-view glides (ledger #8) and callback
            // scroll_to_animated land in the shared ScrollInputQueue outside
            // any platform wheel handler, so no shell ever armed the timer
            // for them — the queue sat undrained until the next physical
            // wheel event. One shared site, same self-termination as wheel
            // scrolling (the physics timer exits when idle).
            {
                use azul_core::task::SCROLL_MOMENTUM_TIMER_ID;
                let needs_scroll_timer = self
                    .get_layout_window()
                    .map(|lw| {
                        lw.scroll_manager.scroll_input_queue.has_pending()
                            && !lw.timers.contains_key(&SCROLL_MOMENTUM_TIMER_ID)
                    })
                    .unwrap_or(false);
                if needs_scroll_timer {
                    let timer = self.get_layout_window().map(|lw| {
                        use azul_core::refany::RefAny;
                        use azul_layout::scroll_timer::{
                            scroll_physics_timer_callback, ScrollPhysicsState,
                        };
                        use azul_layout::timer::{Timer, TimerCallbackType};
                        let physics = lw
                            .system_style
                            .as_ref()
                            .map(|s| s.scroll_physics.clone())
                            .unwrap_or_default();
                        let interval_ms = physics.timer_interval_ms.max(1);
                        let state =
                            ScrollPhysicsState::new(lw.scroll_manager.get_input_queue(), physics);
                        Timer::create(
                            RefAny::new(state),
                            scroll_physics_timer_callback as TimerCallbackType,
                            azul_layout::callbacks::ExternalSystemCallbacks::rust_internal()
                                .get_system_time_fn,
                        )
                        .with_interval(azul_core::task::Duration::System(
                            azul_core::task::SystemTimeDiff::from_millis(u64::from(interval_ms)),
                        ))
                    });
                    if let Some(timer) = timer {
                        if let Some(lw) = self.get_layout_window_mut() {
                            lw.timers.insert(SCROLL_MOMENTUM_TIMER_ID, timer.clone());
                        }
                        self.start_timer(SCROLL_MOMENTUM_TIMER_ID.id, timer);
                    }
                }
            }
        }

        result
    }

    /// The real body of [`Self::process_window_events`] — see that method.
    fn process_window_events_inner(&mut self, depth: usize) -> ProcessEventResult {

        if depth >= MAX_EVENT_RECURSION_DEPTH {
            log_warn!(
                super::debug_server::LogCategory::EventLoop,
                "[PlatformWindow] Max event recursion depth {} reached",
                MAX_EVENT_RECURSION_DEPTH
            );
            if let Some(lw) = self.get_layout_window_mut() {
                lw.frame_report.hit_depth_cap = true;
            }
            return ProcessEventResult::DoNothing;
        }

        // MWA-A1: drain the async capability channels (gamepad / sensors /
        // geolocation / permission / biometric / keyring) BEFORE event
        // determination, so pending-event flags raised by the drain feed
        // THIS pass — a pad press or GPS fix becomes its event with no
        // +1-pass latency. Then re-sync the pump timer to the current
        // subscription set (armed only while some source needs unsolicited
        // wake-ups; see common/capability_pump.rs — timer-only by design,
        // no pump thread exists).
        if depth == 0 {
            if let Some(lw) = self.get_layout_window_mut() {
                super::capability_pump::pump(lw);
            }
            self.sync_capability_pump_timer();
            self.process_transient_dismissal();
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

        // Get EventProvider managers (text input, sensors, gamepad,
        // geolocation, permission, biometric, keyring — the last four added
        // by MWA-A1/A1b so async capability outcomes become events instead
        // of silently updating manager state).
        let providers_ref = self.get_layout_window().map(|w| {
            (
                &w.text_input_manager,
                &w.sensor_manager,
                &w.gamepad_manager,
                &w.geolocation_manager,
                &w.permission_manager,
                &w.biometric_manager,
                &w.keyring_manager,
                &w.eyedropper_manager,
            )
        });

        // C11: the pending STRUCTURAL edit's push notification — an owned
        // snapshot provider (target + id + notified flag), emitting one
        // DocumentEdit event per changeset. Marked delivered right after
        // determination below.
        let document_edit_provider = self
            .get_layout_window()
            .map(|w| w.document_edit_event_provider());

        // Build list of EventProvider managers
        let mut event_providers: Vec<&dyn azul_core::events::EventProvider> = Vec::new();
        if let Some(p) = document_edit_provider.as_ref() {
            event_providers.push(p as &dyn azul_core::events::EventProvider);
        }
        if let Some((tm, sm, gm, geo, pm, bm, km, ed)) = providers_ref {
            event_providers.push(tm as &dyn azul_core::events::EventProvider);
            event_providers.push(sm as &dyn azul_core::events::EventProvider);
            event_providers.push(gm as &dyn azul_core::events::EventProvider);
            event_providers.push(geo as &dyn azul_core::events::EventProvider);
            event_providers.push(pm as &dyn azul_core::events::EventProvider);
            event_providers.push(bm as &dyn azul_core::events::EventProvider);
            event_providers.push(km as &dyn azul_core::events::EventProvider);
            event_providers.push(ed as &dyn azul_core::events::EventProvider);
        }

        // Get current timestamp
        #[cfg(feature = "std")]
        let timestamp = azul_core::task::Instant::from(std::time::Instant::now());
        #[cfg(not(feature = "std"))]
        let timestamp = azul_core::task::Instant::Tick(azul_core::task::SystemTick::new(0));

        // Raw wheel delta recorded by the platform scroll handler this pass (if
        // any). Drives a synthesized Scroll event aimed at the hovered node so
        // wheel-as-zoom widgets (the map) react; cleared right after dispatch.
        let wheel_delta = self
            .get_layout_window()
            .and_then(|w| w.scroll_manager.pending_wheel_event);

        // Determine all events (returns Vec<SyntheticEvent>)
        let mut synthetic_events = if let (Some(fm), Some(fdm), Some(hm)) =
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
                wheel_delta,
                timestamp,
            )
        } else {
            // Fallback: no events if managers not available
            Vec::new()
        };

        // PRESS-TARGET CAPTURE: the node a button was pressed on gets that
        // button's release even when the pointer released elsewhere (or the
        // window lost focus and the OS handler cleared the button). See
        // `HoverManager::apply_press_target_capture` — this is THE fix for the
        // "stuck input" family; widgets no longer need "leave = release".
        if let Some(lw) = self.get_layout_window_mut() {
            let layout_results = &lw.layout_results;
            let in_release_path = |press: azul_core::dom::DomNodeId,
                                   release: azul_core::dom::DomNodeId|
             -> bool {
                // Is `press` the release target or one of its DOM ancestors
                // (i.e. already on the release's propagation path)?
                if press.dom != release.dom {
                    return false;
                }
                let (Some(press_node), Some(mut current)) =
                    (press.node.into_crate_internal(), release.node.into_crate_internal())
                else {
                    return false;
                };
                let Some(lr) = layout_results.get(&release.dom) else {
                    return false;
                };
                let hierarchy = lr.styled_dom.node_hierarchy.as_container();
                loop {
                    if current == press_node {
                        return true;
                    }
                    match hierarchy.get(current).and_then(|n| n.parent_id()) {
                        Some(parent) => current = parent,
                        None => return false,
                    }
                }
            };
            lw.hover_manager
                .apply_press_target_capture(&mut synthetic_events, &in_release_path);
        }

        // Pointer capture: while a node holds the pointer, moves and the
        // release go to IT, not to whatever the hit test found. The capture
        // ends with the release (W3C `setPointerCapture` semantics).
        if let Some(captured) = self.get_layout_window().and_then(|lw| lw.pointer_capture) {
            use azul_core::events::EventType;
            let mut released = false;
            for ev in &mut synthetic_events {
                if matches!(ev.event_type, EventType::MouseOver | EventType::MouseUp) {
                    ev.target = captured;
                    ev.current_target = captured;
                    released |= ev.event_type == EventType::MouseUp;
                }
            }
            if released {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.pointer_capture = None;
                }
            }
        }

        // Clear the sensor/gamepad pending-event flags now that this pass has
        // collected their events (the immutable event_providers borrow ended
        // above). One SensorChanged/GamepadInput fires per change, not per frame.
        if let Some(w) = self.get_layout_window_mut() {
            // C11: the DocumentEdit notification (if any) was collected by THIS
            // pass's determination and will be dispatched below — one event per
            // changeset. From here on, a re-render without an ack rejects the
            // edit (drop honored in layout_and_generate_display_list).
            if synthetic_events
                .iter()
                .any(|e| e.event_type == azul_core::events::EventType::DocumentEdit)
            {
                w.mark_document_edit_notified();
            }
            w.sensor_manager.clear_pending_event();
            w.gamepad_manager.clear_pending_event();
            w.geolocation_manager.clear_pending_event();
            w.permission_manager.clear_pending_changed();
            w.biometric_manager.clear_pending_event();
            w.keyring_manager.clear_pending_event();
            w.eyedropper_manager.clear_pending_event();
            w.gesture_drag_manager.clear_pen_event_pending();
            // The injected native gesture (macOS magnify/rotate, debug-server
            // injection) is NOT cleared here: the PinchIn/PinchOut callbacks
            // this pass is about to dispatch read it live through
            // `CallbackInfo::get_pinch()`. It is cleared right after the
            // dispatch below, next to the wheel delta — the same rule.
            // MWA-B12: a LongPress was just emitted for this hold — mark it
            // so it doesn't re-fire on every later pass of the same session.
            if synthetic_events
                .iter()
                .any(|e| e.event_type == azul_core::events::EventType::LongPress)
            {
                w.gesture_drag_manager.mark_current_long_press_invoked();
            }
        }

        // MENU ACCELERATORS: a key that just went down may be a menu item's
        // chord (see `dispatch_menu_accelerators`). Runs before the DOM
        // dispatch, like AppKit's key equivalents; the key still reaches the
        // DOM afterwards (a chord never types a character).
        let accelerator_result = self.dispatch_menu_accelerators();

        if synthetic_events.is_empty() {
            return accelerator_result;
        }

        // Tooltip-delay timer: on hover transitions onto (or off of) a node
        // that advertises a tooltip source (title/alt/aria-label), start or
        // stop `TOOLTIP_DELAY_TIMER_ID`. Delay comes from
        // `SystemStyle::input_metrics.hover_time_ms` (SPI_GETMOUSEHOVERTIME on
        // Windows, default 400ms). Timer callback emits ShowTooltip on expiry.
        {
            let hover_time_ms = self.get_system_style().input.hover_time_ms;
            let tooltip_action = self
                .get_layout_window()
                .map(|lw| lw.handle_hover_change_for_tooltip(hover_time_ms));

            if let Some(action) = tooltip_action {
                match action {
                    azul_layout::TooltipTimerAction::Start(timer) => {
                        if let Some(lw) = self.get_layout_window_mut() {
                            lw.timers
                                .insert(azul_core::task::TOOLTIP_DELAY_TIMER_ID, timer.clone());
                        }
                        self.start_timer(
                            azul_core::task::TOOLTIP_DELAY_TIMER_ID.id,
                            timer,
                        );
                    }
                    azul_layout::TooltipTimerAction::Stop => {
                        if let Some(lw) = self.get_layout_window_mut() {
                            lw.timers.remove(&azul_core::task::TOOLTIP_DELAY_TIMER_ID);
                        }
                        self.stop_timer(azul_core::task::TOOLTIP_DELAY_TIMER_ID.id);
                        self.hide_tooltip_from_callback();
                    }
                    azul_layout::TooltipTimerAction::NoChange => {}
                }
            }
        }

        // MWA-A3c: incremental :hover restyle. Enter/leave events dispatched
        // to callbacks, but the styled DOM's :hover flags were never updated
        // outside a full DOM regeneration — pure-CSS hover styling was dead
        // on every backend. Collect this pass's MouseEnter/MouseLeave
        // targets per DOM and restyle now; the outcome merges into the pass
        // result at the bottom of this function.
        let hover_restyle_result: Option<ProcessEventResult> = {
            use std::collections::BTreeMap;
            let mut per_dom: BTreeMap<
                azul_core::dom::DomId,
                azul_core::styled_dom::HoverChange,
            > = BTreeMap::new();
            for ev in &synthetic_events {
                let is_enter = ev.event_type == azul_core::events::EventType::MouseEnter;
                let is_leave = ev.event_type == azul_core::events::EventType::MouseLeave;
                if !is_enter && !is_leave {
                    continue;
                }
                let Some(node) = ev.target.node.into_crate_internal() else {
                    continue;
                };
                let entry = per_dom.entry(ev.target.dom).or_insert_with(|| {
                    azul_core::styled_dom::HoverChange {
                        left_nodes: Vec::new(),
                        entered_nodes: Vec::new(),
                    }
                });
                if is_enter {
                    entry.entered_nodes.push(node);
                } else {
                    entry.left_nodes.push(node);
                }
            }
            if per_dom.is_empty() {
                None
            } else {
                self.get_layout_window_mut()
                    .map(|lw| apply_hover_restyle(lw, per_dom))
            }
        };

        // MWA-B12: arm the one-shot long-press wake-up on every MouseDown —
        // a motionless press generates no further events, so no pass would
        // ever evaluate detect_long_press (the press only ever fired if the
        // user happened to wiggle the mouse after the threshold).
        {
            let has_mouse_down = synthetic_events
                .iter()
                .any(|ev| ev.event_type == azul_core::events::EventType::MouseDown);
            if has_mouse_down {
                let threshold_ms = self
                    .get_layout_window()
                    .map(|lw| lw.gesture_drag_manager.config.long_press_time_threshold_ms);
                if let Some(threshold_ms) = threshold_ms {
                    // +15ms so the pass runs safely past the threshold.
                    let timer =
                        super::capability_pump::make_one_shot_pass_timer(threshold_ms + 15);
                    if let Some(lw) = self.get_layout_window_mut() {
                        lw.add_timer(azul_core::task::LONG_PRESS_TIMER_ID, timer.clone());
                    }
                    self.start_timer(azul_core::task::LONG_PRESS_TIMER_ID.id, timer);
                }
            }
        }

        // MWA-B8: keep the auto-scroll timer alive for NON-TEXT drags too —
        // node DnD and OS file hovers never auto-scrolled (TextSelectionDrag
        // was the only StartAutoScrollTimer trigger). The start handler is
        // idempotent (checks timers.contains_key).
        let autoscroll_start_result: Option<ProcessEventResult> = {
            let wants = synthetic_events.iter().any(|ev| {
                matches!(
                    ev.event_type,
                    azul_core::events::EventType::FileHover
                        | azul_core::events::EventType::Drag
                        | azul_core::events::EventType::DragStart
                )
            });
            if wants {
                Some(self.apply_system_change(&SystemChange::StartAutoScrollTimer))
            } else {
                None
            }
        };

        // MWA-C-gesture: cancel an active drag on Escape or window blur.
        // cancel_drag / DeactivateDrag existed but nothing invoked them from
        // input — a drag survived focus loss (Alt-Tab mid-drag left the node
        // stuck to a phantom cursor) and there was no keyboard escape hatch.
        let drag_cancel_result: Option<ProcessEventResult> = {
            let drag_active = self
                .get_layout_window()
                .is_some_and(|lw| lw.gesture_drag_manager.is_dragging());
            let wants_cancel = drag_active
                && synthetic_events.iter().any(|ev| {
                    match ev.event_type {
                        azul_core::events::EventType::WindowFocusOut => true,
                        azul_core::events::EventType::KeyDown => matches!(
                            self.get_current_window_state()
                                .keyboard_state
                                .current_virtual_keycode,
                            azul_core::window::OptionVirtualKeyCode::Some(
                                azul_core::window::VirtualKeyCode::Escape,
                            )
                        ),
                        _ => false,
                    }
                });
            if wants_cancel {
                Some(self.apply_system_change(&SystemChange::DeactivateDrag))
            } else {
                None
            }
        };

        // MWA-C-focus_cursor: pause the caret blink while the window is
        // unfocused — the timer kept background windows repainting every
        // ~530ms and the caret blinked without key focus. On refocus,
        // restart blinking if an editable node still holds focus (the caret
        // stays drawn solid while blurred; only the blink pauses).
        {
            let focus_out = synthetic_events
                .iter()
                .any(|ev| ev.event_type == azul_core::events::EventType::WindowFocusOut);
            let focus_in = synthetic_events
                .iter()
                .any(|ev| ev.event_type == azul_core::events::EventType::WindowFocusIn);
            if focus_out && !focus_in {
                // Pause: keep ALL editing/blink state, only the OS timer dies.
                let was_running = self
                    .get_layout_window_mut()
                    .map(|lw| lw.pause_cursor_blink_for_window_blur())
                    .unwrap_or(false);
                if was_running {
                    self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                }
            } else if focus_in {
                // Resume: state-preserving and idempotent. The old code routed
                // this through handle_focus_change_for_cursor_blink, which
                // never restarted the paused timer (logical flag still true →
                // NoChange) and, with engine focus None (app-driven caret,
                // or focus churn from a KWin interactive-resize grab),
                // CLEARED the editing state — caret and selection wiped.
                let timer = self.get_layout_window_mut().and_then(|lw| {
                    let ws = lw.current_window_state.clone();
                    lw.resume_cursor_blink_after_window_focus(&ws)
                });
                if let Some(timer) = timer {
                    self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                }
            }
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
        // NOTE: DragStart hit tests are stored directly in the DragContext
        // when the drag is activated. No separate hit test update needed here.

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

        // Latch the anchor of a text-selection drag on the PRESS, and only when
        // that press landed on an editable. See
        // `LayoutWindow::text_selection_drag_anchor` for what the previous
        // "current cursor position whenever left_down && editing" produced:
        // dragging the window by its custom titlebar became a selection drag
        // (armed drag-autoscroll, scrolled the UI to the top), and anchor ==
        // current made every selection range empty.
        {
            let (left_down, pos) = {
                let st = self.get_current_window_state();
                (st.mouse_state.left_down, st.mouse_state.cursor_position.get_position())
            };
            let press_on_editable = hit_test_for_dispatch.as_ref().is_some_and(|ht| {
                self.get_layout_window().is_some_and(|lw| {
                    ht.hovered_nodes.iter().any(|(dom_id, hit)| {
                        lw.layout_results.get(dom_id).is_some_and(|lr| {
                            hit.regular_hit_test_nodes.keys().any(|nid| {
                                azul_layout::solver3::getters::is_node_contenteditable_inherited(
                                    &lr.styled_dom,
                                    *nid,
                                )
                            })
                        })
                    })
                })
            });
            if let Some(lw) = self.get_layout_window_mut() {
                if left_down && !lw.prev_left_down {
                    // Press edge: this is the only moment an anchor is born.
                    lw.text_selection_drag_anchor = if press_on_editable { pos } else { None };
                } else if !left_down {
                    lw.text_selection_drag_anchor = None;
                }
                lw.prev_left_down = left_down;
            }
        }

        let current_window_state = self.get_current_window_state();

        // Filter events via the configurable input interpreter callback.
        // Default: standard desktop keybindings (arrows, Ctrl+C/V, etc.)
        // Can be replaced on LayoutWindow for vim, game controls, etc.
        let pre_filter = if let Some(layout_window) = self.get_layout_window() {
            use azul_core::events::{InputInterpreterInfo, InputInterpreterState};
            let info = InputInterpreterInfo {
                events: &synthetic_events,
                hit_test: hit_test_for_dispatch.as_ref(),
                keyboard_state: &current_window_state.keyboard_state,
                mouse_state: &current_window_state.mouse_state,
                state: InputInterpreterState {
                    focused_node: layout_window.focus_manager.get_focused_node().copied(),
                    click_count: 1,
                    // Where the press that began this drag landed — latched
                    // above, and only when it was on an editable.
                    drag_start_position: layout_window.text_selection_drag_anchor,
                    has_selection: layout_window.text_edit_manager.multi_cursor.as_ref()
                        .map(|mc| mc.selections.iter().any(|s| matches!(&s.selection, azul_core::selection::Selection::Range(_))))
                        .unwrap_or(false),
                },
            };
            let interpreter = &layout_window.input_interpreter;
            let ctx = interpreter.ctx.as_ref()
                .map(|r| r.clone())
                .unwrap_or_else(|| {
                    azul_core::refany::RefAny::new(EmptyRefAnyData(0))
                });
            // SAFETY / no-store invariant:
            // `InputInterpreterCallbackType` is an `extern "C" fn`, which cannot be
            // generic over a lifetime, so its info pointer is typed
            // `*const InputInterpreterInfo<'static>`. The `info` we pass actually
            // borrows this stack frame (events, hit_test, keyboard/mouse state),
            // so the `'static` here is a deliberate lifetime *erasure*, not a real
            // promise that the data lives forever.
            //
            // The contract every interpreter callback must uphold (and the built-in
            // `default_input_interpreter_extern` does): the pointer and anything
            // reachable through it are valid ONLY for the duration of this
            // synchronous `(interpreter.cb)(...)` call. The callback must read it
            // immediately and must NOT store the pointer, nor any reference derived
            // from it, beyond the call. `info` is guaranteed live across the call
            // because it is dropped only at the end of this block, after the call
            // returns.
            let info_ptr = &info as *const InputInterpreterInfo as *const InputInterpreterInfo<'static>;
            (interpreter.cb)(ctx, info_ptr)
        } else {
            PreCallbackFilterResult {
                system_changes: Vec::new(),
                user_events: synthetic_events.clone(),
            }
        };

        // Track overall processing result
        let mut result = ProcessEventResult::DoNothing;

        // NOTE: VirtualView re-invocation for scroll edge detection is handled
        // transparently in the ScrollTo processing path (apply_user_change).

        // Get external callbacks for system time
        let external = ExternalSystemCallbacks::rust_internal();

        // Process pre-callback system changes (text selection, shortcuts) via apply_system_change.
        // MWA-C-clipboard: Copy/Cut/Paste are DEFERRED until after callback
        // dispatch so the W3C clipboard events (On::Copy/Cut/Paste) fire
        // first and preventDefault can suppress the OS default action —
        // previously the OS clipboard was written/read before any user
        // callback ran and the clipboard events never fired at all.
        let mut deferred_clipboard: Vec<SystemChange> = Vec::new();
        for system_change in &pre_filter.system_changes {
            match system_change {
                SystemChange::CopyToClipboard
                | SystemChange::CutToClipboard { .. }
                | SystemChange::PasteFromClipboard => {
                    deferred_clipboard.push(system_change.clone());
                }
                _ => {
                    let r = self.apply_system_change(system_change);
                    result = result.max(r);
                }
            }
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

        // THE RULE FOR PER-PASS INPUT THAT CALLBACKS READ LIVE: clear it AFTER
        // dispatch, never during determination.
        //
        // The wheel delta for this pass has now been delivered to any Scroll
        // callback (read via CallbackInfo::get_scroll_delta during dispatch).
        // Clear it so the recursion below — and any later pass — doesn't re-fire
        // a stale Scroll event (which would zoom the map on every mouse move).
        //
        // The injected native gesture (macOS magnify/rotate per MWA-B4, or a
        // debug-server injection) is the same kind of thing: the detectors
        // above turned it into this pass's PinchIn/PinchOut event, and the
        // callback for that event reads the gesture itself through
        // `CallbackInfo::get_pinch()`. It used to be cleared with the other
        // manager flags BEFORE dispatch, so every pinch callback saw `None`
        // and a trackpad pinch over the map did nothing. Clearing it here
        // still stops an ended pinch from re-firing on every later pass
        // (iOS/Android clear per-frame in their own loops).
        if let Some(w) = self.get_layout_window_mut() {
            w.scroll_manager.pending_wheel_event = None;
            w.gesture_drag_manager.clear_native_gesture();
        }

        // MWA-C-clipboard: fire the W3C clipboard events for the deferred
        // Copy/Cut/Paste shortcuts, then apply the OS default action unless
        // a callback preventDefault'ed. For Paste, the OS clipboard is read
        // into the manager FIRST so On::Paste callbacks can inspect it via
        // get_clipboard_content(); the pending content is cleared afterwards
        // (it used to persist forever, returning stale data).
        if !deferred_clipboard.is_empty() {
            let clip_target = old_focus.unwrap_or(azul_core::dom::DomNodeId {
                dom: DomId { inner: 0 },
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                    azul_core::id::NodeId::ZERO,
                )),
            });
            let has_paste = deferred_clipboard
                .iter()
                .any(|c| matches!(c, SystemChange::PasteFromClipboard));
            if has_paste {
                let pasted = get_system_clipboard()
                    .as_ref()
                    .and_then(payload_to_clipboard_content);
                if let Some(clipboard_content) = pasted {
                    if let Some(lw) = self.get_layout_window_mut() {
                        lw.clipboard_manager.set_paste_content(clipboard_content);
                    }
                }
            }
            let now_ts = {
                #[cfg(feature = "std")]
                {
                    azul_core::task::Instant::from(std::time::Instant::now())
                }
                #[cfg(not(feature = "std"))]
                {
                    azul_core::task::Instant::Tick(azul_core::task::SystemTick::new(0))
                }
            };
            let clip_events: Vec<azul_core::events::SyntheticEvent> = deferred_clipboard
                .iter()
                .map(|c| {
                    let et = match c {
                        SystemChange::CopyToClipboard => azul_core::events::EventType::Copy,
                        SystemChange::CutToClipboard { .. } => azul_core::events::EventType::Cut,
                        _ => azul_core::events::EventType::Paste,
                    };
                    azul_core::events::SyntheticEvent::new(
                        et,
                        azul_core::events::EventSource::User,
                        clip_target,
                        now_ts.clone(),
                        azul_core::events::EventData::None,
                    )
                })
                .collect();
            let (clip_result, _clip_update, clip_prevented) =
                self.dispatch_events_propagated(&clip_events);
            result = result.max(clip_result);
            if !clip_prevented {
                for change in &deferred_clipboard {
                    let r = self.apply_system_change(change);
                    result = result.max(r);
                }
            }
            if has_paste {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.clipboard_manager.clear_paste();
                }
            }
        }

        let mut should_recurse = false;

        use azul_core::callbacks::Update;
        match callback_update {
            Update::RefreshDom => {
                self.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
                should_recurse = true;
            }
            Update::RefreshDomAllWindows => {
                self.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
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
                                        let is_draggable = node_data.attributes().as_ref().iter().any(|attr| {
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

            // Record the drag SOURCE (the node under the PRESS point) so
            // DragStart/Drag/DragEnd stick to it for the whole gesture, W3C
            // style — resolved here, where the hit-tester is, and held in the
            // gesture manager (remapped across rebuilds). Without it the drag
            // followed the live hover and "stopped" when the cursor left the
            // dragged element.
            if let Some(source) = self.drag_source_node() {
                if let Some(lw) = self.get_layout_window_mut() {
                    lw.gesture_drag_manager.set_drag_source_node(source);
                }
            }
        }

        // SET :drag-over PSEUDO-STATE ON DragEnter / DragLeave TARGETS
        for event in &pre_filter.user_events {
            match event.event_type {
                azul_core::events::EventType::DragEnter => {
                    post_system_changes.push(SystemChange::SetDragOverState {
                        target: event.target, active: true,
                    });
                    post_system_changes.push(SystemChange::UpdateDropTarget {
                        target: event.target,
                    });
                }
                azul_core::events::EventType::DragLeave => {
                    post_system_changes.push(SystemChange::SetDragOverState {
                        target: event.target, active: false,
                    });
                }
                _ => {}
            }
        }

        // FORCE RE-RENDER DURING ACTIVE DRAG
        let is_node_dragging = self.get_layout_window()
            .map(|lw| lw.gesture_drag_manager.is_node_drag_active())
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

        // Post-filter via configurable callback (default: scroll-into-view, auto-scroll)
        let post_filter_changes = if let Some(layout_window) = self.get_layout_window() {
            let pf = &layout_window.post_filter;
            let slice = azul_core::events::SystemChangeVecSlice {
                ptr: pre_filter.system_changes.as_ptr(),
                len: pre_filter.system_changes.len(),
            };
            let old_dn = old_focus.unwrap_or(azul_core::dom::DomNodeId {
                dom: DomId { inner: 0 },
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(None),
            });
            let new_dn = new_focus.unwrap_or(azul_core::dom::DomNodeId {
                dom: DomId { inner: 0 },
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(None),
            });
            let ctx = pf.ctx.as_ref()
                .map(|r| r.clone())
                .unwrap_or_else(|| {
                    azul_core::refany::RefAny::new(EmptyRefAnyData(0))
                });
            let result_vec: azul_core::events::SystemChangeVec = (pf.cb)(ctx, prevent_default, slice, old_dn, new_dn);
            result_vec.into_library_owned_vec()
        } else {
            azul_core::events::post_callback_filter_system_changes(
                prevent_default, &pre_filter.system_changes, old_focus, new_focus,
            )
        };
        post_system_changes.extend(post_filter_changes);

        // Detect if focus changed (for focus event dispatch later)
        let focus_changed = post_system_changes.iter().any(|c| matches!(c, SystemChange::SetFocus { .. }));

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
        } else if prevent_default {
            // A vetoed edit must DIE, not wait: the pending record would
            // otherwise survive into the next pass, whose unconditional apply
            // would land the vetoed character late (e.g. on the next mouse
            // move). `clear_changeset`'s doc always promised this call.
            if let Some(lw) = self.get_layout_window_mut() {
                lw.text_input_manager.clear_changeset();
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
                    // Contenteditable awareness: Enter/Backspace/Delete at
                    // block boundaries become STRUCTURAL edit records instead
                    // of activation / plain text ops.
                    let editing_state = self
                        .get_layout_window()
                        .and_then(|lw| lw.build_editing_query_state(focused_node));
                    let default_action_result = azul_layout::default_actions::determine_keyboard_default_action_with_editing(
                        keyboard_state, focused_node, layout_results, prevent_default,
                        editing_state.as_ref(),
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
                                synthetic_click_target = Some(*target);
                            }

                            DefaultAction::InsertLineBreakAtCursor { target } => {
                                // Plain-text Enter / Shift+Enter: a literal
                                // "\n" through the standard text pipeline.
                                // The apply tail already ran this pass, so
                                // record + apply directly (same two system
                                // changes the tail uses). Runs only under
                                // !prevent_default, so the veto is honored;
                                // undo + caret-follow come from the changeset
                                // path itself.
                                if let Some(lw) = self.get_layout_window_mut() {
                                    if let Some(node_id) = target.node.into_crate_internal() {
                                        let old_inline = lw.get_text_before_textinput(target.dom, node_id);
                                        let old_text = lw.extract_text_from_inline_content(&old_inline);
                                        use azul_layout::managers::text_input::TextInputSource;
                                        lw.text_input_manager.record_input(
                                            *target,
                                            "\n".to_string(),
                                            old_text,
                                            TextInputSource::Keyboard,
                                        );
                                    }
                                }
                                let r = self.apply_system_change(&SystemChange::ApplyTextChangeset);
                                result = result.max(r);
                                let r = self.apply_system_change(
                                    &SystemChange::ScrollCursorIntoViewAfterTextInput,
                                );
                                result = result.max(r);
                                // Applied outside the record pipeline's event
                                // window — owe the host its Input dispatch.
                                if let Some(lw) = self.get_layout_window_mut() {
                                    lw.text_edit_manager
                                        .pending_edit_notifications
                                        .push(*target);
                                }
                            }

                            DefaultAction::SplitBlockAtCursor { .. }
                            | DefaultAction::MergeWithPrevious { .. }
                            | DefaultAction::MergeWithNext { .. } => {
                                // Structural edits: execution IS recording
                                // (azul never mutates the DOM). The app reads
                                // the changeset and applies it to its model.
                                // A materialized PREVIEW paints on the next
                                // relayout (O3-render), so charge one.
                                if let Some(lw) = self.get_layout_window_mut() {
                                    if lw
                                        .record_structural_default_action(
                                            &default_action_result.action,
                                        )
                                        .is_some()
                                    {
                                        result = result.max(
                                            ProcessEventResult::ShouldIncrementalRelayout,
                                        );
                                    }
                                }
                            }

                            DefaultAction::ScrollFocusedContainer { direction, amount } => {
                                use azul_core::events::{ScrollDirection, ScrollAmount};

                                if let Some(lw) = self.get_layout_window_mut() {
                                    // MWA-C-scroll: anchor on the focused node,
                                    // else the deepest hovered node — arrows /
                                    // PgUp/PgDn/Space over an unfocused scroll
                                    // container previously did nothing.
                                    let anchor = lw.focus_manager.focused_node.or_else(|| {
                                        let hit = lw.hover_manager.get_current(
                                            &azul_layout::managers::hover::InputPointId::Mouse,
                                        )?;
                                        hit.hovered_nodes.iter().next().and_then(|(dom_id, entry)| {
                                            entry.regular_hit_test_nodes.keys().next_back().map(|nid| {
                                                azul_core::dom::DomNodeId {
                                                    dom: *dom_id,
                                                    node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(*nid)),
                                                }
                                            })
                                        })
                                    });
                                    if let Some(focused) = anchor {
                                        if let Some(ancestor) = lw.find_scrollable_ancestor(focused) {
                                            if let Some(anc_node) = ancestor.node.into_crate_internal() {
                                                let anc_bounds = lw.get_node_bounds(ancestor.dom, anc_node);
                                                let vp_h = anc_bounds.map(|b| b.size.height as f32).unwrap_or(DEFAULT_VIEWPORT_HEIGHT);

                                                let magnitude = match amount {
                                                    ScrollAmount::Line => KEYBOARD_SCROLL_LINE_PX,
                                                    ScrollAmount::Page => vp_h * 0.9,
                                                    ScrollAmount::Document => KEYBOARD_SCROLL_DOCUMENT_MAX,
                                                };

                                                let (dx, dy) = match direction {
                                                    ScrollDirection::Up => (0.0, -magnitude),
                                                    ScrollDirection::Down => (0.0, magnitude),
                                                    ScrollDirection::Left => (-magnitude, 0.0),
                                                    ScrollDirection::Right => (magnitude, 0.0),
                                                };

                                                let now: azul_core::task::Instant = std::time::Instant::now().into();
                                                lw.scroll_manager.scroll_by(
                                                    ancestor.dom,
                                                    anc_node,
                                                    azul_core::geom::LogicalPosition { x: dx, y: dy },
                                                    std::time::Duration::from_millis(150).into(),
                                                    azul_core::events::EasingFunction::EaseOut,
                                                    now,
                                                );
                                                result = result.max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
                                            }
                                        }
                                    }
                                }
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

        // TEXT-EDIT NOTIFICATIONS: edits committed OUTSIDE the text-input
        // record pipeline this pass (deletions, multi-cursor paste, the Enter
        // line break) dispatch their Input event here, so widget mirrors
        // observe every committed edit, not only insertions.
        {
            let pending = self
                .get_layout_window_mut()
                .map(azul_layout::window::LayoutWindow::take_text_edit_notifications)
                .unwrap_or_default();
            if !pending.is_empty() {
                let now = azul_core::task::Instant::now();
                let edit_events: Vec<_> = pending
                    .into_iter()
                    .map(|host| {
                        azul_core::events::SyntheticEvent::new(
                            azul_core::events::EventType::Input,
                            azul_core::events::EventSource::User,
                            host,
                            now.clone(),
                            azul_core::events::EventData::None,
                        )
                    })
                    .collect();
                let (edit_result, edit_update, _) = self.dispatch_events_propagated(&edit_events);
                result = result.max(edit_result);
                if matches!(
                    edit_update,
                    azul_core::callbacks::Update::RefreshDom
                        | azul_core::callbacks::Update::RefreshDomAllWindows
                ) {
                    result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
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
                    self.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
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

        // Handle focus changes: generate synthetic FocusIn/FocusOut events.
        //
        // TRACE, not Debug: this fires on EVERY event pass — mouse-move
        // included — and almost always reports "nothing changed", so at Debug
        // it drowns out the lines that do say something.
        log_trace!(
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
                if let Some(ref mut mc) = layout_window.text_edit_manager.multi_cursor {
                    if let Some(cursor) = mc.get_primary_cursor() {
                        mc.set_single_cursor(cursor);
                    }
                }
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
                        self.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
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

        // MWA-A3c: fold the incremental :hover restyle outcome (computed
        // right after event determination above) into the pass result.
        if let Some(r) = hover_restyle_result {
            result = result.max(r);
        }

        // MWA-B8: fold the drag-auto-scroll timer start (if any).
        if let Some(r) = autoscroll_start_result {
            result = result.max(r);
        }

        // MWA-C-gesture: fold the Escape / focus-loss drag cancellation.
        if let Some(r) = drag_cancel_result {
            result = result.max(r);
        }
        result = result.max(accelerator_result);

        // End-of-pass housekeeping (top-level pass only):
        // - MWA-A1: re-sync the pump timer — callbacks above may have added /
        //   removed the DOM's first gamepad/sensor listener or a
        //   GeolocationProbe (flags refresh during regenerate_layout's walk).
        // - MWA-A3e: push any pending a11y tree update so INCREMENTAL updates
        //   (text edits / caret moves computed during this pass) reach the OS
        //   adapter now instead of waiting for the next full relayout.
        if depth == 0 {
            self.sync_capability_pump_timer();
            self.flush_a11y_tree_update();
        }

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

        // R2: every backend calls this from its frame loop at TOP level, never
        // from inside a pass — which makes it the one shared point where a
        // delta some platform handler left unconsumed is still observable
        // before the next handler's snapshot erases it.
        check_input_delta_consumed(
            self.get_previous_window_state().as_ref(),
            self.get_current_window_state(),
            "process_timers_and_threads",
        );

        let (timer_changes_result, timer_results) = self.invoke_expired_timers();
        let mut max_changes_result = timer_changes_result;
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
            max_changes_result = max_changes_result.max(thread_changes_result);
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

        // A CHANGE whose own result is a full DOM regeneration says exactly what
        // a timer that returned `Update::RefreshDom` says, and it must be read.
        //
        // `timer_results` / `thread_update` above are only the update the TIMER
        // (or thread writeback) callback itself returned. They are NOT the only
        // way a rebuild gets requested: `apply_user_change` runs a whole event
        // pass for `ModifyWindowState` / `QueueWindowStateSequence` /
        // `CreateTextInput`, and a USER callback dispatched inside that pass can
        // return `Update::RefreshDom` — which `process_window_events` reports by
        // returning `ShouldRegenerateDomCurrentWindow` (and by calling
        // `request_regeneration` itself).
        //
        // Leaving `needs_layout_regeneration` false for that case was not merely
        // a missed flag: it fell through to the `ShouldIncrementalRelayout` arm
        // below and DOWNGRADED the request to relayout-only. Every frame
        // path — X11, Wayland, Windows and headless — tests `relayout_only`
        // FIRST and clears both flags, so the requested DOM rebuild was not
        // delayed, it was discarded: `relayout_only` re-runs layout on the OLD
        // StyledDom and never re-invokes the layout callback, so the window kept
        // rendering the pre-callback data forever.
        if max_changes_result >= ProcessEventResult::ShouldRegenerateDomCurrentWindow {
            needs_layout_regeneration = true;
            needs_redraw = true;
        }

        // Mark frame for regeneration ONLY when a callback returned RefreshDom
        // (full DOM rebuild). ShouldUpdateDisplayListCurrentWindow means the
        // display list was already regenerated internally (e.g. by CreateTextInput)
        // — we just need a repaint, NOT a full layout() call which would rebuild
        // the DOM from stale application data.
        if needs_layout_regeneration {
            self.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        }

        // If changes produced a new display list (e.g. text edit), mark it dirty
        // so build_atomic_txn sends it to WebRender without a full DOM rebuild.
        if max_changes_result >= ProcessEventResult::ShouldUpdateDisplayListCurrentWindow {
            self.mark_display_list_dirty();
            needs_redraw = true;
        }

        // A change that mutated the EXISTING StyledDom in place (a debug-server DOM
        // mutation, a restyle, a runtime text edit) needs layout re-run ON THAT DOM
        // — NOT a DOM rebuild. Rebuilding is not merely wasteful here, it is WRONG:
        // `regenerate_layout` compares the incoming DOM against the stored one with
        // `is_layout_equivalent`, and after an in-place mutation those two are the
        // SAME object, so the check says "unchanged" and the whole layout pass is
        // skipped — the shaped text, the glyph runs and the geometry all stay at
        // their pre-mutation values. That is the stale screen: the DOM says "AFTER",
        // every frame keeps rendering "before".
        //
        // `!needs_layout_regeneration` is what keeps this arm strictly BELOW the
        // regeneration threshold: a `ShouldRegenerateDom*` result has already set
        // that flag just above, so a rebuild can never be downgraded to a
        // relayout of the DOM it was supposed to replace.
        //
        // This arm used to raise the relayout-only flag ALONE, and the flag
        // alone was invisible to the loops that decide whether a frame is owed:
        // X11's `poll_event` gate and wayland's `frame_done_callback` re-arm
        // both ask "is a regeneration or a redraw pending?" and never looked at
        // relayout-only. A timer callback that only restyled therefore sat
        // there until some unrelated event happened to redraw the window —
        // a caret that stops blinking, a hover style that lands a second late.
        // `request_relayout_only` now raises BOTH, and since every frame path
        // tests relayout-only first, the DOM is still not rebuilt.
        if max_changes_result >= ProcessEventResult::ShouldIncrementalRelayout
            && !needs_layout_regeneration
        {
            self.request_relayout_only();
            needs_redraw = true;
        }

        needs_redraw
    }

    /// Perform scrollbar hit-test at the given position.
    ///
    /// Returns `Some(ScrollbarHitId)` if a scrollbar was hit, `None` otherwise.
    ///
    /// Uses CPU-side ScrollManager geometry instead of WebRender's hit-tester.
    /// WebRender's hit-tester uses the spatial tree from the last display list build,
    /// which is NOT updated during lightweight transactions (skip_scene_builder).
    /// Since scrollbar thumb positions are GPU-animated via reference frame transforms,
    /// the WebRender hit areas become stale after scrolling. The CPU-side geometry
    /// (ScrollbarState) is always up-to-date because calculate_scrollbar_states()
    /// runs on every scroll update.
    fn perform_scrollbar_hit_test(
        &self,
        position: azul_core::geom::LogicalPosition,
    ) -> Option<azul_core::hit_test::ScrollbarHitId> {
        use azul_core::dom::ScrollbarOrientation;
        use azul_layout::managers::scroll_state::ScrollbarComponent;

        let layout_window = self.get_layout_window()?;
        let hit = layout_window.scroll_manager.hit_test_scrollbars(position)?;

        // Convert ScrollbarHit → ScrollbarHitId
        match (hit.orientation, hit.component) {
            (ScrollbarOrientation::Vertical, ScrollbarComponent::Thumb) => {
                Some(azul_core::hit_test::ScrollbarHitId::VerticalThumb(hit.dom_id, hit.node_id))
            }
            (ScrollbarOrientation::Vertical, _) => {
                Some(azul_core::hit_test::ScrollbarHitId::VerticalTrack(hit.dom_id, hit.node_id))
            }
            (ScrollbarOrientation::Horizontal, ScrollbarComponent::Thumb) => {
                Some(azul_core::hit_test::ScrollbarHitId::HorizontalThumb(hit.dom_id, hit.node_id))
            }
            (ScrollbarOrientation::Horizontal, _) => {
                Some(azul_core::hit_test::ScrollbarHitId::HorizontalTrack(hit.dom_id, hit.node_id))
            }
        }
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

    /// THE scrollbar press path: record the pointer state the scrollbar is
    /// about to consume, run [`Self::handle_scrollbar_click`], and swallow the
    /// delta.
    ///
    /// A thumb drag is routed around the event system by design — that is what
    /// the `discard_input_delta` is for — but the button is still PHYSICALLY
    /// DOWN and the cursor still moved. A handler that returns before writing
    /// `mouse_state` leaves `left_down == false` and `cursor_position` stale
    /// for the whole drag, so every reader of the live pointer state
    /// (`CallbackInfo`'s mouse state, `MouseState::matches`, a widget's own
    /// "am I being dragged" test) disagrees with the hardware for as long as
    /// the user holds the thumb. The headless backend — the one the E2E suite
    /// scripts against, so the one whose answer the tests encode — already
    /// wrote them; every desktop backend returned first. macOS and Win32 route
    /// through here now; X11 and Wayland still early-return and should adopt
    /// this too.
    ///
    /// `site` is the audit string for the sanctioned swallow, e.g.
    /// `"macos.handle_mouse_down.scrollbar_click"`.
    fn handle_scrollbar_press(
        &mut self,
        hit_id: azul_core::hit_test::ScrollbarHitId,
        position: azul_core::geom::LogicalPosition,
        button: azul_core::events::MouseButton,
        site: &str,
    ) -> ProcessEventResult {
        self.get_common_mut().update_unsynced_state(|ws| {
            apply_pointer_button_state(&mut ws.mouse_state, position, button, true);
        });
        let result = self.handle_scrollbar_click(hit_id, position);
        self.discard_input_delta(site);
        result
    }

    /// The other half of [`Self::handle_scrollbar_press`]: end an active
    /// scrollbar drag and record the release that ended it.
    ///
    /// `None` means there was no drag and the caller must run its normal
    /// button-up path. Clearing the button here is not optional — the press
    /// set it, and a release that skipped the write would leave the button
    /// latched DOWN forever after the first thumb drag.
    fn end_scrollbar_drag(
        &mut self,
        position: azul_core::geom::LogicalPosition,
        button: azul_core::events::MouseButton,
        site: &str,
    ) -> Option<ProcessEventResult> {
        if self.get_scrollbar_drag_state().is_none() {
            return None;
        }
        *self.get_scrollbar_drag_state_mut() = None;
        self.get_common_mut().update_unsynced_state(|ws| {
            apply_pointer_button_state(&mut ws.mouse_state, position, button, false);
        });
        self.discard_input_delta(site);
        Some(ProcessEventResult::ShouldReRenderCurrentWindow)
    }

    /// Handle a click on the non-thumb part of a scrollbar: arrow buttons
    /// line-scroll, track clicks follow the OS preference (jump-to-position
    /// or page-up/down).
    fn handle_track_click(
        &mut self,
        dom_id: DomId,
        node_id: CoreNodeId,
        click_position: azul_core::geom::LogicalPosition,
        is_vertical: bool,
    ) -> ProcessEventResult {
        use azul_core::dom::ScrollbarOrientation;

        // MWA-C-scroll: SystemStyle.scrollbar_preferences.track_click was
        // computed on every platform but never consumed — every track click
        // hard-jumped to position regardless of the OS setting.
        let track_click_pref = self.get_system_style().scrollbar_preferences.track_click;

        // Get scrollbar state to calculate target position
        let layout_window = match self.get_layout_window() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        // MWA-C-scroll: ScrollbarHitId has no button variants, so arrow
        // buttons arrive here folded into *Track — re-run the component
        // hit-test to tell them apart (they used to jump-scroll like track).
        let component = layout_window
            .scroll_manager
            .hit_test_scrollbars(click_position)
            .map(|h| h.component);

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

        // Get current scroll state. `get_scroll_node_info` rather than
        // `get_scroll_state` because its `max_scroll_x`/`max_scroll_y` are THE
        // definition of "how far can this thing scroll" — they prefer
        // `virtual_scroll_size` over `content_rect`, and on a `VirtualView` the
        // content rect holds the VIEWPORT size (`invoke_virtual_view_callback_impl`
        // overwrites it), so deriving the extent from `content_rect` here made
        // `max_scroll` zero and `JumpToPosition` a silent no-op on every
        // virtualized list. Same accessor `auto_scroll_timer_callback` already uses.
        let scroll_state = match layout_window
            .scroll_manager
            .get_scroll_node_info(dom_id, node_id)
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

        let max_scroll = if is_vertical {
            scroll_state.max_scroll_y
        } else {
            scroll_state.max_scroll_x
        };
        let target_scroll = click_ratio * max_scroll;

        // Calculate delta from current position
        let current_scroll = if is_vertical {
            scroll_state.current_offset.y
        } else {
            scroll_state.current_offset.x
        };

        let scroll_delta = {
            use azul_css::system::ScrollbarTrackClick;
            use azul_layout::managers::scroll_state::ScrollbarComponent;
            match component {
                // Arrow buttons: one line per click, toward the arrow.
                Some(ScrollbarComponent::TopButton) => -KEYBOARD_SCROLL_LINE_PX,
                Some(ScrollbarComponent::BottomButton) => KEYBOARD_SCROLL_LINE_PX,
                _ => match track_click_pref {
                    ScrollbarTrackClick::JumpToPosition => target_scroll - current_scroll,
                    ScrollbarTrackClick::PageUpDown => {
                        // Page toward the click: before the thumb pages
                        // back, past it pages forward (Windows default).
                        let page = container_size * 0.9;
                        let thumb_center = scrollbar_state.thumb_position_ratio
                            + scrollbar_state.thumb_size_ratio * 0.5;
                        if click_ratio < thumb_center {
                            -page
                        } else {
                            page
                        }
                    }
                },
            }
        };

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
        let frame_start: azul_core::task::Instant = current_time.clone();

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

        // MWA-A1/B12: the capability-pump and long-press timers' callbacks
        // are inert markers — when either expires, the real work is a full
        // event pass (pump drains channels; detect_long_press finally gets
        // evaluated for a motionless hold). Detect here; the pass fires
        // after the normal timer loop below.
        let capability_pump_fired = expired_timer_ids.iter().any(|t| {
            *t == azul_core::task::CAPABILITY_PUMP_TIMER_ID
                || *t == azul_core::task::LONG_PRESS_TIMER_ID
        });

        // MWA-B8b: a drag-autoscroll frame moves the VIEW; the selection
        // endpoint has to travel with it, which is what every native editor
        // does. Extension used to ride on `MouseOver` alone — which needs
        // pointer MOTION *and* a hovered hit node — so holding the pointer
        // still past the window edge scrolled the container with the
        // selection frozen at wherever it was when the pointer stopped.
        // It happens here, not in `auto_scroll_timer_callback`, because a
        // timer callback only has an immutable `CallbackInfo` and there is no
        // `CallbackChange` that carries a drag extension.
        let drag_autoscroll_fired = expired_timer_ids
            .iter()
            .any(|t| *t == azul_core::task::DRAG_AUTOSCROLL_TIMER_ID);

        let mut all_results = Vec::new();
        let mut changes_result = ProcessEventResult::DoNothing;

        // Process each expired timer
        for timer_id in expired_timer_ids {
            // Prepare borrows fresh for each timer invocation
            let borrows = self.prepare_callback_invocation();

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
                self.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
            }

            all_results.push(update);
        }

        if drag_autoscroll_fired {
            // The pointer is the drag focus wherever it is — OutOfWindow
            // coordinates are valid input here for the same reason the timer
            // itself accepts them (dragging past the edge is the whole point).
            let pointer = match &self.get_current_window_state().mouse_state.cursor_position {
                azul_core::window::CursorPosition::InWindow(p)
                | azul_core::window::CursorPosition::OutOfWindow(p) => Some(*p),
                azul_core::window::CursorPosition::Uninitialized => None,
            };
            let held = self.get_current_window_state().mouse_state.left_down;
            // A node drag suppresses text selection — the same gate
            // SystemChange::TextSelectionDrag applies.
            let node_dragging = self
                .get_layout_window()
                .is_some_and(|lw| lw.gesture_drag_manager.is_node_drag_active());
            if held && !node_dragging {
                if let (Some(pointer), Some(lw)) = (pointer, self.get_layout_window_mut()) {
                    // The anchor lives on the multi-cursor state, so the start
                    // argument is unused; a missing editing session makes this
                    // a no-op, which is what a node/file drag wants.
                    if lw.process_mouse_drag_for_selection(pointer, pointer).is_some() {
                        changes_result = changes_result
                            .max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
                    }
                }
            }
        }

        if capability_pump_fired {
            let r = self.process_window_events(0);
            changes_result = changes_result.max(r);
        }

        (changes_result, all_results)
    }

    /// MWA-A1: arm / disarm / retune the recurring capability-pump timer
    /// (`CAPABILITY_PUMP_TIMER_ID`) to match the current subscription set —
    /// gamepad/sensor listener flags from the last relayout walk plus an
    /// active geolocation subscription. Single-threaded by design: this
    /// timer is the pump's ONLY wake mechanism (no thread exists), so the
    /// identical code path runs on WASM. With no subscriptions the timer is
    /// removed entirely and an idle app burns zero CPU.
    fn sync_capability_pump_timer(&mut self) {
        use azul_core::task::CAPABILITY_PUMP_TIMER_ID;

        let desired_ms = self
            .get_layout_window()
            .and_then(super::capability_pump::desired_interval_ms);
        let armed_ms = self.get_layout_window().and_then(|lw| {
            lw.timers
                .get(&CAPABILITY_PUMP_TIMER_ID)
                .and_then(super::capability_pump::timer_interval_ms)
        });

        if desired_ms == armed_ms {
            return;
        }

        if armed_ms.is_some() {
            if let Some(lw) = self.get_layout_window_mut() {
                lw.timers.remove(&CAPABILITY_PUMP_TIMER_ID);
            }
            self.stop_timer(CAPABILITY_PUMP_TIMER_ID.id);
        }

        if let Some(ms) = desired_ms {
            let timer = super::capability_pump::make_pump_timer(ms);
            if let Some(lw) = self.get_layout_window_mut() {
                lw.add_timer(CAPABILITY_PUMP_TIMER_ID, timer.clone());
            }
            self.start_timer(CAPABILITY_PUMP_TIMER_ID.id, timer);
        }
    }

    /// MWA-A3e: push any pending a11y tree update to the platform adapter.
    ///
    /// Default is a no-op (headless / mobile shells). The four desktop
    /// backends override it with their adapter push. Called at the end of
    /// every top-level `process_window_events` pass so INCREMENTAL updates
    /// (text edits / caret moves — computed by `update_a11y_tree_incremental`
    /// during the pass and stored in `a11y_manager.last_tree_update`) reach
    /// assistive technology immediately; previously they sat there until the
    /// next full relayout happened to push the tree.
    fn flush_a11y_tree_update(&mut self) {}

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
            let layout_window = self.get_layout_window()?;
            !layout_window.threads.is_empty()
        };

        if !has_threads {
            return None;
        }

        // Get app_data from the platform window (shared across all windows)
        let app_data_arc = self.get_app_data().clone();

        // Prepare borrows for thread invocation
        let borrows = self.prepare_callback_invocation();

        // Call run_all_threads on the layout_window
        let mut app_data = app_data_arc.borrow_mut();
        let (changes, update) = borrows.layout_window.run_all_threads(
            &mut app_data,
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

        // `get_scroll_node_info`, not `get_scroll_state`: its `max_scroll_*`
        // already prefers `virtual_scroll_size` over `content_rect`. On a
        // `VirtualView` the content rect is the VIEWPORT
        // (`invoke_virtual_view_callback_impl` overwrites it), so computing the
        // extent from it gave max_scroll = 0 — the thumb was grabbable but
        // dragging it clamped every target to [0, 0] and moved nothing.
        let scroll_state = match layout_window
            .scroll_manager
            .get_scroll_node_info(dom_id, node_id)
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

        let max_scroll = if is_vertical {
            scroll_state.max_scroll_y
        } else {
            scroll_state.max_scroll_x
        };

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

#[cfg(test)]
mod tests {
    use azul_core::{
        geom::{LogicalSize, PhysicalPositionI32},
        icon::{IconProviderHandle, SharedIconProvider},
        resources::AppConfig,
        window::{CursorPosition, VirtualKeyCode, WindowFrame, WindowPosition, WindowTheme},
    };
    use azul_layout::window_state::WindowCreateOptions;

    use super::*;
    use crate::desktop::shell2::headless::HeadlessWindow;

    // R2: the unconsumed-input-delta guard

    /// Turn the validation gate on, or fail LOUDLY.
    ///
    /// [`validation_enabled`] is unconditionally true in a debug build; a
    /// release test binary reads `AZ_VALIDATE` through a `OnceLock`, so the
    /// variable has to be set before the first read anywhere in the process.
    /// If some earlier reader already latched it off, every test below would
    /// pass VACUOUSLY — the guard would simply return. Assert instead: a
    /// `#[should_panic]` test then fails on the wrong message, and the negative
    /// controls fail outright.
    fn require_validation_gate() {
        // A debug build validates unconditionally, so touching the environment
        // there is unnecessary (and racy in a threaded test binary).
        #[cfg(not(debug_assertions))]
        {
            std::env::set_var("AZ_VALIDATE", "1");
        }
        assert!(
            validation_enabled(),
            "the validation gate is OFF in this test binary, so \
             check_input_delta_consumed cannot fire and this suite proves nothing"
        );
    }

    fn state_pair() -> (FullWindowState, FullWindowState) {
        (FullWindowState::default(), FullWindowState::default())
    }

    #[test]
    #[should_panic(expected = "unconsumed input delta at test.cursor: previous_window_state.mouse_state")]
    fn check_input_delta_consumed_panics_on_an_unconsumed_cursor_move() {
        require_validation_gate();
        let (previous, mut current) = state_pair();
        current.mouse_state.cursor_position =
            CursorPosition::InWindow(LogicalPosition::new(12.0, 34.0));
        check_input_delta_consumed(Some(&previous), &current, "test.cursor");
    }

    #[test]
    #[should_panic(expected = "unconsumed input delta at test.resize: previous_window_state.size.dimensions")]
    fn check_input_delta_consumed_panics_on_an_unconsumed_resize() {
        require_validation_gate();
        let (previous, mut current) = state_pair();
        current.size.dimensions = LogicalSize::new(1234.0, 567.0);
        check_input_delta_consumed(Some(&previous), &current, "test.resize");
    }

    #[test]
    #[should_panic(expected = "unconsumed input delta at test.move: previous_window_state.position")]
    fn check_input_delta_consumed_panics_on_an_unconsumed_window_move() {
        require_validation_gate();
        let (previous, mut current) = state_pair();
        current.position = WindowPosition::Initialized(PhysicalPositionI32::new(400, 300));
        check_input_delta_consumed(Some(&previous), &current, "test.move");
    }

    #[test]
    #[should_panic(expected = "unconsumed input delta at test.dpi: previous_window_state.size.dpi")]
    fn check_input_delta_consumed_panics_on_an_unconsumed_dpi_change() {
        require_validation_gate();
        let (previous, mut current) = state_pair();
        current.size.dpi = 192;
        check_input_delta_consumed(Some(&previous), &current, "test.dpi");
    }

    #[test]
    #[should_panic(expected = "unconsumed input delta at test.theme: previous_window_state.theme")]
    fn check_input_delta_consumed_panics_on_an_unconsumed_theme_change() {
        require_validation_gate();
        let (previous, mut current) = state_pair();
        current.theme = match current.theme {
            WindowTheme::DarkMode => WindowTheme::LightMode,
            WindowTheme::LightMode => WindowTheme::DarkMode,
        };
        check_input_delta_consumed(Some(&previous), &current, "test.theme");
    }

    /// Negative control: without this the panicking tests above could pass for
    /// the wrong reason (any panic at all satisfies a substring that happens to
    /// match).
    #[test]
    fn check_input_delta_consumed_is_silent_on_a_consumed_delta() {
        require_validation_gate();
        let (mut previous, mut current) = state_pair();
        current.mouse_state.cursor_position =
            CursorPosition::InWindow(LogicalPosition::new(12.0, 34.0));
        current.size.dimensions = LogicalSize::new(1234.0, 567.0);
        // What the end of a pass does: advance the event baseline.
        previous.clone_from(&current);
        check_input_delta_consumed(Some(&previous), &current, "test.consumed");
    }

    /// The first frame has no baseline to consume.
    #[test]
    fn check_input_delta_consumed_is_silent_without_a_baseline() {
        require_validation_gate();
        let (_, mut current) = state_pair();
        current.size.dimensions = LogicalSize::new(1234.0, 567.0);
        check_input_delta_consumed(None, &current, "test.first-frame");
    }

    /// The allow-list is STRICT, not a deny-list over `PartialEq`: a field no
    /// event is derived from cannot encode a lost event, and every backend
    /// writes `ime_position` after the pass has already consumed the delta.
    /// A catch-all here panicked on the next poll as soon as the user typed.
    #[test]
    fn check_input_delta_consumed_ignores_fields_no_event_is_derived_from() {
        require_validation_gate();
        let (previous, mut current) = state_pair();
        current.title = "a different title".to_string().into();
        current.ime_position = azul_core::window::ImePosition::Initialized(
            azul_core::geom::LogicalRect::new(
                LogicalPosition::new(1.0, 2.0),
                LogicalSize::new(3.0, 4.0),
            ),
        );
        // NOT flags.frame any more: `EventType::WindowFrameChanged` is derived
        // from it now, so it belongs with the event-bearing fields. The rest of
        // `flags` is still pushed to the OS against a different baseline.
        current.flags.is_always_on_top = !previous.flags.is_always_on_top;
        check_input_delta_consumed(Some(&previous), &current, "test.not-event-bearing");
    }

    fn headless_stub() -> HeadlessWindow {
        HeadlessWindow::new(
            WindowCreateOptions::default(),
            Arc::new(RefCell::new(RefAny::new(()))),
            SharedUndoManager::new(),
            AppConfig::default(),
            SharedIconProvider::from_handle(IconProviderHandle::default()),
            Arc::new(FcFontCache::default()),
            None,
        )
        .unwrap()
    }

    /// The close protocol reports the flag the pass left standing. With no
    /// callback registered nothing vetoes, so a close confirms.
    #[test]
    fn a_close_request_with_no_callback_confirms() {
        let mut window = headless_stub();
        assert!(
            !window.get_current_window_state().flags.close_requested,
            "a fresh window is not already closing"
        );

        let outcome = window.request_window_close("test.close");

        assert!(outcome.confirmed, "nothing vetoed, so the close proceeds");
        assert!(
            window.get_current_window_state().flags.close_requested,
            "the flag stands for the backend to act on"
        );
    }

    /// A callback that clears the flag during the pass VETOES the close. This
    /// is the half X11 never had: it went straight to `is_open = false`, so
    /// there was nothing for a callback to refuse.
    #[test]
    fn clearing_the_flag_during_the_pass_vetoes_the_close() {
        let mut window = headless_stub();

        // Stand in for a callback that refuses: the protocol's verdict is read
        // from the state AFTER the pass, so whatever clears it wins.
        let outcome = window.request_window_close("test.close.vetoed");
        assert!(outcome.confirmed);

        window
            .common
            .update_window_state(WindowStateSource::App, |ws| {
                ws.flags.close_requested = false;
            });
        window.discard_input_delta("test.close.vetoed.cleanup");
        assert!(
            !window.get_current_window_state().flags.close_requested,
            "a cleared flag is what every backend reads as 'stay open'"
        );
    }

    /// The protocol consumes its own delta. Before it was shared, X11's
    /// size-to-content and Wayland's compositor-loss path both flipped this
    /// flag with no snapshot and no pass, leaving a delta live for whatever
    /// handler ran next to be blamed for.
    #[test]
    fn the_close_protocol_leaves_no_unconsumed_delta() {
        require_validation_gate();
        let mut window = headless_stub();

        let _ = window.request_window_close("test.close.consumed");

        // Would panic if the flip were still outstanding.
        window.snapshot_window_state_baseline("test.close.next-handler");
    }

    /// The guard is WIRED: `snapshot_window_state_baseline` — the call every
    /// platform handler opens with — is what trips on a handler that mutated
    /// and returned without a pass.
    #[test]
    #[should_panic(expected = "unconsumed input delta at test.next-handler")]
    fn snapshot_window_state_baseline_trips_on_a_delta_the_previous_handler_left_live() {
        require_validation_gate();
        let mut window = headless_stub();
        window.snapshot_window_state_baseline("test.seed");
        // A handler that mutates and returns without running a pass.
        window.common.update_unsynced_state(|ws| {
            ws.mouse_state.cursor_position =
                CursorPosition::InWindow(LogicalPosition::new(80.0, 90.0));
        });
        window.snapshot_window_state_baseline("test.next-handler");
    }

    /// The sanctioned escape hatch: a handler that consumed the input itself
    /// (scrollbar-thumb drag, a key eaten by an open popup) advances the
    /// baseline so the next snapshot does not read its mutation as a lost event.
    #[test]
    fn discard_input_delta_suppresses_the_guard_for_a_swallowed_input() {
        require_validation_gate();
        let mut window = headless_stub();
        window.snapshot_window_state_baseline("test.seed");
        window.common.update_unsynced_state(|ws| {
            ws.mouse_state.cursor_position =
                CursorPosition::InWindow(LogicalPosition::new(80.0, 90.0));
        });
        window.discard_input_delta("test.scrollbar-drag");
        window.snapshot_window_state_baseline("test.next-handler");
    }

    // The OS-sync / event-diff baseline split

    /// The WM_SIZE / windowDidResize contract, both halves at once: an
    /// OS-reported geometry change must LEAVE the event delta intact (or no
    /// `WindowResize` is ever dispatched — the Win32 and macOS resize findings)
    /// while leaving the OS-sync diff EMPTY (or `sync_window_state` echoes the
    /// size straight back and fights a live drag).
    #[test]
    fn os_reported_resize_keeps_the_event_delta_and_echoes_nothing() {
        let mut window = headless_stub();
        window.common.update_window_state(WindowStateSource::App, |ws| {
            ws.size.dimensions = LogicalSize::new(800.0, 600.0);
            ws.flags.frame = WindowFrame::Normal;
        });
        window.common.mark_os_synced();
        let before = window.common.current_window_state().clone();
        window.common.previous_window_state = Some(before.clone());

        window.common.update_window_state(WindowStateSource::Os, |ws| {
            ws.size.dimensions = LogicalSize::new(1234.0, 567.0);
            ws.flags.frame = WindowFrame::Maximized;
        });

        let baseline = window
            .common
            .previous_window_state
            .as_ref()
            .expect("event baseline was seeded above");
        assert_eq!(
            baseline.size.dimensions, before.size.dimensions,
            "update_window_state must never write the EVENT baseline — doing so \
             zeroes the previous->current diff and the resize reaches no callback"
        );
        assert_ne!(
            baseline.size.dimensions, window.common.current_window_state().size.dimensions,
            "the resize delta must still be there for the pass to dispatch"
        );

        let (synced, current) = window
            .common
            .take_os_sync_diff()
            .expect("the OS-sync baseline was seeded by mark_os_synced");
        assert_eq!(
            synced.size.dimensions, current.size.dimensions,
            "an OS-reported size must leave a ZERO sync diff, or sync_window_state \
             pushes the size we last saw back at the OS mid-drag"
        );
        assert_eq!(
            synced.flags.frame, current.flags.frame,
            "same for the frame flag: the Fullscreen sync arm is a TOGGLE, so a \
             stale OS baseline flaps the window in and out forever"
        );
    }

    /// The other direction: an APP-requested change is exactly what the sync is
    /// for, so it must still show up in the diff.
    #[test]
    fn app_requested_change_is_left_for_the_os_sync_to_push() {
        let mut window = headless_stub();
        window.common.update_window_state(WindowStateSource::App, |ws| {
            ws.flags.frame = WindowFrame::Normal;
        });
        window.common.mark_os_synced();
        window.common.update_window_state(WindowStateSource::App, |ws| {
            ws.flags.frame = WindowFrame::Fullscreen;
        });

        let (synced, current) = window
            .common
            .take_os_sync_diff()
            .expect("the OS-sync baseline was seeded by mark_os_synced");
        assert_ne!(
            synced.flags.frame, current.flags.frame,
            "an App-sourced change is not on the OS yet — the sync has to push it"
        );
        assert_eq!(current.flags.frame, WindowFrame::Fullscreen);
    }

    // Win32: UTF-16 text stream

    #[test]
    fn win32_utf16_stream_pairs_a_surrogate_pair_into_one_char() {
        let mut carry = None;
        // U+20BB7 (a supplementary-plane CJK ideograph) = D842 DFB7.
        assert_eq!(None, win32_utf16_stream_char(&mut carry, 0xD842));
        assert_eq!(Some(0xD842), carry);
        assert_eq!(Some('\u{20BB7}'), win32_utf16_stream_char(&mut carry, 0xDFB7));
        assert_eq!(None, carry, "the carry must be cleared after the pair");

        // U+1F600 GRINNING FACE = D83D DE00 (an IME emoji commit).
        assert_eq!(None, win32_utf16_stream_char(&mut carry, 0xD83D));
        assert_eq!(Some('\u{1F600}'), win32_utf16_stream_char(&mut carry, 0xDE00));
    }

    #[test]
    fn win32_utf16_stream_passes_bmp_text_and_drops_controls() {
        let mut carry = None;
        assert_eq!(Some('a'), win32_utf16_stream_char(&mut carry, u32::from('a')));
        assert_eq!(Some('\u{3042}'), win32_utf16_stream_char(&mut carry, 0x3042));
        // Backspace / Return / Escape arrive as WM_CHAR too and are not text.
        assert_eq!(None, win32_utf16_stream_char(&mut carry, 0x08));
        assert_eq!(None, win32_utf16_stream_char(&mut carry, 0x0D));
        assert_eq!(None, win32_utf16_stream_char(&mut carry, 0x1B));
    }

    #[test]
    fn win32_utf16_stream_rejects_orphaned_surrogates() {
        let mut carry = None;
        // A low half with no high half is not a character.
        assert_eq!(None, win32_utf16_stream_char(&mut carry, 0xDFB7));
        assert_eq!(None, carry);
        // A high half followed by ordinary text: the orphan is dropped, the
        // text still arrives.
        assert_eq!(None, win32_utf16_stream_char(&mut carry, 0xD842));
        assert_eq!(Some('x'), win32_utf16_stream_char(&mut carry, u32::from('x')));
        assert_eq!(None, carry);
    }

    // Win32: wheel notch distance

    #[test]
    fn win32_wheel_pixels_per_notch_tracks_the_user_setting() {
        // The Windows default of 3 lines must stay bit-identical to the shared
        // per-line constant (and therefore to X11 and macOS).
        assert_eq!(
            WHEEL_SCROLL_PIXELS_PER_LINE,
            win32_wheel_pixels_per_notch(3, 800.0)
        );
        // Doubling the Control Panel setting doubles the scroll distance —
        // the setting was captured and consumed by nobody before.
        assert_eq!(
            WHEEL_SCROLL_PIXELS_PER_LINE * 2.0,
            win32_wheel_pixels_per_notch(6, 800.0)
        );
        // 0 lines is a legal setting: "wheel scrolling off".
        assert_eq!(0.0, win32_wheel_pixels_per_notch(0, 800.0));
        // WHEEL_PAGESCROLL (u32::MAX): "one screen at a time".
        assert_eq!(800.0, win32_wheel_pixels_per_notch(u32::MAX, 800.0));
    }

    // macOS: discrete wheel deltas are LINES, the engine takes PIXELS

    #[test]
    fn discrete_scroll_delta_is_scaled_to_pixels_and_precise_deltas_are_not() {
        // One wheel notch (~1 line) has to travel the same distance it does on
        // X11/Win32; passing the line count through raw made macOS wheels crawl.
        assert_eq!(
            f64::from(WHEEL_SCROLL_PIXELS_PER_LINE),
            discrete_scroll_delta_to_pixels(1.0, false)
        );
        assert_eq!(
            -f64::from(WHEEL_SCROLL_PIXELS_PER_LINE) * 3.0,
            discrete_scroll_delta_to_pixels(-3.0, false)
        );
        // A trackpad already reports pixels.
        assert_eq!(7.5, discrete_scroll_delta_to_pixels(7.5, true));
        assert_eq!(0.0, discrete_scroll_delta_to_pixels(0.0, false));
    }

    // macOS: the hardware keycode table
    //
    // Moved here from `macos/events.rs` so it RUNS: no CI job compiles that
    // module, so the tests that lived next to the table never executed.

    #[test]
    fn macos_keycode_conversion() {
        assert_eq!(Some(VirtualKeyCode::A), macos_keycode_to_virtual_key(0x00));
        assert_eq!(Some(VirtualKeyCode::Return), macos_keycode_to_virtual_key(0x24));
        assert_eq!(Some(VirtualKeyCode::Space), macos_keycode_to_virtual_key(0x31));
        assert_eq!(Some(VirtualKeyCode::LShift), macos_keycode_to_virtual_key(0x38));
        assert_eq!(Some(VirtualKeyCode::LControl), macos_keycode_to_virtual_key(0x3B));
        assert_eq!(Some(VirtualKeyCode::LAlt), macos_keycode_to_virtual_key(0x3A));
        assert_eq!(Some(VirtualKeyCode::LWin), macos_keycode_to_virtual_key(0x37));
        assert_eq!(None, macos_keycode_to_virtual_key(0xFF));
    }

    /// The navigation cluster emits Private-Use-Area characters that the
    /// text-input filter (correctly) refuses to insert, so a missing entry here
    /// means the key produces NO engine event whatsoever.
    #[test]
    fn macos_keycode_conversion_navigation() {
        assert_eq!(Some(VirtualKeyCode::Home), macos_keycode_to_virtual_key(0x73));
        assert_eq!(Some(VirtualKeyCode::End), macos_keycode_to_virtual_key(0x77));
        assert_eq!(Some(VirtualKeyCode::PageUp), macos_keycode_to_virtual_key(0x74));
        assert_eq!(Some(VirtualKeyCode::PageDown), macos_keycode_to_virtual_key(0x79));
        // ForwardDelete, NOT Backspace (0x33 = Back).
        assert_eq!(Some(VirtualKeyCode::Delete), macos_keycode_to_virtual_key(0x75));
        assert_eq!(Some(VirtualKeyCode::Back), macos_keycode_to_virtual_key(0x33));
        assert_eq!(Some(VirtualKeyCode::Left), macos_keycode_to_virtual_key(0x7B));
        assert_eq!(Some(VirtualKeyCode::Up), macos_keycode_to_virtual_key(0x7E));
    }

    /// macOS orders the function row by hardware position, not by number — the
    /// table is easy to transpose, so pin every entry.
    #[test]
    fn macos_keycode_conversion_function_row() {
        assert_eq!(Some(VirtualKeyCode::F1), macos_keycode_to_virtual_key(0x7A));
        assert_eq!(Some(VirtualKeyCode::F2), macos_keycode_to_virtual_key(0x78));
        assert_eq!(Some(VirtualKeyCode::F3), macos_keycode_to_virtual_key(0x63));
        assert_eq!(Some(VirtualKeyCode::F4), macos_keycode_to_virtual_key(0x76));
        assert_eq!(Some(VirtualKeyCode::F5), macos_keycode_to_virtual_key(0x60));
        assert_eq!(Some(VirtualKeyCode::F6), macos_keycode_to_virtual_key(0x61));
        assert_eq!(Some(VirtualKeyCode::F7), macos_keycode_to_virtual_key(0x62));
        assert_eq!(Some(VirtualKeyCode::F8), macos_keycode_to_virtual_key(0x64));
        assert_eq!(Some(VirtualKeyCode::F9), macos_keycode_to_virtual_key(0x65));
        assert_eq!(Some(VirtualKeyCode::F10), macos_keycode_to_virtual_key(0x6D));
        assert_eq!(Some(VirtualKeyCode::F11), macos_keycode_to_virtual_key(0x67));
        assert_eq!(Some(VirtualKeyCode::F12), macos_keycode_to_virtual_key(0x6F));
    }

    #[test]
    fn macos_keycode_conversion_keypad_and_right_modifiers() {
        assert_eq!(Some(VirtualKeyCode::Numpad0), macos_keycode_to_virtual_key(0x52));
        assert_eq!(Some(VirtualKeyCode::Numpad7), macos_keycode_to_virtual_key(0x59));
        assert_eq!(Some(VirtualKeyCode::Numpad8), macos_keycode_to_virtual_key(0x5B));
        assert_eq!(Some(VirtualKeyCode::Numpad9), macos_keycode_to_virtual_key(0x5C));
        assert_eq!(Some(VirtualKeyCode::NumpadEnter), macos_keycode_to_virtual_key(0x4C));
        assert_eq!(Some(VirtualKeyCode::NumpadDecimal), macos_keycode_to_virtual_key(0x41));
        assert_eq!(Some(VirtualKeyCode::NumpadAdd), macos_keycode_to_virtual_key(0x45));
        assert_eq!(Some(VirtualKeyCode::NumpadSubtract), macos_keycode_to_virtual_key(0x4E));
        assert_eq!(Some(VirtualKeyCode::NumpadMultiply), macos_keycode_to_virtual_key(0x43));
        assert_eq!(Some(VirtualKeyCode::NumpadDivide), macos_keycode_to_virtual_key(0x4B));

        // Right-hand modifiers must NOT collapse onto their left twins.
        assert_eq!(Some(VirtualKeyCode::RWin), macos_keycode_to_virtual_key(0x36));
        assert_eq!(Some(VirtualKeyCode::RShift), macos_keycode_to_virtual_key(0x3C));
        assert_eq!(Some(VirtualKeyCode::RAlt), macos_keycode_to_virtual_key(0x3D));
        assert_eq!(Some(VirtualKeyCode::RControl), macos_keycode_to_virtual_key(0x3E));
        assert_eq!(Some(VirtualKeyCode::Capital), macos_keycode_to_virtual_key(0x39));
        assert_eq!(Some(VirtualKeyCode::Apps), macos_keycode_to_virtual_key(0x6E));
    }

    // Win32: the VK_* table
    //
    // Moved here from `windows/win_event.rs` so it RUNS: that module is
    // `#[cfg(target_os = "windows")]`, no CI job compiles it on the host that
    // runs the test suite, and it had NO tests at all.

    #[test]
    fn win32_vkey_conversion() {
        let vk = |code: i32| win32_vkey_to_virtual_key(code, None);
        assert_eq!(Some(VirtualKeyCode::Back), vk(win32_vk::VK_BACK));
        assert_eq!(Some(VirtualKeyCode::Return), vk(win32_vk::VK_RETURN));
        assert_eq!(Some(VirtualKeyCode::Space), vk(win32_vk::VK_SPACE));
        assert_eq!(Some(VirtualKeyCode::Escape), vk(win32_vk::VK_ESCAPE));
        assert_eq!(Some(VirtualKeyCode::Key0), vk(0x30));
        assert_eq!(Some(VirtualKeyCode::Key9), vk(0x39));
        assert_eq!(Some(VirtualKeyCode::A), vk(0x41));
        assert_eq!(Some(VirtualKeyCode::Z), vk(0x5A));
        // Not a VK code any Windows version defines.
        assert_eq!(None, vk(0xFF));
    }

    /// MWA-A2: `WM_KEYDOWN` delivers the GENERIC modifier codes unless the
    /// caller runs `MapVirtualKey` on the scancode. Dropping those three arms
    /// meant `ctrl_down()` was NEVER true on Windows and every Ctrl shortcut
    /// was dead, while the side-specific arms kept the table looking complete.
    #[test]
    fn win32_vkey_conversion_generic_and_sided_modifiers() {
        let vk = |code: i32| win32_vkey_to_virtual_key(code, None);
        assert_eq!(Some(VirtualKeyCode::LShift), vk(win32_vk::VK_SHIFT));
        assert_eq!(Some(VirtualKeyCode::LControl), vk(win32_vk::VK_CONTROL));
        assert_eq!(Some(VirtualKeyCode::LAlt), vk(win32_vk::VK_MENU));

        assert_eq!(Some(VirtualKeyCode::LShift), vk(win32_vk::VK_LSHIFT));
        assert_eq!(Some(VirtualKeyCode::RShift), vk(win32_vk::VK_RSHIFT));
        assert_eq!(Some(VirtualKeyCode::LControl), vk(win32_vk::VK_LCONTROL));
        assert_eq!(Some(VirtualKeyCode::RControl), vk(win32_vk::VK_RCONTROL));
        assert_eq!(Some(VirtualKeyCode::LAlt), vk(win32_vk::VK_LMENU));
        assert_eq!(Some(VirtualKeyCode::RAlt), vk(win32_vk::VK_RMENU));
        assert_eq!(Some(VirtualKeyCode::LWin), vk(win32_vk::VK_LWIN));
        assert_eq!(Some(VirtualKeyCode::RWin), vk(win32_vk::VK_RWIN));
    }

    #[test]
    fn win32_vkey_conversion_navigation_keypad_and_function_row() {
        let vk = |code: i32| win32_vkey_to_virtual_key(code, None);
        // PRIOR/NEXT are PageUp/PageDown, which is the easy one to transpose.
        assert_eq!(Some(VirtualKeyCode::PageUp), vk(win32_vk::VK_PRIOR));
        assert_eq!(Some(VirtualKeyCode::PageDown), vk(win32_vk::VK_NEXT));
        assert_eq!(Some(VirtualKeyCode::Home), vk(win32_vk::VK_HOME));
        assert_eq!(Some(VirtualKeyCode::End), vk(win32_vk::VK_END));
        assert_eq!(Some(VirtualKeyCode::Insert), vk(win32_vk::VK_INSERT));
        assert_eq!(Some(VirtualKeyCode::Delete), vk(win32_vk::VK_DELETE));
        assert_eq!(Some(VirtualKeyCode::Left), vk(win32_vk::VK_LEFT));
        assert_eq!(Some(VirtualKeyCode::Up), vk(win32_vk::VK_UP));

        assert_eq!(Some(VirtualKeyCode::Numpad0), vk(win32_vk::VK_NUMPAD0));
        assert_eq!(Some(VirtualKeyCode::Numpad9), vk(win32_vk::VK_NUMPAD9));
        assert_eq!(Some(VirtualKeyCode::NumpadMultiply), vk(win32_vk::VK_MULTIPLY));
        assert_eq!(Some(VirtualKeyCode::NumpadAdd), vk(win32_vk::VK_ADD));
        assert_eq!(Some(VirtualKeyCode::NumpadSubtract), vk(win32_vk::VK_SUBTRACT));
        assert_eq!(Some(VirtualKeyCode::NumpadDecimal), vk(win32_vk::VK_DECIMAL));
        assert_eq!(Some(VirtualKeyCode::NumpadDivide), vk(win32_vk::VK_DIVIDE));

        // The function row is contiguous from VK_F1, all 24 of it.
        for n in 0..24 {
            assert!(
                win32_vkey_to_virtual_key(win32_vk::VK_F1 + n, None).is_some(),
                "VK_F{} has no VirtualKeyCode",
                n + 1
            );
        }
        assert_eq!(Some(VirtualKeyCode::F12), vk(win32_vk::VK_F12));
        assert_eq!(Some(VirtualKeyCode::F24), vk(win32_vk::VK_F24));
    }

    /// The seven `VK_OEM_1..VK_OEM_7` codes name a POSITION, not a character:
    /// only the active layout says which key sits there. `None` for the layout
    /// character therefore has to mean "no virtual key", not "some default US
    /// key" — otherwise a German `#` would report as `Slash`.
    #[test]
    fn win32_oem_keys_resolve_through_the_active_layout_and_not_by_position() {
        assert_eq!(
            Some(VirtualKeyCode::Semicolon),
            win32_vkey_to_virtual_key(win32_vk::VK_OEM_1, Some(';'))
        );
        assert_eq!(
            Some(VirtualKeyCode::Backslash),
            win32_vkey_to_virtual_key(win32_vk::VK_OEM_5, Some('\\'))
        );
        // Same POSITION, a layout that puts something else there.
        assert_eq!(None, win32_vkey_to_virtual_key(win32_vk::VK_OEM_1, Some('ü')));
        assert_eq!(None, win32_vkey_to_virtual_key(win32_vk::VK_OEM_1, None));

        // The four OEM codes that are layout-INDEPENDENT keep their meaning
        // whatever the layout character says.
        assert_eq!(
            Some(VirtualKeyCode::Equals),
            win32_vkey_to_virtual_key(win32_vk::VK_OEM_PLUS, Some('ü'))
        );
        assert_eq!(
            Some(VirtualKeyCode::Comma),
            win32_vkey_to_virtual_key(win32_vk::VK_OEM_COMMA, None)
        );
        assert_eq!(
            Some(VirtualKeyCode::Minus),
            win32_vkey_to_virtual_key(win32_vk::VK_OEM_MINUS, None)
        );
        assert_eq!(
            Some(VirtualKeyCode::Period),
            win32_vkey_to_virtual_key(win32_vk::VK_OEM_PERIOD, None)
        );
        // The 102nd key of an ISO keyboard has its own code.
        assert_eq!(
            Some(VirtualKeyCode::OEM102),
            win32_vkey_to_virtual_key(win32_vk::VK_OEM_102, None)
        );
    }

    // Pointer state a scrollbar consumes

    fn thumb_hit() -> azul_core::hit_test::ScrollbarHitId {
        azul_core::hit_test::ScrollbarHitId::VerticalThumb(DomId { inner: 0 }, CoreNodeId::ZERO)
    }

    /// The scrollbar is routed around the event system, but the BUTTON is still
    /// physically down. A press that skips the `mouse_state` write reports
    /// `left_down == false` for the whole drag, so every reader of the live
    /// pointer state disagrees with the hardware until the user lets go.
    #[test]
    fn a_scrollbar_press_records_the_button_and_swallows_the_delta() {
        require_validation_gate();
        let mut window = headless_stub();
        window.snapshot_window_state_baseline("test.seed");

        let at = LogicalPosition::new(310.0, 120.0);
        let _ = window.handle_scrollbar_press(
            thumb_hit(),
            at,
            azul_core::events::MouseButton::Left,
            "test.scrollbar.press",
        );

        assert!(
            window.get_current_window_state().mouse_state.left_down,
            "the thumb is being HELD: left_down must be true for the whole drag"
        );
        assert_eq!(
            window.get_current_window_state().mouse_state.cursor_position,
            CursorPosition::InWindow(at),
            "the press position must reach the live pointer state"
        );
        assert!(
            !window.get_current_window_state().mouse_state.right_down
                && !window.get_current_window_state().mouse_state.middle_down,
            "only the button that changed may be written"
        );
        // SANCTIONED SWALLOW: the write must NOT also surface as a MouseDown —
        // this snapshot panics if the delta was left live.
        window.snapshot_window_state_baseline("test.scrollbar.next-handler");
    }

    /// The other half: the release that ends the drag has to CLEAR what the
    /// press latched, or the button stays down forever after the first drag.
    #[test]
    fn ending_a_scrollbar_drag_releases_the_button_the_press_latched() {
        require_validation_gate();
        let mut window = headless_stub();
        window.snapshot_window_state_baseline("test.seed");

        let down_at = LogicalPosition::new(310.0, 120.0);
        let _ = window.handle_scrollbar_press(
            thumb_hit(),
            down_at,
            azul_core::events::MouseButton::Left,
            "test.scrollbar.press",
        );
        assert!(
            window.get_scrollbar_drag_state().is_some(),
            "a thumb press starts a drag"
        );

        let up_at = LogicalPosition::new(310.0, 200.0);
        let ended = window.end_scrollbar_drag(
            up_at,
            azul_core::events::MouseButton::Left,
            "test.scrollbar.release",
        );

        assert!(ended.is_some(), "an active drag must report that it ended");
        assert!(
            window.get_scrollbar_drag_state().is_none(),
            "the drag is over"
        );
        assert!(
            !window.get_current_window_state().mouse_state.left_down,
            "the release must clear the button the press set"
        );
        assert_eq!(
            window.get_current_window_state().mouse_state.cursor_position,
            CursorPosition::InWindow(up_at)
        );
        window.snapshot_window_state_baseline("test.scrollbar.next-handler");

        assert!(
            window
                .end_scrollbar_drag(
                    up_at,
                    azul_core::events::MouseButton::Left,
                    "test.scrollbar.none",
                )
                .is_none(),
            "with no drag active the caller must run its normal button-up path"
        );
    }

    /// Buttons 4/5 have no `MouseState` field, so a press may only move the
    /// cursor — but it must not silently clear the buttons that ARE held.
    #[test]
    fn an_extra_button_press_records_the_position_without_clearing_held_buttons() {
        let mut mouse = azul_core::window::MouseState::default();
        mouse.left_down = true;
        apply_pointer_button_state(
            &mut mouse,
            LogicalPosition::new(7.0, 9.0),
            azul_core::events::MouseButton::Other(3),
            true,
        );
        assert!(mouse.left_down, "a thumb button may not release the left one");
        assert_eq!(
            mouse.cursor_position,
            CursorPosition::InWindow(LogicalPosition::new(7.0, 9.0))
        );
    }

    // Win32 X-buttons (mouse 4/5)

    /// The X-button messages are the ONLY `WM_*BUTTON*` messages that carry the
    /// button in the HIGH word of wParam; the low word is the modifier-key set.
    /// Reading wParam whole names the wrong button the moment Shift is held.
    #[test]
    fn win32_xbutton_reads_the_high_word_not_the_whole_wparam() {
        use azul_core::events::MouseButton;

        const MK_SHIFT: usize = 0x0004;
        const MK_CONTROL: usize = 0x0008;

        assert_eq!(
            win32_xbutton_to_mouse_button(0x0001 << 16),
            Some(MouseButton::Other(3)),
            "XBUTTON1 is the thumb BACK button"
        );
        assert_eq!(
            win32_xbutton_to_mouse_button(0x0002 << 16),
            Some(MouseButton::Other(4)),
            "XBUTTON2 is the thumb FORWARD button"
        );
        // Modifiers ride in the low word and must not change the answer.
        assert_eq!(
            win32_xbutton_to_mouse_button((0x0001 << 16) | MK_SHIFT | MK_CONTROL),
            Some(MouseButton::Other(3))
        );
        assert_eq!(win32_xbutton_to_mouse_button(MK_SHIFT), None);
        assert_eq!(win32_xbutton_to_mouse_button(0x0003 << 16), None);
    }

    // Win32 key state

    /// The scancode names the PHYSICAL key and needs no translation table to be
    /// true. Gating its write on `vkey_to_winit_vkey` — what the Win32 backend
    /// did — dropped every key the table does not know.
    #[test]
    fn an_unmapped_win32_key_still_records_its_scancode() {
        let mut ks = azul_core::window::KeyboardState::default();

        apply_win32_key_state_change(&mut ks, None, 0x6A, true);
        assert!(
            ks.pressed_scancodes.as_ref().contains(&0x6A),
            "an unmapped key is still a key that is DOWN"
        );
        assert!(
            ks.pressed_virtual_keycodes.as_ref().is_empty(),
            "there is no virtual key to record"
        );

        apply_win32_key_state_change(&mut ks, None, 0x6A, false);
        assert!(
            ks.pressed_scancodes.as_ref().is_empty(),
            "and the release must be able to take it back out again"
        );
    }

    /// Recording the physical key must stay ADDITIVE. `KeyUp` is derived from
    /// `previous.current_virtual_keycode.is_some() && current.is_none()`, so an
    /// unmapped key that cleared that field would fire a release for a key the
    /// user is still holding down.
    #[test]
    fn an_unmapped_win32_key_does_not_release_the_key_still_held() {
        let mut ks = azul_core::window::KeyboardState::default();
        apply_win32_key_state_change(&mut ks, Some(VirtualKeyCode::A), 0x1E, true);

        // A media key pressed and released while A is still down.
        apply_win32_key_state_change(&mut ks, None, 0x6A, true);
        assert_eq!(
            ks.current_virtual_keycode,
            azul_core::window::OptionVirtualKeyCode::Some(VirtualKeyCode::A),
            "an unmapped PRESS must not synthesize a KeyUp for the held key"
        );
        apply_win32_key_state_change(&mut ks, None, 0x6A, false);
        assert_eq!(
            ks.current_virtual_keycode,
            azul_core::window::OptionVirtualKeyCode::Some(VirtualKeyCode::A),
            "nor must its RELEASE"
        );
        assert!(
            ks.pressed_virtual_keycodes.as_ref().contains(&VirtualKeyCode::A),
            "A is still held throughout"
        );
        assert!(
            ks.pressed_scancodes.as_ref().contains(&0x1E)
                && !ks.pressed_scancodes.as_ref().contains(&0x6A),
            "the physical set tracks both keys correctly"
        );
    }

    /// The mapped case still has to work, and the release must remove both.
    #[test]
    fn a_mapped_win32_key_records_both_the_scancode_and_the_virtual_key() {
        let mut ks = azul_core::window::KeyboardState::default();

        apply_win32_key_state_change(&mut ks, Some(VirtualKeyCode::A), 0x1E, true);
        assert!(ks.pressed_scancodes.as_ref().contains(&0x1E));
        assert!(ks.pressed_virtual_keycodes.as_ref().contains(&VirtualKeyCode::A));
        assert_eq!(
            ks.current_virtual_keycode,
            azul_core::window::OptionVirtualKeyCode::Some(VirtualKeyCode::A)
        );

        apply_win32_key_state_change(&mut ks, Some(VirtualKeyCode::A), 0x1E, false);
        assert!(ks.pressed_scancodes.as_ref().is_empty());
        assert!(ks.pressed_virtual_keycodes.as_ref().is_empty());
        assert_eq!(
            ks.current_virtual_keycode,
            azul_core::window::OptionVirtualKeyCode::None
        );
    }

    // The IME re-sync key

    /// Focus is part of the key, so losing it re-syncs. A backend that left it
    /// out kept the IME engaged for an unfocused window.
    #[test]
    fn losing_the_focus_changes_the_ime_sync_key() {
        let window = headless_stub();
        let focused = ime_sync_key(true, window.get_layout_window());
        let blurred = ime_sync_key(false, window.get_layout_window());
        assert_ne!(focused, blurred);
        assert_eq!(focused, ime_sync_key(true, window.get_layout_window()));
        assert_eq!(
            focused.editing_node, None,
            "nothing is being edited in a freshly built window"
        );
    }

    // What a window-frame transition actually does

    /// `WM_SIZE` and `set_window_frame_and_dispatch` both used to justify their
    /// event pass by claiming the app's Maximize / Restore (resp. window-frame)
    /// callbacks now fire. There is no such event: `flags.frame` is DELIBERATELY
    /// excluded from the event-bearing set, so the pass feeds
    /// `current_window_state` and the OS-sync baseline and dispatches nothing
    /// for the transition itself. Adding the field here would make every
    /// maximize trip the unconsumed-delta guard instead.
    #[test]
    fn a_window_frame_transition_is_event_bearing() {
        // It was NOT, and this test asserted so. `EventType::WindowFrameChanged`
        // is now derived from `flags.frame`, so an unconsumed change here IS a
        // lost event and the guard has to say so.
        let (previous, mut current) = state_pair();
        current.flags.frame = WindowFrame::Fullscreen;
        assert_eq!(
            first_differing_state_field(&previous, &current),
            Some("flags.frame")
        );

        let (previous, mut current) = state_pair();
        current.flags.close_requested = true;
        assert_eq!(
            first_differing_state_field(&previous, &current),
            Some("flags.close_requested")
        );

        // Negative control: the rest of `flags` is still skipped. These are
        // pushed to the OS by sync_window_state against a different baseline,
        // and no event is derived from them.
        let (previous, mut current) = state_pair();
        current.flags.is_always_on_top = !previous.flags.is_always_on_top;
        assert_eq!(
            first_differing_state_field(&previous, &current),
            None,
            "flags is not compared wholesale"
        );
    }

    // Source-text invariants for the two backends nothing here can compile
    //
    // `macos/` and `windows/` sit behind `#[cfg(target_os = ...)]`, so a test
    // that needs a real window can only ever run on that OS's CI job. These
    // pin the two shapes whose ABSENCE is the defect and which have no pure
    // core left to hoist.

    const MACOS_MOD_RS: &str = include_str!("../macos/mod.rs");
    const RUN_RS: &str = include_str!("../run.rs");
    const WINDOWS_MOD_RS: &str = include_str!("../windows/mod.rs");

    /// The body of `source`'s first `fn name`, up to the next method in the
    /// same impl block (4-space indent, this file's style).
    fn fn_body<'a>(source: &'a str, name: &str) -> &'a str {
        let start = source
            .find(name)
            .unwrap_or_else(|| panic!("{name} not found — was it renamed?"));
        let rest = &source[start..];
        match rest[1..].find("\n    fn ") {
            Some(end) => &rest[..=end],
            None => rest,
        }
    }

    /// THE aliasing invariant of the macOS backend.
    ///
    /// `popUpMenuPositioningItem:atLocation:inView:` spins a SYNCHRONOUS nested
    /// tracking run loop. The tick timers are registered in
    /// `kCFRunLoopCommonModes` (deliberately — blink and tweens have to survive
    /// menu tracking and live resize) and `tickTimers:` reconstructs its own
    /// `&mut MacOSWindow` from the registry's raw pointer, so a pop-up sent
    /// while such a borrow is live aliases `&mut` with itself on top of plain
    /// re-entrancy. The only sound shape is park-then-present: the pending menu
    /// holds a RETAINED menu and view and borrows nothing, and the presenter
    /// runs after the borrow has ended. Keeping the pop-up to ONE call site is
    /// what makes that reviewable.
    /// The launch-time menu-bar stub (`setup_main_menu`) must be installed
    /// BEFORE the first window is created: the window's first layout installs
    /// the DOM's `menu_bar` as the main menu, and a stub installed afterwards
    /// overwrites it — which is how AzPaint lost its File / View menus for a
    /// whole session (`apply_menu_bar_from_dom` then saw an unchanged hash
    /// forever). `set_application_menu` is identity-aware now as well, but
    /// the order is the contract: the stub is the fallback, not the override.
    #[test]
    fn the_macos_menu_stub_is_installed_before_the_first_window() {
        let stub = RUN_RS
            .find("setup_main_menu(")
            .expect("run.rs installs the launch-time menu stub");
        let window = RUN_RS
            .find("MacOSWindow::new_with_fc_cache(")
            .expect("run.rs creates the root macOS window");
        assert!(
            stub < window,
            "setup_main_menu() must run before MacOSWindow::new_with_fc_cache():              a stub installed after the window overwrites the DOM's menu bar"
        );
        assert_eq!(
            RUN_RS.matches("setup_main_menu(").count(),
            1,
            "the stub is installed exactly once, before the window"
        );
    }

    #[test]
    fn the_macos_menu_runloop_is_entered_from_exactly_one_place() {
        let call_sites = MACOS_MOD_RS
            .matches("popUpMenuPositioningItem_atLocation_inView")
            .count();
        assert_eq!(
            call_sites, 1,
            "every synchronous menu pop-up must go through \
             present_pending_context_menu, which is only ever called with no \
             &mut MacOSWindow live"
        );

        let presenter = MACOS_MOD_RS
            .find("fn present_pending_context_menu")
            .expect("present_pending_context_menu is THE presenter");
        let struct_after = MACOS_MOD_RS
            .find("pub struct MacOSWindow")
            .expect("the presenter sits directly above the window struct");
        let call = MACOS_MOD_RS
            .find("popUpMenuPositioningItem_atLocation_inView")
            .expect("checked above");
        assert!(
            call > presenter && call < struct_after,
            "the one pop-up call must be inside present_pending_context_menu"
        );
    }

    /// `info.open_menu()` reaches `show_menu_from_callback` from INSIDE a pass,
    /// i.e. with the handler's `&mut MacOSWindow` live. It must park the menu
    /// and let a later run-loop turn present it — the same conversion the
    /// right-click path already got.
    #[test]
    fn a_callback_opened_native_menu_is_parked_not_presented() {
        let body = fn_body(MACOS_MOD_RS, "fn show_menu_from_callback");
        assert!(
            body.contains("queue_native_context_menu_at_position"),
            "the native branch must QUEUE the menu"
        );
        assert!(
            body.contains("schedule_pending_menu_presentation"),
            "and hand the presentation to a later run-loop turn"
        );
        assert!(
            !body.contains("popUpMenuPositioningItem"),
            "it must never pop the menu up while &mut self is held"
        );
    }

    /// Win32 messages the window procedure used to drop on the floor. There is
    /// no pure core to hoist here — the arms ARE the fix — so pin the wiring.
    #[test]
    fn the_win32_wndproc_handles_the_messages_it_used_to_drop() {
        for (name, value) in [
            ("WM_XBUTTONDOWN", "0x020B"),
            ("WM_XBUTTONUP", "0x020C"),
            ("WM_ENTERSIZEMOVE", "0x0231"),
            ("WM_EXITSIZEMOVE", "0x0232"),
        ] {
            assert!(
                WINDOWS_MOD_RS.contains(&format!("const {name}: u32 = {value};")),
                "{name} ({value}) is not even declared"
            );
            assert!(
                WINDOWS_MOD_RS.contains(&format!("{name} =>"))
                    || WINDOWS_MOD_RS.contains(&format!("{name} |")),
                "{name} is declared but the window procedure has no arm for it"
            );
        }
    }

    /// The modal size/move pump must not reuse the thread-poll timer's id:
    /// `SetTimer` with an id already in use REPLACES that timer, so a collision
    /// would silently kill background-thread polling for the rest of the run,
    /// and `KillTimer` at `WM_EXITSIZEMOVE` would never bring it back.
    #[test]
    fn the_win32_modal_pump_timer_has_its_own_id() {
        fn timer_id(name: &str) -> &'static str {
            WINDOWS_MOD_RS
                .lines()
                .find_map(|line| {
                    let line = line.trim();
                    let line = line.strip_prefix("pub(crate) ").unwrap_or(line);
                    let value = line.strip_prefix("const ")?.strip_prefix(name)?;
                    value.split('=').nth(1).map(|v| v.trim().trim_end_matches(';'))
                })
                .unwrap_or_else(|| panic!("{name} is not declared in windows/mod.rs"))
        }

        assert_ne!(
            timer_id("MODAL_LOOP_TIMER_ID"),
            timer_id("THREAD_POLL_TIMER_ID"),
            "the modal pump and the thread poll cannot share a Win32 timer id"
        );
    }
}
