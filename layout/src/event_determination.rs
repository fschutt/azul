//! Unified Event Determination
//!
//! This module provides the single source of truth for what
//! events occurred in a frame. It combines window state changes
//! with events from all managers (scroll, text input, etc.).

use azul_core::{
    dom::{DomId, DomNodeId},
    events::{
        deduplicate_synthetic_events, EventData, EventProvider, EventSource, EventType,
        KeyModifiers, KeyboardEventData, MouseButton, MouseEventData, ScrollDeltaMode,
        ScrollEventData, SyntheticEvent, WindowEventData,
    },
    geom::{LogicalPosition, LogicalRect},
    id::NodeId,
    styled_dom::NodeHierarchyItemId,
    task::{Instant, SystemTick},
    window::{CursorPosition, WindowPosition},
};

use std::collections::BTreeSet;

use crate::window_state::FullWindowState;

/// Unified event determination from all sources.
///
/// This is the **single source of truth** for what events occurred in a frame.
///
/// It combines:
///
/// 1. Window state changes (resize, move, theme, etc.)
/// 2. Manager-reported events (scroll, text input, focus, hover)
/// 3. Deduplicates by node + event type
///
/// ## Architecture
///
/// ```text
/// Platform Input
///     ↓
/// Update Window State + Managers (record_sample, record_input, etc.)
///     ↓
/// determine_events_from_managers() ← Query all managers (immutable)
///     ↓
/// Vec<SyntheticEvent> (deduplicated)
///     ↓
/// dispatch_events() ← Route to callbacks
///     ↓
/// Invoke callbacks
/// ```
///
/// ## Arguments
///
/// All arguments are **immutable** references - no state is modified here.
/// State changes happen earlier via `record_sample()`, `record_input()`, etc.
///
/// - `current_state` - Current window state (after platform updates)
/// - `previous_state` - Window state from previous frame
/// - `managers` - All managers that can provide events
/// - `timestamp` - Current time for event timestamps
///
/// ## Returns
///
/// - Vector of `SyntheticEvents`, deduplicated and ready for dispatch.
// Instant is a ref-counted FFI clock handle threaded through the event loop by value.
#[allow(clippy::needless_pass_by_value)]
pub fn determine_events_from_managers(
    current_state: &FullWindowState,
    previous_state: &FullWindowState,
    managers: &[&dyn EventProvider],
    timestamp: Instant,
) -> Vec<SyntheticEvent> {
    let mut events = Vec::new();

    // 1. Detect window state changes (simple diffing)
    events.extend(detect_window_state_events(
        current_state,
        previous_state,
        timestamp.clone(),
    ));

    // 2. Query all managers for their pending events
    for manager in managers {
        events.extend(manager.get_pending_events(timestamp.clone()));
    }

    // 3. Deduplicate by (node, event_type)
    deduplicate_synthetic_events(events)
}

/// Detect window-level events by comparing states.
///
/// This is a simple sub-function that only handles window-level changes:
///
/// - Window resized
/// - Window moved
/// - Theme changed
/// - Mouse entered/left window
/// - Window focus changed
///
/// Node-level events (hover, focus, scroll, text) come from managers.
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/counter/fixed-point cast
fn detect_window_state_events(
    current: &FullWindowState,
    previous: &FullWindowState,
    timestamp: Instant,
) -> Vec<SyntheticEvent> {
    let mut events = Vec::new();

    // Window resized
    if current.size != previous.size {
        events.push(SyntheticEvent::new(
            EventType::WindowResize,
            EventSource::User,
            DomNodeId {
                dom: DomId { inner: 0 },
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
            },
            timestamp.clone(),
            EventData::Window(WindowEventData {
                size: Some(LogicalRect {
                    origin: LogicalPosition { x: 0.0, y: 0.0 },
                    size: current.size.dimensions,
                }),
                position: None,
            }),
        ));
    }

    // Window moved
    if current.position != previous.position {
        let position = match current.position {
            WindowPosition::Initialized(phys_pos)
            | WindowPosition::RelativeToParentWindow(phys_pos) => Some(LogicalPosition {
                x: phys_pos.x as f32,
                y: phys_pos.y as f32,
            }),
            WindowPosition::Uninitialized => None,
        };

        if let Some(pos) = position {
            events.push(SyntheticEvent::new(
                EventType::WindowMove,
                EventSource::User,
                DomNodeId {
                    dom: DomId { inner: 0 },
                    node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
                },
                timestamp.clone(),
                EventData::Window(WindowEventData {
                    size: None,
                    position: Some(pos),
                }),
            ));
        }
    }

    // Theme changed
    if current.theme != previous.theme {
        events.push(SyntheticEvent::new(
            EventType::ThemeChange,
            EventSource::User,
            DomNodeId {
                dom: DomId { inner: 0 },
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
            },
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Mouse entered window
    let prev_mouse_in_window = matches!(
        previous.mouse_state.cursor_position,
        CursorPosition::InWindow(_)
    );
    let curr_mouse_in_window = matches!(
        current.mouse_state.cursor_position,
        CursorPosition::InWindow(_)
    );

    if curr_mouse_in_window && !prev_mouse_in_window {
        events.push(SyntheticEvent::new(
            EventType::MouseEnter,
            EventSource::User,
            DomNodeId {
                dom: DomId { inner: 0 },
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
            },
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Mouse left window
    if !curr_mouse_in_window && prev_mouse_in_window {
        events.push(SyntheticEvent::new(
            EventType::MouseLeave,
            EventSource::User,
            DomNodeId {
                dom: DomId { inner: 0 },
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
            },
            timestamp,
            EventData::None,
        ));
    }

    events
}

/// Get all hovered node IDs from the hover manager for a given frame.
///
/// `frame_index` 0 = current frame, 1 = previous frame, etc.
/// Returns a `BTreeSet` of all `NodeIds` that are hovered (the full hover chain).
fn get_all_hovered_nodes(
    hover_manager: &crate::managers::hover::HoverManager,
    frame_index: usize,
) -> BTreeSet<(DomId, NodeId)> {
    use crate::managers::hover::InputPointId;
    // MWA-C-hover: walk EVERY hit DOM, not just DomId 0 — the old
    // root-DOM-only read meant MouseEnter/MouseLeave never fired for nodes
    // inside VirtualView / iframe child DOMs.
    hover_manager
        .get_frame(&InputPointId::Mouse, frame_index)
        .map(|ht| {
            ht.hovered_nodes
                .iter()
                .flat_map(|(dom_id, hit)| {
                    hit.regular_hit_test_nodes
                        .keys()
                        .map(move |node_id| (*dom_id, *node_id))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Comprehensive event determination including mouse, keyboard, and gesture events.
///
/// This is the primary event determination function.
/// It generates `SyntheticEvents` for all event types including:
///
/// - Mouse button events (down/up for left/right/middle)
/// - Mouse movement (`MouseOver`)
/// - Keyboard events (VirtualKeyDown/Up)
/// - Window state changes (resize, move, theme, focus)
/// - Gesture events (`DragStart`, Drag, `DragEnd`, `DoubleClick`, `LongPress`, Swipe, Pinch, Rotate, Pen)
/// - File drop events (`HoveredFile`, `DroppedFile`)
///
/// ## Arguments
///
/// * `current_state` - Current window state
/// * `previous_state` - Previous window state
/// * `hover_manager` - For hover/mouse enter/leave detection
/// * `focus_manager` - For focus event detection
/// * `file_drop_manager` - For file drop detection
/// * `gesture_manager` - Optional gesture detection
/// * `managers` - Additional managers (scroll, text, etc.) that implement `EventProvider`
/// * `timestamp` - Current time
///
/// ## Returns
///
/// - Deduplicated vector of `SyntheticEvents` ready for dispatch
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/counter/fixed-point cast
// Instant is a ref-counted FFI clock handle threaded through the event loop by value.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
pub fn determine_all_events(
    current_state: &FullWindowState,
    previous_state: &FullWindowState,
    hover_manager: &crate::managers::hover::HoverManager,
    focus_manager: &crate::managers::focus_cursor::FocusManager,
    file_drop_manager: &crate::managers::file_drop::FileDropManager,
    gesture_manager: Option<&crate::managers::gesture::GestureAndDragManager>,
    managers: &[&dyn EventProvider],
    wheel_delta: Option<LogicalPosition>,
    timestamp: Instant,
) -> Vec<SyntheticEvent> {
    let mut events = Vec::new();

    // Get root node for window-level events
    let root_node = DomNodeId {
        dom: DomId { inner: 0 },
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
    };

    // Helper: get cursor position
    let cursor_pos = current_state
        .mouse_state
        .cursor_position
        .get_position()
        .unwrap_or(LogicalPosition { x: 0.0, y: 0.0 });

    // Helper: build key modifiers from keyboard state
    let modifiers = KeyModifiers {
        shift: current_state.keyboard_state.shift_down(),
        ctrl: current_state.keyboard_state.ctrl_down(),
        alt: current_state.keyboard_state.alt_down(),
        meta: current_state.keyboard_state.super_down(),
    };

    // Helper: compute mouse buttons bitmask
    let buttons: u8 = u8::from(current_state.mouse_state.left_down)
        | (if current_state.mouse_state.right_down { 2 } else { 0 })
        | (if current_state.mouse_state.middle_down { 4 } else { 0 });

    // Helper: get deepest hovered node as the event target for mouse events.
    // Multi-DOM aware: targets VirtualView / iframe child DOMs (higher DomId,
    // composited on top) when the cursor is over them, not just the root DOM.
    // For single-DOM apps only DomId 0 is ever hit, so this is unchanged.
    let mouse_target = hover_manager
        .current_hover_node_full()
        .unwrap_or(root_node);

    // Helper: build MouseEventData for a specific button
    let make_mouse_data = |button: MouseButton| -> EventData {
        EventData::Mouse(MouseEventData {
            position: cursor_pos,
            button,
            buttons,
            modifiers,
        })
    };

    // ========================================================================
    // Mouse Button Events (with proper EventData::Mouse and per-button types)
    // ========================================================================

    let current_mouse_down = current_state.mouse_state.mouse_down();
    let previous_mouse_down = previous_state.mouse_state.mouse_down();

    for (curr_down, prev_down, button) in [
        (
            current_state.mouse_state.left_down,
            previous_state.mouse_state.left_down,
            MouseButton::Left,
        ),
        (
            current_state.mouse_state.right_down,
            previous_state.mouse_state.right_down,
            MouseButton::Right,
        ),
        (
            current_state.mouse_state.middle_down,
            previous_state.mouse_state.middle_down,
            MouseButton::Middle,
        ),
    ] {
        if curr_down && !prev_down {
            events.push(SyntheticEvent::new(
                EventType::MouseDown,
                EventSource::User,
                mouse_target,
                timestamp.clone(),
                make_mouse_data(button),
            ));
        }
        if !curr_down && prev_down {
            events.push(SyntheticEvent::new(
                EventType::MouseUp,
                EventSource::User,
                mouse_target,
                timestamp.clone(),
                make_mouse_data(button),
            ));
        }
    }

    // ========================================================================
    // Click synthesis: if left mouse released on the same node as down
    // ========================================================================
    // Note: proper click synthesis requires tracking mousedown target across frames.
    // For now, if left mouse was released and the hover node hasn't changed, emit Click.
    if !current_state.mouse_state.left_down && previous_state.mouse_state.left_down {
        let prev_hover = hover_manager.previous_hover_node_full();
        let curr_hover = hover_manager.current_hover_node_full();
        if prev_hover == curr_hover && curr_hover.is_some() {
            events.push(SyntheticEvent::new(
                EventType::Click,
                EventSource::User,
                mouse_target,
                timestamp.clone(),
                make_mouse_data(MouseButton::Left),
            ));
        }
    }

    // ========================================================================
    // Mouse Movement Events
    // ========================================================================

    let current_in_window = matches!(
        current_state.mouse_state.cursor_position,
        CursorPosition::InWindow(_)
    );
    let previous_in_window = matches!(
        previous_state.mouse_state.cursor_position,
        CursorPosition::InWindow(_)
    );

    if current_in_window {
        let current_pos = current_state.mouse_state.cursor_position.get_position();
        let previous_pos = previous_state.mouse_state.cursor_position.get_position();

        // MouseOver fires on ANY mouse movement (targeted at hovered node)
        if current_pos != previous_pos {
            events.push(SyntheticEvent::new(
                EventType::MouseOver,
                EventSource::User,
                mouse_target,
                timestamp.clone(),
                make_mouse_data(MouseButton::Left), // MouseOver doesn't care about button
            ));
        }
    }

    // ========================================================================
    // Wheel / trackpad scroll  →  Scroll event on the hovered node
    // ========================================================================
    // The platform recorded a wheel delta this pass. The scroll-physics path
    // handles scroll *containers*; here we additionally fire a W3C-style Scroll
    // event at the hovered node so widgets that treat the wheel specially (the
    // map zooms) get a `HoverEventFilter::Scroll` callback. The delta itself is
    // read back by the callback via `CallbackInfo::get_scroll_delta`.
    if let Some(delta) = wheel_delta {
        if delta.x != 0.0 || delta.y != 0.0 {
            events.push(SyntheticEvent::new(
                EventType::Scroll,
                EventSource::User,
                mouse_target,
                timestamp.clone(),
                EventData::Scroll(ScrollEventData {
                    delta,
                    delta_mode: ScrollDeltaMode::Pixel,
                }),
            ));
        }
    }

    // ========================================================================
    // Per-Node MouseEnter/MouseLeave (W3C compliant)
    // ========================================================================
    // Compare FULL hover chains between current and previous frames.
    // Nodes that gained hover get MouseEnter, nodes that lost hover get MouseLeave.
    {
        let current_hovered = get_all_hovered_nodes(hover_manager, 0);
        let previous_hovered = get_all_hovered_nodes(hover_manager, 1);

        // Nodes that lost hover -> MouseLeave
        for (dom_id, node_id) in previous_hovered.difference(&current_hovered) {
            events.push(SyntheticEvent::new(
                EventType::MouseLeave,
                EventSource::User,
                DomNodeId {
                    dom: *dom_id,
                    node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                },
                timestamp.clone(),
                EventData::None,
            ));
        }

        // Nodes that gained hover -> MouseEnter
        for (dom_id, node_id) in current_hovered.difference(&previous_hovered) {
            events.push(SyntheticEvent::new(
                EventType::MouseEnter,
                EventSource::User,
                DomNodeId {
                    dom: *dom_id,
                    node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                },
                timestamp.clone(),
                EventData::None,
            ));
        }
    }

    // Window-level mouse enter/leave (cursor enters/exits OS window)
    if current_in_window && !previous_in_window {
        events.push(SyntheticEvent::new(
            EventType::MouseEnter,
            EventSource::User,
            root_node,
            timestamp.clone(),
            EventData::None,
        ));
    }
    if !current_in_window && previous_in_window {
        events.push(SyntheticEvent::new(
            EventType::MouseLeave,
            EventSource::User,
            root_node,
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Keyboard Events
    // W3C: Keyboard events target the focused element, falling back to root

    let focus_target = focus_manager
        .get_focused_node()
        .copied()
        .unwrap_or(root_node);

    let current_key = current_state
        .keyboard_state
        .current_virtual_keycode
        .into_option();
    let previous_key = previous_state
        .keyboard_state
        .current_virtual_keycode
        .into_option();

    // KeyDown: Fires when a new key is pressed
    // Case 1: New key pressed (current != previous)
    // Case 2: Same key pressed again after release (current.is_some() && previous.is_none())
    let keyboard_data = EventData::Keyboard(KeyboardEventData {
        key_code: current_key.map_or(0, |k| k as u32),
        char_code: None, // Character is available from the keyboard state
        modifiers,
        repeat: false,
    });

    if current_key.is_some() && (current_key != previous_key || previous_key.is_none()) {
        events.push(SyntheticEvent::new(
            EventType::KeyDown,
            EventSource::User,
            focus_target,
            timestamp.clone(),
            keyboard_data,
        ));
    }
    let key_up_data = EventData::Keyboard(KeyboardEventData {
        key_code: previous_key.map_or(0, |k| k as u32),
        char_code: None,
        modifiers,
        repeat: false,
    });

    if previous_key.is_some() && current_key.is_none() {
        events.push(SyntheticEvent::new(
            EventType::KeyUp,
            EventSource::User,
            focus_target,
            timestamp.clone(),
            key_up_data,
        ));
    }

    // Window State Events

    // Window resize
    if current_state.size.dimensions != previous_state.size.dimensions {
        events.push(SyntheticEvent::new(
            EventType::WindowResize,
            EventSource::User,
            root_node,
            timestamp.clone(),
            EventData::Window(WindowEventData {
                size: Some(LogicalRect {
                    origin: LogicalPosition { x: 0.0, y: 0.0 },
                    size: current_state.size.dimensions,
                }),
                position: None,
            }),
        ));
    }

    // Window moved
    if current_state.position != previous_state.position {
        if let WindowPosition::Initialized(phys_pos) = current_state.position {
            events.push(SyntheticEvent::new(
                EventType::WindowMove,
                EventSource::User,
                root_node,
                timestamp.clone(),
                EventData::Window(WindowEventData {
                    size: None,
                    position: Some(LogicalPosition {
                        x: phys_pos.x as f32,
                        y: phys_pos.y as f32,
                    }),
                }),
            ));
        }
    }

    // Window close requested
    if current_state.flags.close_requested && !previous_state.flags.close_requested {
        events.push(SyntheticEvent::new(
            EventType::WindowClose,
            EventSource::User,
            root_node,
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Window focus changed
    if current_state.window_focused && !previous_state.window_focused {
        events.push(SyntheticEvent::new(
            EventType::WindowFocusIn,
            EventSource::User,
            root_node,
            timestamp.clone(),
            EventData::None,
        ));
    }
    if !current_state.window_focused && previous_state.window_focused {
        events.push(SyntheticEvent::new(
            EventType::WindowFocusOut,
            EventSource::User,
            root_node,
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Theme changed
    if current_state.theme != previous_state.theme {
        events.push(SyntheticEvent::new(
            EventType::ThemeChange,
            EventSource::User,
            root_node,
            timestamp.clone(),
            EventData::None,
        ));
    }

    // DPI changed (moved to a different-DPI monitor, or system DPI setting changed)
    if current_state.size.dpi != previous_state.size.dpi {
        events.push(SyntheticEvent::new(
            EventType::WindowDpiChanged,
            EventSource::User,
            root_node,
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Monitor changed (window moved to a different monitor)
    if current_state.monitor_id != previous_state.monitor_id
        && current_state.monitor_id != azul_css::corety::OptionU32::None
        && previous_state.monitor_id != azul_css::corety::OptionU32::None
    {
        events.push(SyntheticEvent::new(
            EventType::WindowMonitorChanged,
            EventSource::User,
            root_node,
            timestamp.clone(),
            EventData::None,
        ));
    }

    // File Drop Events
    //
    // FileHover fires while a file is hovered over the window; FileHoverCancel
    // fires once when that hover ends without a drop. Both depend on platform
    // backends calling `file_drop_manager.set_hovered_file(Some/None)` from
    // their drag-enter / drag-leave handlers (macOS `draggingEntered` /
    // `draggingExited`, Windows OLE `IDropTarget::DragEnter` / `DragLeave`).

    // MWA-B7: target the node UNDER the drag position (mouse_target already
    // falls back to the root when nothing is hovered). Targeting the root
    // unconditionally made per-node drop targets impossible — a DroppedFile
    // callback registered on any non-root node could never fire.
    if file_drop_manager.get_hovered_file().is_some() {
        events.push(SyntheticEvent::new(
            EventType::FileHover,
            EventSource::User,
            mouse_target,
            timestamp.clone(),
            EventData::None,
        ));
    } else if file_drop_manager.hover_was_cancelled() {
        events.push(SyntheticEvent::new(
            EventType::FileHoverCancel,
            EventSource::User,
            mouse_target,
            timestamp.clone(),
            EventData::None,
        ));
    }

    if file_drop_manager.get_dropped_file().is_some() {
        events.push(SyntheticEvent::new(
            EventType::FileDrop,
            EventSource::User,
            mouse_target,
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Pen / Stylus Events (W3C PointerEvent, pointerType "pen")
    // Diff the gesture manager's pen state; `pen_event_pending` gates one diff per
    // update (the event loop clears it after this pass, like the sensor manager).
    // Targets the hovered node (pen-as-pointer); full pen data via CallbackInfo.
    if let Some(manager) = gesture_manager {
        if manager.pen_event_pending {
            let pen_data = make_mouse_data(MouseButton::Left);
            match (manager.get_previous_pen_state(), manager.get_pen_state()) {
                (None, Some(_)) => events.push(SyntheticEvent::new(
                    EventType::PenEnter,
                    EventSource::User,
                    mouse_target,
                    timestamp.clone(),
                    pen_data,
                )),
                (Some(_), None) => events.push(SyntheticEvent::new(
                    EventType::PenLeave,
                    EventSource::User,
                    mouse_target,
                    timestamp.clone(),
                    pen_data,
                )),
                (Some(p), Some(c)) => {
                    if !p.in_contact && c.in_contact {
                        events.push(SyntheticEvent::new(
                            EventType::PenDown,
                            EventSource::User,
                            mouse_target,
                            timestamp.clone(),
                            pen_data.clone(),
                        ));
                    } else if p.in_contact && !c.in_contact {
                        events.push(SyntheticEvent::new(
                            EventType::PenUp,
                            EventSource::User,
                            mouse_target,
                            timestamp.clone(),
                            pen_data.clone(),
                        ));
                    }
                    if p.position != c.position {
                        events.push(SyntheticEvent::new(
                            EventType::PenMove,
                            EventSource::User,
                            mouse_target,
                            timestamp.clone(),
                            pen_data,
                        ));
                    }
                }
                (None, None) => {}
            }
        }
    }

    // Gesture Events

    if let Some(manager) = gesture_manager {
        let event_was_mouse_release = !current_mouse_down && previous_mouse_down;

        // Detect DragStart (targeted at hovered node)
        if manager.detect_drag().is_some()
            && !manager.is_dragging() {
                events.push(SyntheticEvent::new(
                    EventType::DragStart,
                    EventSource::User,
                    mouse_target,
                    timestamp.clone(),
                    make_mouse_data(MouseButton::Left),
                ));
            }

        // Detect Drag (continuous movement, targeted at hovered node)
        if manager.is_dragging() && current_mouse_down {
            let current_pos = current_state.mouse_state.cursor_position.get_position();
            let previous_pos = previous_state.mouse_state.cursor_position.get_position();

            if current_pos != previous_pos {
                events.push(SyntheticEvent::new(
                    EventType::Drag,
                    EventSource::User,
                    mouse_target,
                    timestamp.clone(),
                    make_mouse_data(MouseButton::Left),
                ));
            }
        }

        // Detect DragEnd (targeted at hovered node)
        if manager.is_dragging() && event_was_mouse_release {
            events.push(SyntheticEvent::new(
                EventType::DragEnd,
                EventSource::User,
                mouse_target,
                timestamp.clone(),
                make_mouse_data(MouseButton::Left),
            ));

            // When mouse is released during a node drag, generate a Drop event
            // on the current drop target (the node under the cursor)
            if manager.is_node_drag_active() {
                events.push(SyntheticEvent::new(
                    EventType::Drop,
                    EventSource::User,
                    mouse_target, // W3C: Drop targets the node under cursor
                    timestamp.clone(),
                    make_mouse_data(MouseButton::Left),
                ));
            }
        }

        // Detect DragEnter/DragOver/DragLeave events on drop targets
        // W3C: These fire ON the drop target node (the node UNDER the cursor)
        if manager.is_node_drag_active() && current_mouse_down {
            let current_hover = hover_manager.current_hover_node();
            let previous_hover = hover_manager.previous_hover_node();

            // If the hover node changed, generate DragLeave on old + DragEnter on new
            if current_hover != previous_hover {
                if let Some(prev_node) = previous_hover {
                    events.push(SyntheticEvent::new(
                        EventType::DragLeave,
                        EventSource::User,
                        DomNodeId {
                            dom: root_node.dom,
                            node: NodeHierarchyItemId::from_crate_internal(Some(prev_node)),
                        },
                        timestamp.clone(),
                        EventData::None,
                    ));
                }
                if let Some(curr_node) = current_hover {
                    events.push(SyntheticEvent::new(
                        EventType::DragEnter,
                        EventSource::User,
                        DomNodeId {
                            dom: root_node.dom,
                            node: NodeHierarchyItemId::from_crate_internal(Some(curr_node)),
                        },
                        timestamp.clone(),
                        EventData::None,
                    ));
                }
            }

            // DragOver fires continuously while hovering a drop target
            if let Some(curr_node) = current_hover {
                events.push(SyntheticEvent::new(
                    EventType::DragOver,
                    EventSource::User,
                    DomNodeId {
                        dom: root_node.dom,
                        node: NodeHierarchyItemId::from_crate_internal(Some(curr_node)),
                    },
                    timestamp.clone(),
                    EventData::None,
                ));
            }
        }

        // Detect DoubleClick (targeted at hovered node)
        if manager.detect_double_click() {
            events.push(SyntheticEvent::new(
                EventType::DoubleClick,
                EventSource::User,
                mouse_target,
                timestamp.clone(),
                make_mouse_data(MouseButton::Left),
            ));
        }

        // Detect LongPress (targeted at hovered node)
        if let Some(long_press) = manager.detect_long_press() {
            if !long_press.callback_invoked {
                events.push(SyntheticEvent::new(
                    EventType::LongPress,
                    EventSource::User,
                    mouse_target,
                    timestamp.clone(),
                    make_mouse_data(MouseButton::Left),
                ));
            }
        }

        // Detect Swipe gestures (targeted at hovered node)
        if let Some(direction) = manager.detect_swipe_direction() {
            use crate::managers::gesture::GestureDirection;
            let event_type = match direction {
                GestureDirection::Left => EventType::SwipeLeft,
                GestureDirection::Right => EventType::SwipeRight,
                GestureDirection::Up => EventType::SwipeUp,
                GestureDirection::Down => EventType::SwipeDown,
            };
            events.push(SyntheticEvent::new(
                event_type,
                EventSource::User,
                mouse_target,
                timestamp.clone(),
                EventData::None,
            ));
        }

        // Detect Pinch gestures (targeted at hovered node)
        if let Some(pinch) = manager.detect_pinch() {
            let event_type = if pinch.scale < 1.0 {
                EventType::PinchIn
            } else {
                EventType::PinchOut
            };
            events.push(SyntheticEvent::new(
                event_type,
                EventSource::User,
                mouse_target,
                timestamp.clone(),
                EventData::None,
            ));
        }

        // Detect Rotation gestures (targeted at hovered node)
        if let Some(rotation) = manager.detect_rotation() {
            let event_type = if rotation.angle_radians > 0.0 {
                EventType::RotateClockwise
            } else {
                EventType::RotateCounterClockwise
            };
            events.push(SyntheticEvent::new(
                event_type,
                EventSource::User,
                mouse_target,
                timestamp.clone(),
                EventData::None,
            ));
        }

        // Detect Pen events (targeted at hovered node)
        if let Some(pen_state) = manager.get_pen_state() {
            if pen_state.in_contact {
                let current_pos = current_state.mouse_state.cursor_position.get_position();
                let previous_pos = previous_state.mouse_state.cursor_position.get_position();

                if current_pos != previous_pos {
                    events.push(SyntheticEvent::new(
                        EventType::TouchMove, // Use TouchMove as fallback
                        EventSource::User,
                        mouse_target,
                        timestamp.clone(),
                        EventData::None,
                    ));
                }
            }
        }
    }

    // Manager Events (Scroll, Text Input, etc.)

    for manager in managers {
        events.extend(manager.get_pending_events(timestamp.clone()));
    }

    // Deduplication

    deduplicate_synthetic_events(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_core::window::{
        VirtualKeyCode, VirtualKeyCodeVec,
        OptionVirtualKeyCode,
    };

    fn ts() -> Instant { Instant::Tick(SystemTick::new(0)) }

    fn default_state() -> FullWindowState {
        FullWindowState::default()
    }

    fn state_with_key(vk: VirtualKeyCode) -> FullWindowState {
        let mut s = default_state();
        s.keyboard_state.current_virtual_keycode = OptionVirtualKeyCode::Some(vk);
        s.keyboard_state.pressed_virtual_keycodes = VirtualKeyCodeVec::from_vec(vec![vk]);
        s
    }

    fn state_with_left_down(x: f32, y: f32) -> FullWindowState {
        let mut s = default_state();
        s.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(x, y));
        s.mouse_state.left_down = true;
        s
    }

    fn state_with_cursor(x: f32, y: f32) -> FullWindowState {
        let mut s = default_state();
        s.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(x, y));
        s
    }

    fn empty_providers() -> Vec<&'static dyn EventProvider> { vec![] }

    fn run_determine(
        current: &FullWindowState,
        previous: &FullWindowState,
    ) -> Vec<SyntheticEvent> {
        let focus = crate::managers::focus_cursor::FocusManager::new();
        let hover = crate::managers::hover::HoverManager::new();
        let filedrop = crate::managers::file_drop::FileDropManager::new();
        let providers = empty_providers();
        determine_all_events(current, previous, &hover, &focus, &filedrop, None, &providers, None, ts())
    }

    // === Keyboard tests ===

    #[test]
    fn keydown_fires_when_key_newly_pressed() {
        let events = run_determine(&state_with_key(VirtualKeyCode::A), &default_state());
        let kd: Vec<_> = events.iter().filter(|e| e.event_type == EventType::KeyDown).collect();
        assert_eq!(kd.len(), 1);
        assert!(matches!(kd[0].data, EventData::Keyboard(_)));
    }

    #[test]
    fn keydown_skipped_when_same_key_held() {
        let s = state_with_key(VirtualKeyCode::A);
        let events = run_determine(&s, &s);
        let kd = events.iter().filter(|e| e.event_type == EventType::KeyDown).count();
        assert_eq!(kd, 0, "KeyDown should not fire when same key held");
    }

    #[test]
    fn keydown_fires_for_different_key() {
        let events = run_determine(
            &state_with_key(VirtualKeyCode::B),
            &state_with_key(VirtualKeyCode::A),
        );
        let kd = events.iter().filter(|e| e.event_type == EventType::KeyDown).count();
        assert_eq!(kd, 1);
    }

    #[test]
    fn keyup_fires_when_key_released() {
        let events = run_determine(&default_state(), &state_with_key(VirtualKeyCode::A));
        let ku: Vec<_> = events.iter().filter(|e| e.event_type == EventType::KeyUp).collect();
        assert_eq!(ku.len(), 1);
        assert!(matches!(ku[0].data, EventData::Keyboard(_)));
    }

    #[test]
    fn backspace_keydown_has_keyboard_data() {
        let events = run_determine(&state_with_key(VirtualKeyCode::Back), &default_state());
        let kd: Vec<_> = events.iter().filter(|e| e.event_type == EventType::KeyDown).collect();
        assert_eq!(kd.len(), 1);
        match &kd[0].data {
            EventData::Keyboard(kb) => assert_eq!(kb.key_code, VirtualKeyCode::Back as u32),
            other => panic!("Expected Keyboard data, got {other:?}"),
        }
    }

    // === Mouse tests ===

    #[test]
    fn mousedown_fires_on_left_press() {
        let events = run_determine(
            &state_with_left_down(100.0, 200.0),
            &state_with_cursor(100.0, 200.0),
        );
        let md = events.iter().filter(|e| e.event_type == EventType::MouseDown).count();
        assert_eq!(md, 1);
    }

    #[test]
    fn mouseup_fires_on_left_release() {
        let events = run_determine(
            &state_with_cursor(100.0, 200.0),
            &state_with_left_down(100.0, 200.0),
        );
        let mu = events.iter().filter(|e| e.event_type == EventType::MouseUp).count();
        assert_eq!(mu, 1);
    }

    #[test]
    fn no_events_when_state_unchanged() {
        let s = default_state();
        let events = run_determine(&s, &s);
        assert!(events.is_empty(), "Got {} events when state unchanged", events.len());
    }

    #[test]
    fn mouseover_fires_on_cursor_move() {
        let events = run_determine(
            &state_with_cursor(150.0, 250.0),
            &state_with_cursor(100.0, 200.0),
        );
        let mo = events.iter().filter(|e| e.event_type == EventType::MouseOver).count();
        assert_eq!(mo, 1);
    }

    #[test]
    fn key_repeat_fires_keydown_when_previous_cleared() {
        // Simulates what the platform layer does for key repeat:
        // previous has current_virtual_keycode=None (cleared by platform),
        // current has Some(Left). This should fire KeyDown even though
        // the key was already pressed in the previous frame.
        let mut previous = state_with_key(VirtualKeyCode::Left);
        // Platform clears this for repeat detection:
        previous.keyboard_state.current_virtual_keycode = OptionVirtualKeyCode::None;

        let current = state_with_key(VirtualKeyCode::Left);

        let events = run_determine(&current, &previous);
        let kd = events.iter().filter(|e| e.event_type == EventType::KeyDown).count();
        assert_eq!(kd, 1, "Key repeat should fire KeyDown when previous is cleared");
    }

    #[test]
    fn key_repeat_skipped_when_previous_not_cleared() {
        // Without the platform fix: both previous and current have Same(Left).
        // KeyDown should NOT fire (this documents the limitation that the
        // platform layer MUST clear previous for repeats).
        let previous = state_with_key(VirtualKeyCode::Left);
        let current = state_with_key(VirtualKeyCode::Left);

        let events = run_determine(&current, &previous);
        let kd = events.iter().filter(|e| e.event_type == EventType::KeyDown).count();
        assert_eq!(kd, 0, "Without platform clearing, repeat is not detected");
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::cast_precision_loss)]
mod autotest_generated {
    use azul_core::{
        geom::{LogicalSize, PhysicalPositionI32},
        hit_test::{FullHitTest, HitTest, HitTestItem},
        window::{
            OptionVirtualKeyCode, VirtualKeyCode, VirtualKeyCodeVec, WindowTheme,
        },
    };
    use azul_css::{corety::OptionU32, AzString};

    use super::*;
    use crate::managers::{
        file_drop::FileDropManager,
        focus_cursor::FocusManager,
        gesture::{
            DetectedLongPress, DetectedPinch, DetectedRotation, GestureAndDragManager,
            GestureDirection, NativeGestureEvent,
        },
        hover::{HoverManager, InputPointId},
    };

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    fn ts(tick: u64) -> Instant {
        Instant::Tick(SystemTick::new(tick))
    }

    fn node(dom: usize, n: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: dom },
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
        }
    }

    fn root() -> DomNodeId {
        node(0, 0)
    }

    fn ev(event_type: EventType, target: DomNodeId, tick: u64) -> SyntheticEvent {
        SyntheticEvent::new(
            event_type,
            EventSource::User,
            target,
            ts(tick),
            EventData::None,
        )
    }

    fn hit_item() -> HitTestItem {
        HitTestItem {
            point_in_viewport: LogicalPosition::new(0.0, 0.0),
            point_relative_to_item: LogicalPosition::new(0.0, 0.0),
            is_focusable: false,
            is_virtual_view_hit: None,
            hit_depth: 0,
        }
    }

    /// A hit test where `(dom, node)` pairs are "regular" hits.
    fn hits(pairs: &[(usize, usize)]) -> FullHitTest {
        let mut ht = FullHitTest::empty(None);
        for (dom, n) in pairs {
            ht.hovered_nodes
                .entry(DomId { inner: *dom })
                .or_insert_with(HitTest::empty)
                .regular_hit_test_nodes
                .insert(NodeId::new(*n), hit_item());
        }
        ht
    }

    /// Hover manager with `previous` recorded first, so it lands at frame 1.
    fn hover_with(previous: FullHitTest, current: FullHitTest) -> HoverManager {
        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Mouse, previous);
        hm.push_hit_test(InputPointId::Mouse, current);
        hm
    }

    fn state() -> FullWindowState {
        FullWindowState::default()
    }

    fn cursor_at(x: f32, y: f32) -> FullWindowState {
        let mut s = state();
        s.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(x, y));
        s
    }

    /// Test-only `EventProvider` that replays a fixed list of events.
    struct StaticProvider(Vec<SyntheticEvent>);

    impl EventProvider for StaticProvider {
        fn get_pending_events(&self, _timestamp: Instant) -> Vec<SyntheticEvent> {
            self.0.clone()
        }
    }

    /// Test-only `EventProvider` that stamps events with the timestamp it is handed.
    struct EchoProvider(DomNodeId);

    impl EventProvider for EchoProvider {
        fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
            vec![SyntheticEvent::new(
                EventType::Scroll,
                EventSource::User,
                self.0,
                timestamp,
                EventData::None,
            )]
        }
    }

    fn count(events: &[SyntheticEvent], ty: EventType) -> usize {
        events.iter().filter(|e| e.event_type == ty).count()
    }

    fn only(events: &[SyntheticEvent], ty: EventType) -> SyntheticEvent {
        let matching: Vec<_> = events.iter().filter(|e| e.event_type == ty).collect();
        assert_eq!(
            matching.len(),
            1,
            "expected exactly one {ty:?}, got {matching:?}"
        );
        matching[0].clone()
    }

    /// `determine_all_events` with default managers and no providers.
    fn run(
        current: &FullWindowState,
        previous: &FullWindowState,
        hover: &HoverManager,
        gesture: Option<&GestureAndDragManager>,
        wheel: Option<LogicalPosition>,
    ) -> Vec<SyntheticEvent> {
        let focus = FocusManager::new();
        let file_drop = FileDropManager::new();
        let providers: Vec<&dyn EventProvider> = Vec::new();
        determine_all_events(
            current,
            previous,
            hover,
            &focus,
            &file_drop,
            gesture,
            &providers,
            wheel,
            ts(0),
        )
    }

    /// `determine_all_events` with an empty hover manager and no gestures.
    fn run_plain(current: &FullWindowState, previous: &FullWindowState) -> Vec<SyntheticEvent> {
        run(current, previous, &HoverManager::new(), None, None)
    }

    // ==================================================================
    // get_all_hovered_nodes  (numeric: zero / min_max / overflow)
    // ==================================================================

    #[test]
    fn hovered_nodes_empty_manager_is_empty_at_frame_zero() {
        let hm = HoverManager::new();
        assert!(get_all_hovered_nodes(&hm, 0).is_empty());
    }

    #[test]
    fn hovered_nodes_frame_index_usize_max_does_not_panic() {
        let hm = hover_with(hits(&[(0, 1)]), hits(&[(0, 2)]));
        // VecDeque::get(usize::MAX) must return None, not index out of bounds.
        assert!(get_all_hovered_nodes(&hm, usize::MAX).is_empty());
        assert!(get_all_hovered_nodes(&hm, usize::MAX - 1).is_empty());
    }

    #[test]
    fn hovered_nodes_frame_index_past_history_is_empty() {
        let hm = hover_with(hits(&[(0, 1)]), hits(&[(0, 2)]));
        assert_eq!(get_all_hovered_nodes(&hm, 0).len(), 1);
        assert_eq!(get_all_hovered_nodes(&hm, 1).len(), 1);
        assert!(get_all_hovered_nodes(&hm, 2).is_empty());
        assert!(get_all_hovered_nodes(&hm, 100).is_empty());
    }

    #[test]
    fn hovered_nodes_history_is_capped_at_five_frames() {
        let mut hm = HoverManager::new();
        for i in 0..8_usize {
            hm.push_hit_test(InputPointId::Mouse, hits(&[(0, i)]));
        }
        // Most recent push is frame 0.
        assert!(get_all_hovered_nodes(&hm, 0).contains(&(DomId { inner: 0 }, NodeId::new(7))));
        // Ring buffer keeps 5 frames: nodes 7,6,5,4,3.
        assert!(get_all_hovered_nodes(&hm, 4).contains(&(DomId { inner: 0 }, NodeId::new(3))));
        assert!(
            get_all_hovered_nodes(&hm, 5).is_empty(),
            "frame 5 must have been evicted by the 5-frame cap"
        );
    }

    #[test]
    fn hovered_nodes_walks_every_hit_dom_not_just_the_root() {
        let hm = hover_with(
            FullHitTest::empty(None),
            hits(&[(0, 1), (0, 2), (7, 3), (usize::MAX, 4)]),
        );
        let set = get_all_hovered_nodes(&hm, 0);
        assert_eq!(set.len(), 4);
        assert!(set.contains(&(DomId { inner: 0 }, NodeId::new(1))));
        assert!(set.contains(&(DomId { inner: 7 }, NodeId::new(3))));
        assert!(set.contains(&(DomId { inner: usize::MAX }, NodeId::new(4))));
    }

    #[test]
    fn hovered_nodes_accepts_extreme_node_ids() {
        // NodeId is a plain usize index here; no encoding happens in this
        // function, so even usize::MAX must round-trip untouched.
        let hm = hover_with(
            FullHitTest::empty(None),
            hits(&[(0, 0), (0, usize::MAX), (0, usize::MAX - 1)]),
        );
        let set = get_all_hovered_nodes(&hm, 0);
        assert_eq!(set.len(), 3);
        assert!(set.contains(&(DomId { inner: 0 }, NodeId::new(usize::MAX))));
        assert!(set.contains(&(DomId { inner: 0 }, NodeId::ZERO)));
    }

    #[test]
    fn hovered_nodes_ignores_non_regular_hits() {
        let mut ht = FullHitTest::empty(None);
        let mut hit = HitTest::empty();
        hit.cursor_hit_test_nodes.insert(
            NodeId::new(9),
            azul_core::hit_test::CursorHitTestItem {
                cursor_type: azul_core::hit_test::CursorType::Default,
                hit_depth: 0,
                point_in_viewport: LogicalPosition::new(0.0, 0.0),
            },
        );
        ht.hovered_nodes.insert(DomId { inner: 0 }, hit);
        let mut hm = HoverManager::new();
        hm.push_hit_test(InputPointId::Mouse, ht);
        assert!(
            get_all_hovered_nodes(&hm, 0).is_empty(),
            "only regular hits count as hovered"
        );
    }

    // ==================================================================
    // detect_window_state_events
    // ==================================================================

    #[test]
    fn window_state_identical_states_yield_no_events() {
        let s = state();
        assert!(detect_window_state_events(&s, &s, ts(0)).is_empty());
    }

    #[test]
    fn window_resize_reports_the_current_dimensions() {
        let mut current = state();
        current.size.dimensions = LogicalSize::new(1234.0, 5678.0);
        let events = detect_window_state_events(&current, &state(), ts(0));
        let e = only(&events, EventType::WindowResize);
        match e.data {
            EventData::Window(w) => {
                let rect = w.size.expect("resize carries a size");
                assert_eq!(rect.origin.x, 0.0);
                assert_eq!(rect.size.width, 1234.0);
                assert_eq!(rect.size.height, 5678.0);
            }
            other => panic!("expected Window data, got {other:?}"),
        }
    }

    #[test]
    fn window_resize_below_the_quantization_step_is_not_a_resize() {
        // LogicalSize::eq quantizes to 1/1000, so a sub-milli-pixel change is
        // *not* a size change and must not spam WindowResize every frame.
        let previous = state();
        let mut current = state();
        current.size.dimensions = LogicalSize::new(640.0004, 480.0);
        assert!(detect_window_state_events(&current, &previous, ts(0)).is_empty());

        current.size.dimensions = LogicalSize::new(640.5, 480.0);
        assert_eq!(
            count(
                &detect_window_state_events(&current, &previous, ts(0)),
                EventType::WindowResize
            ),
            1
        );
    }

    #[test]
    fn window_resize_nan_dimensions_are_stable_and_do_not_panic() {
        let mut nan = state();
        nan.size.dimensions = LogicalSize::new(f32::NAN, f32::NAN);
        // quantize() maps NaN to a single sentinel, so NaN == NaN: a window
        // stuck at NaN must not emit a resize on every single frame.
        assert!(
            detect_window_state_events(&nan, &nan.clone(), ts(0)).is_empty(),
            "NaN size compared against itself must not fire a resize"
        );

        // ...but a real -> NaN transition still reports (and forwards the NaN).
        let events = detect_window_state_events(&nan, &state(), ts(0));
        let e = only(&events, EventType::WindowResize);
        match e.data {
            EventData::Window(w) => {
                assert!(w.size.expect("size").size.width.is_nan());
            }
            other => panic!("expected Window data, got {other:?}"),
        }
    }

    #[test]
    fn window_resize_infinite_dimensions_do_not_panic() {
        let mut current = state();
        current.size.dimensions = LogicalSize::new(f32::INFINITY, f32::NEG_INFINITY);
        let events = detect_window_state_events(&current, &state(), ts(0));
        assert_eq!(count(&events, EventType::WindowResize), 1);
    }

    #[test]
    fn window_move_saturates_extreme_i32_coordinates_into_f32() {
        let mut current = state();
        current.position = WindowPosition::Initialized(PhysicalPositionI32 {
            x: i32::MIN,
            y: i32::MAX,
        });
        let events = detect_window_state_events(&current, &state(), ts(0));
        let e = only(&events, EventType::WindowMove);
        match e.data {
            EventData::Window(w) => {
                let pos = w.position.expect("move carries a position");
                assert_eq!(pos.x, i32::MIN as f32);
                // i32::MAX is not representable in f32: it rounds *up* to 2^31.
                assert_eq!(pos.y, 2_147_483_648.0_f32);
            }
            other => panic!("expected Window data, got {other:?}"),
        }
    }

    #[test]
    fn window_move_to_uninitialized_emits_nothing() {
        let previous = {
            let mut s = state();
            s.position = WindowPosition::Initialized(PhysicalPositionI32 { x: 5, y: 5 });
            s
        };
        // current.position == Uninitialized: state *changed*, but there is no
        // position to report, so no event.
        let events = detect_window_state_events(&state(), &previous, ts(0));
        assert_eq!(count(&events, EventType::WindowMove), 0);
    }

    #[test]
    fn window_move_fires_for_relative_child_window_positions() {
        let mut current = state();
        current.position =
            WindowPosition::RelativeToParentWindow(PhysicalPositionI32 { x: -3, y: 9 });
        let events = detect_window_state_events(&current, &state(), ts(0));
        let e = only(&events, EventType::WindowMove);
        match e.data {
            EventData::Window(w) => {
                let pos = w.position.expect("position");
                assert_eq!(pos.x, -3.0);
                assert_eq!(pos.y, 9.0);
            }
            other => panic!("expected Window data, got {other:?}"),
        }
    }

    #[test]
    fn window_theme_change_emits_exactly_one_theme_event() {
        let mut current = state();
        current.theme = WindowTheme::DarkMode;
        let events = detect_window_state_events(&current, &state(), ts(0));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::ThemeChange);
        assert_eq!(events[0].target, root());
    }

    #[test]
    fn window_mouse_enter_leave_transitions_are_exclusive() {
        let inside = cursor_at(1.0, 1.0);
        let outside = {
            let mut s = state();
            s.mouse_state.cursor_position =
                CursorPosition::OutOfWindow(LogicalPosition::new(1.0, 1.0));
            s
        };
        let uninit = state();

        let enter = detect_window_state_events(&inside, &uninit, ts(0));
        assert_eq!(count(&enter, EventType::MouseEnter), 1);
        assert_eq!(count(&enter, EventType::MouseLeave), 0);

        let leave = detect_window_state_events(&outside, &inside, ts(0));
        assert_eq!(count(&leave, EventType::MouseLeave), 1);
        assert_eq!(count(&leave, EventType::MouseEnter), 0);

        // OutOfWindow -> Uninitialized: neither is "in window", so nothing fires.
        let neither = detect_window_state_events(&uninit, &outside, ts(0));
        assert!(neither.is_empty());
    }

    #[test]
    fn window_state_events_all_target_the_root_node() {
        let mut current = cursor_at(10.0, 10.0);
        current.size.dimensions = LogicalSize::new(800.0, 600.0);
        current.position = WindowPosition::Initialized(PhysicalPositionI32 { x: 1, y: 2 });
        current.theme = WindowTheme::DarkMode;

        let events = detect_window_state_events(&current, &state(), ts(7));
        assert_eq!(events.len(), 4, "resize + move + theme + mouse-enter");
        assert!(events.iter().all(|e| e.target == root()));
        assert!(events.iter().all(|e| e.timestamp == ts(7)));
        assert!(events.iter().all(|e| e.source == EventSource::User));
    }

    #[test]
    fn min_dimensions_change_resizes_in_one_detector_but_not_the_other() {
        // detect_window_state_events compares the whole WindowSize (incl. the
        // min/max constraints), determine_all_events compares only `.dimensions`.
        // Documenting the divergence: a min-size-only change resizes in one and
        // not in the other.
        let mut current = state();
        current.size.min_dimensions = Some(LogicalSize::new(100.0, 100.0)).into();

        let differ = detect_window_state_events(&current, &state(), ts(0));
        assert_eq!(count(&differ, EventType::WindowResize), 1);

        let all = run_plain(&current, &state());
        assert_eq!(
            count(&all, EventType::WindowResize),
            0,
            "determine_all_events only diffs `size.dimensions`"
        );
    }

    // ==================================================================
    // determine_events_from_managers
    // ==================================================================

    #[test]
    fn from_managers_no_change_no_providers_is_empty() {
        let s = state();
        let providers: Vec<&dyn EventProvider> = Vec::new();
        assert!(determine_events_from_managers(&s, &s, &providers, ts(0)).is_empty());
    }

    #[test]
    fn from_managers_collapses_a_flood_of_identical_events() {
        let s = state();
        let target = node(0, 4);
        let provider = StaticProvider((0..5_000).map(|i| ev(EventType::Scroll, target, i)).collect());
        let providers: Vec<&dyn EventProvider> = vec![&provider];
        let events = determine_events_from_managers(&s, &s, &providers, ts(0));
        assert_eq!(events.len(), 1, "same (target, type) must coalesce");
        assert_eq!(events[0].event_type, EventType::Scroll);
    }

    #[test]
    fn from_managers_dedup_keeps_the_latest_timestamp() {
        let s = state();
        let target = node(0, 4);
        let provider = StaticProvider(vec![
            ev(EventType::Scroll, target, 1),
            ev(EventType::Scroll, target, 9),
            ev(EventType::Scroll, target, 5),
        ]);
        let providers: Vec<&dyn EventProvider> = vec![&provider];
        let events = determine_events_from_managers(&s, &s, &providers, ts(0));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].timestamp, ts(9));
    }

    #[test]
    fn from_managers_dedup_survives_u64_max_timestamps() {
        let s = state();
        let target = node(0, 4);
        let provider = StaticProvider(vec![
            ev(EventType::Scroll, target, u64::MAX),
            ev(EventType::Scroll, target, 0),
        ]);
        let providers: Vec<&dyn EventProvider> = vec![&provider];
        let events = determine_events_from_managers(&s, &s, &providers, ts(0));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].timestamp, ts(u64::MAX));
    }

    #[test]
    fn from_managers_provider_event_dedups_against_the_window_event() {
        let mut current = state();
        current.theme = WindowTheme::DarkMode;
        // The provider claims the exact same (root, ThemeChange) slot.
        let provider = StaticProvider(vec![ev(EventType::ThemeChange, root(), 3)]);
        let providers: Vec<&dyn EventProvider> = vec![&provider];
        let events = determine_events_from_managers(&current, &state(), &providers, ts(0));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EventType::ThemeChange);
        assert_eq!(events[0].timestamp, ts(3), "the later timestamp wins");
    }

    #[test]
    fn from_managers_forwards_its_timestamp_to_every_provider() {
        let s = state();
        let provider = EchoProvider(node(0, 2));
        let providers: Vec<&dyn EventProvider> = vec![&provider];
        let events = determine_events_from_managers(&s, &s, &providers, ts(4242));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].timestamp, ts(4242));
    }

    #[test]
    fn from_managers_many_providers_with_distinct_targets_all_survive() {
        let s = state();
        let owned: Vec<StaticProvider> = (0..256_usize)
            .map(|i| StaticProvider(vec![ev(EventType::Scroll, node(i % 4, i), 0)]))
            .collect();
        let providers: Vec<&dyn EventProvider> =
            owned.iter().map(|p| p as &dyn EventProvider).collect();
        let events = determine_events_from_managers(&s, &s, &providers, ts(0));
        assert_eq!(events.len(), 256);
    }

    #[test]
    fn from_managers_empty_provider_lists_are_harmless() {
        let s = state();
        let empty = StaticProvider(Vec::new());
        let providers: Vec<&dyn EventProvider> = vec![&empty, &empty, &empty];
        assert!(determine_events_from_managers(&s, &s, &providers, ts(0)).is_empty());
    }

    // ==================================================================
    // determine_all_events - mouse buttons / bitmask
    // ==================================================================

    #[test]
    fn all_buttons_pressed_at_once_reports_a_full_bitmask() {
        let mut current = cursor_at(10.0, 10.0);
        current.mouse_state.left_down = true;
        current.mouse_state.right_down = true;
        current.mouse_state.middle_down = true;

        let events = run_plain(&current, &cursor_at(10.0, 10.0));
        // NOTE: the three per-button MouseDowns share (target, MouseDown), so
        // deduplication collapses them into a single event - the first (Left).
        let down = only(&events, EventType::MouseDown);
        match down.data {
            EventData::Mouse(m) => {
                assert_eq!(m.buttons, 0b0000_0111, "left|right|middle bitmask");
                assert_eq!(m.button, MouseButton::Left);
                assert_eq!(m.position, LogicalPosition::new(10.0, 10.0));
            }
            other => panic!("expected Mouse data, got {other:?}"),
        }
    }

    #[test]
    fn releasing_every_button_clears_the_bitmask() {
        let mut previous = cursor_at(10.0, 10.0);
        previous.mouse_state.left_down = true;
        previous.mouse_state.right_down = true;
        previous.mouse_state.middle_down = true;

        let events = run_plain(&cursor_at(10.0, 10.0), &previous);
        let up = only(&events, EventType::MouseUp);
        match up.data {
            EventData::Mouse(m) => assert_eq!(m.buttons, 0),
            other => panic!("expected Mouse data, got {other:?}"),
        }
    }

    #[test]
    fn mouse_events_carry_the_live_modifier_state() {
        let mut current = cursor_at(0.0, 0.0);
        current.keyboard_state.pressed_virtual_keycodes = VirtualKeyCodeVec::from_vec(vec![
            VirtualKeyCode::LShift,
            VirtualKeyCode::RControl,
            VirtualKeyCode::LAlt,
            VirtualKeyCode::LWin,
        ]);
        current.mouse_state.left_down = true;

        let events = run_plain(&current, &cursor_at(0.0, 0.0));
        let down = only(&events, EventType::MouseDown);
        match down.data {
            EventData::Mouse(m) => {
                assert!(m.modifiers.shift && m.modifiers.ctrl && m.modifiers.alt && m.modifiers.meta);
            }
            other => panic!("expected Mouse data, got {other:?}"),
        }
    }

    #[test]
    fn mouse_position_falls_back_to_origin_when_the_cursor_is_unknown() {
        // cursor_position is Uninitialized -> get_position() is None -> (0,0).
        let mut current = state();
        current.mouse_state.left_down = true;
        let events = run_plain(&current, &state());
        let down = only(&events, EventType::MouseDown);
        match down.data {
            EventData::Mouse(m) => assert_eq!(m.position, LogicalPosition::new(0.0, 0.0)),
            other => panic!("expected Mouse data, got {other:?}"),
        }
    }

    // ==================================================================
    // determine_all_events - wheel delta (numeric edges)
    // ==================================================================

    #[test]
    fn wheel_delta_none_emits_no_scroll() {
        let s = state();
        assert_eq!(
            count(&run(&s, &s, &HoverManager::new(), None, None), EventType::Scroll),
            0
        );
    }

    #[test]
    fn wheel_delta_zero_emits_no_scroll() {
        let s = state();
        let events = run(
            &s,
            &s,
            &HoverManager::new(),
            None,
            Some(LogicalPosition::new(0.0, 0.0)),
        );
        assert_eq!(count(&events, EventType::Scroll), 0);
    }

    #[test]
    fn wheel_delta_negative_zero_emits_no_scroll() {
        // IEEE-754: -0.0 == 0.0, so a signed-zero delta must stay silent.
        let s = state();
        let events = run(
            &s,
            &s,
            &HoverManager::new(),
            None,
            Some(LogicalPosition::new(-0.0, -0.0)),
        );
        assert_eq!(count(&events, EventType::Scroll), 0);
    }

    #[test]
    fn wheel_delta_nan_still_emits_a_scroll_and_forwards_the_nan() {
        // NaN != 0.0 is true, so a NaN delta *does* fire. Assert it is passed
        // through untouched rather than silently turning into 0.
        let s = state();
        let events = run(
            &s,
            &s,
            &HoverManager::new(),
            None,
            Some(LogicalPosition::new(f32::NAN, f32::NAN)),
        );
        let scroll = only(&events, EventType::Scroll);
        match scroll.data {
            EventData::Scroll(sd) => {
                assert!(sd.delta.x.is_nan() && sd.delta.y.is_nan());
                assert_eq!(sd.delta_mode, ScrollDeltaMode::Pixel);
            }
            other => panic!("expected Scroll data, got {other:?}"),
        }
    }

    #[test]
    fn wheel_delta_infinite_is_forwarded_without_saturation() {
        let s = state();
        let events = run(
            &s,
            &s,
            &HoverManager::new(),
            None,
            Some(LogicalPosition::new(f32::INFINITY, f32::NEG_INFINITY)),
        );
        let scroll = only(&events, EventType::Scroll);
        match scroll.data {
            EventData::Scroll(sd) => {
                assert_eq!(sd.delta.x, f32::INFINITY);
                assert_eq!(sd.delta.y, f32::NEG_INFINITY);
            }
            other => panic!("expected Scroll data, got {other:?}"),
        }
    }

    #[test]
    fn wheel_scroll_targets_the_hovered_node_not_the_root() {
        let s = state();
        let hm = hover_with(hits(&[(2, 6)]), hits(&[(2, 6)]));
        let events = run(
            &s,
            &s,
            &hm,
            None,
            Some(LogicalPosition::new(0.0, -120.0)),
        );
        let scroll = only(&events, EventType::Scroll);
        assert_eq!(scroll.target, node(2, 6));
    }

    // ==================================================================
    // determine_all_events - cursor movement + f32 quantization
    // ==================================================================

    #[test]
    fn mouseover_ignores_sub_quantum_cursor_jitter() {
        // LogicalPosition::eq quantizes to 1/1000 px: a 0.4 micro-pixel move is
        // no move at all, and must not wake the whole event pipeline.
        let events = run_plain(&cursor_at(100.0004, 0.0), &cursor_at(100.0, 0.0));
        assert_eq!(count(&events, EventType::MouseOver), 0);

        let events = run_plain(&cursor_at(100.5, 0.0), &cursor_at(100.0, 0.0));
        assert_eq!(count(&events, EventType::MouseOver), 1);
    }

    #[test]
    fn mouseover_treats_saturating_far_coordinates_as_unchanged() {
        // quantize() saturates on f32 -> i64: 1e30 and f32::MAX both land on
        // i64::MAX, so this "move" is invisible to the comparison. Deterministic
        // and panic-free is what matters here.
        let events = run_plain(&cursor_at(1e30, 0.0), &cursor_at(f32::MAX, 0.0));
        assert!(events.is_empty(), "got {events:?}");
    }

    #[test]
    fn mouseover_nan_cursor_positions_do_not_panic_or_thrash() {
        let events = run_plain(&cursor_at(f32::NAN, f32::NAN), &cursor_at(f32::NAN, f32::NAN));
        assert_eq!(
            count(&events, EventType::MouseOver),
            0,
            "a NaN cursor stuck in place must not fire MouseOver every frame"
        );

        // NaN -> real coordinate is a genuine move.
        let events = run_plain(&cursor_at(5.0, 5.0), &cursor_at(f32::NAN, f32::NAN));
        assert_eq!(count(&events, EventType::MouseOver), 1);
    }

    #[test]
    fn mouseover_never_fires_while_the_cursor_is_outside_the_window() {
        let mut current = state();
        current.mouse_state.cursor_position =
            CursorPosition::OutOfWindow(LogicalPosition::new(9.0, 9.0));
        let events = run_plain(&current, &state());
        assert_eq!(count(&events, EventType::MouseOver), 0);
    }

    #[test]
    fn entering_the_window_emits_mouse_enter_at_the_root() {
        let events = run_plain(&cursor_at(3.0, 3.0), &state());
        let enter = only(&events, EventType::MouseEnter);
        assert_eq!(enter.target, root());
        // Uninitialized -> InWindow is also a position change.
        assert_eq!(count(&events, EventType::MouseOver), 1);
    }

    #[test]
    fn leaving_the_window_emits_mouse_leave_at_the_root() {
        let mut current = state();
        current.mouse_state.cursor_position =
            CursorPosition::OutOfWindow(LogicalPosition::new(3.0, 3.0));
        let events = run_plain(&current, &cursor_at(3.0, 3.0));
        let leave = only(&events, EventType::MouseLeave);
        assert_eq!(leave.target, root());
    }

    // ==================================================================
    // determine_all_events - per-node hover chain (W3C enter/leave)
    // ==================================================================

    #[test]
    fn hover_chain_diff_emits_leave_for_lost_and_enter_for_gained_nodes() {
        let s = state();
        let hm = hover_with(hits(&[(0, 1), (0, 3)]), hits(&[(0, 1), (0, 5)]));
        let events = run(&s, &s, &hm, None, None);

        let leave = only(&events, EventType::MouseLeave);
        assert_eq!(leave.target, node(0, 3));
        let enter = only(&events, EventType::MouseEnter);
        assert_eq!(enter.target, node(0, 5));
        // Node 1 stayed hovered -> no event for it.
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn hover_chain_diff_spans_child_doms() {
        let s = state();
        let hm = hover_with(FullHitTest::empty(None), hits(&[(0, 1), (9, 2)]));
        let events = run(&s, &s, &hm, None, None);
        assert_eq!(count(&events, EventType::MouseEnter), 2);
        assert!(events.iter().any(|e| e.target == node(9, 2)));
        assert!(events.iter().any(|e| e.target == node(0, 1)));
    }

    #[test]
    fn hover_chain_enter_targets_round_trip_through_the_1_based_encoding() {
        // NodeHierarchyItemId stores `index + 1`; make sure the edge indices
        // survive the encode/decode round-trip in the emitted target.
        for id in [0_usize, 1, 12_345, usize::MAX - 1] {
            let s = state();
            let hm = hover_with(FullHitTest::empty(None), hits(&[(0, id)]));
            let events = run(&s, &s, &hm, None, None);
            let enter = only(&events, EventType::MouseEnter);
            assert_eq!(
                enter.target.node.into_crate_internal(),
                Some(NodeId::new(id)),
                "target NodeId must decode back to {id}"
            );
        }
    }

    #[test]
    fn node_level_and_window_level_enter_collapse_at_the_root() {
        // Node 0 of DOM 0 *is* the root target, so its per-node MouseEnter and
        // the window-level MouseEnter share a dedup slot.
        let hm = hover_with(FullHitTest::empty(None), hits(&[(0, 0)]));
        let events = run(&cursor_at(1.0, 1.0), &state(), &hm, None, None);
        assert_eq!(count(&events, EventType::MouseEnter), 1);
    }

    #[test]
    fn an_unchanged_hover_chain_emits_no_enter_or_leave() {
        let s = state();
        let hm = hover_with(hits(&[(0, 1), (4, 2)]), hits(&[(0, 1), (4, 2)]));
        let events = run(&s, &s, &hm, None, None);
        assert!(events.is_empty(), "got {events:?}");
    }

    // ==================================================================
    // determine_all_events - click synthesis
    // ==================================================================

    #[test]
    fn click_is_synthesized_when_release_lands_on_the_press_node() {
        let mut previous = cursor_at(20.0, 20.0);
        previous.mouse_state.left_down = true;
        let hm = hover_with(hits(&[(0, 8)]), hits(&[(0, 8)]));

        let events = run(&cursor_at(20.0, 20.0), &previous, &hm, None, None);
        let click = only(&events, EventType::Click);
        assert_eq!(click.target, node(0, 8));
        assert_eq!(count(&events, EventType::MouseUp), 1);
    }

    #[test]
    fn no_click_when_the_hover_node_changed_between_press_and_release() {
        let mut previous = cursor_at(20.0, 20.0);
        previous.mouse_state.left_down = true;
        let hm = hover_with(hits(&[(0, 3)]), hits(&[(0, 5)]));

        let events = run(&cursor_at(20.0, 20.0), &previous, &hm, None, None);
        assert_eq!(count(&events, EventType::Click), 0);
    }

    #[test]
    fn no_click_when_nothing_is_hovered() {
        let mut previous = cursor_at(20.0, 20.0);
        previous.mouse_state.left_down = true;
        // Empty hover manager: prev == curr == None, but None is not a node.
        let events = run_plain(&cursor_at(20.0, 20.0), &previous);
        assert_eq!(count(&events, EventType::Click), 0);
        assert_eq!(count(&events, EventType::MouseUp), 1);
    }

    // ==================================================================
    // determine_all_events - keyboard
    // ==================================================================

    #[test]
    fn keyboard_events_target_the_focused_node() {
        let mut current = state();
        current.keyboard_state.current_virtual_keycode =
            OptionVirtualKeyCode::Some(VirtualKeyCode::A);
        let mut focus = FocusManager::new();
        focus.set_focused_node(Some(node(3, 11)));

        let hover = HoverManager::new();
        let file_drop = FileDropManager::new();
        let providers: Vec<&dyn EventProvider> = Vec::new();
        let events = determine_all_events(
            &current,
            &state(),
            &hover,
            &focus,
            &file_drop,
            None,
            &providers,
            None,
            ts(0),
        );
        let down = only(&events, EventType::KeyDown);
        assert_eq!(down.target, node(3, 11));
    }

    #[test]
    fn keyup_reports_the_previous_keycode_not_zero() {
        let mut previous = state();
        previous.keyboard_state.current_virtual_keycode =
            OptionVirtualKeyCode::Some(VirtualKeyCode::F12);
        let events = run_plain(&state(), &previous);
        let up = only(&events, EventType::KeyUp);
        match up.data {
            EventData::Keyboard(k) => {
                assert_eq!(k.key_code, VirtualKeyCode::F12 as u32);
                assert!(!k.repeat);
                assert!(k.char_code.is_none());
            }
            other => panic!("expected Keyboard data, got {other:?}"),
        }
    }

    #[test]
    fn no_keyboard_events_when_no_key_is_involved() {
        let s = state();
        let events = run_plain(&s, &s);
        assert_eq!(count(&events, EventType::KeyDown), 0);
        assert_eq!(count(&events, EventType::KeyUp), 0);
    }

    #[test]
    fn swapping_keys_in_one_frame_emits_keydown_but_no_keyup() {
        // Documented behaviour: KeyUp only fires on Some -> None. A direct
        // A -> B swap therefore emits KeyDown(B) and *drops* the KeyUp(A).
        let mut current = state();
        current.keyboard_state.current_virtual_keycode =
            OptionVirtualKeyCode::Some(VirtualKeyCode::B);
        let mut previous = state();
        previous.keyboard_state.current_virtual_keycode =
            OptionVirtualKeyCode::Some(VirtualKeyCode::A);

        let events = run_plain(&current, &previous);
        assert_eq!(count(&events, EventType::KeyDown), 1);
        assert_eq!(count(&events, EventType::KeyUp), 0);
    }

    // ==================================================================
    // determine_all_events - window state
    // ==================================================================

    #[test]
    fn dpi_change_emits_a_dpi_event_at_the_u32_extremes() {
        let mut current = state();
        current.size.dpi = u32::MAX;
        let mut previous = state();
        previous.size.dpi = 0;

        let events = run_plain(&current, &previous);
        assert_eq!(count(&events, EventType::WindowDpiChanged), 1);
        assert_eq!(count(&events, EventType::WindowResize), 0);
    }

    #[test]
    fn monitor_change_needs_a_known_monitor_on_both_sides() {
        let mut current = state();
        current.monitor_id = OptionU32::Some(2);

        // None -> Some(2): the window learned its monitor, that is not a move.
        let events = run_plain(&current, &state());
        assert_eq!(count(&events, EventType::WindowMonitorChanged), 0);

        let mut previous = state();
        previous.monitor_id = OptionU32::Some(1);
        let events = run_plain(&current, &previous);
        assert_eq!(count(&events, EventType::WindowMonitorChanged), 1);

        // Some(u32::MAX) -> None: still not a monitor change.
        let mut max = state();
        max.monitor_id = OptionU32::Some(u32::MAX);
        let events = run_plain(&state(), &max);
        assert_eq!(count(&events, EventType::WindowMonitorChanged), 0);
    }

    #[test]
    fn close_request_emits_window_close_once() {
        let mut current = state();
        current.flags.close_requested = true;
        let events = run_plain(&current, &state());
        assert_eq!(count(&events, EventType::WindowClose), 1);
        // Still requested on the next frame -> no repeat.
        let events = run_plain(&current, &current.clone());
        assert_eq!(count(&events, EventType::WindowClose), 0);
    }

    #[test]
    fn window_focus_transitions_emit_focus_in_and_out() {
        let unfocused = {
            let mut s = state();
            s.window_focused = false;
            s
        };
        let events = run_plain(&unfocused, &state());
        assert_eq!(count(&events, EventType::WindowFocusOut), 1);

        let events = run_plain(&state(), &unfocused);
        assert_eq!(count(&events, EventType::WindowFocusIn), 1);
    }

    #[test]
    fn determine_all_events_ignores_relative_child_window_moves() {
        // Divergence from detect_window_state_events, which *does* report it.
        let mut current = state();
        current.position =
            WindowPosition::RelativeToParentWindow(PhysicalPositionI32 { x: 4, y: 4 });
        let events = run_plain(&current, &state());
        assert_eq!(count(&events, EventType::WindowMove), 0);
    }

    #[test]
    fn determine_all_events_accepts_a_u64_max_timestamp() {
        let mut current = state();
        current.theme = WindowTheme::DarkMode;
        let hover = HoverManager::new();
        let focus = FocusManager::new();
        let file_drop = FileDropManager::new();
        let providers: Vec<&dyn EventProvider> = Vec::new();
        let events = determine_all_events(
            &current,
            &state(),
            &hover,
            &focus,
            &file_drop,
            None,
            &providers,
            None,
            ts(u64::MAX),
        );
        let theme = only(&events, EventType::ThemeChange);
        assert_eq!(theme.timestamp, ts(u64::MAX));
    }

    // ==================================================================
    // determine_all_events - file drop
    // ==================================================================

    fn run_with_file_drop(file_drop: &FileDropManager) -> Vec<SyntheticEvent> {
        let s = state();
        let hover = HoverManager::new();
        let focus = FocusManager::new();
        let providers: Vec<&dyn EventProvider> = Vec::new();
        determine_all_events(
            &s,
            &s,
            &hover,
            &focus,
            file_drop,
            None,
            &providers,
            None,
            ts(0),
        )
    }

    #[test]
    fn hovering_a_file_emits_file_hover_and_suppresses_the_cancel() {
        let mut fd = FileDropManager::new();
        fd.set_hovered_file(Some(AzString::from(String::from("/tmp/a.png"))));
        fd.set_hovered_file(None); // latches hover_cancelled
        fd.set_hovered_file(Some(AzString::from(String::from("/tmp/b.png"))));
        assert!(fd.hover_was_cancelled(), "flag is still latched");

        let events = run_with_file_drop(&fd);
        assert_eq!(count(&events, EventType::FileHover), 1);
        assert_eq!(
            count(&events, EventType::FileHoverCancel),
            0,
            "an active hover must win over the stale cancel flag"
        );
    }

    #[test]
    fn a_cancelled_hover_emits_file_hover_cancel() {
        let mut fd = FileDropManager::new();
        fd.set_hovered_file(Some(AzString::from(String::from("/tmp/a.png"))));
        fd.set_hovered_file(None);

        let events = run_with_file_drop(&fd);
        assert_eq!(count(&events, EventType::FileHoverCancel), 1);
        assert_eq!(count(&events, EventType::FileHover), 0);
    }

    #[test]
    fn an_empty_path_still_counts_as_a_hovered_file() {
        let mut fd = FileDropManager::new();
        fd.set_hovered_file(Some(AzString::from(String::new())));
        let events = run_with_file_drop(&fd);
        assert_eq!(count(&events, EventType::FileHover), 1);
    }

    #[test]
    fn pathological_file_paths_do_not_panic() {
        // 256 KiB of text, an interior NUL, an RTL override and astral-plane
        // codepoints - the event pass must not touch the bytes at all.
        let mut path = "\u{202e}🎉\u{0}".repeat(4);
        path.push_str(&"ß".repeat(256 * 1024));
        let mut fd = FileDropManager::new();
        fd.set_dropped_file(Some(AzString::from(path)));
        fd.set_hovered_file(Some(AzString::from(String::from("\u{feff}"))));

        let events = run_with_file_drop(&fd);
        assert_eq!(count(&events, EventType::FileDrop), 1);
        assert_eq!(count(&events, EventType::FileHover), 1);
    }

    #[test]
    fn a_multi_file_drop_still_emits_exactly_one_file_drop_event() {
        let mut fd = FileDropManager::new();
        fd.set_dropped_files(
            (0..1000)
                .map(|i| AzString::from(format!("/tmp/{i}.bin")))
                .collect(),
        );
        let events = run_with_file_drop(&fd);
        assert_eq!(count(&events, EventType::FileDrop), 1);
    }

    #[test]
    fn file_events_target_the_hovered_node() {
        let s = state();
        let hover = hover_with(hits(&[(1, 4)]), hits(&[(1, 4)]));
        let focus = FocusManager::new();
        let mut fd = FileDropManager::new();
        fd.set_dropped_file(Some(AzString::from(String::from("/tmp/x"))));
        let providers: Vec<&dyn EventProvider> = Vec::new();
        let events = determine_all_events(
            &s, &s, &hover, &focus, &fd, None, &providers, None, ts(0),
        );
        let drop = only(&events, EventType::FileDrop);
        assert_eq!(drop.target, node(1, 4));
    }

    // ==================================================================
    // determine_all_events - gestures (native injection)
    // ==================================================================

    fn pinch(scale: f32) -> GestureAndDragManager {
        let mut g = GestureAndDragManager::new();
        g.inject_native_gesture(NativeGestureEvent::Pinch(DetectedPinch {
            scale,
            center: LogicalPosition::new(0.0, 0.0),
            initial_distance: 0.0,
            current_distance: 0.0,
            duration_ms: 0,
        }));
        g
    }

    fn rotation(angle_radians: f32) -> GestureAndDragManager {
        let mut g = GestureAndDragManager::new();
        g.inject_native_gesture(NativeGestureEvent::Rotation(DetectedRotation {
            angle_radians,
            center: LogicalPosition::new(0.0, 0.0),
            duration_ms: 0,
        }));
        g
    }

    #[test]
    fn pinch_scale_below_one_is_pinch_in_at_and_above_one_is_pinch_out() {
        let s = state();
        for (scale, expected) in [
            (0.0_f32, EventType::PinchIn),
            (0.999_999, EventType::PinchIn),
            (f32::NEG_INFINITY, EventType::PinchIn),
            (1.0, EventType::PinchOut),
            (1.000_001, EventType::PinchOut),
            (f32::INFINITY, EventType::PinchOut),
        ] {
            let g = pinch(scale);
            let events = run(&s, &s, &HoverManager::new(), Some(&g), None);
            assert_eq!(count(&events, expected), 1, "scale {scale} -> {expected:?}");
        }
    }

    #[test]
    fn pinch_scale_nan_is_deterministic_and_does_not_panic() {
        // `pinch.scale < 1.0` is false for NaN, so NaN classifies as PinchOut.
        // The point is that it is *deterministic* and never panics.
        let s = state();
        let g = pinch(f32::NAN);
        let events = run(&s, &s, &HoverManager::new(), Some(&g), None);
        assert_eq!(count(&events, EventType::PinchOut), 1);
        assert_eq!(count(&events, EventType::PinchIn), 0);
    }

    #[test]
    fn rotation_sign_selects_the_direction_including_signed_zero_and_nan() {
        let s = state();
        for (angle, expected) in [
            (0.1_f32, EventType::RotateClockwise),
            (f32::INFINITY, EventType::RotateClockwise),
            (-0.1, EventType::RotateCounterClockwise),
            (0.0, EventType::RotateCounterClockwise),
            (-0.0, EventType::RotateCounterClockwise),
            (f32::NAN, EventType::RotateCounterClockwise),
            (f32::NEG_INFINITY, EventType::RotateCounterClockwise),
        ] {
            let g = rotation(angle);
            let events = run(&s, &s, &HoverManager::new(), Some(&g), None);
            assert_eq!(count(&events, expected), 1, "angle {angle} -> {expected:?}");
        }
    }

    #[test]
    fn every_swipe_direction_maps_to_its_own_event_type() {
        let s = state();
        for (dir, expected) in [
            (GestureDirection::Left, EventType::SwipeLeft),
            (GestureDirection::Right, EventType::SwipeRight),
            (GestureDirection::Up, EventType::SwipeUp),
            (GestureDirection::Down, EventType::SwipeDown),
        ] {
            let mut g = GestureAndDragManager::new();
            g.inject_native_gesture(NativeGestureEvent::Swipe(dir));
            let events = run(&s, &s, &HoverManager::new(), Some(&g), None);
            assert_eq!(count(&events, expected), 1, "{dir:?} -> {expected:?}");
            assert_eq!(events.len(), 1);
        }
    }

    #[test]
    fn long_press_is_suppressed_once_the_callback_ran() {
        let s = state();
        for (invoked, expected) in [(false, 1_usize), (true, 0)] {
            let mut g = GestureAndDragManager::new();
            g.inject_native_gesture(NativeGestureEvent::LongPress(DetectedLongPress {
                position: LogicalPosition::new(1.0, 1.0),
                duration_ms: u64::MAX,
                callback_invoked: invoked,
                session_id: 0,
            }));
            let events = run(&s, &s, &HoverManager::new(), Some(&g), None);
            assert_eq!(count(&events, EventType::LongPress), expected);
        }
    }

    #[test]
    fn a_double_click_fires_exactly_one_event() {
        let s = state();
        let mut g = GestureAndDragManager::new();
        g.inject_native_gesture(NativeGestureEvent::DoubleClick);
        let events = run(&s, &s, &HoverManager::new(), Some(&g), None);
        assert_eq!(count(&events, EventType::DoubleClick), 1);
    }

    #[test]
    fn a_fresh_gesture_manager_produces_no_gesture_events() {
        let s = state();
        let g = GestureAndDragManager::new();
        let events = run(&s, &s, &HoverManager::new(), Some(&g), None);
        assert!(events.is_empty(), "got {events:?}");
    }

    #[test]
    fn gesture_events_target_the_hovered_node() {
        let s = state();
        let hm = hover_with(hits(&[(3, 12)]), hits(&[(3, 12)]));
        let g = pinch(0.5);
        let events = run(&s, &s, &hm, Some(&g), None);
        let e = only(&events, EventType::PinchIn);
        assert_eq!(e.target, node(3, 12));
    }

    // ==================================================================
    // determine_all_events - pen / stylus diffing
    // ==================================================================

    #[test]
    fn pen_events_require_the_pending_flag() {
        let s = state();
        let mut g = GestureAndDragManager::new();
        g.update_pen_state(LogicalPosition::new(1.0, 1.0), 0.5, (0.0, 0.0), true, false, false, 1);
        g.clear_pen_event_pending();

        let events = run(&s, &s, &HoverManager::new(), Some(&g), None);
        assert_eq!(count(&events, EventType::PenEnter), 0);
        assert_eq!(count(&events, EventType::PenDown), 0);
    }

    #[test]
    fn pen_entering_proximity_emits_pen_enter() {
        let s = state();
        let mut g = GestureAndDragManager::new();
        g.update_pen_state(LogicalPosition::new(1.0, 1.0), 0.0, (0.0, 0.0), false, false, false, 1);
        let events = run(&s, &s, &HoverManager::new(), Some(&g), None);
        assert_eq!(count(&events, EventType::PenEnter), 1);
        assert_eq!(count(&events, EventType::PenLeave), 0);
    }

    #[test]
    fn pen_leaving_proximity_emits_pen_leave() {
        let s = state();
        let mut g = GestureAndDragManager::new();
        g.update_pen_state(LogicalPosition::new(1.0, 1.0), 0.0, (0.0, 0.0), false, false, false, 1);
        g.clear_pen_state();
        let events = run(&s, &s, &HoverManager::new(), Some(&g), None);
        assert_eq!(count(&events, EventType::PenLeave), 1);
        assert_eq!(count(&events, EventType::PenEnter), 0);
    }

    #[test]
    fn pen_contact_transitions_emit_pen_down_then_pen_up() {
        let s = state();
        let mut g = GestureAndDragManager::new();
        g.update_pen_state(LogicalPosition::new(1.0, 1.0), 0.0, (0.0, 0.0), false, false, false, 1);
        g.update_pen_state(LogicalPosition::new(1.0, 1.0), 1.0, (0.0, 0.0), true, false, false, 1);
        let down = run(&s, &s, &HoverManager::new(), Some(&g), None);
        assert_eq!(count(&down, EventType::PenDown), 1);
        assert_eq!(count(&down, EventType::PenMove), 0, "position did not change");

        g.update_pen_state(LogicalPosition::new(1.0, 1.0), 0.0, (0.0, 0.0), false, false, false, 1);
        let up = run(&s, &s, &HoverManager::new(), Some(&g), None);
        assert_eq!(count(&up, EventType::PenUp), 1);
        assert_eq!(count(&up, EventType::PenDown), 0);
    }

    #[test]
    fn pen_movement_emits_pen_move() {
        let s = state();
        let mut g = GestureAndDragManager::new();
        g.update_pen_state(LogicalPosition::new(1.0, 1.0), 1.0, (0.0, 0.0), true, false, false, 1);
        g.update_pen_state(LogicalPosition::new(40.0, 90.0), 1.0, (0.0, 0.0), true, false, false, 1);
        let events = run(&s, &s, &HoverManager::new(), Some(&g), None);
        assert_eq!(count(&events, EventType::PenMove), 1);
    }

    #[test]
    fn a_pen_stuck_at_nan_does_not_emit_a_phantom_pen_move() {
        // LogicalPosition::eq quantizes NaN to a single sentinel, so NaN == NaN
        // and a stuck-at-NaN pen must stay quiet instead of firing every frame.
        let s = state();
        let mut g = GestureAndDragManager::new();
        let nan = LogicalPosition::new(f32::NAN, f32::NAN);
        g.update_pen_state(nan, 1.0, (f32::NAN, f32::NAN), true, false, false, 1);
        g.update_pen_state(nan, 1.0, (f32::NAN, f32::NAN), true, false, false, 1);
        let events = run(&s, &s, &HoverManager::new(), Some(&g), None);
        assert_eq!(count(&events, EventType::PenMove), 0);
        assert!(events.is_empty(), "got {events:?}");
    }

    #[test]
    fn a_pen_in_contact_emits_touch_move_when_the_cursor_moves() {
        let mut g = GestureAndDragManager::new();
        g.update_pen_state(LogicalPosition::new(1.0, 1.0), 1.0, (0.0, 0.0), true, false, false, 1);
        g.clear_pen_event_pending(); // isolate the TouchMove path from PenEnter

        let events = run(
            &cursor_at(50.0, 50.0),
            &cursor_at(10.0, 10.0),
            &HoverManager::new(),
            Some(&g),
            None,
        );
        assert_eq!(count(&events, EventType::TouchMove), 1);
    }

    // ==================================================================
    // determine_all_events - manager providers + dedup
    // ==================================================================

    #[test]
    fn provider_events_are_appended_and_deduplicated() {
        let s = state();
        let hover = HoverManager::new();
        let focus = FocusManager::new();
        let file_drop = FileDropManager::new();
        let target = node(0, 42);
        let provider = StaticProvider(vec![
            ev(EventType::Scroll, target, 2),
            ev(EventType::Scroll, target, 8),
            ev(EventType::KeyDown, target, 1),
        ]);
        let providers: Vec<&dyn EventProvider> = vec![&provider];

        let events = determine_all_events(
            &s,
            &s,
            &hover,
            &focus,
            &file_drop,
            None,
            &providers,
            None,
            ts(0),
        );
        assert_eq!(events.len(), 2);
        assert_eq!(only(&events, EventType::Scroll).timestamp, ts(8));
    }

    #[test]
    fn a_provider_can_collide_with_a_wheel_scroll_on_the_same_node() {
        let s = state();
        let hover = HoverManager::new();
        let focus = FocusManager::new();
        let file_drop = FileDropManager::new();
        // Same (root, Scroll) slot as the wheel-generated event.
        let provider = StaticProvider(vec![ev(EventType::Scroll, root(), u64::MAX)]);
        let providers: Vec<&dyn EventProvider> = vec![&provider];

        let events = determine_all_events(
            &s,
            &s,
            &hover,
            &focus,
            &file_drop,
            None,
            &providers,
            Some(LogicalPosition::new(0.0, -1.0)),
            ts(0),
        );
        assert_eq!(count(&events, EventType::Scroll), 1);
        assert_eq!(
            events[0].timestamp,
            ts(u64::MAX),
            "dedup keeps the later timestamp"
        );
    }

    #[test]
    fn a_completely_idle_frame_produces_nothing() {
        let s = state();
        let g = GestureAndDragManager::new();
        let hover = HoverManager::new();
        let focus = FocusManager::new();
        let file_drop = FileDropManager::new();
        let empty = StaticProvider(Vec::new());
        let providers: Vec<&dyn EventProvider> = vec![&empty];
        let events = determine_all_events(
            &s,
            &s,
            &hover,
            &focus,
            &file_drop,
            Some(&g),
            &providers,
            None,
            ts(0),
        );
        assert!(events.is_empty(), "got {events:?}");
    }
}
