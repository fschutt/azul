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
use azul_css::{
    corety::OptionU32, impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug,
    impl_vec_mut, impl_vec_partialeq, props::basic::OptionColorU, AzString,
};

use crate::callbacks::OptionCallback;

/// Options for creating a new window
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WindowCreateOptions {
    /// Initial state for the new window
    pub window_state: FullWindowState,
    /// Optional callback invoked after the window is created
    pub create_callback: OptionCallback,
    /// Optional renderer configuration (e.g., `VSync`, SRGB)
    pub renderer: azul_core::window::OptionRendererOptions,
    /// Optional window theme override (light/dark)
    pub theme: azul_core::window::OptionWindowTheme,
    /// If true, the window is resized to fit its content after the first layout
    pub size_to_content: bool,
    /// If true, enables hot-reloading of CSS and resources
    pub hot_reload: bool,
    /// Parent window's platform id (the window-registry key: X Window id on X11,
    /// `wl_surface` ptr on Wayland, HWND on Windows, `NSWindow` ptr on macOS), or 0
    /// for a top-level window with no parent. Child windows (menus, dropdowns,
    /// dialogs) set this so the backend can position them relative to the parent
    /// and, on X11, reuse the parent's display connection for the single shared
    /// event pump. 0 = no parent.
    pub parent_window_id: u64,
}

impl Default for WindowCreateOptions {
    fn default() -> Self {
        Self {
            window_state: FullWindowState::default(),
            create_callback: OptionCallback::None,
            renderer: azul_core::window::OptionRendererOptions::None,
            theme: azul_core::window::OptionWindowTheme::None,
            size_to_content: false,
            hot_reload: false,
            parent_window_id: 0,
        }
    }
}

impl WindowCreateOptions {
    /// Create a new `WindowCreateOptions` with a layout callback
    pub fn create(layout_callback: impl Into<LayoutCallback>) -> Self {
        let mut options = Self::default();
        options.window_state.layout_callback = layout_callback.into();
        options
    }
}

impl_option!(WindowCreateOptions, OptionWindowCreateOptions, copy = false, [Debug, Clone, PartialEq]);
impl_vec!(WindowCreateOptions, WindowCreateOptionsVec, WindowCreateOptionsVecDestructor, WindowCreateOptionsVecDestructorType, WindowCreateOptionsVecSlice, OptionWindowCreateOptions);
impl_vec_clone!(
    WindowCreateOptions,
    WindowCreateOptionsVec,
    WindowCreateOptionsVecDestructor
);
impl_vec_partialeq!(WindowCreateOptions, WindowCreateOptionsVec);
impl_vec_debug!(WindowCreateOptions, WindowCreateOptionsVec);
impl_vec_mut!(WindowCreateOptions, WindowCreateOptionsVec);

/// Full window state including internal fields not exposed to callbacks
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct FullWindowState {
    /// Platform-specific window options
    pub platform_specific_options: PlatformSpecificOptions,
    /// Current keyboard state (pressed keys, modifiers)
    pub keyboard_state: KeyboardState,
    /// Semantic window identifier for multi-window debugging.
    /// Can be set by the user to identify specific windows (e.g., "main", "settings", "popup-1")
    pub window_id: AzString,
    /// Window title bar text
    pub title: AzString,
    /// Optional callback invoked when the user requests the window to close
    pub close_callback: OptionCallback,
    /// Callback that returns the DOM for this window
    pub layout_callback: LayoutCallback,
    /// Window position on screen
    pub position: WindowPosition,
    /// Current touch/gesture input state
    pub touch_state: TouchState,
    /// Window dimensions (logical and physical)
    pub size: WindowSize,
    /// Window flags (minimized, maximized, fullscreen, etc.)
    pub flags: WindowFlags,
    /// Current mouse cursor state (position, buttons)
    pub mouse_state: MouseState,
    /// Active window theme (light/dark)
    pub theme: WindowTheme,
    /// Position of the IME candidate window
    pub ime_position: ImePosition,
    /// GPU renderer options (`VSync`, SRGB, hardware acceleration)
    pub renderer_options: RendererOptions,
    /// Monitor ID (not the full Monitor struct - just the identifier)
    pub monitor_id: OptionU32,
    /// Debug visualization state (layout borders, repaints, etc.)
    pub debug_state: DebugState,
    /// Window background color. If None, uses system window background color.
    pub background_color: OptionColorU,
    /// Whether this window currently has input focus
    pub window_focused: bool,
    /// Active route match (pattern + extracted parameters).
    /// Set by `CallbackInfo::switch_route()` or by the web server on URL match.
    /// Layout callbacks read this via `LayoutCallbackInfo::get_route_param()`.
    pub active_route: azul_core::resources::OptionRouteMatch,
}

impl_option!(
    FullWindowState,
    OptionFullWindowState,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl Default for FullWindowState {
    fn default() -> Self {
        Self {
            platform_specific_options: PlatformSpecificOptions::default(),
            keyboard_state: KeyboardState::default(),
            window_id: AzString::from_const_str("azul-window"),
            title: AzString::from_const_str("Azul Window"),
            close_callback: OptionCallback::None,
            layout_callback: LayoutCallback::default(),
            position: WindowPosition::default(),
            touch_state: TouchState::default(),
            size: WindowSize::default(),
            flags: WindowFlags::default(),
            mouse_state: MouseState::default(),
            theme: WindowTheme::default(),
            ime_position: ImePosition::default(),
            renderer_options: RendererOptions::default(),
            monitor_id: OptionU32::None,
            debug_state: DebugState::default(),
            background_color: OptionColorU::None,
            window_focused: true,
            active_route: azul_core::resources::OptionRouteMatch::None,
        }
    }
}
