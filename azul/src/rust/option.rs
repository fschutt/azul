    #![allow(dead_code, unused_imports)]
    //! Definition of azuls internal `Option<*>` wrappers
    use crate::dll::*;
    use std::ffi::c_void;

    macro_rules! impl_option_inner {
        ($struct_type:ident, $struct_name:ident) => (

        impl Default for $struct_name {
            fn default() -> $struct_name { $struct_name::None }
        }

        impl $struct_name {
            pub fn as_option(&self) -> Option<&$struct_type> {
                match self {
                    $struct_name::None => None,
                    $struct_name::Some(t) => Some(t),
                }
            }

            pub fn as_mut_option(&mut self) -> Option<&mut $struct_type> {
                match self {
                    $struct_name::None => None,
                    $struct_name::Some(t) => Some(t),
                }
            }

            pub fn is_some(&self) -> bool {
                match self {
                    $struct_name::None => false,
                    $struct_name::Some(_) => true,
                }
            }

            pub fn is_none(&self) -> bool {
                !self.is_some()
            }
        }
    )}

    macro_rules! impl_option {
        ($struct_type:ident, $struct_name:ident, copy = false, clone = false, [$($derive:meta),* ]) => (
            impl_option_inner!($struct_type, $struct_name);

            impl From<Option<$struct_type>> for $struct_name {
                fn from(mut o: Option<$struct_type>) -> $struct_name {
                    if o.is_none() {
                        $struct_name::None
                    } else {
                        $struct_name::Some(o.take().unwrap())
                    }
                }
            }
        );
        ($struct_type:ident, $struct_name:ident, copy = false, [$($derive:meta),* ]) => (
            impl $struct_name {
                pub fn into_option(&self) -> Option<$struct_type> {
                    match &self {
                        $struct_name::None => None,
                        $struct_name::Some(t) => Some(t.clone()),
                    }
                }
            }

            impl From<$struct_name> for Option<$struct_type> {
                fn from(o: $struct_name) -> Option<$struct_type> {
                    match &o {
                        $struct_name::None => None,
                        $struct_name::Some(t) => Some(t.clone()),
                    }
                }
            }

            impl From<Option<$struct_type>> for $struct_name {
                fn from(o: Option<$struct_type>) -> $struct_name {
                    match &o {
                        None => $struct_name::None,
                        Some(t) => $struct_name::Some(t.clone()),
                    }
                }
            }

            impl_option_inner!($struct_type, $struct_name);
        );
        ($struct_type:ident, $struct_name:ident, [$($derive:meta),* ]) => (
            impl $struct_name {
                pub fn into_option(&self) -> Option<$struct_type> {
                    match self {
                        $struct_name::None => None,
                        $struct_name::Some(t) => Some(*t),
                    }
                }
            }

            impl From<$struct_name> for Option<$struct_type> {
                fn from(o: $struct_name) -> Option<$struct_type> {
                    match &o {
                        $struct_name::None => None,
                        $struct_name::Some(t) => Some(*t),
                    }
                }
            }

            impl From<Option<$struct_type>> for $struct_name {
                fn from(o: Option<$struct_type>) -> $struct_name {
                    match &o {
                        None => $struct_name::None,
                        Some(t) => $struct_name::Some(*t),
                    }
                }
            }

            impl_option_inner!($struct_type, $struct_name);
        );
    }

    impl_option!(AzTabIndex, AzOptionTabIndex, copy = false, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzDom, AzOptionDom, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

    impl_option!(AzU8VecRef, AzOptionU8VecRef, copy = false, clone = false, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzTexture, AzOptionTexture, copy = false, clone = false, [Debug, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(usize, AzOptionUsize, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

    impl_option!(AzInstantPtr, AzOptionInstantPtr, copy = false, clone = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzDuration, AzOptionDuration, copy = false, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

    impl_option!(u32, AzOptionChar, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzVirtualKeyCode, AzOptionVirtualKeyCode, copy = false, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(i32, AzOptionI32, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(f32, AzOptionF32, [Debug, Copy, Clone, PartialEq, PartialOrd]);
    impl_option!(AzMouseCursorType, AzOptionMouseCursorType, copy = false, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzString, AzOptionString, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    pub type AzHwndHandle = *mut c_void;
    impl_option!(AzHwndHandle, AzOptionHwndHandle, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    pub type AzX11Visual = *const c_void;
    impl_option!(AzX11Visual, AzOptionX11Visual, copy = false, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzWaylandTheme, AzOptionWaylandTheme, copy = false, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzHotReloadOptions, AzOptionHotReloadOptions, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzLogicalPosition, AzOptionLogicalPosition, copy = false, [Debug, Copy, Clone, PartialEq, PartialOrd]);
    impl_option!(AzLogicalSize, AzOptionLogicalSize, copy = false, [Debug, Copy, Clone, PartialEq, PartialOrd]);
    impl_option!(AzPhysicalPositionI32, AzOptionPhysicalPositionI32, copy = false, [Debug, Copy, Clone, PartialEq, PartialOrd]);
    impl_option!(AzWindowIcon, AzOptionWindowIcon, copy = false, [Debug, Clone, PartialOrd, PartialEq, Eq, Hash, Ord]);
    impl_option!(AzTaskBarIcon, AzOptionTaskBarIcon, copy = false, [Debug, Clone, PartialOrd, PartialEq, Eq, Hash, Ord]);


    /// `OptionNodeId` struct
    pub use crate::dll::AzOptionNodeId as OptionNodeId;

    impl std::fmt::Debug for OptionNodeId { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_node_id_fmt_debug)(self)) } }
    impl Clone for OptionNodeId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_node_id_deep_copy)(self) } }
    impl Drop for OptionNodeId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_node_id_delete)(self); } }


    /// `OptionDomNodeId` struct
    pub use crate::dll::AzOptionDomNodeId as OptionDomNodeId;

    impl std::fmt::Debug for OptionDomNodeId { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_dom_node_id_fmt_debug)(self)) } }
    impl Clone for OptionDomNodeId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_dom_node_id_deep_copy)(self) } }
    impl Drop for OptionDomNodeId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_dom_node_id_delete)(self); } }


    /// `OptionColorU` struct
    pub use crate::dll::AzOptionColorU as OptionColorU;

    impl std::fmt::Debug for OptionColorU { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_color_u_fmt_debug)(self)) } }
    impl Clone for OptionColorU { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_color_u_deep_copy)(self) } }
    impl Drop for OptionColorU { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_color_u_delete)(self); } }


    /// `OptionRawImage` struct
    pub use crate::dll::AzOptionRawImage as OptionRawImage;

    impl std::fmt::Debug for OptionRawImage { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_raw_image_fmt_debug)(self)) } }
    impl Clone for OptionRawImage { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_raw_image_deep_copy)(self) } }
    impl Drop for OptionRawImage { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_raw_image_delete)(self); } }


    /// `OptionSvgDashPattern` struct
    pub use crate::dll::AzOptionSvgDashPattern as OptionSvgDashPattern;

    impl std::fmt::Debug for OptionSvgDashPattern { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_svg_dash_pattern_fmt_debug)(self)) } }
    impl Clone for OptionSvgDashPattern { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_svg_dash_pattern_deep_copy)(self) } }
    impl Drop for OptionSvgDashPattern { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_svg_dash_pattern_delete)(self); } }


    /// `OptionWaylandTheme` struct
    pub use crate::dll::AzOptionWaylandTheme as OptionWaylandTheme;

    impl std::fmt::Debug for OptionWaylandTheme { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_wayland_theme_fmt_debug)(self)) } }
    impl Clone for OptionWaylandTheme { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_wayland_theme_deep_copy)(self) } }
    impl Drop for OptionWaylandTheme { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_wayland_theme_delete)(self); } }


    /// `OptionTaskBarIcon` struct
    pub use crate::dll::AzOptionTaskBarIcon as OptionTaskBarIcon;

    impl std::fmt::Debug for OptionTaskBarIcon { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_task_bar_icon_fmt_debug)(self)) } }
    impl Clone for OptionTaskBarIcon { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_task_bar_icon_deep_copy)(self) } }
    impl Drop for OptionTaskBarIcon { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_task_bar_icon_delete)(self); } }


    /// `OptionHwndHandle` struct
    pub use crate::dll::AzOptionHwndHandle as OptionHwndHandle;

    impl std::fmt::Debug for OptionHwndHandle { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_hwnd_handle_fmt_debug)(self)) } }
    impl Clone for OptionHwndHandle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_hwnd_handle_deep_copy)(self) } }
    impl Drop for OptionHwndHandle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_hwnd_handle_delete)(self); } }


    /// `OptionLogicalPosition` struct
    pub use crate::dll::AzOptionLogicalPosition as OptionLogicalPosition;

    impl std::fmt::Debug for OptionLogicalPosition { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_logical_position_fmt_debug)(self)) } }
    impl Clone for OptionLogicalPosition { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_logical_position_deep_copy)(self) } }
    impl Drop for OptionLogicalPosition { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_logical_position_delete)(self); } }


    /// `OptionPhysicalPositionI32` struct
    pub use crate::dll::AzOptionPhysicalPositionI32 as OptionPhysicalPositionI32;

    impl std::fmt::Debug for OptionPhysicalPositionI32 { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_physical_position_i32_fmt_debug)(self)) } }
    impl Clone for OptionPhysicalPositionI32 { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_physical_position_i32_deep_copy)(self) } }
    impl Drop for OptionPhysicalPositionI32 { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_physical_position_i32_delete)(self); } }


    /// `OptionWindowIcon` struct
    pub use crate::dll::AzOptionWindowIcon as OptionWindowIcon;

    impl std::fmt::Debug for OptionWindowIcon { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_window_icon_fmt_debug)(self)) } }
    impl Clone for OptionWindowIcon { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_window_icon_deep_copy)(self) } }
    impl Drop for OptionWindowIcon { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_window_icon_delete)(self); } }


    /// `OptionString` struct
    pub use crate::dll::AzOptionString as OptionString;

    impl std::fmt::Debug for OptionString { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_string_fmt_debug)(self)) } }
    impl Clone for OptionString { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_string_deep_copy)(self) } }
    impl Drop for OptionString { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_string_delete)(self); } }


    /// `OptionX11Visual` struct
    pub use crate::dll::AzOptionX11Visual as OptionX11Visual;

    impl std::fmt::Debug for OptionX11Visual { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_x11_visual_fmt_debug)(self)) } }
    impl Clone for OptionX11Visual { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_x11_visual_deep_copy)(self) } }
    impl Drop for OptionX11Visual { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_x11_visual_delete)(self); } }


    /// `OptionI32` struct
    pub use crate::dll::AzOptionI32 as OptionI32;

    impl std::fmt::Debug for OptionI32 { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_i32_fmt_debug)(self)) } }
    impl Clone for OptionI32 { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_i32_deep_copy)(self) } }
    impl Drop for OptionI32 { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_i32_delete)(self); } }


    /// `OptionF32` struct
    pub use crate::dll::AzOptionF32 as OptionF32;

    impl std::fmt::Debug for OptionF32 { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_f32_fmt_debug)(self)) } }
    impl Clone for OptionF32 { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_f32_deep_copy)(self) } }
    impl Drop for OptionF32 { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_f32_delete)(self); } }


    /// `OptionMouseCursorType` struct
    pub use crate::dll::AzOptionMouseCursorType as OptionMouseCursorType;

    impl std::fmt::Debug for OptionMouseCursorType { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_mouse_cursor_type_fmt_debug)(self)) } }
    impl Clone for OptionMouseCursorType { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_mouse_cursor_type_deep_copy)(self) } }
    impl Drop for OptionMouseCursorType { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_mouse_cursor_type_delete)(self); } }


    /// `OptionLogicalSize` struct
    pub use crate::dll::AzOptionLogicalSize as OptionLogicalSize;

    impl std::fmt::Debug for OptionLogicalSize { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_logical_size_fmt_debug)(self)) } }
    impl Clone for OptionLogicalSize { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_logical_size_deep_copy)(self) } }
    impl Drop for OptionLogicalSize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_logical_size_delete)(self); } }


    /// Option<char> but the char is a u32, for C FFI stability reasons
    pub use crate::dll::AzOptionChar as OptionChar;

    impl std::fmt::Debug for OptionChar { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_char_fmt_debug)(self)) } }
    impl Clone for OptionChar { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_char_deep_copy)(self) } }
    impl Drop for OptionChar { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_char_delete)(self); } }


    /// `OptionVirtualKeyCode` struct
    pub use crate::dll::AzOptionVirtualKeyCode as OptionVirtualKeyCode;

    impl std::fmt::Debug for OptionVirtualKeyCode { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_virtual_key_code_fmt_debug)(self)) } }
    impl Clone for OptionVirtualKeyCode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_virtual_key_code_deep_copy)(self) } }
    impl Drop for OptionVirtualKeyCode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_virtual_key_code_delete)(self); } }


    /// `OptionPercentageValue` struct
    pub use crate::dll::AzOptionPercentageValue as OptionPercentageValue;

    impl std::fmt::Debug for OptionPercentageValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_percentage_value_fmt_debug)(self)) } }
    impl Clone for OptionPercentageValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_percentage_value_deep_copy)(self) } }
    impl Drop for OptionPercentageValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_percentage_value_delete)(self); } }


    /// `OptionDom` struct
    pub use crate::dll::AzOptionDom as OptionDom;

    impl std::fmt::Debug for OptionDom { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_dom_fmt_debug)(self)) } }
    impl Clone for OptionDom { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_dom_deep_copy)(self) } }
    impl Drop for OptionDom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_dom_delete)(self); } }


    /// `OptionTexture` struct
    pub use crate::dll::AzOptionTexture as OptionTexture;

    impl std::fmt::Debug for OptionTexture { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_texture_fmt_debug)(self)) } }
    impl Drop for OptionTexture { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_texture_delete)(self); } }


    /// `OptionImageMask` struct
    pub use crate::dll::AzOptionImageMask as OptionImageMask;

    impl std::fmt::Debug for OptionImageMask { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_image_mask_fmt_debug)(self)) } }
    impl Clone for OptionImageMask { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_image_mask_deep_copy)(self) } }
    impl Drop for OptionImageMask { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_image_mask_delete)(self); } }


    /// `OptionTabIndex` struct
    pub use crate::dll::AzOptionTabIndex as OptionTabIndex;

    impl std::fmt::Debug for OptionTabIndex { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_tab_index_fmt_debug)(self)) } }
    impl Clone for OptionTabIndex { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_tab_index_deep_copy)(self) } }
    impl Drop for OptionTabIndex { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_tab_index_delete)(self); } }


    /// `OptionStyleBackgroundContentValue` struct
    pub use crate::dll::AzOptionStyleBackgroundContentValue as OptionStyleBackgroundContentValue;

    impl std::fmt::Debug for OptionStyleBackgroundContentValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_background_content_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBackgroundContentValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_background_content_value_deep_copy)(self) } }
    impl Drop for OptionStyleBackgroundContentValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_background_content_value_delete)(self); } }


    /// `OptionStyleBackgroundPositionValue` struct
    pub use crate::dll::AzOptionStyleBackgroundPositionValue as OptionStyleBackgroundPositionValue;

    impl std::fmt::Debug for OptionStyleBackgroundPositionValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_background_position_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBackgroundPositionValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_background_position_value_deep_copy)(self) } }
    impl Drop for OptionStyleBackgroundPositionValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_background_position_value_delete)(self); } }


    /// `OptionStyleBackgroundSizeValue` struct
    pub use crate::dll::AzOptionStyleBackgroundSizeValue as OptionStyleBackgroundSizeValue;

    impl std::fmt::Debug for OptionStyleBackgroundSizeValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_background_size_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBackgroundSizeValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_background_size_value_deep_copy)(self) } }
    impl Drop for OptionStyleBackgroundSizeValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_background_size_value_delete)(self); } }


    /// `OptionStyleBackgroundRepeatValue` struct
    pub use crate::dll::AzOptionStyleBackgroundRepeatValue as OptionStyleBackgroundRepeatValue;

    impl std::fmt::Debug for OptionStyleBackgroundRepeatValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_background_repeat_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBackgroundRepeatValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_background_repeat_value_deep_copy)(self) } }
    impl Drop for OptionStyleBackgroundRepeatValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_background_repeat_value_delete)(self); } }


    /// `OptionStyleFontSizeValue` struct
    pub use crate::dll::AzOptionStyleFontSizeValue as OptionStyleFontSizeValue;

    impl std::fmt::Debug for OptionStyleFontSizeValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_font_size_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleFontSizeValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_font_size_value_deep_copy)(self) } }
    impl Drop for OptionStyleFontSizeValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_font_size_value_delete)(self); } }


    /// `OptionStyleFontFamilyValue` struct
    pub use crate::dll::AzOptionStyleFontFamilyValue as OptionStyleFontFamilyValue;

    impl std::fmt::Debug for OptionStyleFontFamilyValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_font_family_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleFontFamilyValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_font_family_value_deep_copy)(self) } }
    impl Drop for OptionStyleFontFamilyValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_font_family_value_delete)(self); } }


    /// `OptionStyleTextColorValue` struct
    pub use crate::dll::AzOptionStyleTextColorValue as OptionStyleTextColorValue;

    impl std::fmt::Debug for OptionStyleTextColorValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_text_color_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleTextColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_text_color_value_deep_copy)(self) } }
    impl Drop for OptionStyleTextColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_text_color_value_delete)(self); } }


    /// `OptionStyleTextAlignmentHorzValue` struct
    pub use crate::dll::AzOptionStyleTextAlignmentHorzValue as OptionStyleTextAlignmentHorzValue;

    impl std::fmt::Debug for OptionStyleTextAlignmentHorzValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_text_alignment_horz_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleTextAlignmentHorzValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_text_alignment_horz_value_deep_copy)(self) } }
    impl Drop for OptionStyleTextAlignmentHorzValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_text_alignment_horz_value_delete)(self); } }


    /// `OptionStyleLineHeightValue` struct
    pub use crate::dll::AzOptionStyleLineHeightValue as OptionStyleLineHeightValue;

    impl std::fmt::Debug for OptionStyleLineHeightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_line_height_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleLineHeightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_line_height_value_deep_copy)(self) } }
    impl Drop for OptionStyleLineHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_line_height_value_delete)(self); } }


    /// `OptionStyleLetterSpacingValue` struct
    pub use crate::dll::AzOptionStyleLetterSpacingValue as OptionStyleLetterSpacingValue;

    impl std::fmt::Debug for OptionStyleLetterSpacingValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_letter_spacing_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleLetterSpacingValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_letter_spacing_value_deep_copy)(self) } }
    impl Drop for OptionStyleLetterSpacingValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_letter_spacing_value_delete)(self); } }


    /// `OptionStyleWordSpacingValue` struct
    pub use crate::dll::AzOptionStyleWordSpacingValue as OptionStyleWordSpacingValue;

    impl std::fmt::Debug for OptionStyleWordSpacingValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_word_spacing_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleWordSpacingValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_word_spacing_value_deep_copy)(self) } }
    impl Drop for OptionStyleWordSpacingValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_word_spacing_value_delete)(self); } }


    /// `OptionStyleTabWidthValue` struct
    pub use crate::dll::AzOptionStyleTabWidthValue as OptionStyleTabWidthValue;

    impl std::fmt::Debug for OptionStyleTabWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_tab_width_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleTabWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_tab_width_value_deep_copy)(self) } }
    impl Drop for OptionStyleTabWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_tab_width_value_delete)(self); } }


    /// `OptionStyleCursorValue` struct
    pub use crate::dll::AzOptionStyleCursorValue as OptionStyleCursorValue;

    impl std::fmt::Debug for OptionStyleCursorValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_cursor_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleCursorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_cursor_value_deep_copy)(self) } }
    impl Drop for OptionStyleCursorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_cursor_value_delete)(self); } }


    /// `OptionBoxShadowPreDisplayItemValue` struct
    pub use crate::dll::AzOptionBoxShadowPreDisplayItemValue as OptionBoxShadowPreDisplayItemValue;

    impl std::fmt::Debug for OptionBoxShadowPreDisplayItemValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_box_shadow_pre_display_item_value_fmt_debug)(self)) } }
    impl Clone for OptionBoxShadowPreDisplayItemValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_box_shadow_pre_display_item_value_deep_copy)(self) } }
    impl Drop for OptionBoxShadowPreDisplayItemValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_box_shadow_pre_display_item_value_delete)(self); } }


    /// `OptionStyleBorderTopColorValue` struct
    pub use crate::dll::AzOptionStyleBorderTopColorValue as OptionStyleBorderTopColorValue;

    impl std::fmt::Debug for OptionStyleBorderTopColorValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_top_color_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderTopColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_top_color_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderTopColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_top_color_value_delete)(self); } }


    /// `OptionStyleBorderLeftColorValue` struct
    pub use crate::dll::AzOptionStyleBorderLeftColorValue as OptionStyleBorderLeftColorValue;

    impl std::fmt::Debug for OptionStyleBorderLeftColorValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_left_color_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderLeftColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_left_color_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderLeftColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_left_color_value_delete)(self); } }


    /// `OptionStyleBorderRightColorValue` struct
    pub use crate::dll::AzOptionStyleBorderRightColorValue as OptionStyleBorderRightColorValue;

    impl std::fmt::Debug for OptionStyleBorderRightColorValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_right_color_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderRightColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_right_color_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderRightColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_right_color_value_delete)(self); } }


    /// `OptionStyleBorderBottomColorValue` struct
    pub use crate::dll::AzOptionStyleBorderBottomColorValue as OptionStyleBorderBottomColorValue;

    impl std::fmt::Debug for OptionStyleBorderBottomColorValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_bottom_color_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderBottomColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_bottom_color_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderBottomColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_bottom_color_value_delete)(self); } }


    /// `OptionStyleBorderTopStyleValue` struct
    pub use crate::dll::AzOptionStyleBorderTopStyleValue as OptionStyleBorderTopStyleValue;

    impl std::fmt::Debug for OptionStyleBorderTopStyleValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_top_style_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderTopStyleValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_top_style_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderTopStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_top_style_value_delete)(self); } }


    /// `OptionStyleBorderLeftStyleValue` struct
    pub use crate::dll::AzOptionStyleBorderLeftStyleValue as OptionStyleBorderLeftStyleValue;

    impl std::fmt::Debug for OptionStyleBorderLeftStyleValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_left_style_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderLeftStyleValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_left_style_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderLeftStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_left_style_value_delete)(self); } }


    /// `OptionStyleBorderRightStyleValue` struct
    pub use crate::dll::AzOptionStyleBorderRightStyleValue as OptionStyleBorderRightStyleValue;

    impl std::fmt::Debug for OptionStyleBorderRightStyleValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_right_style_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderRightStyleValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_right_style_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderRightStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_right_style_value_delete)(self); } }


    /// `OptionStyleBorderBottomStyleValue` struct
    pub use crate::dll::AzOptionStyleBorderBottomStyleValue as OptionStyleBorderBottomStyleValue;

    impl std::fmt::Debug for OptionStyleBorderBottomStyleValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_bottom_style_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderBottomStyleValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_bottom_style_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderBottomStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_bottom_style_value_delete)(self); } }


    /// `OptionStyleBorderTopLeftRadiusValue` struct
    pub use crate::dll::AzOptionStyleBorderTopLeftRadiusValue as OptionStyleBorderTopLeftRadiusValue;

    impl std::fmt::Debug for OptionStyleBorderTopLeftRadiusValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_top_left_radius_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderTopLeftRadiusValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_top_left_radius_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderTopLeftRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_top_left_radius_value_delete)(self); } }


    /// `OptionStyleBorderTopRightRadiusValue` struct
    pub use crate::dll::AzOptionStyleBorderTopRightRadiusValue as OptionStyleBorderTopRightRadiusValue;

    impl std::fmt::Debug for OptionStyleBorderTopRightRadiusValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_top_right_radius_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderTopRightRadiusValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_top_right_radius_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderTopRightRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_top_right_radius_value_delete)(self); } }


    /// `OptionStyleBorderBottomLeftRadiusValue` struct
    pub use crate::dll::AzOptionStyleBorderBottomLeftRadiusValue as OptionStyleBorderBottomLeftRadiusValue;

    impl std::fmt::Debug for OptionStyleBorderBottomLeftRadiusValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_bottom_left_radius_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderBottomLeftRadiusValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_bottom_left_radius_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderBottomLeftRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_bottom_left_radius_value_delete)(self); } }


    /// `OptionStyleBorderBottomRightRadiusValue` struct
    pub use crate::dll::AzOptionStyleBorderBottomRightRadiusValue as OptionStyleBorderBottomRightRadiusValue;

    impl std::fmt::Debug for OptionStyleBorderBottomRightRadiusValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_bottom_right_radius_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderBottomRightRadiusValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_bottom_right_radius_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderBottomRightRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_bottom_right_radius_value_delete)(self); } }


    /// `OptionLayoutDisplayValue` struct
    pub use crate::dll::AzOptionLayoutDisplayValue as OptionLayoutDisplayValue;

    impl std::fmt::Debug for OptionLayoutDisplayValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_display_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutDisplayValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_display_value_deep_copy)(self) } }
    impl Drop for OptionLayoutDisplayValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_display_value_delete)(self); } }


    /// `OptionLayoutFloatValue` struct
    pub use crate::dll::AzOptionLayoutFloatValue as OptionLayoutFloatValue;

    impl std::fmt::Debug for OptionLayoutFloatValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_float_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutFloatValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_float_value_deep_copy)(self) } }
    impl Drop for OptionLayoutFloatValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_float_value_delete)(self); } }


    /// `OptionLayoutBoxSizingValue` struct
    pub use crate::dll::AzOptionLayoutBoxSizingValue as OptionLayoutBoxSizingValue;

    impl std::fmt::Debug for OptionLayoutBoxSizingValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_box_sizing_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutBoxSizingValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_box_sizing_value_deep_copy)(self) } }
    impl Drop for OptionLayoutBoxSizingValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_box_sizing_value_delete)(self); } }


    /// `OptionLayoutWidthValue` struct
    pub use crate::dll::AzOptionLayoutWidthValue as OptionLayoutWidthValue;

    impl std::fmt::Debug for OptionLayoutWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_width_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_width_value_deep_copy)(self) } }
    impl Drop for OptionLayoutWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_width_value_delete)(self); } }


    /// `OptionLayoutHeightValue` struct
    pub use crate::dll::AzOptionLayoutHeightValue as OptionLayoutHeightValue;

    impl std::fmt::Debug for OptionLayoutHeightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_height_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutHeightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_height_value_deep_copy)(self) } }
    impl Drop for OptionLayoutHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_height_value_delete)(self); } }


    /// `OptionLayoutMinWidthValue` struct
    pub use crate::dll::AzOptionLayoutMinWidthValue as OptionLayoutMinWidthValue;

    impl std::fmt::Debug for OptionLayoutMinWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_min_width_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutMinWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_min_width_value_deep_copy)(self) } }
    impl Drop for OptionLayoutMinWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_min_width_value_delete)(self); } }


    /// `OptionLayoutMinHeightValue` struct
    pub use crate::dll::AzOptionLayoutMinHeightValue as OptionLayoutMinHeightValue;

    impl std::fmt::Debug for OptionLayoutMinHeightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_min_height_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutMinHeightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_min_height_value_deep_copy)(self) } }
    impl Drop for OptionLayoutMinHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_min_height_value_delete)(self); } }


    /// `OptionLayoutMaxWidthValue` struct
    pub use crate::dll::AzOptionLayoutMaxWidthValue as OptionLayoutMaxWidthValue;

    impl std::fmt::Debug for OptionLayoutMaxWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_max_width_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutMaxWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_max_width_value_deep_copy)(self) } }
    impl Drop for OptionLayoutMaxWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_max_width_value_delete)(self); } }


    /// `OptionLayoutMaxHeightValue` struct
    pub use crate::dll::AzOptionLayoutMaxHeightValue as OptionLayoutMaxHeightValue;

    impl std::fmt::Debug for OptionLayoutMaxHeightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_max_height_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutMaxHeightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_max_height_value_deep_copy)(self) } }
    impl Drop for OptionLayoutMaxHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_max_height_value_delete)(self); } }


    /// `OptionLayoutPositionValue` struct
    pub use crate::dll::AzOptionLayoutPositionValue as OptionLayoutPositionValue;

    impl std::fmt::Debug for OptionLayoutPositionValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_position_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutPositionValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_position_value_deep_copy)(self) } }
    impl Drop for OptionLayoutPositionValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_position_value_delete)(self); } }


    /// `OptionLayoutTopValue` struct
    pub use crate::dll::AzOptionLayoutTopValue as OptionLayoutTopValue;

    impl std::fmt::Debug for OptionLayoutTopValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_top_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutTopValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_top_value_deep_copy)(self) } }
    impl Drop for OptionLayoutTopValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_top_value_delete)(self); } }


    /// `OptionLayoutBottomValue` struct
    pub use crate::dll::AzOptionLayoutBottomValue as OptionLayoutBottomValue;

    impl std::fmt::Debug for OptionLayoutBottomValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_bottom_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutBottomValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_bottom_value_deep_copy)(self) } }
    impl Drop for OptionLayoutBottomValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_bottom_value_delete)(self); } }


    /// `OptionLayoutRightValue` struct
    pub use crate::dll::AzOptionLayoutRightValue as OptionLayoutRightValue;

    impl std::fmt::Debug for OptionLayoutRightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_right_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutRightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_right_value_deep_copy)(self) } }
    impl Drop for OptionLayoutRightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_right_value_delete)(self); } }


    /// `OptionLayoutLeftValue` struct
    pub use crate::dll::AzOptionLayoutLeftValue as OptionLayoutLeftValue;

    impl std::fmt::Debug for OptionLayoutLeftValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_left_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutLeftValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_left_value_deep_copy)(self) } }
    impl Drop for OptionLayoutLeftValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_left_value_delete)(self); } }


    /// `OptionLayoutPaddingTopValue` struct
    pub use crate::dll::AzOptionLayoutPaddingTopValue as OptionLayoutPaddingTopValue;

    impl std::fmt::Debug for OptionLayoutPaddingTopValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_padding_top_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutPaddingTopValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_padding_top_value_deep_copy)(self) } }
    impl Drop for OptionLayoutPaddingTopValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_padding_top_value_delete)(self); } }


    /// `OptionLayoutPaddingBottomValue` struct
    pub use crate::dll::AzOptionLayoutPaddingBottomValue as OptionLayoutPaddingBottomValue;

    impl std::fmt::Debug for OptionLayoutPaddingBottomValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_padding_bottom_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutPaddingBottomValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_padding_bottom_value_deep_copy)(self) } }
    impl Drop for OptionLayoutPaddingBottomValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_padding_bottom_value_delete)(self); } }


    /// `OptionLayoutPaddingLeftValue` struct
    pub use crate::dll::AzOptionLayoutPaddingLeftValue as OptionLayoutPaddingLeftValue;

    impl std::fmt::Debug for OptionLayoutPaddingLeftValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_padding_left_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutPaddingLeftValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_padding_left_value_deep_copy)(self) } }
    impl Drop for OptionLayoutPaddingLeftValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_padding_left_value_delete)(self); } }


    /// `OptionLayoutPaddingRightValue` struct
    pub use crate::dll::AzOptionLayoutPaddingRightValue as OptionLayoutPaddingRightValue;

    impl std::fmt::Debug for OptionLayoutPaddingRightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_padding_right_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutPaddingRightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_padding_right_value_deep_copy)(self) } }
    impl Drop for OptionLayoutPaddingRightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_padding_right_value_delete)(self); } }


    /// `OptionLayoutMarginTopValue` struct
    pub use crate::dll::AzOptionLayoutMarginTopValue as OptionLayoutMarginTopValue;

    impl std::fmt::Debug for OptionLayoutMarginTopValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_margin_top_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutMarginTopValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_margin_top_value_deep_copy)(self) } }
    impl Drop for OptionLayoutMarginTopValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_margin_top_value_delete)(self); } }


    /// `OptionLayoutMarginBottomValue` struct
    pub use crate::dll::AzOptionLayoutMarginBottomValue as OptionLayoutMarginBottomValue;

    impl std::fmt::Debug for OptionLayoutMarginBottomValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_margin_bottom_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutMarginBottomValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_margin_bottom_value_deep_copy)(self) } }
    impl Drop for OptionLayoutMarginBottomValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_margin_bottom_value_delete)(self); } }


    /// `OptionLayoutMarginLeftValue` struct
    pub use crate::dll::AzOptionLayoutMarginLeftValue as OptionLayoutMarginLeftValue;

    impl std::fmt::Debug for OptionLayoutMarginLeftValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_margin_left_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutMarginLeftValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_margin_left_value_deep_copy)(self) } }
    impl Drop for OptionLayoutMarginLeftValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_margin_left_value_delete)(self); } }


    /// `OptionLayoutMarginRightValue` struct
    pub use crate::dll::AzOptionLayoutMarginRightValue as OptionLayoutMarginRightValue;

    impl std::fmt::Debug for OptionLayoutMarginRightValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_margin_right_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutMarginRightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_margin_right_value_deep_copy)(self) } }
    impl Drop for OptionLayoutMarginRightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_margin_right_value_delete)(self); } }


    /// `OptionStyleBorderTopWidthValue` struct
    pub use crate::dll::AzOptionStyleBorderTopWidthValue as OptionStyleBorderTopWidthValue;

    impl std::fmt::Debug for OptionStyleBorderTopWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_top_width_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderTopWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_top_width_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderTopWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_top_width_value_delete)(self); } }


    /// `OptionStyleBorderLeftWidthValue` struct
    pub use crate::dll::AzOptionStyleBorderLeftWidthValue as OptionStyleBorderLeftWidthValue;

    impl std::fmt::Debug for OptionStyleBorderLeftWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_left_width_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderLeftWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_left_width_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderLeftWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_left_width_value_delete)(self); } }


    /// `OptionStyleBorderRightWidthValue` struct
    pub use crate::dll::AzOptionStyleBorderRightWidthValue as OptionStyleBorderRightWidthValue;

    impl std::fmt::Debug for OptionStyleBorderRightWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_right_width_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderRightWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_right_width_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderRightWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_right_width_value_delete)(self); } }


    /// `OptionStyleBorderBottomWidthValue` struct
    pub use crate::dll::AzOptionStyleBorderBottomWidthValue as OptionStyleBorderBottomWidthValue;

    impl std::fmt::Debug for OptionStyleBorderBottomWidthValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_style_border_bottom_width_value_fmt_debug)(self)) } }
    impl Clone for OptionStyleBorderBottomWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_style_border_bottom_width_value_deep_copy)(self) } }
    impl Drop for OptionStyleBorderBottomWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_style_border_bottom_width_value_delete)(self); } }


    /// `OptionOverflowValue` struct
    pub use crate::dll::AzOptionOverflowValue as OptionOverflowValue;

    impl std::fmt::Debug for OptionOverflowValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_overflow_value_fmt_debug)(self)) } }
    impl Clone for OptionOverflowValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_overflow_value_deep_copy)(self) } }
    impl Drop for OptionOverflowValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_overflow_value_delete)(self); } }


    /// `OptionLayoutDirectionValue` struct
    pub use crate::dll::AzOptionLayoutDirectionValue as OptionLayoutDirectionValue;

    impl std::fmt::Debug for OptionLayoutDirectionValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_direction_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutDirectionValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_direction_value_deep_copy)(self) } }
    impl Drop for OptionLayoutDirectionValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_direction_value_delete)(self); } }


    /// `OptionLayoutWrapValue` struct
    pub use crate::dll::AzOptionLayoutWrapValue as OptionLayoutWrapValue;

    impl std::fmt::Debug for OptionLayoutWrapValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_wrap_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutWrapValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_wrap_value_deep_copy)(self) } }
    impl Drop for OptionLayoutWrapValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_wrap_value_delete)(self); } }


    /// `OptionLayoutFlexGrowValue` struct
    pub use crate::dll::AzOptionLayoutFlexGrowValue as OptionLayoutFlexGrowValue;

    impl std::fmt::Debug for OptionLayoutFlexGrowValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_flex_grow_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutFlexGrowValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_flex_grow_value_deep_copy)(self) } }
    impl Drop for OptionLayoutFlexGrowValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_flex_grow_value_delete)(self); } }


    /// `OptionLayoutFlexShrinkValue` struct
    pub use crate::dll::AzOptionLayoutFlexShrinkValue as OptionLayoutFlexShrinkValue;

    impl std::fmt::Debug for OptionLayoutFlexShrinkValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_flex_shrink_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutFlexShrinkValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_flex_shrink_value_deep_copy)(self) } }
    impl Drop for OptionLayoutFlexShrinkValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_flex_shrink_value_delete)(self); } }


    /// `OptionLayoutJustifyContentValue` struct
    pub use crate::dll::AzOptionLayoutJustifyContentValue as OptionLayoutJustifyContentValue;

    impl std::fmt::Debug for OptionLayoutJustifyContentValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_justify_content_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutJustifyContentValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_justify_content_value_deep_copy)(self) } }
    impl Drop for OptionLayoutJustifyContentValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_justify_content_value_delete)(self); } }


    /// `OptionLayoutAlignItemsValue` struct
    pub use crate::dll::AzOptionLayoutAlignItemsValue as OptionLayoutAlignItemsValue;

    impl std::fmt::Debug for OptionLayoutAlignItemsValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_align_items_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutAlignItemsValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_align_items_value_deep_copy)(self) } }
    impl Drop for OptionLayoutAlignItemsValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_align_items_value_delete)(self); } }


    /// `OptionLayoutAlignContentValue` struct
    pub use crate::dll::AzOptionLayoutAlignContentValue as OptionLayoutAlignContentValue;

    impl std::fmt::Debug for OptionLayoutAlignContentValue { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_layout_align_content_value_fmt_debug)(self)) } }
    impl Clone for OptionLayoutAlignContentValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_layout_align_content_value_deep_copy)(self) } }
    impl Drop for OptionLayoutAlignContentValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_layout_align_content_value_delete)(self); } }


    /// `OptionHoverGroup` struct
    pub use crate::dll::AzOptionHoverGroup as OptionHoverGroup;

    impl std::fmt::Debug for OptionHoverGroup { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_hover_group_fmt_debug)(self)) } }
    impl Clone for OptionHoverGroup { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_hover_group_deep_copy)(self) } }
    impl Drop for OptionHoverGroup { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_hover_group_delete)(self); } }


    /// `OptionTagId` struct
    pub use crate::dll::AzOptionTagId as OptionTagId;

    impl std::fmt::Debug for OptionTagId { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_tag_id_fmt_debug)(self)) } }
    impl Clone for OptionTagId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_tag_id_deep_copy)(self) } }
    impl Drop for OptionTagId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_tag_id_delete)(self); } }


    /// `OptionDuration` struct
    pub use crate::dll::AzOptionDuration as OptionDuration;

    impl std::fmt::Debug for OptionDuration { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_duration_fmt_debug)(self)) } }
    impl Clone for OptionDuration { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_duration_deep_copy)(self) } }
    impl Drop for OptionDuration { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_duration_delete)(self); } }


    /// `OptionInstantPtr` struct
    pub use crate::dll::AzOptionInstantPtr as OptionInstantPtr;

    impl std::fmt::Debug for OptionInstantPtr { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_instant_ptr_fmt_debug)(self)) } }
    impl Clone for OptionInstantPtr { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_instant_ptr_deep_copy)(self) } }
    impl Drop for OptionInstantPtr { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_instant_ptr_delete)(self); } }


    /// `OptionUsize` struct
    pub use crate::dll::AzOptionUsize as OptionUsize;

    impl std::fmt::Debug for OptionUsize { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_usize_fmt_debug)(self)) } }
    impl Clone for OptionUsize { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_usize_deep_copy)(self) } }
    impl Drop for OptionUsize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_usize_delete)(self); } }


    /// `OptionU8VecRef` struct
    pub use crate::dll::AzOptionU8VecRef as OptionU8VecRef;

    impl std::fmt::Debug for OptionU8VecRef { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_option_u8_vec_ref_fmt_debug)(self)) } }
    impl Drop for OptionU8VecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_u8_vec_ref_delete)(self); } }
