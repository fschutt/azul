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
        }
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
