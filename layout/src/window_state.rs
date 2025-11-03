//! Window state types for azul-layout
//!
//! These types are defined here (rather than azul-core) because CallbackInfo
//! needs to reference them, and CallbackInfo must live in azul-layout (since
//! it requires LayoutWindow).

use alloc::collections::BTreeMap;

use azul_core::{
    callbacks::LayoutCallback,
    dom::DomId,
    window::{
        DebugState, ImePosition, KeyboardState, Monitor, MouseState, PlatformSpecificOptions,
        RendererOptions, TouchState, WindowFlags, WindowPosition, WindowSize, WindowTheme,
    },
};
use azul_css::{impl_option, impl_option_inner, props::basic::color::ColorU, AzString};

use crate::callbacks::OptionCallback;

/// Window state that can be modified by callbacks
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WindowState {
    pub title: AzString,
    pub theme: WindowTheme,
    pub size: WindowSize,
    pub position: WindowPosition,
    pub flags: WindowFlags,
    pub debug_state: DebugState,
    pub keyboard_state: KeyboardState,
    pub mouse_state: MouseState,
    pub touch_state: TouchState,
    pub ime_position: ImePosition,
    pub monitor: Monitor,
    pub platform_specific_options: PlatformSpecificOptions,
    pub renderer_options: RendererOptions,
    pub background_color: ColorU,
    pub layout_callback: LayoutCallback,
    pub close_callback: OptionCallback,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            title: AzString::from_const_str("Azul Window"),
            theme: WindowTheme::default(),
            size: WindowSize::default(),
            position: WindowPosition::default(),
            flags: WindowFlags::default(),
            debug_state: DebugState::default(),
            keyboard_state: KeyboardState::default(),
            mouse_state: MouseState::default(),
            touch_state: TouchState::default(),
            ime_position: ImePosition::default(),
            monitor: Monitor::default(),
            platform_specific_options: PlatformSpecificOptions::default(),
            renderer_options: RendererOptions::default(),
            background_color: ColorU::WHITE,
            layout_callback: LayoutCallback::default(),
            close_callback: OptionCallback::None,
        }
    }
}

impl_option!(
    WindowState,
    OptionWindowState,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// Options for creating a new window
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WindowCreateOptions {
    pub state: WindowState,
    pub size_to_content: bool,
    pub renderer: azul_core::window::OptionRendererOptions,
    pub theme: azul_core::window::OptionWindowTheme,
    pub create_callback: OptionCallback,
    pub hot_reload: bool,
}

impl Default for WindowCreateOptions {
    fn default() -> Self {
        Self {
            state: WindowState::default(),
            size_to_content: false,
            renderer: azul_core::window::OptionRendererOptions::None,
            theme: azul_core::window::OptionWindowTheme::None,
            create_callback: OptionCallback::None,
            hot_reload: false,
        }
    }
}

impl WindowCreateOptions {
    /// Create a new WindowCreateOptions with a layout callback
    pub fn new(layout_callback: azul_core::callbacks::LayoutCallbackType) -> Self {
        let mut options = Self::default();
        options.state.layout_callback =
            azul_core::callbacks::LayoutCallback::Raw(azul_core::callbacks::LayoutCallbackInner {
                cb: layout_callback,
            });
        options
    }
}

/// Full window state including internal fields not exposed to callbacks
#[derive(Debug, Clone, PartialEq)]
pub struct FullWindowState {
    pub theme: WindowTheme,
    pub title: AzString,
    pub size: WindowSize,
    pub position: WindowPosition,
    pub flags: WindowFlags,
    pub debug_state: DebugState,
    pub keyboard_state: KeyboardState,
    pub mouse_state: MouseState,
    pub touch_state: TouchState,
    pub ime_position: ImePosition,
    pub platform_specific_options: PlatformSpecificOptions,
    pub renderer_options: RendererOptions,
    pub background_color: ColorU,
    pub layout_callback: LayoutCallback,
    pub close_callback: OptionCallback,
    /// Monitor ID (not the full Monitor struct - just the identifier)
    pub monitor_id: Option<u32>,
    pub window_focused: bool,
}

impl Default for FullWindowState {
    fn default() -> Self {
        Self {
            theme: WindowTheme::default(),
            title: AzString::from_const_str("Azul Window"),
            size: WindowSize::default(),
            position: WindowPosition::default(),
            flags: WindowFlags::default(),
            debug_state: DebugState::default(),
            keyboard_state: KeyboardState::default(),
            mouse_state: MouseState::default(),
            touch_state: TouchState::default(),
            ime_position: ImePosition::default(),
            platform_specific_options: PlatformSpecificOptions::default(),
            background_color: ColorU::WHITE,
            layout_callback: LayoutCallback::default(),
            close_callback: OptionCallback::None,
            renderer_options: RendererOptions::default(),
            monitor_id: None,
            window_focused: true,
        }
    }
}

impl FullWindowState {
    /// Convert FullWindowState to WindowState (for PlatformWindow trait)
    pub fn to_window_state(&self) -> WindowState {
        self.clone().into()
    }
}

impl From<FullWindowState> for WindowState {
    fn from(full: FullWindowState) -> Self {
        Self {
            title: full.title,
            theme: full.theme,
            size: full.size,
            position: full.position,
            flags: full.flags,
            debug_state: full.debug_state,
            keyboard_state: full.keyboard_state,
            mouse_state: full.mouse_state,
            touch_state: full.touch_state,
            ime_position: full.ime_position,
            monitor: Monitor::default(), /* Monitor info needs to be looked up from platform via
                                          * monitor_id */
            platform_specific_options: full.platform_specific_options,
            renderer_options: full.renderer_options,
            background_color: full.background_color,
            layout_callback: full.layout_callback,
            close_callback: full.close_callback,
        }
    }
}

/* DEPRECATED: Replaced by determine_events_from_managers() in event_determination.rs
/// Create Events by comparing current and previous window states.
/// This is the cross-platform event detection layer.
///
/// Requires the managers for complete event detection (focus, hover files, drag-drop, gestures).
pub fn create_events_from_states(
    current_state: &FullWindowState,
    previous_state: &FullWindowState,
    focus_manager: &crate::managers::focus_cursor::FocusManager,
    previous_focus: Option<&crate::managers::focus_cursor::FocusManager>,
    file_drop_manager: &crate::managers::file_drop::FileDropManager,
    previous_file_drop: Option<&crate::managers::file_drop::FileDropManager>,
    hover_manager: &crate::managers::hover::HoverManager,
) -> azul_core::events::Events {
    create_events_from_states_with_gestures(
        current_state,
        previous_state,
        focus_manager,
        previous_focus,
        file_drop_manager,
        previous_file_drop,
        hover_manager,
        None,
        None, // No scroll manager
    )
}

/// Create Events with gesture detection and scroll manager support.
///
/// This version takes managers and an optional `GestureAndDragManager` to detect
/// multi-frame gestures that can't be detected from state diffing alone.
///
/// The `scroll_manager` is used to detect scroll events by querying scroll deltas
/// instead of diffing MouseState fields (which no longer contain scroll_x/scroll_y).
pub fn create_events_from_states_with_gestures(
    current_state: &FullWindowState,
    previous_state: &FullWindowState,
    focus_manager: &crate::managers::focus_cursor::FocusManager,
    previous_focus: Option<&crate::managers::focus_cursor::FocusManager>,
    file_drop_manager: &crate::managers::file_drop::FileDropManager,
    previous_file_drop: Option<&crate::managers::file_drop::FileDropManager>,
    hover_manager: &crate::managers::hover::HoverManager,
    gesture_manager: Option<&crate::managers::gesture::GestureAndDragManager>,
    scroll_manager: Option<&crate::managers::scroll_state::ScrollManager>,
) -> azul_core::events::Events {
    use azul_core::{
        events::{Events, FocusEventFilter, HoverEventFilter, WindowEventFilter},
        window::CursorPosition,
    };

    let mut window_events = Vec::new();
    let mut hover_events = Vec::new();
    let mut focus_events = Vec::new();

    // Mouse state changes
    let current_mouse_down = current_state.mouse_state.mouse_down();
    let previous_mouse_down = previous_state.mouse_state.mouse_down();

    let event_was_mouse_down = current_mouse_down && !previous_mouse_down;
    let event_was_mouse_release = !current_mouse_down && previous_mouse_down;

    // Mouse button events
    if current_state.mouse_state.left_down && !previous_state.mouse_state.left_down {
        window_events.push(WindowEventFilter::LeftMouseDown);
        hover_events.push(HoverEventFilter::LeftMouseDown);
    }
    if !current_state.mouse_state.left_down && previous_state.mouse_state.left_down {
        window_events.push(WindowEventFilter::LeftMouseUp);
        hover_events.push(HoverEventFilter::LeftMouseUp);
    }

    if current_state.mouse_state.right_down && !previous_state.mouse_state.right_down {
        window_events.push(WindowEventFilter::RightMouseDown);
        hover_events.push(HoverEventFilter::RightMouseDown);
    }
    if !current_state.mouse_state.right_down && previous_state.mouse_state.right_down {
        window_events.push(WindowEventFilter::RightMouseUp);
        hover_events.push(HoverEventFilter::RightMouseUp);
    }

    if current_state.mouse_state.middle_down && !previous_state.mouse_state.middle_down {
        window_events.push(WindowEventFilter::MiddleMouseDown);
        hover_events.push(HoverEventFilter::MiddleMouseDown);
    }
    if !current_state.mouse_state.middle_down && previous_state.mouse_state.middle_down {
        window_events.push(WindowEventFilter::MiddleMouseUp);
        hover_events.push(HoverEventFilter::MiddleMouseUp);
    }

    // Mouse position changes (MouseOver = continuous move)
    let current_in_window = matches!(
        current_state.mouse_state.cursor_position,
        CursorPosition::InWindow(_)
    );
    let previous_in_window = matches!(
        previous_state.mouse_state.cursor_position,
        CursorPosition::InWindow(_)
    );

    let event_was_mouse_leave = previous_in_window && !current_in_window;

    if current_in_window {
        let current_pos = current_state.mouse_state.cursor_position.get_position();
        let previous_pos = previous_state.mouse_state.cursor_position.get_position();

        // MouseOver fires on ANY mouse movement (even on same node)
        if current_pos != previous_pos {
            window_events.push(WindowEventFilter::MouseOver);
            hover_events.push(HoverEventFilter::MouseOver);
        }

        // Window-level MouseEnter (cursor entered window)
        if !previous_in_window {
            window_events.push(WindowEventFilter::MouseEnter);
        }
    }

    // Window-level MouseLeave (cursor left window)
    if event_was_mouse_leave {
        window_events.push(WindowEventFilter::MouseLeave);
    }

    // Scroll events are now detected by querying the ScrollManager
    // The ScrollManager tracks scroll deltas per node, set by record_sample()
    // during event processing (before this function is called)
    if let Some(sm) = scroll_manager {
        // Check if ANY node had scroll activity this frame
        // This generates window-level and hover-level scroll events
        let had_any_scroll = sm.end_frame().had_scroll_activity;
        
        if had_any_scroll {
            window_events.push(WindowEventFilter::Scroll);
            hover_events.push(HoverEventFilter::Scroll);
        }
    }

    // Keyboard events (VirtualKeyDown/Up)
    let current_key = current_state
        .keyboard_state
        .current_virtual_keycode
        .into_option();
    let previous_key = previous_state
        .keyboard_state
        .current_virtual_keycode
        .into_option();

    if current_key.is_some() && current_key != previous_key {
        window_events.push(WindowEventFilter::VirtualKeyDown);
        hover_events.push(HoverEventFilter::VirtualKeyDown);
        focus_events.push(FocusEventFilter::VirtualKeyDown);
    }
    if previous_key.is_some() && current_key.is_none() {
        window_events.push(WindowEventFilter::VirtualKeyUp);
        hover_events.push(HoverEventFilter::VirtualKeyUp);
        focus_events.push(FocusEventFilter::VirtualKeyUp);
    }

    // TextInput events are now handled by LayoutWindow::process_text_input()
    // which is called directly from event_v2.rs when the platform provides text input.
    // The platform layer receives IME-composed text and calls process_text_input(text_input: &str).
    // This approach is simpler and more compatible with complex input methods (IME, accents, etc.)

    // Window resize
    if current_state.size.dimensions != previous_state.size.dimensions {
        window_events.push(WindowEventFilter::Resized);
    }

    // Window moved
    if current_state.position != previous_state.position {
        window_events.push(WindowEventFilter::Moved);
    }

    // Window close requested
    if current_state.flags.close_requested && !previous_state.flags.close_requested {
        window_events.push(WindowEventFilter::CloseRequested);
    }

    // Window focus changed
    if current_state.window_focused && !previous_state.window_focused {
        window_events.push(WindowEventFilter::WindowFocusReceived);
    }
    if !current_state.window_focused && previous_state.window_focused {
        window_events.push(WindowEventFilter::WindowFocusLost);
    }

    // Theme changed
    if current_state.theme != previous_state.theme {
        window_events.push(WindowEventFilter::ThemeChanged);
    }

    // File hover events (from FileDropManager)
    let current_hovered = file_drop_manager.get_hovered_file();
    let previous_hovered = previous_file_drop.and_then(|c| c.get_hovered_file());

    if current_hovered.is_some() && previous_hovered.is_none() {
        window_events.push(WindowEventFilter::HoveredFile);
        hover_events.push(HoverEventFilter::HoveredFile);
    }
    if current_hovered.is_none() && previous_hovered.is_some() {
        window_events.push(WindowEventFilter::HoveredFileCancelled);
        hover_events.push(HoverEventFilter::HoveredFileCancelled);
    }

    // File dropped (from FileDropManager - note: dropped_file is one-shot, so we only check
    // current) Dropped file events are generated when dropped_file transitions from None to
    // Some
    if file_drop_manager.dropped_file.is_some() {
        let previous_dropped = previous_file_drop.and_then(|c| c.dropped_file.as_ref());
        if previous_dropped.is_none() {
            window_events.push(WindowEventFilter::DroppedFile);
            hover_events.push(HoverEventFilter::DroppedFile);
        }
    }

    // Extract old hit node IDs from previous hover state (mouse only for now)
    use crate::managers::InputPointId;
    let old_hit_node_ids = hover_manager
        .get_frame(&InputPointId::Mouse, 1) // Get mouse hit test from 1 frame ago
        .map(|hit_test| {
            hit_test
                .hovered_nodes
                .iter()
                .map(|(k, v)| (k.clone(), v.regular_hit_test_nodes.clone()))
                .collect()
        })
        .unwrap_or_else(|| BTreeMap::new());

    // ========================================================================
    // Gesture Detection (optional, requires GestureAndDragManager)
    // ========================================================================

    if let Some(manager) = gesture_manager {
        // Detect DragStart (gesture detection threshold exceeded)
        if let Some(_detected_drag) = manager.detect_drag() {
            // Only fire DragStart if we don't already have an active drag
            // (to avoid repeated DragStart events)
            if !manager.is_dragging() {
                window_events.push(WindowEventFilter::DragStart);
                hover_events.push(HoverEventFilter::DragStart);
                focus_events.push(FocusEventFilter::DragStart);
            }
        }

        // Detect Drag (continuous drag movement)
        // Fire this when actively dragging (node_drag or window_drag active)
        if manager.is_dragging() && current_mouse_down {
            // Check if mouse actually moved during drag
            let current_pos = current_state.mouse_state.cursor_position.get_position();
            let previous_pos = previous_state.mouse_state.cursor_position.get_position();

            if current_pos != previous_pos {
                window_events.push(WindowEventFilter::Drag);
                hover_events.push(HoverEventFilter::Drag);
                focus_events.push(FocusEventFilter::Drag);
            }
        }

        // Detect DragEnd (mouse button released after drag)
        if manager.is_dragging() && event_was_mouse_release {
            window_events.push(WindowEventFilter::DragEnd);
            hover_events.push(HoverEventFilter::DragEnd);
            focus_events.push(FocusEventFilter::DragEnd);
        }

        // Detect DoubleClick
        if manager.detect_double_click() {
            window_events.push(WindowEventFilter::DoubleClick);
            hover_events.push(HoverEventFilter::DoubleClick);
            focus_events.push(FocusEventFilter::DoubleClick);
        }

        // Detect LongPress
        if let Some(long_press) = manager.detect_long_press() {
            // Only fire once per long press session
            if !long_press.callback_invoked {
                window_events.push(WindowEventFilter::LongPress);
                hover_events.push(HoverEventFilter::LongPress);
                focus_events.push(FocusEventFilter::LongPress);
            }
        }

        // Detect Swipe gestures (fast directional movement)
        if let Some(direction) = manager.detect_swipe_direction() {
            use crate::managers::gesture::GestureDirection;
            match direction {
                GestureDirection::Left => {
                    window_events.push(WindowEventFilter::SwipeLeft);
                    hover_events.push(HoverEventFilter::SwipeLeft);
                    focus_events.push(FocusEventFilter::SwipeLeft);
                }
                GestureDirection::Right => {
                    window_events.push(WindowEventFilter::SwipeRight);
                    hover_events.push(HoverEventFilter::SwipeRight);
                    focus_events.push(FocusEventFilter::SwipeRight);
                }
                GestureDirection::Up => {
                    window_events.push(WindowEventFilter::SwipeUp);
                    hover_events.push(HoverEventFilter::SwipeUp);
                    focus_events.push(FocusEventFilter::SwipeUp);
                }
                GestureDirection::Down => {
                    window_events.push(WindowEventFilter::SwipeDown);
                    hover_events.push(HoverEventFilter::SwipeDown);
                    focus_events.push(FocusEventFilter::SwipeDown);
                }
            }
        }

        // Detect Pinch gestures (two-finger zoom)
        if let Some(pinch) = manager.detect_pinch() {
            if pinch.scale < 1.0 {
                // Pinch In (zoom out)
                window_events.push(WindowEventFilter::PinchIn);
                hover_events.push(HoverEventFilter::PinchIn);
                focus_events.push(FocusEventFilter::PinchIn);
            } else {
                // Pinch Out (zoom in)
                window_events.push(WindowEventFilter::PinchOut);
                hover_events.push(HoverEventFilter::PinchOut);
                focus_events.push(FocusEventFilter::PinchOut);
            }
        }

        // Detect Rotation gestures (two-finger rotate)
        if let Some(rotation) = manager.detect_rotation() {
            if rotation.angle_radians > 0.0 {
                // Clockwise rotation
                window_events.push(WindowEventFilter::RotateClockwise);
                hover_events.push(HoverEventFilter::RotateClockwise);
                focus_events.push(FocusEventFilter::RotateClockwise);
            } else {
                // Counterclockwise rotation
                window_events.push(WindowEventFilter::RotateCounterClockwise);
                hover_events.push(HoverEventFilter::RotateCounterClockwise);
                focus_events.push(FocusEventFilter::RotateCounterClockwise);
            }
        }

        // Detect Pen events (stylus/pen input)
        if let Some(pen_state) = manager.get_pen_state() {
            // Pen contact state changed
            if pen_state.in_contact {
                // Check against previous state to see if this is a new contact
                // For now, we'll fire PenDown when pen is in contact
                // TODO: Track previous pen state to only fire on state change
                // window_events.push(WindowEventFilter::PenDown);
                // hover_events.push(HoverEventFilter::PenDown);
                // focus_events.push(FocusEventFilter::PenDown);
            }

            // Pen movement (only when in contact)
            if pen_state.in_contact {
                let current_pos = current_state.mouse_state.cursor_position.get_position();
                let previous_pos = previous_state.mouse_state.cursor_position.get_position();

                if current_pos != previous_pos {
                    window_events.push(WindowEventFilter::PenMove);
                    hover_events.push(HoverEventFilter::PenMove);
                    focus_events.push(FocusEventFilter::PenMove);
                }
            }
        }
    }

    // Get old focus node from FocusManager
    let old_focus_node = previous_focus.and_then(|f| f.get_focused_node().copied());

    Events {
        window_events,
        hover_events,
        focus_events,
        old_hit_node_ids,
        old_focus_node,
        current_window_state_mouse_is_down: current_mouse_down,
        previous_window_state_mouse_is_down: previous_mouse_down,
        event_was_mouse_down,
        event_was_mouse_leave,
        event_was_mouse_release,
    }
}
*/ // End of deprecated create_events_from_states functions
