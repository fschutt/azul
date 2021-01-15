    #![allow(dead_code, unused_imports)]
    //! Definition of azuls internal `Option<*>` wrappers
    use crate::dll::*;
    use std::ffi::c_void;
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
                ::std::mem::replace(self, $struct_name::Some(value))
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
                            let uninitialized = unsafe{ std::mem::zeroed::<$struct_type>() };
                            let t = std::mem::replace(t, uninitialized);
                            Some(t)
                        },
                    };

                    std::mem::forget(o); // do not run the destructor

                    res
                }
            }

            impl From<Option<$struct_type>> for $struct_name {
                fn from(mut o: Option<$struct_type>) -> $struct_name {

                    // we need to the the Some(t) out without dropping the t value
                    let res = match &mut o {
                        None => { $struct_name::None },
                        Some(t) => {
                            let uninitialized = unsafe{ std::mem::zeroed::<$struct_type>() };
                            let t = std::mem::replace(t, uninitialized);
                            $struct_name::Some(t)
                        },
                    };

                    std::mem::forget(o); // do not run the destructor

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
    impl_option!(AzStyleOpacityValue, AzOptionStyleOpacityValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleTransformVecValue, AzOptionStyleTransformVecValue, copy = false, [Debug, Clone]);
    impl_option!(AzStyleTransformOriginValue, AzOptionStyleTransformOriginValue, [Debug, Copy, Clone]);
    impl_option!(AzStylePerspectiveOriginValue, AzOptionStylePerspectiveOriginValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBackfaceVisibilityValue, AzOptionStyleBackfaceVisibilityValue, [Debug, Copy, Clone]);
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
    impl_option!(AzStyleBackgroundContentValue, AzOptionStyleBackgroundContentValue, copy = false, [Debug, Clone]);
    impl_option!(AzStyleBackgroundPositionValue, AzOptionStyleBackgroundPositionValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBackgroundSizeValue, AzOptionStyleBackgroundSizeValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBackgroundRepeatValue, AzOptionStyleBackgroundRepeatValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleFontSizeValue, AzOptionStyleFontSizeValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleFontFamilyValue, AzOptionStyleFontFamilyValue, copy = false, [Debug, Clone]);
    impl_option!(AzStyleTextColorValue, AzOptionStyleTextColorValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleTextAlignmentHorzValue, AzOptionStyleTextAlignmentHorzValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleLineHeightValue, AzOptionStyleLineHeightValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleLetterSpacingValue, AzOptionStyleLetterSpacingValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleWordSpacingValue, AzOptionStyleWordSpacingValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleTabWidthValue, AzOptionStyleTabWidthValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleCursorValue, AzOptionStyleCursorValue, [Debug, Copy, Clone]);
    impl_option!(AzBoxShadowPreDisplayItemValue, AzOptionBoxShadowPreDisplayItemValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderTopColorValue, AzOptionStyleBorderTopColorValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderLeftColorValue, AzOptionStyleBorderLeftColorValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderRightColorValue, AzOptionStyleBorderRightColorValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderBottomColorValue, AzOptionStyleBorderBottomColorValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderTopStyleValue, AzOptionStyleBorderTopStyleValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderLeftStyleValue, AzOptionStyleBorderLeftStyleValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderRightStyleValue, AzOptionStyleBorderRightStyleValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderBottomStyleValue, AzOptionStyleBorderBottomStyleValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderTopLeftRadiusValue, AzOptionStyleBorderTopLeftRadiusValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderTopRightRadiusValue, AzOptionStyleBorderTopRightRadiusValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderBottomLeftRadiusValue, AzOptionStyleBorderBottomLeftRadiusValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderBottomRightRadiusValue, AzOptionStyleBorderBottomRightRadiusValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutDisplayValue, AzOptionLayoutDisplayValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutFloatValue, AzOptionLayoutFloatValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutBoxSizingValue, AzOptionLayoutBoxSizingValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutWidthValue, AzOptionLayoutWidthValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutHeightValue, AzOptionLayoutHeightValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutMinWidthValue, AzOptionLayoutMinWidthValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutMinHeightValue, AzOptionLayoutMinHeightValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutMaxWidthValue, AzOptionLayoutMaxWidthValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutMaxHeightValue, AzOptionLayoutMaxHeightValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutPositionValue, AzOptionLayoutPositionValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutTopValue, AzOptionLayoutTopValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutBottomValue, AzOptionLayoutBottomValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutRightValue, AzOptionLayoutRightValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutLeftValue, AzOptionLayoutLeftValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutPaddingTopValue, AzOptionLayoutPaddingTopValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutPaddingBottomValue, AzOptionLayoutPaddingBottomValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutPaddingLeftValue, AzOptionLayoutPaddingLeftValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutPaddingRightValue, AzOptionLayoutPaddingRightValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutMarginTopValue, AzOptionLayoutMarginTopValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutMarginBottomValue, AzOptionLayoutMarginBottomValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutMarginLeftValue, AzOptionLayoutMarginLeftValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutMarginRightValue, AzOptionLayoutMarginRightValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderTopWidthValue, AzOptionStyleBorderTopWidthValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderLeftWidthValue, AzOptionStyleBorderLeftWidthValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderRightWidthValue, AzOptionStyleBorderRightWidthValue, [Debug, Copy, Clone]);
    impl_option!(AzStyleBorderBottomWidthValue, AzOptionStyleBorderBottomWidthValue, [Debug, Copy, Clone]);
    impl_option!(AzOverflowValue, AzOptionOverflowValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutFlexDirectionValue, AzOptionLayoutFlexDirectionValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutWrapValue, AzOptionLayoutWrapValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutFlexGrowValue, AzOptionLayoutFlexGrowValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutFlexShrinkValue, AzOptionLayoutFlexShrinkValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutJustifyContentValue, AzOptionLayoutJustifyContentValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutAlignItemsValue, AzOptionLayoutAlignItemsValue, [Debug, Copy, Clone]);
    impl_option!(AzLayoutAlignContentValue, AzOptionLayoutAlignContentValue, [Debug, Copy, Clone]);
    impl_option!(AzCallback, AzOptionCallback, [Debug, Copy, Clone]);
    impl_option!(AzTagId, AzOptionTagId, [Debug, Copy, Clone]);
    impl_option!(AzDuration, AzOptionDuration, [Debug, Copy, Clone]);
    impl_option!(AzInstantPtr, AzOptionInstantPtr, copy = false, clone = false, [Debug]); // TODO: impl clone!
    impl_option!(AzU8VecRef, AzOptionU8VecRef, copy = false, clone = false, [Debug]);


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


    /// `OptionStyleOpacityValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleOpacityValue as OptionStyleOpacityValue;

    impl Clone for OptionStyleOpacityValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleOpacityValue { }


    /// `OptionStyleTransformVecValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleTransformVecValue as OptionStyleTransformVecValue;

    impl Clone for OptionStyleTransformVecValue { fn clone(&self) -> Self { unsafe { crate::dll::az_option_style_transform_vec_value_deep_copy(self) } } }
    impl Drop for OptionStyleTransformVecValue { fn drop(&mut self) { unsafe { crate::dll::az_option_style_transform_vec_value_delete(self) }; } }


    /// `OptionStyleTransformOriginValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleTransformOriginValue as OptionStyleTransformOriginValue;

    impl Clone for OptionStyleTransformOriginValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleTransformOriginValue { }


    /// `OptionStylePerspectiveOriginValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStylePerspectiveOriginValue as OptionStylePerspectiveOriginValue;

    impl Clone for OptionStylePerspectiveOriginValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStylePerspectiveOriginValue { }


    /// `OptionStyleBackfaceVisibilityValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBackfaceVisibilityValue as OptionStyleBackfaceVisibilityValue;

    impl Clone for OptionStyleBackfaceVisibilityValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBackfaceVisibilityValue { }


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


    /// `OptionPercentageValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionPercentageValue as OptionPercentageValue;

    impl Clone for OptionPercentageValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionPercentageValue { }


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


    /// `OptionStyleBackgroundContentValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBackgroundContentValue as OptionStyleBackgroundContentValue;

    impl Clone for OptionStyleBackgroundContentValue { fn clone(&self) -> Self { unsafe { crate::dll::az_option_style_background_content_value_deep_copy(self) } } }
    impl Drop for OptionStyleBackgroundContentValue { fn drop(&mut self) { unsafe { crate::dll::az_option_style_background_content_value_delete(self) }; } }


    /// `OptionStyleBackgroundPositionValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBackgroundPositionValue as OptionStyleBackgroundPositionValue;

    impl Clone for OptionStyleBackgroundPositionValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBackgroundPositionValue { }


    /// `OptionStyleBackgroundSizeValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBackgroundSizeValue as OptionStyleBackgroundSizeValue;

    impl Clone for OptionStyleBackgroundSizeValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBackgroundSizeValue { }


    /// `OptionStyleBackgroundRepeatValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBackgroundRepeatValue as OptionStyleBackgroundRepeatValue;

    impl Clone for OptionStyleBackgroundRepeatValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBackgroundRepeatValue { }


    /// `OptionStyleFontSizeValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleFontSizeValue as OptionStyleFontSizeValue;

    impl Clone for OptionStyleFontSizeValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleFontSizeValue { }


    /// `OptionStyleFontFamilyValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleFontFamilyValue as OptionStyleFontFamilyValue;

    impl Clone for OptionStyleFontFamilyValue { fn clone(&self) -> Self { unsafe { crate::dll::az_option_style_font_family_value_deep_copy(self) } } }
    impl Drop for OptionStyleFontFamilyValue { fn drop(&mut self) { unsafe { crate::dll::az_option_style_font_family_value_delete(self) }; } }


    /// `OptionStyleTextColorValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleTextColorValue as OptionStyleTextColorValue;

    impl Clone for OptionStyleTextColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleTextColorValue { }


    /// `OptionStyleTextAlignmentHorzValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleTextAlignmentHorzValue as OptionStyleTextAlignmentHorzValue;

    impl Clone for OptionStyleTextAlignmentHorzValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleTextAlignmentHorzValue { }


    /// `OptionStyleLineHeightValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleLineHeightValue as OptionStyleLineHeightValue;

    impl Clone for OptionStyleLineHeightValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleLineHeightValue { }


    /// `OptionStyleLetterSpacingValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleLetterSpacingValue as OptionStyleLetterSpacingValue;

    impl Clone for OptionStyleLetterSpacingValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleLetterSpacingValue { }


    /// `OptionStyleWordSpacingValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleWordSpacingValue as OptionStyleWordSpacingValue;

    impl Clone for OptionStyleWordSpacingValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleWordSpacingValue { }


    /// `OptionStyleTabWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleTabWidthValue as OptionStyleTabWidthValue;

    impl Clone for OptionStyleTabWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleTabWidthValue { }


    /// `OptionStyleCursorValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleCursorValue as OptionStyleCursorValue;

    impl Clone for OptionStyleCursorValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleCursorValue { }


    /// `OptionBoxShadowPreDisplayItemValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionBoxShadowPreDisplayItemValue as OptionBoxShadowPreDisplayItemValue;

    impl Clone for OptionBoxShadowPreDisplayItemValue { fn clone(&self) -> Self { unsafe { crate::dll::az_option_box_shadow_pre_display_item_value_deep_copy(self) } } }
    impl Drop for OptionBoxShadowPreDisplayItemValue { fn drop(&mut self) { unsafe { crate::dll::az_option_box_shadow_pre_display_item_value_delete(self) }; } }


    /// `OptionStyleBorderTopColorValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderTopColorValue as OptionStyleBorderTopColorValue;

    impl Clone for OptionStyleBorderTopColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderTopColorValue { }


    /// `OptionStyleBorderLeftColorValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderLeftColorValue as OptionStyleBorderLeftColorValue;

    impl Clone for OptionStyleBorderLeftColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderLeftColorValue { }


    /// `OptionStyleBorderRightColorValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderRightColorValue as OptionStyleBorderRightColorValue;

    impl Clone for OptionStyleBorderRightColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderRightColorValue { }


    /// `OptionStyleBorderBottomColorValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderBottomColorValue as OptionStyleBorderBottomColorValue;

    impl Clone for OptionStyleBorderBottomColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderBottomColorValue { }


    /// `OptionStyleBorderTopStyleValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderTopStyleValue as OptionStyleBorderTopStyleValue;

    impl Clone for OptionStyleBorderTopStyleValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderTopStyleValue { }


    /// `OptionStyleBorderLeftStyleValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderLeftStyleValue as OptionStyleBorderLeftStyleValue;

    impl Clone for OptionStyleBorderLeftStyleValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderLeftStyleValue { }


    /// `OptionStyleBorderRightStyleValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderRightStyleValue as OptionStyleBorderRightStyleValue;

    impl Clone for OptionStyleBorderRightStyleValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderRightStyleValue { }


    /// `OptionStyleBorderBottomStyleValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderBottomStyleValue as OptionStyleBorderBottomStyleValue;

    impl Clone for OptionStyleBorderBottomStyleValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderBottomStyleValue { }


    /// `OptionStyleBorderTopLeftRadiusValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderTopLeftRadiusValue as OptionStyleBorderTopLeftRadiusValue;

    impl Clone for OptionStyleBorderTopLeftRadiusValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderTopLeftRadiusValue { }


    /// `OptionStyleBorderTopRightRadiusValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderTopRightRadiusValue as OptionStyleBorderTopRightRadiusValue;

    impl Clone for OptionStyleBorderTopRightRadiusValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderTopRightRadiusValue { }


    /// `OptionStyleBorderBottomLeftRadiusValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderBottomLeftRadiusValue as OptionStyleBorderBottomLeftRadiusValue;

    impl Clone for OptionStyleBorderBottomLeftRadiusValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderBottomLeftRadiusValue { }


    /// `OptionStyleBorderBottomRightRadiusValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderBottomRightRadiusValue as OptionStyleBorderBottomRightRadiusValue;

    impl Clone for OptionStyleBorderBottomRightRadiusValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderBottomRightRadiusValue { }


    /// `OptionLayoutDisplayValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutDisplayValue as OptionLayoutDisplayValue;

    impl Clone for OptionLayoutDisplayValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutDisplayValue { }


    /// `OptionLayoutFloatValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutFloatValue as OptionLayoutFloatValue;

    impl Clone for OptionLayoutFloatValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutFloatValue { }


    /// `OptionLayoutBoxSizingValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutBoxSizingValue as OptionLayoutBoxSizingValue;

    impl Clone for OptionLayoutBoxSizingValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutBoxSizingValue { }


    /// `OptionLayoutWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutWidthValue as OptionLayoutWidthValue;

    impl Clone for OptionLayoutWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutWidthValue { }


    /// `OptionLayoutHeightValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutHeightValue as OptionLayoutHeightValue;

    impl Clone for OptionLayoutHeightValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutHeightValue { }


    /// `OptionLayoutMinWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutMinWidthValue as OptionLayoutMinWidthValue;

    impl Clone for OptionLayoutMinWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutMinWidthValue { }


    /// `OptionLayoutMinHeightValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutMinHeightValue as OptionLayoutMinHeightValue;

    impl Clone for OptionLayoutMinHeightValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutMinHeightValue { }


    /// `OptionLayoutMaxWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutMaxWidthValue as OptionLayoutMaxWidthValue;

    impl Clone for OptionLayoutMaxWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutMaxWidthValue { }


    /// `OptionLayoutMaxHeightValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutMaxHeightValue as OptionLayoutMaxHeightValue;

    impl Clone for OptionLayoutMaxHeightValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutMaxHeightValue { }


    /// `OptionLayoutPositionValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutPositionValue as OptionLayoutPositionValue;

    impl Clone for OptionLayoutPositionValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutPositionValue { }


    /// `OptionLayoutTopValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutTopValue as OptionLayoutTopValue;

    impl Clone for OptionLayoutTopValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutTopValue { }


    /// `OptionLayoutBottomValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutBottomValue as OptionLayoutBottomValue;

    impl Clone for OptionLayoutBottomValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutBottomValue { }


    /// `OptionLayoutRightValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutRightValue as OptionLayoutRightValue;

    impl Clone for OptionLayoutRightValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutRightValue { }


    /// `OptionLayoutLeftValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutLeftValue as OptionLayoutLeftValue;

    impl Clone for OptionLayoutLeftValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutLeftValue { }


    /// `OptionLayoutPaddingTopValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutPaddingTopValue as OptionLayoutPaddingTopValue;

    impl Clone for OptionLayoutPaddingTopValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutPaddingTopValue { }


    /// `OptionLayoutPaddingBottomValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutPaddingBottomValue as OptionLayoutPaddingBottomValue;

    impl Clone for OptionLayoutPaddingBottomValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutPaddingBottomValue { }


    /// `OptionLayoutPaddingLeftValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutPaddingLeftValue as OptionLayoutPaddingLeftValue;

    impl Clone for OptionLayoutPaddingLeftValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutPaddingLeftValue { }


    /// `OptionLayoutPaddingRightValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutPaddingRightValue as OptionLayoutPaddingRightValue;

    impl Clone for OptionLayoutPaddingRightValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutPaddingRightValue { }


    /// `OptionLayoutMarginTopValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutMarginTopValue as OptionLayoutMarginTopValue;

    impl Clone for OptionLayoutMarginTopValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutMarginTopValue { }


    /// `OptionLayoutMarginBottomValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutMarginBottomValue as OptionLayoutMarginBottomValue;

    impl Clone for OptionLayoutMarginBottomValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutMarginBottomValue { }


    /// `OptionLayoutMarginLeftValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutMarginLeftValue as OptionLayoutMarginLeftValue;

    impl Clone for OptionLayoutMarginLeftValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutMarginLeftValue { }


    /// `OptionLayoutMarginRightValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutMarginRightValue as OptionLayoutMarginRightValue;

    impl Clone for OptionLayoutMarginRightValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutMarginRightValue { }


    /// `OptionStyleBorderTopWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderTopWidthValue as OptionStyleBorderTopWidthValue;

    impl Clone for OptionStyleBorderTopWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderTopWidthValue { }


    /// `OptionStyleBorderLeftWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderLeftWidthValue as OptionStyleBorderLeftWidthValue;

    impl Clone for OptionStyleBorderLeftWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderLeftWidthValue { }


    /// `OptionStyleBorderRightWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderRightWidthValue as OptionStyleBorderRightWidthValue;

    impl Clone for OptionStyleBorderRightWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderRightWidthValue { }


    /// `OptionStyleBorderBottomWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionStyleBorderBottomWidthValue as OptionStyleBorderBottomWidthValue;

    impl Clone for OptionStyleBorderBottomWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionStyleBorderBottomWidthValue { }


    /// `OptionOverflowValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionOverflowValue as OptionOverflowValue;

    impl Clone for OptionOverflowValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionOverflowValue { }


    /// `OptionLayoutFlexDirectionValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutFlexDirectionValue as OptionLayoutFlexDirectionValue;

    impl Clone for OptionLayoutFlexDirectionValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutFlexDirectionValue { }


    /// `OptionLayoutWrapValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutWrapValue as OptionLayoutWrapValue;

    impl Clone for OptionLayoutWrapValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutWrapValue { }


    /// `OptionLayoutFlexGrowValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutFlexGrowValue as OptionLayoutFlexGrowValue;

    impl Clone for OptionLayoutFlexGrowValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutFlexGrowValue { }


    /// `OptionLayoutFlexShrinkValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutFlexShrinkValue as OptionLayoutFlexShrinkValue;

    impl Clone for OptionLayoutFlexShrinkValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutFlexShrinkValue { }


    /// `OptionLayoutJustifyContentValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutJustifyContentValue as OptionLayoutJustifyContentValue;

    impl Clone for OptionLayoutJustifyContentValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutJustifyContentValue { }


    /// `OptionLayoutAlignItemsValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutAlignItemsValue as OptionLayoutAlignItemsValue;

    impl Clone for OptionLayoutAlignItemsValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutAlignItemsValue { }


    /// `OptionLayoutAlignContentValue` struct
    #[doc(inline)] pub use crate::dll::AzOptionLayoutAlignContentValue as OptionLayoutAlignContentValue;

    impl Clone for OptionLayoutAlignContentValue { fn clone(&self) -> Self { *self } }
    impl Copy for OptionLayoutAlignContentValue { }


    /// `OptionTagId` struct
    #[doc(inline)] pub use crate::dll::AzOptionTagId as OptionTagId;

    impl Clone for OptionTagId { fn clone(&self) -> Self { *self } }
    impl Copy for OptionTagId { }


    /// `OptionDuration` struct
    #[doc(inline)] pub use crate::dll::AzOptionDuration as OptionDuration;

    impl Clone for OptionDuration { fn clone(&self) -> Self { *self } }
    impl Copy for OptionDuration { }


    /// `OptionInstantPtr` struct
    #[doc(inline)] pub use crate::dll::AzOptionInstantPtr as OptionInstantPtr;

    impl Clone for OptionInstantPtr { fn clone(&self) -> Self { unsafe { crate::dll::az_option_instant_ptr_deep_copy(self) } } }
    impl Drop for OptionInstantPtr { fn drop(&mut self) { unsafe { crate::dll::az_option_instant_ptr_delete(self) }; } }


    /// `OptionUsize` struct
    #[doc(inline)] pub use crate::dll::AzOptionUsize as OptionUsize;

    impl Clone for OptionUsize { fn clone(&self) -> Self { *self } }
    impl Copy for OptionUsize { }


    /// `OptionU8VecRef` struct
    #[doc(inline)] pub use crate::dll::AzOptionU8VecRef as OptionU8VecRef;

    impl Drop for OptionU8VecRef { fn drop(&mut self) { unsafe { crate::dll::az_option_u8_vec_ref_delete(self) }; } }
