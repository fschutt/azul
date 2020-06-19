    #![allow(dead_code, unused_imports)]
    //! Window creation / startup configuration
    use crate::dll::*;
    use std::ffi::c_void;
    use crate::css::Css;


    /// `TaskBarIcon` struct
    pub use crate::dll::AzTaskBarIcon as TaskBarIcon;

    impl std::fmt::Debug for TaskBarIcon { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_task_bar_icon_fmt_debug)(self)) } }
    impl Clone for TaskBarIcon { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_task_bar_icon_deep_copy)(self) } }
    impl Drop for TaskBarIcon { fn drop(&mut self) { (crate::dll::get_azul_dll().az_task_bar_icon_delete)(self); } }


    /// `XWindowType` struct
    pub use crate::dll::AzXWindowType as XWindowType;

    impl std::fmt::Debug for XWindowType { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_x_window_type_fmt_debug)(self)) } }
    impl Clone for XWindowType { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_x_window_type_deep_copy)(self) } }
    impl Drop for XWindowType { fn drop(&mut self) { (crate::dll::get_azul_dll().az_x_window_type_delete)(self); } }


    /// `PhysicalPositionI32` struct
    pub use crate::dll::AzPhysicalPositionI32 as PhysicalPositionI32;

    impl std::fmt::Debug for PhysicalPositionI32 { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_physical_position_i32_fmt_debug)(self)) } }
    impl Clone for PhysicalPositionI32 { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_physical_position_i32_deep_copy)(self) } }
    impl Drop for PhysicalPositionI32 { fn drop(&mut self) { (crate::dll::get_azul_dll().az_physical_position_i32_delete)(self); } }


    /// `LogicalPosition` struct
    pub use crate::dll::AzLogicalPosition as LogicalPosition;

    impl std::fmt::Debug for LogicalPosition { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_logical_position_fmt_debug)(self)) } }
    impl Clone for LogicalPosition { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_logical_position_deep_copy)(self) } }
    impl Drop for LogicalPosition { fn drop(&mut self) { (crate::dll::get_azul_dll().az_logical_position_delete)(self); } }


    /// `IconKey` struct
    pub use crate::dll::AzIconKey as IconKey;

    impl std::fmt::Debug for IconKey { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_icon_key_fmt_debug)(self)) } }
    impl Clone for IconKey { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_icon_key_deep_copy)(self) } }
    impl Drop for IconKey { fn drop(&mut self) { (crate::dll::get_azul_dll().az_icon_key_delete)(self); } }


    /// `SmallWindowIconBytes` struct
    pub use crate::dll::AzSmallWindowIconBytes as SmallWindowIconBytes;

    impl std::fmt::Debug for SmallWindowIconBytes { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_small_window_icon_bytes_fmt_debug)(self)) } }
    impl Clone for SmallWindowIconBytes { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_small_window_icon_bytes_deep_copy)(self) } }
    impl Drop for SmallWindowIconBytes { fn drop(&mut self) { (crate::dll::get_azul_dll().az_small_window_icon_bytes_delete)(self); } }


    /// `LargeWindowIconBytes` struct
    pub use crate::dll::AzLargeWindowIconBytes as LargeWindowIconBytes;

    impl std::fmt::Debug for LargeWindowIconBytes { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_large_window_icon_bytes_fmt_debug)(self)) } }
    impl Clone for LargeWindowIconBytes { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_large_window_icon_bytes_deep_copy)(self) } }
    impl Drop for LargeWindowIconBytes { fn drop(&mut self) { (crate::dll::get_azul_dll().az_large_window_icon_bytes_delete)(self); } }


    /// `WindowIcon` struct
    pub use crate::dll::AzWindowIcon as WindowIcon;

    impl std::fmt::Debug for WindowIcon { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_window_icon_fmt_debug)(self)) } }
    impl Clone for WindowIcon { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_icon_deep_copy)(self) } }
    impl Drop for WindowIcon { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_icon_delete)(self); } }


    /// `VirtualKeyCode` struct
    pub use crate::dll::AzVirtualKeyCode as VirtualKeyCode;

    impl std::fmt::Debug for VirtualKeyCode { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_virtual_key_code_fmt_debug)(self)) } }
    impl Clone for VirtualKeyCode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_virtual_key_code_deep_copy)(self) } }
    impl Drop for VirtualKeyCode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_virtual_key_code_delete)(self); } }


    /// `AcceleratorKey` struct
    pub use crate::dll::AzAcceleratorKey as AcceleratorKey;

    impl std::fmt::Debug for AcceleratorKey { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_accelerator_key_fmt_debug)(self)) } }
    impl Clone for AcceleratorKey { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_accelerator_key_deep_copy)(self) } }
    impl Drop for AcceleratorKey { fn drop(&mut self) { (crate::dll::get_azul_dll().az_accelerator_key_delete)(self); } }


    /// `WindowSize` struct
    pub use crate::dll::AzWindowSize as WindowSize;

    impl std::fmt::Debug for WindowSize { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_window_size_fmt_debug)(self)) } }
    impl Clone for WindowSize { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_size_deep_copy)(self) } }
    impl Drop for WindowSize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_size_delete)(self); } }


    /// `WindowFlags` struct
    pub use crate::dll::AzWindowFlags as WindowFlags;

    impl std::fmt::Debug for WindowFlags { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_window_flags_fmt_debug)(self)) } }
    impl Clone for WindowFlags { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_flags_deep_copy)(self) } }
    impl Drop for WindowFlags { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_flags_delete)(self); } }


    /// `DebugState` struct
    pub use crate::dll::AzDebugState as DebugState;

    impl std::fmt::Debug for DebugState { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_debug_state_fmt_debug)(self)) } }
    impl Clone for DebugState { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_debug_state_deep_copy)(self) } }
    impl Drop for DebugState { fn drop(&mut self) { (crate::dll::get_azul_dll().az_debug_state_delete)(self); } }


    /// `KeyboardState` struct
    pub use crate::dll::AzKeyboardState as KeyboardState;

    impl std::fmt::Debug for KeyboardState { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_keyboard_state_fmt_debug)(self)) } }
    impl Clone for KeyboardState { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_keyboard_state_deep_copy)(self) } }
    impl Drop for KeyboardState { fn drop(&mut self) { (crate::dll::get_azul_dll().az_keyboard_state_delete)(self); } }


    /// `MouseCursorType` struct
    pub use crate::dll::AzMouseCursorType as MouseCursorType;

    impl std::fmt::Debug for MouseCursorType { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_mouse_cursor_type_fmt_debug)(self)) } }
    impl Clone for MouseCursorType { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_mouse_cursor_type_deep_copy)(self) } }
    impl Drop for MouseCursorType { fn drop(&mut self) { (crate::dll::get_azul_dll().az_mouse_cursor_type_delete)(self); } }


    /// `CursorPosition` struct
    pub use crate::dll::AzCursorPosition as CursorPosition;

    impl std::fmt::Debug for CursorPosition { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_cursor_position_fmt_debug)(self)) } }
    impl Clone for CursorPosition { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_cursor_position_deep_copy)(self) } }
    impl Drop for CursorPosition { fn drop(&mut self) { (crate::dll::get_azul_dll().az_cursor_position_delete)(self); } }


    /// `MouseState` struct
    pub use crate::dll::AzMouseState as MouseState;

    impl std::fmt::Debug for MouseState { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_mouse_state_fmt_debug)(self)) } }
    impl Clone for MouseState { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_mouse_state_deep_copy)(self) } }
    impl Drop for MouseState { fn drop(&mut self) { (crate::dll::get_azul_dll().az_mouse_state_delete)(self); } }


    /// `PlatformSpecificOptions` struct
    pub use crate::dll::AzPlatformSpecificOptions as PlatformSpecificOptions;

    impl std::fmt::Debug for PlatformSpecificOptions { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_platform_specific_options_fmt_debug)(self)) } }
    impl Clone for PlatformSpecificOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_platform_specific_options_deep_copy)(self) } }
    impl Drop for PlatformSpecificOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_platform_specific_options_delete)(self); } }


    /// `WindowsWindowOptions` struct
    pub use crate::dll::AzWindowsWindowOptions as WindowsWindowOptions;

    impl std::fmt::Debug for WindowsWindowOptions { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_windows_window_options_fmt_debug)(self)) } }
    impl Clone for WindowsWindowOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_windows_window_options_deep_copy)(self) } }
    impl Drop for WindowsWindowOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_windows_window_options_delete)(self); } }


    /// `WaylandTheme` struct
    pub use crate::dll::AzWaylandTheme as WaylandTheme;

    impl std::fmt::Debug for WaylandTheme { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_wayland_theme_fmt_debug)(self)) } }
    impl Clone for WaylandTheme { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_wayland_theme_deep_copy)(self) } }
    impl Drop for WaylandTheme { fn drop(&mut self) { (crate::dll::get_azul_dll().az_wayland_theme_delete)(self); } }


    /// `RendererType` struct
    pub use crate::dll::AzRendererType as RendererType;

    impl std::fmt::Debug for RendererType { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_renderer_type_fmt_debug)(self)) } }
    impl Clone for RendererType { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_renderer_type_deep_copy)(self) } }
    impl Drop for RendererType { fn drop(&mut self) { (crate::dll::get_azul_dll().az_renderer_type_delete)(self); } }


    /// `StringPair` struct
    pub use crate::dll::AzStringPair as StringPair;

    impl std::fmt::Debug for StringPair { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_string_pair_fmt_debug)(self)) } }
    impl Clone for StringPair { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_string_pair_deep_copy)(self) } }
    impl Drop for StringPair { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_pair_delete)(self); } }


    /// `LinuxWindowOptions` struct
    pub use crate::dll::AzLinuxWindowOptions as LinuxWindowOptions;

    impl std::fmt::Debug for LinuxWindowOptions { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_linux_window_options_fmt_debug)(self)) } }
    impl Clone for LinuxWindowOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_linux_window_options_deep_copy)(self) } }
    impl Drop for LinuxWindowOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_linux_window_options_delete)(self); } }


    /// `MacWindowOptions` struct
    pub use crate::dll::AzMacWindowOptions as MacWindowOptions;

    impl std::fmt::Debug for MacWindowOptions { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_mac_window_options_fmt_debug)(self)) } }
    impl Clone for MacWindowOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_mac_window_options_deep_copy)(self) } }
    impl Drop for MacWindowOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_mac_window_options_delete)(self); } }


    /// `WasmWindowOptions` struct
    pub use crate::dll::AzWasmWindowOptions as WasmWindowOptions;

    impl std::fmt::Debug for WasmWindowOptions { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_wasm_window_options_fmt_debug)(self)) } }
    impl Clone for WasmWindowOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_wasm_window_options_deep_copy)(self) } }
    impl Drop for WasmWindowOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_wasm_window_options_delete)(self); } }


    /// `FullScreenMode` struct
    pub use crate::dll::AzFullScreenMode as FullScreenMode;

    impl std::fmt::Debug for FullScreenMode { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_full_screen_mode_fmt_debug)(self)) } }
    impl Clone for FullScreenMode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_full_screen_mode_deep_copy)(self) } }
    impl Drop for FullScreenMode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_full_screen_mode_delete)(self); } }


    /// `WindowState` struct
    pub use crate::dll::AzWindowState as WindowState;

    impl std::fmt::Debug for WindowState { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_window_state_fmt_debug)(self)) } }
    impl Clone for WindowState { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_state_deep_copy)(self) } }
    impl Drop for WindowState { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_state_delete)(self); } }


    /// `LogicalSize` struct
    pub use crate::dll::AzLogicalSize as LogicalSize;

    impl std::fmt::Debug for LogicalSize { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_logical_size_fmt_debug)(self)) } }
    impl Clone for LogicalSize { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_logical_size_deep_copy)(self) } }
    impl Drop for LogicalSize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_logical_size_delete)(self); } }


    /// `HotReloadOptions` struct
    pub use crate::dll::AzHotReloadOptions as HotReloadOptions;

    impl std::fmt::Debug for HotReloadOptions { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_hot_reload_options_fmt_debug)(self)) } }
    impl Clone for HotReloadOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_hot_reload_options_deep_copy)(self) } }
    impl Drop for HotReloadOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_hot_reload_options_delete)(self); } }


    /// `WindowCreateOptions` struct
    pub use crate::dll::AzWindowCreateOptions as WindowCreateOptions;

    impl WindowCreateOptions {
        /// Creates a new `WindowCreateOptions` instance.
        pub fn new(css: Css) -> Self { (crate::dll::get_azul_dll().az_window_create_options_new)(css) }
    }

    impl std::fmt::Debug for WindowCreateOptions { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_window_create_options_fmt_debug)(self)) } }
    impl Clone for WindowCreateOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_create_options_deep_copy)(self) } }
    impl Drop for WindowCreateOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_create_options_delete)(self); } }
