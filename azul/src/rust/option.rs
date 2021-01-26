    #![allow(dead_code, unused_imports)]
    //! Definition of azuls internal `Option<*>` wrappers
    use crate::dll::*;
    use core::ffi::c_void;
    use crate::dll::*;

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
            pub fn into_option(self) -> Option<$struct_type> {
                self.into()
            }
            pub fn replace(&mut self, value: $struct_type) -> $struct_name {
                ::core::mem::replace(self, $struct_name::Some(value))
            }
            pub const fn is_some(&self) -> bool {
                match self {
                    $struct_name::None => false,
                    $struct_name::Some(_) => true,
                }
            }
            pub const fn is_none(&self) -> bool {
                !self.is_some()
            }
            pub const fn as_ref(&self) -> Option<&$struct_type> {
                match *self {
                    $struct_name::Some(ref x) => Some(x),
                    $struct_name::None => None,
                }
            }

            pub fn map<U, F: FnOnce($struct_type) -> U>(self, f: F) -> Option<U> {
                match self.into_option() {
                    None => None,
                    Some(s) => Some(f(s)),
                }
            }

            pub fn and_then<U, F>(self, f: F) -> Option<U> where F: FnOnce($struct_type) -> Option<U> {
                match self.into_option() {
                    None => None,
                    Some(s) => f(s),
                }
            }
        }
    )}

    macro_rules! impl_option {
        ($struct_type:ident, $struct_name:ident, copy = false, clone = false, [$($derive:meta),* ]) => (
            impl_option_inner!($struct_type, $struct_name);

            impl From<$struct_name> for Option<$struct_type> {
                fn from(mut o: $struct_name) -> Option<$struct_type> {
                    // we need to the the Some(t) out without dropping the t value
                    let res = match &mut o {
                        $struct_name::None => { None },
                        $struct_name::Some(t) => {
                            let uninitialized = unsafe{ core::mem::zeroed::<$struct_type>() };
                            let t = core::mem::replace(t, uninitialized);
                            Some(t)
                        },
                    };

                    core::mem::forget(o); // do not run the destructor

                    res
                }
            }

            impl From<Option<$struct_type>> for $struct_name {
                fn from(mut o: Option<$struct_type>) -> $struct_name {

                    // we need to the the Some(t) out without dropping the t value
                    let res = match &mut o {
                        None => { $struct_name::None },
                        Some(t) => {
                            let uninitialized = unsafe{ core::mem::zeroed::<$struct_type>() };
                            let t = core::mem::replace(t, uninitialized);
                            $struct_name::Some(t)
                        },
                    };

                    core::mem::forget(o); // do not run the destructor

                    res
                }
            }
        );
        ($struct_type:ident, $struct_name:ident, copy = false, [$($derive:meta),* ]) => (
            impl_option_inner!($struct_type, $struct_name);

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
        );
        ($struct_type:ident, $struct_name:ident, [$($derive:meta),* ]) => (
            impl_option_inner!($struct_type, $struct_name);

            impl From<$struct_name> for Option<$struct_type> {
                fn from(o: $struct_name) -> Option<$struct_type> {
                    match o {
                        $struct_name::None => None,
                        $struct_name::Some(t) => Some(t),
                    }
                }
            }

            impl From<Option<$struct_type>> for $struct_name {
                fn from(o: Option<$struct_type>) -> $struct_name {
                    match o {
                        None => $struct_name::None,
                        Some(t) => $struct_name::Some(t),
                    }
                }
            }
        );
    }

    pub type AzX11Visual = *const c_void;
    pub type AzHwndHandle = *mut c_void;

    impl_option!(i32, AzOptionI32, [Debug, Copy, Clone]);
    impl_option!(f32, AzOptionF32, [Debug, Copy, Clone]);
    impl_option!(usize, AzOptionUsize, [Debug, Copy, Clone]);
    impl_option!(u32, AzOptionChar, [Debug, Copy, Clone]);

    impl_option!(AzThreadSendMsg, AzOptionThreadSendMsg, [Debug, Copy, Clone]);
    impl_option!(AzLayoutRect, AzOptionLayoutRect, [Debug, Copy, Clone]);
    impl_option!(AzRefAny, AzOptionRefAny, copy = false, [Debug, Clone]);
    impl_option!(AzLayoutPoint, AzOptionLayoutPoint, [Debug, Copy, Clone]);
    impl_option!(AzWindowTheme, AzOptionWindowTheme, [Debug, Copy, Clone]);
    impl_option!(AzNodeId, AzOptionNodeId, [Debug, Copy, Clone]);
    impl_option!(AzDomNodeId, AzOptionDomNodeId, [Debug, Copy, Clone]);
    impl_option!(AzColorU, AzOptionColorU, [Debug, Copy, Clone]);
    impl_option!(AzRawImage, AzOptionRawImage, copy = false, [Debug, Clone]);
    impl_option!(AzSvgDashPattern, AzOptionSvgDashPattern, [Debug, Copy, Clone]);
    impl_option!(AzWaylandTheme, AzOptionWaylandTheme, copy = false, [Debug, Clone]);
    impl_option!(AzTaskBarIcon, AzOptionTaskBarIcon, copy = false, [Debug, Clone]);
    impl_option!(AzLogicalPosition, AzOptionLogicalPosition, [Debug, Copy, Clone]);
    impl_option!(AzPhysicalPositionI32, AzOptionPhysicalPositionI32, [Debug, Copy, Clone]);
    impl_option!(AzWindowIcon, AzOptionWindowIcon, copy = false, [Debug, Clone]);
    impl_option!(AzString, AzOptionString, copy = false, [Debug, Clone]);
    impl_option!(AzMouseCursorType, AzOptionMouseCursorType, [Debug, Copy, Clone]);
    impl_option!(AzLogicalSize, AzOptionLogicalSize, [Debug, Copy, Clone]);
    impl_option!(AzVirtualKeyCode, AzOptionVirtualKeyCode, [Debug, Copy, Clone]);
    impl_option!(AzPercentageValue, AzOptionPercentageValue, [Debug, Copy, Clone]);
    impl_option!(AzDom, AzOptionDom, copy = false, [Debug, Clone]);
    impl_option!(AzTexture, AzOptionTexture, copy = false, clone = false, [Debug]);
    impl_option!(AzImageMask, AzOptionImageMask, copy = false, [Debug, Clone]);
    impl_option!(AzTabIndex, AzOptionTabIndex, [Debug, Copy, Clone]);
    impl_option!(AzCallback, AzOptionCallback, [Debug, Copy, Clone]);
    impl_option!(AzTagId, AzOptionTagId, [Debug, Copy, Clone]);
    impl_option!(AzDuration, AzOptionDuration, [Debug, Copy, Clone]);
    impl_option!(AzInstantPtr, AzOptionInstantPtr, copy = false, clone = false, [Debug]); // TODO: impl clone!
    impl_option!(AzU8VecRef, AzOptionU8VecRef, copy = false, clone = false, [Debug]);


    /// `OptionPercentageValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionPercentageValue as OptionPercentageValue;

    impl Clone for OptionPercentageValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionPercentageValue { }


    /// `OptionAngleValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionAngleValue as OptionAngleValue;

    impl Clone for OptionAngleValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionAngleValue { }


    /// `OptionRendererOptions` struct
    #[doc(inline)] pub use crate::dll::AzOptionRendererOptions as OptionRendererOptions;

    impl Clone for OptionRendererOptions { fn clone(&self) -> Self { *self } }
    impl Copy for OptionRendererOptions { }


    /// `OptionCallback` struct
    #[doc(inline)] pub use crate::dll::AzOptionCallback as OptionCallback;

    impl Clone for OptionCallback { fn clone(&self) -> Self { *self } }
    impl Copy for OptionCallback { }


    /// `OptionThreadSendMsg` struct
    #[doc(inline)] pub use crate::dll::AzOptionThreadSendMsg as OptionThreadSendMsg;

    impl Clone for OptionThreadSendMsg { fn clone(&self) -> Self { *self } }
    impl Copy for OptionThreadSendMsg { }


    /// `OptionLayoutRect` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutRect as OptionLayoutRect;

    impl Clone for OptionLayoutRect { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutRect { }


    /// `OptionRefAny` struct
    #[doc(inline)] pub use crate::dll::AzOptionRefAny as OptionRefAny;

    impl Clone for OptionRefAny { fn clone(&self) -> Self { unsafe { crate::dll::az_option_ref_any_deep_copy(self) } } }
    impl Drop for OptionRefAny { fn drop(&mut self) { unsafe { crate::dll::az_option_ref_any_delete(self) }; } }


    /// `OptionLayoutPoint` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutPoint as OptionLayoutPoint;

    impl Clone for OptionLayoutPoint { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutPoint { }


    /// `OptionWindowTheme` struct
    #[doc(inline)] pub use crate::dll::AzOptionWindowTheme as OptionWindowTheme;

    impl Clone for OptionWindowTheme { fn clone(&self) -> Self { *self } }
    impl Copy for OptionWindowTheme { }


    /// `OptionNodeId` struct
    #[doc(inline)] pub use crate::dll::AzOptionNodeId as OptionNodeId;

    impl Clone for OptionNodeId { fn clone(&self) -> Self { *self } }
    impl Copy for OptionNodeId { }


    /// `OptionDomNodeId` struct
    #[doc(inline)] pub use crate::dll::AzOptionDomNodeId as OptionDomNodeId;

    impl Clone for OptionDomNodeId { fn clone(&self) -> Self { *self } }
    impl Copy for OptionDomNodeId { }


    /// `OptionColorU` struct
    #[doc(inline)] pub use crate::dll::AzOptionColorU as OptionColorU;

    impl Clone for OptionColorU { fn clone(&self) -> Self { *self } }
    impl Copy for OptionColorU { }


    /// `OptionRawImage` struct
    #[doc(inline)] pub use crate::dll::AzOptionRawImage as OptionRawImage;

    impl Clone for OptionRawImage { fn clone(&self) -> Self { unsafe { crate::dll::az_option_raw_image_deep_copy(self) } } }
    impl Drop for OptionRawImage { fn drop(&mut self) { unsafe { crate::dll::az_option_raw_image_delete(self) }; } }


    /// `OptionSvgDashPattern` struct
    #[doc(inline)] pub use crate::dll::AzOptionSvgDashPattern as OptionSvgDashPattern;

    impl Clone for OptionSvgDashPattern { fn clone(&self) -> Self { *self } }
    impl Copy for OptionSvgDashPattern { }


    /// `OptionWaylandTheme` struct
    #[doc(inline)] pub use crate::dll::AzOptionWaylandTheme as OptionWaylandTheme;

    impl Clone for OptionWaylandTheme { fn clone(&self) -> Self { unsafe { crate::dll::az_option_wayland_theme_deep_copy(self) } } }
    impl Drop for OptionWaylandTheme { fn drop(&mut self) { unsafe { crate::dll::az_option_wayland_theme_delete(self) }; } }


    /// `OptionTaskBarIcon` struct
    #[doc(inline)] pub use crate::dll::AzOptionTaskBarIcon as OptionTaskBarIcon;

    impl Clone for OptionTaskBarIcon { fn clone(&self) -> Self { unsafe { crate::dll::az_option_task_bar_icon_deep_copy(self) } } }
    impl Drop for OptionTaskBarIcon { fn drop(&mut self) { unsafe { crate::dll::az_option_task_bar_icon_delete(self) }; } }


    /// `OptionHwndHandle` struct
    #[doc(inline)] pub use crate::dll::AzOptionHwndHandle as OptionHwndHandle;

    impl Clone for OptionHwndHandle { fn clone(&self) -> Self { *self } }
    impl Copy for OptionHwndHandle { }


    /// `OptionLogicalPosition` struct
    #[doc(inline)] pub use crate::dll::AzOptionLogicalPosition as OptionLogicalPosition;

    impl Clone for OptionLogicalPosition { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLogicalPosition { }


    /// `OptionPhysicalPositionI32` struct
    #[doc(inline)] pub use crate::dll::AzOptionPhysicalPositionI32 as OptionPhysicalPositionI32;

    impl Clone for OptionPhysicalPositionI32 { fn clone(&self) -> Self { *self } }
    impl Copy for OptionPhysicalPositionI32 { }


    /// `OptionWindowIcon` struct
    #[doc(inline)] pub use crate::dll::AzOptionWindowIcon as OptionWindowIcon;

    impl Clone for OptionWindowIcon { fn clone(&self) -> Self { unsafe { crate::dll::az_option_window_icon_deep_copy(self) } } }
    impl Drop for OptionWindowIcon { fn drop(&mut self) { unsafe { crate::dll::az_option_window_icon_delete(self) }; } }


    /// `OptionString` struct
    #[doc(inline)] pub use crate::dll::AzOptionString as OptionString;

    impl Clone for OptionString { fn clone(&self) -> Self { unsafe { crate::dll::az_option_string_deep_copy(self) } } }
    impl Drop for OptionString { fn drop(&mut self) { unsafe { crate::dll::az_option_string_delete(self) }; } }


    /// `OptionX11Visual` struct
    #[doc(inline)] pub use crate::dll::AzOptionX11Visual as OptionX11Visual;

    impl Clone for OptionX11Visual { fn clone(&self) -> Self { *self } }
    impl Copy for OptionX11Visual { }


    /// `OptionI32` struct
    #[doc(inline)] pub use crate::dll::AzOptionI32 as OptionI32;

    impl Clone for OptionI32 { fn clone(&self) -> Self { *self } }
    impl Copy for OptionI32 { }


    /// `OptionF32` struct
    #[doc(inline)] pub use crate::dll::AzOptionF32 as OptionF32;

    impl Clone for OptionF32 { fn clone(&self) -> Self { *self } }
    impl Copy for OptionF32 { }


    /// `OptionMouseCursorType` struct
    #[doc(inline)] pub use crate::dll::AzOptionMouseCursorType as OptionMouseCursorType;

    impl Clone for OptionMouseCursorType { fn clone(&self) -> Self { *self } }
    impl Copy for OptionMouseCursorType { }


    /// `OptionLogicalSize` struct
    #[doc(inline)] pub use crate::dll::AzOptionLogicalSize as OptionLogicalSize;

    impl Clone for OptionLogicalSize { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLogicalSize { }


    /// Option<char> but the char is a u32, for C FFI stability reasons
    #[doc(inline)] pub use crate::dll::AzOptionChar as OptionChar;

    impl Clone for OptionChar { fn clone(&self) -> Self { *self } }
    impl Copy for OptionChar { }


    /// `OptionVirtualKeyCode` struct
    #[doc(inline)] pub use crate::dll::AzOptionVirtualKeyCode as OptionVirtualKeyCode;

    impl Clone for OptionVirtualKeyCode { fn clone(&self) -> Self { *self } }
    impl Copy for OptionVirtualKeyCode { }


    /// `OptionDom` struct
    #[doc(inline)] pub use crate::dll::AzOptionDom as OptionDom;

    impl Clone for OptionDom { fn clone(&self) -> Self { unsafe { crate::dll::az_option_dom_deep_copy(self) } } }
    impl Drop for OptionDom { fn drop(&mut self) { unsafe { crate::dll::az_option_dom_delete(self) }; } }


    /// `OptionTexture` struct
    #[doc(inline)] pub use crate::dll::AzOptionTexture as OptionTexture;

    impl Drop for OptionTexture { fn drop(&mut self) { unsafe { crate::dll::az_option_texture_delete(self) }; } }


    /// `OptionImageMask` struct
    #[doc(inline)] pub use crate::dll::AzOptionImageMask as OptionImageMask;

    impl Clone for OptionImageMask { fn clone(&self) -> Self { unsafe { crate::dll::az_option_image_mask_deep_copy(self) } } }
    impl Drop for OptionImageMask { fn drop(&mut self) { unsafe { crate::dll::az_option_image_mask_delete(self) }; } }


    /// `OptionTabIndex` struct
    #[doc(inline)] pub use crate::dll::AzOptionTabIndex as OptionTabIndex;

    impl Clone for OptionTabIndex { fn clone(&self) -> Self { *self } }
    impl Copy for OptionTabIndex { }


    /// `OptionTagId` struct
    #[doc(inline)] pub use crate::dll::AzOptionTagId as OptionTagId;

    impl Clone for OptionTagId { fn clone(&self) -> Self { *self } }
    impl Copy for OptionTagId { }


    /// `OptionDuration` struct
    #[doc(inline)] pub use crate::dll::AzOptionDuration as OptionDuration;

    impl Clone for OptionDuration { fn clone(&self) -> Self { *self } }
    impl Copy for OptionDuration { }


    /// `OptionInstant` struct
    #[doc(inline)] pub use crate::dll::AzOptionInstant as OptionInstant;

    impl Clone for OptionInstant { fn clone(&self) -> Self { unsafe { crate::dll::az_option_instant_deep_copy(self) } } }
    impl Drop for OptionInstant { fn drop(&mut self) { unsafe { crate::dll::az_option_instant_delete(self) }; } }


    /// `OptionUsize` struct
    #[doc(inline)] pub use crate::dll::AzOptionUsize as OptionUsize;

    impl Clone for OptionUsize { fn clone(&self) -> Self { *self } }
    impl Copy for OptionUsize { }


    /// `OptionU8VecRef` struct
    #[doc(inline)] pub use crate::dll::AzOptionU8VecRef as OptionU8VecRef;

    impl Drop for OptionU8VecRef { fn drop(&mut self) { unsafe { crate::dll::az_option_u8_vec_ref_delete(self) }; } }
