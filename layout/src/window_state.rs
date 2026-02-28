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
    pub window_state: FullWindowState,
    pub create_callback: OptionCallback,
    pub renderer: azul_core::window::OptionRendererOptions,
    pub theme: azul_core::window::OptionWindowTheme,
    pub size_to_content: bool,
    pub hot_reload: bool,
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
        }
    }
}

impl WindowCreateOptions {
    /// Create a new WindowCreateOptions with a layout callback
    pub fn create(layout_callback: impl Into<azul_core::callbacks::LayoutCallback>) -> Self {
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
    pub keyboard_state: KeyboardState,
    /// Semantic window identifier for multi-window debugging
    /// Can be set by the user to identify specific windows (e.g., "main", "settings", "popup-1")
    pub window_id: AzString,
    pub title: AzString,
    pub close_callback: OptionCallback,
    pub layout_callback: LayoutCallback,
    pub position: WindowPosition,
    pub touch_state: TouchState,
    pub size: WindowSize,
    pub flags: WindowFlags,
    pub mouse_state: MouseState,
    pub theme: WindowTheme,
    pub ime_position: ImePosition,
    pub renderer_options: RendererOptions,
    /// Monitor ID (not the full Monitor struct - just the identifier)
    pub monitor_id: OptionU32,
    pub debug_state: DebugState,
    /// Window background color. If None, uses system window background color.
    pub background_color: OptionColorU,
    pub window_focused: bool,
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
        }
    }
}
