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
/// - Vector of SyntheticEvents, deduplicated and ready for dispatch.
pub fn determine_events_from_managers<'a>(
    current_state: &FullWindowState,
    previous_state: &FullWindowState,
    managers: &[&'a dyn EventProvider],
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
                    size: current.size.dimensions.clone(),
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
            timestamp.clone(),
            EventData::None,
        ));
    }

    events
}

/// Get all hovered node IDs from the hover manager for a given frame.
///
/// frame_index 0 = current frame, 1 = previous frame, etc.
/// Returns a BTreeSet of all NodeIds that are hovered (the full hover chain).
fn get_all_hovered_nodes(
    hover_manager: &crate::managers::hover::HoverManager,
    frame_index: usize,
) -> BTreeSet<NodeId> {
    use crate::managers::hover::InputPointId;
    let dom_id = DomId { inner: 0 };
    hover_manager
        .get_frame(&InputPointId::Mouse, frame_index)
        .and_then(|ht| ht.hovered_nodes.get(&dom_id))
        .map(|ht| ht.regular_hit_test_nodes.keys().copied().collect())
        .unwrap_or_default()
}

/// Comprehensive event determination including mouse, keyboard, and gesture events.
///
/// This is the primary event determination function.
/// It generates SyntheticEvents for all event types including:
///
/// - Mouse button events (down/up for left/right/middle)
/// - Mouse movement (MouseOver)
/// - Keyboard events (VirtualKeyDown/Up)
/// - Window state changes (resize, move, theme, focus)
/// - Gesture events (DragStart, Drag, DragEnd, DoubleClick, LongPress, Swipe, Pinch, Rotate, Pen)
/// - File drop events (HoveredFile, DroppedFile)
///
/// ## Arguments
///
/// * `current_state` - Current window state
/// * `previous_state` - Previous window state
/// * `hover_manager` - For hover/mouse enter/leave detection
/// * `focus_manager` - For focus event detection
/// * `file_drop_manager` - For file drop detection
/// * `gesture_manager` - Optional gesture detection
/// * `managers` - Additional managers (scroll, text, etc.) that implement EventProvider
/// * `timestamp` - Current time
///
/// ## Returns
///
/// - Deduplicated vector of SyntheticEvents ready for dispatch
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
    let buttons: u8 = (if current_state.mouse_state.left_down { 1 } else { 0 })
        | (if current_state.mouse_state.right_down { 2 } else { 0 })
        | (if current_state.mouse_state.middle_down { 4 } else { 0 });

    // Helper: get deepest hovered node as the event target for mouse events.
    // Multi-DOM aware: targets VirtualView / iframe child DOMs (higher DomId,
    // composited on top) when the cursor is over them, not just the root DOM.
    // For single-DOM apps only DomId 0 is ever hit, so this is unchanged.
    let mouse_target = hover_manager
        .current_hover_node_full()
        .unwrap_or(root_node.clone());

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
                mouse_target.clone(),
                timestamp.clone(),
                make_mouse_data(button),
            ));
        }
        if !curr_down && prev_down {
            events.push(SyntheticEvent::new(
                EventType::MouseUp,
                EventSource::User,
                mouse_target.clone(),
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
                mouse_target.clone(),
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
                mouse_target.clone(),
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
                mouse_target.clone(),
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
        for node_id in previous_hovered.difference(&current_hovered) {
            events.push(SyntheticEvent::new(
                EventType::MouseLeave,
                EventSource::User,
                DomNodeId {
                    dom: root_node.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                },
                timestamp.clone(),
                EventData::None,
            ));
        }

        // Nodes that gained hover -> MouseEnter
        for node_id in current_hovered.difference(&previous_hovered) {
            events.push(SyntheticEvent::new(
                EventType::MouseEnter,
                EventSource::User,
                DomNodeId {
                    dom: root_node.dom,
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
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }
    if !current_in_window && previous_in_window {
        events.push(SyntheticEvent::new(
            EventType::MouseLeave,
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Keyboard Events
    // W3C: Keyboard events target the focused element, falling back to root

    let focus_target = focus_manager
        .get_focused_node()
        .cloned()
        .unwrap_or(root_node.clone());

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
        key_code: current_key.map(|k| k as u32).unwrap_or(0),
        char_code: None, // Character is available from the keyboard state
        modifiers,
        repeat: false,
    });

    if current_key.is_some() && (current_key != previous_key || previous_key.is_none()) {
        events.push(SyntheticEvent::new(
            EventType::KeyDown,
            EventSource::User,
            focus_target.clone(),
            timestamp.clone(),
            keyboard_data.clone(),
        ));
    }
    let key_up_data = EventData::Keyboard(KeyboardEventData {
        key_code: previous_key.map(|k| k as u32).unwrap_or(0),
        char_code: None,
        modifiers,
        repeat: false,
    });

    if previous_key.is_some() && current_key.is_none() {
        events.push(SyntheticEvent::new(
            EventType::KeyUp,
            EventSource::User,
            focus_target.clone(),
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
            root_node.clone(),
            timestamp.clone(),
            EventData::Window(WindowEventData {
                size: Some(LogicalRect {
                    origin: LogicalPosition { x: 0.0, y: 0.0 },
                    size: current_state.size.dimensions.clone(),
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
                root_node.clone(),
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
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Window focus changed
    if current_state.window_focused && !previous_state.window_focused {
        events.push(SyntheticEvent::new(
            EventType::WindowFocusIn,
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }
    if !current_state.window_focused && previous_state.window_focused {
        events.push(SyntheticEvent::new(
            EventType::WindowFocusOut,
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Theme changed
    if current_state.theme != previous_state.theme {
        events.push(SyntheticEvent::new(
            EventType::ThemeChange,
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }

    // DPI changed (moved to a different-DPI monitor, or system DPI setting changed)
    if current_state.size.dpi != previous_state.size.dpi {
        events.push(SyntheticEvent::new(
            EventType::WindowDpiChanged,
            EventSource::User,
            root_node.clone(),
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
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }

    // File Drop Events

    if file_drop_manager.get_hovered_file().is_some() {
        events.push(SyntheticEvent::new(
            EventType::FileHover,
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }

    if file_drop_manager.get_dropped_file().is_some() {
        events.push(SyntheticEvent::new(
            EventType::FileDrop,
            EventSource::User,
            root_node.clone(),
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
                    mouse_target.clone(),
                    timestamp.clone(),
                    pen_data.clone(),
                )),
                (Some(_), None) => events.push(SyntheticEvent::new(
                    EventType::PenLeave,
                    EventSource::User,
                    mouse_target.clone(),
                    timestamp.clone(),
                    pen_data.clone(),
                )),
                (Some(p), Some(c)) => {
                    if !p.in_contact && c.in_contact {
                        events.push(SyntheticEvent::new(
                            EventType::PenDown,
                            EventSource::User,
                            mouse_target.clone(),
                            timestamp.clone(),
                            pen_data.clone(),
                        ));
                    } else if p.in_contact && !c.in_contact {
                        events.push(SyntheticEvent::new(
                            EventType::PenUp,
                            EventSource::User,
                            mouse_target.clone(),
                            timestamp.clone(),
                            pen_data.clone(),
                        ));
                    }
                    if p.position != c.position {
                        events.push(SyntheticEvent::new(
                            EventType::PenMove,
                            EventSource::User,
                            mouse_target.clone(),
                            timestamp.clone(),
                            pen_data.clone(),
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
        if manager.detect_drag().is_some() {
            if !manager.is_dragging() {
                events.push(SyntheticEvent::new(
                    EventType::DragStart,
                    EventSource::User,
                    mouse_target.clone(),
                    timestamp.clone(),
                    make_mouse_data(MouseButton::Left),
                ));
            }
        }

        // Detect Drag (continuous movement, targeted at hovered node)
        if manager.is_dragging() && current_mouse_down {
            let current_pos = current_state.mouse_state.cursor_position.get_position();
            let previous_pos = previous_state.mouse_state.cursor_position.get_position();

            if current_pos != previous_pos {
                events.push(SyntheticEvent::new(
                    EventType::Drag,
                    EventSource::User,
                    mouse_target.clone(),
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
                mouse_target.clone(),
                timestamp.clone(),
                make_mouse_data(MouseButton::Left),
            ));

            // When mouse is released during a node drag, generate a Drop event
            // on the current drop target (the node under the cursor)
            if manager.is_node_drag_active() {
                events.push(SyntheticEvent::new(
                    EventType::Drop,
                    EventSource::User,
                    mouse_target.clone(), // W3C: Drop targets the node under cursor
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
                mouse_target.clone(),
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
                    mouse_target.clone(),
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
                mouse_target.clone(),
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
                mouse_target.clone(),
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
                mouse_target.clone(),
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
                        mouse_target.clone(),
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

    fn default_state() -> crate::window_state::FullWindowState {
        crate::window_state::FullWindowState::default()
    }

    fn state_with_key(vk: VirtualKeyCode) -> crate::window_state::FullWindowState {
        let mut s = default_state();
        s.keyboard_state.current_virtual_keycode = OptionVirtualKeyCode::Some(vk);
        s.keyboard_state.pressed_virtual_keycodes = VirtualKeyCodeVec::from_vec(vec![vk]);
        s
    }

    fn state_with_left_down(x: f32, y: f32) -> crate::window_state::FullWindowState {
        let mut s = default_state();
        s.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(x, y));
        s.mouse_state.left_down = true;
        s
    }

    fn state_with_cursor(x: f32, y: f32) -> crate::window_state::FullWindowState {
        let mut s = default_state();
        s.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(x, y));
        s
    }

    fn empty_providers() -> Vec<&'static dyn EventProvider> { vec![] }

    fn run_determine(
        current: &crate::window_state::FullWindowState,
        previous: &crate::window_state::FullWindowState,
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
        let kd: Vec<_> = events.iter().filter(|e| e.event_type == EventType::KeyDown).collect();
        assert_eq!(kd.len(), 0, "KeyDown should not fire when same key held");
    }

    #[test]
    fn keydown_fires_for_different_key() {
        let events = run_determine(
            &state_with_key(VirtualKeyCode::B),
            &state_with_key(VirtualKeyCode::A),
        );
        let kd: Vec<_> = events.iter().filter(|e| e.event_type == EventType::KeyDown).collect();
        assert_eq!(kd.len(), 1);
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
            other => panic!("Expected Keyboard data, got {:?}", other),
        }
    }

    // === Mouse tests ===

    #[test]
    fn mousedown_fires_on_left_press() {
        let events = run_determine(
            &state_with_left_down(100.0, 200.0),
            &state_with_cursor(100.0, 200.0),
        );
        let md: Vec<_> = events.iter().filter(|e| e.event_type == EventType::MouseDown).collect();
        assert_eq!(md.len(), 1);
    }

    #[test]
    fn mouseup_fires_on_left_release() {
        let events = run_determine(
            &state_with_cursor(100.0, 200.0),
            &state_with_left_down(100.0, 200.0),
        );
        let mu: Vec<_> = events.iter().filter(|e| e.event_type == EventType::MouseUp).collect();
        assert_eq!(mu.len(), 1);
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
        let mo: Vec<_> = events.iter().filter(|e| e.event_type == EventType::MouseOver).collect();
        assert_eq!(mo.len(), 1);
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
        let kd: Vec<_> = events.iter().filter(|e| e.event_type == EventType::KeyDown).collect();
        assert_eq!(kd.len(), 1, "Key repeat should fire KeyDown when previous is cleared");
    }

    #[test]
    fn key_repeat_skipped_when_previous_not_cleared() {
        // Without the platform fix: both previous and current have Same(Left).
        // KeyDown should NOT fire (this documents the limitation that the
        // platform layer MUST clear previous for repeats).
        let previous = state_with_key(VirtualKeyCode::Left);
        let current = state_with_key(VirtualKeyCode::Left);

        let events = run_determine(&current, &previous);
        let kd: Vec<_> = events.iter().filter(|e| e.event_type == EventType::KeyDown).collect();
        assert_eq!(kd.len(), 0, "Without platform clearing, repeat is not detected");
    }
}
