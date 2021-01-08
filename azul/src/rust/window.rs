    #![allow(dead_code, unused_imports)]
    //! Window creation / startup configuration
    use crate::dll::*;
    use std::ffi::c_void;

    impl LayoutSize {
        #[inline(always)]
        pub const fn new(width: isize, height: isize) -> Self { Self { width, height } }
        #[inline(always)]
        pub const fn zero() -> Self { Self::new(0, 0) }
    }

    impl LayoutPoint {
        #[inline(always)]
        pub const fn new(x: isize, y: isize) -> Self { Self { x, y } }
        #[inline(always)]
        pub const fn zero() -> Self { Self::new(0, 0) }
    }

    impl LayoutRect {
        #[inline(always)]
        pub const fn new(origin: LayoutPoint, size: LayoutSize) -> Self { Self { origin, size } }
        #[inline(always)]
        pub const fn zero() -> Self { Self::new(LayoutPoint::zero(), LayoutSize::zero()) }
        #[inline(always)]
        pub const fn max_x(&self) -> isize { self.origin.x + self.size.width }
        #[inline(always)]
        pub const fn min_x(&self) -> isize { self.origin.x }
        #[inline(always)]
        pub const fn max_y(&self) -> isize { self.origin.y + self.size.height }
        #[inline(always)]
        pub const fn min_y(&self) -> isize { self.origin.y }

        pub const fn contains(&self, other: &LayoutPoint) -> bool {
            self.min_x() <= other.x && other.x < self.max_x() &&
            self.min_y() <= other.y && other.y < self.max_y()
        }

        pub fn contains_f32(&self, other_x: f32, other_y: f32) -> bool {
            self.min_x() as f32 <= other_x && other_x < self.max_x() as f32 &&
            self.min_y() as f32 <= other_y && other_y < self.max_y() as f32
        }

        /// Same as `contains()`, but returns the (x, y) offset of the hit point
        ///
        /// On a regular computer this function takes ~3.2ns to run
        #[inline]
        pub const fn hit_test(&self, other: &LayoutPoint) -> Option<LayoutPoint> {
            let dx_left_edge = other.x - self.min_x();
            let dx_right_edge = self.max_x() - other.x;
            let dy_top_edge = other.y - self.min_y();
            let dy_bottom_edge = self.max_y() - other.y;
            if dx_left_edge > 0 &&
               dx_right_edge > 0 &&
               dy_top_edge > 0 &&
               dy_bottom_edge > 0
            {
                Some(LayoutPoint::new(dx_left_edge, dy_top_edge))
            } else {
                None
            }
        }

        // Returns if b overlaps a
        #[inline(always)]
        pub const fn contains_rect(&self, b: &LayoutRect) -> bool {

            let a = self;

            let a_x         = a.origin.x;
            let a_y         = a.origin.y;
            let a_width     = a.size.width;
            let a_height    = a.size.height;

            let b_x         = b.origin.x;
            let b_y         = b.origin.y;
            let b_width     = b.size.width;
            let b_height    = b.size.height;

            b_x >= a_x &&
            b_y >= a_y &&
            b_x + b_width <= a_x + a_width &&
            b_y + b_height <= a_y + a_height
        }
    }    use crate::callbacks::LayoutCallbackType;


    /// `LayoutPoint` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPoint as LayoutPoint;

    impl Clone for LayoutPoint { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPoint { }


    /// `LayoutSize` struct
    #[doc(inline)] pub use crate::dll::AzLayoutSize as LayoutSize;

    impl Clone for LayoutSize { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutSize { }


    /// `LayoutRect` struct
    #[doc(inline)] pub use crate::dll::AzLayoutRect as LayoutRect;

    impl Clone for LayoutRect { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutRect { }


    /// `RawWindowHandle` struct
    #[doc(inline)] pub use crate::dll::AzRawWindowHandle as RawWindowHandle;

    impl Clone for RawWindowHandle { fn clone(&self) -> Self { *self } }
    impl Copy for RawWindowHandle { }


    /// `IOSHandle` struct
    #[doc(inline)] pub use crate::dll::AzIOSHandle as IOSHandle;

    impl Clone for IOSHandle { fn clone(&self) -> Self { *self } }
    impl Copy for IOSHandle { }


    /// `MacOSHandle` struct
    #[doc(inline)] pub use crate::dll::AzMacOSHandle as MacOSHandle;

    impl Clone for MacOSHandle { fn clone(&self) -> Self { *self } }
    impl Copy for MacOSHandle { }


    /// `XlibHandle` struct
    #[doc(inline)] pub use crate::dll::AzXlibHandle as XlibHandle;

    impl Clone for XlibHandle { fn clone(&self) -> Self { *self } }
    impl Copy for XlibHandle { }


    /// `XcbHandle` struct
    #[doc(inline)] pub use crate::dll::AzXcbHandle as XcbHandle;

    impl Clone for XcbHandle { fn clone(&self) -> Self { *self } }
    impl Copy for XcbHandle { }


    /// `WaylandHandle` struct
    #[doc(inline)] pub use crate::dll::AzWaylandHandle as WaylandHandle;

    impl Clone for WaylandHandle { fn clone(&self) -> Self { *self } }
    impl Copy for WaylandHandle { }


    /// `WindowsHandle` struct
    #[doc(inline)] pub use crate::dll::AzWindowsHandle as WindowsHandle;

    impl Clone for WindowsHandle { fn clone(&self) -> Self { *self } }
    impl Copy for WindowsHandle { }


    /// `WebHandle` struct
    #[doc(inline)] pub use crate::dll::AzWebHandle as WebHandle;

    impl Clone for WebHandle { fn clone(&self) -> Self { *self } }
    impl Copy for WebHandle { }


    /// `AndroidHandle` struct
    #[doc(inline)] pub use crate::dll::AzAndroidHandle as AndroidHandle;

    impl Clone for AndroidHandle { fn clone(&self) -> Self { *self } }
    impl Copy for AndroidHandle { }


    /// `TaskBarIcon` struct
    #[doc(inline)] pub use crate::dll::AzTaskBarIcon as TaskBarIcon;

    impl Clone for TaskBarIcon { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_task_bar_icon_deep_copy)(self) } }
    impl Drop for TaskBarIcon { fn drop(&mut self) { (crate::dll::get_azul_dll().az_task_bar_icon_delete)(self); } }


    /// `XWindowType` struct
    #[doc(inline)] pub use crate::dll::AzXWindowType as XWindowType;

    impl Clone for XWindowType { fn clone(&self) -> Self { *self } }
    impl Copy for XWindowType { }


    /// `PhysicalPositionI32` struct
    #[doc(inline)] pub use crate::dll::AzPhysicalPositionI32 as PhysicalPositionI32;

    impl Clone for PhysicalPositionI32 { fn clone(&self) -> Self { *self } }
    impl Copy for PhysicalPositionI32 { }


    /// `PhysicalSizeU32` struct
    #[doc(inline)] pub use crate::dll::AzPhysicalSizeU32 as PhysicalSizeU32;

    impl Clone for PhysicalSizeU32 { fn clone(&self) -> Self { *self } }
    impl Copy for PhysicalSizeU32 { }


    /// `LogicalPosition` struct
    #[doc(inline)] pub use crate::dll::AzLogicalPosition as LogicalPosition;

    impl Clone for LogicalPosition { fn clone(&self) -> Self { *self } }
    impl Copy for LogicalPosition { }


    /// `LogicalRect` struct
    #[doc(inline)] pub use crate::dll::AzLogicalRect as LogicalRect;

    impl Clone for LogicalRect { fn clone(&self) -> Self { *self } }
    impl Copy for LogicalRect { }


    /// `IconKey` struct
    #[doc(inline)] pub use crate::dll::AzIconKey as IconKey;

    impl Clone for IconKey { fn clone(&self) -> Self { *self } }
    impl Copy for IconKey { }


    /// `SmallWindowIconBytes` struct
    #[doc(inline)] pub use crate::dll::AzSmallWindowIconBytes as SmallWindowIconBytes;

    impl Clone for SmallWindowIconBytes { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_small_window_icon_bytes_deep_copy)(self) } }
    impl Drop for SmallWindowIconBytes { fn drop(&mut self) { (crate::dll::get_azul_dll().az_small_window_icon_bytes_delete)(self); } }


    /// `LargeWindowIconBytes` struct
    #[doc(inline)] pub use crate::dll::AzLargeWindowIconBytes as LargeWindowIconBytes;

    impl Clone for LargeWindowIconBytes { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_large_window_icon_bytes_deep_copy)(self) } }
    impl Drop for LargeWindowIconBytes { fn drop(&mut self) { (crate::dll::get_azul_dll().az_large_window_icon_bytes_delete)(self); } }


    /// `WindowIcon` struct
    #[doc(inline)] pub use crate::dll::AzWindowIcon as WindowIcon;

    impl Clone for WindowIcon { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_icon_deep_copy)(self) } }
    impl Drop for WindowIcon { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_icon_delete)(self); } }


    /// `VirtualKeyCode` struct
    #[doc(inline)] pub use crate::dll::AzVirtualKeyCode as VirtualKeyCode;

    impl Clone for VirtualKeyCode { fn clone(&self) -> Self { *self } }
    impl Copy for VirtualKeyCode { }


    /// `AcceleratorKey` struct
    #[doc(inline)] pub use crate::dll::AzAcceleratorKey as AcceleratorKey;

    impl Clone for AcceleratorKey { fn clone(&self) -> Self { *self } }
    impl Copy for AcceleratorKey { }


    /// `WindowSize` struct
    #[doc(inline)] pub use crate::dll::AzWindowSize as WindowSize;

    impl Clone for WindowSize { fn clone(&self) -> Self { *self } }
    impl Copy for WindowSize { }


    /// `WindowFlags` struct
    #[doc(inline)] pub use crate::dll::AzWindowFlags as WindowFlags;

    impl Clone for WindowFlags { fn clone(&self) -> Self { *self } }
    impl Copy for WindowFlags { }


    /// `DebugState` struct
    #[doc(inline)] pub use crate::dll::AzDebugState as DebugState;

    impl Clone for DebugState { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_debug_state_deep_copy)(self) } }
    impl Drop for DebugState { fn drop(&mut self) { (crate::dll::get_azul_dll().az_debug_state_delete)(self); } }


    /// `KeyboardState` struct
    #[doc(inline)] pub use crate::dll::AzKeyboardState as KeyboardState;

    impl Clone for KeyboardState { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_keyboard_state_deep_copy)(self) } }
    impl Drop for KeyboardState { fn drop(&mut self) { (crate::dll::get_azul_dll().az_keyboard_state_delete)(self); } }


    /// `MouseCursorType` struct
    #[doc(inline)] pub use crate::dll::AzMouseCursorType as MouseCursorType;

    impl Clone for MouseCursorType { fn clone(&self) -> Self { *self } }
    impl Copy for MouseCursorType { }


    /// `CursorPosition` struct
    #[doc(inline)] pub use crate::dll::AzCursorPosition as CursorPosition;

    impl Clone for CursorPosition { fn clone(&self) -> Self { *self } }
    impl Copy for CursorPosition { }


    /// `MouseState` struct
    #[doc(inline)] pub use crate::dll::AzMouseState as MouseState;

    impl Clone for MouseState { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_mouse_state_deep_copy)(self) } }
    impl Drop for MouseState { fn drop(&mut self) { (crate::dll::get_azul_dll().az_mouse_state_delete)(self); } }


    /// `PlatformSpecificOptions` struct
    #[doc(inline)] pub use crate::dll::AzPlatformSpecificOptions as PlatformSpecificOptions;

    impl Clone for PlatformSpecificOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_platform_specific_options_deep_copy)(self) } }
    impl Drop for PlatformSpecificOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_platform_specific_options_delete)(self); } }


    /// `WindowsWindowOptions` struct
    #[doc(inline)] pub use crate::dll::AzWindowsWindowOptions as WindowsWindowOptions;

    impl Clone for WindowsWindowOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_windows_window_options_deep_copy)(self) } }
    impl Drop for WindowsWindowOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_windows_window_options_delete)(self); } }


    /// `WaylandTheme` struct
    #[doc(inline)] pub use crate::dll::AzWaylandTheme as WaylandTheme;

    impl Clone for WaylandTheme { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_wayland_theme_deep_copy)(self) } }
    impl Drop for WaylandTheme { fn drop(&mut self) { (crate::dll::get_azul_dll().az_wayland_theme_delete)(self); } }


    /// `RendererType` struct
    #[doc(inline)] pub use crate::dll::AzRendererType as RendererType;

    impl Clone for RendererType { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_renderer_type_deep_copy)(self) } }
    impl Drop for RendererType { fn drop(&mut self) { (crate::dll::get_azul_dll().az_renderer_type_delete)(self); } }


    /// `StringPair` struct
    #[doc(inline)] pub use crate::dll::AzStringPair as StringPair;

    impl Clone for StringPair { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_string_pair_deep_copy)(self) } }
    impl Drop for StringPair { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_pair_delete)(self); } }


    /// `LinuxWindowOptions` struct
    #[doc(inline)] pub use crate::dll::AzLinuxWindowOptions as LinuxWindowOptions;

    impl Clone for LinuxWindowOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_linux_window_options_deep_copy)(self) } }
    impl Drop for LinuxWindowOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_linux_window_options_delete)(self); } }


    /// `MacWindowOptions` struct
    #[doc(inline)] pub use crate::dll::AzMacWindowOptions as MacWindowOptions;

    impl Clone for MacWindowOptions { fn clone(&self) -> Self { *self } }
    impl Copy for MacWindowOptions { }


    /// `WasmWindowOptions` struct
    #[doc(inline)] pub use crate::dll::AzWasmWindowOptions as WasmWindowOptions;

    impl Clone for WasmWindowOptions { fn clone(&self) -> Self { *self } }
    impl Copy for WasmWindowOptions { }


    /// `FullScreenMode` struct
    #[doc(inline)] pub use crate::dll::AzFullScreenMode as FullScreenMode;

    impl Clone for FullScreenMode { fn clone(&self) -> Self { *self } }
    impl Copy for FullScreenMode { }


    /// `WindowTheme` struct
    #[doc(inline)] pub use crate::dll::AzWindowTheme as WindowTheme;

    impl Clone for WindowTheme { fn clone(&self) -> Self { *self } }
    impl Copy for WindowTheme { }


    /// `WindowPosition` struct
    #[doc(inline)] pub use crate::dll::AzWindowPosition as WindowPosition;

    impl Clone for WindowPosition { fn clone(&self) -> Self { *self } }
    impl Copy for WindowPosition { }


    /// `ImePosition` struct
    #[doc(inline)] pub use crate::dll::AzImePosition as ImePosition;

    impl Clone for ImePosition { fn clone(&self) -> Self { *self } }
    impl Copy for ImePosition { }


    /// `TouchState` struct
    #[doc(inline)] pub use crate::dll::AzTouchState as TouchState;

    impl Clone for TouchState { fn clone(&self) -> Self { *self } }
    impl Copy for TouchState { }


    /// `WindowState` struct
    #[doc(inline)] pub use crate::dll::AzWindowState as WindowState;

    impl WindowState {
        /// Creates a new `WindowState` instance.
        pub fn new(layout_callback: LayoutCallbackType) -> Self { (crate::dll::get_azul_dll().az_window_state_new)(layout_callback) }
    }

    impl Clone for WindowState { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_state_deep_copy)(self) } }
    impl Drop for WindowState { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_state_delete)(self); } }


    /// `LogicalSize` struct
    #[doc(inline)] pub use crate::dll::AzLogicalSize as LogicalSize;

    impl Clone for LogicalSize { fn clone(&self) -> Self { *self } }
    impl Copy for LogicalSize { }


    /// `WindowCreateOptions` struct
    #[doc(inline)] pub use crate::dll::AzWindowCreateOptions as WindowCreateOptions;

    impl WindowCreateOptions {
        /// Creates a new `WindowCreateOptions` instance.
        pub fn new(layout_callback: LayoutCallbackType) -> Self { (crate::dll::get_azul_dll().az_window_create_options_new)(layout_callback) }
    }

    impl Clone for WindowCreateOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_create_options_deep_copy)(self) } }
    impl Drop for WindowCreateOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_create_options_delete)(self); } }
