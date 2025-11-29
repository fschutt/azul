//! Unified Event Determination
//!
//! This module provides the single source of truth for what 
//! events occurred in a frame. It combines window state changes 
//! with events from all managers (scroll, text input, etc.).

use azul_core::{
    dom::{DomId, DomNodeId},
    events::{
        deduplicate_synthetic_events, EventData, EventProvider, EventSource, EventType,
        SyntheticEvent, WindowEventData,
    },
    geom::{LogicalPosition, LogicalRect},
    id::NodeId,
    styled_dom::NodeHierarchyItemId,
    task::{Instant, SystemTick},
    window::{CursorPosition, WindowPosition},
};
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
            WindowPosition::Initialized(phys_pos) => Some(LogicalPosition {
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

/// Comprehensive event determination including mouse, keyboard, and gesture events.
///
/// This is the full replacement for `create_events_from_states_with_gestures()`.
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
    timestamp: Instant,
) -> Vec<SyntheticEvent> {

    let mut events = Vec::new();

    // Get root node for window-level events
    let root_node = DomNodeId {
        dom: DomId { inner: 0 },
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
    };

    // Mouse Button Events

    let current_mouse_down = current_state.mouse_state.mouse_down();
    let previous_mouse_down = previous_state.mouse_state.mouse_down();

    // Left mouse button
    if current_state.mouse_state.left_down && !previous_state.mouse_state.left_down {
        events.push(SyntheticEvent::new(
            EventType::MouseDown,
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }
    if !current_state.mouse_state.left_down && previous_state.mouse_state.left_down {
        events.push(SyntheticEvent::new(
            EventType::MouseUp,
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Right mouse button
    if current_state.mouse_state.right_down && !previous_state.mouse_state.right_down {
        events.push(SyntheticEvent::new(
            EventType::MouseDown, // Use generic MouseDown for now
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }
    if !current_state.mouse_state.right_down && previous_state.mouse_state.right_down {
        events.push(SyntheticEvent::new(
            EventType::MouseUp, // Use generic MouseUp for now
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Middle mouse button
    if current_state.mouse_state.middle_down && !previous_state.mouse_state.middle_down {
        events.push(SyntheticEvent::new(
            EventType::MouseDown, // Use generic MouseDown for now
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }
    if !current_state.mouse_state.middle_down && previous_state.mouse_state.middle_down {
        events.push(SyntheticEvent::new(
            EventType::MouseUp, // Use generic MouseUp for now
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Mouse Movement Events

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

        // MouseOver fires on ANY mouse movement
        if current_pos != previous_pos {
            events.push(SyntheticEvent::new(
                EventType::MouseOver,
                EventSource::User,
                root_node.clone(),
                timestamp.clone(),
                EventData::None,
            ));
        }

        // MouseEnter (cursor entered window)
        if !previous_in_window {
            events.push(SyntheticEvent::new(
                EventType::MouseEnter,
                EventSource::User,
                root_node.clone(),
                timestamp.clone(),
                EventData::None,
            ));
        }
    }

    // MouseLeave (cursor left window)
    if previous_in_window && !current_in_window {
        events.push(SyntheticEvent::new(
            EventType::MouseLeave,
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Keyboard Events

    let current_key = current_state
        .keyboard_state
        .current_virtual_keycode
        .into_option();
    let previous_key = previous_state
        .keyboard_state
        .current_virtual_keycode
        .into_option();

    if current_key.is_some() && current_key != previous_key {
        events.push(SyntheticEvent::new(
            EventType::KeyDown,
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }
    if previous_key.is_some() && current_key.is_none() {
        events.push(SyntheticEvent::new(
            EventType::KeyUp,
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
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

    // File Drop Events

    if let Some(_hovered_file) = file_drop_manager.get_hovered_file() {
        events.push(SyntheticEvent::new(
            EventType::FileHover,
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }

    if file_drop_manager.dropped_file.is_some() {
        events.push(SyntheticEvent::new(
            EventType::FileDrop,
            EventSource::User,
            root_node.clone(),
            timestamp.clone(),
            EventData::None,
        ));
    }

    // Gesture Events

    if let Some(manager) = gesture_manager {
        let event_was_mouse_release = !current_mouse_down && previous_mouse_down;

        // Detect DragStart
        if let Some(_detected_drag) = manager.detect_drag() {
            if !manager.is_dragging() {
                events.push(SyntheticEvent::new(
                    EventType::DragStart,
                    EventSource::User,
                    root_node.clone(),
                    timestamp.clone(),
                    EventData::None,
                ));
            }
        }

        // Detect Drag (continuous movement)
        if manager.is_dragging() && current_mouse_down {
            let current_pos = current_state.mouse_state.cursor_position.get_position();
            let previous_pos = previous_state.mouse_state.cursor_position.get_position();

            if current_pos != previous_pos {
                events.push(SyntheticEvent::new(
                    EventType::Drag,
                    EventSource::User,
                    root_node.clone(),
                    timestamp.clone(),
                    EventData::None,
                ));
            }
        }

        // Detect DragEnd
        if manager.is_dragging() && event_was_mouse_release {
            events.push(SyntheticEvent::new(
                EventType::DragEnd,
                EventSource::User,
                root_node.clone(),
                timestamp.clone(),
                EventData::None,
            ));
        }

        // Detect DoubleClick
        if manager.detect_double_click() {
            events.push(SyntheticEvent::new(
                EventType::DoubleClick,
                EventSource::User,
                root_node.clone(),
                timestamp.clone(),
                EventData::None,
            ));
        }

        // Detect LongPress
        if let Some(long_press) = manager.detect_long_press() {
            if !long_press.callback_invoked {
                events.push(SyntheticEvent::new(
                    EventType::LongPress,
                    EventSource::User,
                    root_node.clone(),
                    timestamp.clone(),
                    EventData::None,
                ));
            }
        }

        // Detect Swipe gestures
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
                root_node.clone(),
                timestamp.clone(),
                EventData::None,
            ));
        }

        // Detect Pinch gestures
        if let Some(pinch) = manager.detect_pinch() {
            let event_type = if pinch.scale < 1.0 {
                EventType::PinchIn
            } else {
                EventType::PinchOut
            };
            events.push(SyntheticEvent::new(
                event_type,
                EventSource::User,
                root_node.clone(),
                timestamp.clone(),
                EventData::None,
            ));
        }

        // Detect Rotation gestures
        if let Some(rotation) = manager.detect_rotation() {
            let event_type = if rotation.angle_radians > 0.0 {
                EventType::RotateClockwise
            } else {
                EventType::RotateCounterClockwise
            };
            events.push(SyntheticEvent::new(
                event_type,
                EventSource::User,
                root_node.clone(),
                timestamp.clone(),
                EventData::None,
            ));
        }

        // Detect Pen events (not in EventType yet, use TouchMove for now)
        if let Some(pen_state) = manager.get_pen_state() {
            if pen_state.in_contact {
                let current_pos = current_state.mouse_state.cursor_position.get_position();
                let previous_pos = previous_state.mouse_state.cursor_position.get_position();

                if current_pos != previous_pos {
                    events.push(SyntheticEvent::new(
                        EventType::TouchMove, // Use TouchMove as fallback
                        EventSource::User,
                        root_node.clone(),
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
