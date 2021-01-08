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
    impl_option!(AzTagId, AzOptionTagId, [Debug, Copy, Clone]);
    impl_option!(AzDuration, AzOptionDuration, [Debug, Copy, Clone]);
    impl_option!(AzInstantPtr, AzOptionInstantPtr, copy = false, clone = false, [Debug]); // TODO: impl clone!
    impl_option!(AzU8VecRef, AzOptionU8VecRef, copy = false, clone = false, [Debug]);
