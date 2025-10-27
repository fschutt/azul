//! Window state types for azul-layout
//!
//! These types are defined here (rather than azul-core) because CallbackInfo
//! needs to reference them, and CallbackInfo must live in azul-layout (since
//! it requires LayoutWindow).

use alloc::collections::BTreeMap;

use azul_core::{
    callbacks::LayoutCallback,
    dom::{DomId, DomNodeId},
    selection::SelectionState,
    window::{
        DebugState, ImePosition, KeyboardState, Monitor, MouseState, PlatformSpecificOptions,
        RendererOptions, TouchState, WindowFlags, WindowPosition, WindowSize, WindowTheme,
    },
};
use azul_css::{impl_option, impl_option_inner, props::basic::color::ColorU, AzString};

use crate::{callbacks::OptionCallback, hit_test::FullHitTest};

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
    pub monitor: Monitor,
    pub hovered_file: Option<AzString>,
    pub dropped_file: Option<AzString>,
    pub focused_node: Option<DomNodeId>,
    pub last_hit_test: FullHitTest,
    pub selections: BTreeMap<DomId, SelectionState>,
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
            monitor: Monitor::default(),
            hovered_file: None,
            dropped_file: None,
            focused_node: None,
            last_hit_test: FullHitTest::empty(None),
            selections: BTreeMap::new(),
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
            monitor: full.monitor,
            platform_specific_options: full.platform_specific_options,
            renderer_options: full.renderer_options,
            background_color: full.background_color,
            layout_callback: full.layout_callback,
            close_callback: full.close_callback,
        }
    }
}

/// Create Events by comparing current and previous window states.
/// This is the cross-platform event detection layer.
pub fn create_events_from_states(
    current_state: &FullWindowState,
    previous_state: &FullWindowState,
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

    // Scroll events
    let current_scroll_x = current_state
        .mouse_state
        .scroll_x
        .into_option()
        .unwrap_or(0.0);
    let current_scroll_y = current_state
        .mouse_state
        .scroll_y
        .into_option()
        .unwrap_or(0.0);
    let previous_scroll_x = previous_state
        .mouse_state
        .scroll_x
        .into_option()
        .unwrap_or(0.0);
    let previous_scroll_y = previous_state
        .mouse_state
        .scroll_y
        .into_option()
        .unwrap_or(0.0);

    if (current_scroll_x - previous_scroll_x).abs() > 0.01
        || (current_scroll_y - previous_scroll_y).abs() > 0.01
    {
        window_events.push(WindowEventFilter::Scroll);
        hover_events.push(HoverEventFilter::Scroll);
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

    // Text input
    let current_char = current_state.keyboard_state.current_char.into_option();
    let previous_char = previous_state.keyboard_state.current_char.into_option();

    if current_char.is_some() && current_char != previous_char {
        window_events.push(WindowEventFilter::TextInput);
        hover_events.push(HoverEventFilter::TextInput);
        focus_events.push(FocusEventFilter::TextInput);
    }

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

    // File hover events
    if current_state.hovered_file.is_some() && previous_state.hovered_file.is_none() {
        window_events.push(WindowEventFilter::HoveredFile);
        hover_events.push(HoverEventFilter::HoveredFile);
    }
    if current_state.hovered_file.is_none() && previous_state.hovered_file.is_some() {
        window_events.push(WindowEventFilter::HoveredFileCancelled);
        hover_events.push(HoverEventFilter::HoveredFileCancelled);
    }

    // File dropped
    if current_state.dropped_file.is_some() && previous_state.dropped_file.is_none() {
        window_events.push(WindowEventFilter::DroppedFile);
        hover_events.push(HoverEventFilter::DroppedFile);
    }

    // Extract old hit node IDs from previous state's hit test
    let old_hit_node_ids = previous_state
        .last_hit_test
        .hovered_nodes
        .iter()
        .map(|(k, v)| (k.clone(), v.regular_hit_test_nodes.clone()))
        .collect();

    Events {
        window_events,
        hover_events,
        focus_events,
        old_hit_node_ids,
        old_focus_node: previous_state.focused_node,
        current_window_state_mouse_is_down: current_mouse_down,
        previous_window_state_mouse_is_down: previous_mouse_down,
        event_was_mouse_down,
        event_was_mouse_leave,
        event_was_mouse_release,
    }
}
