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
    impl_vec_mut, impl_vec_partialeq, props::basic::color::ColorU, AzString,
};

use crate::callbacks::OptionCallback;

/// Options for creating a new window
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WindowCreateOptions {
    pub window_state: FullWindowState,
    pub size_to_content: bool,
    pub renderer: azul_core::window::OptionRendererOptions,
    pub theme: azul_core::window::OptionWindowTheme,
    pub create_callback: OptionCallback,
    pub hot_reload: bool,
}

impl Default for WindowCreateOptions {
    fn default() -> Self {
        Self {
            window_state: FullWindowState::default(),
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
    pub fn create(layout_callback: impl Into<azul_core::callbacks::LayoutCallback>) -> Self {
        let mut options = Self::default();
        options.window_state.layout_callback = layout_callback.into();
        options
    }
}

impl_vec!(
    WindowCreateOptions,
    WindowCreateOptionsVec,
    WindowCreateOptionsVecDestructor
);
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
    pub monitor_id: OptionU32,
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
            monitor_id: OptionU32::None,
            window_focused: true,
        }
    }
}
