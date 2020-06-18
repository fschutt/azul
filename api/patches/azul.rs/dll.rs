    /// Return type of a regular callback - currently `AzUpdateScreen`
    pub type AzCallbackReturn = AzUpdateScreen;
    /// Callback for responding to window events
    pub type AzCallbackType = fn(AzCallbackInfoPtr) -> AzCallbackReturn;
    /// Callback fn that returns the DOM of the app
    pub type AzLayoutCallbackType = fn(AzRefAny, AzLayoutInfoPtr) -> AzDom;
    /// Callback for rendering to an OpenGL texture
    pub type AzGlCallbackType = fn(AzGlCallbackInfoPtr) -> AzGlCallbackReturn;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzIFrameCallbackType = fn(AzIFrameCallbackInfoPtr) -> AzIFrameCallbackReturn;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzRefAnyDestructorType = fn(*const c_void);
    /// Callback for the `Timer` class
    pub type AzTimerCallbackType = fn(AzTimerCallbackInfoPtr) -> AzTimerCallbackReturn;
    /// Callback for the `Thread` class
    pub type AzThreadCallbackType = fn(AzRefAny) -> AzRefAny;
    /// Callback for the `Task` class
    pub type AzTaskCallbackType= fn(AzArcMutexRefAnyPtr, AzDropCheckPtr) -> AzUpdateScreen;

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

    impl_option!(char, AzOptionChar, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
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
