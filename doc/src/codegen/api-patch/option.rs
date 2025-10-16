
    

    
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
        }
    )}

    macro_rules! impl_option {
        ($struct_type:ident, $struct_name:ident, copy = false, clone = false, [$($derive:meta),* ]) => (
            impl_option_inner!($struct_type, $struct_name);
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

            
            impl $struct_name {
                pub fn into_option(self) -> Option<$struct_type> {
                    self.into()
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

            
            impl $struct_name {
                pub fn into_option(self) -> Option<$struct_type> {
                    self.into()
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
        );
    }

    pub type AzX11Visual = *const c_void;
    pub type AzHwndHandle = *mut c_void;

    impl_option!(i32, AzOptionI32, [Debug, Copy, Clone]);
    impl_option!(f32, AzOptionF32, [Debug, Copy, Clone]);
    impl_option!(usize, AzOptionUsize, [Debug, Copy, Clone]);
    impl_option!(u32, AzOptionChar, [Debug, Copy, Clone]);

    impl_option!(AzThreadId, AzOptionThreadId, [Debug, Copy, Clone]);
    impl_option!(AzTimerId, AzOptionTimerId, [Debug, Copy, Clone]);
    impl_option!(AzThreadSendMsg, AzOptionThreadSendMsg, [Debug, Copy, Clone]);
    impl_option!(AzLayoutRect, AzOptionLayoutRect, [Debug, Copy, Clone]);
    impl_option!(AzRefAny, AzOptionRefAny, copy = false, clone = false, [Debug, Clone]);
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
    impl_option!(AzDom, AzOptionDom, copy = false, clone = false, [Debug, Clone]);
    impl_option!(AzTexture, AzOptionTexture, copy = false, clone = false, [Debug]);
    impl_option!(AzImageMask, AzOptionImageMask, copy = false, [Debug, Clone]);
    impl_option!(AzTabIndex, AzOptionTabIndex, [Debug, Copy, Clone]);
    impl_option!(AzCallback, AzOptionCallback, [Debug, Copy, Clone]);
    impl_option!(AzTagId, AzOptionTagId, [Debug, Copy, Clone]);
    impl_option!(AzDuration, AzOptionDuration, [Debug, Copy, Clone]);
    impl_option!(AzInstant, AzOptionInstant, copy = false, clone = false, [Debug]); // TODO: impl clone!
    impl_option!(AzU8VecRef, AzOptionU8VecRef, copy = false, clone = false, [Debug]);
    impl_option!(AzSystemClipboard, AzOptionSystemClipboard, copy = false,  clone = false, [Debug]);
    impl_option!(AzFileTypeList, AzOptionFileTypeList, copy = false, [Debug, Clone]);
    impl_option!(AzWindowState, AzOptionWindowState, copy = false, [Debug, Clone]);
    impl_option!(AzKeyboardState, AzOptionKeyboardState, copy = false, [Debug, Clone]);
    impl_option!(AzMouseState, AzOptionMouseState, [Debug, Clone]);
    impl_option!(AzOnNodeAdded, AzOptionOnNodeAdded, [Debug, Copy, Clone]);
    impl_option!(AzOnNodeRemoved, AzOptionOnNodeRemoved, [Debug, Copy, Clone]);
    impl_option!(AzOnNodeDragged, AzOptionOnNodeDragged, [Debug, Copy, Clone]);
    impl_option!(AzOnNodeGraphDragged, AzOptionOnNodeGraphDragged, [Debug, Copy, Clone]);
    impl_option!(AzOnNodeConnected, AzOptionOnNodeConnected, [Debug, Copy, Clone]);
    impl_option!(AzOnNodeInputDisconnected, AzOptionOnNodeInputDisconnected, [Debug, Copy, Clone]);
    impl_option!(AzOnNodeOutputDisconnected, AzOptionOnNodeOutputDisconnected, [Debug, Copy, Clone]);
    impl_option!(AzOnNodeFieldEdited, AzOptionOnNodeFieldEdited, [Debug, Copy, Clone]);
    impl_option!(AzGl, AzOptionGl, copy = false, [Debug, Clone]);
    impl_option!(AzPixelValueNoPercent, AzOptionPixelValueNoPercent, copy = false, [Debug, Copy, Clone]);
    impl_option!(AzSvgPoint, AzOptionSvgPoint, [Debug, Copy, Clone]);
    impl_option!(AzStyleTextAlign, AzOptionStyleTextAlign, [Debug, Copy, Clone]);
