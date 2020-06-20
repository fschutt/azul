    use std::ffi::c_void;

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
    /// Re-export of rust-allocated (stack based) `String` struct
    #[repr(C)] pub struct AzString {
        pub vec: AzU8Vec,
    }
    /// Wrapper over a Rust-allocated `XWindowType`
    #[repr(C)] pub struct AzXWindowTypeVec {
        pub(crate) ptr: *mut AzXWindowType,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `VirtualKeyCode`
    #[repr(C)] pub struct AzVirtualKeyCodeVec {
        pub(crate) ptr: *mut AzVirtualKeyCode,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `ScanCode`
    #[repr(C)] pub struct AzScanCodeVec {
        pub(crate) ptr: *mut u32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CssDeclaration`
    #[repr(C)] pub struct AzCssDeclarationVec {
        pub(crate) ptr: *mut AzCssDeclaration,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CssPathSelector`
    #[repr(C)] pub struct AzCssPathSelectorVec {
        pub(crate) ptr: *mut AzCssPathSelector,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Stylesheet`
    #[repr(C)] pub struct AzStylesheetVec {
        pub(crate) ptr: *mut AzStylesheet,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CssRuleBlock`
    #[repr(C)] pub struct AzCssRuleBlockVec {
        pub(crate) ptr: *mut AzCssRuleBlock,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `U8Vec`
    #[repr(C)] pub struct AzU8Vec {
        pub(crate) ptr: *mut u8,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CallbackData`
    #[repr(C)] pub struct AzCallbackDataVec {
        pub(crate) ptr: *mut AzCallbackData,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<DebugMessage>`
    #[repr(C)] pub struct AzDebugMessageVec {
        pub(crate) ptr: *mut AzDebugMessage,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `U32Vec`
    #[repr(C)] pub struct AzGLuintVec {
        pub(crate) ptr: *mut u32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `GLintVec`
    #[repr(C)] pub struct AzGLintVec {
        pub(crate) ptr: *mut i32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `OverridePropertyVec`
    #[repr(C)] pub struct AzOverridePropertyVec {
        pub(crate) ptr: *mut AzOverrideProperty,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `DomVec`
    #[repr(C)] pub struct AzDomVec {
        pub(crate) ptr: *mut AzDom,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `StringVec`
    #[repr(C)] pub struct AzStringVec {
        pub(crate) ptr: *mut AzString,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `StringPairVec`
    #[repr(C)] pub struct AzStringPairVec {
        pub(crate) ptr: *mut AzStringPair,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `GradientStopPreVec`
    #[repr(C)] pub struct AzGradientStopPreVec {
        pub(crate) ptr: *mut AzGradientStopPre,
        pub len: usize,
        pub cap: usize,
    }
    /// Re-export of rust-allocated (stack based) `OptionWaylandTheme` struct
    #[repr(C, u8)] pub enum AzOptionWaylandTheme {
        None,
        Some(AzWaylandTheme),
    }
    /// Re-export of rust-allocated (stack based) `OptionTaskBarIcon` struct
    #[repr(C, u8)] pub enum AzOptionTaskBarIcon {
        None,
        Some(AzTaskBarIcon),
    }
    /// Re-export of rust-allocated (stack based) `OptionHwndHandle` struct
    #[repr(C, u8)] pub enum AzOptionHwndHandle {
        None,
        Some(*mut c_void),
    }
    /// Re-export of rust-allocated (stack based) `OptionLogicalPosition` struct
    #[repr(C, u8)] pub enum AzOptionLogicalPosition {
        None,
        Some(AzLogicalPosition),
    }
    /// Re-export of rust-allocated (stack based) `OptionHotReloadOptions` struct
    #[repr(C, u8)] pub enum AzOptionHotReloadOptions {
        None,
        Some(AzHotReloadOptions),
    }
    /// Re-export of rust-allocated (stack based) `OptionPhysicalPositionI32` struct
    #[repr(C, u8)] pub enum AzOptionPhysicalPositionI32 {
        None,
        Some(AzPhysicalPositionI32),
    }
    /// Re-export of rust-allocated (stack based) `OptionWindowIcon` struct
    #[repr(C, u8)] pub enum AzOptionWindowIcon {
        None,
        Some(AzWindowIcon),
    }
    /// Re-export of rust-allocated (stack based) `OptionString` struct
    #[repr(C, u8)] pub enum AzOptionString {
        None,
        Some(AzString),
    }
    /// Re-export of rust-allocated (stack based) `OptionX11Visual` struct
    #[repr(C, u8)] pub enum AzOptionX11Visual {
        None,
        Some(*const c_void),
    }
    /// Re-export of rust-allocated (stack based) `OptionI32` struct
    #[repr(C, u8)] pub enum AzOptionI32 {
        None,
        Some(i32),
    }
    /// Re-export of rust-allocated (stack based) `OptionF32` struct
    #[repr(C, u8)] pub enum AzOptionF32 {
        None,
        Some(f32),
    }
    /// Re-export of rust-allocated (stack based) `OptionMouseCursorType` struct
    #[repr(C, u8)] pub enum AzOptionMouseCursorType {
        None,
        Some(AzMouseCursorType),
    }
    /// Re-export of rust-allocated (stack based) `OptionLogicalSize` struct
    #[repr(C, u8)] pub enum AzOptionLogicalSize {
        None,
        Some(AzLogicalSize),
    }
    /// Re-export of rust-allocated (stack based) `OptionChar` struct
    #[repr(C, u8)] pub enum AzOptionChar {
        None,
        Some(char),
    }
    /// Re-export of rust-allocated (stack based) `OptionVirtualKeyCode` struct
    #[repr(C, u8)] pub enum AzOptionVirtualKeyCode {
        None,
        Some(AzVirtualKeyCode),
    }
    /// Re-export of rust-allocated (stack based) `OptionPercentageValue` struct
    #[repr(C, u8)] pub enum AzOptionPercentageValue {
        None,
        Some(AzPercentageValue),
    }
    /// Re-export of rust-allocated (stack based) `OptionDom` struct
    #[repr(C, u8)] pub enum AzOptionDom {
        None,
        Some(AzDom),
    }
    /// Re-export of rust-allocated (stack based) `OptionTexture` struct
    #[repr(C, u8)] pub enum AzOptionTexture {
        None,
        Some(AzTexture),
    }
    /// Re-export of rust-allocated (stack based) `OptionTabIndex` struct
    #[repr(C, u8)] pub enum AzOptionTabIndex {
        None,
        Some(AzTabIndex),
    }
    /// Re-export of rust-allocated (stack based) `OptionDuration` struct
    #[repr(C, u8)] pub enum AzOptionDuration {
        None,
        Some(AzDuration),
    }
    /// Re-export of rust-allocated (stack based) `OptionInstantPtr` struct
    #[repr(C, u8)] pub enum AzOptionInstantPtr {
        None,
        Some(AzInstantPtr),
    }
    /// Re-export of rust-allocated (stack based) `OptionUsize` struct
    #[repr(C, u8)] pub enum AzOptionUsize {
        None,
        Some(usize),
    }
    /// Re-export of rust-allocated (stack based) `OptionU8VecRef` struct
    #[repr(C, u8)] pub enum AzOptionU8VecRef {
        None,
        Some(AzU8VecRef),
    }
    /// Re-export of rust-allocated (stack based) `ResultRefAnyBlockError` struct
    #[repr(C, u8)] pub enum AzResultRefAnyBlockError {
        Ok(AzRefAny),
        Err(AzBlockError),
    }
    /// Pointer to rust-allocated `Box<Instant>` struct
    #[repr(C)] pub struct AzInstantPtr {
        pub(crate) ptr: *mut c_void,
    }

    impl PartialEq for AzInstantPtr { fn eq(&self, rhs: &AzInstantPtr) -> bool { (crate::dll::get_azul_dll().az_instant_ptr_partial_eq)(self, rhs) } }

    impl Eq for AzInstantPtr { }

    impl PartialOrd for AzInstantPtr { fn partial_cmp(&self, rhs: &AzInstantPtr) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_instant_ptr_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzInstantPtr { fn cmp(&self, rhs: &AzInstantPtr) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_instant_ptr_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }
    /// Re-export of rust-allocated (stack based) `Duration` struct
    #[repr(C)] pub struct AzDuration {
        pub secs: u64,
        pub nanos: u32,
    }
    /// Pointer to rust-allocated `Box<AppConfig>` struct
    #[repr(C)] pub struct AzAppConfigPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Pointer to rust-allocated `Box<App>` struct
    #[repr(C)] pub struct AzAppPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `HidpiAdjustedBounds` struct
    #[repr(C)] pub struct AzHidpiAdjustedBounds {
        pub logical_size: AzLogicalSize,
        pub hidpi_factor: f32,
    }
    /// Re-export of rust-allocated (stack based) `LayoutCallback` struct
    #[repr(C)] pub struct AzLayoutCallback {
        pub cb: AzLayoutCallbackType,
    }
    /// Re-export of rust-allocated (stack based) `Callback` struct
    #[repr(C)] pub struct AzCallback {
        pub cb: AzCallbackType,
    }

    impl PartialEq for AzCallback { fn eq(&self, rhs: &AzCallback) -> bool { (crate::dll::get_azul_dll().az_callback_partial_eq)(self, rhs) } }

    impl Eq for AzCallback { }

    impl PartialOrd for AzCallback { fn partial_cmp(&self, rhs: &AzCallback) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_callback_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzCallback { fn cmp(&self, rhs: &AzCallback) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_callback_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzCallback { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_callback_hash)(self)).hash(state) } }
    /// Pointer to rust-allocated `Box<CallbackInfo>` struct
    #[repr(C)] pub struct AzCallbackInfoPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Specifies if the screen should be updated after the callback function has returned
    #[repr(C)] pub enum AzUpdateScreen {
        Redraw,
        DontRedraw,
        UpdateScrollStates,
        UpdateTransforms,
    }
    /// Re-export of rust-allocated (stack based) `IFrameCallback` struct
    #[repr(C)] pub struct AzIFrameCallback {
        pub cb: AzIFrameCallbackType,
    }
    /// Pointer to rust-allocated `Box<IFrameCallbackInfo>` struct
    #[repr(C)] pub struct AzIFrameCallbackInfoPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `IFrameCallbackReturn` struct
    #[repr(C)] pub struct AzIFrameCallbackReturn {
        pub dom: AzOptionDom,
    }
    /// Re-export of rust-allocated (stack based) `GlCallback` struct
    #[repr(C)] pub struct AzGlCallback {
        pub cb: AzGlCallbackType,
    }
    /// Pointer to rust-allocated `Box<GlCallbackInfo>` struct
    #[repr(C)] pub struct AzGlCallbackInfoPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `GlCallbackReturn` struct
    #[repr(C)] pub struct AzGlCallbackReturn {
        pub texture: AzOptionTexture,
    }
    /// Re-export of rust-allocated (stack based) `TimerCallback` struct
    #[repr(C)] pub struct AzTimerCallback {
        pub cb: AzTimerCallbackType,
    }
    /// Pointer to rust-allocated `Box<TimerCallbackType>` struct
    #[repr(C)] pub struct AzTimerCallbackTypePtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Pointer to rust-allocated `Box<TimerCallbackInfo>` struct
    #[repr(C)] pub struct AzTimerCallbackInfoPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `TimerCallbackReturn` struct
    #[repr(C)] pub struct AzTimerCallbackReturn {
        pub should_update: AzUpdateScreen,
        pub should_terminate: AzTerminateTimer,
    }
    /// Re-export of rust-allocated (stack based) `RefAnySharingInfo` struct
    #[repr(C)] pub struct AzRefAnySharingInfo {
        pub(crate) ptr: *const c_void,
    }
    /// RefAny is a reference-counted, type-erased pointer, which stores a reference to a struct. `RefAny` can be up- and downcasted (this usually done via generics and can't be expressed in the Rust API)
    #[repr(C)] pub struct AzRefAny {
        pub _internal_ptr: *const c_void,
        pub _internal_len: usize,
        pub _internal_layout_size: usize,
        pub _internal_layout_align: usize,
        pub type_id: u64,
        pub type_name: AzString,
        pub _sharing_info_ptr: *const AzRefAnySharingInfo,
        pub custom_destructor: AzRefAnyDestructorType,
    }

    impl PartialEq for AzRefAny { fn eq(&self, rhs: &AzRefAny) -> bool { (crate::dll::get_azul_dll().az_ref_any_partial_eq)(self, rhs) } }

    impl Eq for AzRefAny { }

    impl PartialOrd for AzRefAny { fn partial_cmp(&self, rhs: &AzRefAny) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_ref_any_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzRefAny { fn cmp(&self, rhs: &AzRefAny) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_ref_any_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzRefAny { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_ref_any_hash)(self)).hash(state) } }
    /// Pointer to rust-allocated `Box<LayoutInfo>` struct
    #[repr(C)] pub struct AzLayoutInfoPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `CssRuleBlock` struct
    #[repr(C)] pub struct AzCssRuleBlock {
        pub path: AzCssPath,
        pub declarations: AzCssDeclarationVec,
    }
    /// Re-export of rust-allocated (stack based) `CssDeclaration` struct
    #[repr(C, u8)] pub enum AzCssDeclaration {
        Static(AzCssProperty),
        Dynamic(AzDynamicCssProperty),
    }
    /// Re-export of rust-allocated (stack based) `DynamicCssProperty` struct
    #[repr(C)] pub struct AzDynamicCssProperty {
        pub dynamic_id: AzString,
        pub default_value: AzCssProperty,
    }
    /// Re-export of rust-allocated (stack based) `CssPath` struct
    #[repr(C)] pub struct AzCssPath {
        pub selectors: AzCssPathSelectorVec,
    }
    /// Re-export of rust-allocated (stack based) `CssPathSelector` struct
    #[repr(C, u8)] pub enum AzCssPathSelector {
        Global,
        Type(AzNodeTypePath),
        Class(AzString),
        Id(AzString),
        PseudoSelector(AzCssPathPseudoSelector),
        DirectChildren,
        Children,
    }
    /// Re-export of rust-allocated (stack based) `NodeTypePath` struct
    #[repr(C)] pub enum AzNodeTypePath {
        Body,
        Div,
        P,
        Img,
        Texture,
        IFrame,
    }
    /// Re-export of rust-allocated (stack based) `CssPathPseudoSelector` struct
    #[repr(C, u8)] pub enum AzCssPathPseudoSelector {
        First,
        Last,
        NthChild(AzCssNthChildSelector),
        Hover,
        Active,
        Focus,
    }
    /// Re-export of rust-allocated (stack based) `CssNthChildSelector` struct
    #[repr(C, u8)] pub enum AzCssNthChildSelector {
        Number(usize),
        Even,
        Odd,
        Pattern(AzCssNthChildPattern),
    }
    /// Re-export of rust-allocated (stack based) `CssNthChildPattern` struct
    #[repr(C)] pub struct AzCssNthChildPattern {
        pub repeat: usize,
        pub offset: usize,
    }
    /// Re-export of rust-allocated (stack based) `Stylesheet` struct
    #[repr(C)] pub struct AzStylesheet {
        pub rules: AzCssRuleBlockVec,
    }
    /// Re-export of rust-allocated (stack based) `Css` struct
    #[repr(C)] pub struct AzCss {
        pub stylesheets: AzStylesheetVec,
    }
    /// Re-export of rust-allocated (stack based) `ColorU` struct
    #[repr(C)] pub struct AzColorU {
        pub r: u8,
        pub g: u8,
        pub b: u8,
        pub a: u8,
    }
    /// Re-export of rust-allocated (stack based) `SizeMetric` struct
    #[repr(C)] pub enum AzSizeMetric {
        Px,
        Pt,
        Em,
        Percent,
    }
    /// Re-export of rust-allocated (stack based) `FloatValue` struct
    #[repr(C)] pub struct AzFloatValue {
        pub number: isize,
    }
    /// Re-export of rust-allocated (stack based) `PixelValue` struct
    #[repr(C)] pub struct AzPixelValue {
        pub metric: AzSizeMetric,
        pub number: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `PixelValueNoPercent` struct
    #[repr(C)] pub struct AzPixelValueNoPercent {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `BoxShadowClipMode` struct
    #[repr(C)] pub enum AzBoxShadowClipMode {
        Outset,
        Inset,
    }
    /// Re-export of rust-allocated (stack based) `BoxShadowPreDisplayItem` struct
    #[repr(C)] pub struct AzBoxShadowPreDisplayItem {
        pub offset: [AzPixelValueNoPercent;2],
        pub color: AzColorU,
        pub blur_radius: AzPixelValueNoPercent,
        pub spread_radius: AzPixelValueNoPercent,
        pub clip_mode: AzBoxShadowClipMode,
    }
    /// Re-export of rust-allocated (stack based) `LayoutAlignContent` struct
    #[repr(C)] pub enum AzLayoutAlignContent {
        Stretch,
        Center,
        Start,
        End,
        SpaceBetween,
        SpaceAround,
    }
    /// Re-export of rust-allocated (stack based) `LayoutAlignItems` struct
    #[repr(C)] pub enum AzLayoutAlignItems {
        Stretch,
        Center,
        Start,
        End,
    }
    /// Re-export of rust-allocated (stack based) `LayoutBottom` struct
    #[repr(C)] pub struct AzLayoutBottom {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutBoxSizing` struct
    #[repr(C)] pub enum AzLayoutBoxSizing {
        ContentBox,
        BorderBox,
    }
    /// Re-export of rust-allocated (stack based) `LayoutDirection` struct
    #[repr(C)] pub enum AzLayoutDirection {
        Row,
        RowReverse,
        Column,
        ColumnReverse,
    }
    /// Re-export of rust-allocated (stack based) `LayoutDisplay` struct
    #[repr(C)] pub enum AzLayoutDisplay {
        Flex,
        Block,
        InlineBlock,
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexGrow` struct
    #[repr(C)] pub struct AzLayoutFlexGrow {
        pub inner: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexShrink` struct
    #[repr(C)] pub struct AzLayoutFlexShrink {
        pub inner: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutFloat` struct
    #[repr(C)] pub enum AzLayoutFloat {
        Left,
        Right,
    }
    /// Re-export of rust-allocated (stack based) `LayoutHeight` struct
    #[repr(C)] pub struct AzLayoutHeight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutJustifyContent` struct
    #[repr(C)] pub enum AzLayoutJustifyContent {
        Start,
        End,
        Center,
        SpaceBetween,
        SpaceAround,
        SpaceEvenly,
    }
    /// Re-export of rust-allocated (stack based) `LayoutLeft` struct
    #[repr(C)] pub struct AzLayoutLeft {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginBottom` struct
    #[repr(C)] pub struct AzLayoutMarginBottom {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginLeft` struct
    #[repr(C)] pub struct AzLayoutMarginLeft {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginRight` struct
    #[repr(C)] pub struct AzLayoutMarginRight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginTop` struct
    #[repr(C)] pub struct AzLayoutMarginTop {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMaxHeight` struct
    #[repr(C)] pub struct AzLayoutMaxHeight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMaxWidth` struct
    #[repr(C)] pub struct AzLayoutMaxWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMinHeight` struct
    #[repr(C)] pub struct AzLayoutMinHeight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutMinWidth` struct
    #[repr(C)] pub struct AzLayoutMinWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingBottom` struct
    #[repr(C)] pub struct AzLayoutPaddingBottom {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingLeft` struct
    #[repr(C)] pub struct AzLayoutPaddingLeft {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingRight` struct
    #[repr(C)] pub struct AzLayoutPaddingRight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingTop` struct
    #[repr(C)] pub struct AzLayoutPaddingTop {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutPosition` struct
    #[repr(C)] pub enum AzLayoutPosition {
        Static,
        Relative,
        Absolute,
        Fixed,
    }
    /// Re-export of rust-allocated (stack based) `LayoutRight` struct
    #[repr(C)] pub struct AzLayoutRight {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutTop` struct
    #[repr(C)] pub struct AzLayoutTop {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutWidth` struct
    #[repr(C)] pub struct AzLayoutWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `LayoutWrap` struct
    #[repr(C)] pub enum AzLayoutWrap {
        Wrap,
        NoWrap,
    }
    /// Re-export of rust-allocated (stack based) `Overflow` struct
    #[repr(C)] pub enum AzOverflow {
        Scroll,
        Auto,
        Hidden,
        Visible,
    }
    /// Re-export of rust-allocated (stack based) `PercentageValue` struct
    #[repr(C)] pub struct AzPercentageValue {
        pub number: AzFloatValue,
    }
    /// Re-export of rust-allocated (stack based) `GradientStopPre` struct
    #[repr(C)] pub struct AzGradientStopPre {
        pub offset: AzOptionPercentageValue,
        pub color: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `DirectionCorner` struct
    #[repr(C)] pub enum AzDirectionCorner {
        Right,
        Left,
        Top,
        Bottom,
        TopRight,
        TopLeft,
        BottomRight,
        BottomLeft,
    }
    /// Re-export of rust-allocated (stack based) `DirectionCorners` struct
    #[repr(C)] pub struct AzDirectionCorners {
        pub from: AzDirectionCorner,
        pub to: AzDirectionCorner,
    }
    /// Re-export of rust-allocated (stack based) `Direction` struct
    #[repr(C, u8)] pub enum AzDirection {
        Angle(AzFloatValue),
        FromTo(AzDirectionCorners),
    }
    /// Re-export of rust-allocated (stack based) `ExtendMode` struct
    #[repr(C)] pub enum AzExtendMode {
        Clamp,
        Repeat,
    }
    /// Re-export of rust-allocated (stack based) `LinearGradient` struct
    #[repr(C)] pub struct AzLinearGradient {
        pub direction: AzDirection,
        pub extend_mode: AzExtendMode,
        pub stops: AzGradientStopPreVec,
    }
    /// Re-export of rust-allocated (stack based) `Shape` struct
    #[repr(C)] pub enum AzShape {
        Ellipse,
        Circle,
    }
    /// Re-export of rust-allocated (stack based) `RadialGradient` struct
    #[repr(C)] pub struct AzRadialGradient {
        pub shape: AzShape,
        pub extend_mode: AzExtendMode,
        pub stops: AzGradientStopPreVec,
    }
    /// Re-export of rust-allocated (stack based) `CssImageId` struct
    #[repr(C)] pub struct AzCssImageId {
        pub inner: AzString,
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundContent` struct
    #[repr(C, u8)] pub enum AzStyleBackgroundContent {
        LinearGradient(AzLinearGradient),
        RadialGradient(AzRadialGradient),
        Image(AzCssImageId),
        Color(AzColorU),
    }
    /// Re-export of rust-allocated (stack based) `BackgroundPositionHorizontal` struct
    #[repr(C, u8)] pub enum AzBackgroundPositionHorizontal {
        Left,
        Center,
        Right,
        Exact(AzPixelValue),
    }
    /// Re-export of rust-allocated (stack based) `BackgroundPositionVertical` struct
    #[repr(C, u8)] pub enum AzBackgroundPositionVertical {
        Top,
        Center,
        Bottom,
        Exact(AzPixelValue),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundPosition` struct
    #[repr(C)] pub struct AzStyleBackgroundPosition {
        pub horizontal: AzBackgroundPositionHorizontal,
        pub vertical: AzBackgroundPositionVertical,
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundRepeat` struct
    #[repr(C)] pub enum AzStyleBackgroundRepeat {
        NoRepeat,
        Repeat,
        RepeatX,
        RepeatY,
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundSize` struct
    #[repr(C, u8)] pub enum AzStyleBackgroundSize {
        ExactSize([AzPixelValue;2]),
        Contain,
        Cover,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomColor` struct
    #[repr(C)] pub struct AzStyleBorderBottomColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomLeftRadius` struct
    #[repr(C)] pub struct AzStyleBorderBottomLeftRadius {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomRightRadius` struct
    #[repr(C)] pub struct AzStyleBorderBottomRightRadius {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `BorderStyle` struct
    #[repr(C)] pub enum AzBorderStyle {
        None,
        Solid,
        Double,
        Dotted,
        Dashed,
        Hidden,
        Groove,
        Ridge,
        Inset,
        Outset,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomStyle` struct
    #[repr(C)] pub struct AzStyleBorderBottomStyle {
        pub inner: AzBorderStyle,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomWidth` struct
    #[repr(C)] pub struct AzStyleBorderBottomWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftColor` struct
    #[repr(C)] pub struct AzStyleBorderLeftColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftStyle` struct
    #[repr(C)] pub struct AzStyleBorderLeftStyle {
        pub inner: AzBorderStyle,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftWidth` struct
    #[repr(C)] pub struct AzStyleBorderLeftWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightColor` struct
    #[repr(C)] pub struct AzStyleBorderRightColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightStyle` struct
    #[repr(C)] pub struct AzStyleBorderRightStyle {
        pub inner: AzBorderStyle,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightWidth` struct
    #[repr(C)] pub struct AzStyleBorderRightWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopColor` struct
    #[repr(C)] pub struct AzStyleBorderTopColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopLeftRadius` struct
    #[repr(C)] pub struct AzStyleBorderTopLeftRadius {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopRightRadius` struct
    #[repr(C)] pub struct AzStyleBorderTopRightRadius {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopStyle` struct
    #[repr(C)] pub struct AzStyleBorderTopStyle {
        pub inner: AzBorderStyle,
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopWidth` struct
    #[repr(C)] pub struct AzStyleBorderTopWidth {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleCursor` struct
    #[repr(C)] pub enum AzStyleCursor {
        Alias,
        AllScroll,
        Cell,
        ColResize,
        ContextMenu,
        Copy,
        Crosshair,
        Default,
        EResize,
        EwResize,
        Grab,
        Grabbing,
        Help,
        Move,
        NResize,
        NsResize,
        NeswResize,
        NwseResize,
        Pointer,
        Progress,
        RowResize,
        SResize,
        SeResize,
        Text,
        Unset,
        VerticalText,
        WResize,
        Wait,
        ZoomIn,
        ZoomOut,
    }
    /// Re-export of rust-allocated (stack based) `StyleFontFamily` struct
    #[repr(C)] pub struct AzStyleFontFamily {
        pub fonts: AzStringVec,
    }
    /// Re-export of rust-allocated (stack based) `StyleFontSize` struct
    #[repr(C)] pub struct AzStyleFontSize {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleLetterSpacing` struct
    #[repr(C)] pub struct AzStyleLetterSpacing {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleLineHeight` struct
    #[repr(C)] pub struct AzStyleLineHeight {
        pub inner: AzPercentageValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTabWidth` struct
    #[repr(C)] pub struct AzStyleTabWidth {
        pub inner: AzPercentageValue,
    }
    /// Re-export of rust-allocated (stack based) `StyleTextAlignmentHorz` struct
    #[repr(C)] pub enum AzStyleTextAlignmentHorz {
        Left,
        Center,
        Right,
    }
    /// Re-export of rust-allocated (stack based) `StyleTextColor` struct
    #[repr(C)] pub struct AzStyleTextColor {
        pub inner: AzColorU,
    }
    /// Re-export of rust-allocated (stack based) `StyleWordSpacing` struct
    #[repr(C)] pub struct AzStyleWordSpacing {
        pub inner: AzPixelValue,
    }
    /// Re-export of rust-allocated (stack based) `BoxShadowPreDisplayItemValue` struct
    #[repr(C, u8)] pub enum AzBoxShadowPreDisplayItemValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzBoxShadowPreDisplayItem),
    }
    /// Re-export of rust-allocated (stack based) `LayoutAlignContentValue` struct
    #[repr(C, u8)] pub enum AzLayoutAlignContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutAlignContent),
    }
    /// Re-export of rust-allocated (stack based) `LayoutAlignItemsValue` struct
    #[repr(C, u8)] pub enum AzLayoutAlignItemsValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutAlignItems),
    }
    /// Re-export of rust-allocated (stack based) `LayoutBottomValue` struct
    #[repr(C, u8)] pub enum AzLayoutBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBottom),
    }
    /// Re-export of rust-allocated (stack based) `LayoutBoxSizingValue` struct
    #[repr(C, u8)] pub enum AzLayoutBoxSizingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBoxSizing),
    }
    /// Re-export of rust-allocated (stack based) `LayoutDirectionValue` struct
    #[repr(C, u8)] pub enum AzLayoutDirectionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutDirection),
    }
    /// Re-export of rust-allocated (stack based) `LayoutDisplayValue` struct
    #[repr(C, u8)] pub enum AzLayoutDisplayValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutDisplay),
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexGrowValue` struct
    #[repr(C, u8)] pub enum AzLayoutFlexGrowValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexGrow),
    }
    /// Re-export of rust-allocated (stack based) `LayoutFlexShrinkValue` struct
    #[repr(C, u8)] pub enum AzLayoutFlexShrinkValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexShrink),
    }
    /// Re-export of rust-allocated (stack based) `LayoutFloatValue` struct
    #[repr(C, u8)] pub enum AzLayoutFloatValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFloat),
    }
    /// Re-export of rust-allocated (stack based) `LayoutHeightValue` struct
    #[repr(C, u8)] pub enum AzLayoutHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutHeight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutJustifyContentValue` struct
    #[repr(C, u8)] pub enum AzLayoutJustifyContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutJustifyContent),
    }
    /// Re-export of rust-allocated (stack based) `LayoutLeftValue` struct
    #[repr(C, u8)] pub enum AzLayoutLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutLeft),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginBottomValue` struct
    #[repr(C, u8)] pub enum AzLayoutMarginBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginBottom),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginLeftValue` struct
    #[repr(C, u8)] pub enum AzLayoutMarginLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginLeft),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginRightValue` struct
    #[repr(C, u8)] pub enum AzLayoutMarginRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginRight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMarginTopValue` struct
    #[repr(C, u8)] pub enum AzLayoutMarginTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginTop),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMaxHeightValue` struct
    #[repr(C, u8)] pub enum AzLayoutMaxHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMaxHeight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMaxWidthValue` struct
    #[repr(C, u8)] pub enum AzLayoutMaxWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMaxWidth),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMinHeightValue` struct
    #[repr(C, u8)] pub enum AzLayoutMinHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMinHeight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutMinWidthValue` struct
    #[repr(C, u8)] pub enum AzLayoutMinWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMinWidth),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingBottomValue` struct
    #[repr(C, u8)] pub enum AzLayoutPaddingBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingBottom),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingLeftValue` struct
    #[repr(C, u8)] pub enum AzLayoutPaddingLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingLeft),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingRightValue` struct
    #[repr(C, u8)] pub enum AzLayoutPaddingRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingRight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPaddingTopValue` struct
    #[repr(C, u8)] pub enum AzLayoutPaddingTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingTop),
    }
    /// Re-export of rust-allocated (stack based) `LayoutPositionValue` struct
    #[repr(C, u8)] pub enum AzLayoutPositionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPosition),
    }
    /// Re-export of rust-allocated (stack based) `LayoutRightValue` struct
    #[repr(C, u8)] pub enum AzLayoutRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutRight),
    }
    /// Re-export of rust-allocated (stack based) `LayoutTopValue` struct
    #[repr(C, u8)] pub enum AzLayoutTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutTop),
    }
    /// Re-export of rust-allocated (stack based) `LayoutWidthValue` struct
    #[repr(C, u8)] pub enum AzLayoutWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutWidth),
    }
    /// Re-export of rust-allocated (stack based) `LayoutWrapValue` struct
    #[repr(C, u8)] pub enum AzLayoutWrapValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutWrap),
    }
    /// Re-export of rust-allocated (stack based) `OverflowValue` struct
    #[repr(C, u8)] pub enum AzOverflowValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzOverflow),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundContentValue` struct
    #[repr(C, u8)] pub enum AzStyleBackgroundContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundContent),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundPositionValue` struct
    #[repr(C, u8)] pub enum AzStyleBackgroundPositionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundPosition),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundRepeatValue` struct
    #[repr(C, u8)] pub enum AzStyleBackgroundRepeatValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundRepeat),
    }
    /// Re-export of rust-allocated (stack based) `StyleBackgroundSizeValue` struct
    #[repr(C, u8)] pub enum AzStyleBackgroundSizeValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundSize),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomColorValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderBottomColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomLeftRadiusValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderBottomLeftRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomLeftRadius),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomRightRadiusValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderBottomRightRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomRightRadius),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomStyleValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderBottomStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomStyle),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderBottomWidthValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderBottomWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftColorValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderLeftColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftStyleValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderLeftStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftStyle),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderLeftWidthValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderLeftWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightColorValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderRightColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightStyleValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderRightStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightStyle),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderRightWidthValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderRightWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopColorValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderTopColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopLeftRadiusValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderTopLeftRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopLeftRadius),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopRightRadiusValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderTopRightRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopRightRadius),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopStyleValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderTopStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopStyle),
    }
    /// Re-export of rust-allocated (stack based) `StyleBorderTopWidthValue` struct
    #[repr(C, u8)] pub enum AzStyleBorderTopWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleCursorValue` struct
    #[repr(C, u8)] pub enum AzStyleCursorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleCursor),
    }
    /// Re-export of rust-allocated (stack based) `StyleFontFamilyValue` struct
    #[repr(C, u8)] pub enum AzStyleFontFamilyValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleFontFamily),
    }
    /// Re-export of rust-allocated (stack based) `StyleFontSizeValue` struct
    #[repr(C, u8)] pub enum AzStyleFontSizeValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleFontSize),
    }
    /// Re-export of rust-allocated (stack based) `StyleLetterSpacingValue` struct
    #[repr(C, u8)] pub enum AzStyleLetterSpacingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleLetterSpacing),
    }
    /// Re-export of rust-allocated (stack based) `StyleLineHeightValue` struct
    #[repr(C, u8)] pub enum AzStyleLineHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleLineHeight),
    }
    /// Re-export of rust-allocated (stack based) `StyleTabWidthValue` struct
    #[repr(C, u8)] pub enum AzStyleTabWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTabWidth),
    }
    /// Re-export of rust-allocated (stack based) `StyleTextAlignmentHorzValue` struct
    #[repr(C, u8)] pub enum AzStyleTextAlignmentHorzValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTextAlignmentHorz),
    }
    /// Re-export of rust-allocated (stack based) `StyleTextColorValue` struct
    #[repr(C, u8)] pub enum AzStyleTextColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTextColor),
    }
    /// Re-export of rust-allocated (stack based) `StyleWordSpacingValue` struct
    #[repr(C, u8)] pub enum AzStyleWordSpacingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleWordSpacing),
    }
    /// Parsed CSS key-value pair
    #[repr(C, u8)] pub enum AzCssProperty {
        TextColor(AzStyleTextColorValue),
        FontSize(AzStyleFontSizeValue),
        FontFamily(AzStyleFontFamilyValue),
        TextAlign(AzStyleTextAlignmentHorzValue),
        LetterSpacing(AzStyleLetterSpacingValue),
        LineHeight(AzStyleLineHeightValue),
        WordSpacing(AzStyleWordSpacingValue),
        TabWidth(AzStyleTabWidthValue),
        Cursor(AzStyleCursorValue),
        Display(AzLayoutDisplayValue),
        Float(AzLayoutFloatValue),
        BoxSizing(AzLayoutBoxSizingValue),
        Width(AzLayoutWidthValue),
        Height(AzLayoutHeightValue),
        MinWidth(AzLayoutMinWidthValue),
        MinHeight(AzLayoutMinHeightValue),
        MaxWidth(AzLayoutMaxWidthValue),
        MaxHeight(AzLayoutMaxHeightValue),
        Position(AzLayoutPositionValue),
        Top(AzLayoutTopValue),
        Right(AzLayoutRightValue),
        Left(AzLayoutLeftValue),
        Bottom(AzLayoutBottomValue),
        FlexWrap(AzLayoutWrapValue),
        FlexDirection(AzLayoutDirectionValue),
        FlexGrow(AzLayoutFlexGrowValue),
        FlexShrink(AzLayoutFlexShrinkValue),
        JustifyContent(AzLayoutJustifyContentValue),
        AlignItems(AzLayoutAlignItemsValue),
        AlignContent(AzLayoutAlignContentValue),
        BackgroundContent(AzStyleBackgroundContentValue),
        BackgroundPosition(AzStyleBackgroundPositionValue),
        BackgroundSize(AzStyleBackgroundSizeValue),
        BackgroundRepeat(AzStyleBackgroundRepeatValue),
        OverflowX(AzOverflowValue),
        OverflowY(AzOverflowValue),
        PaddingTop(AzLayoutPaddingTopValue),
        PaddingLeft(AzLayoutPaddingLeftValue),
        PaddingRight(AzLayoutPaddingRightValue),
        PaddingBottom(AzLayoutPaddingBottomValue),
        MarginTop(AzLayoutMarginTopValue),
        MarginLeft(AzLayoutMarginLeftValue),
        MarginRight(AzLayoutMarginRightValue),
        MarginBottom(AzLayoutMarginBottomValue),
        BorderTopLeftRadius(AzStyleBorderTopLeftRadiusValue),
        BorderTopRightRadius(AzStyleBorderTopRightRadiusValue),
        BorderBottomLeftRadius(AzStyleBorderBottomLeftRadiusValue),
        BorderBottomRightRadius(AzStyleBorderBottomRightRadiusValue),
        BorderTopColor(AzStyleBorderTopColorValue),
        BorderRightColor(AzStyleBorderRightColorValue),
        BorderLeftColor(AzStyleBorderLeftColorValue),
        BorderBottomColor(AzStyleBorderBottomColorValue),
        BorderTopStyle(AzStyleBorderTopStyleValue),
        BorderRightStyle(AzStyleBorderRightStyleValue),
        BorderLeftStyle(AzStyleBorderLeftStyleValue),
        BorderBottomStyle(AzStyleBorderBottomStyleValue),
        BorderTopWidth(AzStyleBorderTopWidthValue),
        BorderRightWidth(AzStyleBorderRightWidthValue),
        BorderLeftWidth(AzStyleBorderLeftWidthValue),
        BorderBottomWidth(AzStyleBorderBottomWidthValue),
        BoxShadowLeft(AzBoxShadowPreDisplayItemValue),
        BoxShadowRight(AzBoxShadowPreDisplayItemValue),
        BoxShadowTop(AzBoxShadowPreDisplayItemValue),
        BoxShadowBottom(AzBoxShadowPreDisplayItemValue),
    }
    /// Re-export of rust-allocated (stack based) `Dom` struct
    #[repr(C)] pub struct AzDom {
        pub root: AzNodeData,
        pub children: AzDomVec,
        pub estimated_total_children: usize,
    }
    /// Re-export of rust-allocated (stack based) `GlTextureNode` struct
    #[repr(C)] pub struct AzGlTextureNode {
        pub callback: AzGlCallback,
        pub data: AzRefAny,
    }
    /// Re-export of rust-allocated (stack based) `IFrameNode` struct
    #[repr(C)] pub struct AzIFrameNode {
        pub callback: AzIFrameCallback,
        pub data: AzRefAny,
    }
    /// Re-export of rust-allocated (stack based) `CallbackData` struct
    #[repr(C)] pub struct AzCallbackData {
        pub event: AzEventFilter,
        pub callback: AzCallback,
        pub data: AzRefAny,
    }
    /// Re-export of rust-allocated (stack based) `OverrideProperty` struct
    #[repr(C)] pub struct AzOverrideProperty {
        pub property_id: AzString,
        pub override_value: AzCssProperty,
    }
    /// Represents one single DOM node (node type, classes, ids and callbacks are stored here)
    #[repr(C)] pub struct AzNodeData {
        pub node_type: AzNodeType,
        pub ids: AzStringVec,
        pub classes: AzStringVec,
        pub callbacks: AzCallbackDataVec,
        pub dynamic_css_overrides: AzOverridePropertyVec,
        pub is_draggable: bool,
        pub tab_index: AzOptionTabIndex,
    }
    /// List of core DOM node types built-into by `azul`
    #[repr(C, u8)] pub enum AzNodeType {
        Div,
        Body,
        Label(AzString),
        Text(AzTextId),
        Image(AzImageId),
        GlTexture(AzGlTextureNode),
        IFrame(AzIFrameNode),
    }
    /// When to call a callback action - `On::MouseOver`, `On::MouseOut`, etc.
    #[repr(C)] pub enum AzOn {
        MouseOver,
        MouseDown,
        LeftMouseDown,
        MiddleMouseDown,
        RightMouseDown,
        MouseUp,
        LeftMouseUp,
        MiddleMouseUp,
        RightMouseUp,
        MouseEnter,
        MouseLeave,
        Scroll,
        TextInput,
        VirtualKeyDown,
        VirtualKeyUp,
        HoveredFile,
        DroppedFile,
        HoveredFileCancelled,
        FocusReceived,
        FocusLost,
    }
    /// Re-export of rust-allocated (stack based) `EventFilter` struct
    #[repr(C, u8)] pub enum AzEventFilter {
        Hover(AzHoverEventFilter),
        Not(AzNotEventFilter),
        Focus(AzFocusEventFilter),
        Window(AzWindowEventFilter),
    }
    /// Re-export of rust-allocated (stack based) `HoverEventFilter` struct
    #[repr(C)] pub enum AzHoverEventFilter {
        MouseOver,
        MouseDown,
        LeftMouseDown,
        RightMouseDown,
        MiddleMouseDown,
        MouseUp,
        LeftMouseUp,
        RightMouseUp,
        MiddleMouseUp,
        MouseEnter,
        MouseLeave,
        Scroll,
        ScrollStart,
        ScrollEnd,
        TextInput,
        VirtualKeyDown,
        VirtualKeyUp,
        HoveredFile,
        DroppedFile,
        HoveredFileCancelled,
    }
    /// Re-export of rust-allocated (stack based) `FocusEventFilter` struct
    #[repr(C)] pub enum AzFocusEventFilter {
        MouseOver,
        MouseDown,
        LeftMouseDown,
        RightMouseDown,
        MiddleMouseDown,
        MouseUp,
        LeftMouseUp,
        RightMouseUp,
        MiddleMouseUp,
        MouseEnter,
        MouseLeave,
        Scroll,
        ScrollStart,
        ScrollEnd,
        TextInput,
        VirtualKeyDown,
        VirtualKeyUp,
        FocusReceived,
        FocusLost,
    }
    /// Re-export of rust-allocated (stack based) `NotEventFilter` struct
    #[repr(C, u8)] pub enum AzNotEventFilter {
        Hover(AzHoverEventFilter),
        Focus(AzFocusEventFilter),
    }
    /// Re-export of rust-allocated (stack based) `WindowEventFilter` struct
    #[repr(C)] pub enum AzWindowEventFilter {
        MouseOver,
        MouseDown,
        LeftMouseDown,
        RightMouseDown,
        MiddleMouseDown,
        MouseUp,
        LeftMouseUp,
        RightMouseUp,
        MiddleMouseUp,
        MouseEnter,
        MouseLeave,
        Scroll,
        ScrollStart,
        ScrollEnd,
        TextInput,
        VirtualKeyDown,
        VirtualKeyUp,
        HoveredFile,
        DroppedFile,
        HoveredFileCancelled,
    }
    /// Re-export of rust-allocated (stack based) `TabIndex` struct
    #[repr(C, u8)] pub enum AzTabIndex {
        Auto,
        OverrideInParent(usize),
        NoKeyboardFocus,
    }
    /// Re-export of rust-allocated (stack based) `GlType` struct
    #[repr(C)] pub enum AzGlType {
        Gl,
        Gles,
    }
    /// Re-export of rust-allocated (stack based) `DebugMessage` struct
    #[repr(C)] pub struct AzDebugMessage {
        pub message: AzString,
        pub source: u32,
        pub ty: u32,
        pub id: u32,
        pub severity: u32,
    }
    /// C-ABI stable reexport of `&[u8]`
    #[repr(C)] pub struct AzU8VecRef {
        pub(crate) ptr: *const u8,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [u8]`
    #[repr(C)] pub struct AzU8VecRefMut {
        pub(crate) ptr: *mut u8,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[f32]`
    #[repr(C)] pub struct AzF32VecRef {
        pub(crate) ptr: *const f32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[i32]`
    #[repr(C)] pub struct AzI32VecRef {
        pub(crate) ptr: *const i32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[GLuint]` aka `&[u32]`
    #[repr(C)] pub struct AzGLuintVecRef {
        pub(crate) ptr: *const u32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[GLenum]` aka `&[u32]`
    #[repr(C)] pub struct AzGLenumVecRef {
        pub(crate) ptr: *const u32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [GLint]` aka `&mut [i32]`
    #[repr(C)] pub struct AzGLintVecRefMut {
        pub(crate) ptr: *mut i32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [GLint64]` aka `&mut [i64]`
    #[repr(C)] pub struct AzGLint64VecRefMut {
        pub(crate) ptr: *mut i64,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [GLboolean]` aka `&mut [u8]`
    #[repr(C)] pub struct AzGLbooleanVecRefMut {
        pub(crate) ptr: *mut u8,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&mut [GLfloat]` aka `&mut [f32]`
    #[repr(C)] pub struct AzGLfloatVecRefMut {
        pub(crate) ptr: *mut f32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[Refstr]` aka `&mut [&str]`
    #[repr(C)] pub struct AzRefstrVecRef {
        pub(crate) ptr: *const AzRefstr,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&str`
    #[repr(C)] pub struct AzRefstr {
        pub(crate) ptr: *const u8,
        pub len: usize,
    }
    /// C-ABI stable reexport of `(U8Vec, u32)`
    #[repr(C)] pub struct AzGetProgramBinaryReturn {
        pub _0: AzU8Vec,
        pub _1: u32,
    }
    /// C-ABI stable reexport of `(i32, u32, AzString)`
    #[repr(C)] pub struct AzGetActiveAttribReturn {
        pub _0: i32,
        pub _1: u32,
        pub _2: AzString,
    }
    /// C-ABI stable reexport of `*const gleam::gl::GLsync`
    #[repr(C)] pub struct AzGLsyncPtr {
        pub(crate) ptr: *const c_void,
    }
    /// C-ABI stable reexport of `(i32, u32, AzString)`
    #[repr(C)] pub struct AzGetActiveUniformReturn {
        pub _0: i32,
        pub _1: u32,
        pub _2: AzString,
    }
    /// Re-export of rust-allocated (stack based) `GlContextPtr` struct
    #[repr(C)] pub struct AzGlContextPtr {
        pub(crate) ptr: *const c_void,
    }
    /// Re-export of rust-allocated (stack based) `Texture` struct
    #[repr(C)] pub struct AzTexture {
        pub texture_id: u32,
        pub flags: AzTextureFlags,
        pub size: AzLogicalSize,
        pub gl_context: AzGlContextPtr,
    }
    /// Re-export of rust-allocated (stack based) `TextureFlags` struct
    #[repr(C)] pub struct AzTextureFlags {
        pub is_opaque: bool,
        pub is_video_texture: bool,
    }
    /// Re-export of rust-allocated (stack based) `TextId` struct
    #[repr(C)] pub struct AzTextId {
        pub id: usize,
    }
    /// Re-export of rust-allocated (stack based) `ImageId` struct
    #[repr(C)] pub struct AzImageId {
        pub id: usize,
    }

    impl PartialEq for AzImageId { fn eq(&self, rhs: &AzImageId) -> bool { (crate::dll::get_azul_dll().az_image_id_partial_eq)(self, rhs) } }

    impl Eq for AzImageId { }

    impl PartialOrd for AzImageId { fn partial_cmp(&self, rhs: &AzImageId) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_image_id_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzImageId { fn cmp(&self, rhs: &AzImageId) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_image_id_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzImageId { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_image_id_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `FontId` struct
    #[repr(C)] pub struct AzFontId {
        pub id: usize,
    }
    /// Re-export of rust-allocated (stack based) `ImageSource` struct
    #[repr(C, u8)] pub enum AzImageSource {
        Embedded(AzU8Vec),
        File(AzString),
        Raw(AzRawImage),
    }
    /// Re-export of rust-allocated (stack based) `FontSource` struct
    #[repr(C, u8)] pub enum AzFontSource {
        Embedded(AzU8Vec),
        File(AzString),
        System(AzString),
    }
    /// Re-export of rust-allocated (stack based) `RawImage` struct
    #[repr(C)] pub struct AzRawImage {
        pub pixels: AzU8Vec,
        pub width: usize,
        pub height: usize,
        pub data_format: AzRawImageFormat,
    }
    /// Re-export of rust-allocated (stack based) `RawImageFormat` struct
    #[repr(C)] pub enum AzRawImageFormat {
        R8,
        R16,
        RG16,
        BGRA8,
        RGBAF32,
        RG8,
        RGBAI32,
        RGBA8,
    }
    /// Pointer to rust-allocated `Box<DropCheckPtr>` struct
    #[repr(C)] pub struct AzDropCheckPtrPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Pointer to rust-allocated `Box<ArcMutexRefAny>` struct
    #[repr(C)] pub struct AzArcMutexRefAnyPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `Timer` struct
    #[repr(C)] pub struct AzTimer {
        pub created: AzInstantPtr,
        pub last_run: AzOptionInstantPtr,
        pub delay: AzOptionInstantPtr,
        pub interval: AzOptionDuration,
        pub timeout: AzOptionDuration,
        pub callback: AzTimerCallback,
    }
    /// Pointer to rust-allocated `Box<Task>` struct
    #[repr(C)] pub struct AzTaskPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Pointer to rust-allocated `Box<Thread>` struct
    #[repr(C)] pub struct AzThreadPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Pointer to rust-allocated `Box<DropCheck>` struct
    #[repr(C)] pub struct AzDropCheckPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `TimerId` struct
    #[repr(C)] pub struct AzTimerId {
        pub id: usize,
    }
    /// Should a timer terminate or not - used to remove active timers
    #[repr(C)] pub enum AzTerminateTimer {
        Terminate,
        Continue,
    }
    /// Re-export of rust-allocated (stack based) `BlockError` struct
    #[repr(C)] pub enum AzBlockError {
        ArcUnlockError,
        ThreadJoinError,
        MutexIntoInnerError,
    }
    /// Re-export of rust-allocated (stack based) `TaskBarIcon` struct
    #[repr(C)] pub struct AzTaskBarIcon {
        pub key: AzIconKey,
        pub rgba_bytes: AzU8Vec,
    }
    /// Re-export of rust-allocated (stack based) `XWindowType` struct
    #[repr(C)] pub enum AzXWindowType {
        Desktop,
        Dock,
        Toolbar,
        Menu,
        Utility,
        Splash,
        Dialog,
        DropdownMenu,
        PopupMenu,
        Tooltip,
        Notification,
        Combo,
        Dnd,
        Normal,
    }
    /// Re-export of rust-allocated (stack based) `PhysicalPositionI32` struct
    #[repr(C)] pub struct AzPhysicalPositionI32 {
        pub x: i32,
        pub y: i32,
    }

    impl PartialEq for AzPhysicalPositionI32 { fn eq(&self, rhs: &AzPhysicalPositionI32) -> bool { (crate::dll::get_azul_dll().az_physical_position_i32_partial_eq)(self, rhs) } }

    impl Eq for AzPhysicalPositionI32 { }

    impl PartialOrd for AzPhysicalPositionI32 { fn partial_cmp(&self, rhs: &AzPhysicalPositionI32) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_physical_position_i32_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzPhysicalPositionI32 { fn cmp(&self, rhs: &AzPhysicalPositionI32) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_physical_position_i32_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzPhysicalPositionI32 { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_physical_position_i32_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `PhysicalSizeU32` struct
    #[repr(C)] pub struct AzPhysicalSizeU32 {
        pub width: u32,
        pub height: u32,
    }

    impl PartialEq for AzPhysicalSizeU32 { fn eq(&self, rhs: &AzPhysicalSizeU32) -> bool { (crate::dll::get_azul_dll().az_physical_size_u32_partial_eq)(self, rhs) } }

    impl Eq for AzPhysicalSizeU32 { }

    impl PartialOrd for AzPhysicalSizeU32 { fn partial_cmp(&self, rhs: &AzPhysicalSizeU32) -> Option<std::cmp::Ordering> { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_physical_size_u32_partial_cmp)(self, rhs) { 1 => Some(Less), 2 => Some(Equal), 3 => Some(Greater), _ => None } } }

    impl Ord for AzPhysicalSizeU32 { fn cmp(&self, rhs: &AzPhysicalSizeU32) -> std::cmp::Ordering { use std::cmp::Ordering::*; match (crate::dll::get_azul_dll().az_physical_size_u32_cmp)(self, rhs) { 0 => Less, 1 => Equal, _ => Greater } } }

    impl std::hash::Hash for AzPhysicalSizeU32 { fn hash<H: std::hash::Hasher>(&self, state: &mut H) { ((crate::dll::get_azul_dll().az_physical_size_u32_hash)(self)).hash(state) } }
    /// Re-export of rust-allocated (stack based) `LogicalPosition` struct
    #[repr(C)] pub struct AzLogicalPosition {
        pub x: f32,
        pub y: f32,
    }
    /// Re-export of rust-allocated (stack based) `IconKey` struct
    #[repr(C)] pub struct AzIconKey {
        pub id: usize,
    }
    /// Re-export of rust-allocated (stack based) `SmallWindowIconBytes` struct
    #[repr(C)] pub struct AzSmallWindowIconBytes {
        pub key: AzIconKey,
        pub rgba_bytes: AzU8Vec,
    }
    /// Re-export of rust-allocated (stack based) `LargeWindowIconBytes` struct
    #[repr(C)] pub struct AzLargeWindowIconBytes {
        pub key: AzIconKey,
        pub rgba_bytes: AzU8Vec,
    }
    /// Re-export of rust-allocated (stack based) `WindowIcon` struct
    #[repr(C, u8)] pub enum AzWindowIcon {
        Small(AzSmallWindowIconBytes),
        Large(AzLargeWindowIconBytes),
    }
    /// Re-export of rust-allocated (stack based) `VirtualKeyCode` struct
    #[repr(C)] pub enum AzVirtualKeyCode {
        Key1,
        Key2,
        Key3,
        Key4,
        Key5,
        Key6,
        Key7,
        Key8,
        Key9,
        Key0,
        A,
        B,
        C,
        D,
        E,
        F,
        G,
        H,
        I,
        J,
        K,
        L,
        M,
        N,
        O,
        P,
        Q,
        R,
        S,
        T,
        U,
        V,
        W,
        X,
        Y,
        Z,
        Escape,
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        F11,
        F12,
        F13,
        F14,
        F15,
        F16,
        F17,
        F18,
        F19,
        F20,
        F21,
        F22,
        F23,
        F24,
        Snapshot,
        Scroll,
        Pause,
        Insert,
        Home,
        Delete,
        End,
        PageDown,
        PageUp,
        Left,
        Up,
        Right,
        Down,
        Back,
        Return,
        Space,
        Compose,
        Caret,
        Numlock,
        Numpad0,
        Numpad1,
        Numpad2,
        Numpad3,
        Numpad4,
        Numpad5,
        Numpad6,
        Numpad7,
        Numpad8,
        Numpad9,
        AbntC1,
        AbntC2,
        Add,
        Apostrophe,
        Apps,
        At,
        Ax,
        Backslash,
        Calculator,
        Capital,
        Colon,
        Comma,
        Convert,
        Decimal,
        Divide,
        Equals,
        Grave,
        Kana,
        Kanji,
        LAlt,
        LBracket,
        LControl,
        LShift,
        LWin,
        Mail,
        MediaSelect,
        MediaStop,
        Minus,
        Multiply,
        Mute,
        MyComputer,
        NavigateForward,
        NavigateBackward,
        NextTrack,
        NoConvert,
        NumpadComma,
        NumpadEnter,
        NumpadEquals,
        OEM102,
        Period,
        PlayPause,
        Power,
        PrevTrack,
        RAlt,
        RBracket,
        RControl,
        RShift,
        RWin,
        Semicolon,
        Slash,
        Sleep,
        Stop,
        Subtract,
        Sysrq,
        Tab,
        Underline,
        Unlabeled,
        VolumeDown,
        VolumeUp,
        Wake,
        WebBack,
        WebFavorites,
        WebForward,
        WebHome,
        WebRefresh,
        WebSearch,
        WebStop,
        Yen,
        Copy,
        Paste,
        Cut,
    }
    /// Re-export of rust-allocated (stack based) `AcceleratorKey` struct
    #[repr(C, u8)] pub enum AzAcceleratorKey {
        Ctrl,
        Alt,
        Shift,
        Key(AzVirtualKeyCode),
    }
    /// Re-export of rust-allocated (stack based) `WindowSize` struct
    #[repr(C)] pub struct AzWindowSize {
        pub dimensions: AzLogicalSize,
        pub hidpi_factor: f32,
        pub winit_hidpi_factor: f32,
        pub min_dimensions: AzOptionLogicalSize,
        pub max_dimensions: AzOptionLogicalSize,
    }
    /// Re-export of rust-allocated (stack based) `WindowFlags` struct
    #[repr(C)] pub struct AzWindowFlags {
        pub is_maximized: bool,
        pub is_fullscreen: bool,
        pub has_decorations: bool,
        pub is_visible: bool,
        pub is_always_on_top: bool,
        pub is_resizable: bool,
    }
    /// Re-export of rust-allocated (stack based) `DebugState` struct
    #[repr(C)] pub struct AzDebugState {
        pub profiler_dbg: bool,
        pub render_target_dbg: bool,
        pub texture_cache_dbg: bool,
        pub gpu_time_queries: bool,
        pub gpu_sample_queries: bool,
        pub disable_batching: bool,
        pub epochs: bool,
        pub compact_profiler: bool,
        pub echo_driver_messages: bool,
        pub new_frame_indicator: bool,
        pub new_scene_indicator: bool,
        pub show_overdraw: bool,
        pub gpu_cache_dbg: bool,
    }
    /// Re-export of rust-allocated (stack based) `KeyboardState` struct
    #[repr(C)] pub struct AzKeyboardState {
        pub shift_down: bool,
        pub ctrl_down: bool,
        pub alt_down: bool,
        pub super_down: bool,
        pub current_char: AzOptionChar,
        pub current_virtual_keycode: AzOptionVirtualKeyCode,
        pub pressed_virtual_keycodes: AzVirtualKeyCodeVec,
        pub pressed_scancodes: AzScanCodeVec,
    }
    /// Re-export of rust-allocated (stack based) `MouseCursorType` struct
    #[repr(C)] pub enum AzMouseCursorType {
        Default,
        Crosshair,
        Hand,
        Arrow,
        Move,
        Text,
        Wait,
        Help,
        Progress,
        NotAllowed,
        ContextMenu,
        Cell,
        VerticalText,
        Alias,
        Copy,
        NoDrop,
        Grab,
        Grabbing,
        AllScroll,
        ZoomIn,
        ZoomOut,
        EResize,
        NResize,
        NeResize,
        NwResize,
        SResize,
        SeResize,
        SwResize,
        WResize,
        EwResize,
        NsResize,
        NeswResize,
        NwseResize,
        ColResize,
        RowResize,
    }
    /// Re-export of rust-allocated (stack based) `CursorPosition` struct
    #[repr(C, u8)] pub enum AzCursorPosition {
        OutOfWindow,
        Uninitialized,
        InWindow(AzLogicalPosition),
    }
    /// Re-export of rust-allocated (stack based) `MouseState` struct
    #[repr(C)] pub struct AzMouseState {
        pub mouse_cursor_type: AzOptionMouseCursorType,
        pub cursor_position: AzCursorPosition,
        pub is_cursor_locked: bool,
        pub left_down: bool,
        pub right_down: bool,
        pub middle_down: bool,
        pub scroll_x: AzOptionF32,
        pub scroll_y: AzOptionF32,
    }
    /// Re-export of rust-allocated (stack based) `PlatformSpecificOptions` struct
    #[repr(C)] pub struct AzPlatformSpecificOptions {
        pub windows_options: AzWindowsWindowOptions,
        pub linux_options: AzLinuxWindowOptions,
        pub mac_options: AzMacWindowOptions,
        pub wasm_options: AzWasmWindowOptions,
    }
    /// Re-export of rust-allocated (stack based) `WindowsWindowOptions` struct
    #[repr(C)] pub struct AzWindowsWindowOptions {
        pub no_redirection_bitmap: bool,
        pub window_icon: AzOptionWindowIcon,
        pub taskbar_icon: AzOptionTaskBarIcon,
        pub parent_window: AzOptionHwndHandle,
    }
    /// Re-export of rust-allocated (stack based) `WaylandTheme` struct
    #[repr(C)] pub struct AzWaylandTheme {
        pub primary_color_active: [u8;4],
        pub primary_color_inactive: [u8;4],
        pub secondary_color_active: [u8;4],
        pub secondary_color_inactive: [u8;4],
        pub close_button_color_idle: [u8;4],
        pub close_button_color_hovered: [u8;4],
        pub close_button_color_disabled: [u8;4],
        pub maximize_button_color_idle: [u8;4],
        pub maximize_button_color_hovered: [u8;4],
        pub maximize_button_color_disabled: [u8;4],
        pub minimize_button_color_idle: [u8;4],
        pub minimize_button_color_hovered: [u8;4],
        pub minimize_button_color_disabled: [u8;4],
    }
    /// Re-export of rust-allocated (stack based) `RendererType` struct
    #[repr(C, u8)] pub enum AzRendererType {
        Default,
        ForceHardware,
        ForceSoftware,
        Custom(AzGlContextPtr),
    }
    /// Re-export of rust-allocated (stack based) `StringPair` struct
    #[repr(C)] pub struct AzStringPair {
        pub key: AzString,
        pub value: AzString,
    }
    /// Re-export of rust-allocated (stack based) `LinuxWindowOptions` struct
    #[repr(C)] pub struct AzLinuxWindowOptions {
        pub x11_visual: AzOptionX11Visual,
        pub x11_screen: AzOptionI32,
        pub x11_wm_classes: AzStringPairVec,
        pub x11_override_redirect: bool,
        pub x11_window_types: AzXWindowTypeVec,
        pub x11_gtk_theme_variant: AzOptionString,
        pub x11_resize_increments: AzOptionLogicalSize,
        pub x11_base_size: AzOptionLogicalSize,
        pub wayland_app_id: AzOptionString,
        pub wayland_theme: AzOptionWaylandTheme,
        pub request_user_attention: bool,
        pub window_icon: AzOptionWindowIcon,
    }
    /// Re-export of rust-allocated (stack based) `MacWindowOptions` struct
    #[repr(C)] pub struct AzMacWindowOptions {
        pub request_user_attention: bool,
    }
    /// Re-export of rust-allocated (stack based) `WasmWindowOptions` struct
    #[repr(C)] pub struct AzWasmWindowOptions {
    }
    /// Re-export of rust-allocated (stack based) `FullScreenMode` struct
    #[repr(C)] pub enum AzFullScreenMode {
        SlowFullScreen,
        FastFullScreen,
        SlowWindowed,
        FastWindowed,
    }
    /// Re-export of rust-allocated (stack based) `WindowState` struct
    #[repr(C)] pub struct AzWindowState {
        pub title: AzString,
        pub size: AzWindowSize,
        pub position: AzOptionPhysicalPositionI32,
        pub flags: AzWindowFlags,
        pub debug_state: AzDebugState,
        pub keyboard_state: AzKeyboardState,
        pub mouse_state: AzMouseState,
        pub ime_position: AzOptionLogicalPosition,
        pub platform_specific_options: AzPlatformSpecificOptions,
        pub css: AzCss,
    }
    /// Re-export of rust-allocated (stack based) `LogicalSize` struct
    #[repr(C)] pub struct AzLogicalSize {
        pub width: f32,
        pub height: f32,
    }
    /// Re-export of rust-allocated (stack based) `HotReloadOptions` struct
    #[repr(C)] pub struct AzHotReloadOptions {
        pub path: AzString,
        pub reload_interval: AzDuration,
        pub apply_native_css: bool,
    }
    /// Re-export of rust-allocated (stack based) `WindowCreateOptions` struct
    #[repr(C)] pub struct AzWindowCreateOptions {
        pub state: AzWindowState,
        pub renderer_type: AzRendererType,
        pub hot_reload: AzOptionHotReloadOptions,
    }


    use libloading_mini::Library;
    pub struct AzulDll {
        pub lib: Library,
        pub az_string_from_utf8_unchecked: extern "C" fn(_:  *const u8, _:  usize) -> AzString,
        pub az_string_from_utf8_lossy: extern "C" fn(_:  *const u8, _:  usize) -> AzString,
        pub az_string_into_bytes: extern "C" fn(_:  AzString) -> AzU8Vec,
        pub az_string_delete: extern "C" fn(_:  &mut AzString),
        pub az_string_deep_copy: extern "C" fn(_:  &AzString) -> AzString,
        pub az_string_fmt_debug: extern "C" fn(_:  &AzString) -> AzString,
        pub az_x_window_type_vec_copy_from: extern "C" fn(_:  *mut AzXWindowType, _:  usize) -> AzXWindowTypeVec,
        pub az_x_window_type_vec_delete: extern "C" fn(_:  &mut AzXWindowTypeVec),
        pub az_x_window_type_vec_deep_copy: extern "C" fn(_:  &AzXWindowTypeVec) -> AzXWindowTypeVec,
        pub az_x_window_type_vec_fmt_debug: extern "C" fn(_:  &AzXWindowTypeVec) -> AzString,
        pub az_virtual_key_code_vec_copy_from: extern "C" fn(_:  *mut AzVirtualKeyCode, _:  usize) -> AzVirtualKeyCodeVec,
        pub az_virtual_key_code_vec_delete: extern "C" fn(_:  &mut AzVirtualKeyCodeVec),
        pub az_virtual_key_code_vec_deep_copy: extern "C" fn(_:  &AzVirtualKeyCodeVec) -> AzVirtualKeyCodeVec,
        pub az_virtual_key_code_vec_fmt_debug: extern "C" fn(_:  &AzVirtualKeyCodeVec) -> AzString,
        pub az_scan_code_vec_copy_from: extern "C" fn(_:  *mut u32, _:  usize) -> AzScanCodeVec,
        pub az_scan_code_vec_delete: extern "C" fn(_:  &mut AzScanCodeVec),
        pub az_scan_code_vec_deep_copy: extern "C" fn(_:  &AzScanCodeVec) -> AzScanCodeVec,
        pub az_scan_code_vec_fmt_debug: extern "C" fn(_:  &AzScanCodeVec) -> AzString,
        pub az_css_declaration_vec_copy_from: extern "C" fn(_:  *mut AzCssDeclaration, _:  usize) -> AzCssDeclarationVec,
        pub az_css_declaration_vec_delete: extern "C" fn(_:  &mut AzCssDeclarationVec),
        pub az_css_declaration_vec_deep_copy: extern "C" fn(_:  &AzCssDeclarationVec) -> AzCssDeclarationVec,
        pub az_css_declaration_vec_fmt_debug: extern "C" fn(_:  &AzCssDeclarationVec) -> AzString,
        pub az_css_path_selector_vec_copy_from: extern "C" fn(_:  *mut AzCssPathSelector, _:  usize) -> AzCssPathSelectorVec,
        pub az_css_path_selector_vec_delete: extern "C" fn(_:  &mut AzCssPathSelectorVec),
        pub az_css_path_selector_vec_deep_copy: extern "C" fn(_:  &AzCssPathSelectorVec) -> AzCssPathSelectorVec,
        pub az_css_path_selector_vec_fmt_debug: extern "C" fn(_:  &AzCssPathSelectorVec) -> AzString,
        pub az_stylesheet_vec_copy_from: extern "C" fn(_:  *mut AzStylesheet, _:  usize) -> AzStylesheetVec,
        pub az_stylesheet_vec_delete: extern "C" fn(_:  &mut AzStylesheetVec),
        pub az_stylesheet_vec_deep_copy: extern "C" fn(_:  &AzStylesheetVec) -> AzStylesheetVec,
        pub az_stylesheet_vec_fmt_debug: extern "C" fn(_:  &AzStylesheetVec) -> AzString,
        pub az_css_rule_block_vec_copy_from: extern "C" fn(_:  *mut AzCssRuleBlock, _:  usize) -> AzCssRuleBlockVec,
        pub az_css_rule_block_vec_delete: extern "C" fn(_:  &mut AzCssRuleBlockVec),
        pub az_css_rule_block_vec_deep_copy: extern "C" fn(_:  &AzCssRuleBlockVec) -> AzCssRuleBlockVec,
        pub az_css_rule_block_vec_fmt_debug: extern "C" fn(_:  &AzCssRuleBlockVec) -> AzString,
        pub az_u8_vec_copy_from: extern "C" fn(_:  *mut u8, _:  usize) -> AzU8Vec,
        pub az_u8_vec_delete: extern "C" fn(_:  &mut AzU8Vec),
        pub az_u8_vec_deep_copy: extern "C" fn(_:  &AzU8Vec) -> AzU8Vec,
        pub az_u8_vec_fmt_debug: extern "C" fn(_:  &AzU8Vec) -> AzString,
        pub az_callback_data_vec_copy_from: extern "C" fn(_:  *mut AzCallbackData, _:  usize) -> AzCallbackDataVec,
        pub az_callback_data_vec_delete: extern "C" fn(_:  &mut AzCallbackDataVec),
        pub az_callback_data_vec_deep_copy: extern "C" fn(_:  &AzCallbackDataVec) -> AzCallbackDataVec,
        pub az_callback_data_vec_fmt_debug: extern "C" fn(_:  &AzCallbackDataVec) -> AzString,
        pub az_debug_message_vec_copy_from: extern "C" fn(_:  *mut AzDebugMessage, _:  usize) -> AzDebugMessageVec,
        pub az_debug_message_vec_delete: extern "C" fn(_:  &mut AzDebugMessageVec),
        pub az_debug_message_vec_deep_copy: extern "C" fn(_:  &AzDebugMessageVec) -> AzDebugMessageVec,
        pub az_debug_message_vec_fmt_debug: extern "C" fn(_:  &AzDebugMessageVec) -> AzString,
        pub az_g_luint_vec_copy_from: extern "C" fn(_:  *mut u32, _:  usize) -> AzGLuintVec,
        pub az_g_luint_vec_delete: extern "C" fn(_:  &mut AzGLuintVec),
        pub az_g_luint_vec_deep_copy: extern "C" fn(_:  &AzGLuintVec) -> AzGLuintVec,
        pub az_g_luint_vec_fmt_debug: extern "C" fn(_:  &AzGLuintVec) -> AzString,
        pub az_g_lint_vec_copy_from: extern "C" fn(_:  *mut i32, _:  usize) -> AzGLintVec,
        pub az_g_lint_vec_delete: extern "C" fn(_:  &mut AzGLintVec),
        pub az_g_lint_vec_deep_copy: extern "C" fn(_:  &AzGLintVec) -> AzGLintVec,
        pub az_g_lint_vec_fmt_debug: extern "C" fn(_:  &AzGLintVec) -> AzString,
        pub az_override_property_vec_copy_from: extern "C" fn(_:  *mut AzOverrideProperty, _:  usize) -> AzOverridePropertyVec,
        pub az_override_property_vec_delete: extern "C" fn(_:  &mut AzOverridePropertyVec),
        pub az_override_property_vec_deep_copy: extern "C" fn(_:  &AzOverridePropertyVec) -> AzOverridePropertyVec,
        pub az_override_property_vec_fmt_debug: extern "C" fn(_:  &AzOverridePropertyVec) -> AzString,
        pub az_dom_vec_copy_from: extern "C" fn(_:  *mut AzDom, _:  usize) -> AzDomVec,
        pub az_dom_vec_delete: extern "C" fn(_:  &mut AzDomVec),
        pub az_dom_vec_deep_copy: extern "C" fn(_:  &AzDomVec) -> AzDomVec,
        pub az_dom_vec_fmt_debug: extern "C" fn(_:  &AzDomVec) -> AzString,
        pub az_string_vec_copy_from: extern "C" fn(_:  *mut AzString, _:  usize) -> AzStringVec,
        pub az_string_vec_delete: extern "C" fn(_:  &mut AzStringVec),
        pub az_string_vec_deep_copy: extern "C" fn(_:  &AzStringVec) -> AzStringVec,
        pub az_string_vec_fmt_debug: extern "C" fn(_:  &AzStringVec) -> AzString,
        pub az_string_pair_vec_copy_from: extern "C" fn(_:  *mut AzStringPair, _:  usize) -> AzStringPairVec,
        pub az_string_pair_vec_delete: extern "C" fn(_:  &mut AzStringPairVec),
        pub az_string_pair_vec_deep_copy: extern "C" fn(_:  &AzStringPairVec) -> AzStringPairVec,
        pub az_string_pair_vec_fmt_debug: extern "C" fn(_:  &AzStringPairVec) -> AzString,
        pub az_gradient_stop_pre_vec_copy_from: extern "C" fn(_:  *mut AzGradientStopPre, _:  usize) -> AzGradientStopPreVec,
        pub az_gradient_stop_pre_vec_delete: extern "C" fn(_:  &mut AzGradientStopPreVec),
        pub az_gradient_stop_pre_vec_deep_copy: extern "C" fn(_:  &AzGradientStopPreVec) -> AzGradientStopPreVec,
        pub az_gradient_stop_pre_vec_fmt_debug: extern "C" fn(_:  &AzGradientStopPreVec) -> AzString,
        pub az_option_wayland_theme_delete: extern "C" fn(_:  &mut AzOptionWaylandTheme),
        pub az_option_wayland_theme_deep_copy: extern "C" fn(_:  &AzOptionWaylandTheme) -> AzOptionWaylandTheme,
        pub az_option_wayland_theme_fmt_debug: extern "C" fn(_:  &AzOptionWaylandTheme) -> AzString,
        pub az_option_task_bar_icon_delete: extern "C" fn(_:  &mut AzOptionTaskBarIcon),
        pub az_option_task_bar_icon_deep_copy: extern "C" fn(_:  &AzOptionTaskBarIcon) -> AzOptionTaskBarIcon,
        pub az_option_task_bar_icon_fmt_debug: extern "C" fn(_:  &AzOptionTaskBarIcon) -> AzString,
        pub az_option_hwnd_handle_delete: extern "C" fn(_:  &mut AzOptionHwndHandle),
        pub az_option_hwnd_handle_deep_copy: extern "C" fn(_:  &AzOptionHwndHandle) -> AzOptionHwndHandle,
        pub az_option_hwnd_handle_fmt_debug: extern "C" fn(_:  &AzOptionHwndHandle) -> AzString,
        pub az_option_logical_position_delete: extern "C" fn(_:  &mut AzOptionLogicalPosition),
        pub az_option_logical_position_deep_copy: extern "C" fn(_:  &AzOptionLogicalPosition) -> AzOptionLogicalPosition,
        pub az_option_logical_position_fmt_debug: extern "C" fn(_:  &AzOptionLogicalPosition) -> AzString,
        pub az_option_hot_reload_options_delete: extern "C" fn(_:  &mut AzOptionHotReloadOptions),
        pub az_option_hot_reload_options_deep_copy: extern "C" fn(_:  &AzOptionHotReloadOptions) -> AzOptionHotReloadOptions,
        pub az_option_hot_reload_options_fmt_debug: extern "C" fn(_:  &AzOptionHotReloadOptions) -> AzString,
        pub az_option_physical_position_i32_delete: extern "C" fn(_:  &mut AzOptionPhysicalPositionI32),
        pub az_option_physical_position_i32_deep_copy: extern "C" fn(_:  &AzOptionPhysicalPositionI32) -> AzOptionPhysicalPositionI32,
        pub az_option_physical_position_i32_fmt_debug: extern "C" fn(_:  &AzOptionPhysicalPositionI32) -> AzString,
        pub az_option_window_icon_delete: extern "C" fn(_:  &mut AzOptionWindowIcon),
        pub az_option_window_icon_deep_copy: extern "C" fn(_:  &AzOptionWindowIcon) -> AzOptionWindowIcon,
        pub az_option_window_icon_fmt_debug: extern "C" fn(_:  &AzOptionWindowIcon) -> AzString,
        pub az_option_string_delete: extern "C" fn(_:  &mut AzOptionString),
        pub az_option_string_deep_copy: extern "C" fn(_:  &AzOptionString) -> AzOptionString,
        pub az_option_string_fmt_debug: extern "C" fn(_:  &AzOptionString) -> AzString,
        pub az_option_x11_visual_delete: extern "C" fn(_:  &mut AzOptionX11Visual),
        pub az_option_x11_visual_deep_copy: extern "C" fn(_:  &AzOptionX11Visual) -> AzOptionX11Visual,
        pub az_option_x11_visual_fmt_debug: extern "C" fn(_:  &AzOptionX11Visual) -> AzString,
        pub az_option_i32_delete: extern "C" fn(_:  &mut AzOptionI32),
        pub az_option_i32_deep_copy: extern "C" fn(_:  &AzOptionI32) -> AzOptionI32,
        pub az_option_i32_fmt_debug: extern "C" fn(_:  &AzOptionI32) -> AzString,
        pub az_option_f32_delete: extern "C" fn(_:  &mut AzOptionF32),
        pub az_option_f32_deep_copy: extern "C" fn(_:  &AzOptionF32) -> AzOptionF32,
        pub az_option_f32_fmt_debug: extern "C" fn(_:  &AzOptionF32) -> AzString,
        pub az_option_mouse_cursor_type_delete: extern "C" fn(_:  &mut AzOptionMouseCursorType),
        pub az_option_mouse_cursor_type_deep_copy: extern "C" fn(_:  &AzOptionMouseCursorType) -> AzOptionMouseCursorType,
        pub az_option_mouse_cursor_type_fmt_debug: extern "C" fn(_:  &AzOptionMouseCursorType) -> AzString,
        pub az_option_logical_size_delete: extern "C" fn(_:  &mut AzOptionLogicalSize),
        pub az_option_logical_size_deep_copy: extern "C" fn(_:  &AzOptionLogicalSize) -> AzOptionLogicalSize,
        pub az_option_logical_size_fmt_debug: extern "C" fn(_:  &AzOptionLogicalSize) -> AzString,
        pub az_option_char_delete: extern "C" fn(_:  &mut AzOptionChar),
        pub az_option_char_deep_copy: extern "C" fn(_:  &AzOptionChar) -> AzOptionChar,
        pub az_option_char_fmt_debug: extern "C" fn(_:  &AzOptionChar) -> AzString,
        pub az_option_virtual_key_code_delete: extern "C" fn(_:  &mut AzOptionVirtualKeyCode),
        pub az_option_virtual_key_code_deep_copy: extern "C" fn(_:  &AzOptionVirtualKeyCode) -> AzOptionVirtualKeyCode,
        pub az_option_virtual_key_code_fmt_debug: extern "C" fn(_:  &AzOptionVirtualKeyCode) -> AzString,
        pub az_option_percentage_value_delete: extern "C" fn(_:  &mut AzOptionPercentageValue),
        pub az_option_percentage_value_deep_copy: extern "C" fn(_:  &AzOptionPercentageValue) -> AzOptionPercentageValue,
        pub az_option_percentage_value_fmt_debug: extern "C" fn(_:  &AzOptionPercentageValue) -> AzString,
        pub az_option_dom_delete: extern "C" fn(_:  &mut AzOptionDom),
        pub az_option_dom_deep_copy: extern "C" fn(_:  &AzOptionDom) -> AzOptionDom,
        pub az_option_dom_fmt_debug: extern "C" fn(_:  &AzOptionDom) -> AzString,
        pub az_option_texture_delete: extern "C" fn(_:  &mut AzOptionTexture),
        pub az_option_texture_fmt_debug: extern "C" fn(_:  &AzOptionTexture) -> AzString,
        pub az_option_tab_index_delete: extern "C" fn(_:  &mut AzOptionTabIndex),
        pub az_option_tab_index_deep_copy: extern "C" fn(_:  &AzOptionTabIndex) -> AzOptionTabIndex,
        pub az_option_tab_index_fmt_debug: extern "C" fn(_:  &AzOptionTabIndex) -> AzString,
        pub az_option_duration_delete: extern "C" fn(_:  &mut AzOptionDuration),
        pub az_option_duration_deep_copy: extern "C" fn(_:  &AzOptionDuration) -> AzOptionDuration,
        pub az_option_duration_fmt_debug: extern "C" fn(_:  &AzOptionDuration) -> AzString,
        pub az_option_instant_ptr_delete: extern "C" fn(_:  &mut AzOptionInstantPtr),
        pub az_option_instant_ptr_deep_copy: extern "C" fn(_:  &AzOptionInstantPtr) -> AzOptionInstantPtr,
        pub az_option_instant_ptr_fmt_debug: extern "C" fn(_:  &AzOptionInstantPtr) -> AzString,
        pub az_option_usize_delete: extern "C" fn(_:  &mut AzOptionUsize),
        pub az_option_usize_deep_copy: extern "C" fn(_:  &AzOptionUsize) -> AzOptionUsize,
        pub az_option_usize_fmt_debug: extern "C" fn(_:  &AzOptionUsize) -> AzString,
        pub az_option_u8_vec_ref_delete: extern "C" fn(_:  &mut AzOptionU8VecRef),
        pub az_option_u8_vec_ref_fmt_debug: extern "C" fn(_:  &AzOptionU8VecRef) -> AzString,
        pub az_result_ref_any_block_error_delete: extern "C" fn(_:  &mut AzResultRefAnyBlockError),
        pub az_result_ref_any_block_error_deep_copy: extern "C" fn(_:  &AzResultRefAnyBlockError) -> AzResultRefAnyBlockError,
        pub az_result_ref_any_block_error_fmt_debug: extern "C" fn(_:  &AzResultRefAnyBlockError) -> AzString,
        pub az_instant_ptr_now: extern "C" fn() -> AzInstantPtr,
        pub az_instant_ptr_delete: extern "C" fn(_:  &mut AzInstantPtr),
        pub az_instant_ptr_fmt_debug: extern "C" fn(_:  &AzInstantPtr) -> AzString,
        pub az_instant_ptr_partial_eq: extern "C" fn(_:  &AzInstantPtr, _:  &AzInstantPtr) -> bool,
        pub az_instant_ptr_partial_cmp: extern "C" fn(_:  &AzInstantPtr, _:  &AzInstantPtr) -> u8,
        pub az_instant_ptr_cmp: extern "C" fn(_:  &AzInstantPtr, _:  &AzInstantPtr) -> u8,
        pub az_duration_delete: extern "C" fn(_:  &mut AzDuration),
        pub az_duration_deep_copy: extern "C" fn(_:  &AzDuration) -> AzDuration,
        pub az_duration_fmt_debug: extern "C" fn(_:  &AzDuration) -> AzString,
        pub az_app_config_ptr_default: extern "C" fn() -> AzAppConfigPtr,
        pub az_app_config_ptr_delete: extern "C" fn(_:  &mut AzAppConfigPtr),
        pub az_app_config_ptr_fmt_debug: extern "C" fn(_:  &AzAppConfigPtr) -> AzString,
        pub az_app_ptr_new: extern "C" fn(_:  AzRefAny, _:  AzAppConfigPtr, _:  AzLayoutCallbackType) -> AzAppPtr,
        pub az_app_ptr_run: extern "C" fn(_:  AzAppPtr, _:  AzWindowCreateOptions),
        pub az_app_ptr_delete: extern "C" fn(_:  &mut AzAppPtr),
        pub az_app_ptr_fmt_debug: extern "C" fn(_:  &AzAppPtr) -> AzString,
        pub az_hidpi_adjusted_bounds_get_logical_size: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzLogicalSize,
        pub az_hidpi_adjusted_bounds_get_physical_size: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzPhysicalSizeU32,
        pub az_hidpi_adjusted_bounds_get_hidpi_factor: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> f32,
        pub az_hidpi_adjusted_bounds_delete: extern "C" fn(_:  &mut AzHidpiAdjustedBounds),
        pub az_hidpi_adjusted_bounds_deep_copy: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzHidpiAdjustedBounds,
        pub az_hidpi_adjusted_bounds_fmt_debug: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzString,
        pub az_layout_callback_delete: extern "C" fn(_:  &mut AzLayoutCallback),
        pub az_layout_callback_deep_copy: extern "C" fn(_:  &AzLayoutCallback) -> AzLayoutCallback,
        pub az_layout_callback_fmt_debug: extern "C" fn(_:  &AzLayoutCallback) -> AzString,
        pub az_callback_delete: extern "C" fn(_:  &mut AzCallback),
        pub az_callback_deep_copy: extern "C" fn(_:  &AzCallback) -> AzCallback,
        pub az_callback_fmt_debug: extern "C" fn(_:  &AzCallback) -> AzString,
        pub az_callback_partial_eq: extern "C" fn(_:  &AzCallback, _:  &AzCallback) -> bool,
        pub az_callback_partial_cmp: extern "C" fn(_:  &AzCallback, _:  &AzCallback) -> u8,
        pub az_callback_cmp: extern "C" fn(_:  &AzCallback, _:  &AzCallback) -> u8,
        pub az_callback_hash: extern "C" fn(_:  &AzCallback) -> u64,
        pub az_callback_info_ptr_get_state: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzRefAny,
        pub az_callback_info_ptr_get_keyboard_state: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzKeyboardState,
        pub az_callback_info_ptr_get_mouse_state: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzMouseState,
        pub az_callback_info_ptr_set_window_state: extern "C" fn(_:  &mut AzCallbackInfoPtr, _:  AzWindowState),
        pub az_callback_info_ptr_delete: extern "C" fn(_:  &mut AzCallbackInfoPtr),
        pub az_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzString,
        pub az_update_screen_delete: extern "C" fn(_:  &mut AzUpdateScreen),
        pub az_update_screen_deep_copy: extern "C" fn(_:  &AzUpdateScreen) -> AzUpdateScreen,
        pub az_update_screen_fmt_debug: extern "C" fn(_:  &AzUpdateScreen) -> AzString,
        pub az_i_frame_callback_delete: extern "C" fn(_:  &mut AzIFrameCallback),
        pub az_i_frame_callback_deep_copy: extern "C" fn(_:  &AzIFrameCallback) -> AzIFrameCallback,
        pub az_i_frame_callback_fmt_debug: extern "C" fn(_:  &AzIFrameCallback) -> AzString,
        pub az_i_frame_callback_info_ptr_get_state: extern "C" fn(_:  &AzIFrameCallbackInfoPtr) -> AzRefAny,
        pub az_i_frame_callback_info_ptr_get_bounds: extern "C" fn(_:  &AzIFrameCallbackInfoPtr) -> AzHidpiAdjustedBounds,
        pub az_i_frame_callback_info_ptr_delete: extern "C" fn(_:  &mut AzIFrameCallbackInfoPtr),
        pub az_i_frame_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzIFrameCallbackInfoPtr) -> AzString,
        pub az_i_frame_callback_return_delete: extern "C" fn(_:  &mut AzIFrameCallbackReturn),
        pub az_i_frame_callback_return_deep_copy: extern "C" fn(_:  &AzIFrameCallbackReturn) -> AzIFrameCallbackReturn,
        pub az_i_frame_callback_return_fmt_debug: extern "C" fn(_:  &AzIFrameCallbackReturn) -> AzString,
        pub az_gl_callback_delete: extern "C" fn(_:  &mut AzGlCallback),
        pub az_gl_callback_deep_copy: extern "C" fn(_:  &AzGlCallback) -> AzGlCallback,
        pub az_gl_callback_fmt_debug: extern "C" fn(_:  &AzGlCallback) -> AzString,
        pub az_gl_callback_info_ptr_get_state: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzRefAny,
        pub az_gl_callback_info_ptr_get_bounds: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzHidpiAdjustedBounds,
        pub az_gl_callback_info_ptr_delete: extern "C" fn(_:  &mut AzGlCallbackInfoPtr),
        pub az_gl_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzString,
        pub az_gl_callback_return_delete: extern "C" fn(_:  &mut AzGlCallbackReturn),
        pub az_gl_callback_return_fmt_debug: extern "C" fn(_:  &AzGlCallbackReturn) -> AzString,
        pub az_timer_callback_delete: extern "C" fn(_:  &mut AzTimerCallback),
        pub az_timer_callback_deep_copy: extern "C" fn(_:  &AzTimerCallback) -> AzTimerCallback,
        pub az_timer_callback_fmt_debug: extern "C" fn(_:  &AzTimerCallback) -> AzString,
        pub az_timer_callback_type_ptr_delete: extern "C" fn(_:  &mut AzTimerCallbackTypePtr),
        pub az_timer_callback_type_ptr_fmt_debug: extern "C" fn(_:  &AzTimerCallbackTypePtr) -> AzString,
        pub az_timer_callback_info_ptr_get_state: extern "C" fn(_:  &AzTimerCallbackInfoPtr) -> AzRefAny,
        pub az_timer_callback_info_ptr_delete: extern "C" fn(_:  &mut AzTimerCallbackInfoPtr),
        pub az_timer_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzTimerCallbackInfoPtr) -> AzString,
        pub az_timer_callback_return_delete: extern "C" fn(_:  &mut AzTimerCallbackReturn),
        pub az_timer_callback_return_deep_copy: extern "C" fn(_:  &AzTimerCallbackReturn) -> AzTimerCallbackReturn,
        pub az_timer_callback_return_fmt_debug: extern "C" fn(_:  &AzTimerCallbackReturn) -> AzString,
        pub az_ref_any_sharing_info_can_be_shared: extern "C" fn(_:  &AzRefAnySharingInfo) -> bool,
        pub az_ref_any_sharing_info_can_be_shared_mut: extern "C" fn(_:  &AzRefAnySharingInfo) -> bool,
        pub az_ref_any_sharing_info_increase_ref: extern "C" fn(_:  &mut AzRefAnySharingInfo),
        pub az_ref_any_sharing_info_decrease_ref: extern "C" fn(_:  &mut AzRefAnySharingInfo),
        pub az_ref_any_sharing_info_increase_refmut: extern "C" fn(_:  &mut AzRefAnySharingInfo),
        pub az_ref_any_sharing_info_decrease_refmut: extern "C" fn(_:  &mut AzRefAnySharingInfo),
        pub az_ref_any_sharing_info_delete: extern "C" fn(_:  &mut AzRefAnySharingInfo),
        pub az_ref_any_sharing_info_fmt_debug: extern "C" fn(_:  &AzRefAnySharingInfo) -> AzString,
        pub az_ref_any_new_c: extern "C" fn(_:  *const c_void, _:  usize, _:  u64, _:  AzString, _:  AzRefAnyDestructorType) -> AzRefAny,
        pub az_ref_any_is_type: extern "C" fn(_:  &AzRefAny, _:  u64) -> bool,
        pub az_ref_any_get_type_name: extern "C" fn(_:  &AzRefAny) -> AzString,
        pub az_ref_any_can_be_shared: extern "C" fn(_:  &AzRefAny) -> bool,
        pub az_ref_any_can_be_shared_mut: extern "C" fn(_:  &AzRefAny) -> bool,
        pub az_ref_any_increase_ref: extern "C" fn(_:  &AzRefAny),
        pub az_ref_any_decrease_ref: extern "C" fn(_:  &AzRefAny),
        pub az_ref_any_increase_refmut: extern "C" fn(_:  &AzRefAny),
        pub az_ref_any_decrease_refmut: extern "C" fn(_:  &AzRefAny),
        pub az_ref_any_delete: extern "C" fn(_:  &mut AzRefAny),
        pub az_ref_any_deep_copy: extern "C" fn(_:  &AzRefAny) -> AzRefAny,
        pub az_ref_any_fmt_debug: extern "C" fn(_:  &AzRefAny) -> AzString,
        pub az_ref_any_partial_eq: extern "C" fn(_:  &AzRefAny, _:  &AzRefAny) -> bool,
        pub az_ref_any_partial_cmp: extern "C" fn(_:  &AzRefAny, _:  &AzRefAny) -> u8,
        pub az_ref_any_cmp: extern "C" fn(_:  &AzRefAny, _:  &AzRefAny) -> u8,
        pub az_ref_any_hash: extern "C" fn(_:  &AzRefAny) -> u64,
        pub az_layout_info_ptr_delete: extern "C" fn(_:  &mut AzLayoutInfoPtr),
        pub az_layout_info_ptr_fmt_debug: extern "C" fn(_:  &AzLayoutInfoPtr) -> AzString,
        pub az_css_rule_block_delete: extern "C" fn(_:  &mut AzCssRuleBlock),
        pub az_css_rule_block_deep_copy: extern "C" fn(_:  &AzCssRuleBlock) -> AzCssRuleBlock,
        pub az_css_rule_block_fmt_debug: extern "C" fn(_:  &AzCssRuleBlock) -> AzString,
        pub az_css_declaration_delete: extern "C" fn(_:  &mut AzCssDeclaration),
        pub az_css_declaration_deep_copy: extern "C" fn(_:  &AzCssDeclaration) -> AzCssDeclaration,
        pub az_css_declaration_fmt_debug: extern "C" fn(_:  &AzCssDeclaration) -> AzString,
        pub az_dynamic_css_property_delete: extern "C" fn(_:  &mut AzDynamicCssProperty),
        pub az_dynamic_css_property_deep_copy: extern "C" fn(_:  &AzDynamicCssProperty) -> AzDynamicCssProperty,
        pub az_dynamic_css_property_fmt_debug: extern "C" fn(_:  &AzDynamicCssProperty) -> AzString,
        pub az_css_path_delete: extern "C" fn(_:  &mut AzCssPath),
        pub az_css_path_deep_copy: extern "C" fn(_:  &AzCssPath) -> AzCssPath,
        pub az_css_path_fmt_debug: extern "C" fn(_:  &AzCssPath) -> AzString,
        pub az_css_path_selector_delete: extern "C" fn(_:  &mut AzCssPathSelector),
        pub az_css_path_selector_deep_copy: extern "C" fn(_:  &AzCssPathSelector) -> AzCssPathSelector,
        pub az_css_path_selector_fmt_debug: extern "C" fn(_:  &AzCssPathSelector) -> AzString,
        pub az_node_type_path_delete: extern "C" fn(_:  &mut AzNodeTypePath),
        pub az_node_type_path_deep_copy: extern "C" fn(_:  &AzNodeTypePath) -> AzNodeTypePath,
        pub az_node_type_path_fmt_debug: extern "C" fn(_:  &AzNodeTypePath) -> AzString,
        pub az_css_path_pseudo_selector_delete: extern "C" fn(_:  &mut AzCssPathPseudoSelector),
        pub az_css_path_pseudo_selector_deep_copy: extern "C" fn(_:  &AzCssPathPseudoSelector) -> AzCssPathPseudoSelector,
        pub az_css_path_pseudo_selector_fmt_debug: extern "C" fn(_:  &AzCssPathPseudoSelector) -> AzString,
        pub az_css_nth_child_selector_delete: extern "C" fn(_:  &mut AzCssNthChildSelector),
        pub az_css_nth_child_selector_deep_copy: extern "C" fn(_:  &AzCssNthChildSelector) -> AzCssNthChildSelector,
        pub az_css_nth_child_selector_fmt_debug: extern "C" fn(_:  &AzCssNthChildSelector) -> AzString,
        pub az_css_nth_child_pattern_delete: extern "C" fn(_:  &mut AzCssNthChildPattern),
        pub az_css_nth_child_pattern_deep_copy: extern "C" fn(_:  &AzCssNthChildPattern) -> AzCssNthChildPattern,
        pub az_css_nth_child_pattern_fmt_debug: extern "C" fn(_:  &AzCssNthChildPattern) -> AzString,
        pub az_stylesheet_delete: extern "C" fn(_:  &mut AzStylesheet),
        pub az_stylesheet_deep_copy: extern "C" fn(_:  &AzStylesheet) -> AzStylesheet,
        pub az_stylesheet_fmt_debug: extern "C" fn(_:  &AzStylesheet) -> AzString,
        pub az_css_native: extern "C" fn() -> AzCss,
        pub az_css_empty: extern "C" fn() -> AzCss,
        pub az_css_from_string: extern "C" fn(_:  AzString) -> AzCss,
        pub az_css_override_native: extern "C" fn(_:  AzString) -> AzCss,
        pub az_css_delete: extern "C" fn(_:  &mut AzCss),
        pub az_css_deep_copy: extern "C" fn(_:  &AzCss) -> AzCss,
        pub az_css_fmt_debug: extern "C" fn(_:  &AzCss) -> AzString,
        pub az_color_u_delete: extern "C" fn(_:  &mut AzColorU),
        pub az_color_u_deep_copy: extern "C" fn(_:  &AzColorU) -> AzColorU,
        pub az_color_u_fmt_debug: extern "C" fn(_:  &AzColorU) -> AzString,
        pub az_size_metric_delete: extern "C" fn(_:  &mut AzSizeMetric),
        pub az_size_metric_deep_copy: extern "C" fn(_:  &AzSizeMetric) -> AzSizeMetric,
        pub az_size_metric_fmt_debug: extern "C" fn(_:  &AzSizeMetric) -> AzString,
        pub az_float_value_delete: extern "C" fn(_:  &mut AzFloatValue),
        pub az_float_value_deep_copy: extern "C" fn(_:  &AzFloatValue) -> AzFloatValue,
        pub az_float_value_fmt_debug: extern "C" fn(_:  &AzFloatValue) -> AzString,
        pub az_pixel_value_delete: extern "C" fn(_:  &mut AzPixelValue),
        pub az_pixel_value_deep_copy: extern "C" fn(_:  &AzPixelValue) -> AzPixelValue,
        pub az_pixel_value_fmt_debug: extern "C" fn(_:  &AzPixelValue) -> AzString,
        pub az_pixel_value_no_percent_delete: extern "C" fn(_:  &mut AzPixelValueNoPercent),
        pub az_pixel_value_no_percent_deep_copy: extern "C" fn(_:  &AzPixelValueNoPercent) -> AzPixelValueNoPercent,
        pub az_pixel_value_no_percent_fmt_debug: extern "C" fn(_:  &AzPixelValueNoPercent) -> AzString,
        pub az_box_shadow_clip_mode_delete: extern "C" fn(_:  &mut AzBoxShadowClipMode),
        pub az_box_shadow_clip_mode_deep_copy: extern "C" fn(_:  &AzBoxShadowClipMode) -> AzBoxShadowClipMode,
        pub az_box_shadow_clip_mode_fmt_debug: extern "C" fn(_:  &AzBoxShadowClipMode) -> AzString,
        pub az_box_shadow_pre_display_item_delete: extern "C" fn(_:  &mut AzBoxShadowPreDisplayItem),
        pub az_box_shadow_pre_display_item_deep_copy: extern "C" fn(_:  &AzBoxShadowPreDisplayItem) -> AzBoxShadowPreDisplayItem,
        pub az_box_shadow_pre_display_item_fmt_debug: extern "C" fn(_:  &AzBoxShadowPreDisplayItem) -> AzString,
        pub az_layout_align_content_delete: extern "C" fn(_:  &mut AzLayoutAlignContent),
        pub az_layout_align_content_deep_copy: extern "C" fn(_:  &AzLayoutAlignContent) -> AzLayoutAlignContent,
        pub az_layout_align_content_fmt_debug: extern "C" fn(_:  &AzLayoutAlignContent) -> AzString,
        pub az_layout_align_items_delete: extern "C" fn(_:  &mut AzLayoutAlignItems),
        pub az_layout_align_items_deep_copy: extern "C" fn(_:  &AzLayoutAlignItems) -> AzLayoutAlignItems,
        pub az_layout_align_items_fmt_debug: extern "C" fn(_:  &AzLayoutAlignItems) -> AzString,
        pub az_layout_bottom_delete: extern "C" fn(_:  &mut AzLayoutBottom),
        pub az_layout_bottom_deep_copy: extern "C" fn(_:  &AzLayoutBottom) -> AzLayoutBottom,
        pub az_layout_bottom_fmt_debug: extern "C" fn(_:  &AzLayoutBottom) -> AzString,
        pub az_layout_box_sizing_delete: extern "C" fn(_:  &mut AzLayoutBoxSizing),
        pub az_layout_box_sizing_deep_copy: extern "C" fn(_:  &AzLayoutBoxSizing) -> AzLayoutBoxSizing,
        pub az_layout_box_sizing_fmt_debug: extern "C" fn(_:  &AzLayoutBoxSizing) -> AzString,
        pub az_layout_direction_delete: extern "C" fn(_:  &mut AzLayoutDirection),
        pub az_layout_direction_deep_copy: extern "C" fn(_:  &AzLayoutDirection) -> AzLayoutDirection,
        pub az_layout_direction_fmt_debug: extern "C" fn(_:  &AzLayoutDirection) -> AzString,
        pub az_layout_display_delete: extern "C" fn(_:  &mut AzLayoutDisplay),
        pub az_layout_display_deep_copy: extern "C" fn(_:  &AzLayoutDisplay) -> AzLayoutDisplay,
        pub az_layout_display_fmt_debug: extern "C" fn(_:  &AzLayoutDisplay) -> AzString,
        pub az_layout_flex_grow_delete: extern "C" fn(_:  &mut AzLayoutFlexGrow),
        pub az_layout_flex_grow_deep_copy: extern "C" fn(_:  &AzLayoutFlexGrow) -> AzLayoutFlexGrow,
        pub az_layout_flex_grow_fmt_debug: extern "C" fn(_:  &AzLayoutFlexGrow) -> AzString,
        pub az_layout_flex_shrink_delete: extern "C" fn(_:  &mut AzLayoutFlexShrink),
        pub az_layout_flex_shrink_deep_copy: extern "C" fn(_:  &AzLayoutFlexShrink) -> AzLayoutFlexShrink,
        pub az_layout_flex_shrink_fmt_debug: extern "C" fn(_:  &AzLayoutFlexShrink) -> AzString,
        pub az_layout_float_delete: extern "C" fn(_:  &mut AzLayoutFloat),
        pub az_layout_float_deep_copy: extern "C" fn(_:  &AzLayoutFloat) -> AzLayoutFloat,
        pub az_layout_float_fmt_debug: extern "C" fn(_:  &AzLayoutFloat) -> AzString,
        pub az_layout_height_delete: extern "C" fn(_:  &mut AzLayoutHeight),
        pub az_layout_height_deep_copy: extern "C" fn(_:  &AzLayoutHeight) -> AzLayoutHeight,
        pub az_layout_height_fmt_debug: extern "C" fn(_:  &AzLayoutHeight) -> AzString,
        pub az_layout_justify_content_delete: extern "C" fn(_:  &mut AzLayoutJustifyContent),
        pub az_layout_justify_content_deep_copy: extern "C" fn(_:  &AzLayoutJustifyContent) -> AzLayoutJustifyContent,
        pub az_layout_justify_content_fmt_debug: extern "C" fn(_:  &AzLayoutJustifyContent) -> AzString,
        pub az_layout_left_delete: extern "C" fn(_:  &mut AzLayoutLeft),
        pub az_layout_left_deep_copy: extern "C" fn(_:  &AzLayoutLeft) -> AzLayoutLeft,
        pub az_layout_left_fmt_debug: extern "C" fn(_:  &AzLayoutLeft) -> AzString,
        pub az_layout_margin_bottom_delete: extern "C" fn(_:  &mut AzLayoutMarginBottom),
        pub az_layout_margin_bottom_deep_copy: extern "C" fn(_:  &AzLayoutMarginBottom) -> AzLayoutMarginBottom,
        pub az_layout_margin_bottom_fmt_debug: extern "C" fn(_:  &AzLayoutMarginBottom) -> AzString,
        pub az_layout_margin_left_delete: extern "C" fn(_:  &mut AzLayoutMarginLeft),
        pub az_layout_margin_left_deep_copy: extern "C" fn(_:  &AzLayoutMarginLeft) -> AzLayoutMarginLeft,
        pub az_layout_margin_left_fmt_debug: extern "C" fn(_:  &AzLayoutMarginLeft) -> AzString,
        pub az_layout_margin_right_delete: extern "C" fn(_:  &mut AzLayoutMarginRight),
        pub az_layout_margin_right_deep_copy: extern "C" fn(_:  &AzLayoutMarginRight) -> AzLayoutMarginRight,
        pub az_layout_margin_right_fmt_debug: extern "C" fn(_:  &AzLayoutMarginRight) -> AzString,
        pub az_layout_margin_top_delete: extern "C" fn(_:  &mut AzLayoutMarginTop),
        pub az_layout_margin_top_deep_copy: extern "C" fn(_:  &AzLayoutMarginTop) -> AzLayoutMarginTop,
        pub az_layout_margin_top_fmt_debug: extern "C" fn(_:  &AzLayoutMarginTop) -> AzString,
        pub az_layout_max_height_delete: extern "C" fn(_:  &mut AzLayoutMaxHeight),
        pub az_layout_max_height_deep_copy: extern "C" fn(_:  &AzLayoutMaxHeight) -> AzLayoutMaxHeight,
        pub az_layout_max_height_fmt_debug: extern "C" fn(_:  &AzLayoutMaxHeight) -> AzString,
        pub az_layout_max_width_delete: extern "C" fn(_:  &mut AzLayoutMaxWidth),
        pub az_layout_max_width_deep_copy: extern "C" fn(_:  &AzLayoutMaxWidth) -> AzLayoutMaxWidth,
        pub az_layout_max_width_fmt_debug: extern "C" fn(_:  &AzLayoutMaxWidth) -> AzString,
        pub az_layout_min_height_delete: extern "C" fn(_:  &mut AzLayoutMinHeight),
        pub az_layout_min_height_deep_copy: extern "C" fn(_:  &AzLayoutMinHeight) -> AzLayoutMinHeight,
        pub az_layout_min_height_fmt_debug: extern "C" fn(_:  &AzLayoutMinHeight) -> AzString,
        pub az_layout_min_width_delete: extern "C" fn(_:  &mut AzLayoutMinWidth),
        pub az_layout_min_width_deep_copy: extern "C" fn(_:  &AzLayoutMinWidth) -> AzLayoutMinWidth,
        pub az_layout_min_width_fmt_debug: extern "C" fn(_:  &AzLayoutMinWidth) -> AzString,
        pub az_layout_padding_bottom_delete: extern "C" fn(_:  &mut AzLayoutPaddingBottom),
        pub az_layout_padding_bottom_deep_copy: extern "C" fn(_:  &AzLayoutPaddingBottom) -> AzLayoutPaddingBottom,
        pub az_layout_padding_bottom_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingBottom) -> AzString,
        pub az_layout_padding_left_delete: extern "C" fn(_:  &mut AzLayoutPaddingLeft),
        pub az_layout_padding_left_deep_copy: extern "C" fn(_:  &AzLayoutPaddingLeft) -> AzLayoutPaddingLeft,
        pub az_layout_padding_left_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingLeft) -> AzString,
        pub az_layout_padding_right_delete: extern "C" fn(_:  &mut AzLayoutPaddingRight),
        pub az_layout_padding_right_deep_copy: extern "C" fn(_:  &AzLayoutPaddingRight) -> AzLayoutPaddingRight,
        pub az_layout_padding_right_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingRight) -> AzString,
        pub az_layout_padding_top_delete: extern "C" fn(_:  &mut AzLayoutPaddingTop),
        pub az_layout_padding_top_deep_copy: extern "C" fn(_:  &AzLayoutPaddingTop) -> AzLayoutPaddingTop,
        pub az_layout_padding_top_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingTop) -> AzString,
        pub az_layout_position_delete: extern "C" fn(_:  &mut AzLayoutPosition),
        pub az_layout_position_deep_copy: extern "C" fn(_:  &AzLayoutPosition) -> AzLayoutPosition,
        pub az_layout_position_fmt_debug: extern "C" fn(_:  &AzLayoutPosition) -> AzString,
        pub az_layout_right_delete: extern "C" fn(_:  &mut AzLayoutRight),
        pub az_layout_right_deep_copy: extern "C" fn(_:  &AzLayoutRight) -> AzLayoutRight,
        pub az_layout_right_fmt_debug: extern "C" fn(_:  &AzLayoutRight) -> AzString,
        pub az_layout_top_delete: extern "C" fn(_:  &mut AzLayoutTop),
        pub az_layout_top_deep_copy: extern "C" fn(_:  &AzLayoutTop) -> AzLayoutTop,
        pub az_layout_top_fmt_debug: extern "C" fn(_:  &AzLayoutTop) -> AzString,
        pub az_layout_width_delete: extern "C" fn(_:  &mut AzLayoutWidth),
        pub az_layout_width_deep_copy: extern "C" fn(_:  &AzLayoutWidth) -> AzLayoutWidth,
        pub az_layout_width_fmt_debug: extern "C" fn(_:  &AzLayoutWidth) -> AzString,
        pub az_layout_wrap_delete: extern "C" fn(_:  &mut AzLayoutWrap),
        pub az_layout_wrap_deep_copy: extern "C" fn(_:  &AzLayoutWrap) -> AzLayoutWrap,
        pub az_layout_wrap_fmt_debug: extern "C" fn(_:  &AzLayoutWrap) -> AzString,
        pub az_overflow_delete: extern "C" fn(_:  &mut AzOverflow),
        pub az_overflow_deep_copy: extern "C" fn(_:  &AzOverflow) -> AzOverflow,
        pub az_overflow_fmt_debug: extern "C" fn(_:  &AzOverflow) -> AzString,
        pub az_percentage_value_delete: extern "C" fn(_:  &mut AzPercentageValue),
        pub az_percentage_value_deep_copy: extern "C" fn(_:  &AzPercentageValue) -> AzPercentageValue,
        pub az_percentage_value_fmt_debug: extern "C" fn(_:  &AzPercentageValue) -> AzString,
        pub az_gradient_stop_pre_delete: extern "C" fn(_:  &mut AzGradientStopPre),
        pub az_gradient_stop_pre_deep_copy: extern "C" fn(_:  &AzGradientStopPre) -> AzGradientStopPre,
        pub az_gradient_stop_pre_fmt_debug: extern "C" fn(_:  &AzGradientStopPre) -> AzString,
        pub az_direction_corner_delete: extern "C" fn(_:  &mut AzDirectionCorner),
        pub az_direction_corner_deep_copy: extern "C" fn(_:  &AzDirectionCorner) -> AzDirectionCorner,
        pub az_direction_corner_fmt_debug: extern "C" fn(_:  &AzDirectionCorner) -> AzString,
        pub az_direction_corners_delete: extern "C" fn(_:  &mut AzDirectionCorners),
        pub az_direction_corners_deep_copy: extern "C" fn(_:  &AzDirectionCorners) -> AzDirectionCorners,
        pub az_direction_corners_fmt_debug: extern "C" fn(_:  &AzDirectionCorners) -> AzString,
        pub az_direction_delete: extern "C" fn(_:  &mut AzDirection),
        pub az_direction_deep_copy: extern "C" fn(_:  &AzDirection) -> AzDirection,
        pub az_direction_fmt_debug: extern "C" fn(_:  &AzDirection) -> AzString,
        pub az_extend_mode_delete: extern "C" fn(_:  &mut AzExtendMode),
        pub az_extend_mode_deep_copy: extern "C" fn(_:  &AzExtendMode) -> AzExtendMode,
        pub az_extend_mode_fmt_debug: extern "C" fn(_:  &AzExtendMode) -> AzString,
        pub az_linear_gradient_delete: extern "C" fn(_:  &mut AzLinearGradient),
        pub az_linear_gradient_deep_copy: extern "C" fn(_:  &AzLinearGradient) -> AzLinearGradient,
        pub az_linear_gradient_fmt_debug: extern "C" fn(_:  &AzLinearGradient) -> AzString,
        pub az_shape_delete: extern "C" fn(_:  &mut AzShape),
        pub az_shape_deep_copy: extern "C" fn(_:  &AzShape) -> AzShape,
        pub az_shape_fmt_debug: extern "C" fn(_:  &AzShape) -> AzString,
        pub az_radial_gradient_delete: extern "C" fn(_:  &mut AzRadialGradient),
        pub az_radial_gradient_deep_copy: extern "C" fn(_:  &AzRadialGradient) -> AzRadialGradient,
        pub az_radial_gradient_fmt_debug: extern "C" fn(_:  &AzRadialGradient) -> AzString,
        pub az_css_image_id_delete: extern "C" fn(_:  &mut AzCssImageId),
        pub az_css_image_id_deep_copy: extern "C" fn(_:  &AzCssImageId) -> AzCssImageId,
        pub az_css_image_id_fmt_debug: extern "C" fn(_:  &AzCssImageId) -> AzString,
        pub az_style_background_content_delete: extern "C" fn(_:  &mut AzStyleBackgroundContent),
        pub az_style_background_content_deep_copy: extern "C" fn(_:  &AzStyleBackgroundContent) -> AzStyleBackgroundContent,
        pub az_style_background_content_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundContent) -> AzString,
        pub az_background_position_horizontal_delete: extern "C" fn(_:  &mut AzBackgroundPositionHorizontal),
        pub az_background_position_horizontal_deep_copy: extern "C" fn(_:  &AzBackgroundPositionHorizontal) -> AzBackgroundPositionHorizontal,
        pub az_background_position_horizontal_fmt_debug: extern "C" fn(_:  &AzBackgroundPositionHorizontal) -> AzString,
        pub az_background_position_vertical_delete: extern "C" fn(_:  &mut AzBackgroundPositionVertical),
        pub az_background_position_vertical_deep_copy: extern "C" fn(_:  &AzBackgroundPositionVertical) -> AzBackgroundPositionVertical,
        pub az_background_position_vertical_fmt_debug: extern "C" fn(_:  &AzBackgroundPositionVertical) -> AzString,
        pub az_style_background_position_delete: extern "C" fn(_:  &mut AzStyleBackgroundPosition),
        pub az_style_background_position_deep_copy: extern "C" fn(_:  &AzStyleBackgroundPosition) -> AzStyleBackgroundPosition,
        pub az_style_background_position_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundPosition) -> AzString,
        pub az_style_background_repeat_delete: extern "C" fn(_:  &mut AzStyleBackgroundRepeat),
        pub az_style_background_repeat_deep_copy: extern "C" fn(_:  &AzStyleBackgroundRepeat) -> AzStyleBackgroundRepeat,
        pub az_style_background_repeat_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundRepeat) -> AzString,
        pub az_style_background_size_delete: extern "C" fn(_:  &mut AzStyleBackgroundSize),
        pub az_style_background_size_deep_copy: extern "C" fn(_:  &AzStyleBackgroundSize) -> AzStyleBackgroundSize,
        pub az_style_background_size_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundSize) -> AzString,
        pub az_style_border_bottom_color_delete: extern "C" fn(_:  &mut AzStyleBorderBottomColor),
        pub az_style_border_bottom_color_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomColor) -> AzStyleBorderBottomColor,
        pub az_style_border_bottom_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomColor) -> AzString,
        pub az_style_border_bottom_left_radius_delete: extern "C" fn(_:  &mut AzStyleBorderBottomLeftRadius),
        pub az_style_border_bottom_left_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadius,
        pub az_style_border_bottom_left_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomLeftRadius) -> AzString,
        pub az_style_border_bottom_right_radius_delete: extern "C" fn(_:  &mut AzStyleBorderBottomRightRadius),
        pub az_style_border_bottom_right_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadius,
        pub az_style_border_bottom_right_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomRightRadius) -> AzString,
        pub az_border_style_delete: extern "C" fn(_:  &mut AzBorderStyle),
        pub az_border_style_deep_copy: extern "C" fn(_:  &AzBorderStyle) -> AzBorderStyle,
        pub az_border_style_fmt_debug: extern "C" fn(_:  &AzBorderStyle) -> AzString,
        pub az_style_border_bottom_style_delete: extern "C" fn(_:  &mut AzStyleBorderBottomStyle),
        pub az_style_border_bottom_style_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomStyle) -> AzStyleBorderBottomStyle,
        pub az_style_border_bottom_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomStyle) -> AzString,
        pub az_style_border_bottom_width_delete: extern "C" fn(_:  &mut AzStyleBorderBottomWidth),
        pub az_style_border_bottom_width_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomWidth) -> AzStyleBorderBottomWidth,
        pub az_style_border_bottom_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomWidth) -> AzString,
        pub az_style_border_left_color_delete: extern "C" fn(_:  &mut AzStyleBorderLeftColor),
        pub az_style_border_left_color_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftColor) -> AzStyleBorderLeftColor,
        pub az_style_border_left_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftColor) -> AzString,
        pub az_style_border_left_style_delete: extern "C" fn(_:  &mut AzStyleBorderLeftStyle),
        pub az_style_border_left_style_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftStyle) -> AzStyleBorderLeftStyle,
        pub az_style_border_left_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftStyle) -> AzString,
        pub az_style_border_left_width_delete: extern "C" fn(_:  &mut AzStyleBorderLeftWidth),
        pub az_style_border_left_width_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftWidth) -> AzStyleBorderLeftWidth,
        pub az_style_border_left_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftWidth) -> AzString,
        pub az_style_border_right_color_delete: extern "C" fn(_:  &mut AzStyleBorderRightColor),
        pub az_style_border_right_color_deep_copy: extern "C" fn(_:  &AzStyleBorderRightColor) -> AzStyleBorderRightColor,
        pub az_style_border_right_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightColor) -> AzString,
        pub az_style_border_right_style_delete: extern "C" fn(_:  &mut AzStyleBorderRightStyle),
        pub az_style_border_right_style_deep_copy: extern "C" fn(_:  &AzStyleBorderRightStyle) -> AzStyleBorderRightStyle,
        pub az_style_border_right_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightStyle) -> AzString,
        pub az_style_border_right_width_delete: extern "C" fn(_:  &mut AzStyleBorderRightWidth),
        pub az_style_border_right_width_deep_copy: extern "C" fn(_:  &AzStyleBorderRightWidth) -> AzStyleBorderRightWidth,
        pub az_style_border_right_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightWidth) -> AzString,
        pub az_style_border_top_color_delete: extern "C" fn(_:  &mut AzStyleBorderTopColor),
        pub az_style_border_top_color_deep_copy: extern "C" fn(_:  &AzStyleBorderTopColor) -> AzStyleBorderTopColor,
        pub az_style_border_top_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopColor) -> AzString,
        pub az_style_border_top_left_radius_delete: extern "C" fn(_:  &mut AzStyleBorderTopLeftRadius),
        pub az_style_border_top_left_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderTopLeftRadius) -> AzStyleBorderTopLeftRadius,
        pub az_style_border_top_left_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopLeftRadius) -> AzString,
        pub az_style_border_top_right_radius_delete: extern "C" fn(_:  &mut AzStyleBorderTopRightRadius),
        pub az_style_border_top_right_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderTopRightRadius) -> AzStyleBorderTopRightRadius,
        pub az_style_border_top_right_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopRightRadius) -> AzString,
        pub az_style_border_top_style_delete: extern "C" fn(_:  &mut AzStyleBorderTopStyle),
        pub az_style_border_top_style_deep_copy: extern "C" fn(_:  &AzStyleBorderTopStyle) -> AzStyleBorderTopStyle,
        pub az_style_border_top_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopStyle) -> AzString,
        pub az_style_border_top_width_delete: extern "C" fn(_:  &mut AzStyleBorderTopWidth),
        pub az_style_border_top_width_deep_copy: extern "C" fn(_:  &AzStyleBorderTopWidth) -> AzStyleBorderTopWidth,
        pub az_style_border_top_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopWidth) -> AzString,
        pub az_style_cursor_delete: extern "C" fn(_:  &mut AzStyleCursor),
        pub az_style_cursor_deep_copy: extern "C" fn(_:  &AzStyleCursor) -> AzStyleCursor,
        pub az_style_cursor_fmt_debug: extern "C" fn(_:  &AzStyleCursor) -> AzString,
        pub az_style_font_family_delete: extern "C" fn(_:  &mut AzStyleFontFamily),
        pub az_style_font_family_deep_copy: extern "C" fn(_:  &AzStyleFontFamily) -> AzStyleFontFamily,
        pub az_style_font_family_fmt_debug: extern "C" fn(_:  &AzStyleFontFamily) -> AzString,
        pub az_style_font_size_delete: extern "C" fn(_:  &mut AzStyleFontSize),
        pub az_style_font_size_deep_copy: extern "C" fn(_:  &AzStyleFontSize) -> AzStyleFontSize,
        pub az_style_font_size_fmt_debug: extern "C" fn(_:  &AzStyleFontSize) -> AzString,
        pub az_style_letter_spacing_delete: extern "C" fn(_:  &mut AzStyleLetterSpacing),
        pub az_style_letter_spacing_deep_copy: extern "C" fn(_:  &AzStyleLetterSpacing) -> AzStyleLetterSpacing,
        pub az_style_letter_spacing_fmt_debug: extern "C" fn(_:  &AzStyleLetterSpacing) -> AzString,
        pub az_style_line_height_delete: extern "C" fn(_:  &mut AzStyleLineHeight),
        pub az_style_line_height_deep_copy: extern "C" fn(_:  &AzStyleLineHeight) -> AzStyleLineHeight,
        pub az_style_line_height_fmt_debug: extern "C" fn(_:  &AzStyleLineHeight) -> AzString,
        pub az_style_tab_width_delete: extern "C" fn(_:  &mut AzStyleTabWidth),
        pub az_style_tab_width_deep_copy: extern "C" fn(_:  &AzStyleTabWidth) -> AzStyleTabWidth,
        pub az_style_tab_width_fmt_debug: extern "C" fn(_:  &AzStyleTabWidth) -> AzString,
        pub az_style_text_alignment_horz_delete: extern "C" fn(_:  &mut AzStyleTextAlignmentHorz),
        pub az_style_text_alignment_horz_deep_copy: extern "C" fn(_:  &AzStyleTextAlignmentHorz) -> AzStyleTextAlignmentHorz,
        pub az_style_text_alignment_horz_fmt_debug: extern "C" fn(_:  &AzStyleTextAlignmentHorz) -> AzString,
        pub az_style_text_color_delete: extern "C" fn(_:  &mut AzStyleTextColor),
        pub az_style_text_color_deep_copy: extern "C" fn(_:  &AzStyleTextColor) -> AzStyleTextColor,
        pub az_style_text_color_fmt_debug: extern "C" fn(_:  &AzStyleTextColor) -> AzString,
        pub az_style_word_spacing_delete: extern "C" fn(_:  &mut AzStyleWordSpacing),
        pub az_style_word_spacing_deep_copy: extern "C" fn(_:  &AzStyleWordSpacing) -> AzStyleWordSpacing,
        pub az_style_word_spacing_fmt_debug: extern "C" fn(_:  &AzStyleWordSpacing) -> AzString,
        pub az_box_shadow_pre_display_item_value_delete: extern "C" fn(_:  &mut AzBoxShadowPreDisplayItemValue),
        pub az_box_shadow_pre_display_item_value_deep_copy: extern "C" fn(_:  &AzBoxShadowPreDisplayItemValue) -> AzBoxShadowPreDisplayItemValue,
        pub az_box_shadow_pre_display_item_value_fmt_debug: extern "C" fn(_:  &AzBoxShadowPreDisplayItemValue) -> AzString,
        pub az_layout_align_content_value_delete: extern "C" fn(_:  &mut AzLayoutAlignContentValue),
        pub az_layout_align_content_value_deep_copy: extern "C" fn(_:  &AzLayoutAlignContentValue) -> AzLayoutAlignContentValue,
        pub az_layout_align_content_value_fmt_debug: extern "C" fn(_:  &AzLayoutAlignContentValue) -> AzString,
        pub az_layout_align_items_value_delete: extern "C" fn(_:  &mut AzLayoutAlignItemsValue),
        pub az_layout_align_items_value_deep_copy: extern "C" fn(_:  &AzLayoutAlignItemsValue) -> AzLayoutAlignItemsValue,
        pub az_layout_align_items_value_fmt_debug: extern "C" fn(_:  &AzLayoutAlignItemsValue) -> AzString,
        pub az_layout_bottom_value_delete: extern "C" fn(_:  &mut AzLayoutBottomValue),
        pub az_layout_bottom_value_deep_copy: extern "C" fn(_:  &AzLayoutBottomValue) -> AzLayoutBottomValue,
        pub az_layout_bottom_value_fmt_debug: extern "C" fn(_:  &AzLayoutBottomValue) -> AzString,
        pub az_layout_box_sizing_value_delete: extern "C" fn(_:  &mut AzLayoutBoxSizingValue),
        pub az_layout_box_sizing_value_deep_copy: extern "C" fn(_:  &AzLayoutBoxSizingValue) -> AzLayoutBoxSizingValue,
        pub az_layout_box_sizing_value_fmt_debug: extern "C" fn(_:  &AzLayoutBoxSizingValue) -> AzString,
        pub az_layout_direction_value_delete: extern "C" fn(_:  &mut AzLayoutDirectionValue),
        pub az_layout_direction_value_deep_copy: extern "C" fn(_:  &AzLayoutDirectionValue) -> AzLayoutDirectionValue,
        pub az_layout_direction_value_fmt_debug: extern "C" fn(_:  &AzLayoutDirectionValue) -> AzString,
        pub az_layout_display_value_delete: extern "C" fn(_:  &mut AzLayoutDisplayValue),
        pub az_layout_display_value_deep_copy: extern "C" fn(_:  &AzLayoutDisplayValue) -> AzLayoutDisplayValue,
        pub az_layout_display_value_fmt_debug: extern "C" fn(_:  &AzLayoutDisplayValue) -> AzString,
        pub az_layout_flex_grow_value_delete: extern "C" fn(_:  &mut AzLayoutFlexGrowValue),
        pub az_layout_flex_grow_value_deep_copy: extern "C" fn(_:  &AzLayoutFlexGrowValue) -> AzLayoutFlexGrowValue,
        pub az_layout_flex_grow_value_fmt_debug: extern "C" fn(_:  &AzLayoutFlexGrowValue) -> AzString,
        pub az_layout_flex_shrink_value_delete: extern "C" fn(_:  &mut AzLayoutFlexShrinkValue),
        pub az_layout_flex_shrink_value_deep_copy: extern "C" fn(_:  &AzLayoutFlexShrinkValue) -> AzLayoutFlexShrinkValue,
        pub az_layout_flex_shrink_value_fmt_debug: extern "C" fn(_:  &AzLayoutFlexShrinkValue) -> AzString,
        pub az_layout_float_value_delete: extern "C" fn(_:  &mut AzLayoutFloatValue),
        pub az_layout_float_value_deep_copy: extern "C" fn(_:  &AzLayoutFloatValue) -> AzLayoutFloatValue,
        pub az_layout_float_value_fmt_debug: extern "C" fn(_:  &AzLayoutFloatValue) -> AzString,
        pub az_layout_height_value_delete: extern "C" fn(_:  &mut AzLayoutHeightValue),
        pub az_layout_height_value_deep_copy: extern "C" fn(_:  &AzLayoutHeightValue) -> AzLayoutHeightValue,
        pub az_layout_height_value_fmt_debug: extern "C" fn(_:  &AzLayoutHeightValue) -> AzString,
        pub az_layout_justify_content_value_delete: extern "C" fn(_:  &mut AzLayoutJustifyContentValue),
        pub az_layout_justify_content_value_deep_copy: extern "C" fn(_:  &AzLayoutJustifyContentValue) -> AzLayoutJustifyContentValue,
        pub az_layout_justify_content_value_fmt_debug: extern "C" fn(_:  &AzLayoutJustifyContentValue) -> AzString,
        pub az_layout_left_value_delete: extern "C" fn(_:  &mut AzLayoutLeftValue),
        pub az_layout_left_value_deep_copy: extern "C" fn(_:  &AzLayoutLeftValue) -> AzLayoutLeftValue,
        pub az_layout_left_value_fmt_debug: extern "C" fn(_:  &AzLayoutLeftValue) -> AzString,
        pub az_layout_margin_bottom_value_delete: extern "C" fn(_:  &mut AzLayoutMarginBottomValue),
        pub az_layout_margin_bottom_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginBottomValue) -> AzLayoutMarginBottomValue,
        pub az_layout_margin_bottom_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginBottomValue) -> AzString,
        pub az_layout_margin_left_value_delete: extern "C" fn(_:  &mut AzLayoutMarginLeftValue),
        pub az_layout_margin_left_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginLeftValue) -> AzLayoutMarginLeftValue,
        pub az_layout_margin_left_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginLeftValue) -> AzString,
        pub az_layout_margin_right_value_delete: extern "C" fn(_:  &mut AzLayoutMarginRightValue),
        pub az_layout_margin_right_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginRightValue) -> AzLayoutMarginRightValue,
        pub az_layout_margin_right_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginRightValue) -> AzString,
        pub az_layout_margin_top_value_delete: extern "C" fn(_:  &mut AzLayoutMarginTopValue),
        pub az_layout_margin_top_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginTopValue) -> AzLayoutMarginTopValue,
        pub az_layout_margin_top_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginTopValue) -> AzString,
        pub az_layout_max_height_value_delete: extern "C" fn(_:  &mut AzLayoutMaxHeightValue),
        pub az_layout_max_height_value_deep_copy: extern "C" fn(_:  &AzLayoutMaxHeightValue) -> AzLayoutMaxHeightValue,
        pub az_layout_max_height_value_fmt_debug: extern "C" fn(_:  &AzLayoutMaxHeightValue) -> AzString,
        pub az_layout_max_width_value_delete: extern "C" fn(_:  &mut AzLayoutMaxWidthValue),
        pub az_layout_max_width_value_deep_copy: extern "C" fn(_:  &AzLayoutMaxWidthValue) -> AzLayoutMaxWidthValue,
        pub az_layout_max_width_value_fmt_debug: extern "C" fn(_:  &AzLayoutMaxWidthValue) -> AzString,
        pub az_layout_min_height_value_delete: extern "C" fn(_:  &mut AzLayoutMinHeightValue),
        pub az_layout_min_height_value_deep_copy: extern "C" fn(_:  &AzLayoutMinHeightValue) -> AzLayoutMinHeightValue,
        pub az_layout_min_height_value_fmt_debug: extern "C" fn(_:  &AzLayoutMinHeightValue) -> AzString,
        pub az_layout_min_width_value_delete: extern "C" fn(_:  &mut AzLayoutMinWidthValue),
        pub az_layout_min_width_value_deep_copy: extern "C" fn(_:  &AzLayoutMinWidthValue) -> AzLayoutMinWidthValue,
        pub az_layout_min_width_value_fmt_debug: extern "C" fn(_:  &AzLayoutMinWidthValue) -> AzString,
        pub az_layout_padding_bottom_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingBottomValue),
        pub az_layout_padding_bottom_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingBottomValue) -> AzLayoutPaddingBottomValue,
        pub az_layout_padding_bottom_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingBottomValue) -> AzString,
        pub az_layout_padding_left_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingLeftValue),
        pub az_layout_padding_left_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingLeftValue) -> AzLayoutPaddingLeftValue,
        pub az_layout_padding_left_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingLeftValue) -> AzString,
        pub az_layout_padding_right_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingRightValue),
        pub az_layout_padding_right_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingRightValue) -> AzLayoutPaddingRightValue,
        pub az_layout_padding_right_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingRightValue) -> AzString,
        pub az_layout_padding_top_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingTopValue),
        pub az_layout_padding_top_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingTopValue) -> AzLayoutPaddingTopValue,
        pub az_layout_padding_top_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingTopValue) -> AzString,
        pub az_layout_position_value_delete: extern "C" fn(_:  &mut AzLayoutPositionValue),
        pub az_layout_position_value_deep_copy: extern "C" fn(_:  &AzLayoutPositionValue) -> AzLayoutPositionValue,
        pub az_layout_position_value_fmt_debug: extern "C" fn(_:  &AzLayoutPositionValue) -> AzString,
        pub az_layout_right_value_delete: extern "C" fn(_:  &mut AzLayoutRightValue),
        pub az_layout_right_value_deep_copy: extern "C" fn(_:  &AzLayoutRightValue) -> AzLayoutRightValue,
        pub az_layout_right_value_fmt_debug: extern "C" fn(_:  &AzLayoutRightValue) -> AzString,
        pub az_layout_top_value_delete: extern "C" fn(_:  &mut AzLayoutTopValue),
        pub az_layout_top_value_deep_copy: extern "C" fn(_:  &AzLayoutTopValue) -> AzLayoutTopValue,
        pub az_layout_top_value_fmt_debug: extern "C" fn(_:  &AzLayoutTopValue) -> AzString,
        pub az_layout_width_value_delete: extern "C" fn(_:  &mut AzLayoutWidthValue),
        pub az_layout_width_value_deep_copy: extern "C" fn(_:  &AzLayoutWidthValue) -> AzLayoutWidthValue,
        pub az_layout_width_value_fmt_debug: extern "C" fn(_:  &AzLayoutWidthValue) -> AzString,
        pub az_layout_wrap_value_delete: extern "C" fn(_:  &mut AzLayoutWrapValue),
        pub az_layout_wrap_value_deep_copy: extern "C" fn(_:  &AzLayoutWrapValue) -> AzLayoutWrapValue,
        pub az_layout_wrap_value_fmt_debug: extern "C" fn(_:  &AzLayoutWrapValue) -> AzString,
        pub az_overflow_value_delete: extern "C" fn(_:  &mut AzOverflowValue),
        pub az_overflow_value_deep_copy: extern "C" fn(_:  &AzOverflowValue) -> AzOverflowValue,
        pub az_overflow_value_fmt_debug: extern "C" fn(_:  &AzOverflowValue) -> AzString,
        pub az_style_background_content_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundContentValue),
        pub az_style_background_content_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue,
        pub az_style_background_content_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundContentValue) -> AzString,
        pub az_style_background_position_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundPositionValue),
        pub az_style_background_position_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundPositionValue) -> AzStyleBackgroundPositionValue,
        pub az_style_background_position_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundPositionValue) -> AzString,
        pub az_style_background_repeat_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundRepeatValue),
        pub az_style_background_repeat_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundRepeatValue) -> AzStyleBackgroundRepeatValue,
        pub az_style_background_repeat_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundRepeatValue) -> AzString,
        pub az_style_background_size_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundSizeValue),
        pub az_style_background_size_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundSizeValue) -> AzStyleBackgroundSizeValue,
        pub az_style_background_size_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundSizeValue) -> AzString,
        pub az_style_border_bottom_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomColorValue),
        pub az_style_border_bottom_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomColorValue) -> AzStyleBorderBottomColorValue,
        pub az_style_border_bottom_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomColorValue) -> AzString,
        pub az_style_border_bottom_left_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomLeftRadiusValue),
        pub az_style_border_bottom_left_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomLeftRadiusValue) -> AzStyleBorderBottomLeftRadiusValue,
        pub az_style_border_bottom_left_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomLeftRadiusValue) -> AzString,
        pub az_style_border_bottom_right_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomRightRadiusValue),
        pub az_style_border_bottom_right_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomRightRadiusValue) -> AzStyleBorderBottomRightRadiusValue,
        pub az_style_border_bottom_right_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomRightRadiusValue) -> AzString,
        pub az_style_border_bottom_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomStyleValue),
        pub az_style_border_bottom_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomStyleValue) -> AzStyleBorderBottomStyleValue,
        pub az_style_border_bottom_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomStyleValue) -> AzString,
        pub az_style_border_bottom_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomWidthValue),
        pub az_style_border_bottom_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomWidthValue) -> AzStyleBorderBottomWidthValue,
        pub az_style_border_bottom_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomWidthValue) -> AzString,
        pub az_style_border_left_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderLeftColorValue),
        pub az_style_border_left_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftColorValue) -> AzStyleBorderLeftColorValue,
        pub az_style_border_left_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftColorValue) -> AzString,
        pub az_style_border_left_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderLeftStyleValue),
        pub az_style_border_left_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftStyleValue) -> AzStyleBorderLeftStyleValue,
        pub az_style_border_left_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftStyleValue) -> AzString,
        pub az_style_border_left_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderLeftWidthValue),
        pub az_style_border_left_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftWidthValue) -> AzStyleBorderLeftWidthValue,
        pub az_style_border_left_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftWidthValue) -> AzString,
        pub az_style_border_right_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderRightColorValue),
        pub az_style_border_right_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderRightColorValue) -> AzStyleBorderRightColorValue,
        pub az_style_border_right_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightColorValue) -> AzString,
        pub az_style_border_right_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderRightStyleValue),
        pub az_style_border_right_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderRightStyleValue) -> AzStyleBorderRightStyleValue,
        pub az_style_border_right_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightStyleValue) -> AzString,
        pub az_style_border_right_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderRightWidthValue),
        pub az_style_border_right_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderRightWidthValue) -> AzStyleBorderRightWidthValue,
        pub az_style_border_right_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightWidthValue) -> AzString,
        pub az_style_border_top_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopColorValue),
        pub az_style_border_top_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopColorValue) -> AzStyleBorderTopColorValue,
        pub az_style_border_top_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopColorValue) -> AzString,
        pub az_style_border_top_left_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopLeftRadiusValue),
        pub az_style_border_top_left_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopLeftRadiusValue) -> AzStyleBorderTopLeftRadiusValue,
        pub az_style_border_top_left_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopLeftRadiusValue) -> AzString,
        pub az_style_border_top_right_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopRightRadiusValue),
        pub az_style_border_top_right_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopRightRadiusValue) -> AzStyleBorderTopRightRadiusValue,
        pub az_style_border_top_right_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopRightRadiusValue) -> AzString,
        pub az_style_border_top_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopStyleValue),
        pub az_style_border_top_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopStyleValue) -> AzStyleBorderTopStyleValue,
        pub az_style_border_top_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopStyleValue) -> AzString,
        pub az_style_border_top_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopWidthValue),
        pub az_style_border_top_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopWidthValue) -> AzStyleBorderTopWidthValue,
        pub az_style_border_top_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopWidthValue) -> AzString,
        pub az_style_cursor_value_delete: extern "C" fn(_:  &mut AzStyleCursorValue),
        pub az_style_cursor_value_deep_copy: extern "C" fn(_:  &AzStyleCursorValue) -> AzStyleCursorValue,
        pub az_style_cursor_value_fmt_debug: extern "C" fn(_:  &AzStyleCursorValue) -> AzString,
        pub az_style_font_family_value_delete: extern "C" fn(_:  &mut AzStyleFontFamilyValue),
        pub az_style_font_family_value_deep_copy: extern "C" fn(_:  &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue,
        pub az_style_font_family_value_fmt_debug: extern "C" fn(_:  &AzStyleFontFamilyValue) -> AzString,
        pub az_style_font_size_value_delete: extern "C" fn(_:  &mut AzStyleFontSizeValue),
        pub az_style_font_size_value_deep_copy: extern "C" fn(_:  &AzStyleFontSizeValue) -> AzStyleFontSizeValue,
        pub az_style_font_size_value_fmt_debug: extern "C" fn(_:  &AzStyleFontSizeValue) -> AzString,
        pub az_style_letter_spacing_value_delete: extern "C" fn(_:  &mut AzStyleLetterSpacingValue),
        pub az_style_letter_spacing_value_deep_copy: extern "C" fn(_:  &AzStyleLetterSpacingValue) -> AzStyleLetterSpacingValue,
        pub az_style_letter_spacing_value_fmt_debug: extern "C" fn(_:  &AzStyleLetterSpacingValue) -> AzString,
        pub az_style_line_height_value_delete: extern "C" fn(_:  &mut AzStyleLineHeightValue),
        pub az_style_line_height_value_deep_copy: extern "C" fn(_:  &AzStyleLineHeightValue) -> AzStyleLineHeightValue,
        pub az_style_line_height_value_fmt_debug: extern "C" fn(_:  &AzStyleLineHeightValue) -> AzString,
        pub az_style_tab_width_value_delete: extern "C" fn(_:  &mut AzStyleTabWidthValue),
        pub az_style_tab_width_value_deep_copy: extern "C" fn(_:  &AzStyleTabWidthValue) -> AzStyleTabWidthValue,
        pub az_style_tab_width_value_fmt_debug: extern "C" fn(_:  &AzStyleTabWidthValue) -> AzString,
        pub az_style_text_alignment_horz_value_delete: extern "C" fn(_:  &mut AzStyleTextAlignmentHorzValue),
        pub az_style_text_alignment_horz_value_deep_copy: extern "C" fn(_:  &AzStyleTextAlignmentHorzValue) -> AzStyleTextAlignmentHorzValue,
        pub az_style_text_alignment_horz_value_fmt_debug: extern "C" fn(_:  &AzStyleTextAlignmentHorzValue) -> AzString,
        pub az_style_text_color_value_delete: extern "C" fn(_:  &mut AzStyleTextColorValue),
        pub az_style_text_color_value_deep_copy: extern "C" fn(_:  &AzStyleTextColorValue) -> AzStyleTextColorValue,
        pub az_style_text_color_value_fmt_debug: extern "C" fn(_:  &AzStyleTextColorValue) -> AzString,
        pub az_style_word_spacing_value_delete: extern "C" fn(_:  &mut AzStyleWordSpacingValue),
        pub az_style_word_spacing_value_deep_copy: extern "C" fn(_:  &AzStyleWordSpacingValue) -> AzStyleWordSpacingValue,
        pub az_style_word_spacing_value_fmt_debug: extern "C" fn(_:  &AzStyleWordSpacingValue) -> AzString,
        pub az_css_property_delete: extern "C" fn(_:  &mut AzCssProperty),
        pub az_css_property_deep_copy: extern "C" fn(_:  &AzCssProperty) -> AzCssProperty,
        pub az_css_property_fmt_debug: extern "C" fn(_:  &AzCssProperty) -> AzString,
        pub az_dom_new: extern "C" fn(_:  AzNodeType) -> AzDom,
        pub az_dom_div: extern "C" fn() -> AzDom,
        pub az_dom_body: extern "C" fn() -> AzDom,
        pub az_dom_label: extern "C" fn(_:  AzString) -> AzDom,
        pub az_dom_text: extern "C" fn(_:  AzTextId) -> AzDom,
        pub az_dom_image: extern "C" fn(_:  AzImageId) -> AzDom,
        pub az_dom_gl_texture: extern "C" fn(_:  AzRefAny, _:  AzGlCallbackType) -> AzDom,
        pub az_dom_iframe: extern "C" fn(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzDom,
        pub az_dom_add_id: extern "C" fn(_:  &mut AzDom, _:  AzString),
        pub az_dom_with_id: extern "C" fn(_:  AzDom, _:  AzString) -> AzDom,
        pub az_dom_set_ids: extern "C" fn(_:  &mut AzDom, _:  AzStringVec),
        pub az_dom_with_ids: extern "C" fn(_:  AzDom, _:  AzStringVec) -> AzDom,
        pub az_dom_add_class: extern "C" fn(_:  &mut AzDom, _:  AzString),
        pub az_dom_with_class: extern "C" fn(_:  AzDom, _:  AzString) -> AzDom,
        pub az_dom_set_classes: extern "C" fn(_:  &mut AzDom, _:  AzStringVec),
        pub az_dom_with_classes: extern "C" fn(_:  AzDom, _:  AzStringVec) -> AzDom,
        pub az_dom_add_callback: extern "C" fn(_:  &mut AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType),
        pub az_dom_with_callback: extern "C" fn(_:  AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzDom,
        pub az_dom_add_css_override: extern "C" fn(_:  &mut AzDom, _:  AzString, _:  AzCssProperty),
        pub az_dom_with_css_override: extern "C" fn(_:  AzDom, _:  AzString, _:  AzCssProperty) -> AzDom,
        pub az_dom_set_is_draggable: extern "C" fn(_:  &mut AzDom, _:  bool),
        pub az_dom_is_draggable: extern "C" fn(_:  AzDom, _:  bool) -> AzDom,
        pub az_dom_set_tab_index: extern "C" fn(_:  &mut AzDom, _:  AzOptionTabIndex),
        pub az_dom_with_tab_index: extern "C" fn(_:  AzDom, _:  AzOptionTabIndex) -> AzDom,
        pub az_dom_has_id: extern "C" fn(_:  &mut AzDom, _:  AzString) -> bool,
        pub az_dom_has_class: extern "C" fn(_:  &mut AzDom, _:  AzString) -> bool,
        pub az_dom_add_child: extern "C" fn(_:  &mut AzDom, _:  AzDom),
        pub az_dom_with_child: extern "C" fn(_:  AzDom, _:  AzDom) -> AzDom,
        pub az_dom_get_html_string: extern "C" fn(_:  &AzDom) -> AzString,
        pub az_dom_delete: extern "C" fn(_:  &mut AzDom),
        pub az_dom_deep_copy: extern "C" fn(_:  &AzDom) -> AzDom,
        pub az_dom_fmt_debug: extern "C" fn(_:  &AzDom) -> AzString,
        pub az_gl_texture_node_delete: extern "C" fn(_:  &mut AzGlTextureNode),
        pub az_gl_texture_node_deep_copy: extern "C" fn(_:  &AzGlTextureNode) -> AzGlTextureNode,
        pub az_gl_texture_node_fmt_debug: extern "C" fn(_:  &AzGlTextureNode) -> AzString,
        pub az_i_frame_node_delete: extern "C" fn(_:  &mut AzIFrameNode),
        pub az_i_frame_node_deep_copy: extern "C" fn(_:  &AzIFrameNode) -> AzIFrameNode,
        pub az_i_frame_node_fmt_debug: extern "C" fn(_:  &AzIFrameNode) -> AzString,
        pub az_callback_data_delete: extern "C" fn(_:  &mut AzCallbackData),
        pub az_callback_data_deep_copy: extern "C" fn(_:  &AzCallbackData) -> AzCallbackData,
        pub az_callback_data_fmt_debug: extern "C" fn(_:  &AzCallbackData) -> AzString,
        pub az_override_property_delete: extern "C" fn(_:  &mut AzOverrideProperty),
        pub az_override_property_deep_copy: extern "C" fn(_:  &AzOverrideProperty) -> AzOverrideProperty,
        pub az_override_property_fmt_debug: extern "C" fn(_:  &AzOverrideProperty) -> AzString,
        pub az_node_data_new: extern "C" fn(_:  AzNodeType) -> AzNodeData,
        pub az_node_data_div: extern "C" fn() -> AzNodeData,
        pub az_node_data_body: extern "C" fn() -> AzNodeData,
        pub az_node_data_label: extern "C" fn(_:  AzString) -> AzNodeData,
        pub az_node_data_text: extern "C" fn(_:  AzTextId) -> AzNodeData,
        pub az_node_data_image: extern "C" fn(_:  AzImageId) -> AzNodeData,
        pub az_node_data_gl_texture: extern "C" fn(_:  AzRefAny, _:  AzGlCallbackType) -> AzNodeData,
        pub az_node_data_iframe: extern "C" fn(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzNodeData,
        pub az_node_data_default: extern "C" fn() -> AzNodeData,
        pub az_node_data_add_id: extern "C" fn(_:  &mut AzNodeData, _:  AzString),
        pub az_node_data_with_id: extern "C" fn(_:  AzNodeData, _:  AzString) -> AzNodeData,
        pub az_node_data_set_ids: extern "C" fn(_:  &mut AzNodeData, _:  AzStringVec),
        pub az_node_data_with_ids: extern "C" fn(_:  AzNodeData, _:  AzStringVec) -> AzNodeData,
        pub az_node_data_add_class: extern "C" fn(_:  &mut AzNodeData, _:  AzString),
        pub az_node_data_with_class: extern "C" fn(_:  AzNodeData, _:  AzString) -> AzNodeData,
        pub az_node_data_set_classes: extern "C" fn(_:  &mut AzNodeData, _:  AzStringVec),
        pub az_node_data_with_classes: extern "C" fn(_:  AzNodeData, _:  AzStringVec) -> AzNodeData,
        pub az_node_data_add_callback: extern "C" fn(_:  &mut AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType),
        pub az_node_data_with_callback: extern "C" fn(_:  AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzNodeData,
        pub az_node_data_add_css_override: extern "C" fn(_:  &mut AzNodeData, _:  AzString, _:  AzCssProperty),
        pub az_node_data_with_css_override: extern "C" fn(_:  AzNodeData, _:  AzString, _:  AzCssProperty) -> AzNodeData,
        pub az_node_data_set_is_draggable: extern "C" fn(_:  &mut AzNodeData, _:  bool),
        pub az_node_data_is_draggable: extern "C" fn(_:  AzNodeData, _:  bool) -> AzNodeData,
        pub az_node_data_set_tab_index: extern "C" fn(_:  &mut AzNodeData, _:  AzOptionTabIndex),
        pub az_node_data_with_tab_index: extern "C" fn(_:  AzNodeData, _:  AzOptionTabIndex) -> AzNodeData,
        pub az_node_data_has_id: extern "C" fn(_:  &mut AzNodeData, _:  AzString) -> bool,
        pub az_node_data_has_class: extern "C" fn(_:  &mut AzNodeData, _:  AzString) -> bool,
        pub az_node_data_delete: extern "C" fn(_:  &mut AzNodeData),
        pub az_node_data_deep_copy: extern "C" fn(_:  &AzNodeData) -> AzNodeData,
        pub az_node_data_fmt_debug: extern "C" fn(_:  &AzNodeData) -> AzString,
        pub az_node_type_delete: extern "C" fn(_:  &mut AzNodeType),
        pub az_node_type_deep_copy: extern "C" fn(_:  &AzNodeType) -> AzNodeType,
        pub az_node_type_fmt_debug: extern "C" fn(_:  &AzNodeType) -> AzString,
        pub az_on_into_event_filter: extern "C" fn(_:  AzOn) -> AzEventFilter,
        pub az_on_delete: extern "C" fn(_:  &mut AzOn),
        pub az_on_deep_copy: extern "C" fn(_:  &AzOn) -> AzOn,
        pub az_on_fmt_debug: extern "C" fn(_:  &AzOn) -> AzString,
        pub az_event_filter_delete: extern "C" fn(_:  &mut AzEventFilter),
        pub az_event_filter_deep_copy: extern "C" fn(_:  &AzEventFilter) -> AzEventFilter,
        pub az_event_filter_fmt_debug: extern "C" fn(_:  &AzEventFilter) -> AzString,
        pub az_hover_event_filter_delete: extern "C" fn(_:  &mut AzHoverEventFilter),
        pub az_hover_event_filter_deep_copy: extern "C" fn(_:  &AzHoverEventFilter) -> AzHoverEventFilter,
        pub az_hover_event_filter_fmt_debug: extern "C" fn(_:  &AzHoverEventFilter) -> AzString,
        pub az_focus_event_filter_delete: extern "C" fn(_:  &mut AzFocusEventFilter),
        pub az_focus_event_filter_deep_copy: extern "C" fn(_:  &AzFocusEventFilter) -> AzFocusEventFilter,
        pub az_focus_event_filter_fmt_debug: extern "C" fn(_:  &AzFocusEventFilter) -> AzString,
        pub az_not_event_filter_delete: extern "C" fn(_:  &mut AzNotEventFilter),
        pub az_not_event_filter_deep_copy: extern "C" fn(_:  &AzNotEventFilter) -> AzNotEventFilter,
        pub az_not_event_filter_fmt_debug: extern "C" fn(_:  &AzNotEventFilter) -> AzString,
        pub az_window_event_filter_delete: extern "C" fn(_:  &mut AzWindowEventFilter),
        pub az_window_event_filter_deep_copy: extern "C" fn(_:  &AzWindowEventFilter) -> AzWindowEventFilter,
        pub az_window_event_filter_fmt_debug: extern "C" fn(_:  &AzWindowEventFilter) -> AzString,
        pub az_tab_index_delete: extern "C" fn(_:  &mut AzTabIndex),
        pub az_tab_index_deep_copy: extern "C" fn(_:  &AzTabIndex) -> AzTabIndex,
        pub az_tab_index_fmt_debug: extern "C" fn(_:  &AzTabIndex) -> AzString,
        pub az_gl_type_delete: extern "C" fn(_:  &mut AzGlType),
        pub az_gl_type_deep_copy: extern "C" fn(_:  &AzGlType) -> AzGlType,
        pub az_gl_type_fmt_debug: extern "C" fn(_:  &AzGlType) -> AzString,
        pub az_debug_message_delete: extern "C" fn(_:  &mut AzDebugMessage),
        pub az_debug_message_deep_copy: extern "C" fn(_:  &AzDebugMessage) -> AzDebugMessage,
        pub az_debug_message_fmt_debug: extern "C" fn(_:  &AzDebugMessage) -> AzString,
        pub az_u8_vec_ref_delete: extern "C" fn(_:  &mut AzU8VecRef),
        pub az_u8_vec_ref_fmt_debug: extern "C" fn(_:  &AzU8VecRef) -> AzString,
        pub az_u8_vec_ref_mut_delete: extern "C" fn(_:  &mut AzU8VecRefMut),
        pub az_u8_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzU8VecRefMut) -> AzString,
        pub az_f32_vec_ref_delete: extern "C" fn(_:  &mut AzF32VecRef),
        pub az_f32_vec_ref_fmt_debug: extern "C" fn(_:  &AzF32VecRef) -> AzString,
        pub az_i32_vec_ref_delete: extern "C" fn(_:  &mut AzI32VecRef),
        pub az_i32_vec_ref_fmt_debug: extern "C" fn(_:  &AzI32VecRef) -> AzString,
        pub az_g_luint_vec_ref_delete: extern "C" fn(_:  &mut AzGLuintVecRef),
        pub az_g_luint_vec_ref_fmt_debug: extern "C" fn(_:  &AzGLuintVecRef) -> AzString,
        pub az_g_lenum_vec_ref_delete: extern "C" fn(_:  &mut AzGLenumVecRef),
        pub az_g_lenum_vec_ref_fmt_debug: extern "C" fn(_:  &AzGLenumVecRef) -> AzString,
        pub az_g_lint_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLintVecRefMut),
        pub az_g_lint_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLintVecRefMut) -> AzString,
        pub az_g_lint64_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLint64VecRefMut),
        pub az_g_lint64_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLint64VecRefMut) -> AzString,
        pub az_g_lboolean_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLbooleanVecRefMut),
        pub az_g_lboolean_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLbooleanVecRefMut) -> AzString,
        pub az_g_lfloat_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLfloatVecRefMut),
        pub az_g_lfloat_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLfloatVecRefMut) -> AzString,
        pub az_refstr_vec_ref_delete: extern "C" fn(_:  &mut AzRefstrVecRef),
        pub az_refstr_vec_ref_fmt_debug: extern "C" fn(_:  &AzRefstrVecRef) -> AzString,
        pub az_refstr_delete: extern "C" fn(_:  &mut AzRefstr),
        pub az_refstr_fmt_debug: extern "C" fn(_:  &AzRefstr) -> AzString,
        pub az_get_program_binary_return_delete: extern "C" fn(_:  &mut AzGetProgramBinaryReturn),
        pub az_get_program_binary_return_deep_copy: extern "C" fn(_:  &AzGetProgramBinaryReturn) -> AzGetProgramBinaryReturn,
        pub az_get_program_binary_return_fmt_debug: extern "C" fn(_:  &AzGetProgramBinaryReturn) -> AzString,
        pub az_get_active_attrib_return_delete: extern "C" fn(_:  &mut AzGetActiveAttribReturn),
        pub az_get_active_attrib_return_deep_copy: extern "C" fn(_:  &AzGetActiveAttribReturn) -> AzGetActiveAttribReturn,
        pub az_get_active_attrib_return_fmt_debug: extern "C" fn(_:  &AzGetActiveAttribReturn) -> AzString,
        pub az_g_lsync_ptr_delete: extern "C" fn(_:  &mut AzGLsyncPtr),
        pub az_g_lsync_ptr_fmt_debug: extern "C" fn(_:  &AzGLsyncPtr) -> AzString,
        pub az_get_active_uniform_return_delete: extern "C" fn(_:  &mut AzGetActiveUniformReturn),
        pub az_get_active_uniform_return_deep_copy: extern "C" fn(_:  &AzGetActiveUniformReturn) -> AzGetActiveUniformReturn,
        pub az_get_active_uniform_return_fmt_debug: extern "C" fn(_:  &AzGetActiveUniformReturn) -> AzString,
        pub az_gl_context_ptr_get_type: extern "C" fn(_:  &AzGlContextPtr) -> AzGlType,
        pub az_gl_context_ptr_buffer_data_untyped: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  *const c_void, _:  u32),
        pub az_gl_context_ptr_buffer_sub_data_untyped: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  *const c_void),
        pub az_gl_context_ptr_map_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> *mut c_void,
        pub az_gl_context_ptr_map_buffer_range: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  u32) -> *mut c_void,
        pub az_gl_context_ptr_unmap_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8,
        pub az_gl_context_ptr_tex_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_shader_source: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzStringVec),
        pub az_gl_context_ptr_read_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_read_pixels_into_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut),
        pub az_gl_context_ptr_read_pixels: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32) -> AzU8Vec,
        pub az_gl_context_ptr_read_pixels_into_pbo: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32),
        pub az_gl_context_ptr_sample_coverage: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  bool),
        pub az_gl_context_ptr_polygon_offset: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  f32),
        pub az_gl_context_ptr_pixel_store_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32),
        pub az_gl_context_ptr_gen_buffers: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_gen_renderbuffers: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_gen_framebuffers: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_gen_textures: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_gen_vertex_arrays: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_gen_queries: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_begin_query: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_end_query: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_query_counter: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_get_query_object_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32,
        pub az_gl_context_ptr_get_query_object_uiv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u32,
        pub az_gl_context_ptr_get_query_object_i64v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i64,
        pub az_gl_context_ptr_get_query_object_ui64v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u64,
        pub az_gl_context_ptr_delete_queries: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_delete_vertex_arrays: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_delete_buffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_delete_renderbuffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_delete_framebuffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_delete_textures: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_framebuffer_renderbuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_renderbuffer_storage: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32),
        pub az_gl_context_ptr_depth_func: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_active_texture: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_attach_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_bind_attrib_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr),
        pub az_gl_context_ptr_get_uniform_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLintVecRefMut),
        pub az_gl_context_ptr_get_uniform_fv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLfloatVecRefMut),
        pub az_gl_context_ptr_get_uniform_block_index: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> u32,
        pub az_gl_context_ptr_get_uniform_indices: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstrVecRef) -> AzGLuintVec,
        pub az_gl_context_ptr_bind_buffer_base: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_bind_buffer_range: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  isize, _:  isize),
        pub az_gl_context_ptr_uniform_block_binding: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_bind_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_bind_vertex_array: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_bind_renderbuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_bind_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_bind_texture: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_draw_buffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLenumVecRef),
        pub az_gl_context_ptr_tex_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef),
        pub az_gl_context_ptr_compressed_tex_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  AzU8VecRef),
        pub az_gl_context_ptr_compressed_tex_sub_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  AzU8VecRef),
        pub az_gl_context_ptr_tex_image_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef),
        pub az_gl_context_ptr_copy_tex_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_copy_tex_sub_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_copy_tex_sub_image_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_tex_sub_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef),
        pub az_gl_context_ptr_tex_sub_image_2d_pbo: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize),
        pub az_gl_context_ptr_tex_sub_image_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef),
        pub az_gl_context_ptr_tex_sub_image_3d_pbo: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize),
        pub az_gl_context_ptr_tex_storage_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32),
        pub az_gl_context_ptr_tex_storage_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_get_tex_image_into_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut),
        pub az_gl_context_ptr_copy_image_sub_data: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_invalidate_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef),
        pub az_gl_context_ptr_invalidate_sub_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_get_integer_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLintVecRefMut),
        pub az_gl_context_ptr_get_integer_64v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLint64VecRefMut),
        pub az_gl_context_ptr_get_integer_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut),
        pub az_gl_context_ptr_get_integer_64iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLint64VecRefMut),
        pub az_gl_context_ptr_get_boolean_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLbooleanVecRefMut),
        pub az_gl_context_ptr_get_float_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLfloatVecRefMut),
        pub az_gl_context_ptr_get_framebuffer_attachment_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32,
        pub az_gl_context_ptr_get_renderbuffer_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32,
        pub az_gl_context_ptr_get_tex_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32,
        pub az_gl_context_ptr_get_tex_parameter_fv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> f32,
        pub az_gl_context_ptr_tex_parameter_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32),
        pub az_gl_context_ptr_tex_parameter_f: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  f32),
        pub az_gl_context_ptr_framebuffer_texture_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  i32),
        pub az_gl_context_ptr_framebuffer_texture_layer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  i32, _:  i32),
        pub az_gl_context_ptr_blit_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32),
        pub az_gl_context_ptr_vertex_attrib_4f: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  f32, _:  f32, _:  f32, _:  f32),
        pub az_gl_context_ptr_vertex_attrib_pointer_f32: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  bool, _:  i32, _:  u32),
        pub az_gl_context_ptr_vertex_attrib_pointer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  bool, _:  i32, _:  u32),
        pub az_gl_context_ptr_vertex_attrib_i_pointer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  u32),
        pub az_gl_context_ptr_vertex_attrib_divisor: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_viewport: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_scissor: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_line_width: extern "C" fn(_:  &AzGlContextPtr, _:  f32),
        pub az_gl_context_ptr_use_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_validate_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_draw_arrays: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32),
        pub az_gl_context_ptr_draw_arrays_instanced: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_draw_elements: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32),
        pub az_gl_context_ptr_draw_elements_instanced: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32),
        pub az_gl_context_ptr_blend_color: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32),
        pub az_gl_context_ptr_blend_func: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_blend_func_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_blend_equation: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_blend_equation_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_color_mask: extern "C" fn(_:  &AzGlContextPtr, _:  bool, _:  bool, _:  bool, _:  bool),
        pub az_gl_context_ptr_cull_face: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_front_face: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_enable: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_disable: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_hint: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_is_enabled: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8,
        pub az_gl_context_ptr_is_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8,
        pub az_gl_context_ptr_is_texture: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8,
        pub az_gl_context_ptr_is_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8,
        pub az_gl_context_ptr_is_renderbuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8,
        pub az_gl_context_ptr_check_frame_buffer_status: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u32,
        pub az_gl_context_ptr_enable_vertex_attrib_array: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_disable_vertex_attrib_array: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_uniform_1f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32),
        pub az_gl_context_ptr_uniform_1fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef),
        pub az_gl_context_ptr_uniform_1i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32),
        pub az_gl_context_ptr_uniform_1iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef),
        pub az_gl_context_ptr_uniform_1ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32),
        pub az_gl_context_ptr_uniform_2f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32),
        pub az_gl_context_ptr_uniform_2fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef),
        pub az_gl_context_ptr_uniform_2i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_uniform_2iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef),
        pub az_gl_context_ptr_uniform_2ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32),
        pub az_gl_context_ptr_uniform_3f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32),
        pub az_gl_context_ptr_uniform_3fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef),
        pub az_gl_context_ptr_uniform_3i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_uniform_3iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef),
        pub az_gl_context_ptr_uniform_3ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_uniform_4f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32, _:  f32),
        pub az_gl_context_ptr_uniform_4i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32),
        pub az_gl_context_ptr_uniform_4iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef),
        pub az_gl_context_ptr_uniform_4ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_uniform_4fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef),
        pub az_gl_context_ptr_uniform_matrix_2fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef),
        pub az_gl_context_ptr_uniform_matrix_3fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef),
        pub az_gl_context_ptr_uniform_matrix_4fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef),
        pub az_gl_context_ptr_depth_mask: extern "C" fn(_:  &AzGlContextPtr, _:  bool),
        pub az_gl_context_ptr_depth_range: extern "C" fn(_:  &AzGlContextPtr, _:  f64, _:  f64),
        pub az_gl_context_ptr_get_active_attrib: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveAttribReturn,
        pub az_gl_context_ptr_get_active_uniform: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveUniformReturn,
        pub az_gl_context_ptr_get_active_uniforms_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLuintVec, _:  u32) -> AzGLintVec,
        pub az_gl_context_ptr_get_active_uniform_block_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32,
        pub az_gl_context_ptr_get_active_uniform_block_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> AzGLintVec,
        pub az_gl_context_ptr_get_active_uniform_block_name: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString,
        pub az_gl_context_ptr_get_attrib_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32,
        pub az_gl_context_ptr_get_frag_data_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32,
        pub az_gl_context_ptr_get_uniform_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32,
        pub az_gl_context_ptr_get_program_info_log: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzString,
        pub az_gl_context_ptr_get_program_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut),
        pub az_gl_context_ptr_get_program_binary: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzGetProgramBinaryReturn,
        pub az_gl_context_ptr_program_binary: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzU8VecRef),
        pub az_gl_context_ptr_program_parameter_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32),
        pub az_gl_context_ptr_get_vertex_attrib_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut),
        pub az_gl_context_ptr_get_vertex_attrib_fv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLfloatVecRefMut),
        pub az_gl_context_ptr_get_vertex_attrib_pointer_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> isize,
        pub az_gl_context_ptr_get_buffer_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32,
        pub az_gl_context_ptr_get_shader_info_log: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzString,
        pub az_gl_context_ptr_get_string: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzString,
        pub az_gl_context_ptr_get_string_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString,
        pub az_gl_context_ptr_get_shader_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut),
        pub az_gl_context_ptr_get_shader_precision_format: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> [i32;3],
        pub az_gl_context_ptr_compile_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_create_program: extern "C" fn(_:  &AzGlContextPtr) -> u32,
        pub az_gl_context_ptr_delete_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_create_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u32,
        pub az_gl_context_ptr_delete_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_detach_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_link_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_clear_color: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32),
        pub az_gl_context_ptr_clear: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_clear_depth: extern "C" fn(_:  &AzGlContextPtr, _:  f64),
        pub az_gl_context_ptr_clear_stencil: extern "C" fn(_:  &AzGlContextPtr, _:  i32),
        pub az_gl_context_ptr_flush: extern "C" fn(_:  &AzGlContextPtr),
        pub az_gl_context_ptr_finish: extern "C" fn(_:  &AzGlContextPtr),
        pub az_gl_context_ptr_get_error: extern "C" fn(_:  &AzGlContextPtr) -> u32,
        pub az_gl_context_ptr_stencil_mask: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_stencil_mask_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_stencil_func: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32),
        pub az_gl_context_ptr_stencil_func_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  u32),
        pub az_gl_context_ptr_stencil_op: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_stencil_op_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32),
        pub az_gl_context_ptr_egl_image_target_texture2d_oes: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void),
        pub az_gl_context_ptr_generate_mipmap: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_insert_event_marker_ext: extern "C" fn(_:  &AzGlContextPtr, _:  AzRefstr),
        pub az_gl_context_ptr_push_group_marker_ext: extern "C" fn(_:  &AzGlContextPtr, _:  AzRefstr),
        pub az_gl_context_ptr_pop_group_marker_ext: extern "C" fn(_:  &AzGlContextPtr),
        pub az_gl_context_ptr_debug_message_insert_khr: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  AzRefstr),
        pub az_gl_context_ptr_push_debug_group_khr: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr),
        pub az_gl_context_ptr_pop_debug_group_khr: extern "C" fn(_:  &AzGlContextPtr),
        pub az_gl_context_ptr_fence_sync: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGLsyncPtr,
        pub az_gl_context_ptr_client_wait_sync: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64),
        pub az_gl_context_ptr_wait_sync: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64),
        pub az_gl_context_ptr_delete_sync: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr),
        pub az_gl_context_ptr_texture_range_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzU8VecRef),
        pub az_gl_context_ptr_gen_fences_apple: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_delete_fences_apple: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_set_fence_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_finish_fence_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_test_fence_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_test_object_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u8,
        pub az_gl_context_ptr_finish_object_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32),
        pub az_gl_context_ptr_get_frag_data_index: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32,
        pub az_gl_context_ptr_blend_barrier_khr: extern "C" fn(_:  &AzGlContextPtr),
        pub az_gl_context_ptr_bind_frag_data_location_indexed: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  AzRefstr),
        pub az_gl_context_ptr_get_debug_messages: extern "C" fn(_:  &AzGlContextPtr) -> AzDebugMessageVec,
        pub az_gl_context_ptr_provoking_vertex_angle: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_gen_vertex_arrays_apple: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec,
        pub az_gl_context_ptr_bind_vertex_array_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32),
        pub az_gl_context_ptr_delete_vertex_arrays_apple: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef),
        pub az_gl_context_ptr_copy_texture_chromium: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8),
        pub az_gl_context_ptr_copy_sub_texture_chromium: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8),
        pub az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void),
        pub az_gl_context_ptr_copy_texture_3d_angle: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8),
        pub az_gl_context_ptr_copy_sub_texture_3d_angle: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8),
        pub az_gl_context_ptr_delete: extern "C" fn(_:  &mut AzGlContextPtr),
        pub az_gl_context_ptr_deep_copy: extern "C" fn(_:  &AzGlContextPtr) -> AzGlContextPtr,
        pub az_gl_context_ptr_fmt_debug: extern "C" fn(_:  &AzGlContextPtr) -> AzString,
        pub az_texture_delete: extern "C" fn(_:  &mut AzTexture),
        pub az_texture_fmt_debug: extern "C" fn(_:  &AzTexture) -> AzString,
        pub az_texture_flags_delete: extern "C" fn(_:  &mut AzTextureFlags),
        pub az_texture_flags_deep_copy: extern "C" fn(_:  &AzTextureFlags) -> AzTextureFlags,
        pub az_texture_flags_fmt_debug: extern "C" fn(_:  &AzTextureFlags) -> AzString,
        pub az_text_id_new: extern "C" fn() -> AzTextId,
        pub az_text_id_delete: extern "C" fn(_:  &mut AzTextId),
        pub az_text_id_deep_copy: extern "C" fn(_:  &AzTextId) -> AzTextId,
        pub az_text_id_fmt_debug: extern "C" fn(_:  &AzTextId) -> AzString,
        pub az_image_id_new: extern "C" fn() -> AzImageId,
        pub az_image_id_delete: extern "C" fn(_:  &mut AzImageId),
        pub az_image_id_deep_copy: extern "C" fn(_:  &AzImageId) -> AzImageId,
        pub az_image_id_fmt_debug: extern "C" fn(_:  &AzImageId) -> AzString,
        pub az_image_id_partial_eq: extern "C" fn(_:  &AzImageId, _:  &AzImageId) -> bool,
        pub az_image_id_partial_cmp: extern "C" fn(_:  &AzImageId, _:  &AzImageId) -> u8,
        pub az_image_id_cmp: extern "C" fn(_:  &AzImageId, _:  &AzImageId) -> u8,
        pub az_image_id_hash: extern "C" fn(_:  &AzImageId) -> u64,
        pub az_font_id_new: extern "C" fn() -> AzFontId,
        pub az_font_id_delete: extern "C" fn(_:  &mut AzFontId),
        pub az_font_id_deep_copy: extern "C" fn(_:  &AzFontId) -> AzFontId,
        pub az_font_id_fmt_debug: extern "C" fn(_:  &AzFontId) -> AzString,
        pub az_image_source_delete: extern "C" fn(_:  &mut AzImageSource),
        pub az_image_source_deep_copy: extern "C" fn(_:  &AzImageSource) -> AzImageSource,
        pub az_image_source_fmt_debug: extern "C" fn(_:  &AzImageSource) -> AzString,
        pub az_font_source_delete: extern "C" fn(_:  &mut AzFontSource),
        pub az_font_source_deep_copy: extern "C" fn(_:  &AzFontSource) -> AzFontSource,
        pub az_font_source_fmt_debug: extern "C" fn(_:  &AzFontSource) -> AzString,
        pub az_raw_image_new: extern "C" fn(_:  AzU8Vec, _:  usize, _:  usize, _:  AzRawImageFormat) -> AzRawImage,
        pub az_raw_image_delete: extern "C" fn(_:  &mut AzRawImage),
        pub az_raw_image_deep_copy: extern "C" fn(_:  &AzRawImage) -> AzRawImage,
        pub az_raw_image_fmt_debug: extern "C" fn(_:  &AzRawImage) -> AzString,
        pub az_raw_image_format_delete: extern "C" fn(_:  &mut AzRawImageFormat),
        pub az_raw_image_format_deep_copy: extern "C" fn(_:  &AzRawImageFormat) -> AzRawImageFormat,
        pub az_raw_image_format_fmt_debug: extern "C" fn(_:  &AzRawImageFormat) -> AzString,
        pub az_drop_check_ptr_ptr_delete: extern "C" fn(_:  &mut AzDropCheckPtrPtr),
        pub az_drop_check_ptr_ptr_fmt_debug: extern "C" fn(_:  &AzDropCheckPtrPtr) -> AzString,
        pub az_arc_mutex_ref_any_ptr_delete: extern "C" fn(_:  &mut AzArcMutexRefAnyPtr),
        pub az_arc_mutex_ref_any_ptr_fmt_debug: extern "C" fn(_:  &AzArcMutexRefAnyPtr) -> AzString,
        pub az_timer_delete: extern "C" fn(_:  &mut AzTimer),
        pub az_timer_deep_copy: extern "C" fn(_:  &AzTimer) -> AzTimer,
        pub az_timer_fmt_debug: extern "C" fn(_:  &AzTimer) -> AzString,
        pub az_task_ptr_new: extern "C" fn(_:  AzArcMutexRefAnyPtr, _:  AzTaskCallbackType) -> AzTaskPtr,
        pub az_task_ptr_then: extern "C" fn(_:  AzTaskPtr, _:  AzTimer) -> AzTaskPtr,
        pub az_task_ptr_delete: extern "C" fn(_:  &mut AzTaskPtr),
        pub az_task_ptr_fmt_debug: extern "C" fn(_:  &AzTaskPtr) -> AzString,
        pub az_thread_ptr_new: extern "C" fn(_:  AzRefAny, _:  AzThreadCallbackType) -> AzThreadPtr,
        pub az_thread_ptr_block: extern "C" fn(_:  AzThreadPtr) -> AzResultRefAnyBlockError,
        pub az_thread_ptr_delete: extern "C" fn(_:  &mut AzThreadPtr),
        pub az_thread_ptr_fmt_debug: extern "C" fn(_:  &AzThreadPtr) -> AzString,
        pub az_drop_check_ptr_delete: extern "C" fn(_:  &mut AzDropCheckPtr),
        pub az_drop_check_ptr_fmt_debug: extern "C" fn(_:  &AzDropCheckPtr) -> AzString,
        pub az_timer_id_delete: extern "C" fn(_:  &mut AzTimerId),
        pub az_timer_id_deep_copy: extern "C" fn(_:  &AzTimerId) -> AzTimerId,
        pub az_timer_id_fmt_debug: extern "C" fn(_:  &AzTimerId) -> AzString,
        pub az_terminate_timer_delete: extern "C" fn(_:  &mut AzTerminateTimer),
        pub az_terminate_timer_deep_copy: extern "C" fn(_:  &AzTerminateTimer) -> AzTerminateTimer,
        pub az_terminate_timer_fmt_debug: extern "C" fn(_:  &AzTerminateTimer) -> AzString,
        pub az_block_error_delete: extern "C" fn(_:  &mut AzBlockError),
        pub az_block_error_deep_copy: extern "C" fn(_:  &AzBlockError) -> AzBlockError,
        pub az_block_error_fmt_debug: extern "C" fn(_:  &AzBlockError) -> AzString,
        pub az_task_bar_icon_delete: extern "C" fn(_:  &mut AzTaskBarIcon),
        pub az_task_bar_icon_deep_copy: extern "C" fn(_:  &AzTaskBarIcon) -> AzTaskBarIcon,
        pub az_task_bar_icon_fmt_debug: extern "C" fn(_:  &AzTaskBarIcon) -> AzString,
        pub az_x_window_type_delete: extern "C" fn(_:  &mut AzXWindowType),
        pub az_x_window_type_deep_copy: extern "C" fn(_:  &AzXWindowType) -> AzXWindowType,
        pub az_x_window_type_fmt_debug: extern "C" fn(_:  &AzXWindowType) -> AzString,
        pub az_physical_position_i32_delete: extern "C" fn(_:  &mut AzPhysicalPositionI32),
        pub az_physical_position_i32_deep_copy: extern "C" fn(_:  &AzPhysicalPositionI32) -> AzPhysicalPositionI32,
        pub az_physical_position_i32_fmt_debug: extern "C" fn(_:  &AzPhysicalPositionI32) -> AzString,
        pub az_physical_position_i32_partial_eq: extern "C" fn(_:  &AzPhysicalPositionI32, _:  &AzPhysicalPositionI32) -> bool,
        pub az_physical_position_i32_partial_cmp: extern "C" fn(_:  &AzPhysicalPositionI32, _:  &AzPhysicalPositionI32) -> u8,
        pub az_physical_position_i32_cmp: extern "C" fn(_:  &AzPhysicalPositionI32, _:  &AzPhysicalPositionI32) -> u8,
        pub az_physical_position_i32_hash: extern "C" fn(_:  &AzPhysicalPositionI32) -> u64,
        pub az_physical_size_u32_delete: extern "C" fn(_:  &mut AzPhysicalSizeU32),
        pub az_physical_size_u32_deep_copy: extern "C" fn(_:  &AzPhysicalSizeU32) -> AzPhysicalSizeU32,
        pub az_physical_size_u32_fmt_debug: extern "C" fn(_:  &AzPhysicalSizeU32) -> AzString,
        pub az_physical_size_u32_partial_eq: extern "C" fn(_:  &AzPhysicalSizeU32, _:  &AzPhysicalSizeU32) -> bool,
        pub az_physical_size_u32_partial_cmp: extern "C" fn(_:  &AzPhysicalSizeU32, _:  &AzPhysicalSizeU32) -> u8,
        pub az_physical_size_u32_cmp: extern "C" fn(_:  &AzPhysicalSizeU32, _:  &AzPhysicalSizeU32) -> u8,
        pub az_physical_size_u32_hash: extern "C" fn(_:  &AzPhysicalSizeU32) -> u64,
        pub az_logical_position_delete: extern "C" fn(_:  &mut AzLogicalPosition),
        pub az_logical_position_deep_copy: extern "C" fn(_:  &AzLogicalPosition) -> AzLogicalPosition,
        pub az_logical_position_fmt_debug: extern "C" fn(_:  &AzLogicalPosition) -> AzString,
        pub az_icon_key_delete: extern "C" fn(_:  &mut AzIconKey),
        pub az_icon_key_deep_copy: extern "C" fn(_:  &AzIconKey) -> AzIconKey,
        pub az_icon_key_fmt_debug: extern "C" fn(_:  &AzIconKey) -> AzString,
        pub az_small_window_icon_bytes_delete: extern "C" fn(_:  &mut AzSmallWindowIconBytes),
        pub az_small_window_icon_bytes_deep_copy: extern "C" fn(_:  &AzSmallWindowIconBytes) -> AzSmallWindowIconBytes,
        pub az_small_window_icon_bytes_fmt_debug: extern "C" fn(_:  &AzSmallWindowIconBytes) -> AzString,
        pub az_large_window_icon_bytes_delete: extern "C" fn(_:  &mut AzLargeWindowIconBytes),
        pub az_large_window_icon_bytes_deep_copy: extern "C" fn(_:  &AzLargeWindowIconBytes) -> AzLargeWindowIconBytes,
        pub az_large_window_icon_bytes_fmt_debug: extern "C" fn(_:  &AzLargeWindowIconBytes) -> AzString,
        pub az_window_icon_delete: extern "C" fn(_:  &mut AzWindowIcon),
        pub az_window_icon_deep_copy: extern "C" fn(_:  &AzWindowIcon) -> AzWindowIcon,
        pub az_window_icon_fmt_debug: extern "C" fn(_:  &AzWindowIcon) -> AzString,
        pub az_virtual_key_code_delete: extern "C" fn(_:  &mut AzVirtualKeyCode),
        pub az_virtual_key_code_deep_copy: extern "C" fn(_:  &AzVirtualKeyCode) -> AzVirtualKeyCode,
        pub az_virtual_key_code_fmt_debug: extern "C" fn(_:  &AzVirtualKeyCode) -> AzString,
        pub az_accelerator_key_delete: extern "C" fn(_:  &mut AzAcceleratorKey),
        pub az_accelerator_key_deep_copy: extern "C" fn(_:  &AzAcceleratorKey) -> AzAcceleratorKey,
        pub az_accelerator_key_fmt_debug: extern "C" fn(_:  &AzAcceleratorKey) -> AzString,
        pub az_window_size_delete: extern "C" fn(_:  &mut AzWindowSize),
        pub az_window_size_deep_copy: extern "C" fn(_:  &AzWindowSize) -> AzWindowSize,
        pub az_window_size_fmt_debug: extern "C" fn(_:  &AzWindowSize) -> AzString,
        pub az_window_flags_delete: extern "C" fn(_:  &mut AzWindowFlags),
        pub az_window_flags_deep_copy: extern "C" fn(_:  &AzWindowFlags) -> AzWindowFlags,
        pub az_window_flags_fmt_debug: extern "C" fn(_:  &AzWindowFlags) -> AzString,
        pub az_debug_state_delete: extern "C" fn(_:  &mut AzDebugState),
        pub az_debug_state_deep_copy: extern "C" fn(_:  &AzDebugState) -> AzDebugState,
        pub az_debug_state_fmt_debug: extern "C" fn(_:  &AzDebugState) -> AzString,
        pub az_keyboard_state_delete: extern "C" fn(_:  &mut AzKeyboardState),
        pub az_keyboard_state_deep_copy: extern "C" fn(_:  &AzKeyboardState) -> AzKeyboardState,
        pub az_keyboard_state_fmt_debug: extern "C" fn(_:  &AzKeyboardState) -> AzString,
        pub az_mouse_cursor_type_delete: extern "C" fn(_:  &mut AzMouseCursorType),
        pub az_mouse_cursor_type_deep_copy: extern "C" fn(_:  &AzMouseCursorType) -> AzMouseCursorType,
        pub az_mouse_cursor_type_fmt_debug: extern "C" fn(_:  &AzMouseCursorType) -> AzString,
        pub az_cursor_position_delete: extern "C" fn(_:  &mut AzCursorPosition),
        pub az_cursor_position_deep_copy: extern "C" fn(_:  &AzCursorPosition) -> AzCursorPosition,
        pub az_cursor_position_fmt_debug: extern "C" fn(_:  &AzCursorPosition) -> AzString,
        pub az_mouse_state_delete: extern "C" fn(_:  &mut AzMouseState),
        pub az_mouse_state_deep_copy: extern "C" fn(_:  &AzMouseState) -> AzMouseState,
        pub az_mouse_state_fmt_debug: extern "C" fn(_:  &AzMouseState) -> AzString,
        pub az_platform_specific_options_delete: extern "C" fn(_:  &mut AzPlatformSpecificOptions),
        pub az_platform_specific_options_deep_copy: extern "C" fn(_:  &AzPlatformSpecificOptions) -> AzPlatformSpecificOptions,
        pub az_platform_specific_options_fmt_debug: extern "C" fn(_:  &AzPlatformSpecificOptions) -> AzString,
        pub az_windows_window_options_delete: extern "C" fn(_:  &mut AzWindowsWindowOptions),
        pub az_windows_window_options_deep_copy: extern "C" fn(_:  &AzWindowsWindowOptions) -> AzWindowsWindowOptions,
        pub az_windows_window_options_fmt_debug: extern "C" fn(_:  &AzWindowsWindowOptions) -> AzString,
        pub az_wayland_theme_delete: extern "C" fn(_:  &mut AzWaylandTheme),
        pub az_wayland_theme_deep_copy: extern "C" fn(_:  &AzWaylandTheme) -> AzWaylandTheme,
        pub az_wayland_theme_fmt_debug: extern "C" fn(_:  &AzWaylandTheme) -> AzString,
        pub az_renderer_type_delete: extern "C" fn(_:  &mut AzRendererType),
        pub az_renderer_type_deep_copy: extern "C" fn(_:  &AzRendererType) -> AzRendererType,
        pub az_renderer_type_fmt_debug: extern "C" fn(_:  &AzRendererType) -> AzString,
        pub az_string_pair_delete: extern "C" fn(_:  &mut AzStringPair),
        pub az_string_pair_deep_copy: extern "C" fn(_:  &AzStringPair) -> AzStringPair,
        pub az_string_pair_fmt_debug: extern "C" fn(_:  &AzStringPair) -> AzString,
        pub az_linux_window_options_delete: extern "C" fn(_:  &mut AzLinuxWindowOptions),
        pub az_linux_window_options_deep_copy: extern "C" fn(_:  &AzLinuxWindowOptions) -> AzLinuxWindowOptions,
        pub az_linux_window_options_fmt_debug: extern "C" fn(_:  &AzLinuxWindowOptions) -> AzString,
        pub az_mac_window_options_delete: extern "C" fn(_:  &mut AzMacWindowOptions),
        pub az_mac_window_options_deep_copy: extern "C" fn(_:  &AzMacWindowOptions) -> AzMacWindowOptions,
        pub az_mac_window_options_fmt_debug: extern "C" fn(_:  &AzMacWindowOptions) -> AzString,
        pub az_wasm_window_options_delete: extern "C" fn(_:  &mut AzWasmWindowOptions),
        pub az_wasm_window_options_deep_copy: extern "C" fn(_:  &AzWasmWindowOptions) -> AzWasmWindowOptions,
        pub az_wasm_window_options_fmt_debug: extern "C" fn(_:  &AzWasmWindowOptions) -> AzString,
        pub az_full_screen_mode_delete: extern "C" fn(_:  &mut AzFullScreenMode),
        pub az_full_screen_mode_deep_copy: extern "C" fn(_:  &AzFullScreenMode) -> AzFullScreenMode,
        pub az_full_screen_mode_fmt_debug: extern "C" fn(_:  &AzFullScreenMode) -> AzString,
        pub az_window_state_delete: extern "C" fn(_:  &mut AzWindowState),
        pub az_window_state_deep_copy: extern "C" fn(_:  &AzWindowState) -> AzWindowState,
        pub az_window_state_fmt_debug: extern "C" fn(_:  &AzWindowState) -> AzString,
        pub az_logical_size_delete: extern "C" fn(_:  &mut AzLogicalSize),
        pub az_logical_size_deep_copy: extern "C" fn(_:  &AzLogicalSize) -> AzLogicalSize,
        pub az_logical_size_fmt_debug: extern "C" fn(_:  &AzLogicalSize) -> AzString,
        pub az_hot_reload_options_delete: extern "C" fn(_:  &mut AzHotReloadOptions),
        pub az_hot_reload_options_deep_copy: extern "C" fn(_:  &AzHotReloadOptions) -> AzHotReloadOptions,
        pub az_hot_reload_options_fmt_debug: extern "C" fn(_:  &AzHotReloadOptions) -> AzString,
        pub az_window_create_options_new: extern "C" fn(_:  AzCss) -> AzWindowCreateOptions,
        pub az_window_create_options_delete: extern "C" fn(_:  &mut AzWindowCreateOptions),
        pub az_window_create_options_deep_copy: extern "C" fn(_:  &AzWindowCreateOptions) -> AzWindowCreateOptions,
        pub az_window_create_options_fmt_debug: extern "C" fn(_:  &AzWindowCreateOptions) -> AzString,
    }

    pub fn initialize_library(path: &std::path::Path) -> Option<AzulDll> {
        use std::mem::transmute;
        let lib = Library::new(path)?;
        unsafe {
            let az_string_from_utf8_unchecked: extern "C" fn(_:  *const u8, _:  usize) -> AzString = transmute(lib.get(b"az_string_from_utf8_unchecked")?);
            let az_string_from_utf8_lossy: extern "C" fn(_:  *const u8, _:  usize) -> AzString = transmute(lib.get(b"az_string_from_utf8_lossy")?);
            let az_string_into_bytes: extern "C" fn(_:  AzString) -> AzU8Vec = transmute(lib.get(b"az_string_into_bytes")?);
            let az_string_delete: extern "C" fn(_:  &mut AzString) = transmute(lib.get(b"az_string_delete")?);
            let az_string_deep_copy: extern "C" fn(_:  &AzString) -> AzString = transmute(lib.get(b"az_string_deep_copy")?);
            let az_string_fmt_debug: extern "C" fn(_:  &AzString) -> AzString = transmute(lib.get(b"az_string_fmt_debug")?);
            let az_x_window_type_vec_copy_from: extern "C" fn(_:  *mut AzXWindowType, _:  usize) -> AzXWindowTypeVec = transmute(lib.get(b"az_x_window_type_vec_copy_from")?);
            let az_x_window_type_vec_delete: extern "C" fn(_:  &mut AzXWindowTypeVec) = transmute(lib.get(b"az_x_window_type_vec_delete")?);
            let az_x_window_type_vec_deep_copy: extern "C" fn(_:  &AzXWindowTypeVec) -> AzXWindowTypeVec = transmute(lib.get(b"az_x_window_type_vec_deep_copy")?);
            let az_x_window_type_vec_fmt_debug: extern "C" fn(_:  &AzXWindowTypeVec) -> AzString = transmute(lib.get(b"az_x_window_type_vec_fmt_debug")?);
            let az_virtual_key_code_vec_copy_from: extern "C" fn(_:  *mut AzVirtualKeyCode, _:  usize) -> AzVirtualKeyCodeVec = transmute(lib.get(b"az_virtual_key_code_vec_copy_from")?);
            let az_virtual_key_code_vec_delete: extern "C" fn(_:  &mut AzVirtualKeyCodeVec) = transmute(lib.get(b"az_virtual_key_code_vec_delete")?);
            let az_virtual_key_code_vec_deep_copy: extern "C" fn(_:  &AzVirtualKeyCodeVec) -> AzVirtualKeyCodeVec = transmute(lib.get(b"az_virtual_key_code_vec_deep_copy")?);
            let az_virtual_key_code_vec_fmt_debug: extern "C" fn(_:  &AzVirtualKeyCodeVec) -> AzString = transmute(lib.get(b"az_virtual_key_code_vec_fmt_debug")?);
            let az_scan_code_vec_copy_from: extern "C" fn(_:  *mut u32, _:  usize) -> AzScanCodeVec = transmute(lib.get(b"az_scan_code_vec_copy_from")?);
            let az_scan_code_vec_delete: extern "C" fn(_:  &mut AzScanCodeVec) = transmute(lib.get(b"az_scan_code_vec_delete")?);
            let az_scan_code_vec_deep_copy: extern "C" fn(_:  &AzScanCodeVec) -> AzScanCodeVec = transmute(lib.get(b"az_scan_code_vec_deep_copy")?);
            let az_scan_code_vec_fmt_debug: extern "C" fn(_:  &AzScanCodeVec) -> AzString = transmute(lib.get(b"az_scan_code_vec_fmt_debug")?);
            let az_css_declaration_vec_copy_from: extern "C" fn(_:  *mut AzCssDeclaration, _:  usize) -> AzCssDeclarationVec = transmute(lib.get(b"az_css_declaration_vec_copy_from")?);
            let az_css_declaration_vec_delete: extern "C" fn(_:  &mut AzCssDeclarationVec) = transmute(lib.get(b"az_css_declaration_vec_delete")?);
            let az_css_declaration_vec_deep_copy: extern "C" fn(_:  &AzCssDeclarationVec) -> AzCssDeclarationVec = transmute(lib.get(b"az_css_declaration_vec_deep_copy")?);
            let az_css_declaration_vec_fmt_debug: extern "C" fn(_:  &AzCssDeclarationVec) -> AzString = transmute(lib.get(b"az_css_declaration_vec_fmt_debug")?);
            let az_css_path_selector_vec_copy_from: extern "C" fn(_:  *mut AzCssPathSelector, _:  usize) -> AzCssPathSelectorVec = transmute(lib.get(b"az_css_path_selector_vec_copy_from")?);
            let az_css_path_selector_vec_delete: extern "C" fn(_:  &mut AzCssPathSelectorVec) = transmute(lib.get(b"az_css_path_selector_vec_delete")?);
            let az_css_path_selector_vec_deep_copy: extern "C" fn(_:  &AzCssPathSelectorVec) -> AzCssPathSelectorVec = transmute(lib.get(b"az_css_path_selector_vec_deep_copy")?);
            let az_css_path_selector_vec_fmt_debug: extern "C" fn(_:  &AzCssPathSelectorVec) -> AzString = transmute(lib.get(b"az_css_path_selector_vec_fmt_debug")?);
            let az_stylesheet_vec_copy_from: extern "C" fn(_:  *mut AzStylesheet, _:  usize) -> AzStylesheetVec = transmute(lib.get(b"az_stylesheet_vec_copy_from")?);
            let az_stylesheet_vec_delete: extern "C" fn(_:  &mut AzStylesheetVec) = transmute(lib.get(b"az_stylesheet_vec_delete")?);
            let az_stylesheet_vec_deep_copy: extern "C" fn(_:  &AzStylesheetVec) -> AzStylesheetVec = transmute(lib.get(b"az_stylesheet_vec_deep_copy")?);
            let az_stylesheet_vec_fmt_debug: extern "C" fn(_:  &AzStylesheetVec) -> AzString = transmute(lib.get(b"az_stylesheet_vec_fmt_debug")?);
            let az_css_rule_block_vec_copy_from: extern "C" fn(_:  *mut AzCssRuleBlock, _:  usize) -> AzCssRuleBlockVec = transmute(lib.get(b"az_css_rule_block_vec_copy_from")?);
            let az_css_rule_block_vec_delete: extern "C" fn(_:  &mut AzCssRuleBlockVec) = transmute(lib.get(b"az_css_rule_block_vec_delete")?);
            let az_css_rule_block_vec_deep_copy: extern "C" fn(_:  &AzCssRuleBlockVec) -> AzCssRuleBlockVec = transmute(lib.get(b"az_css_rule_block_vec_deep_copy")?);
            let az_css_rule_block_vec_fmt_debug: extern "C" fn(_:  &AzCssRuleBlockVec) -> AzString = transmute(lib.get(b"az_css_rule_block_vec_fmt_debug")?);
            let az_u8_vec_copy_from: extern "C" fn(_:  *mut u8, _:  usize) -> AzU8Vec = transmute(lib.get(b"az_u8_vec_copy_from")?);
            let az_u8_vec_delete: extern "C" fn(_:  &mut AzU8Vec) = transmute(lib.get(b"az_u8_vec_delete")?);
            let az_u8_vec_deep_copy: extern "C" fn(_:  &AzU8Vec) -> AzU8Vec = transmute(lib.get(b"az_u8_vec_deep_copy")?);
            let az_u8_vec_fmt_debug: extern "C" fn(_:  &AzU8Vec) -> AzString = transmute(lib.get(b"az_u8_vec_fmt_debug")?);
            let az_callback_data_vec_copy_from: extern "C" fn(_:  *mut AzCallbackData, _:  usize) -> AzCallbackDataVec = transmute(lib.get(b"az_callback_data_vec_copy_from")?);
            let az_callback_data_vec_delete: extern "C" fn(_:  &mut AzCallbackDataVec) = transmute(lib.get(b"az_callback_data_vec_delete")?);
            let az_callback_data_vec_deep_copy: extern "C" fn(_:  &AzCallbackDataVec) -> AzCallbackDataVec = transmute(lib.get(b"az_callback_data_vec_deep_copy")?);
            let az_callback_data_vec_fmt_debug: extern "C" fn(_:  &AzCallbackDataVec) -> AzString = transmute(lib.get(b"az_callback_data_vec_fmt_debug")?);
            let az_debug_message_vec_copy_from: extern "C" fn(_:  *mut AzDebugMessage, _:  usize) -> AzDebugMessageVec = transmute(lib.get(b"az_debug_message_vec_copy_from")?);
            let az_debug_message_vec_delete: extern "C" fn(_:  &mut AzDebugMessageVec) = transmute(lib.get(b"az_debug_message_vec_delete")?);
            let az_debug_message_vec_deep_copy: extern "C" fn(_:  &AzDebugMessageVec) -> AzDebugMessageVec = transmute(lib.get(b"az_debug_message_vec_deep_copy")?);
            let az_debug_message_vec_fmt_debug: extern "C" fn(_:  &AzDebugMessageVec) -> AzString = transmute(lib.get(b"az_debug_message_vec_fmt_debug")?);
            let az_g_luint_vec_copy_from: extern "C" fn(_:  *mut u32, _:  usize) -> AzGLuintVec = transmute(lib.get(b"az_g_luint_vec_copy_from")?);
            let az_g_luint_vec_delete: extern "C" fn(_:  &mut AzGLuintVec) = transmute(lib.get(b"az_g_luint_vec_delete")?);
            let az_g_luint_vec_deep_copy: extern "C" fn(_:  &AzGLuintVec) -> AzGLuintVec = transmute(lib.get(b"az_g_luint_vec_deep_copy")?);
            let az_g_luint_vec_fmt_debug: extern "C" fn(_:  &AzGLuintVec) -> AzString = transmute(lib.get(b"az_g_luint_vec_fmt_debug")?);
            let az_g_lint_vec_copy_from: extern "C" fn(_:  *mut i32, _:  usize) -> AzGLintVec = transmute(lib.get(b"az_g_lint_vec_copy_from")?);
            let az_g_lint_vec_delete: extern "C" fn(_:  &mut AzGLintVec) = transmute(lib.get(b"az_g_lint_vec_delete")?);
            let az_g_lint_vec_deep_copy: extern "C" fn(_:  &AzGLintVec) -> AzGLintVec = transmute(lib.get(b"az_g_lint_vec_deep_copy")?);
            let az_g_lint_vec_fmt_debug: extern "C" fn(_:  &AzGLintVec) -> AzString = transmute(lib.get(b"az_g_lint_vec_fmt_debug")?);
            let az_override_property_vec_copy_from: extern "C" fn(_:  *mut AzOverrideProperty, _:  usize) -> AzOverridePropertyVec = transmute(lib.get(b"az_override_property_vec_copy_from")?);
            let az_override_property_vec_delete: extern "C" fn(_:  &mut AzOverridePropertyVec) = transmute(lib.get(b"az_override_property_vec_delete")?);
            let az_override_property_vec_deep_copy: extern "C" fn(_:  &AzOverridePropertyVec) -> AzOverridePropertyVec = transmute(lib.get(b"az_override_property_vec_deep_copy")?);
            let az_override_property_vec_fmt_debug: extern "C" fn(_:  &AzOverridePropertyVec) -> AzString = transmute(lib.get(b"az_override_property_vec_fmt_debug")?);
            let az_dom_vec_copy_from: extern "C" fn(_:  *mut AzDom, _:  usize) -> AzDomVec = transmute(lib.get(b"az_dom_vec_copy_from")?);
            let az_dom_vec_delete: extern "C" fn(_:  &mut AzDomVec) = transmute(lib.get(b"az_dom_vec_delete")?);
            let az_dom_vec_deep_copy: extern "C" fn(_:  &AzDomVec) -> AzDomVec = transmute(lib.get(b"az_dom_vec_deep_copy")?);
            let az_dom_vec_fmt_debug: extern "C" fn(_:  &AzDomVec) -> AzString = transmute(lib.get(b"az_dom_vec_fmt_debug")?);
            let az_string_vec_copy_from: extern "C" fn(_:  *mut AzString, _:  usize) -> AzStringVec = transmute(lib.get(b"az_string_vec_copy_from")?);
            let az_string_vec_delete: extern "C" fn(_:  &mut AzStringVec) = transmute(lib.get(b"az_string_vec_delete")?);
            let az_string_vec_deep_copy: extern "C" fn(_:  &AzStringVec) -> AzStringVec = transmute(lib.get(b"az_string_vec_deep_copy")?);
            let az_string_vec_fmt_debug: extern "C" fn(_:  &AzStringVec) -> AzString = transmute(lib.get(b"az_string_vec_fmt_debug")?);
            let az_string_pair_vec_copy_from: extern "C" fn(_:  *mut AzStringPair, _:  usize) -> AzStringPairVec = transmute(lib.get(b"az_string_pair_vec_copy_from")?);
            let az_string_pair_vec_delete: extern "C" fn(_:  &mut AzStringPairVec) = transmute(lib.get(b"az_string_pair_vec_delete")?);
            let az_string_pair_vec_deep_copy: extern "C" fn(_:  &AzStringPairVec) -> AzStringPairVec = transmute(lib.get(b"az_string_pair_vec_deep_copy")?);
            let az_string_pair_vec_fmt_debug: extern "C" fn(_:  &AzStringPairVec) -> AzString = transmute(lib.get(b"az_string_pair_vec_fmt_debug")?);
            let az_gradient_stop_pre_vec_copy_from: extern "C" fn(_:  *mut AzGradientStopPre, _:  usize) -> AzGradientStopPreVec = transmute(lib.get(b"az_gradient_stop_pre_vec_copy_from")?);
            let az_gradient_stop_pre_vec_delete: extern "C" fn(_:  &mut AzGradientStopPreVec) = transmute(lib.get(b"az_gradient_stop_pre_vec_delete")?);
            let az_gradient_stop_pre_vec_deep_copy: extern "C" fn(_:  &AzGradientStopPreVec) -> AzGradientStopPreVec = transmute(lib.get(b"az_gradient_stop_pre_vec_deep_copy")?);
            let az_gradient_stop_pre_vec_fmt_debug: extern "C" fn(_:  &AzGradientStopPreVec) -> AzString = transmute(lib.get(b"az_gradient_stop_pre_vec_fmt_debug")?);
            let az_option_wayland_theme_delete: extern "C" fn(_:  &mut AzOptionWaylandTheme) = transmute(lib.get(b"az_option_wayland_theme_delete")?);
            let az_option_wayland_theme_deep_copy: extern "C" fn(_:  &AzOptionWaylandTheme) -> AzOptionWaylandTheme = transmute(lib.get(b"az_option_wayland_theme_deep_copy")?);
            let az_option_wayland_theme_fmt_debug: extern "C" fn(_:  &AzOptionWaylandTheme) -> AzString = transmute(lib.get(b"az_option_wayland_theme_fmt_debug")?);
            let az_option_task_bar_icon_delete: extern "C" fn(_:  &mut AzOptionTaskBarIcon) = transmute(lib.get(b"az_option_task_bar_icon_delete")?);
            let az_option_task_bar_icon_deep_copy: extern "C" fn(_:  &AzOptionTaskBarIcon) -> AzOptionTaskBarIcon = transmute(lib.get(b"az_option_task_bar_icon_deep_copy")?);
            let az_option_task_bar_icon_fmt_debug: extern "C" fn(_:  &AzOptionTaskBarIcon) -> AzString = transmute(lib.get(b"az_option_task_bar_icon_fmt_debug")?);
            let az_option_hwnd_handle_delete: extern "C" fn(_:  &mut AzOptionHwndHandle) = transmute(lib.get(b"az_option_hwnd_handle_delete")?);
            let az_option_hwnd_handle_deep_copy: extern "C" fn(_:  &AzOptionHwndHandle) -> AzOptionHwndHandle = transmute(lib.get(b"az_option_hwnd_handle_deep_copy")?);
            let az_option_hwnd_handle_fmt_debug: extern "C" fn(_:  &AzOptionHwndHandle) -> AzString = transmute(lib.get(b"az_option_hwnd_handle_fmt_debug")?);
            let az_option_logical_position_delete: extern "C" fn(_:  &mut AzOptionLogicalPosition) = transmute(lib.get(b"az_option_logical_position_delete")?);
            let az_option_logical_position_deep_copy: extern "C" fn(_:  &AzOptionLogicalPosition) -> AzOptionLogicalPosition = transmute(lib.get(b"az_option_logical_position_deep_copy")?);
            let az_option_logical_position_fmt_debug: extern "C" fn(_:  &AzOptionLogicalPosition) -> AzString = transmute(lib.get(b"az_option_logical_position_fmt_debug")?);
            let az_option_hot_reload_options_delete: extern "C" fn(_:  &mut AzOptionHotReloadOptions) = transmute(lib.get(b"az_option_hot_reload_options_delete")?);
            let az_option_hot_reload_options_deep_copy: extern "C" fn(_:  &AzOptionHotReloadOptions) -> AzOptionHotReloadOptions = transmute(lib.get(b"az_option_hot_reload_options_deep_copy")?);
            let az_option_hot_reload_options_fmt_debug: extern "C" fn(_:  &AzOptionHotReloadOptions) -> AzString = transmute(lib.get(b"az_option_hot_reload_options_fmt_debug")?);
            let az_option_physical_position_i32_delete: extern "C" fn(_:  &mut AzOptionPhysicalPositionI32) = transmute(lib.get(b"az_option_physical_position_i32_delete")?);
            let az_option_physical_position_i32_deep_copy: extern "C" fn(_:  &AzOptionPhysicalPositionI32) -> AzOptionPhysicalPositionI32 = transmute(lib.get(b"az_option_physical_position_i32_deep_copy")?);
            let az_option_physical_position_i32_fmt_debug: extern "C" fn(_:  &AzOptionPhysicalPositionI32) -> AzString = transmute(lib.get(b"az_option_physical_position_i32_fmt_debug")?);
            let az_option_window_icon_delete: extern "C" fn(_:  &mut AzOptionWindowIcon) = transmute(lib.get(b"az_option_window_icon_delete")?);
            let az_option_window_icon_deep_copy: extern "C" fn(_:  &AzOptionWindowIcon) -> AzOptionWindowIcon = transmute(lib.get(b"az_option_window_icon_deep_copy")?);
            let az_option_window_icon_fmt_debug: extern "C" fn(_:  &AzOptionWindowIcon) -> AzString = transmute(lib.get(b"az_option_window_icon_fmt_debug")?);
            let az_option_string_delete: extern "C" fn(_:  &mut AzOptionString) = transmute(lib.get(b"az_option_string_delete")?);
            let az_option_string_deep_copy: extern "C" fn(_:  &AzOptionString) -> AzOptionString = transmute(lib.get(b"az_option_string_deep_copy")?);
            let az_option_string_fmt_debug: extern "C" fn(_:  &AzOptionString) -> AzString = transmute(lib.get(b"az_option_string_fmt_debug")?);
            let az_option_x11_visual_delete: extern "C" fn(_:  &mut AzOptionX11Visual) = transmute(lib.get(b"az_option_x11_visual_delete")?);
            let az_option_x11_visual_deep_copy: extern "C" fn(_:  &AzOptionX11Visual) -> AzOptionX11Visual = transmute(lib.get(b"az_option_x11_visual_deep_copy")?);
            let az_option_x11_visual_fmt_debug: extern "C" fn(_:  &AzOptionX11Visual) -> AzString = transmute(lib.get(b"az_option_x11_visual_fmt_debug")?);
            let az_option_i32_delete: extern "C" fn(_:  &mut AzOptionI32) = transmute(lib.get(b"az_option_i32_delete")?);
            let az_option_i32_deep_copy: extern "C" fn(_:  &AzOptionI32) -> AzOptionI32 = transmute(lib.get(b"az_option_i32_deep_copy")?);
            let az_option_i32_fmt_debug: extern "C" fn(_:  &AzOptionI32) -> AzString = transmute(lib.get(b"az_option_i32_fmt_debug")?);
            let az_option_f32_delete: extern "C" fn(_:  &mut AzOptionF32) = transmute(lib.get(b"az_option_f32_delete")?);
            let az_option_f32_deep_copy: extern "C" fn(_:  &AzOptionF32) -> AzOptionF32 = transmute(lib.get(b"az_option_f32_deep_copy")?);
            let az_option_f32_fmt_debug: extern "C" fn(_:  &AzOptionF32) -> AzString = transmute(lib.get(b"az_option_f32_fmt_debug")?);
            let az_option_mouse_cursor_type_delete: extern "C" fn(_:  &mut AzOptionMouseCursorType) = transmute(lib.get(b"az_option_mouse_cursor_type_delete")?);
            let az_option_mouse_cursor_type_deep_copy: extern "C" fn(_:  &AzOptionMouseCursorType) -> AzOptionMouseCursorType = transmute(lib.get(b"az_option_mouse_cursor_type_deep_copy")?);
            let az_option_mouse_cursor_type_fmt_debug: extern "C" fn(_:  &AzOptionMouseCursorType) -> AzString = transmute(lib.get(b"az_option_mouse_cursor_type_fmt_debug")?);
            let az_option_logical_size_delete: extern "C" fn(_:  &mut AzOptionLogicalSize) = transmute(lib.get(b"az_option_logical_size_delete")?);
            let az_option_logical_size_deep_copy: extern "C" fn(_:  &AzOptionLogicalSize) -> AzOptionLogicalSize = transmute(lib.get(b"az_option_logical_size_deep_copy")?);
            let az_option_logical_size_fmt_debug: extern "C" fn(_:  &AzOptionLogicalSize) -> AzString = transmute(lib.get(b"az_option_logical_size_fmt_debug")?);
            let az_option_char_delete: extern "C" fn(_:  &mut AzOptionChar) = transmute(lib.get(b"az_option_char_delete")?);
            let az_option_char_deep_copy: extern "C" fn(_:  &AzOptionChar) -> AzOptionChar = transmute(lib.get(b"az_option_char_deep_copy")?);
            let az_option_char_fmt_debug: extern "C" fn(_:  &AzOptionChar) -> AzString = transmute(lib.get(b"az_option_char_fmt_debug")?);
            let az_option_virtual_key_code_delete: extern "C" fn(_:  &mut AzOptionVirtualKeyCode) = transmute(lib.get(b"az_option_virtual_key_code_delete")?);
            let az_option_virtual_key_code_deep_copy: extern "C" fn(_:  &AzOptionVirtualKeyCode) -> AzOptionVirtualKeyCode = transmute(lib.get(b"az_option_virtual_key_code_deep_copy")?);
            let az_option_virtual_key_code_fmt_debug: extern "C" fn(_:  &AzOptionVirtualKeyCode) -> AzString = transmute(lib.get(b"az_option_virtual_key_code_fmt_debug")?);
            let az_option_percentage_value_delete: extern "C" fn(_:  &mut AzOptionPercentageValue) = transmute(lib.get(b"az_option_percentage_value_delete")?);
            let az_option_percentage_value_deep_copy: extern "C" fn(_:  &AzOptionPercentageValue) -> AzOptionPercentageValue = transmute(lib.get(b"az_option_percentage_value_deep_copy")?);
            let az_option_percentage_value_fmt_debug: extern "C" fn(_:  &AzOptionPercentageValue) -> AzString = transmute(lib.get(b"az_option_percentage_value_fmt_debug")?);
            let az_option_dom_delete: extern "C" fn(_:  &mut AzOptionDom) = transmute(lib.get(b"az_option_dom_delete")?);
            let az_option_dom_deep_copy: extern "C" fn(_:  &AzOptionDom) -> AzOptionDom = transmute(lib.get(b"az_option_dom_deep_copy")?);
            let az_option_dom_fmt_debug: extern "C" fn(_:  &AzOptionDom) -> AzString = transmute(lib.get(b"az_option_dom_fmt_debug")?);
            let az_option_texture_delete: extern "C" fn(_:  &mut AzOptionTexture) = transmute(lib.get(b"az_option_texture_delete")?);
            let az_option_texture_fmt_debug: extern "C" fn(_:  &AzOptionTexture) -> AzString = transmute(lib.get(b"az_option_texture_fmt_debug")?);
            let az_option_tab_index_delete: extern "C" fn(_:  &mut AzOptionTabIndex) = transmute(lib.get(b"az_option_tab_index_delete")?);
            let az_option_tab_index_deep_copy: extern "C" fn(_:  &AzOptionTabIndex) -> AzOptionTabIndex = transmute(lib.get(b"az_option_tab_index_deep_copy")?);
            let az_option_tab_index_fmt_debug: extern "C" fn(_:  &AzOptionTabIndex) -> AzString = transmute(lib.get(b"az_option_tab_index_fmt_debug")?);
            let az_option_duration_delete: extern "C" fn(_:  &mut AzOptionDuration) = transmute(lib.get(b"az_option_duration_delete")?);
            let az_option_duration_deep_copy: extern "C" fn(_:  &AzOptionDuration) -> AzOptionDuration = transmute(lib.get(b"az_option_duration_deep_copy")?);
            let az_option_duration_fmt_debug: extern "C" fn(_:  &AzOptionDuration) -> AzString = transmute(lib.get(b"az_option_duration_fmt_debug")?);
            let az_option_instant_ptr_delete: extern "C" fn(_:  &mut AzOptionInstantPtr) = transmute(lib.get(b"az_option_instant_ptr_delete")?);
            let az_option_instant_ptr_deep_copy: extern "C" fn(_:  &AzOptionInstantPtr) -> AzOptionInstantPtr = transmute(lib.get(b"az_option_instant_ptr_deep_copy")?);
            let az_option_instant_ptr_fmt_debug: extern "C" fn(_:  &AzOptionInstantPtr) -> AzString = transmute(lib.get(b"az_option_instant_ptr_fmt_debug")?);
            let az_option_usize_delete: extern "C" fn(_:  &mut AzOptionUsize) = transmute(lib.get(b"az_option_usize_delete")?);
            let az_option_usize_deep_copy: extern "C" fn(_:  &AzOptionUsize) -> AzOptionUsize = transmute(lib.get(b"az_option_usize_deep_copy")?);
            let az_option_usize_fmt_debug: extern "C" fn(_:  &AzOptionUsize) -> AzString = transmute(lib.get(b"az_option_usize_fmt_debug")?);
            let az_option_u8_vec_ref_delete: extern "C" fn(_:  &mut AzOptionU8VecRef) = transmute(lib.get(b"az_option_u8_vec_ref_delete")?);
            let az_option_u8_vec_ref_fmt_debug: extern "C" fn(_:  &AzOptionU8VecRef) -> AzString = transmute(lib.get(b"az_option_u8_vec_ref_fmt_debug")?);
            let az_result_ref_any_block_error_delete: extern "C" fn(_:  &mut AzResultRefAnyBlockError) = transmute(lib.get(b"az_result_ref_any_block_error_delete")?);
            let az_result_ref_any_block_error_deep_copy: extern "C" fn(_:  &AzResultRefAnyBlockError) -> AzResultRefAnyBlockError = transmute(lib.get(b"az_result_ref_any_block_error_deep_copy")?);
            let az_result_ref_any_block_error_fmt_debug: extern "C" fn(_:  &AzResultRefAnyBlockError) -> AzString = transmute(lib.get(b"az_result_ref_any_block_error_fmt_debug")?);
            let az_instant_ptr_now: extern "C" fn() -> AzInstantPtr = transmute(lib.get(b"az_instant_ptr_now")?);
            let az_instant_ptr_delete: extern "C" fn(_:  &mut AzInstantPtr) = transmute(lib.get(b"az_instant_ptr_delete")?);
            let az_instant_ptr_fmt_debug: extern "C" fn(_:  &AzInstantPtr) -> AzString = transmute(lib.get(b"az_instant_ptr_fmt_debug")?);
            let az_instant_ptr_partial_eq: extern "C" fn(_:  &AzInstantPtr, _:  &AzInstantPtr) -> bool = transmute(lib.get(b"az_instant_ptr_partial_eq")?);
            let az_instant_ptr_partial_cmp: extern "C" fn(_:  &AzInstantPtr, _:  &AzInstantPtr) -> u8 = transmute(lib.get(b"az_instant_ptr_partial_cmp")?);
            let az_instant_ptr_cmp: extern "C" fn(_:  &AzInstantPtr, _:  &AzInstantPtr) -> u8 = transmute(lib.get(b"az_instant_ptr_cmp")?);
            let az_duration_delete: extern "C" fn(_:  &mut AzDuration) = transmute(lib.get(b"az_duration_delete")?);
            let az_duration_deep_copy: extern "C" fn(_:  &AzDuration) -> AzDuration = transmute(lib.get(b"az_duration_deep_copy")?);
            let az_duration_fmt_debug: extern "C" fn(_:  &AzDuration) -> AzString = transmute(lib.get(b"az_duration_fmt_debug")?);
            let az_app_config_ptr_default: extern "C" fn() -> AzAppConfigPtr = transmute(lib.get(b"az_app_config_ptr_default")?);
            let az_app_config_ptr_delete: extern "C" fn(_:  &mut AzAppConfigPtr) = transmute(lib.get(b"az_app_config_ptr_delete")?);
            let az_app_config_ptr_fmt_debug: extern "C" fn(_:  &AzAppConfigPtr) -> AzString = transmute(lib.get(b"az_app_config_ptr_fmt_debug")?);
            let az_app_ptr_new: extern "C" fn(_:  AzRefAny, _:  AzAppConfigPtr, _:  AzLayoutCallbackType) -> AzAppPtr = transmute(lib.get(b"az_app_ptr_new")?);
            let az_app_ptr_run: extern "C" fn(_:  AzAppPtr, _:  AzWindowCreateOptions) = transmute(lib.get(b"az_app_ptr_run")?);
            let az_app_ptr_delete: extern "C" fn(_:  &mut AzAppPtr) = transmute(lib.get(b"az_app_ptr_delete")?);
            let az_app_ptr_fmt_debug: extern "C" fn(_:  &AzAppPtr) -> AzString = transmute(lib.get(b"az_app_ptr_fmt_debug")?);
            let az_hidpi_adjusted_bounds_get_logical_size: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzLogicalSize = transmute(lib.get(b"az_hidpi_adjusted_bounds_get_logical_size")?);
            let az_hidpi_adjusted_bounds_get_physical_size: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzPhysicalSizeU32 = transmute(lib.get(b"az_hidpi_adjusted_bounds_get_physical_size")?);
            let az_hidpi_adjusted_bounds_get_hidpi_factor: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> f32 = transmute(lib.get(b"az_hidpi_adjusted_bounds_get_hidpi_factor")?);
            let az_hidpi_adjusted_bounds_delete: extern "C" fn(_:  &mut AzHidpiAdjustedBounds) = transmute(lib.get(b"az_hidpi_adjusted_bounds_delete")?);
            let az_hidpi_adjusted_bounds_deep_copy: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzHidpiAdjustedBounds = transmute(lib.get(b"az_hidpi_adjusted_bounds_deep_copy")?);
            let az_hidpi_adjusted_bounds_fmt_debug: extern "C" fn(_:  &AzHidpiAdjustedBounds) -> AzString = transmute(lib.get(b"az_hidpi_adjusted_bounds_fmt_debug")?);
            let az_layout_callback_delete: extern "C" fn(_:  &mut AzLayoutCallback) = transmute(lib.get(b"az_layout_callback_delete")?);
            let az_layout_callback_deep_copy: extern "C" fn(_:  &AzLayoutCallback) -> AzLayoutCallback = transmute(lib.get(b"az_layout_callback_deep_copy")?);
            let az_layout_callback_fmt_debug: extern "C" fn(_:  &AzLayoutCallback) -> AzString = transmute(lib.get(b"az_layout_callback_fmt_debug")?);
            let az_callback_delete: extern "C" fn(_:  &mut AzCallback) = transmute(lib.get(b"az_callback_delete")?);
            let az_callback_deep_copy: extern "C" fn(_:  &AzCallback) -> AzCallback = transmute(lib.get(b"az_callback_deep_copy")?);
            let az_callback_fmt_debug: extern "C" fn(_:  &AzCallback) -> AzString = transmute(lib.get(b"az_callback_fmt_debug")?);
            let az_callback_partial_eq: extern "C" fn(_:  &AzCallback, _:  &AzCallback) -> bool = transmute(lib.get(b"az_callback_partial_eq")?);
            let az_callback_partial_cmp: extern "C" fn(_:  &AzCallback, _:  &AzCallback) -> u8 = transmute(lib.get(b"az_callback_partial_cmp")?);
            let az_callback_cmp: extern "C" fn(_:  &AzCallback, _:  &AzCallback) -> u8 = transmute(lib.get(b"az_callback_cmp")?);
            let az_callback_hash: extern "C" fn(_:  &AzCallback) -> u64 = transmute(lib.get(b"az_callback_hash")?);
            let az_callback_info_ptr_get_state: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzRefAny = transmute(lib.get(b"az_callback_info_ptr_get_state")?);
            let az_callback_info_ptr_get_keyboard_state: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzKeyboardState = transmute(lib.get(b"az_callback_info_ptr_get_keyboard_state")?);
            let az_callback_info_ptr_get_mouse_state: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzMouseState = transmute(lib.get(b"az_callback_info_ptr_get_mouse_state")?);
            let az_callback_info_ptr_set_window_state: extern "C" fn(_:  &mut AzCallbackInfoPtr, _:  AzWindowState) = transmute(lib.get(b"az_callback_info_ptr_set_window_state")?);
            let az_callback_info_ptr_delete: extern "C" fn(_:  &mut AzCallbackInfoPtr) = transmute(lib.get(b"az_callback_info_ptr_delete")?);
            let az_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzCallbackInfoPtr) -> AzString = transmute(lib.get(b"az_callback_info_ptr_fmt_debug")?);
            let az_update_screen_delete: extern "C" fn(_:  &mut AzUpdateScreen) = transmute(lib.get(b"az_update_screen_delete")?);
            let az_update_screen_deep_copy: extern "C" fn(_:  &AzUpdateScreen) -> AzUpdateScreen = transmute(lib.get(b"az_update_screen_deep_copy")?);
            let az_update_screen_fmt_debug: extern "C" fn(_:  &AzUpdateScreen) -> AzString = transmute(lib.get(b"az_update_screen_fmt_debug")?);
            let az_i_frame_callback_delete: extern "C" fn(_:  &mut AzIFrameCallback) = transmute(lib.get(b"az_i_frame_callback_delete")?);
            let az_i_frame_callback_deep_copy: extern "C" fn(_:  &AzIFrameCallback) -> AzIFrameCallback = transmute(lib.get(b"az_i_frame_callback_deep_copy")?);
            let az_i_frame_callback_fmt_debug: extern "C" fn(_:  &AzIFrameCallback) -> AzString = transmute(lib.get(b"az_i_frame_callback_fmt_debug")?);
            let az_i_frame_callback_info_ptr_get_state: extern "C" fn(_:  &AzIFrameCallbackInfoPtr) -> AzRefAny = transmute(lib.get(b"az_i_frame_callback_info_ptr_get_state")?);
            let az_i_frame_callback_info_ptr_get_bounds: extern "C" fn(_:  &AzIFrameCallbackInfoPtr) -> AzHidpiAdjustedBounds = transmute(lib.get(b"az_i_frame_callback_info_ptr_get_bounds")?);
            let az_i_frame_callback_info_ptr_delete: extern "C" fn(_:  &mut AzIFrameCallbackInfoPtr) = transmute(lib.get(b"az_i_frame_callback_info_ptr_delete")?);
            let az_i_frame_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzIFrameCallbackInfoPtr) -> AzString = transmute(lib.get(b"az_i_frame_callback_info_ptr_fmt_debug")?);
            let az_i_frame_callback_return_delete: extern "C" fn(_:  &mut AzIFrameCallbackReturn) = transmute(lib.get(b"az_i_frame_callback_return_delete")?);
            let az_i_frame_callback_return_deep_copy: extern "C" fn(_:  &AzIFrameCallbackReturn) -> AzIFrameCallbackReturn = transmute(lib.get(b"az_i_frame_callback_return_deep_copy")?);
            let az_i_frame_callback_return_fmt_debug: extern "C" fn(_:  &AzIFrameCallbackReturn) -> AzString = transmute(lib.get(b"az_i_frame_callback_return_fmt_debug")?);
            let az_gl_callback_delete: extern "C" fn(_:  &mut AzGlCallback) = transmute(lib.get(b"az_gl_callback_delete")?);
            let az_gl_callback_deep_copy: extern "C" fn(_:  &AzGlCallback) -> AzGlCallback = transmute(lib.get(b"az_gl_callback_deep_copy")?);
            let az_gl_callback_fmt_debug: extern "C" fn(_:  &AzGlCallback) -> AzString = transmute(lib.get(b"az_gl_callback_fmt_debug")?);
            let az_gl_callback_info_ptr_get_state: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzRefAny = transmute(lib.get(b"az_gl_callback_info_ptr_get_state")?);
            let az_gl_callback_info_ptr_get_bounds: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzHidpiAdjustedBounds = transmute(lib.get(b"az_gl_callback_info_ptr_get_bounds")?);
            let az_gl_callback_info_ptr_delete: extern "C" fn(_:  &mut AzGlCallbackInfoPtr) = transmute(lib.get(b"az_gl_callback_info_ptr_delete")?);
            let az_gl_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzGlCallbackInfoPtr) -> AzString = transmute(lib.get(b"az_gl_callback_info_ptr_fmt_debug")?);
            let az_gl_callback_return_delete: extern "C" fn(_:  &mut AzGlCallbackReturn) = transmute(lib.get(b"az_gl_callback_return_delete")?);
            let az_gl_callback_return_fmt_debug: extern "C" fn(_:  &AzGlCallbackReturn) -> AzString = transmute(lib.get(b"az_gl_callback_return_fmt_debug")?);
            let az_timer_callback_delete: extern "C" fn(_:  &mut AzTimerCallback) = transmute(lib.get(b"az_timer_callback_delete")?);
            let az_timer_callback_deep_copy: extern "C" fn(_:  &AzTimerCallback) -> AzTimerCallback = transmute(lib.get(b"az_timer_callback_deep_copy")?);
            let az_timer_callback_fmt_debug: extern "C" fn(_:  &AzTimerCallback) -> AzString = transmute(lib.get(b"az_timer_callback_fmt_debug")?);
            let az_timer_callback_type_ptr_delete: extern "C" fn(_:  &mut AzTimerCallbackTypePtr) = transmute(lib.get(b"az_timer_callback_type_ptr_delete")?);
            let az_timer_callback_type_ptr_fmt_debug: extern "C" fn(_:  &AzTimerCallbackTypePtr) -> AzString = transmute(lib.get(b"az_timer_callback_type_ptr_fmt_debug")?);
            let az_timer_callback_info_ptr_get_state: extern "C" fn(_:  &AzTimerCallbackInfoPtr) -> AzRefAny = transmute(lib.get(b"az_timer_callback_info_ptr_get_state")?);
            let az_timer_callback_info_ptr_delete: extern "C" fn(_:  &mut AzTimerCallbackInfoPtr) = transmute(lib.get(b"az_timer_callback_info_ptr_delete")?);
            let az_timer_callback_info_ptr_fmt_debug: extern "C" fn(_:  &AzTimerCallbackInfoPtr) -> AzString = transmute(lib.get(b"az_timer_callback_info_ptr_fmt_debug")?);
            let az_timer_callback_return_delete: extern "C" fn(_:  &mut AzTimerCallbackReturn) = transmute(lib.get(b"az_timer_callback_return_delete")?);
            let az_timer_callback_return_deep_copy: extern "C" fn(_:  &AzTimerCallbackReturn) -> AzTimerCallbackReturn = transmute(lib.get(b"az_timer_callback_return_deep_copy")?);
            let az_timer_callback_return_fmt_debug: extern "C" fn(_:  &AzTimerCallbackReturn) -> AzString = transmute(lib.get(b"az_timer_callback_return_fmt_debug")?);
            let az_ref_any_sharing_info_can_be_shared: extern "C" fn(_:  &AzRefAnySharingInfo) -> bool = transmute(lib.get(b"az_ref_any_sharing_info_can_be_shared")?);
            let az_ref_any_sharing_info_can_be_shared_mut: extern "C" fn(_:  &AzRefAnySharingInfo) -> bool = transmute(lib.get(b"az_ref_any_sharing_info_can_be_shared_mut")?);
            let az_ref_any_sharing_info_increase_ref: extern "C" fn(_:  &mut AzRefAnySharingInfo) = transmute(lib.get(b"az_ref_any_sharing_info_increase_ref")?);
            let az_ref_any_sharing_info_decrease_ref: extern "C" fn(_:  &mut AzRefAnySharingInfo) = transmute(lib.get(b"az_ref_any_sharing_info_decrease_ref")?);
            let az_ref_any_sharing_info_increase_refmut: extern "C" fn(_:  &mut AzRefAnySharingInfo) = transmute(lib.get(b"az_ref_any_sharing_info_increase_refmut")?);
            let az_ref_any_sharing_info_decrease_refmut: extern "C" fn(_:  &mut AzRefAnySharingInfo) = transmute(lib.get(b"az_ref_any_sharing_info_decrease_refmut")?);
            let az_ref_any_sharing_info_delete: extern "C" fn(_:  &mut AzRefAnySharingInfo) = transmute(lib.get(b"az_ref_any_sharing_info_delete")?);
            let az_ref_any_sharing_info_fmt_debug: extern "C" fn(_:  &AzRefAnySharingInfo) -> AzString = transmute(lib.get(b"az_ref_any_sharing_info_fmt_debug")?);
            let az_ref_any_new_c: extern "C" fn(_:  *const c_void, _:  usize, _:  u64, _:  AzString, _:  AzRefAnyDestructorType) -> AzRefAny = transmute(lib.get(b"az_ref_any_new_c")?);
            let az_ref_any_is_type: extern "C" fn(_:  &AzRefAny, _:  u64) -> bool = transmute(lib.get(b"az_ref_any_is_type")?);
            let az_ref_any_get_type_name: extern "C" fn(_:  &AzRefAny) -> AzString = transmute(lib.get(b"az_ref_any_get_type_name")?);
            let az_ref_any_can_be_shared: extern "C" fn(_:  &AzRefAny) -> bool = transmute(lib.get(b"az_ref_any_can_be_shared")?);
            let az_ref_any_can_be_shared_mut: extern "C" fn(_:  &AzRefAny) -> bool = transmute(lib.get(b"az_ref_any_can_be_shared_mut")?);
            let az_ref_any_increase_ref: extern "C" fn(_:  &AzRefAny) = transmute(lib.get(b"az_ref_any_increase_ref")?);
            let az_ref_any_decrease_ref: extern "C" fn(_:  &AzRefAny) = transmute(lib.get(b"az_ref_any_decrease_ref")?);
            let az_ref_any_increase_refmut: extern "C" fn(_:  &AzRefAny) = transmute(lib.get(b"az_ref_any_increase_refmut")?);
            let az_ref_any_decrease_refmut: extern "C" fn(_:  &AzRefAny) = transmute(lib.get(b"az_ref_any_decrease_refmut")?);
            let az_ref_any_delete: extern "C" fn(_:  &mut AzRefAny) = transmute(lib.get(b"az_ref_any_delete")?);
            let az_ref_any_deep_copy: extern "C" fn(_:  &AzRefAny) -> AzRefAny = transmute(lib.get(b"az_ref_any_deep_copy")?);
            let az_ref_any_fmt_debug: extern "C" fn(_:  &AzRefAny) -> AzString = transmute(lib.get(b"az_ref_any_fmt_debug")?);
            let az_ref_any_partial_eq: extern "C" fn(_:  &AzRefAny, _:  &AzRefAny) -> bool = transmute(lib.get(b"az_ref_any_partial_eq")?);
            let az_ref_any_partial_cmp: extern "C" fn(_:  &AzRefAny, _:  &AzRefAny) -> u8 = transmute(lib.get(b"az_ref_any_partial_cmp")?);
            let az_ref_any_cmp: extern "C" fn(_:  &AzRefAny, _:  &AzRefAny) -> u8 = transmute(lib.get(b"az_ref_any_cmp")?);
            let az_ref_any_hash: extern "C" fn(_:  &AzRefAny) -> u64 = transmute(lib.get(b"az_ref_any_hash")?);
            let az_layout_info_ptr_delete: extern "C" fn(_:  &mut AzLayoutInfoPtr) = transmute(lib.get(b"az_layout_info_ptr_delete")?);
            let az_layout_info_ptr_fmt_debug: extern "C" fn(_:  &AzLayoutInfoPtr) -> AzString = transmute(lib.get(b"az_layout_info_ptr_fmt_debug")?);
            let az_css_rule_block_delete: extern "C" fn(_:  &mut AzCssRuleBlock) = transmute(lib.get(b"az_css_rule_block_delete")?);
            let az_css_rule_block_deep_copy: extern "C" fn(_:  &AzCssRuleBlock) -> AzCssRuleBlock = transmute(lib.get(b"az_css_rule_block_deep_copy")?);
            let az_css_rule_block_fmt_debug: extern "C" fn(_:  &AzCssRuleBlock) -> AzString = transmute(lib.get(b"az_css_rule_block_fmt_debug")?);
            let az_css_declaration_delete: extern "C" fn(_:  &mut AzCssDeclaration) = transmute(lib.get(b"az_css_declaration_delete")?);
            let az_css_declaration_deep_copy: extern "C" fn(_:  &AzCssDeclaration) -> AzCssDeclaration = transmute(lib.get(b"az_css_declaration_deep_copy")?);
            let az_css_declaration_fmt_debug: extern "C" fn(_:  &AzCssDeclaration) -> AzString = transmute(lib.get(b"az_css_declaration_fmt_debug")?);
            let az_dynamic_css_property_delete: extern "C" fn(_:  &mut AzDynamicCssProperty) = transmute(lib.get(b"az_dynamic_css_property_delete")?);
            let az_dynamic_css_property_deep_copy: extern "C" fn(_:  &AzDynamicCssProperty) -> AzDynamicCssProperty = transmute(lib.get(b"az_dynamic_css_property_deep_copy")?);
            let az_dynamic_css_property_fmt_debug: extern "C" fn(_:  &AzDynamicCssProperty) -> AzString = transmute(lib.get(b"az_dynamic_css_property_fmt_debug")?);
            let az_css_path_delete: extern "C" fn(_:  &mut AzCssPath) = transmute(lib.get(b"az_css_path_delete")?);
            let az_css_path_deep_copy: extern "C" fn(_:  &AzCssPath) -> AzCssPath = transmute(lib.get(b"az_css_path_deep_copy")?);
            let az_css_path_fmt_debug: extern "C" fn(_:  &AzCssPath) -> AzString = transmute(lib.get(b"az_css_path_fmt_debug")?);
            let az_css_path_selector_delete: extern "C" fn(_:  &mut AzCssPathSelector) = transmute(lib.get(b"az_css_path_selector_delete")?);
            let az_css_path_selector_deep_copy: extern "C" fn(_:  &AzCssPathSelector) -> AzCssPathSelector = transmute(lib.get(b"az_css_path_selector_deep_copy")?);
            let az_css_path_selector_fmt_debug: extern "C" fn(_:  &AzCssPathSelector) -> AzString = transmute(lib.get(b"az_css_path_selector_fmt_debug")?);
            let az_node_type_path_delete: extern "C" fn(_:  &mut AzNodeTypePath) = transmute(lib.get(b"az_node_type_path_delete")?);
            let az_node_type_path_deep_copy: extern "C" fn(_:  &AzNodeTypePath) -> AzNodeTypePath = transmute(lib.get(b"az_node_type_path_deep_copy")?);
            let az_node_type_path_fmt_debug: extern "C" fn(_:  &AzNodeTypePath) -> AzString = transmute(lib.get(b"az_node_type_path_fmt_debug")?);
            let az_css_path_pseudo_selector_delete: extern "C" fn(_:  &mut AzCssPathPseudoSelector) = transmute(lib.get(b"az_css_path_pseudo_selector_delete")?);
            let az_css_path_pseudo_selector_deep_copy: extern "C" fn(_:  &AzCssPathPseudoSelector) -> AzCssPathPseudoSelector = transmute(lib.get(b"az_css_path_pseudo_selector_deep_copy")?);
            let az_css_path_pseudo_selector_fmt_debug: extern "C" fn(_:  &AzCssPathPseudoSelector) -> AzString = transmute(lib.get(b"az_css_path_pseudo_selector_fmt_debug")?);
            let az_css_nth_child_selector_delete: extern "C" fn(_:  &mut AzCssNthChildSelector) = transmute(lib.get(b"az_css_nth_child_selector_delete")?);
            let az_css_nth_child_selector_deep_copy: extern "C" fn(_:  &AzCssNthChildSelector) -> AzCssNthChildSelector = transmute(lib.get(b"az_css_nth_child_selector_deep_copy")?);
            let az_css_nth_child_selector_fmt_debug: extern "C" fn(_:  &AzCssNthChildSelector) -> AzString = transmute(lib.get(b"az_css_nth_child_selector_fmt_debug")?);
            let az_css_nth_child_pattern_delete: extern "C" fn(_:  &mut AzCssNthChildPattern) = transmute(lib.get(b"az_css_nth_child_pattern_delete")?);
            let az_css_nth_child_pattern_deep_copy: extern "C" fn(_:  &AzCssNthChildPattern) -> AzCssNthChildPattern = transmute(lib.get(b"az_css_nth_child_pattern_deep_copy")?);
            let az_css_nth_child_pattern_fmt_debug: extern "C" fn(_:  &AzCssNthChildPattern) -> AzString = transmute(lib.get(b"az_css_nth_child_pattern_fmt_debug")?);
            let az_stylesheet_delete: extern "C" fn(_:  &mut AzStylesheet) = transmute(lib.get(b"az_stylesheet_delete")?);
            let az_stylesheet_deep_copy: extern "C" fn(_:  &AzStylesheet) -> AzStylesheet = transmute(lib.get(b"az_stylesheet_deep_copy")?);
            let az_stylesheet_fmt_debug: extern "C" fn(_:  &AzStylesheet) -> AzString = transmute(lib.get(b"az_stylesheet_fmt_debug")?);
            let az_css_native: extern "C" fn() -> AzCss = transmute(lib.get(b"az_css_native")?);
            let az_css_empty: extern "C" fn() -> AzCss = transmute(lib.get(b"az_css_empty")?);
            let az_css_from_string: extern "C" fn(_:  AzString) -> AzCss = transmute(lib.get(b"az_css_from_string")?);
            let az_css_override_native: extern "C" fn(_:  AzString) -> AzCss = transmute(lib.get(b"az_css_override_native")?);
            let az_css_delete: extern "C" fn(_:  &mut AzCss) = transmute(lib.get(b"az_css_delete")?);
            let az_css_deep_copy: extern "C" fn(_:  &AzCss) -> AzCss = transmute(lib.get(b"az_css_deep_copy")?);
            let az_css_fmt_debug: extern "C" fn(_:  &AzCss) -> AzString = transmute(lib.get(b"az_css_fmt_debug")?);
            let az_color_u_delete: extern "C" fn(_:  &mut AzColorU) = transmute(lib.get(b"az_color_u_delete")?);
            let az_color_u_deep_copy: extern "C" fn(_:  &AzColorU) -> AzColorU = transmute(lib.get(b"az_color_u_deep_copy")?);
            let az_color_u_fmt_debug: extern "C" fn(_:  &AzColorU) -> AzString = transmute(lib.get(b"az_color_u_fmt_debug")?);
            let az_size_metric_delete: extern "C" fn(_:  &mut AzSizeMetric) = transmute(lib.get(b"az_size_metric_delete")?);
            let az_size_metric_deep_copy: extern "C" fn(_:  &AzSizeMetric) -> AzSizeMetric = transmute(lib.get(b"az_size_metric_deep_copy")?);
            let az_size_metric_fmt_debug: extern "C" fn(_:  &AzSizeMetric) -> AzString = transmute(lib.get(b"az_size_metric_fmt_debug")?);
            let az_float_value_delete: extern "C" fn(_:  &mut AzFloatValue) = transmute(lib.get(b"az_float_value_delete")?);
            let az_float_value_deep_copy: extern "C" fn(_:  &AzFloatValue) -> AzFloatValue = transmute(lib.get(b"az_float_value_deep_copy")?);
            let az_float_value_fmt_debug: extern "C" fn(_:  &AzFloatValue) -> AzString = transmute(lib.get(b"az_float_value_fmt_debug")?);
            let az_pixel_value_delete: extern "C" fn(_:  &mut AzPixelValue) = transmute(lib.get(b"az_pixel_value_delete")?);
            let az_pixel_value_deep_copy: extern "C" fn(_:  &AzPixelValue) -> AzPixelValue = transmute(lib.get(b"az_pixel_value_deep_copy")?);
            let az_pixel_value_fmt_debug: extern "C" fn(_:  &AzPixelValue) -> AzString = transmute(lib.get(b"az_pixel_value_fmt_debug")?);
            let az_pixel_value_no_percent_delete: extern "C" fn(_:  &mut AzPixelValueNoPercent) = transmute(lib.get(b"az_pixel_value_no_percent_delete")?);
            let az_pixel_value_no_percent_deep_copy: extern "C" fn(_:  &AzPixelValueNoPercent) -> AzPixelValueNoPercent = transmute(lib.get(b"az_pixel_value_no_percent_deep_copy")?);
            let az_pixel_value_no_percent_fmt_debug: extern "C" fn(_:  &AzPixelValueNoPercent) -> AzString = transmute(lib.get(b"az_pixel_value_no_percent_fmt_debug")?);
            let az_box_shadow_clip_mode_delete: extern "C" fn(_:  &mut AzBoxShadowClipMode) = transmute(lib.get(b"az_box_shadow_clip_mode_delete")?);
            let az_box_shadow_clip_mode_deep_copy: extern "C" fn(_:  &AzBoxShadowClipMode) -> AzBoxShadowClipMode = transmute(lib.get(b"az_box_shadow_clip_mode_deep_copy")?);
            let az_box_shadow_clip_mode_fmt_debug: extern "C" fn(_:  &AzBoxShadowClipMode) -> AzString = transmute(lib.get(b"az_box_shadow_clip_mode_fmt_debug")?);
            let az_box_shadow_pre_display_item_delete: extern "C" fn(_:  &mut AzBoxShadowPreDisplayItem) = transmute(lib.get(b"az_box_shadow_pre_display_item_delete")?);
            let az_box_shadow_pre_display_item_deep_copy: extern "C" fn(_:  &AzBoxShadowPreDisplayItem) -> AzBoxShadowPreDisplayItem = transmute(lib.get(b"az_box_shadow_pre_display_item_deep_copy")?);
            let az_box_shadow_pre_display_item_fmt_debug: extern "C" fn(_:  &AzBoxShadowPreDisplayItem) -> AzString = transmute(lib.get(b"az_box_shadow_pre_display_item_fmt_debug")?);
            let az_layout_align_content_delete: extern "C" fn(_:  &mut AzLayoutAlignContent) = transmute(lib.get(b"az_layout_align_content_delete")?);
            let az_layout_align_content_deep_copy: extern "C" fn(_:  &AzLayoutAlignContent) -> AzLayoutAlignContent = transmute(lib.get(b"az_layout_align_content_deep_copy")?);
            let az_layout_align_content_fmt_debug: extern "C" fn(_:  &AzLayoutAlignContent) -> AzString = transmute(lib.get(b"az_layout_align_content_fmt_debug")?);
            let az_layout_align_items_delete: extern "C" fn(_:  &mut AzLayoutAlignItems) = transmute(lib.get(b"az_layout_align_items_delete")?);
            let az_layout_align_items_deep_copy: extern "C" fn(_:  &AzLayoutAlignItems) -> AzLayoutAlignItems = transmute(lib.get(b"az_layout_align_items_deep_copy")?);
            let az_layout_align_items_fmt_debug: extern "C" fn(_:  &AzLayoutAlignItems) -> AzString = transmute(lib.get(b"az_layout_align_items_fmt_debug")?);
            let az_layout_bottom_delete: extern "C" fn(_:  &mut AzLayoutBottom) = transmute(lib.get(b"az_layout_bottom_delete")?);
            let az_layout_bottom_deep_copy: extern "C" fn(_:  &AzLayoutBottom) -> AzLayoutBottom = transmute(lib.get(b"az_layout_bottom_deep_copy")?);
            let az_layout_bottom_fmt_debug: extern "C" fn(_:  &AzLayoutBottom) -> AzString = transmute(lib.get(b"az_layout_bottom_fmt_debug")?);
            let az_layout_box_sizing_delete: extern "C" fn(_:  &mut AzLayoutBoxSizing) = transmute(lib.get(b"az_layout_box_sizing_delete")?);
            let az_layout_box_sizing_deep_copy: extern "C" fn(_:  &AzLayoutBoxSizing) -> AzLayoutBoxSizing = transmute(lib.get(b"az_layout_box_sizing_deep_copy")?);
            let az_layout_box_sizing_fmt_debug: extern "C" fn(_:  &AzLayoutBoxSizing) -> AzString = transmute(lib.get(b"az_layout_box_sizing_fmt_debug")?);
            let az_layout_direction_delete: extern "C" fn(_:  &mut AzLayoutDirection) = transmute(lib.get(b"az_layout_direction_delete")?);
            let az_layout_direction_deep_copy: extern "C" fn(_:  &AzLayoutDirection) -> AzLayoutDirection = transmute(lib.get(b"az_layout_direction_deep_copy")?);
            let az_layout_direction_fmt_debug: extern "C" fn(_:  &AzLayoutDirection) -> AzString = transmute(lib.get(b"az_layout_direction_fmt_debug")?);
            let az_layout_display_delete: extern "C" fn(_:  &mut AzLayoutDisplay) = transmute(lib.get(b"az_layout_display_delete")?);
            let az_layout_display_deep_copy: extern "C" fn(_:  &AzLayoutDisplay) -> AzLayoutDisplay = transmute(lib.get(b"az_layout_display_deep_copy")?);
            let az_layout_display_fmt_debug: extern "C" fn(_:  &AzLayoutDisplay) -> AzString = transmute(lib.get(b"az_layout_display_fmt_debug")?);
            let az_layout_flex_grow_delete: extern "C" fn(_:  &mut AzLayoutFlexGrow) = transmute(lib.get(b"az_layout_flex_grow_delete")?);
            let az_layout_flex_grow_deep_copy: extern "C" fn(_:  &AzLayoutFlexGrow) -> AzLayoutFlexGrow = transmute(lib.get(b"az_layout_flex_grow_deep_copy")?);
            let az_layout_flex_grow_fmt_debug: extern "C" fn(_:  &AzLayoutFlexGrow) -> AzString = transmute(lib.get(b"az_layout_flex_grow_fmt_debug")?);
            let az_layout_flex_shrink_delete: extern "C" fn(_:  &mut AzLayoutFlexShrink) = transmute(lib.get(b"az_layout_flex_shrink_delete")?);
            let az_layout_flex_shrink_deep_copy: extern "C" fn(_:  &AzLayoutFlexShrink) -> AzLayoutFlexShrink = transmute(lib.get(b"az_layout_flex_shrink_deep_copy")?);
            let az_layout_flex_shrink_fmt_debug: extern "C" fn(_:  &AzLayoutFlexShrink) -> AzString = transmute(lib.get(b"az_layout_flex_shrink_fmt_debug")?);
            let az_layout_float_delete: extern "C" fn(_:  &mut AzLayoutFloat) = transmute(lib.get(b"az_layout_float_delete")?);
            let az_layout_float_deep_copy: extern "C" fn(_:  &AzLayoutFloat) -> AzLayoutFloat = transmute(lib.get(b"az_layout_float_deep_copy")?);
            let az_layout_float_fmt_debug: extern "C" fn(_:  &AzLayoutFloat) -> AzString = transmute(lib.get(b"az_layout_float_fmt_debug")?);
            let az_layout_height_delete: extern "C" fn(_:  &mut AzLayoutHeight) = transmute(lib.get(b"az_layout_height_delete")?);
            let az_layout_height_deep_copy: extern "C" fn(_:  &AzLayoutHeight) -> AzLayoutHeight = transmute(lib.get(b"az_layout_height_deep_copy")?);
            let az_layout_height_fmt_debug: extern "C" fn(_:  &AzLayoutHeight) -> AzString = transmute(lib.get(b"az_layout_height_fmt_debug")?);
            let az_layout_justify_content_delete: extern "C" fn(_:  &mut AzLayoutJustifyContent) = transmute(lib.get(b"az_layout_justify_content_delete")?);
            let az_layout_justify_content_deep_copy: extern "C" fn(_:  &AzLayoutJustifyContent) -> AzLayoutJustifyContent = transmute(lib.get(b"az_layout_justify_content_deep_copy")?);
            let az_layout_justify_content_fmt_debug: extern "C" fn(_:  &AzLayoutJustifyContent) -> AzString = transmute(lib.get(b"az_layout_justify_content_fmt_debug")?);
            let az_layout_left_delete: extern "C" fn(_:  &mut AzLayoutLeft) = transmute(lib.get(b"az_layout_left_delete")?);
            let az_layout_left_deep_copy: extern "C" fn(_:  &AzLayoutLeft) -> AzLayoutLeft = transmute(lib.get(b"az_layout_left_deep_copy")?);
            let az_layout_left_fmt_debug: extern "C" fn(_:  &AzLayoutLeft) -> AzString = transmute(lib.get(b"az_layout_left_fmt_debug")?);
            let az_layout_margin_bottom_delete: extern "C" fn(_:  &mut AzLayoutMarginBottom) = transmute(lib.get(b"az_layout_margin_bottom_delete")?);
            let az_layout_margin_bottom_deep_copy: extern "C" fn(_:  &AzLayoutMarginBottom) -> AzLayoutMarginBottom = transmute(lib.get(b"az_layout_margin_bottom_deep_copy")?);
            let az_layout_margin_bottom_fmt_debug: extern "C" fn(_:  &AzLayoutMarginBottom) -> AzString = transmute(lib.get(b"az_layout_margin_bottom_fmt_debug")?);
            let az_layout_margin_left_delete: extern "C" fn(_:  &mut AzLayoutMarginLeft) = transmute(lib.get(b"az_layout_margin_left_delete")?);
            let az_layout_margin_left_deep_copy: extern "C" fn(_:  &AzLayoutMarginLeft) -> AzLayoutMarginLeft = transmute(lib.get(b"az_layout_margin_left_deep_copy")?);
            let az_layout_margin_left_fmt_debug: extern "C" fn(_:  &AzLayoutMarginLeft) -> AzString = transmute(lib.get(b"az_layout_margin_left_fmt_debug")?);
            let az_layout_margin_right_delete: extern "C" fn(_:  &mut AzLayoutMarginRight) = transmute(lib.get(b"az_layout_margin_right_delete")?);
            let az_layout_margin_right_deep_copy: extern "C" fn(_:  &AzLayoutMarginRight) -> AzLayoutMarginRight = transmute(lib.get(b"az_layout_margin_right_deep_copy")?);
            let az_layout_margin_right_fmt_debug: extern "C" fn(_:  &AzLayoutMarginRight) -> AzString = transmute(lib.get(b"az_layout_margin_right_fmt_debug")?);
            let az_layout_margin_top_delete: extern "C" fn(_:  &mut AzLayoutMarginTop) = transmute(lib.get(b"az_layout_margin_top_delete")?);
            let az_layout_margin_top_deep_copy: extern "C" fn(_:  &AzLayoutMarginTop) -> AzLayoutMarginTop = transmute(lib.get(b"az_layout_margin_top_deep_copy")?);
            let az_layout_margin_top_fmt_debug: extern "C" fn(_:  &AzLayoutMarginTop) -> AzString = transmute(lib.get(b"az_layout_margin_top_fmt_debug")?);
            let az_layout_max_height_delete: extern "C" fn(_:  &mut AzLayoutMaxHeight) = transmute(lib.get(b"az_layout_max_height_delete")?);
            let az_layout_max_height_deep_copy: extern "C" fn(_:  &AzLayoutMaxHeight) -> AzLayoutMaxHeight = transmute(lib.get(b"az_layout_max_height_deep_copy")?);
            let az_layout_max_height_fmt_debug: extern "C" fn(_:  &AzLayoutMaxHeight) -> AzString = transmute(lib.get(b"az_layout_max_height_fmt_debug")?);
            let az_layout_max_width_delete: extern "C" fn(_:  &mut AzLayoutMaxWidth) = transmute(lib.get(b"az_layout_max_width_delete")?);
            let az_layout_max_width_deep_copy: extern "C" fn(_:  &AzLayoutMaxWidth) -> AzLayoutMaxWidth = transmute(lib.get(b"az_layout_max_width_deep_copy")?);
            let az_layout_max_width_fmt_debug: extern "C" fn(_:  &AzLayoutMaxWidth) -> AzString = transmute(lib.get(b"az_layout_max_width_fmt_debug")?);
            let az_layout_min_height_delete: extern "C" fn(_:  &mut AzLayoutMinHeight) = transmute(lib.get(b"az_layout_min_height_delete")?);
            let az_layout_min_height_deep_copy: extern "C" fn(_:  &AzLayoutMinHeight) -> AzLayoutMinHeight = transmute(lib.get(b"az_layout_min_height_deep_copy")?);
            let az_layout_min_height_fmt_debug: extern "C" fn(_:  &AzLayoutMinHeight) -> AzString = transmute(lib.get(b"az_layout_min_height_fmt_debug")?);
            let az_layout_min_width_delete: extern "C" fn(_:  &mut AzLayoutMinWidth) = transmute(lib.get(b"az_layout_min_width_delete")?);
            let az_layout_min_width_deep_copy: extern "C" fn(_:  &AzLayoutMinWidth) -> AzLayoutMinWidth = transmute(lib.get(b"az_layout_min_width_deep_copy")?);
            let az_layout_min_width_fmt_debug: extern "C" fn(_:  &AzLayoutMinWidth) -> AzString = transmute(lib.get(b"az_layout_min_width_fmt_debug")?);
            let az_layout_padding_bottom_delete: extern "C" fn(_:  &mut AzLayoutPaddingBottom) = transmute(lib.get(b"az_layout_padding_bottom_delete")?);
            let az_layout_padding_bottom_deep_copy: extern "C" fn(_:  &AzLayoutPaddingBottom) -> AzLayoutPaddingBottom = transmute(lib.get(b"az_layout_padding_bottom_deep_copy")?);
            let az_layout_padding_bottom_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingBottom) -> AzString = transmute(lib.get(b"az_layout_padding_bottom_fmt_debug")?);
            let az_layout_padding_left_delete: extern "C" fn(_:  &mut AzLayoutPaddingLeft) = transmute(lib.get(b"az_layout_padding_left_delete")?);
            let az_layout_padding_left_deep_copy: extern "C" fn(_:  &AzLayoutPaddingLeft) -> AzLayoutPaddingLeft = transmute(lib.get(b"az_layout_padding_left_deep_copy")?);
            let az_layout_padding_left_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingLeft) -> AzString = transmute(lib.get(b"az_layout_padding_left_fmt_debug")?);
            let az_layout_padding_right_delete: extern "C" fn(_:  &mut AzLayoutPaddingRight) = transmute(lib.get(b"az_layout_padding_right_delete")?);
            let az_layout_padding_right_deep_copy: extern "C" fn(_:  &AzLayoutPaddingRight) -> AzLayoutPaddingRight = transmute(lib.get(b"az_layout_padding_right_deep_copy")?);
            let az_layout_padding_right_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingRight) -> AzString = transmute(lib.get(b"az_layout_padding_right_fmt_debug")?);
            let az_layout_padding_top_delete: extern "C" fn(_:  &mut AzLayoutPaddingTop) = transmute(lib.get(b"az_layout_padding_top_delete")?);
            let az_layout_padding_top_deep_copy: extern "C" fn(_:  &AzLayoutPaddingTop) -> AzLayoutPaddingTop = transmute(lib.get(b"az_layout_padding_top_deep_copy")?);
            let az_layout_padding_top_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingTop) -> AzString = transmute(lib.get(b"az_layout_padding_top_fmt_debug")?);
            let az_layout_position_delete: extern "C" fn(_:  &mut AzLayoutPosition) = transmute(lib.get(b"az_layout_position_delete")?);
            let az_layout_position_deep_copy: extern "C" fn(_:  &AzLayoutPosition) -> AzLayoutPosition = transmute(lib.get(b"az_layout_position_deep_copy")?);
            let az_layout_position_fmt_debug: extern "C" fn(_:  &AzLayoutPosition) -> AzString = transmute(lib.get(b"az_layout_position_fmt_debug")?);
            let az_layout_right_delete: extern "C" fn(_:  &mut AzLayoutRight) = transmute(lib.get(b"az_layout_right_delete")?);
            let az_layout_right_deep_copy: extern "C" fn(_:  &AzLayoutRight) -> AzLayoutRight = transmute(lib.get(b"az_layout_right_deep_copy")?);
            let az_layout_right_fmt_debug: extern "C" fn(_:  &AzLayoutRight) -> AzString = transmute(lib.get(b"az_layout_right_fmt_debug")?);
            let az_layout_top_delete: extern "C" fn(_:  &mut AzLayoutTop) = transmute(lib.get(b"az_layout_top_delete")?);
            let az_layout_top_deep_copy: extern "C" fn(_:  &AzLayoutTop) -> AzLayoutTop = transmute(lib.get(b"az_layout_top_deep_copy")?);
            let az_layout_top_fmt_debug: extern "C" fn(_:  &AzLayoutTop) -> AzString = transmute(lib.get(b"az_layout_top_fmt_debug")?);
            let az_layout_width_delete: extern "C" fn(_:  &mut AzLayoutWidth) = transmute(lib.get(b"az_layout_width_delete")?);
            let az_layout_width_deep_copy: extern "C" fn(_:  &AzLayoutWidth) -> AzLayoutWidth = transmute(lib.get(b"az_layout_width_deep_copy")?);
            let az_layout_width_fmt_debug: extern "C" fn(_:  &AzLayoutWidth) -> AzString = transmute(lib.get(b"az_layout_width_fmt_debug")?);
            let az_layout_wrap_delete: extern "C" fn(_:  &mut AzLayoutWrap) = transmute(lib.get(b"az_layout_wrap_delete")?);
            let az_layout_wrap_deep_copy: extern "C" fn(_:  &AzLayoutWrap) -> AzLayoutWrap = transmute(lib.get(b"az_layout_wrap_deep_copy")?);
            let az_layout_wrap_fmt_debug: extern "C" fn(_:  &AzLayoutWrap) -> AzString = transmute(lib.get(b"az_layout_wrap_fmt_debug")?);
            let az_overflow_delete: extern "C" fn(_:  &mut AzOverflow) = transmute(lib.get(b"az_overflow_delete")?);
            let az_overflow_deep_copy: extern "C" fn(_:  &AzOverflow) -> AzOverflow = transmute(lib.get(b"az_overflow_deep_copy")?);
            let az_overflow_fmt_debug: extern "C" fn(_:  &AzOverflow) -> AzString = transmute(lib.get(b"az_overflow_fmt_debug")?);
            let az_percentage_value_delete: extern "C" fn(_:  &mut AzPercentageValue) = transmute(lib.get(b"az_percentage_value_delete")?);
            let az_percentage_value_deep_copy: extern "C" fn(_:  &AzPercentageValue) -> AzPercentageValue = transmute(lib.get(b"az_percentage_value_deep_copy")?);
            let az_percentage_value_fmt_debug: extern "C" fn(_:  &AzPercentageValue) -> AzString = transmute(lib.get(b"az_percentage_value_fmt_debug")?);
            let az_gradient_stop_pre_delete: extern "C" fn(_:  &mut AzGradientStopPre) = transmute(lib.get(b"az_gradient_stop_pre_delete")?);
            let az_gradient_stop_pre_deep_copy: extern "C" fn(_:  &AzGradientStopPre) -> AzGradientStopPre = transmute(lib.get(b"az_gradient_stop_pre_deep_copy")?);
            let az_gradient_stop_pre_fmt_debug: extern "C" fn(_:  &AzGradientStopPre) -> AzString = transmute(lib.get(b"az_gradient_stop_pre_fmt_debug")?);
            let az_direction_corner_delete: extern "C" fn(_:  &mut AzDirectionCorner) = transmute(lib.get(b"az_direction_corner_delete")?);
            let az_direction_corner_deep_copy: extern "C" fn(_:  &AzDirectionCorner) -> AzDirectionCorner = transmute(lib.get(b"az_direction_corner_deep_copy")?);
            let az_direction_corner_fmt_debug: extern "C" fn(_:  &AzDirectionCorner) -> AzString = transmute(lib.get(b"az_direction_corner_fmt_debug")?);
            let az_direction_corners_delete: extern "C" fn(_:  &mut AzDirectionCorners) = transmute(lib.get(b"az_direction_corners_delete")?);
            let az_direction_corners_deep_copy: extern "C" fn(_:  &AzDirectionCorners) -> AzDirectionCorners = transmute(lib.get(b"az_direction_corners_deep_copy")?);
            let az_direction_corners_fmt_debug: extern "C" fn(_:  &AzDirectionCorners) -> AzString = transmute(lib.get(b"az_direction_corners_fmt_debug")?);
            let az_direction_delete: extern "C" fn(_:  &mut AzDirection) = transmute(lib.get(b"az_direction_delete")?);
            let az_direction_deep_copy: extern "C" fn(_:  &AzDirection) -> AzDirection = transmute(lib.get(b"az_direction_deep_copy")?);
            let az_direction_fmt_debug: extern "C" fn(_:  &AzDirection) -> AzString = transmute(lib.get(b"az_direction_fmt_debug")?);
            let az_extend_mode_delete: extern "C" fn(_:  &mut AzExtendMode) = transmute(lib.get(b"az_extend_mode_delete")?);
            let az_extend_mode_deep_copy: extern "C" fn(_:  &AzExtendMode) -> AzExtendMode = transmute(lib.get(b"az_extend_mode_deep_copy")?);
            let az_extend_mode_fmt_debug: extern "C" fn(_:  &AzExtendMode) -> AzString = transmute(lib.get(b"az_extend_mode_fmt_debug")?);
            let az_linear_gradient_delete: extern "C" fn(_:  &mut AzLinearGradient) = transmute(lib.get(b"az_linear_gradient_delete")?);
            let az_linear_gradient_deep_copy: extern "C" fn(_:  &AzLinearGradient) -> AzLinearGradient = transmute(lib.get(b"az_linear_gradient_deep_copy")?);
            let az_linear_gradient_fmt_debug: extern "C" fn(_:  &AzLinearGradient) -> AzString = transmute(lib.get(b"az_linear_gradient_fmt_debug")?);
            let az_shape_delete: extern "C" fn(_:  &mut AzShape) = transmute(lib.get(b"az_shape_delete")?);
            let az_shape_deep_copy: extern "C" fn(_:  &AzShape) -> AzShape = transmute(lib.get(b"az_shape_deep_copy")?);
            let az_shape_fmt_debug: extern "C" fn(_:  &AzShape) -> AzString = transmute(lib.get(b"az_shape_fmt_debug")?);
            let az_radial_gradient_delete: extern "C" fn(_:  &mut AzRadialGradient) = transmute(lib.get(b"az_radial_gradient_delete")?);
            let az_radial_gradient_deep_copy: extern "C" fn(_:  &AzRadialGradient) -> AzRadialGradient = transmute(lib.get(b"az_radial_gradient_deep_copy")?);
            let az_radial_gradient_fmt_debug: extern "C" fn(_:  &AzRadialGradient) -> AzString = transmute(lib.get(b"az_radial_gradient_fmt_debug")?);
            let az_css_image_id_delete: extern "C" fn(_:  &mut AzCssImageId) = transmute(lib.get(b"az_css_image_id_delete")?);
            let az_css_image_id_deep_copy: extern "C" fn(_:  &AzCssImageId) -> AzCssImageId = transmute(lib.get(b"az_css_image_id_deep_copy")?);
            let az_css_image_id_fmt_debug: extern "C" fn(_:  &AzCssImageId) -> AzString = transmute(lib.get(b"az_css_image_id_fmt_debug")?);
            let az_style_background_content_delete: extern "C" fn(_:  &mut AzStyleBackgroundContent) = transmute(lib.get(b"az_style_background_content_delete")?);
            let az_style_background_content_deep_copy: extern "C" fn(_:  &AzStyleBackgroundContent) -> AzStyleBackgroundContent = transmute(lib.get(b"az_style_background_content_deep_copy")?);
            let az_style_background_content_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundContent) -> AzString = transmute(lib.get(b"az_style_background_content_fmt_debug")?);
            let az_background_position_horizontal_delete: extern "C" fn(_:  &mut AzBackgroundPositionHorizontal) = transmute(lib.get(b"az_background_position_horizontal_delete")?);
            let az_background_position_horizontal_deep_copy: extern "C" fn(_:  &AzBackgroundPositionHorizontal) -> AzBackgroundPositionHorizontal = transmute(lib.get(b"az_background_position_horizontal_deep_copy")?);
            let az_background_position_horizontal_fmt_debug: extern "C" fn(_:  &AzBackgroundPositionHorizontal) -> AzString = transmute(lib.get(b"az_background_position_horizontal_fmt_debug")?);
            let az_background_position_vertical_delete: extern "C" fn(_:  &mut AzBackgroundPositionVertical) = transmute(lib.get(b"az_background_position_vertical_delete")?);
            let az_background_position_vertical_deep_copy: extern "C" fn(_:  &AzBackgroundPositionVertical) -> AzBackgroundPositionVertical = transmute(lib.get(b"az_background_position_vertical_deep_copy")?);
            let az_background_position_vertical_fmt_debug: extern "C" fn(_:  &AzBackgroundPositionVertical) -> AzString = transmute(lib.get(b"az_background_position_vertical_fmt_debug")?);
            let az_style_background_position_delete: extern "C" fn(_:  &mut AzStyleBackgroundPosition) = transmute(lib.get(b"az_style_background_position_delete")?);
            let az_style_background_position_deep_copy: extern "C" fn(_:  &AzStyleBackgroundPosition) -> AzStyleBackgroundPosition = transmute(lib.get(b"az_style_background_position_deep_copy")?);
            let az_style_background_position_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundPosition) -> AzString = transmute(lib.get(b"az_style_background_position_fmt_debug")?);
            let az_style_background_repeat_delete: extern "C" fn(_:  &mut AzStyleBackgroundRepeat) = transmute(lib.get(b"az_style_background_repeat_delete")?);
            let az_style_background_repeat_deep_copy: extern "C" fn(_:  &AzStyleBackgroundRepeat) -> AzStyleBackgroundRepeat = transmute(lib.get(b"az_style_background_repeat_deep_copy")?);
            let az_style_background_repeat_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundRepeat) -> AzString = transmute(lib.get(b"az_style_background_repeat_fmt_debug")?);
            let az_style_background_size_delete: extern "C" fn(_:  &mut AzStyleBackgroundSize) = transmute(lib.get(b"az_style_background_size_delete")?);
            let az_style_background_size_deep_copy: extern "C" fn(_:  &AzStyleBackgroundSize) -> AzStyleBackgroundSize = transmute(lib.get(b"az_style_background_size_deep_copy")?);
            let az_style_background_size_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundSize) -> AzString = transmute(lib.get(b"az_style_background_size_fmt_debug")?);
            let az_style_border_bottom_color_delete: extern "C" fn(_:  &mut AzStyleBorderBottomColor) = transmute(lib.get(b"az_style_border_bottom_color_delete")?);
            let az_style_border_bottom_color_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomColor) -> AzStyleBorderBottomColor = transmute(lib.get(b"az_style_border_bottom_color_deep_copy")?);
            let az_style_border_bottom_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomColor) -> AzString = transmute(lib.get(b"az_style_border_bottom_color_fmt_debug")?);
            let az_style_border_bottom_left_radius_delete: extern "C" fn(_:  &mut AzStyleBorderBottomLeftRadius) = transmute(lib.get(b"az_style_border_bottom_left_radius_delete")?);
            let az_style_border_bottom_left_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadius = transmute(lib.get(b"az_style_border_bottom_left_radius_deep_copy")?);
            let az_style_border_bottom_left_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomLeftRadius) -> AzString = transmute(lib.get(b"az_style_border_bottom_left_radius_fmt_debug")?);
            let az_style_border_bottom_right_radius_delete: extern "C" fn(_:  &mut AzStyleBorderBottomRightRadius) = transmute(lib.get(b"az_style_border_bottom_right_radius_delete")?);
            let az_style_border_bottom_right_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadius = transmute(lib.get(b"az_style_border_bottom_right_radius_deep_copy")?);
            let az_style_border_bottom_right_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomRightRadius) -> AzString = transmute(lib.get(b"az_style_border_bottom_right_radius_fmt_debug")?);
            let az_border_style_delete: extern "C" fn(_:  &mut AzBorderStyle) = transmute(lib.get(b"az_border_style_delete")?);
            let az_border_style_deep_copy: extern "C" fn(_:  &AzBorderStyle) -> AzBorderStyle = transmute(lib.get(b"az_border_style_deep_copy")?);
            let az_border_style_fmt_debug: extern "C" fn(_:  &AzBorderStyle) -> AzString = transmute(lib.get(b"az_border_style_fmt_debug")?);
            let az_style_border_bottom_style_delete: extern "C" fn(_:  &mut AzStyleBorderBottomStyle) = transmute(lib.get(b"az_style_border_bottom_style_delete")?);
            let az_style_border_bottom_style_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomStyle) -> AzStyleBorderBottomStyle = transmute(lib.get(b"az_style_border_bottom_style_deep_copy")?);
            let az_style_border_bottom_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomStyle) -> AzString = transmute(lib.get(b"az_style_border_bottom_style_fmt_debug")?);
            let az_style_border_bottom_width_delete: extern "C" fn(_:  &mut AzStyleBorderBottomWidth) = transmute(lib.get(b"az_style_border_bottom_width_delete")?);
            let az_style_border_bottom_width_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomWidth) -> AzStyleBorderBottomWidth = transmute(lib.get(b"az_style_border_bottom_width_deep_copy")?);
            let az_style_border_bottom_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomWidth) -> AzString = transmute(lib.get(b"az_style_border_bottom_width_fmt_debug")?);
            let az_style_border_left_color_delete: extern "C" fn(_:  &mut AzStyleBorderLeftColor) = transmute(lib.get(b"az_style_border_left_color_delete")?);
            let az_style_border_left_color_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftColor) -> AzStyleBorderLeftColor = transmute(lib.get(b"az_style_border_left_color_deep_copy")?);
            let az_style_border_left_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftColor) -> AzString = transmute(lib.get(b"az_style_border_left_color_fmt_debug")?);
            let az_style_border_left_style_delete: extern "C" fn(_:  &mut AzStyleBorderLeftStyle) = transmute(lib.get(b"az_style_border_left_style_delete")?);
            let az_style_border_left_style_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftStyle) -> AzStyleBorderLeftStyle = transmute(lib.get(b"az_style_border_left_style_deep_copy")?);
            let az_style_border_left_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftStyle) -> AzString = transmute(lib.get(b"az_style_border_left_style_fmt_debug")?);
            let az_style_border_left_width_delete: extern "C" fn(_:  &mut AzStyleBorderLeftWidth) = transmute(lib.get(b"az_style_border_left_width_delete")?);
            let az_style_border_left_width_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftWidth) -> AzStyleBorderLeftWidth = transmute(lib.get(b"az_style_border_left_width_deep_copy")?);
            let az_style_border_left_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftWidth) -> AzString = transmute(lib.get(b"az_style_border_left_width_fmt_debug")?);
            let az_style_border_right_color_delete: extern "C" fn(_:  &mut AzStyleBorderRightColor) = transmute(lib.get(b"az_style_border_right_color_delete")?);
            let az_style_border_right_color_deep_copy: extern "C" fn(_:  &AzStyleBorderRightColor) -> AzStyleBorderRightColor = transmute(lib.get(b"az_style_border_right_color_deep_copy")?);
            let az_style_border_right_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightColor) -> AzString = transmute(lib.get(b"az_style_border_right_color_fmt_debug")?);
            let az_style_border_right_style_delete: extern "C" fn(_:  &mut AzStyleBorderRightStyle) = transmute(lib.get(b"az_style_border_right_style_delete")?);
            let az_style_border_right_style_deep_copy: extern "C" fn(_:  &AzStyleBorderRightStyle) -> AzStyleBorderRightStyle = transmute(lib.get(b"az_style_border_right_style_deep_copy")?);
            let az_style_border_right_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightStyle) -> AzString = transmute(lib.get(b"az_style_border_right_style_fmt_debug")?);
            let az_style_border_right_width_delete: extern "C" fn(_:  &mut AzStyleBorderRightWidth) = transmute(lib.get(b"az_style_border_right_width_delete")?);
            let az_style_border_right_width_deep_copy: extern "C" fn(_:  &AzStyleBorderRightWidth) -> AzStyleBorderRightWidth = transmute(lib.get(b"az_style_border_right_width_deep_copy")?);
            let az_style_border_right_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightWidth) -> AzString = transmute(lib.get(b"az_style_border_right_width_fmt_debug")?);
            let az_style_border_top_color_delete: extern "C" fn(_:  &mut AzStyleBorderTopColor) = transmute(lib.get(b"az_style_border_top_color_delete")?);
            let az_style_border_top_color_deep_copy: extern "C" fn(_:  &AzStyleBorderTopColor) -> AzStyleBorderTopColor = transmute(lib.get(b"az_style_border_top_color_deep_copy")?);
            let az_style_border_top_color_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopColor) -> AzString = transmute(lib.get(b"az_style_border_top_color_fmt_debug")?);
            let az_style_border_top_left_radius_delete: extern "C" fn(_:  &mut AzStyleBorderTopLeftRadius) = transmute(lib.get(b"az_style_border_top_left_radius_delete")?);
            let az_style_border_top_left_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderTopLeftRadius) -> AzStyleBorderTopLeftRadius = transmute(lib.get(b"az_style_border_top_left_radius_deep_copy")?);
            let az_style_border_top_left_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopLeftRadius) -> AzString = transmute(lib.get(b"az_style_border_top_left_radius_fmt_debug")?);
            let az_style_border_top_right_radius_delete: extern "C" fn(_:  &mut AzStyleBorderTopRightRadius) = transmute(lib.get(b"az_style_border_top_right_radius_delete")?);
            let az_style_border_top_right_radius_deep_copy: extern "C" fn(_:  &AzStyleBorderTopRightRadius) -> AzStyleBorderTopRightRadius = transmute(lib.get(b"az_style_border_top_right_radius_deep_copy")?);
            let az_style_border_top_right_radius_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopRightRadius) -> AzString = transmute(lib.get(b"az_style_border_top_right_radius_fmt_debug")?);
            let az_style_border_top_style_delete: extern "C" fn(_:  &mut AzStyleBorderTopStyle) = transmute(lib.get(b"az_style_border_top_style_delete")?);
            let az_style_border_top_style_deep_copy: extern "C" fn(_:  &AzStyleBorderTopStyle) -> AzStyleBorderTopStyle = transmute(lib.get(b"az_style_border_top_style_deep_copy")?);
            let az_style_border_top_style_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopStyle) -> AzString = transmute(lib.get(b"az_style_border_top_style_fmt_debug")?);
            let az_style_border_top_width_delete: extern "C" fn(_:  &mut AzStyleBorderTopWidth) = transmute(lib.get(b"az_style_border_top_width_delete")?);
            let az_style_border_top_width_deep_copy: extern "C" fn(_:  &AzStyleBorderTopWidth) -> AzStyleBorderTopWidth = transmute(lib.get(b"az_style_border_top_width_deep_copy")?);
            let az_style_border_top_width_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopWidth) -> AzString = transmute(lib.get(b"az_style_border_top_width_fmt_debug")?);
            let az_style_cursor_delete: extern "C" fn(_:  &mut AzStyleCursor) = transmute(lib.get(b"az_style_cursor_delete")?);
            let az_style_cursor_deep_copy: extern "C" fn(_:  &AzStyleCursor) -> AzStyleCursor = transmute(lib.get(b"az_style_cursor_deep_copy")?);
            let az_style_cursor_fmt_debug: extern "C" fn(_:  &AzStyleCursor) -> AzString = transmute(lib.get(b"az_style_cursor_fmt_debug")?);
            let az_style_font_family_delete: extern "C" fn(_:  &mut AzStyleFontFamily) = transmute(lib.get(b"az_style_font_family_delete")?);
            let az_style_font_family_deep_copy: extern "C" fn(_:  &AzStyleFontFamily) -> AzStyleFontFamily = transmute(lib.get(b"az_style_font_family_deep_copy")?);
            let az_style_font_family_fmt_debug: extern "C" fn(_:  &AzStyleFontFamily) -> AzString = transmute(lib.get(b"az_style_font_family_fmt_debug")?);
            let az_style_font_size_delete: extern "C" fn(_:  &mut AzStyleFontSize) = transmute(lib.get(b"az_style_font_size_delete")?);
            let az_style_font_size_deep_copy: extern "C" fn(_:  &AzStyleFontSize) -> AzStyleFontSize = transmute(lib.get(b"az_style_font_size_deep_copy")?);
            let az_style_font_size_fmt_debug: extern "C" fn(_:  &AzStyleFontSize) -> AzString = transmute(lib.get(b"az_style_font_size_fmt_debug")?);
            let az_style_letter_spacing_delete: extern "C" fn(_:  &mut AzStyleLetterSpacing) = transmute(lib.get(b"az_style_letter_spacing_delete")?);
            let az_style_letter_spacing_deep_copy: extern "C" fn(_:  &AzStyleLetterSpacing) -> AzStyleLetterSpacing = transmute(lib.get(b"az_style_letter_spacing_deep_copy")?);
            let az_style_letter_spacing_fmt_debug: extern "C" fn(_:  &AzStyleLetterSpacing) -> AzString = transmute(lib.get(b"az_style_letter_spacing_fmt_debug")?);
            let az_style_line_height_delete: extern "C" fn(_:  &mut AzStyleLineHeight) = transmute(lib.get(b"az_style_line_height_delete")?);
            let az_style_line_height_deep_copy: extern "C" fn(_:  &AzStyleLineHeight) -> AzStyleLineHeight = transmute(lib.get(b"az_style_line_height_deep_copy")?);
            let az_style_line_height_fmt_debug: extern "C" fn(_:  &AzStyleLineHeight) -> AzString = transmute(lib.get(b"az_style_line_height_fmt_debug")?);
            let az_style_tab_width_delete: extern "C" fn(_:  &mut AzStyleTabWidth) = transmute(lib.get(b"az_style_tab_width_delete")?);
            let az_style_tab_width_deep_copy: extern "C" fn(_:  &AzStyleTabWidth) -> AzStyleTabWidth = transmute(lib.get(b"az_style_tab_width_deep_copy")?);
            let az_style_tab_width_fmt_debug: extern "C" fn(_:  &AzStyleTabWidth) -> AzString = transmute(lib.get(b"az_style_tab_width_fmt_debug")?);
            let az_style_text_alignment_horz_delete: extern "C" fn(_:  &mut AzStyleTextAlignmentHorz) = transmute(lib.get(b"az_style_text_alignment_horz_delete")?);
            let az_style_text_alignment_horz_deep_copy: extern "C" fn(_:  &AzStyleTextAlignmentHorz) -> AzStyleTextAlignmentHorz = transmute(lib.get(b"az_style_text_alignment_horz_deep_copy")?);
            let az_style_text_alignment_horz_fmt_debug: extern "C" fn(_:  &AzStyleTextAlignmentHorz) -> AzString = transmute(lib.get(b"az_style_text_alignment_horz_fmt_debug")?);
            let az_style_text_color_delete: extern "C" fn(_:  &mut AzStyleTextColor) = transmute(lib.get(b"az_style_text_color_delete")?);
            let az_style_text_color_deep_copy: extern "C" fn(_:  &AzStyleTextColor) -> AzStyleTextColor = transmute(lib.get(b"az_style_text_color_deep_copy")?);
            let az_style_text_color_fmt_debug: extern "C" fn(_:  &AzStyleTextColor) -> AzString = transmute(lib.get(b"az_style_text_color_fmt_debug")?);
            let az_style_word_spacing_delete: extern "C" fn(_:  &mut AzStyleWordSpacing) = transmute(lib.get(b"az_style_word_spacing_delete")?);
            let az_style_word_spacing_deep_copy: extern "C" fn(_:  &AzStyleWordSpacing) -> AzStyleWordSpacing = transmute(lib.get(b"az_style_word_spacing_deep_copy")?);
            let az_style_word_spacing_fmt_debug: extern "C" fn(_:  &AzStyleWordSpacing) -> AzString = transmute(lib.get(b"az_style_word_spacing_fmt_debug")?);
            let az_box_shadow_pre_display_item_value_delete: extern "C" fn(_:  &mut AzBoxShadowPreDisplayItemValue) = transmute(lib.get(b"az_box_shadow_pre_display_item_value_delete")?);
            let az_box_shadow_pre_display_item_value_deep_copy: extern "C" fn(_:  &AzBoxShadowPreDisplayItemValue) -> AzBoxShadowPreDisplayItemValue = transmute(lib.get(b"az_box_shadow_pre_display_item_value_deep_copy")?);
            let az_box_shadow_pre_display_item_value_fmt_debug: extern "C" fn(_:  &AzBoxShadowPreDisplayItemValue) -> AzString = transmute(lib.get(b"az_box_shadow_pre_display_item_value_fmt_debug")?);
            let az_layout_align_content_value_delete: extern "C" fn(_:  &mut AzLayoutAlignContentValue) = transmute(lib.get(b"az_layout_align_content_value_delete")?);
            let az_layout_align_content_value_deep_copy: extern "C" fn(_:  &AzLayoutAlignContentValue) -> AzLayoutAlignContentValue = transmute(lib.get(b"az_layout_align_content_value_deep_copy")?);
            let az_layout_align_content_value_fmt_debug: extern "C" fn(_:  &AzLayoutAlignContentValue) -> AzString = transmute(lib.get(b"az_layout_align_content_value_fmt_debug")?);
            let az_layout_align_items_value_delete: extern "C" fn(_:  &mut AzLayoutAlignItemsValue) = transmute(lib.get(b"az_layout_align_items_value_delete")?);
            let az_layout_align_items_value_deep_copy: extern "C" fn(_:  &AzLayoutAlignItemsValue) -> AzLayoutAlignItemsValue = transmute(lib.get(b"az_layout_align_items_value_deep_copy")?);
            let az_layout_align_items_value_fmt_debug: extern "C" fn(_:  &AzLayoutAlignItemsValue) -> AzString = transmute(lib.get(b"az_layout_align_items_value_fmt_debug")?);
            let az_layout_bottom_value_delete: extern "C" fn(_:  &mut AzLayoutBottomValue) = transmute(lib.get(b"az_layout_bottom_value_delete")?);
            let az_layout_bottom_value_deep_copy: extern "C" fn(_:  &AzLayoutBottomValue) -> AzLayoutBottomValue = transmute(lib.get(b"az_layout_bottom_value_deep_copy")?);
            let az_layout_bottom_value_fmt_debug: extern "C" fn(_:  &AzLayoutBottomValue) -> AzString = transmute(lib.get(b"az_layout_bottom_value_fmt_debug")?);
            let az_layout_box_sizing_value_delete: extern "C" fn(_:  &mut AzLayoutBoxSizingValue) = transmute(lib.get(b"az_layout_box_sizing_value_delete")?);
            let az_layout_box_sizing_value_deep_copy: extern "C" fn(_:  &AzLayoutBoxSizingValue) -> AzLayoutBoxSizingValue = transmute(lib.get(b"az_layout_box_sizing_value_deep_copy")?);
            let az_layout_box_sizing_value_fmt_debug: extern "C" fn(_:  &AzLayoutBoxSizingValue) -> AzString = transmute(lib.get(b"az_layout_box_sizing_value_fmt_debug")?);
            let az_layout_direction_value_delete: extern "C" fn(_:  &mut AzLayoutDirectionValue) = transmute(lib.get(b"az_layout_direction_value_delete")?);
            let az_layout_direction_value_deep_copy: extern "C" fn(_:  &AzLayoutDirectionValue) -> AzLayoutDirectionValue = transmute(lib.get(b"az_layout_direction_value_deep_copy")?);
            let az_layout_direction_value_fmt_debug: extern "C" fn(_:  &AzLayoutDirectionValue) -> AzString = transmute(lib.get(b"az_layout_direction_value_fmt_debug")?);
            let az_layout_display_value_delete: extern "C" fn(_:  &mut AzLayoutDisplayValue) = transmute(lib.get(b"az_layout_display_value_delete")?);
            let az_layout_display_value_deep_copy: extern "C" fn(_:  &AzLayoutDisplayValue) -> AzLayoutDisplayValue = transmute(lib.get(b"az_layout_display_value_deep_copy")?);
            let az_layout_display_value_fmt_debug: extern "C" fn(_:  &AzLayoutDisplayValue) -> AzString = transmute(lib.get(b"az_layout_display_value_fmt_debug")?);
            let az_layout_flex_grow_value_delete: extern "C" fn(_:  &mut AzLayoutFlexGrowValue) = transmute(lib.get(b"az_layout_flex_grow_value_delete")?);
            let az_layout_flex_grow_value_deep_copy: extern "C" fn(_:  &AzLayoutFlexGrowValue) -> AzLayoutFlexGrowValue = transmute(lib.get(b"az_layout_flex_grow_value_deep_copy")?);
            let az_layout_flex_grow_value_fmt_debug: extern "C" fn(_:  &AzLayoutFlexGrowValue) -> AzString = transmute(lib.get(b"az_layout_flex_grow_value_fmt_debug")?);
            let az_layout_flex_shrink_value_delete: extern "C" fn(_:  &mut AzLayoutFlexShrinkValue) = transmute(lib.get(b"az_layout_flex_shrink_value_delete")?);
            let az_layout_flex_shrink_value_deep_copy: extern "C" fn(_:  &AzLayoutFlexShrinkValue) -> AzLayoutFlexShrinkValue = transmute(lib.get(b"az_layout_flex_shrink_value_deep_copy")?);
            let az_layout_flex_shrink_value_fmt_debug: extern "C" fn(_:  &AzLayoutFlexShrinkValue) -> AzString = transmute(lib.get(b"az_layout_flex_shrink_value_fmt_debug")?);
            let az_layout_float_value_delete: extern "C" fn(_:  &mut AzLayoutFloatValue) = transmute(lib.get(b"az_layout_float_value_delete")?);
            let az_layout_float_value_deep_copy: extern "C" fn(_:  &AzLayoutFloatValue) -> AzLayoutFloatValue = transmute(lib.get(b"az_layout_float_value_deep_copy")?);
            let az_layout_float_value_fmt_debug: extern "C" fn(_:  &AzLayoutFloatValue) -> AzString = transmute(lib.get(b"az_layout_float_value_fmt_debug")?);
            let az_layout_height_value_delete: extern "C" fn(_:  &mut AzLayoutHeightValue) = transmute(lib.get(b"az_layout_height_value_delete")?);
            let az_layout_height_value_deep_copy: extern "C" fn(_:  &AzLayoutHeightValue) -> AzLayoutHeightValue = transmute(lib.get(b"az_layout_height_value_deep_copy")?);
            let az_layout_height_value_fmt_debug: extern "C" fn(_:  &AzLayoutHeightValue) -> AzString = transmute(lib.get(b"az_layout_height_value_fmt_debug")?);
            let az_layout_justify_content_value_delete: extern "C" fn(_:  &mut AzLayoutJustifyContentValue) = transmute(lib.get(b"az_layout_justify_content_value_delete")?);
            let az_layout_justify_content_value_deep_copy: extern "C" fn(_:  &AzLayoutJustifyContentValue) -> AzLayoutJustifyContentValue = transmute(lib.get(b"az_layout_justify_content_value_deep_copy")?);
            let az_layout_justify_content_value_fmt_debug: extern "C" fn(_:  &AzLayoutJustifyContentValue) -> AzString = transmute(lib.get(b"az_layout_justify_content_value_fmt_debug")?);
            let az_layout_left_value_delete: extern "C" fn(_:  &mut AzLayoutLeftValue) = transmute(lib.get(b"az_layout_left_value_delete")?);
            let az_layout_left_value_deep_copy: extern "C" fn(_:  &AzLayoutLeftValue) -> AzLayoutLeftValue = transmute(lib.get(b"az_layout_left_value_deep_copy")?);
            let az_layout_left_value_fmt_debug: extern "C" fn(_:  &AzLayoutLeftValue) -> AzString = transmute(lib.get(b"az_layout_left_value_fmt_debug")?);
            let az_layout_margin_bottom_value_delete: extern "C" fn(_:  &mut AzLayoutMarginBottomValue) = transmute(lib.get(b"az_layout_margin_bottom_value_delete")?);
            let az_layout_margin_bottom_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginBottomValue) -> AzLayoutMarginBottomValue = transmute(lib.get(b"az_layout_margin_bottom_value_deep_copy")?);
            let az_layout_margin_bottom_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginBottomValue) -> AzString = transmute(lib.get(b"az_layout_margin_bottom_value_fmt_debug")?);
            let az_layout_margin_left_value_delete: extern "C" fn(_:  &mut AzLayoutMarginLeftValue) = transmute(lib.get(b"az_layout_margin_left_value_delete")?);
            let az_layout_margin_left_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginLeftValue) -> AzLayoutMarginLeftValue = transmute(lib.get(b"az_layout_margin_left_value_deep_copy")?);
            let az_layout_margin_left_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginLeftValue) -> AzString = transmute(lib.get(b"az_layout_margin_left_value_fmt_debug")?);
            let az_layout_margin_right_value_delete: extern "C" fn(_:  &mut AzLayoutMarginRightValue) = transmute(lib.get(b"az_layout_margin_right_value_delete")?);
            let az_layout_margin_right_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginRightValue) -> AzLayoutMarginRightValue = transmute(lib.get(b"az_layout_margin_right_value_deep_copy")?);
            let az_layout_margin_right_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginRightValue) -> AzString = transmute(lib.get(b"az_layout_margin_right_value_fmt_debug")?);
            let az_layout_margin_top_value_delete: extern "C" fn(_:  &mut AzLayoutMarginTopValue) = transmute(lib.get(b"az_layout_margin_top_value_delete")?);
            let az_layout_margin_top_value_deep_copy: extern "C" fn(_:  &AzLayoutMarginTopValue) -> AzLayoutMarginTopValue = transmute(lib.get(b"az_layout_margin_top_value_deep_copy")?);
            let az_layout_margin_top_value_fmt_debug: extern "C" fn(_:  &AzLayoutMarginTopValue) -> AzString = transmute(lib.get(b"az_layout_margin_top_value_fmt_debug")?);
            let az_layout_max_height_value_delete: extern "C" fn(_:  &mut AzLayoutMaxHeightValue) = transmute(lib.get(b"az_layout_max_height_value_delete")?);
            let az_layout_max_height_value_deep_copy: extern "C" fn(_:  &AzLayoutMaxHeightValue) -> AzLayoutMaxHeightValue = transmute(lib.get(b"az_layout_max_height_value_deep_copy")?);
            let az_layout_max_height_value_fmt_debug: extern "C" fn(_:  &AzLayoutMaxHeightValue) -> AzString = transmute(lib.get(b"az_layout_max_height_value_fmt_debug")?);
            let az_layout_max_width_value_delete: extern "C" fn(_:  &mut AzLayoutMaxWidthValue) = transmute(lib.get(b"az_layout_max_width_value_delete")?);
            let az_layout_max_width_value_deep_copy: extern "C" fn(_:  &AzLayoutMaxWidthValue) -> AzLayoutMaxWidthValue = transmute(lib.get(b"az_layout_max_width_value_deep_copy")?);
            let az_layout_max_width_value_fmt_debug: extern "C" fn(_:  &AzLayoutMaxWidthValue) -> AzString = transmute(lib.get(b"az_layout_max_width_value_fmt_debug")?);
            let az_layout_min_height_value_delete: extern "C" fn(_:  &mut AzLayoutMinHeightValue) = transmute(lib.get(b"az_layout_min_height_value_delete")?);
            let az_layout_min_height_value_deep_copy: extern "C" fn(_:  &AzLayoutMinHeightValue) -> AzLayoutMinHeightValue = transmute(lib.get(b"az_layout_min_height_value_deep_copy")?);
            let az_layout_min_height_value_fmt_debug: extern "C" fn(_:  &AzLayoutMinHeightValue) -> AzString = transmute(lib.get(b"az_layout_min_height_value_fmt_debug")?);
            let az_layout_min_width_value_delete: extern "C" fn(_:  &mut AzLayoutMinWidthValue) = transmute(lib.get(b"az_layout_min_width_value_delete")?);
            let az_layout_min_width_value_deep_copy: extern "C" fn(_:  &AzLayoutMinWidthValue) -> AzLayoutMinWidthValue = transmute(lib.get(b"az_layout_min_width_value_deep_copy")?);
            let az_layout_min_width_value_fmt_debug: extern "C" fn(_:  &AzLayoutMinWidthValue) -> AzString = transmute(lib.get(b"az_layout_min_width_value_fmt_debug")?);
            let az_layout_padding_bottom_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingBottomValue) = transmute(lib.get(b"az_layout_padding_bottom_value_delete")?);
            let az_layout_padding_bottom_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingBottomValue) -> AzLayoutPaddingBottomValue = transmute(lib.get(b"az_layout_padding_bottom_value_deep_copy")?);
            let az_layout_padding_bottom_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingBottomValue) -> AzString = transmute(lib.get(b"az_layout_padding_bottom_value_fmt_debug")?);
            let az_layout_padding_left_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingLeftValue) = transmute(lib.get(b"az_layout_padding_left_value_delete")?);
            let az_layout_padding_left_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingLeftValue) -> AzLayoutPaddingLeftValue = transmute(lib.get(b"az_layout_padding_left_value_deep_copy")?);
            let az_layout_padding_left_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingLeftValue) -> AzString = transmute(lib.get(b"az_layout_padding_left_value_fmt_debug")?);
            let az_layout_padding_right_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingRightValue) = transmute(lib.get(b"az_layout_padding_right_value_delete")?);
            let az_layout_padding_right_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingRightValue) -> AzLayoutPaddingRightValue = transmute(lib.get(b"az_layout_padding_right_value_deep_copy")?);
            let az_layout_padding_right_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingRightValue) -> AzString = transmute(lib.get(b"az_layout_padding_right_value_fmt_debug")?);
            let az_layout_padding_top_value_delete: extern "C" fn(_:  &mut AzLayoutPaddingTopValue) = transmute(lib.get(b"az_layout_padding_top_value_delete")?);
            let az_layout_padding_top_value_deep_copy: extern "C" fn(_:  &AzLayoutPaddingTopValue) -> AzLayoutPaddingTopValue = transmute(lib.get(b"az_layout_padding_top_value_deep_copy")?);
            let az_layout_padding_top_value_fmt_debug: extern "C" fn(_:  &AzLayoutPaddingTopValue) -> AzString = transmute(lib.get(b"az_layout_padding_top_value_fmt_debug")?);
            let az_layout_position_value_delete: extern "C" fn(_:  &mut AzLayoutPositionValue) = transmute(lib.get(b"az_layout_position_value_delete")?);
            let az_layout_position_value_deep_copy: extern "C" fn(_:  &AzLayoutPositionValue) -> AzLayoutPositionValue = transmute(lib.get(b"az_layout_position_value_deep_copy")?);
            let az_layout_position_value_fmt_debug: extern "C" fn(_:  &AzLayoutPositionValue) -> AzString = transmute(lib.get(b"az_layout_position_value_fmt_debug")?);
            let az_layout_right_value_delete: extern "C" fn(_:  &mut AzLayoutRightValue) = transmute(lib.get(b"az_layout_right_value_delete")?);
            let az_layout_right_value_deep_copy: extern "C" fn(_:  &AzLayoutRightValue) -> AzLayoutRightValue = transmute(lib.get(b"az_layout_right_value_deep_copy")?);
            let az_layout_right_value_fmt_debug: extern "C" fn(_:  &AzLayoutRightValue) -> AzString = transmute(lib.get(b"az_layout_right_value_fmt_debug")?);
            let az_layout_top_value_delete: extern "C" fn(_:  &mut AzLayoutTopValue) = transmute(lib.get(b"az_layout_top_value_delete")?);
            let az_layout_top_value_deep_copy: extern "C" fn(_:  &AzLayoutTopValue) -> AzLayoutTopValue = transmute(lib.get(b"az_layout_top_value_deep_copy")?);
            let az_layout_top_value_fmt_debug: extern "C" fn(_:  &AzLayoutTopValue) -> AzString = transmute(lib.get(b"az_layout_top_value_fmt_debug")?);
            let az_layout_width_value_delete: extern "C" fn(_:  &mut AzLayoutWidthValue) = transmute(lib.get(b"az_layout_width_value_delete")?);
            let az_layout_width_value_deep_copy: extern "C" fn(_:  &AzLayoutWidthValue) -> AzLayoutWidthValue = transmute(lib.get(b"az_layout_width_value_deep_copy")?);
            let az_layout_width_value_fmt_debug: extern "C" fn(_:  &AzLayoutWidthValue) -> AzString = transmute(lib.get(b"az_layout_width_value_fmt_debug")?);
            let az_layout_wrap_value_delete: extern "C" fn(_:  &mut AzLayoutWrapValue) = transmute(lib.get(b"az_layout_wrap_value_delete")?);
            let az_layout_wrap_value_deep_copy: extern "C" fn(_:  &AzLayoutWrapValue) -> AzLayoutWrapValue = transmute(lib.get(b"az_layout_wrap_value_deep_copy")?);
            let az_layout_wrap_value_fmt_debug: extern "C" fn(_:  &AzLayoutWrapValue) -> AzString = transmute(lib.get(b"az_layout_wrap_value_fmt_debug")?);
            let az_overflow_value_delete: extern "C" fn(_:  &mut AzOverflowValue) = transmute(lib.get(b"az_overflow_value_delete")?);
            let az_overflow_value_deep_copy: extern "C" fn(_:  &AzOverflowValue) -> AzOverflowValue = transmute(lib.get(b"az_overflow_value_deep_copy")?);
            let az_overflow_value_fmt_debug: extern "C" fn(_:  &AzOverflowValue) -> AzString = transmute(lib.get(b"az_overflow_value_fmt_debug")?);
            let az_style_background_content_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundContentValue) = transmute(lib.get(b"az_style_background_content_value_delete")?);
            let az_style_background_content_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue = transmute(lib.get(b"az_style_background_content_value_deep_copy")?);
            let az_style_background_content_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundContentValue) -> AzString = transmute(lib.get(b"az_style_background_content_value_fmt_debug")?);
            let az_style_background_position_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundPositionValue) = transmute(lib.get(b"az_style_background_position_value_delete")?);
            let az_style_background_position_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundPositionValue) -> AzStyleBackgroundPositionValue = transmute(lib.get(b"az_style_background_position_value_deep_copy")?);
            let az_style_background_position_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundPositionValue) -> AzString = transmute(lib.get(b"az_style_background_position_value_fmt_debug")?);
            let az_style_background_repeat_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundRepeatValue) = transmute(lib.get(b"az_style_background_repeat_value_delete")?);
            let az_style_background_repeat_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundRepeatValue) -> AzStyleBackgroundRepeatValue = transmute(lib.get(b"az_style_background_repeat_value_deep_copy")?);
            let az_style_background_repeat_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundRepeatValue) -> AzString = transmute(lib.get(b"az_style_background_repeat_value_fmt_debug")?);
            let az_style_background_size_value_delete: extern "C" fn(_:  &mut AzStyleBackgroundSizeValue) = transmute(lib.get(b"az_style_background_size_value_delete")?);
            let az_style_background_size_value_deep_copy: extern "C" fn(_:  &AzStyleBackgroundSizeValue) -> AzStyleBackgroundSizeValue = transmute(lib.get(b"az_style_background_size_value_deep_copy")?);
            let az_style_background_size_value_fmt_debug: extern "C" fn(_:  &AzStyleBackgroundSizeValue) -> AzString = transmute(lib.get(b"az_style_background_size_value_fmt_debug")?);
            let az_style_border_bottom_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomColorValue) = transmute(lib.get(b"az_style_border_bottom_color_value_delete")?);
            let az_style_border_bottom_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomColorValue) -> AzStyleBorderBottomColorValue = transmute(lib.get(b"az_style_border_bottom_color_value_deep_copy")?);
            let az_style_border_bottom_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomColorValue) -> AzString = transmute(lib.get(b"az_style_border_bottom_color_value_fmt_debug")?);
            let az_style_border_bottom_left_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomLeftRadiusValue) = transmute(lib.get(b"az_style_border_bottom_left_radius_value_delete")?);
            let az_style_border_bottom_left_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomLeftRadiusValue) -> AzStyleBorderBottomLeftRadiusValue = transmute(lib.get(b"az_style_border_bottom_left_radius_value_deep_copy")?);
            let az_style_border_bottom_left_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomLeftRadiusValue) -> AzString = transmute(lib.get(b"az_style_border_bottom_left_radius_value_fmt_debug")?);
            let az_style_border_bottom_right_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomRightRadiusValue) = transmute(lib.get(b"az_style_border_bottom_right_radius_value_delete")?);
            let az_style_border_bottom_right_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomRightRadiusValue) -> AzStyleBorderBottomRightRadiusValue = transmute(lib.get(b"az_style_border_bottom_right_radius_value_deep_copy")?);
            let az_style_border_bottom_right_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomRightRadiusValue) -> AzString = transmute(lib.get(b"az_style_border_bottom_right_radius_value_fmt_debug")?);
            let az_style_border_bottom_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomStyleValue) = transmute(lib.get(b"az_style_border_bottom_style_value_delete")?);
            let az_style_border_bottom_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomStyleValue) -> AzStyleBorderBottomStyleValue = transmute(lib.get(b"az_style_border_bottom_style_value_deep_copy")?);
            let az_style_border_bottom_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomStyleValue) -> AzString = transmute(lib.get(b"az_style_border_bottom_style_value_fmt_debug")?);
            let az_style_border_bottom_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderBottomWidthValue) = transmute(lib.get(b"az_style_border_bottom_width_value_delete")?);
            let az_style_border_bottom_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderBottomWidthValue) -> AzStyleBorderBottomWidthValue = transmute(lib.get(b"az_style_border_bottom_width_value_deep_copy")?);
            let az_style_border_bottom_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderBottomWidthValue) -> AzString = transmute(lib.get(b"az_style_border_bottom_width_value_fmt_debug")?);
            let az_style_border_left_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderLeftColorValue) = transmute(lib.get(b"az_style_border_left_color_value_delete")?);
            let az_style_border_left_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftColorValue) -> AzStyleBorderLeftColorValue = transmute(lib.get(b"az_style_border_left_color_value_deep_copy")?);
            let az_style_border_left_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftColorValue) -> AzString = transmute(lib.get(b"az_style_border_left_color_value_fmt_debug")?);
            let az_style_border_left_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderLeftStyleValue) = transmute(lib.get(b"az_style_border_left_style_value_delete")?);
            let az_style_border_left_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftStyleValue) -> AzStyleBorderLeftStyleValue = transmute(lib.get(b"az_style_border_left_style_value_deep_copy")?);
            let az_style_border_left_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftStyleValue) -> AzString = transmute(lib.get(b"az_style_border_left_style_value_fmt_debug")?);
            let az_style_border_left_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderLeftWidthValue) = transmute(lib.get(b"az_style_border_left_width_value_delete")?);
            let az_style_border_left_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderLeftWidthValue) -> AzStyleBorderLeftWidthValue = transmute(lib.get(b"az_style_border_left_width_value_deep_copy")?);
            let az_style_border_left_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderLeftWidthValue) -> AzString = transmute(lib.get(b"az_style_border_left_width_value_fmt_debug")?);
            let az_style_border_right_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderRightColorValue) = transmute(lib.get(b"az_style_border_right_color_value_delete")?);
            let az_style_border_right_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderRightColorValue) -> AzStyleBorderRightColorValue = transmute(lib.get(b"az_style_border_right_color_value_deep_copy")?);
            let az_style_border_right_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightColorValue) -> AzString = transmute(lib.get(b"az_style_border_right_color_value_fmt_debug")?);
            let az_style_border_right_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderRightStyleValue) = transmute(lib.get(b"az_style_border_right_style_value_delete")?);
            let az_style_border_right_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderRightStyleValue) -> AzStyleBorderRightStyleValue = transmute(lib.get(b"az_style_border_right_style_value_deep_copy")?);
            let az_style_border_right_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightStyleValue) -> AzString = transmute(lib.get(b"az_style_border_right_style_value_fmt_debug")?);
            let az_style_border_right_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderRightWidthValue) = transmute(lib.get(b"az_style_border_right_width_value_delete")?);
            let az_style_border_right_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderRightWidthValue) -> AzStyleBorderRightWidthValue = transmute(lib.get(b"az_style_border_right_width_value_deep_copy")?);
            let az_style_border_right_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderRightWidthValue) -> AzString = transmute(lib.get(b"az_style_border_right_width_value_fmt_debug")?);
            let az_style_border_top_color_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopColorValue) = transmute(lib.get(b"az_style_border_top_color_value_delete")?);
            let az_style_border_top_color_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopColorValue) -> AzStyleBorderTopColorValue = transmute(lib.get(b"az_style_border_top_color_value_deep_copy")?);
            let az_style_border_top_color_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopColorValue) -> AzString = transmute(lib.get(b"az_style_border_top_color_value_fmt_debug")?);
            let az_style_border_top_left_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopLeftRadiusValue) = transmute(lib.get(b"az_style_border_top_left_radius_value_delete")?);
            let az_style_border_top_left_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopLeftRadiusValue) -> AzStyleBorderTopLeftRadiusValue = transmute(lib.get(b"az_style_border_top_left_radius_value_deep_copy")?);
            let az_style_border_top_left_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopLeftRadiusValue) -> AzString = transmute(lib.get(b"az_style_border_top_left_radius_value_fmt_debug")?);
            let az_style_border_top_right_radius_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopRightRadiusValue) = transmute(lib.get(b"az_style_border_top_right_radius_value_delete")?);
            let az_style_border_top_right_radius_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopRightRadiusValue) -> AzStyleBorderTopRightRadiusValue = transmute(lib.get(b"az_style_border_top_right_radius_value_deep_copy")?);
            let az_style_border_top_right_radius_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopRightRadiusValue) -> AzString = transmute(lib.get(b"az_style_border_top_right_radius_value_fmt_debug")?);
            let az_style_border_top_style_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopStyleValue) = transmute(lib.get(b"az_style_border_top_style_value_delete")?);
            let az_style_border_top_style_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopStyleValue) -> AzStyleBorderTopStyleValue = transmute(lib.get(b"az_style_border_top_style_value_deep_copy")?);
            let az_style_border_top_style_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopStyleValue) -> AzString = transmute(lib.get(b"az_style_border_top_style_value_fmt_debug")?);
            let az_style_border_top_width_value_delete: extern "C" fn(_:  &mut AzStyleBorderTopWidthValue) = transmute(lib.get(b"az_style_border_top_width_value_delete")?);
            let az_style_border_top_width_value_deep_copy: extern "C" fn(_:  &AzStyleBorderTopWidthValue) -> AzStyleBorderTopWidthValue = transmute(lib.get(b"az_style_border_top_width_value_deep_copy")?);
            let az_style_border_top_width_value_fmt_debug: extern "C" fn(_:  &AzStyleBorderTopWidthValue) -> AzString = transmute(lib.get(b"az_style_border_top_width_value_fmt_debug")?);
            let az_style_cursor_value_delete: extern "C" fn(_:  &mut AzStyleCursorValue) = transmute(lib.get(b"az_style_cursor_value_delete")?);
            let az_style_cursor_value_deep_copy: extern "C" fn(_:  &AzStyleCursorValue) -> AzStyleCursorValue = transmute(lib.get(b"az_style_cursor_value_deep_copy")?);
            let az_style_cursor_value_fmt_debug: extern "C" fn(_:  &AzStyleCursorValue) -> AzString = transmute(lib.get(b"az_style_cursor_value_fmt_debug")?);
            let az_style_font_family_value_delete: extern "C" fn(_:  &mut AzStyleFontFamilyValue) = transmute(lib.get(b"az_style_font_family_value_delete")?);
            let az_style_font_family_value_deep_copy: extern "C" fn(_:  &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue = transmute(lib.get(b"az_style_font_family_value_deep_copy")?);
            let az_style_font_family_value_fmt_debug: extern "C" fn(_:  &AzStyleFontFamilyValue) -> AzString = transmute(lib.get(b"az_style_font_family_value_fmt_debug")?);
            let az_style_font_size_value_delete: extern "C" fn(_:  &mut AzStyleFontSizeValue) = transmute(lib.get(b"az_style_font_size_value_delete")?);
            let az_style_font_size_value_deep_copy: extern "C" fn(_:  &AzStyleFontSizeValue) -> AzStyleFontSizeValue = transmute(lib.get(b"az_style_font_size_value_deep_copy")?);
            let az_style_font_size_value_fmt_debug: extern "C" fn(_:  &AzStyleFontSizeValue) -> AzString = transmute(lib.get(b"az_style_font_size_value_fmt_debug")?);
            let az_style_letter_spacing_value_delete: extern "C" fn(_:  &mut AzStyleLetterSpacingValue) = transmute(lib.get(b"az_style_letter_spacing_value_delete")?);
            let az_style_letter_spacing_value_deep_copy: extern "C" fn(_:  &AzStyleLetterSpacingValue) -> AzStyleLetterSpacingValue = transmute(lib.get(b"az_style_letter_spacing_value_deep_copy")?);
            let az_style_letter_spacing_value_fmt_debug: extern "C" fn(_:  &AzStyleLetterSpacingValue) -> AzString = transmute(lib.get(b"az_style_letter_spacing_value_fmt_debug")?);
            let az_style_line_height_value_delete: extern "C" fn(_:  &mut AzStyleLineHeightValue) = transmute(lib.get(b"az_style_line_height_value_delete")?);
            let az_style_line_height_value_deep_copy: extern "C" fn(_:  &AzStyleLineHeightValue) -> AzStyleLineHeightValue = transmute(lib.get(b"az_style_line_height_value_deep_copy")?);
            let az_style_line_height_value_fmt_debug: extern "C" fn(_:  &AzStyleLineHeightValue) -> AzString = transmute(lib.get(b"az_style_line_height_value_fmt_debug")?);
            let az_style_tab_width_value_delete: extern "C" fn(_:  &mut AzStyleTabWidthValue) = transmute(lib.get(b"az_style_tab_width_value_delete")?);
            let az_style_tab_width_value_deep_copy: extern "C" fn(_:  &AzStyleTabWidthValue) -> AzStyleTabWidthValue = transmute(lib.get(b"az_style_tab_width_value_deep_copy")?);
            let az_style_tab_width_value_fmt_debug: extern "C" fn(_:  &AzStyleTabWidthValue) -> AzString = transmute(lib.get(b"az_style_tab_width_value_fmt_debug")?);
            let az_style_text_alignment_horz_value_delete: extern "C" fn(_:  &mut AzStyleTextAlignmentHorzValue) = transmute(lib.get(b"az_style_text_alignment_horz_value_delete")?);
            let az_style_text_alignment_horz_value_deep_copy: extern "C" fn(_:  &AzStyleTextAlignmentHorzValue) -> AzStyleTextAlignmentHorzValue = transmute(lib.get(b"az_style_text_alignment_horz_value_deep_copy")?);
            let az_style_text_alignment_horz_value_fmt_debug: extern "C" fn(_:  &AzStyleTextAlignmentHorzValue) -> AzString = transmute(lib.get(b"az_style_text_alignment_horz_value_fmt_debug")?);
            let az_style_text_color_value_delete: extern "C" fn(_:  &mut AzStyleTextColorValue) = transmute(lib.get(b"az_style_text_color_value_delete")?);
            let az_style_text_color_value_deep_copy: extern "C" fn(_:  &AzStyleTextColorValue) -> AzStyleTextColorValue = transmute(lib.get(b"az_style_text_color_value_deep_copy")?);
            let az_style_text_color_value_fmt_debug: extern "C" fn(_:  &AzStyleTextColorValue) -> AzString = transmute(lib.get(b"az_style_text_color_value_fmt_debug")?);
            let az_style_word_spacing_value_delete: extern "C" fn(_:  &mut AzStyleWordSpacingValue) = transmute(lib.get(b"az_style_word_spacing_value_delete")?);
            let az_style_word_spacing_value_deep_copy: extern "C" fn(_:  &AzStyleWordSpacingValue) -> AzStyleWordSpacingValue = transmute(lib.get(b"az_style_word_spacing_value_deep_copy")?);
            let az_style_word_spacing_value_fmt_debug: extern "C" fn(_:  &AzStyleWordSpacingValue) -> AzString = transmute(lib.get(b"az_style_word_spacing_value_fmt_debug")?);
            let az_css_property_delete: extern "C" fn(_:  &mut AzCssProperty) = transmute(lib.get(b"az_css_property_delete")?);
            let az_css_property_deep_copy: extern "C" fn(_:  &AzCssProperty) -> AzCssProperty = transmute(lib.get(b"az_css_property_deep_copy")?);
            let az_css_property_fmt_debug: extern "C" fn(_:  &AzCssProperty) -> AzString = transmute(lib.get(b"az_css_property_fmt_debug")?);
            let az_dom_new: extern "C" fn(_:  AzNodeType) -> AzDom = transmute(lib.get(b"az_dom_new")?);
            let az_dom_div: extern "C" fn() -> AzDom = transmute(lib.get(b"az_dom_div")?);
            let az_dom_body: extern "C" fn() -> AzDom = transmute(lib.get(b"az_dom_body")?);
            let az_dom_label: extern "C" fn(_:  AzString) -> AzDom = transmute(lib.get(b"az_dom_label")?);
            let az_dom_text: extern "C" fn(_:  AzTextId) -> AzDom = transmute(lib.get(b"az_dom_text")?);
            let az_dom_image: extern "C" fn(_:  AzImageId) -> AzDom = transmute(lib.get(b"az_dom_image")?);
            let az_dom_gl_texture: extern "C" fn(_:  AzRefAny, _:  AzGlCallbackType) -> AzDom = transmute(lib.get(b"az_dom_gl_texture")?);
            let az_dom_iframe: extern "C" fn(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzDom = transmute(lib.get(b"az_dom_iframe")?);
            let az_dom_add_id: extern "C" fn(_:  &mut AzDom, _:  AzString) = transmute(lib.get(b"az_dom_add_id")?);
            let az_dom_with_id: extern "C" fn(_:  AzDom, _:  AzString) -> AzDom = transmute(lib.get(b"az_dom_with_id")?);
            let az_dom_set_ids: extern "C" fn(_:  &mut AzDom, _:  AzStringVec) = transmute(lib.get(b"az_dom_set_ids")?);
            let az_dom_with_ids: extern "C" fn(_:  AzDom, _:  AzStringVec) -> AzDom = transmute(lib.get(b"az_dom_with_ids")?);
            let az_dom_add_class: extern "C" fn(_:  &mut AzDom, _:  AzString) = transmute(lib.get(b"az_dom_add_class")?);
            let az_dom_with_class: extern "C" fn(_:  AzDom, _:  AzString) -> AzDom = transmute(lib.get(b"az_dom_with_class")?);
            let az_dom_set_classes: extern "C" fn(_:  &mut AzDom, _:  AzStringVec) = transmute(lib.get(b"az_dom_set_classes")?);
            let az_dom_with_classes: extern "C" fn(_:  AzDom, _:  AzStringVec) -> AzDom = transmute(lib.get(b"az_dom_with_classes")?);
            let az_dom_add_callback: extern "C" fn(_:  &mut AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) = transmute(lib.get(b"az_dom_add_callback")?);
            let az_dom_with_callback: extern "C" fn(_:  AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzDom = transmute(lib.get(b"az_dom_with_callback")?);
            let az_dom_add_css_override: extern "C" fn(_:  &mut AzDom, _:  AzString, _:  AzCssProperty) = transmute(lib.get(b"az_dom_add_css_override")?);
            let az_dom_with_css_override: extern "C" fn(_:  AzDom, _:  AzString, _:  AzCssProperty) -> AzDom = transmute(lib.get(b"az_dom_with_css_override")?);
            let az_dom_set_is_draggable: extern "C" fn(_:  &mut AzDom, _:  bool) = transmute(lib.get(b"az_dom_set_is_draggable")?);
            let az_dom_is_draggable: extern "C" fn(_:  AzDom, _:  bool) -> AzDom = transmute(lib.get(b"az_dom_is_draggable")?);
            let az_dom_set_tab_index: extern "C" fn(_:  &mut AzDom, _:  AzOptionTabIndex) = transmute(lib.get(b"az_dom_set_tab_index")?);
            let az_dom_with_tab_index: extern "C" fn(_:  AzDom, _:  AzOptionTabIndex) -> AzDom = transmute(lib.get(b"az_dom_with_tab_index")?);
            let az_dom_has_id: extern "C" fn(_:  &mut AzDom, _:  AzString) -> bool = transmute(lib.get(b"az_dom_has_id")?);
            let az_dom_has_class: extern "C" fn(_:  &mut AzDom, _:  AzString) -> bool = transmute(lib.get(b"az_dom_has_class")?);
            let az_dom_add_child: extern "C" fn(_:  &mut AzDom, _:  AzDom) = transmute(lib.get(b"az_dom_add_child")?);
            let az_dom_with_child: extern "C" fn(_:  AzDom, _:  AzDom) -> AzDom = transmute(lib.get(b"az_dom_with_child")?);
            let az_dom_get_html_string: extern "C" fn(_:  &AzDom) -> AzString = transmute(lib.get(b"az_dom_get_html_string")?);
            let az_dom_delete: extern "C" fn(_:  &mut AzDom) = transmute(lib.get(b"az_dom_delete")?);
            let az_dom_deep_copy: extern "C" fn(_:  &AzDom) -> AzDom = transmute(lib.get(b"az_dom_deep_copy")?);
            let az_dom_fmt_debug: extern "C" fn(_:  &AzDom) -> AzString = transmute(lib.get(b"az_dom_fmt_debug")?);
            let az_gl_texture_node_delete: extern "C" fn(_:  &mut AzGlTextureNode) = transmute(lib.get(b"az_gl_texture_node_delete")?);
            let az_gl_texture_node_deep_copy: extern "C" fn(_:  &AzGlTextureNode) -> AzGlTextureNode = transmute(lib.get(b"az_gl_texture_node_deep_copy")?);
            let az_gl_texture_node_fmt_debug: extern "C" fn(_:  &AzGlTextureNode) -> AzString = transmute(lib.get(b"az_gl_texture_node_fmt_debug")?);
            let az_i_frame_node_delete: extern "C" fn(_:  &mut AzIFrameNode) = transmute(lib.get(b"az_i_frame_node_delete")?);
            let az_i_frame_node_deep_copy: extern "C" fn(_:  &AzIFrameNode) -> AzIFrameNode = transmute(lib.get(b"az_i_frame_node_deep_copy")?);
            let az_i_frame_node_fmt_debug: extern "C" fn(_:  &AzIFrameNode) -> AzString = transmute(lib.get(b"az_i_frame_node_fmt_debug")?);
            let az_callback_data_delete: extern "C" fn(_:  &mut AzCallbackData) = transmute(lib.get(b"az_callback_data_delete")?);
            let az_callback_data_deep_copy: extern "C" fn(_:  &AzCallbackData) -> AzCallbackData = transmute(lib.get(b"az_callback_data_deep_copy")?);
            let az_callback_data_fmt_debug: extern "C" fn(_:  &AzCallbackData) -> AzString = transmute(lib.get(b"az_callback_data_fmt_debug")?);
            let az_override_property_delete: extern "C" fn(_:  &mut AzOverrideProperty) = transmute(lib.get(b"az_override_property_delete")?);
            let az_override_property_deep_copy: extern "C" fn(_:  &AzOverrideProperty) -> AzOverrideProperty = transmute(lib.get(b"az_override_property_deep_copy")?);
            let az_override_property_fmt_debug: extern "C" fn(_:  &AzOverrideProperty) -> AzString = transmute(lib.get(b"az_override_property_fmt_debug")?);
            let az_node_data_new: extern "C" fn(_:  AzNodeType) -> AzNodeData = transmute(lib.get(b"az_node_data_new")?);
            let az_node_data_div: extern "C" fn() -> AzNodeData = transmute(lib.get(b"az_node_data_div")?);
            let az_node_data_body: extern "C" fn() -> AzNodeData = transmute(lib.get(b"az_node_data_body")?);
            let az_node_data_label: extern "C" fn(_:  AzString) -> AzNodeData = transmute(lib.get(b"az_node_data_label")?);
            let az_node_data_text: extern "C" fn(_:  AzTextId) -> AzNodeData = transmute(lib.get(b"az_node_data_text")?);
            let az_node_data_image: extern "C" fn(_:  AzImageId) -> AzNodeData = transmute(lib.get(b"az_node_data_image")?);
            let az_node_data_gl_texture: extern "C" fn(_:  AzRefAny, _:  AzGlCallbackType) -> AzNodeData = transmute(lib.get(b"az_node_data_gl_texture")?);
            let az_node_data_iframe: extern "C" fn(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzNodeData = transmute(lib.get(b"az_node_data_iframe")?);
            let az_node_data_default: extern "C" fn() -> AzNodeData = transmute(lib.get(b"az_node_data_default")?);
            let az_node_data_add_id: extern "C" fn(_:  &mut AzNodeData, _:  AzString) = transmute(lib.get(b"az_node_data_add_id")?);
            let az_node_data_with_id: extern "C" fn(_:  AzNodeData, _:  AzString) -> AzNodeData = transmute(lib.get(b"az_node_data_with_id")?);
            let az_node_data_set_ids: extern "C" fn(_:  &mut AzNodeData, _:  AzStringVec) = transmute(lib.get(b"az_node_data_set_ids")?);
            let az_node_data_with_ids: extern "C" fn(_:  AzNodeData, _:  AzStringVec) -> AzNodeData = transmute(lib.get(b"az_node_data_with_ids")?);
            let az_node_data_add_class: extern "C" fn(_:  &mut AzNodeData, _:  AzString) = transmute(lib.get(b"az_node_data_add_class")?);
            let az_node_data_with_class: extern "C" fn(_:  AzNodeData, _:  AzString) -> AzNodeData = transmute(lib.get(b"az_node_data_with_class")?);
            let az_node_data_set_classes: extern "C" fn(_:  &mut AzNodeData, _:  AzStringVec) = transmute(lib.get(b"az_node_data_set_classes")?);
            let az_node_data_with_classes: extern "C" fn(_:  AzNodeData, _:  AzStringVec) -> AzNodeData = transmute(lib.get(b"az_node_data_with_classes")?);
            let az_node_data_add_callback: extern "C" fn(_:  &mut AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) = transmute(lib.get(b"az_node_data_add_callback")?);
            let az_node_data_with_callback: extern "C" fn(_:  AzNodeData, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzNodeData = transmute(lib.get(b"az_node_data_with_callback")?);
            let az_node_data_add_css_override: extern "C" fn(_:  &mut AzNodeData, _:  AzString, _:  AzCssProperty) = transmute(lib.get(b"az_node_data_add_css_override")?);
            let az_node_data_with_css_override: extern "C" fn(_:  AzNodeData, _:  AzString, _:  AzCssProperty) -> AzNodeData = transmute(lib.get(b"az_node_data_with_css_override")?);
            let az_node_data_set_is_draggable: extern "C" fn(_:  &mut AzNodeData, _:  bool) = transmute(lib.get(b"az_node_data_set_is_draggable")?);
            let az_node_data_is_draggable: extern "C" fn(_:  AzNodeData, _:  bool) -> AzNodeData = transmute(lib.get(b"az_node_data_is_draggable")?);
            let az_node_data_set_tab_index: extern "C" fn(_:  &mut AzNodeData, _:  AzOptionTabIndex) = transmute(lib.get(b"az_node_data_set_tab_index")?);
            let az_node_data_with_tab_index: extern "C" fn(_:  AzNodeData, _:  AzOptionTabIndex) -> AzNodeData = transmute(lib.get(b"az_node_data_with_tab_index")?);
            let az_node_data_has_id: extern "C" fn(_:  &mut AzNodeData, _:  AzString) -> bool = transmute(lib.get(b"az_node_data_has_id")?);
            let az_node_data_has_class: extern "C" fn(_:  &mut AzNodeData, _:  AzString) -> bool = transmute(lib.get(b"az_node_data_has_class")?);
            let az_node_data_delete: extern "C" fn(_:  &mut AzNodeData) = transmute(lib.get(b"az_node_data_delete")?);
            let az_node_data_deep_copy: extern "C" fn(_:  &AzNodeData) -> AzNodeData = transmute(lib.get(b"az_node_data_deep_copy")?);
            let az_node_data_fmt_debug: extern "C" fn(_:  &AzNodeData) -> AzString = transmute(lib.get(b"az_node_data_fmt_debug")?);
            let az_node_type_delete: extern "C" fn(_:  &mut AzNodeType) = transmute(lib.get(b"az_node_type_delete")?);
            let az_node_type_deep_copy: extern "C" fn(_:  &AzNodeType) -> AzNodeType = transmute(lib.get(b"az_node_type_deep_copy")?);
            let az_node_type_fmt_debug: extern "C" fn(_:  &AzNodeType) -> AzString = transmute(lib.get(b"az_node_type_fmt_debug")?);
            let az_on_into_event_filter: extern "C" fn(_:  AzOn) -> AzEventFilter = transmute(lib.get(b"az_on_into_event_filter")?);
            let az_on_delete: extern "C" fn(_:  &mut AzOn) = transmute(lib.get(b"az_on_delete")?);
            let az_on_deep_copy: extern "C" fn(_:  &AzOn) -> AzOn = transmute(lib.get(b"az_on_deep_copy")?);
            let az_on_fmt_debug: extern "C" fn(_:  &AzOn) -> AzString = transmute(lib.get(b"az_on_fmt_debug")?);
            let az_event_filter_delete: extern "C" fn(_:  &mut AzEventFilter) = transmute(lib.get(b"az_event_filter_delete")?);
            let az_event_filter_deep_copy: extern "C" fn(_:  &AzEventFilter) -> AzEventFilter = transmute(lib.get(b"az_event_filter_deep_copy")?);
            let az_event_filter_fmt_debug: extern "C" fn(_:  &AzEventFilter) -> AzString = transmute(lib.get(b"az_event_filter_fmt_debug")?);
            let az_hover_event_filter_delete: extern "C" fn(_:  &mut AzHoverEventFilter) = transmute(lib.get(b"az_hover_event_filter_delete")?);
            let az_hover_event_filter_deep_copy: extern "C" fn(_:  &AzHoverEventFilter) -> AzHoverEventFilter = transmute(lib.get(b"az_hover_event_filter_deep_copy")?);
            let az_hover_event_filter_fmt_debug: extern "C" fn(_:  &AzHoverEventFilter) -> AzString = transmute(lib.get(b"az_hover_event_filter_fmt_debug")?);
            let az_focus_event_filter_delete: extern "C" fn(_:  &mut AzFocusEventFilter) = transmute(lib.get(b"az_focus_event_filter_delete")?);
            let az_focus_event_filter_deep_copy: extern "C" fn(_:  &AzFocusEventFilter) -> AzFocusEventFilter = transmute(lib.get(b"az_focus_event_filter_deep_copy")?);
            let az_focus_event_filter_fmt_debug: extern "C" fn(_:  &AzFocusEventFilter) -> AzString = transmute(lib.get(b"az_focus_event_filter_fmt_debug")?);
            let az_not_event_filter_delete: extern "C" fn(_:  &mut AzNotEventFilter) = transmute(lib.get(b"az_not_event_filter_delete")?);
            let az_not_event_filter_deep_copy: extern "C" fn(_:  &AzNotEventFilter) -> AzNotEventFilter = transmute(lib.get(b"az_not_event_filter_deep_copy")?);
            let az_not_event_filter_fmt_debug: extern "C" fn(_:  &AzNotEventFilter) -> AzString = transmute(lib.get(b"az_not_event_filter_fmt_debug")?);
            let az_window_event_filter_delete: extern "C" fn(_:  &mut AzWindowEventFilter) = transmute(lib.get(b"az_window_event_filter_delete")?);
            let az_window_event_filter_deep_copy: extern "C" fn(_:  &AzWindowEventFilter) -> AzWindowEventFilter = transmute(lib.get(b"az_window_event_filter_deep_copy")?);
            let az_window_event_filter_fmt_debug: extern "C" fn(_:  &AzWindowEventFilter) -> AzString = transmute(lib.get(b"az_window_event_filter_fmt_debug")?);
            let az_tab_index_delete: extern "C" fn(_:  &mut AzTabIndex) = transmute(lib.get(b"az_tab_index_delete")?);
            let az_tab_index_deep_copy: extern "C" fn(_:  &AzTabIndex) -> AzTabIndex = transmute(lib.get(b"az_tab_index_deep_copy")?);
            let az_tab_index_fmt_debug: extern "C" fn(_:  &AzTabIndex) -> AzString = transmute(lib.get(b"az_tab_index_fmt_debug")?);
            let az_gl_type_delete: extern "C" fn(_:  &mut AzGlType) = transmute(lib.get(b"az_gl_type_delete")?);
            let az_gl_type_deep_copy: extern "C" fn(_:  &AzGlType) -> AzGlType = transmute(lib.get(b"az_gl_type_deep_copy")?);
            let az_gl_type_fmt_debug: extern "C" fn(_:  &AzGlType) -> AzString = transmute(lib.get(b"az_gl_type_fmt_debug")?);
            let az_debug_message_delete: extern "C" fn(_:  &mut AzDebugMessage) = transmute(lib.get(b"az_debug_message_delete")?);
            let az_debug_message_deep_copy: extern "C" fn(_:  &AzDebugMessage) -> AzDebugMessage = transmute(lib.get(b"az_debug_message_deep_copy")?);
            let az_debug_message_fmt_debug: extern "C" fn(_:  &AzDebugMessage) -> AzString = transmute(lib.get(b"az_debug_message_fmt_debug")?);
            let az_u8_vec_ref_delete: extern "C" fn(_:  &mut AzU8VecRef) = transmute(lib.get(b"az_u8_vec_ref_delete")?);
            let az_u8_vec_ref_fmt_debug: extern "C" fn(_:  &AzU8VecRef) -> AzString = transmute(lib.get(b"az_u8_vec_ref_fmt_debug")?);
            let az_u8_vec_ref_mut_delete: extern "C" fn(_:  &mut AzU8VecRefMut) = transmute(lib.get(b"az_u8_vec_ref_mut_delete")?);
            let az_u8_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzU8VecRefMut) -> AzString = transmute(lib.get(b"az_u8_vec_ref_mut_fmt_debug")?);
            let az_f32_vec_ref_delete: extern "C" fn(_:  &mut AzF32VecRef) = transmute(lib.get(b"az_f32_vec_ref_delete")?);
            let az_f32_vec_ref_fmt_debug: extern "C" fn(_:  &AzF32VecRef) -> AzString = transmute(lib.get(b"az_f32_vec_ref_fmt_debug")?);
            let az_i32_vec_ref_delete: extern "C" fn(_:  &mut AzI32VecRef) = transmute(lib.get(b"az_i32_vec_ref_delete")?);
            let az_i32_vec_ref_fmt_debug: extern "C" fn(_:  &AzI32VecRef) -> AzString = transmute(lib.get(b"az_i32_vec_ref_fmt_debug")?);
            let az_g_luint_vec_ref_delete: extern "C" fn(_:  &mut AzGLuintVecRef) = transmute(lib.get(b"az_g_luint_vec_ref_delete")?);
            let az_g_luint_vec_ref_fmt_debug: extern "C" fn(_:  &AzGLuintVecRef) -> AzString = transmute(lib.get(b"az_g_luint_vec_ref_fmt_debug")?);
            let az_g_lenum_vec_ref_delete: extern "C" fn(_:  &mut AzGLenumVecRef) = transmute(lib.get(b"az_g_lenum_vec_ref_delete")?);
            let az_g_lenum_vec_ref_fmt_debug: extern "C" fn(_:  &AzGLenumVecRef) -> AzString = transmute(lib.get(b"az_g_lenum_vec_ref_fmt_debug")?);
            let az_g_lint_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLintVecRefMut) = transmute(lib.get(b"az_g_lint_vec_ref_mut_delete")?);
            let az_g_lint_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLintVecRefMut) -> AzString = transmute(lib.get(b"az_g_lint_vec_ref_mut_fmt_debug")?);
            let az_g_lint64_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLint64VecRefMut) = transmute(lib.get(b"az_g_lint64_vec_ref_mut_delete")?);
            let az_g_lint64_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLint64VecRefMut) -> AzString = transmute(lib.get(b"az_g_lint64_vec_ref_mut_fmt_debug")?);
            let az_g_lboolean_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLbooleanVecRefMut) = transmute(lib.get(b"az_g_lboolean_vec_ref_mut_delete")?);
            let az_g_lboolean_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLbooleanVecRefMut) -> AzString = transmute(lib.get(b"az_g_lboolean_vec_ref_mut_fmt_debug")?);
            let az_g_lfloat_vec_ref_mut_delete: extern "C" fn(_:  &mut AzGLfloatVecRefMut) = transmute(lib.get(b"az_g_lfloat_vec_ref_mut_delete")?);
            let az_g_lfloat_vec_ref_mut_fmt_debug: extern "C" fn(_:  &AzGLfloatVecRefMut) -> AzString = transmute(lib.get(b"az_g_lfloat_vec_ref_mut_fmt_debug")?);
            let az_refstr_vec_ref_delete: extern "C" fn(_:  &mut AzRefstrVecRef) = transmute(lib.get(b"az_refstr_vec_ref_delete")?);
            let az_refstr_vec_ref_fmt_debug: extern "C" fn(_:  &AzRefstrVecRef) -> AzString = transmute(lib.get(b"az_refstr_vec_ref_fmt_debug")?);
            let az_refstr_delete: extern "C" fn(_:  &mut AzRefstr) = transmute(lib.get(b"az_refstr_delete")?);
            let az_refstr_fmt_debug: extern "C" fn(_:  &AzRefstr) -> AzString = transmute(lib.get(b"az_refstr_fmt_debug")?);
            let az_get_program_binary_return_delete: extern "C" fn(_:  &mut AzGetProgramBinaryReturn) = transmute(lib.get(b"az_get_program_binary_return_delete")?);
            let az_get_program_binary_return_deep_copy: extern "C" fn(_:  &AzGetProgramBinaryReturn) -> AzGetProgramBinaryReturn = transmute(lib.get(b"az_get_program_binary_return_deep_copy")?);
            let az_get_program_binary_return_fmt_debug: extern "C" fn(_:  &AzGetProgramBinaryReturn) -> AzString = transmute(lib.get(b"az_get_program_binary_return_fmt_debug")?);
            let az_get_active_attrib_return_delete: extern "C" fn(_:  &mut AzGetActiveAttribReturn) = transmute(lib.get(b"az_get_active_attrib_return_delete")?);
            let az_get_active_attrib_return_deep_copy: extern "C" fn(_:  &AzGetActiveAttribReturn) -> AzGetActiveAttribReturn = transmute(lib.get(b"az_get_active_attrib_return_deep_copy")?);
            let az_get_active_attrib_return_fmt_debug: extern "C" fn(_:  &AzGetActiveAttribReturn) -> AzString = transmute(lib.get(b"az_get_active_attrib_return_fmt_debug")?);
            let az_g_lsync_ptr_delete: extern "C" fn(_:  &mut AzGLsyncPtr) = transmute(lib.get(b"az_g_lsync_ptr_delete")?);
            let az_g_lsync_ptr_fmt_debug: extern "C" fn(_:  &AzGLsyncPtr) -> AzString = transmute(lib.get(b"az_g_lsync_ptr_fmt_debug")?);
            let az_get_active_uniform_return_delete: extern "C" fn(_:  &mut AzGetActiveUniformReturn) = transmute(lib.get(b"az_get_active_uniform_return_delete")?);
            let az_get_active_uniform_return_deep_copy: extern "C" fn(_:  &AzGetActiveUniformReturn) -> AzGetActiveUniformReturn = transmute(lib.get(b"az_get_active_uniform_return_deep_copy")?);
            let az_get_active_uniform_return_fmt_debug: extern "C" fn(_:  &AzGetActiveUniformReturn) -> AzString = transmute(lib.get(b"az_get_active_uniform_return_fmt_debug")?);
            let az_gl_context_ptr_get_type: extern "C" fn(_:  &AzGlContextPtr) -> AzGlType = transmute(lib.get(b"az_gl_context_ptr_get_type")?);
            let az_gl_context_ptr_buffer_data_untyped: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  *const c_void, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_buffer_data_untyped")?);
            let az_gl_context_ptr_buffer_sub_data_untyped: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  *const c_void) = transmute(lib.get(b"az_gl_context_ptr_buffer_sub_data_untyped")?);
            let az_gl_context_ptr_map_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> *mut c_void = transmute(lib.get(b"az_gl_context_ptr_map_buffer")?);
            let az_gl_context_ptr_map_buffer_range: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  u32) -> *mut c_void = transmute(lib.get(b"az_gl_context_ptr_map_buffer_range")?);
            let az_gl_context_ptr_unmap_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_unmap_buffer")?);
            let az_gl_context_ptr_tex_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_tex_buffer")?);
            let az_gl_context_ptr_shader_source: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzStringVec) = transmute(lib.get(b"az_gl_context_ptr_shader_source")?);
            let az_gl_context_ptr_read_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_read_buffer")?);
            let az_gl_context_ptr_read_pixels_into_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut) = transmute(lib.get(b"az_gl_context_ptr_read_pixels_into_buffer")?);
            let az_gl_context_ptr_read_pixels: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32) -> AzU8Vec = transmute(lib.get(b"az_gl_context_ptr_read_pixels")?);
            let az_gl_context_ptr_read_pixels_into_pbo: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_read_pixels_into_pbo")?);
            let az_gl_context_ptr_sample_coverage: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  bool) = transmute(lib.get(b"az_gl_context_ptr_sample_coverage")?);
            let az_gl_context_ptr_polygon_offset: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_polygon_offset")?);
            let az_gl_context_ptr_pixel_store_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_pixel_store_i")?);
            let az_gl_context_ptr_gen_buffers: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_buffers")?);
            let az_gl_context_ptr_gen_renderbuffers: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_renderbuffers")?);
            let az_gl_context_ptr_gen_framebuffers: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_framebuffers")?);
            let az_gl_context_ptr_gen_textures: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_textures")?);
            let az_gl_context_ptr_gen_vertex_arrays: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_vertex_arrays")?);
            let az_gl_context_ptr_gen_queries: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_queries")?);
            let az_gl_context_ptr_begin_query: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_begin_query")?);
            let az_gl_context_ptr_end_query: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_end_query")?);
            let az_gl_context_ptr_query_counter: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_query_counter")?);
            let az_gl_context_ptr_get_query_object_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_query_object_iv")?);
            let az_gl_context_ptr_get_query_object_uiv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u32 = transmute(lib.get(b"az_gl_context_ptr_get_query_object_uiv")?);
            let az_gl_context_ptr_get_query_object_i64v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i64 = transmute(lib.get(b"az_gl_context_ptr_get_query_object_i64v")?);
            let az_gl_context_ptr_get_query_object_ui64v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u64 = transmute(lib.get(b"az_gl_context_ptr_get_query_object_ui64v")?);
            let az_gl_context_ptr_delete_queries: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_queries")?);
            let az_gl_context_ptr_delete_vertex_arrays: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_vertex_arrays")?);
            let az_gl_context_ptr_delete_buffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_buffers")?);
            let az_gl_context_ptr_delete_renderbuffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_renderbuffers")?);
            let az_gl_context_ptr_delete_framebuffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_framebuffers")?);
            let az_gl_context_ptr_delete_textures: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_textures")?);
            let az_gl_context_ptr_framebuffer_renderbuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_framebuffer_renderbuffer")?);
            let az_gl_context_ptr_renderbuffer_storage: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_renderbuffer_storage")?);
            let az_gl_context_ptr_depth_func: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_depth_func")?);
            let az_gl_context_ptr_active_texture: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_active_texture")?);
            let az_gl_context_ptr_attach_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_attach_shader")?);
            let az_gl_context_ptr_bind_attrib_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr) = transmute(lib.get(b"az_gl_context_ptr_bind_attrib_location")?);
            let az_gl_context_ptr_get_uniform_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLintVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_uniform_iv")?);
            let az_gl_context_ptr_get_uniform_fv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLfloatVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_uniform_fv")?);
            let az_gl_context_ptr_get_uniform_block_index: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> u32 = transmute(lib.get(b"az_gl_context_ptr_get_uniform_block_index")?);
            let az_gl_context_ptr_get_uniform_indices: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstrVecRef) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_get_uniform_indices")?);
            let az_gl_context_ptr_bind_buffer_base: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_buffer_base")?);
            let az_gl_context_ptr_bind_buffer_range: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  isize, _:  isize) = transmute(lib.get(b"az_gl_context_ptr_bind_buffer_range")?);
            let az_gl_context_ptr_uniform_block_binding: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_uniform_block_binding")?);
            let az_gl_context_ptr_bind_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_buffer")?);
            let az_gl_context_ptr_bind_vertex_array: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_vertex_array")?);
            let az_gl_context_ptr_bind_renderbuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_renderbuffer")?);
            let az_gl_context_ptr_bind_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_framebuffer")?);
            let az_gl_context_ptr_bind_texture: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_texture")?);
            let az_gl_context_ptr_draw_buffers: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLenumVecRef) = transmute(lib.get(b"az_gl_context_ptr_draw_buffers")?);
            let az_gl_context_ptr_tex_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_tex_image_2d")?);
            let az_gl_context_ptr_compressed_tex_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  AzU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_compressed_tex_image_2d")?);
            let az_gl_context_ptr_compressed_tex_sub_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  AzU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_compressed_tex_sub_image_2d")?);
            let az_gl_context_ptr_tex_image_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_tex_image_3d")?);
            let az_gl_context_ptr_copy_tex_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_copy_tex_image_2d")?);
            let az_gl_context_ptr_copy_tex_sub_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_copy_tex_sub_image_2d")?);
            let az_gl_context_ptr_copy_tex_sub_image_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_copy_tex_sub_image_3d")?);
            let az_gl_context_ptr_tex_sub_image_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_tex_sub_image_2d")?);
            let az_gl_context_ptr_tex_sub_image_2d_pbo: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize) = transmute(lib.get(b"az_gl_context_ptr_tex_sub_image_2d_pbo")?);
            let az_gl_context_ptr_tex_sub_image_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_tex_sub_image_3d")?);
            let az_gl_context_ptr_tex_sub_image_3d_pbo: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize) = transmute(lib.get(b"az_gl_context_ptr_tex_sub_image_3d_pbo")?);
            let az_gl_context_ptr_tex_storage_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_tex_storage_2d")?);
            let az_gl_context_ptr_tex_storage_3d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_tex_storage_3d")?);
            let az_gl_context_ptr_get_tex_image_into_buffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_tex_image_into_buffer")?);
            let az_gl_context_ptr_copy_image_sub_data: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_copy_image_sub_data")?);
            let az_gl_context_ptr_invalidate_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef) = transmute(lib.get(b"az_gl_context_ptr_invalidate_framebuffer")?);
            let az_gl_context_ptr_invalidate_sub_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_invalidate_sub_framebuffer")?);
            let az_gl_context_ptr_get_integer_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLintVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_integer_v")?);
            let az_gl_context_ptr_get_integer_64v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLint64VecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_integer_64v")?);
            let az_gl_context_ptr_get_integer_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_integer_iv")?);
            let az_gl_context_ptr_get_integer_64iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLint64VecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_integer_64iv")?);
            let az_gl_context_ptr_get_boolean_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLbooleanVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_boolean_v")?);
            let az_gl_context_ptr_get_float_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLfloatVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_float_v")?);
            let az_gl_context_ptr_get_framebuffer_attachment_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_framebuffer_attachment_parameter_iv")?);
            let az_gl_context_ptr_get_renderbuffer_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_renderbuffer_parameter_iv")?);
            let az_gl_context_ptr_get_tex_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_tex_parameter_iv")?);
            let az_gl_context_ptr_get_tex_parameter_fv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> f32 = transmute(lib.get(b"az_gl_context_ptr_get_tex_parameter_fv")?);
            let az_gl_context_ptr_tex_parameter_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_tex_parameter_i")?);
            let az_gl_context_ptr_tex_parameter_f: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_tex_parameter_f")?);
            let az_gl_context_ptr_framebuffer_texture_2d: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_framebuffer_texture_2d")?);
            let az_gl_context_ptr_framebuffer_texture_layer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_framebuffer_texture_layer")?);
            let az_gl_context_ptr_blit_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_blit_framebuffer")?);
            let az_gl_context_ptr_vertex_attrib_4f: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  f32, _:  f32, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_vertex_attrib_4f")?);
            let az_gl_context_ptr_vertex_attrib_pointer_f32: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  bool, _:  i32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_vertex_attrib_pointer_f32")?);
            let az_gl_context_ptr_vertex_attrib_pointer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  bool, _:  i32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_vertex_attrib_pointer")?);
            let az_gl_context_ptr_vertex_attrib_i_pointer: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_vertex_attrib_i_pointer")?);
            let az_gl_context_ptr_vertex_attrib_divisor: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_vertex_attrib_divisor")?);
            let az_gl_context_ptr_viewport: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_viewport")?);
            let az_gl_context_ptr_scissor: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_scissor")?);
            let az_gl_context_ptr_line_width: extern "C" fn(_:  &AzGlContextPtr, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_line_width")?);
            let az_gl_context_ptr_use_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_use_program")?);
            let az_gl_context_ptr_validate_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_validate_program")?);
            let az_gl_context_ptr_draw_arrays: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_draw_arrays")?);
            let az_gl_context_ptr_draw_arrays_instanced: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_draw_arrays_instanced")?);
            let az_gl_context_ptr_draw_elements: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_draw_elements")?);
            let az_gl_context_ptr_draw_elements_instanced: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_draw_elements_instanced")?);
            let az_gl_context_ptr_blend_color: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_blend_color")?);
            let az_gl_context_ptr_blend_func: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_blend_func")?);
            let az_gl_context_ptr_blend_func_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_blend_func_separate")?);
            let az_gl_context_ptr_blend_equation: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_blend_equation")?);
            let az_gl_context_ptr_blend_equation_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_blend_equation_separate")?);
            let az_gl_context_ptr_color_mask: extern "C" fn(_:  &AzGlContextPtr, _:  bool, _:  bool, _:  bool, _:  bool) = transmute(lib.get(b"az_gl_context_ptr_color_mask")?);
            let az_gl_context_ptr_cull_face: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_cull_face")?);
            let az_gl_context_ptr_front_face: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_front_face")?);
            let az_gl_context_ptr_enable: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_enable")?);
            let az_gl_context_ptr_disable: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_disable")?);
            let az_gl_context_ptr_hint: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_hint")?);
            let az_gl_context_ptr_is_enabled: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_is_enabled")?);
            let az_gl_context_ptr_is_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_is_shader")?);
            let az_gl_context_ptr_is_texture: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_is_texture")?);
            let az_gl_context_ptr_is_framebuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_is_framebuffer")?);
            let az_gl_context_ptr_is_renderbuffer: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_is_renderbuffer")?);
            let az_gl_context_ptr_check_frame_buffer_status: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u32 = transmute(lib.get(b"az_gl_context_ptr_check_frame_buffer_status")?);
            let az_gl_context_ptr_enable_vertex_attrib_array: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_enable_vertex_attrib_array")?);
            let az_gl_context_ptr_disable_vertex_attrib_array: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_disable_vertex_attrib_array")?);
            let az_gl_context_ptr_uniform_1f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_uniform_1f")?);
            let az_gl_context_ptr_uniform_1fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_1fv")?);
            let az_gl_context_ptr_uniform_1i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_uniform_1i")?);
            let az_gl_context_ptr_uniform_1iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_1iv")?);
            let az_gl_context_ptr_uniform_1ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_uniform_1ui")?);
            let az_gl_context_ptr_uniform_2f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_uniform_2f")?);
            let az_gl_context_ptr_uniform_2fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_2fv")?);
            let az_gl_context_ptr_uniform_2i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_uniform_2i")?);
            let az_gl_context_ptr_uniform_2iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_2iv")?);
            let az_gl_context_ptr_uniform_2ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_uniform_2ui")?);
            let az_gl_context_ptr_uniform_3f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_uniform_3f")?);
            let az_gl_context_ptr_uniform_3fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_3fv")?);
            let az_gl_context_ptr_uniform_3i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_uniform_3i")?);
            let az_gl_context_ptr_uniform_3iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_3iv")?);
            let az_gl_context_ptr_uniform_3ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_uniform_3ui")?);
            let az_gl_context_ptr_uniform_4f: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_uniform_4f")?);
            let az_gl_context_ptr_uniform_4i: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_uniform_4i")?);
            let az_gl_context_ptr_uniform_4iv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_4iv")?);
            let az_gl_context_ptr_uniform_4ui: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_uniform_4ui")?);
            let az_gl_context_ptr_uniform_4fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_4fv")?);
            let az_gl_context_ptr_uniform_matrix_2fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_matrix_2fv")?);
            let az_gl_context_ptr_uniform_matrix_3fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_matrix_3fv")?);
            let az_gl_context_ptr_uniform_matrix_4fv: extern "C" fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef) = transmute(lib.get(b"az_gl_context_ptr_uniform_matrix_4fv")?);
            let az_gl_context_ptr_depth_mask: extern "C" fn(_:  &AzGlContextPtr, _:  bool) = transmute(lib.get(b"az_gl_context_ptr_depth_mask")?);
            let az_gl_context_ptr_depth_range: extern "C" fn(_:  &AzGlContextPtr, _:  f64, _:  f64) = transmute(lib.get(b"az_gl_context_ptr_depth_range")?);
            let az_gl_context_ptr_get_active_attrib: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveAttribReturn = transmute(lib.get(b"az_gl_context_ptr_get_active_attrib")?);
            let az_gl_context_ptr_get_active_uniform: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveUniformReturn = transmute(lib.get(b"az_gl_context_ptr_get_active_uniform")?);
            let az_gl_context_ptr_get_active_uniforms_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLuintVec, _:  u32) -> AzGLintVec = transmute(lib.get(b"az_gl_context_ptr_get_active_uniforms_iv")?);
            let az_gl_context_ptr_get_active_uniform_block_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_active_uniform_block_i")?);
            let az_gl_context_ptr_get_active_uniform_block_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> AzGLintVec = transmute(lib.get(b"az_gl_context_ptr_get_active_uniform_block_iv")?);
            let az_gl_context_ptr_get_active_uniform_block_name: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString = transmute(lib.get(b"az_gl_context_ptr_get_active_uniform_block_name")?);
            let az_gl_context_ptr_get_attrib_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_attrib_location")?);
            let az_gl_context_ptr_get_frag_data_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_frag_data_location")?);
            let az_gl_context_ptr_get_uniform_location: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_uniform_location")?);
            let az_gl_context_ptr_get_program_info_log: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzString = transmute(lib.get(b"az_gl_context_ptr_get_program_info_log")?);
            let az_gl_context_ptr_get_program_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_program_iv")?);
            let az_gl_context_ptr_get_program_binary: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzGetProgramBinaryReturn = transmute(lib.get(b"az_gl_context_ptr_get_program_binary")?);
            let az_gl_context_ptr_program_binary: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_program_binary")?);
            let az_gl_context_ptr_program_parameter_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_program_parameter_i")?);
            let az_gl_context_ptr_get_vertex_attrib_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_vertex_attrib_iv")?);
            let az_gl_context_ptr_get_vertex_attrib_fv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLfloatVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_vertex_attrib_fv")?);
            let az_gl_context_ptr_get_vertex_attrib_pointer_v: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> isize = transmute(lib.get(b"az_gl_context_ptr_get_vertex_attrib_pointer_v")?);
            let az_gl_context_ptr_get_buffer_parameter_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_buffer_parameter_iv")?);
            let az_gl_context_ptr_get_shader_info_log: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzString = transmute(lib.get(b"az_gl_context_ptr_get_shader_info_log")?);
            let az_gl_context_ptr_get_string: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> AzString = transmute(lib.get(b"az_gl_context_ptr_get_string")?);
            let az_gl_context_ptr_get_string_i: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString = transmute(lib.get(b"az_gl_context_ptr_get_string_i")?);
            let az_gl_context_ptr_get_shader_iv: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut) = transmute(lib.get(b"az_gl_context_ptr_get_shader_iv")?);
            let az_gl_context_ptr_get_shader_precision_format: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> [i32;3] = transmute(lib.get(b"az_gl_context_ptr_get_shader_precision_format")?);
            let az_gl_context_ptr_compile_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_compile_shader")?);
            let az_gl_context_ptr_create_program: extern "C" fn(_:  &AzGlContextPtr) -> u32 = transmute(lib.get(b"az_gl_context_ptr_create_program")?);
            let az_gl_context_ptr_delete_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_delete_program")?);
            let az_gl_context_ptr_create_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32) -> u32 = transmute(lib.get(b"az_gl_context_ptr_create_shader")?);
            let az_gl_context_ptr_delete_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_delete_shader")?);
            let az_gl_context_ptr_detach_shader: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_detach_shader")?);
            let az_gl_context_ptr_link_program: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_link_program")?);
            let az_gl_context_ptr_clear_color: extern "C" fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32) = transmute(lib.get(b"az_gl_context_ptr_clear_color")?);
            let az_gl_context_ptr_clear: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_clear")?);
            let az_gl_context_ptr_clear_depth: extern "C" fn(_:  &AzGlContextPtr, _:  f64) = transmute(lib.get(b"az_gl_context_ptr_clear_depth")?);
            let az_gl_context_ptr_clear_stencil: extern "C" fn(_:  &AzGlContextPtr, _:  i32) = transmute(lib.get(b"az_gl_context_ptr_clear_stencil")?);
            let az_gl_context_ptr_flush: extern "C" fn(_:  &AzGlContextPtr) = transmute(lib.get(b"az_gl_context_ptr_flush")?);
            let az_gl_context_ptr_finish: extern "C" fn(_:  &AzGlContextPtr) = transmute(lib.get(b"az_gl_context_ptr_finish")?);
            let az_gl_context_ptr_get_error: extern "C" fn(_:  &AzGlContextPtr) -> u32 = transmute(lib.get(b"az_gl_context_ptr_get_error")?);
            let az_gl_context_ptr_stencil_mask: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_stencil_mask")?);
            let az_gl_context_ptr_stencil_mask_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_stencil_mask_separate")?);
            let az_gl_context_ptr_stencil_func: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_stencil_func")?);
            let az_gl_context_ptr_stencil_func_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_stencil_func_separate")?);
            let az_gl_context_ptr_stencil_op: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_stencil_op")?);
            let az_gl_context_ptr_stencil_op_separate: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_stencil_op_separate")?);
            let az_gl_context_ptr_egl_image_target_texture2d_oes: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void) = transmute(lib.get(b"az_gl_context_ptr_egl_image_target_texture2d_oes")?);
            let az_gl_context_ptr_generate_mipmap: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_generate_mipmap")?);
            let az_gl_context_ptr_insert_event_marker_ext: extern "C" fn(_:  &AzGlContextPtr, _:  AzRefstr) = transmute(lib.get(b"az_gl_context_ptr_insert_event_marker_ext")?);
            let az_gl_context_ptr_push_group_marker_ext: extern "C" fn(_:  &AzGlContextPtr, _:  AzRefstr) = transmute(lib.get(b"az_gl_context_ptr_push_group_marker_ext")?);
            let az_gl_context_ptr_pop_group_marker_ext: extern "C" fn(_:  &AzGlContextPtr) = transmute(lib.get(b"az_gl_context_ptr_pop_group_marker_ext")?);
            let az_gl_context_ptr_debug_message_insert_khr: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  AzRefstr) = transmute(lib.get(b"az_gl_context_ptr_debug_message_insert_khr")?);
            let az_gl_context_ptr_push_debug_group_khr: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr) = transmute(lib.get(b"az_gl_context_ptr_push_debug_group_khr")?);
            let az_gl_context_ptr_pop_debug_group_khr: extern "C" fn(_:  &AzGlContextPtr) = transmute(lib.get(b"az_gl_context_ptr_pop_debug_group_khr")?);
            let az_gl_context_ptr_fence_sync: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGLsyncPtr = transmute(lib.get(b"az_gl_context_ptr_fence_sync")?);
            let az_gl_context_ptr_client_wait_sync: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64) = transmute(lib.get(b"az_gl_context_ptr_client_wait_sync")?);
            let az_gl_context_ptr_wait_sync: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64) = transmute(lib.get(b"az_gl_context_ptr_wait_sync")?);
            let az_gl_context_ptr_delete_sync: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr) = transmute(lib.get(b"az_gl_context_ptr_delete_sync")?);
            let az_gl_context_ptr_texture_range_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzU8VecRef) = transmute(lib.get(b"az_gl_context_ptr_texture_range_apple")?);
            let az_gl_context_ptr_gen_fences_apple: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_fences_apple")?);
            let az_gl_context_ptr_delete_fences_apple: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_fences_apple")?);
            let az_gl_context_ptr_set_fence_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_set_fence_apple")?);
            let az_gl_context_ptr_finish_fence_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_finish_fence_apple")?);
            let az_gl_context_ptr_test_fence_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_test_fence_apple")?);
            let az_gl_context_ptr_test_object_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u8 = transmute(lib.get(b"az_gl_context_ptr_test_object_apple")?);
            let az_gl_context_ptr_finish_object_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_finish_object_apple")?);
            let az_gl_context_ptr_get_frag_data_index: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32 = transmute(lib.get(b"az_gl_context_ptr_get_frag_data_index")?);
            let az_gl_context_ptr_blend_barrier_khr: extern "C" fn(_:  &AzGlContextPtr) = transmute(lib.get(b"az_gl_context_ptr_blend_barrier_khr")?);
            let az_gl_context_ptr_bind_frag_data_location_indexed: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  AzRefstr) = transmute(lib.get(b"az_gl_context_ptr_bind_frag_data_location_indexed")?);
            let az_gl_context_ptr_get_debug_messages: extern "C" fn(_:  &AzGlContextPtr) -> AzDebugMessageVec = transmute(lib.get(b"az_gl_context_ptr_get_debug_messages")?);
            let az_gl_context_ptr_provoking_vertex_angle: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_provoking_vertex_angle")?);
            let az_gl_context_ptr_gen_vertex_arrays_apple: extern "C" fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec = transmute(lib.get(b"az_gl_context_ptr_gen_vertex_arrays_apple")?);
            let az_gl_context_ptr_bind_vertex_array_apple: extern "C" fn(_:  &AzGlContextPtr, _:  u32) = transmute(lib.get(b"az_gl_context_ptr_bind_vertex_array_apple")?);
            let az_gl_context_ptr_delete_vertex_arrays_apple: extern "C" fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef) = transmute(lib.get(b"az_gl_context_ptr_delete_vertex_arrays_apple")?);
            let az_gl_context_ptr_copy_texture_chromium: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8) = transmute(lib.get(b"az_gl_context_ptr_copy_texture_chromium")?);
            let az_gl_context_ptr_copy_sub_texture_chromium: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8) = transmute(lib.get(b"az_gl_context_ptr_copy_sub_texture_chromium")?);
            let az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void) = transmute(lib.get(b"az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes")?);
            let az_gl_context_ptr_copy_texture_3d_angle: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8) = transmute(lib.get(b"az_gl_context_ptr_copy_texture_3d_angle")?);
            let az_gl_context_ptr_copy_sub_texture_3d_angle: extern "C" fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8) = transmute(lib.get(b"az_gl_context_ptr_copy_sub_texture_3d_angle")?);
            let az_gl_context_ptr_delete: extern "C" fn(_:  &mut AzGlContextPtr) = transmute(lib.get(b"az_gl_context_ptr_delete")?);
            let az_gl_context_ptr_deep_copy: extern "C" fn(_:  &AzGlContextPtr) -> AzGlContextPtr = transmute(lib.get(b"az_gl_context_ptr_deep_copy")?);
            let az_gl_context_ptr_fmt_debug: extern "C" fn(_:  &AzGlContextPtr) -> AzString = transmute(lib.get(b"az_gl_context_ptr_fmt_debug")?);
            let az_texture_delete: extern "C" fn(_:  &mut AzTexture) = transmute(lib.get(b"az_texture_delete")?);
            let az_texture_fmt_debug: extern "C" fn(_:  &AzTexture) -> AzString = transmute(lib.get(b"az_texture_fmt_debug")?);
            let az_texture_flags_delete: extern "C" fn(_:  &mut AzTextureFlags) = transmute(lib.get(b"az_texture_flags_delete")?);
            let az_texture_flags_deep_copy: extern "C" fn(_:  &AzTextureFlags) -> AzTextureFlags = transmute(lib.get(b"az_texture_flags_deep_copy")?);
            let az_texture_flags_fmt_debug: extern "C" fn(_:  &AzTextureFlags) -> AzString = transmute(lib.get(b"az_texture_flags_fmt_debug")?);
            let az_text_id_new: extern "C" fn() -> AzTextId = transmute(lib.get(b"az_text_id_new")?);
            let az_text_id_delete: extern "C" fn(_:  &mut AzTextId) = transmute(lib.get(b"az_text_id_delete")?);
            let az_text_id_deep_copy: extern "C" fn(_:  &AzTextId) -> AzTextId = transmute(lib.get(b"az_text_id_deep_copy")?);
            let az_text_id_fmt_debug: extern "C" fn(_:  &AzTextId) -> AzString = transmute(lib.get(b"az_text_id_fmt_debug")?);
            let az_image_id_new: extern "C" fn() -> AzImageId = transmute(lib.get(b"az_image_id_new")?);
            let az_image_id_delete: extern "C" fn(_:  &mut AzImageId) = transmute(lib.get(b"az_image_id_delete")?);
            let az_image_id_deep_copy: extern "C" fn(_:  &AzImageId) -> AzImageId = transmute(lib.get(b"az_image_id_deep_copy")?);
            let az_image_id_fmt_debug: extern "C" fn(_:  &AzImageId) -> AzString = transmute(lib.get(b"az_image_id_fmt_debug")?);
            let az_image_id_partial_eq: extern "C" fn(_:  &AzImageId, _:  &AzImageId) -> bool = transmute(lib.get(b"az_image_id_partial_eq")?);
            let az_image_id_partial_cmp: extern "C" fn(_:  &AzImageId, _:  &AzImageId) -> u8 = transmute(lib.get(b"az_image_id_partial_cmp")?);
            let az_image_id_cmp: extern "C" fn(_:  &AzImageId, _:  &AzImageId) -> u8 = transmute(lib.get(b"az_image_id_cmp")?);
            let az_image_id_hash: extern "C" fn(_:  &AzImageId) -> u64 = transmute(lib.get(b"az_image_id_hash")?);
            let az_font_id_new: extern "C" fn() -> AzFontId = transmute(lib.get(b"az_font_id_new")?);
            let az_font_id_delete: extern "C" fn(_:  &mut AzFontId) = transmute(lib.get(b"az_font_id_delete")?);
            let az_font_id_deep_copy: extern "C" fn(_:  &AzFontId) -> AzFontId = transmute(lib.get(b"az_font_id_deep_copy")?);
            let az_font_id_fmt_debug: extern "C" fn(_:  &AzFontId) -> AzString = transmute(lib.get(b"az_font_id_fmt_debug")?);
            let az_image_source_delete: extern "C" fn(_:  &mut AzImageSource) = transmute(lib.get(b"az_image_source_delete")?);
            let az_image_source_deep_copy: extern "C" fn(_:  &AzImageSource) -> AzImageSource = transmute(lib.get(b"az_image_source_deep_copy")?);
            let az_image_source_fmt_debug: extern "C" fn(_:  &AzImageSource) -> AzString = transmute(lib.get(b"az_image_source_fmt_debug")?);
            let az_font_source_delete: extern "C" fn(_:  &mut AzFontSource) = transmute(lib.get(b"az_font_source_delete")?);
            let az_font_source_deep_copy: extern "C" fn(_:  &AzFontSource) -> AzFontSource = transmute(lib.get(b"az_font_source_deep_copy")?);
            let az_font_source_fmt_debug: extern "C" fn(_:  &AzFontSource) -> AzString = transmute(lib.get(b"az_font_source_fmt_debug")?);
            let az_raw_image_new: extern "C" fn(_:  AzU8Vec, _:  usize, _:  usize, _:  AzRawImageFormat) -> AzRawImage = transmute(lib.get(b"az_raw_image_new")?);
            let az_raw_image_delete: extern "C" fn(_:  &mut AzRawImage) = transmute(lib.get(b"az_raw_image_delete")?);
            let az_raw_image_deep_copy: extern "C" fn(_:  &AzRawImage) -> AzRawImage = transmute(lib.get(b"az_raw_image_deep_copy")?);
            let az_raw_image_fmt_debug: extern "C" fn(_:  &AzRawImage) -> AzString = transmute(lib.get(b"az_raw_image_fmt_debug")?);
            let az_raw_image_format_delete: extern "C" fn(_:  &mut AzRawImageFormat) = transmute(lib.get(b"az_raw_image_format_delete")?);
            let az_raw_image_format_deep_copy: extern "C" fn(_:  &AzRawImageFormat) -> AzRawImageFormat = transmute(lib.get(b"az_raw_image_format_deep_copy")?);
            let az_raw_image_format_fmt_debug: extern "C" fn(_:  &AzRawImageFormat) -> AzString = transmute(lib.get(b"az_raw_image_format_fmt_debug")?);
            let az_drop_check_ptr_ptr_delete: extern "C" fn(_:  &mut AzDropCheckPtrPtr) = transmute(lib.get(b"az_drop_check_ptr_ptr_delete")?);
            let az_drop_check_ptr_ptr_fmt_debug: extern "C" fn(_:  &AzDropCheckPtrPtr) -> AzString = transmute(lib.get(b"az_drop_check_ptr_ptr_fmt_debug")?);
            let az_arc_mutex_ref_any_ptr_delete: extern "C" fn(_:  &mut AzArcMutexRefAnyPtr) = transmute(lib.get(b"az_arc_mutex_ref_any_ptr_delete")?);
            let az_arc_mutex_ref_any_ptr_fmt_debug: extern "C" fn(_:  &AzArcMutexRefAnyPtr) -> AzString = transmute(lib.get(b"az_arc_mutex_ref_any_ptr_fmt_debug")?);
            let az_timer_delete: extern "C" fn(_:  &mut AzTimer) = transmute(lib.get(b"az_timer_delete")?);
            let az_timer_deep_copy: extern "C" fn(_:  &AzTimer) -> AzTimer = transmute(lib.get(b"az_timer_deep_copy")?);
            let az_timer_fmt_debug: extern "C" fn(_:  &AzTimer) -> AzString = transmute(lib.get(b"az_timer_fmt_debug")?);
            let az_task_ptr_new: extern "C" fn(_:  AzArcMutexRefAnyPtr, _:  AzTaskCallbackType) -> AzTaskPtr = transmute(lib.get(b"az_task_ptr_new")?);
            let az_task_ptr_then: extern "C" fn(_:  AzTaskPtr, _:  AzTimer) -> AzTaskPtr = transmute(lib.get(b"az_task_ptr_then")?);
            let az_task_ptr_delete: extern "C" fn(_:  &mut AzTaskPtr) = transmute(lib.get(b"az_task_ptr_delete")?);
            let az_task_ptr_fmt_debug: extern "C" fn(_:  &AzTaskPtr) -> AzString = transmute(lib.get(b"az_task_ptr_fmt_debug")?);
            let az_thread_ptr_new: extern "C" fn(_:  AzRefAny, _:  AzThreadCallbackType) -> AzThreadPtr = transmute(lib.get(b"az_thread_ptr_new")?);
            let az_thread_ptr_block: extern "C" fn(_:  AzThreadPtr) -> AzResultRefAnyBlockError = transmute(lib.get(b"az_thread_ptr_block")?);
            let az_thread_ptr_delete: extern "C" fn(_:  &mut AzThreadPtr) = transmute(lib.get(b"az_thread_ptr_delete")?);
            let az_thread_ptr_fmt_debug: extern "C" fn(_:  &AzThreadPtr) -> AzString = transmute(lib.get(b"az_thread_ptr_fmt_debug")?);
            let az_drop_check_ptr_delete: extern "C" fn(_:  &mut AzDropCheckPtr) = transmute(lib.get(b"az_drop_check_ptr_delete")?);
            let az_drop_check_ptr_fmt_debug: extern "C" fn(_:  &AzDropCheckPtr) -> AzString = transmute(lib.get(b"az_drop_check_ptr_fmt_debug")?);
            let az_timer_id_delete: extern "C" fn(_:  &mut AzTimerId) = transmute(lib.get(b"az_timer_id_delete")?);
            let az_timer_id_deep_copy: extern "C" fn(_:  &AzTimerId) -> AzTimerId = transmute(lib.get(b"az_timer_id_deep_copy")?);
            let az_timer_id_fmt_debug: extern "C" fn(_:  &AzTimerId) -> AzString = transmute(lib.get(b"az_timer_id_fmt_debug")?);
            let az_terminate_timer_delete: extern "C" fn(_:  &mut AzTerminateTimer) = transmute(lib.get(b"az_terminate_timer_delete")?);
            let az_terminate_timer_deep_copy: extern "C" fn(_:  &AzTerminateTimer) -> AzTerminateTimer = transmute(lib.get(b"az_terminate_timer_deep_copy")?);
            let az_terminate_timer_fmt_debug: extern "C" fn(_:  &AzTerminateTimer) -> AzString = transmute(lib.get(b"az_terminate_timer_fmt_debug")?);
            let az_block_error_delete: extern "C" fn(_:  &mut AzBlockError) = transmute(lib.get(b"az_block_error_delete")?);
            let az_block_error_deep_copy: extern "C" fn(_:  &AzBlockError) -> AzBlockError = transmute(lib.get(b"az_block_error_deep_copy")?);
            let az_block_error_fmt_debug: extern "C" fn(_:  &AzBlockError) -> AzString = transmute(lib.get(b"az_block_error_fmt_debug")?);
            let az_task_bar_icon_delete: extern "C" fn(_:  &mut AzTaskBarIcon) = transmute(lib.get(b"az_task_bar_icon_delete")?);
            let az_task_bar_icon_deep_copy: extern "C" fn(_:  &AzTaskBarIcon) -> AzTaskBarIcon = transmute(lib.get(b"az_task_bar_icon_deep_copy")?);
            let az_task_bar_icon_fmt_debug: extern "C" fn(_:  &AzTaskBarIcon) -> AzString = transmute(lib.get(b"az_task_bar_icon_fmt_debug")?);
            let az_x_window_type_delete: extern "C" fn(_:  &mut AzXWindowType) = transmute(lib.get(b"az_x_window_type_delete")?);
            let az_x_window_type_deep_copy: extern "C" fn(_:  &AzXWindowType) -> AzXWindowType = transmute(lib.get(b"az_x_window_type_deep_copy")?);
            let az_x_window_type_fmt_debug: extern "C" fn(_:  &AzXWindowType) -> AzString = transmute(lib.get(b"az_x_window_type_fmt_debug")?);
            let az_physical_position_i32_delete: extern "C" fn(_:  &mut AzPhysicalPositionI32) = transmute(lib.get(b"az_physical_position_i32_delete")?);
            let az_physical_position_i32_deep_copy: extern "C" fn(_:  &AzPhysicalPositionI32) -> AzPhysicalPositionI32 = transmute(lib.get(b"az_physical_position_i32_deep_copy")?);
            let az_physical_position_i32_fmt_debug: extern "C" fn(_:  &AzPhysicalPositionI32) -> AzString = transmute(lib.get(b"az_physical_position_i32_fmt_debug")?);
            let az_physical_position_i32_partial_eq: extern "C" fn(_:  &AzPhysicalPositionI32, _:  &AzPhysicalPositionI32) -> bool = transmute(lib.get(b"az_physical_position_i32_partial_eq")?);
            let az_physical_position_i32_partial_cmp: extern "C" fn(_:  &AzPhysicalPositionI32, _:  &AzPhysicalPositionI32) -> u8 = transmute(lib.get(b"az_physical_position_i32_partial_cmp")?);
            let az_physical_position_i32_cmp: extern "C" fn(_:  &AzPhysicalPositionI32, _:  &AzPhysicalPositionI32) -> u8 = transmute(lib.get(b"az_physical_position_i32_cmp")?);
            let az_physical_position_i32_hash: extern "C" fn(_:  &AzPhysicalPositionI32) -> u64 = transmute(lib.get(b"az_physical_position_i32_hash")?);
            let az_physical_size_u32_delete: extern "C" fn(_:  &mut AzPhysicalSizeU32) = transmute(lib.get(b"az_physical_size_u32_delete")?);
            let az_physical_size_u32_deep_copy: extern "C" fn(_:  &AzPhysicalSizeU32) -> AzPhysicalSizeU32 = transmute(lib.get(b"az_physical_size_u32_deep_copy")?);
            let az_physical_size_u32_fmt_debug: extern "C" fn(_:  &AzPhysicalSizeU32) -> AzString = transmute(lib.get(b"az_physical_size_u32_fmt_debug")?);
            let az_physical_size_u32_partial_eq: extern "C" fn(_:  &AzPhysicalSizeU32, _:  &AzPhysicalSizeU32) -> bool = transmute(lib.get(b"az_physical_size_u32_partial_eq")?);
            let az_physical_size_u32_partial_cmp: extern "C" fn(_:  &AzPhysicalSizeU32, _:  &AzPhysicalSizeU32) -> u8 = transmute(lib.get(b"az_physical_size_u32_partial_cmp")?);
            let az_physical_size_u32_cmp: extern "C" fn(_:  &AzPhysicalSizeU32, _:  &AzPhysicalSizeU32) -> u8 = transmute(lib.get(b"az_physical_size_u32_cmp")?);
            let az_physical_size_u32_hash: extern "C" fn(_:  &AzPhysicalSizeU32) -> u64 = transmute(lib.get(b"az_physical_size_u32_hash")?);
            let az_logical_position_delete: extern "C" fn(_:  &mut AzLogicalPosition) = transmute(lib.get(b"az_logical_position_delete")?);
            let az_logical_position_deep_copy: extern "C" fn(_:  &AzLogicalPosition) -> AzLogicalPosition = transmute(lib.get(b"az_logical_position_deep_copy")?);
            let az_logical_position_fmt_debug: extern "C" fn(_:  &AzLogicalPosition) -> AzString = transmute(lib.get(b"az_logical_position_fmt_debug")?);
            let az_icon_key_delete: extern "C" fn(_:  &mut AzIconKey) = transmute(lib.get(b"az_icon_key_delete")?);
            let az_icon_key_deep_copy: extern "C" fn(_:  &AzIconKey) -> AzIconKey = transmute(lib.get(b"az_icon_key_deep_copy")?);
            let az_icon_key_fmt_debug: extern "C" fn(_:  &AzIconKey) -> AzString = transmute(lib.get(b"az_icon_key_fmt_debug")?);
            let az_small_window_icon_bytes_delete: extern "C" fn(_:  &mut AzSmallWindowIconBytes) = transmute(lib.get(b"az_small_window_icon_bytes_delete")?);
            let az_small_window_icon_bytes_deep_copy: extern "C" fn(_:  &AzSmallWindowIconBytes) -> AzSmallWindowIconBytes = transmute(lib.get(b"az_small_window_icon_bytes_deep_copy")?);
            let az_small_window_icon_bytes_fmt_debug: extern "C" fn(_:  &AzSmallWindowIconBytes) -> AzString = transmute(lib.get(b"az_small_window_icon_bytes_fmt_debug")?);
            let az_large_window_icon_bytes_delete: extern "C" fn(_:  &mut AzLargeWindowIconBytes) = transmute(lib.get(b"az_large_window_icon_bytes_delete")?);
            let az_large_window_icon_bytes_deep_copy: extern "C" fn(_:  &AzLargeWindowIconBytes) -> AzLargeWindowIconBytes = transmute(lib.get(b"az_large_window_icon_bytes_deep_copy")?);
            let az_large_window_icon_bytes_fmt_debug: extern "C" fn(_:  &AzLargeWindowIconBytes) -> AzString = transmute(lib.get(b"az_large_window_icon_bytes_fmt_debug")?);
            let az_window_icon_delete: extern "C" fn(_:  &mut AzWindowIcon) = transmute(lib.get(b"az_window_icon_delete")?);
            let az_window_icon_deep_copy: extern "C" fn(_:  &AzWindowIcon) -> AzWindowIcon = transmute(lib.get(b"az_window_icon_deep_copy")?);
            let az_window_icon_fmt_debug: extern "C" fn(_:  &AzWindowIcon) -> AzString = transmute(lib.get(b"az_window_icon_fmt_debug")?);
            let az_virtual_key_code_delete: extern "C" fn(_:  &mut AzVirtualKeyCode) = transmute(lib.get(b"az_virtual_key_code_delete")?);
            let az_virtual_key_code_deep_copy: extern "C" fn(_:  &AzVirtualKeyCode) -> AzVirtualKeyCode = transmute(lib.get(b"az_virtual_key_code_deep_copy")?);
            let az_virtual_key_code_fmt_debug: extern "C" fn(_:  &AzVirtualKeyCode) -> AzString = transmute(lib.get(b"az_virtual_key_code_fmt_debug")?);
            let az_accelerator_key_delete: extern "C" fn(_:  &mut AzAcceleratorKey) = transmute(lib.get(b"az_accelerator_key_delete")?);
            let az_accelerator_key_deep_copy: extern "C" fn(_:  &AzAcceleratorKey) -> AzAcceleratorKey = transmute(lib.get(b"az_accelerator_key_deep_copy")?);
            let az_accelerator_key_fmt_debug: extern "C" fn(_:  &AzAcceleratorKey) -> AzString = transmute(lib.get(b"az_accelerator_key_fmt_debug")?);
            let az_window_size_delete: extern "C" fn(_:  &mut AzWindowSize) = transmute(lib.get(b"az_window_size_delete")?);
            let az_window_size_deep_copy: extern "C" fn(_:  &AzWindowSize) -> AzWindowSize = transmute(lib.get(b"az_window_size_deep_copy")?);
            let az_window_size_fmt_debug: extern "C" fn(_:  &AzWindowSize) -> AzString = transmute(lib.get(b"az_window_size_fmt_debug")?);
            let az_window_flags_delete: extern "C" fn(_:  &mut AzWindowFlags) = transmute(lib.get(b"az_window_flags_delete")?);
            let az_window_flags_deep_copy: extern "C" fn(_:  &AzWindowFlags) -> AzWindowFlags = transmute(lib.get(b"az_window_flags_deep_copy")?);
            let az_window_flags_fmt_debug: extern "C" fn(_:  &AzWindowFlags) -> AzString = transmute(lib.get(b"az_window_flags_fmt_debug")?);
            let az_debug_state_delete: extern "C" fn(_:  &mut AzDebugState) = transmute(lib.get(b"az_debug_state_delete")?);
            let az_debug_state_deep_copy: extern "C" fn(_:  &AzDebugState) -> AzDebugState = transmute(lib.get(b"az_debug_state_deep_copy")?);
            let az_debug_state_fmt_debug: extern "C" fn(_:  &AzDebugState) -> AzString = transmute(lib.get(b"az_debug_state_fmt_debug")?);
            let az_keyboard_state_delete: extern "C" fn(_:  &mut AzKeyboardState) = transmute(lib.get(b"az_keyboard_state_delete")?);
            let az_keyboard_state_deep_copy: extern "C" fn(_:  &AzKeyboardState) -> AzKeyboardState = transmute(lib.get(b"az_keyboard_state_deep_copy")?);
            let az_keyboard_state_fmt_debug: extern "C" fn(_:  &AzKeyboardState) -> AzString = transmute(lib.get(b"az_keyboard_state_fmt_debug")?);
            let az_mouse_cursor_type_delete: extern "C" fn(_:  &mut AzMouseCursorType) = transmute(lib.get(b"az_mouse_cursor_type_delete")?);
            let az_mouse_cursor_type_deep_copy: extern "C" fn(_:  &AzMouseCursorType) -> AzMouseCursorType = transmute(lib.get(b"az_mouse_cursor_type_deep_copy")?);
            let az_mouse_cursor_type_fmt_debug: extern "C" fn(_:  &AzMouseCursorType) -> AzString = transmute(lib.get(b"az_mouse_cursor_type_fmt_debug")?);
            let az_cursor_position_delete: extern "C" fn(_:  &mut AzCursorPosition) = transmute(lib.get(b"az_cursor_position_delete")?);
            let az_cursor_position_deep_copy: extern "C" fn(_:  &AzCursorPosition) -> AzCursorPosition = transmute(lib.get(b"az_cursor_position_deep_copy")?);
            let az_cursor_position_fmt_debug: extern "C" fn(_:  &AzCursorPosition) -> AzString = transmute(lib.get(b"az_cursor_position_fmt_debug")?);
            let az_mouse_state_delete: extern "C" fn(_:  &mut AzMouseState) = transmute(lib.get(b"az_mouse_state_delete")?);
            let az_mouse_state_deep_copy: extern "C" fn(_:  &AzMouseState) -> AzMouseState = transmute(lib.get(b"az_mouse_state_deep_copy")?);
            let az_mouse_state_fmt_debug: extern "C" fn(_:  &AzMouseState) -> AzString = transmute(lib.get(b"az_mouse_state_fmt_debug")?);
            let az_platform_specific_options_delete: extern "C" fn(_:  &mut AzPlatformSpecificOptions) = transmute(lib.get(b"az_platform_specific_options_delete")?);
            let az_platform_specific_options_deep_copy: extern "C" fn(_:  &AzPlatformSpecificOptions) -> AzPlatformSpecificOptions = transmute(lib.get(b"az_platform_specific_options_deep_copy")?);
            let az_platform_specific_options_fmt_debug: extern "C" fn(_:  &AzPlatformSpecificOptions) -> AzString = transmute(lib.get(b"az_platform_specific_options_fmt_debug")?);
            let az_windows_window_options_delete: extern "C" fn(_:  &mut AzWindowsWindowOptions) = transmute(lib.get(b"az_windows_window_options_delete")?);
            let az_windows_window_options_deep_copy: extern "C" fn(_:  &AzWindowsWindowOptions) -> AzWindowsWindowOptions = transmute(lib.get(b"az_windows_window_options_deep_copy")?);
            let az_windows_window_options_fmt_debug: extern "C" fn(_:  &AzWindowsWindowOptions) -> AzString = transmute(lib.get(b"az_windows_window_options_fmt_debug")?);
            let az_wayland_theme_delete: extern "C" fn(_:  &mut AzWaylandTheme) = transmute(lib.get(b"az_wayland_theme_delete")?);
            let az_wayland_theme_deep_copy: extern "C" fn(_:  &AzWaylandTheme) -> AzWaylandTheme = transmute(lib.get(b"az_wayland_theme_deep_copy")?);
            let az_wayland_theme_fmt_debug: extern "C" fn(_:  &AzWaylandTheme) -> AzString = transmute(lib.get(b"az_wayland_theme_fmt_debug")?);
            let az_renderer_type_delete: extern "C" fn(_:  &mut AzRendererType) = transmute(lib.get(b"az_renderer_type_delete")?);
            let az_renderer_type_deep_copy: extern "C" fn(_:  &AzRendererType) -> AzRendererType = transmute(lib.get(b"az_renderer_type_deep_copy")?);
            let az_renderer_type_fmt_debug: extern "C" fn(_:  &AzRendererType) -> AzString = transmute(lib.get(b"az_renderer_type_fmt_debug")?);
            let az_string_pair_delete: extern "C" fn(_:  &mut AzStringPair) = transmute(lib.get(b"az_string_pair_delete")?);
            let az_string_pair_deep_copy: extern "C" fn(_:  &AzStringPair) -> AzStringPair = transmute(lib.get(b"az_string_pair_deep_copy")?);
            let az_string_pair_fmt_debug: extern "C" fn(_:  &AzStringPair) -> AzString = transmute(lib.get(b"az_string_pair_fmt_debug")?);
            let az_linux_window_options_delete: extern "C" fn(_:  &mut AzLinuxWindowOptions) = transmute(lib.get(b"az_linux_window_options_delete")?);
            let az_linux_window_options_deep_copy: extern "C" fn(_:  &AzLinuxWindowOptions) -> AzLinuxWindowOptions = transmute(lib.get(b"az_linux_window_options_deep_copy")?);
            let az_linux_window_options_fmt_debug: extern "C" fn(_:  &AzLinuxWindowOptions) -> AzString = transmute(lib.get(b"az_linux_window_options_fmt_debug")?);
            let az_mac_window_options_delete: extern "C" fn(_:  &mut AzMacWindowOptions) = transmute(lib.get(b"az_mac_window_options_delete")?);
            let az_mac_window_options_deep_copy: extern "C" fn(_:  &AzMacWindowOptions) -> AzMacWindowOptions = transmute(lib.get(b"az_mac_window_options_deep_copy")?);
            let az_mac_window_options_fmt_debug: extern "C" fn(_:  &AzMacWindowOptions) -> AzString = transmute(lib.get(b"az_mac_window_options_fmt_debug")?);
            let az_wasm_window_options_delete: extern "C" fn(_:  &mut AzWasmWindowOptions) = transmute(lib.get(b"az_wasm_window_options_delete")?);
            let az_wasm_window_options_deep_copy: extern "C" fn(_:  &AzWasmWindowOptions) -> AzWasmWindowOptions = transmute(lib.get(b"az_wasm_window_options_deep_copy")?);
            let az_wasm_window_options_fmt_debug: extern "C" fn(_:  &AzWasmWindowOptions) -> AzString = transmute(lib.get(b"az_wasm_window_options_fmt_debug")?);
            let az_full_screen_mode_delete: extern "C" fn(_:  &mut AzFullScreenMode) = transmute(lib.get(b"az_full_screen_mode_delete")?);
            let az_full_screen_mode_deep_copy: extern "C" fn(_:  &AzFullScreenMode) -> AzFullScreenMode = transmute(lib.get(b"az_full_screen_mode_deep_copy")?);
            let az_full_screen_mode_fmt_debug: extern "C" fn(_:  &AzFullScreenMode) -> AzString = transmute(lib.get(b"az_full_screen_mode_fmt_debug")?);
            let az_window_state_delete: extern "C" fn(_:  &mut AzWindowState) = transmute(lib.get(b"az_window_state_delete")?);
            let az_window_state_deep_copy: extern "C" fn(_:  &AzWindowState) -> AzWindowState = transmute(lib.get(b"az_window_state_deep_copy")?);
            let az_window_state_fmt_debug: extern "C" fn(_:  &AzWindowState) -> AzString = transmute(lib.get(b"az_window_state_fmt_debug")?);
            let az_logical_size_delete: extern "C" fn(_:  &mut AzLogicalSize) = transmute(lib.get(b"az_logical_size_delete")?);
            let az_logical_size_deep_copy: extern "C" fn(_:  &AzLogicalSize) -> AzLogicalSize = transmute(lib.get(b"az_logical_size_deep_copy")?);
            let az_logical_size_fmt_debug: extern "C" fn(_:  &AzLogicalSize) -> AzString = transmute(lib.get(b"az_logical_size_fmt_debug")?);
            let az_hot_reload_options_delete: extern "C" fn(_:  &mut AzHotReloadOptions) = transmute(lib.get(b"az_hot_reload_options_delete")?);
            let az_hot_reload_options_deep_copy: extern "C" fn(_:  &AzHotReloadOptions) -> AzHotReloadOptions = transmute(lib.get(b"az_hot_reload_options_deep_copy")?);
            let az_hot_reload_options_fmt_debug: extern "C" fn(_:  &AzHotReloadOptions) -> AzString = transmute(lib.get(b"az_hot_reload_options_fmt_debug")?);
            let az_window_create_options_new: extern "C" fn(_:  AzCss) -> AzWindowCreateOptions = transmute(lib.get(b"az_window_create_options_new")?);
            let az_window_create_options_delete: extern "C" fn(_:  &mut AzWindowCreateOptions) = transmute(lib.get(b"az_window_create_options_delete")?);
            let az_window_create_options_deep_copy: extern "C" fn(_:  &AzWindowCreateOptions) -> AzWindowCreateOptions = transmute(lib.get(b"az_window_create_options_deep_copy")?);
            let az_window_create_options_fmt_debug: extern "C" fn(_:  &AzWindowCreateOptions) -> AzString = transmute(lib.get(b"az_window_create_options_fmt_debug")?);
            Some(AzulDll {
                lib: lib,
                az_string_from_utf8_unchecked,
                az_string_from_utf8_lossy,
                az_string_into_bytes,
                az_string_delete,
                az_string_deep_copy,
                az_string_fmt_debug,
                az_x_window_type_vec_copy_from,
                az_x_window_type_vec_delete,
                az_x_window_type_vec_deep_copy,
                az_x_window_type_vec_fmt_debug,
                az_virtual_key_code_vec_copy_from,
                az_virtual_key_code_vec_delete,
                az_virtual_key_code_vec_deep_copy,
                az_virtual_key_code_vec_fmt_debug,
                az_scan_code_vec_copy_from,
                az_scan_code_vec_delete,
                az_scan_code_vec_deep_copy,
                az_scan_code_vec_fmt_debug,
                az_css_declaration_vec_copy_from,
                az_css_declaration_vec_delete,
                az_css_declaration_vec_deep_copy,
                az_css_declaration_vec_fmt_debug,
                az_css_path_selector_vec_copy_from,
                az_css_path_selector_vec_delete,
                az_css_path_selector_vec_deep_copy,
                az_css_path_selector_vec_fmt_debug,
                az_stylesheet_vec_copy_from,
                az_stylesheet_vec_delete,
                az_stylesheet_vec_deep_copy,
                az_stylesheet_vec_fmt_debug,
                az_css_rule_block_vec_copy_from,
                az_css_rule_block_vec_delete,
                az_css_rule_block_vec_deep_copy,
                az_css_rule_block_vec_fmt_debug,
                az_u8_vec_copy_from,
                az_u8_vec_delete,
                az_u8_vec_deep_copy,
                az_u8_vec_fmt_debug,
                az_callback_data_vec_copy_from,
                az_callback_data_vec_delete,
                az_callback_data_vec_deep_copy,
                az_callback_data_vec_fmt_debug,
                az_debug_message_vec_copy_from,
                az_debug_message_vec_delete,
                az_debug_message_vec_deep_copy,
                az_debug_message_vec_fmt_debug,
                az_g_luint_vec_copy_from,
                az_g_luint_vec_delete,
                az_g_luint_vec_deep_copy,
                az_g_luint_vec_fmt_debug,
                az_g_lint_vec_copy_from,
                az_g_lint_vec_delete,
                az_g_lint_vec_deep_copy,
                az_g_lint_vec_fmt_debug,
                az_override_property_vec_copy_from,
                az_override_property_vec_delete,
                az_override_property_vec_deep_copy,
                az_override_property_vec_fmt_debug,
                az_dom_vec_copy_from,
                az_dom_vec_delete,
                az_dom_vec_deep_copy,
                az_dom_vec_fmt_debug,
                az_string_vec_copy_from,
                az_string_vec_delete,
                az_string_vec_deep_copy,
                az_string_vec_fmt_debug,
                az_string_pair_vec_copy_from,
                az_string_pair_vec_delete,
                az_string_pair_vec_deep_copy,
                az_string_pair_vec_fmt_debug,
                az_gradient_stop_pre_vec_copy_from,
                az_gradient_stop_pre_vec_delete,
                az_gradient_stop_pre_vec_deep_copy,
                az_gradient_stop_pre_vec_fmt_debug,
                az_option_wayland_theme_delete,
                az_option_wayland_theme_deep_copy,
                az_option_wayland_theme_fmt_debug,
                az_option_task_bar_icon_delete,
                az_option_task_bar_icon_deep_copy,
                az_option_task_bar_icon_fmt_debug,
                az_option_hwnd_handle_delete,
                az_option_hwnd_handle_deep_copy,
                az_option_hwnd_handle_fmt_debug,
                az_option_logical_position_delete,
                az_option_logical_position_deep_copy,
                az_option_logical_position_fmt_debug,
                az_option_hot_reload_options_delete,
                az_option_hot_reload_options_deep_copy,
                az_option_hot_reload_options_fmt_debug,
                az_option_physical_position_i32_delete,
                az_option_physical_position_i32_deep_copy,
                az_option_physical_position_i32_fmt_debug,
                az_option_window_icon_delete,
                az_option_window_icon_deep_copy,
                az_option_window_icon_fmt_debug,
                az_option_string_delete,
                az_option_string_deep_copy,
                az_option_string_fmt_debug,
                az_option_x11_visual_delete,
                az_option_x11_visual_deep_copy,
                az_option_x11_visual_fmt_debug,
                az_option_i32_delete,
                az_option_i32_deep_copy,
                az_option_i32_fmt_debug,
                az_option_f32_delete,
                az_option_f32_deep_copy,
                az_option_f32_fmt_debug,
                az_option_mouse_cursor_type_delete,
                az_option_mouse_cursor_type_deep_copy,
                az_option_mouse_cursor_type_fmt_debug,
                az_option_logical_size_delete,
                az_option_logical_size_deep_copy,
                az_option_logical_size_fmt_debug,
                az_option_char_delete,
                az_option_char_deep_copy,
                az_option_char_fmt_debug,
                az_option_virtual_key_code_delete,
                az_option_virtual_key_code_deep_copy,
                az_option_virtual_key_code_fmt_debug,
                az_option_percentage_value_delete,
                az_option_percentage_value_deep_copy,
                az_option_percentage_value_fmt_debug,
                az_option_dom_delete,
                az_option_dom_deep_copy,
                az_option_dom_fmt_debug,
                az_option_texture_delete,
                az_option_texture_fmt_debug,
                az_option_tab_index_delete,
                az_option_tab_index_deep_copy,
                az_option_tab_index_fmt_debug,
                az_option_duration_delete,
                az_option_duration_deep_copy,
                az_option_duration_fmt_debug,
                az_option_instant_ptr_delete,
                az_option_instant_ptr_deep_copy,
                az_option_instant_ptr_fmt_debug,
                az_option_usize_delete,
                az_option_usize_deep_copy,
                az_option_usize_fmt_debug,
                az_option_u8_vec_ref_delete,
                az_option_u8_vec_ref_fmt_debug,
                az_result_ref_any_block_error_delete,
                az_result_ref_any_block_error_deep_copy,
                az_result_ref_any_block_error_fmt_debug,
                az_instant_ptr_now,
                az_instant_ptr_delete,
                az_instant_ptr_fmt_debug,
                az_instant_ptr_partial_eq,
                az_instant_ptr_partial_cmp,
                az_instant_ptr_cmp,
                az_duration_delete,
                az_duration_deep_copy,
                az_duration_fmt_debug,
                az_app_config_ptr_default,
                az_app_config_ptr_delete,
                az_app_config_ptr_fmt_debug,
                az_app_ptr_new,
                az_app_ptr_run,
                az_app_ptr_delete,
                az_app_ptr_fmt_debug,
                az_hidpi_adjusted_bounds_get_logical_size,
                az_hidpi_adjusted_bounds_get_physical_size,
                az_hidpi_adjusted_bounds_get_hidpi_factor,
                az_hidpi_adjusted_bounds_delete,
                az_hidpi_adjusted_bounds_deep_copy,
                az_hidpi_adjusted_bounds_fmt_debug,
                az_layout_callback_delete,
                az_layout_callback_deep_copy,
                az_layout_callback_fmt_debug,
                az_callback_delete,
                az_callback_deep_copy,
                az_callback_fmt_debug,
                az_callback_partial_eq,
                az_callback_partial_cmp,
                az_callback_cmp,
                az_callback_hash,
                az_callback_info_ptr_get_state,
                az_callback_info_ptr_get_keyboard_state,
                az_callback_info_ptr_get_mouse_state,
                az_callback_info_ptr_set_window_state,
                az_callback_info_ptr_delete,
                az_callback_info_ptr_fmt_debug,
                az_update_screen_delete,
                az_update_screen_deep_copy,
                az_update_screen_fmt_debug,
                az_i_frame_callback_delete,
                az_i_frame_callback_deep_copy,
                az_i_frame_callback_fmt_debug,
                az_i_frame_callback_info_ptr_get_state,
                az_i_frame_callback_info_ptr_get_bounds,
                az_i_frame_callback_info_ptr_delete,
                az_i_frame_callback_info_ptr_fmt_debug,
                az_i_frame_callback_return_delete,
                az_i_frame_callback_return_deep_copy,
                az_i_frame_callback_return_fmt_debug,
                az_gl_callback_delete,
                az_gl_callback_deep_copy,
                az_gl_callback_fmt_debug,
                az_gl_callback_info_ptr_get_state,
                az_gl_callback_info_ptr_get_bounds,
                az_gl_callback_info_ptr_delete,
                az_gl_callback_info_ptr_fmt_debug,
                az_gl_callback_return_delete,
                az_gl_callback_return_fmt_debug,
                az_timer_callback_delete,
                az_timer_callback_deep_copy,
                az_timer_callback_fmt_debug,
                az_timer_callback_type_ptr_delete,
                az_timer_callback_type_ptr_fmt_debug,
                az_timer_callback_info_ptr_get_state,
                az_timer_callback_info_ptr_delete,
                az_timer_callback_info_ptr_fmt_debug,
                az_timer_callback_return_delete,
                az_timer_callback_return_deep_copy,
                az_timer_callback_return_fmt_debug,
                az_ref_any_sharing_info_can_be_shared,
                az_ref_any_sharing_info_can_be_shared_mut,
                az_ref_any_sharing_info_increase_ref,
                az_ref_any_sharing_info_decrease_ref,
                az_ref_any_sharing_info_increase_refmut,
                az_ref_any_sharing_info_decrease_refmut,
                az_ref_any_sharing_info_delete,
                az_ref_any_sharing_info_fmt_debug,
                az_ref_any_new_c,
                az_ref_any_is_type,
                az_ref_any_get_type_name,
                az_ref_any_can_be_shared,
                az_ref_any_can_be_shared_mut,
                az_ref_any_increase_ref,
                az_ref_any_decrease_ref,
                az_ref_any_increase_refmut,
                az_ref_any_decrease_refmut,
                az_ref_any_delete,
                az_ref_any_deep_copy,
                az_ref_any_fmt_debug,
                az_ref_any_partial_eq,
                az_ref_any_partial_cmp,
                az_ref_any_cmp,
                az_ref_any_hash,
                az_layout_info_ptr_delete,
                az_layout_info_ptr_fmt_debug,
                az_css_rule_block_delete,
                az_css_rule_block_deep_copy,
                az_css_rule_block_fmt_debug,
                az_css_declaration_delete,
                az_css_declaration_deep_copy,
                az_css_declaration_fmt_debug,
                az_dynamic_css_property_delete,
                az_dynamic_css_property_deep_copy,
                az_dynamic_css_property_fmt_debug,
                az_css_path_delete,
                az_css_path_deep_copy,
                az_css_path_fmt_debug,
                az_css_path_selector_delete,
                az_css_path_selector_deep_copy,
                az_css_path_selector_fmt_debug,
                az_node_type_path_delete,
                az_node_type_path_deep_copy,
                az_node_type_path_fmt_debug,
                az_css_path_pseudo_selector_delete,
                az_css_path_pseudo_selector_deep_copy,
                az_css_path_pseudo_selector_fmt_debug,
                az_css_nth_child_selector_delete,
                az_css_nth_child_selector_deep_copy,
                az_css_nth_child_selector_fmt_debug,
                az_css_nth_child_pattern_delete,
                az_css_nth_child_pattern_deep_copy,
                az_css_nth_child_pattern_fmt_debug,
                az_stylesheet_delete,
                az_stylesheet_deep_copy,
                az_stylesheet_fmt_debug,
                az_css_native,
                az_css_empty,
                az_css_from_string,
                az_css_override_native,
                az_css_delete,
                az_css_deep_copy,
                az_css_fmt_debug,
                az_color_u_delete,
                az_color_u_deep_copy,
                az_color_u_fmt_debug,
                az_size_metric_delete,
                az_size_metric_deep_copy,
                az_size_metric_fmt_debug,
                az_float_value_delete,
                az_float_value_deep_copy,
                az_float_value_fmt_debug,
                az_pixel_value_delete,
                az_pixel_value_deep_copy,
                az_pixel_value_fmt_debug,
                az_pixel_value_no_percent_delete,
                az_pixel_value_no_percent_deep_copy,
                az_pixel_value_no_percent_fmt_debug,
                az_box_shadow_clip_mode_delete,
                az_box_shadow_clip_mode_deep_copy,
                az_box_shadow_clip_mode_fmt_debug,
                az_box_shadow_pre_display_item_delete,
                az_box_shadow_pre_display_item_deep_copy,
                az_box_shadow_pre_display_item_fmt_debug,
                az_layout_align_content_delete,
                az_layout_align_content_deep_copy,
                az_layout_align_content_fmt_debug,
                az_layout_align_items_delete,
                az_layout_align_items_deep_copy,
                az_layout_align_items_fmt_debug,
                az_layout_bottom_delete,
                az_layout_bottom_deep_copy,
                az_layout_bottom_fmt_debug,
                az_layout_box_sizing_delete,
                az_layout_box_sizing_deep_copy,
                az_layout_box_sizing_fmt_debug,
                az_layout_direction_delete,
                az_layout_direction_deep_copy,
                az_layout_direction_fmt_debug,
                az_layout_display_delete,
                az_layout_display_deep_copy,
                az_layout_display_fmt_debug,
                az_layout_flex_grow_delete,
                az_layout_flex_grow_deep_copy,
                az_layout_flex_grow_fmt_debug,
                az_layout_flex_shrink_delete,
                az_layout_flex_shrink_deep_copy,
                az_layout_flex_shrink_fmt_debug,
                az_layout_float_delete,
                az_layout_float_deep_copy,
                az_layout_float_fmt_debug,
                az_layout_height_delete,
                az_layout_height_deep_copy,
                az_layout_height_fmt_debug,
                az_layout_justify_content_delete,
                az_layout_justify_content_deep_copy,
                az_layout_justify_content_fmt_debug,
                az_layout_left_delete,
                az_layout_left_deep_copy,
                az_layout_left_fmt_debug,
                az_layout_margin_bottom_delete,
                az_layout_margin_bottom_deep_copy,
                az_layout_margin_bottom_fmt_debug,
                az_layout_margin_left_delete,
                az_layout_margin_left_deep_copy,
                az_layout_margin_left_fmt_debug,
                az_layout_margin_right_delete,
                az_layout_margin_right_deep_copy,
                az_layout_margin_right_fmt_debug,
                az_layout_margin_top_delete,
                az_layout_margin_top_deep_copy,
                az_layout_margin_top_fmt_debug,
                az_layout_max_height_delete,
                az_layout_max_height_deep_copy,
                az_layout_max_height_fmt_debug,
                az_layout_max_width_delete,
                az_layout_max_width_deep_copy,
                az_layout_max_width_fmt_debug,
                az_layout_min_height_delete,
                az_layout_min_height_deep_copy,
                az_layout_min_height_fmt_debug,
                az_layout_min_width_delete,
                az_layout_min_width_deep_copy,
                az_layout_min_width_fmt_debug,
                az_layout_padding_bottom_delete,
                az_layout_padding_bottom_deep_copy,
                az_layout_padding_bottom_fmt_debug,
                az_layout_padding_left_delete,
                az_layout_padding_left_deep_copy,
                az_layout_padding_left_fmt_debug,
                az_layout_padding_right_delete,
                az_layout_padding_right_deep_copy,
                az_layout_padding_right_fmt_debug,
                az_layout_padding_top_delete,
                az_layout_padding_top_deep_copy,
                az_layout_padding_top_fmt_debug,
                az_layout_position_delete,
                az_layout_position_deep_copy,
                az_layout_position_fmt_debug,
                az_layout_right_delete,
                az_layout_right_deep_copy,
                az_layout_right_fmt_debug,
                az_layout_top_delete,
                az_layout_top_deep_copy,
                az_layout_top_fmt_debug,
                az_layout_width_delete,
                az_layout_width_deep_copy,
                az_layout_width_fmt_debug,
                az_layout_wrap_delete,
                az_layout_wrap_deep_copy,
                az_layout_wrap_fmt_debug,
                az_overflow_delete,
                az_overflow_deep_copy,
                az_overflow_fmt_debug,
                az_percentage_value_delete,
                az_percentage_value_deep_copy,
                az_percentage_value_fmt_debug,
                az_gradient_stop_pre_delete,
                az_gradient_stop_pre_deep_copy,
                az_gradient_stop_pre_fmt_debug,
                az_direction_corner_delete,
                az_direction_corner_deep_copy,
                az_direction_corner_fmt_debug,
                az_direction_corners_delete,
                az_direction_corners_deep_copy,
                az_direction_corners_fmt_debug,
                az_direction_delete,
                az_direction_deep_copy,
                az_direction_fmt_debug,
                az_extend_mode_delete,
                az_extend_mode_deep_copy,
                az_extend_mode_fmt_debug,
                az_linear_gradient_delete,
                az_linear_gradient_deep_copy,
                az_linear_gradient_fmt_debug,
                az_shape_delete,
                az_shape_deep_copy,
                az_shape_fmt_debug,
                az_radial_gradient_delete,
                az_radial_gradient_deep_copy,
                az_radial_gradient_fmt_debug,
                az_css_image_id_delete,
                az_css_image_id_deep_copy,
                az_css_image_id_fmt_debug,
                az_style_background_content_delete,
                az_style_background_content_deep_copy,
                az_style_background_content_fmt_debug,
                az_background_position_horizontal_delete,
                az_background_position_horizontal_deep_copy,
                az_background_position_horizontal_fmt_debug,
                az_background_position_vertical_delete,
                az_background_position_vertical_deep_copy,
                az_background_position_vertical_fmt_debug,
                az_style_background_position_delete,
                az_style_background_position_deep_copy,
                az_style_background_position_fmt_debug,
                az_style_background_repeat_delete,
                az_style_background_repeat_deep_copy,
                az_style_background_repeat_fmt_debug,
                az_style_background_size_delete,
                az_style_background_size_deep_copy,
                az_style_background_size_fmt_debug,
                az_style_border_bottom_color_delete,
                az_style_border_bottom_color_deep_copy,
                az_style_border_bottom_color_fmt_debug,
                az_style_border_bottom_left_radius_delete,
                az_style_border_bottom_left_radius_deep_copy,
                az_style_border_bottom_left_radius_fmt_debug,
                az_style_border_bottom_right_radius_delete,
                az_style_border_bottom_right_radius_deep_copy,
                az_style_border_bottom_right_radius_fmt_debug,
                az_border_style_delete,
                az_border_style_deep_copy,
                az_border_style_fmt_debug,
                az_style_border_bottom_style_delete,
                az_style_border_bottom_style_deep_copy,
                az_style_border_bottom_style_fmt_debug,
                az_style_border_bottom_width_delete,
                az_style_border_bottom_width_deep_copy,
                az_style_border_bottom_width_fmt_debug,
                az_style_border_left_color_delete,
                az_style_border_left_color_deep_copy,
                az_style_border_left_color_fmt_debug,
                az_style_border_left_style_delete,
                az_style_border_left_style_deep_copy,
                az_style_border_left_style_fmt_debug,
                az_style_border_left_width_delete,
                az_style_border_left_width_deep_copy,
                az_style_border_left_width_fmt_debug,
                az_style_border_right_color_delete,
                az_style_border_right_color_deep_copy,
                az_style_border_right_color_fmt_debug,
                az_style_border_right_style_delete,
                az_style_border_right_style_deep_copy,
                az_style_border_right_style_fmt_debug,
                az_style_border_right_width_delete,
                az_style_border_right_width_deep_copy,
                az_style_border_right_width_fmt_debug,
                az_style_border_top_color_delete,
                az_style_border_top_color_deep_copy,
                az_style_border_top_color_fmt_debug,
                az_style_border_top_left_radius_delete,
                az_style_border_top_left_radius_deep_copy,
                az_style_border_top_left_radius_fmt_debug,
                az_style_border_top_right_radius_delete,
                az_style_border_top_right_radius_deep_copy,
                az_style_border_top_right_radius_fmt_debug,
                az_style_border_top_style_delete,
                az_style_border_top_style_deep_copy,
                az_style_border_top_style_fmt_debug,
                az_style_border_top_width_delete,
                az_style_border_top_width_deep_copy,
                az_style_border_top_width_fmt_debug,
                az_style_cursor_delete,
                az_style_cursor_deep_copy,
                az_style_cursor_fmt_debug,
                az_style_font_family_delete,
                az_style_font_family_deep_copy,
                az_style_font_family_fmt_debug,
                az_style_font_size_delete,
                az_style_font_size_deep_copy,
                az_style_font_size_fmt_debug,
                az_style_letter_spacing_delete,
                az_style_letter_spacing_deep_copy,
                az_style_letter_spacing_fmt_debug,
                az_style_line_height_delete,
                az_style_line_height_deep_copy,
                az_style_line_height_fmt_debug,
                az_style_tab_width_delete,
                az_style_tab_width_deep_copy,
                az_style_tab_width_fmt_debug,
                az_style_text_alignment_horz_delete,
                az_style_text_alignment_horz_deep_copy,
                az_style_text_alignment_horz_fmt_debug,
                az_style_text_color_delete,
                az_style_text_color_deep_copy,
                az_style_text_color_fmt_debug,
                az_style_word_spacing_delete,
                az_style_word_spacing_deep_copy,
                az_style_word_spacing_fmt_debug,
                az_box_shadow_pre_display_item_value_delete,
                az_box_shadow_pre_display_item_value_deep_copy,
                az_box_shadow_pre_display_item_value_fmt_debug,
                az_layout_align_content_value_delete,
                az_layout_align_content_value_deep_copy,
                az_layout_align_content_value_fmt_debug,
                az_layout_align_items_value_delete,
                az_layout_align_items_value_deep_copy,
                az_layout_align_items_value_fmt_debug,
                az_layout_bottom_value_delete,
                az_layout_bottom_value_deep_copy,
                az_layout_bottom_value_fmt_debug,
                az_layout_box_sizing_value_delete,
                az_layout_box_sizing_value_deep_copy,
                az_layout_box_sizing_value_fmt_debug,
                az_layout_direction_value_delete,
                az_layout_direction_value_deep_copy,
                az_layout_direction_value_fmt_debug,
                az_layout_display_value_delete,
                az_layout_display_value_deep_copy,
                az_layout_display_value_fmt_debug,
                az_layout_flex_grow_value_delete,
                az_layout_flex_grow_value_deep_copy,
                az_layout_flex_grow_value_fmt_debug,
                az_layout_flex_shrink_value_delete,
                az_layout_flex_shrink_value_deep_copy,
                az_layout_flex_shrink_value_fmt_debug,
                az_layout_float_value_delete,
                az_layout_float_value_deep_copy,
                az_layout_float_value_fmt_debug,
                az_layout_height_value_delete,
                az_layout_height_value_deep_copy,
                az_layout_height_value_fmt_debug,
                az_layout_justify_content_value_delete,
                az_layout_justify_content_value_deep_copy,
                az_layout_justify_content_value_fmt_debug,
                az_layout_left_value_delete,
                az_layout_left_value_deep_copy,
                az_layout_left_value_fmt_debug,
                az_layout_margin_bottom_value_delete,
                az_layout_margin_bottom_value_deep_copy,
                az_layout_margin_bottom_value_fmt_debug,
                az_layout_margin_left_value_delete,
                az_layout_margin_left_value_deep_copy,
                az_layout_margin_left_value_fmt_debug,
                az_layout_margin_right_value_delete,
                az_layout_margin_right_value_deep_copy,
                az_layout_margin_right_value_fmt_debug,
                az_layout_margin_top_value_delete,
                az_layout_margin_top_value_deep_copy,
                az_layout_margin_top_value_fmt_debug,
                az_layout_max_height_value_delete,
                az_layout_max_height_value_deep_copy,
                az_layout_max_height_value_fmt_debug,
                az_layout_max_width_value_delete,
                az_layout_max_width_value_deep_copy,
                az_layout_max_width_value_fmt_debug,
                az_layout_min_height_value_delete,
                az_layout_min_height_value_deep_copy,
                az_layout_min_height_value_fmt_debug,
                az_layout_min_width_value_delete,
                az_layout_min_width_value_deep_copy,
                az_layout_min_width_value_fmt_debug,
                az_layout_padding_bottom_value_delete,
                az_layout_padding_bottom_value_deep_copy,
                az_layout_padding_bottom_value_fmt_debug,
                az_layout_padding_left_value_delete,
                az_layout_padding_left_value_deep_copy,
                az_layout_padding_left_value_fmt_debug,
                az_layout_padding_right_value_delete,
                az_layout_padding_right_value_deep_copy,
                az_layout_padding_right_value_fmt_debug,
                az_layout_padding_top_value_delete,
                az_layout_padding_top_value_deep_copy,
                az_layout_padding_top_value_fmt_debug,
                az_layout_position_value_delete,
                az_layout_position_value_deep_copy,
                az_layout_position_value_fmt_debug,
                az_layout_right_value_delete,
                az_layout_right_value_deep_copy,
                az_layout_right_value_fmt_debug,
                az_layout_top_value_delete,
                az_layout_top_value_deep_copy,
                az_layout_top_value_fmt_debug,
                az_layout_width_value_delete,
                az_layout_width_value_deep_copy,
                az_layout_width_value_fmt_debug,
                az_layout_wrap_value_delete,
                az_layout_wrap_value_deep_copy,
                az_layout_wrap_value_fmt_debug,
                az_overflow_value_delete,
                az_overflow_value_deep_copy,
                az_overflow_value_fmt_debug,
                az_style_background_content_value_delete,
                az_style_background_content_value_deep_copy,
                az_style_background_content_value_fmt_debug,
                az_style_background_position_value_delete,
                az_style_background_position_value_deep_copy,
                az_style_background_position_value_fmt_debug,
                az_style_background_repeat_value_delete,
                az_style_background_repeat_value_deep_copy,
                az_style_background_repeat_value_fmt_debug,
                az_style_background_size_value_delete,
                az_style_background_size_value_deep_copy,
                az_style_background_size_value_fmt_debug,
                az_style_border_bottom_color_value_delete,
                az_style_border_bottom_color_value_deep_copy,
                az_style_border_bottom_color_value_fmt_debug,
                az_style_border_bottom_left_radius_value_delete,
                az_style_border_bottom_left_radius_value_deep_copy,
                az_style_border_bottom_left_radius_value_fmt_debug,
                az_style_border_bottom_right_radius_value_delete,
                az_style_border_bottom_right_radius_value_deep_copy,
                az_style_border_bottom_right_radius_value_fmt_debug,
                az_style_border_bottom_style_value_delete,
                az_style_border_bottom_style_value_deep_copy,
                az_style_border_bottom_style_value_fmt_debug,
                az_style_border_bottom_width_value_delete,
                az_style_border_bottom_width_value_deep_copy,
                az_style_border_bottom_width_value_fmt_debug,
                az_style_border_left_color_value_delete,
                az_style_border_left_color_value_deep_copy,
                az_style_border_left_color_value_fmt_debug,
                az_style_border_left_style_value_delete,
                az_style_border_left_style_value_deep_copy,
                az_style_border_left_style_value_fmt_debug,
                az_style_border_left_width_value_delete,
                az_style_border_left_width_value_deep_copy,
                az_style_border_left_width_value_fmt_debug,
                az_style_border_right_color_value_delete,
                az_style_border_right_color_value_deep_copy,
                az_style_border_right_color_value_fmt_debug,
                az_style_border_right_style_value_delete,
                az_style_border_right_style_value_deep_copy,
                az_style_border_right_style_value_fmt_debug,
                az_style_border_right_width_value_delete,
                az_style_border_right_width_value_deep_copy,
                az_style_border_right_width_value_fmt_debug,
                az_style_border_top_color_value_delete,
                az_style_border_top_color_value_deep_copy,
                az_style_border_top_color_value_fmt_debug,
                az_style_border_top_left_radius_value_delete,
                az_style_border_top_left_radius_value_deep_copy,
                az_style_border_top_left_radius_value_fmt_debug,
                az_style_border_top_right_radius_value_delete,
                az_style_border_top_right_radius_value_deep_copy,
                az_style_border_top_right_radius_value_fmt_debug,
                az_style_border_top_style_value_delete,
                az_style_border_top_style_value_deep_copy,
                az_style_border_top_style_value_fmt_debug,
                az_style_border_top_width_value_delete,
                az_style_border_top_width_value_deep_copy,
                az_style_border_top_width_value_fmt_debug,
                az_style_cursor_value_delete,
                az_style_cursor_value_deep_copy,
                az_style_cursor_value_fmt_debug,
                az_style_font_family_value_delete,
                az_style_font_family_value_deep_copy,
                az_style_font_family_value_fmt_debug,
                az_style_font_size_value_delete,
                az_style_font_size_value_deep_copy,
                az_style_font_size_value_fmt_debug,
                az_style_letter_spacing_value_delete,
                az_style_letter_spacing_value_deep_copy,
                az_style_letter_spacing_value_fmt_debug,
                az_style_line_height_value_delete,
                az_style_line_height_value_deep_copy,
                az_style_line_height_value_fmt_debug,
                az_style_tab_width_value_delete,
                az_style_tab_width_value_deep_copy,
                az_style_tab_width_value_fmt_debug,
                az_style_text_alignment_horz_value_delete,
                az_style_text_alignment_horz_value_deep_copy,
                az_style_text_alignment_horz_value_fmt_debug,
                az_style_text_color_value_delete,
                az_style_text_color_value_deep_copy,
                az_style_text_color_value_fmt_debug,
                az_style_word_spacing_value_delete,
                az_style_word_spacing_value_deep_copy,
                az_style_word_spacing_value_fmt_debug,
                az_css_property_delete,
                az_css_property_deep_copy,
                az_css_property_fmt_debug,
                az_dom_new,
                az_dom_div,
                az_dom_body,
                az_dom_label,
                az_dom_text,
                az_dom_image,
                az_dom_gl_texture,
                az_dom_iframe,
                az_dom_add_id,
                az_dom_with_id,
                az_dom_set_ids,
                az_dom_with_ids,
                az_dom_add_class,
                az_dom_with_class,
                az_dom_set_classes,
                az_dom_with_classes,
                az_dom_add_callback,
                az_dom_with_callback,
                az_dom_add_css_override,
                az_dom_with_css_override,
                az_dom_set_is_draggable,
                az_dom_is_draggable,
                az_dom_set_tab_index,
                az_dom_with_tab_index,
                az_dom_has_id,
                az_dom_has_class,
                az_dom_add_child,
                az_dom_with_child,
                az_dom_get_html_string,
                az_dom_delete,
                az_dom_deep_copy,
                az_dom_fmt_debug,
                az_gl_texture_node_delete,
                az_gl_texture_node_deep_copy,
                az_gl_texture_node_fmt_debug,
                az_i_frame_node_delete,
                az_i_frame_node_deep_copy,
                az_i_frame_node_fmt_debug,
                az_callback_data_delete,
                az_callback_data_deep_copy,
                az_callback_data_fmt_debug,
                az_override_property_delete,
                az_override_property_deep_copy,
                az_override_property_fmt_debug,
                az_node_data_new,
                az_node_data_div,
                az_node_data_body,
                az_node_data_label,
                az_node_data_text,
                az_node_data_image,
                az_node_data_gl_texture,
                az_node_data_iframe,
                az_node_data_default,
                az_node_data_add_id,
                az_node_data_with_id,
                az_node_data_set_ids,
                az_node_data_with_ids,
                az_node_data_add_class,
                az_node_data_with_class,
                az_node_data_set_classes,
                az_node_data_with_classes,
                az_node_data_add_callback,
                az_node_data_with_callback,
                az_node_data_add_css_override,
                az_node_data_with_css_override,
                az_node_data_set_is_draggable,
                az_node_data_is_draggable,
                az_node_data_set_tab_index,
                az_node_data_with_tab_index,
                az_node_data_has_id,
                az_node_data_has_class,
                az_node_data_delete,
                az_node_data_deep_copy,
                az_node_data_fmt_debug,
                az_node_type_delete,
                az_node_type_deep_copy,
                az_node_type_fmt_debug,
                az_on_into_event_filter,
                az_on_delete,
                az_on_deep_copy,
                az_on_fmt_debug,
                az_event_filter_delete,
                az_event_filter_deep_copy,
                az_event_filter_fmt_debug,
                az_hover_event_filter_delete,
                az_hover_event_filter_deep_copy,
                az_hover_event_filter_fmt_debug,
                az_focus_event_filter_delete,
                az_focus_event_filter_deep_copy,
                az_focus_event_filter_fmt_debug,
                az_not_event_filter_delete,
                az_not_event_filter_deep_copy,
                az_not_event_filter_fmt_debug,
                az_window_event_filter_delete,
                az_window_event_filter_deep_copy,
                az_window_event_filter_fmt_debug,
                az_tab_index_delete,
                az_tab_index_deep_copy,
                az_tab_index_fmt_debug,
                az_gl_type_delete,
                az_gl_type_deep_copy,
                az_gl_type_fmt_debug,
                az_debug_message_delete,
                az_debug_message_deep_copy,
                az_debug_message_fmt_debug,
                az_u8_vec_ref_delete,
                az_u8_vec_ref_fmt_debug,
                az_u8_vec_ref_mut_delete,
                az_u8_vec_ref_mut_fmt_debug,
                az_f32_vec_ref_delete,
                az_f32_vec_ref_fmt_debug,
                az_i32_vec_ref_delete,
                az_i32_vec_ref_fmt_debug,
                az_g_luint_vec_ref_delete,
                az_g_luint_vec_ref_fmt_debug,
                az_g_lenum_vec_ref_delete,
                az_g_lenum_vec_ref_fmt_debug,
                az_g_lint_vec_ref_mut_delete,
                az_g_lint_vec_ref_mut_fmt_debug,
                az_g_lint64_vec_ref_mut_delete,
                az_g_lint64_vec_ref_mut_fmt_debug,
                az_g_lboolean_vec_ref_mut_delete,
                az_g_lboolean_vec_ref_mut_fmt_debug,
                az_g_lfloat_vec_ref_mut_delete,
                az_g_lfloat_vec_ref_mut_fmt_debug,
                az_refstr_vec_ref_delete,
                az_refstr_vec_ref_fmt_debug,
                az_refstr_delete,
                az_refstr_fmt_debug,
                az_get_program_binary_return_delete,
                az_get_program_binary_return_deep_copy,
                az_get_program_binary_return_fmt_debug,
                az_get_active_attrib_return_delete,
                az_get_active_attrib_return_deep_copy,
                az_get_active_attrib_return_fmt_debug,
                az_g_lsync_ptr_delete,
                az_g_lsync_ptr_fmt_debug,
                az_get_active_uniform_return_delete,
                az_get_active_uniform_return_deep_copy,
                az_get_active_uniform_return_fmt_debug,
                az_gl_context_ptr_get_type,
                az_gl_context_ptr_buffer_data_untyped,
                az_gl_context_ptr_buffer_sub_data_untyped,
                az_gl_context_ptr_map_buffer,
                az_gl_context_ptr_map_buffer_range,
                az_gl_context_ptr_unmap_buffer,
                az_gl_context_ptr_tex_buffer,
                az_gl_context_ptr_shader_source,
                az_gl_context_ptr_read_buffer,
                az_gl_context_ptr_read_pixels_into_buffer,
                az_gl_context_ptr_read_pixels,
                az_gl_context_ptr_read_pixels_into_pbo,
                az_gl_context_ptr_sample_coverage,
                az_gl_context_ptr_polygon_offset,
                az_gl_context_ptr_pixel_store_i,
                az_gl_context_ptr_gen_buffers,
                az_gl_context_ptr_gen_renderbuffers,
                az_gl_context_ptr_gen_framebuffers,
                az_gl_context_ptr_gen_textures,
                az_gl_context_ptr_gen_vertex_arrays,
                az_gl_context_ptr_gen_queries,
                az_gl_context_ptr_begin_query,
                az_gl_context_ptr_end_query,
                az_gl_context_ptr_query_counter,
                az_gl_context_ptr_get_query_object_iv,
                az_gl_context_ptr_get_query_object_uiv,
                az_gl_context_ptr_get_query_object_i64v,
                az_gl_context_ptr_get_query_object_ui64v,
                az_gl_context_ptr_delete_queries,
                az_gl_context_ptr_delete_vertex_arrays,
                az_gl_context_ptr_delete_buffers,
                az_gl_context_ptr_delete_renderbuffers,
                az_gl_context_ptr_delete_framebuffers,
                az_gl_context_ptr_delete_textures,
                az_gl_context_ptr_framebuffer_renderbuffer,
                az_gl_context_ptr_renderbuffer_storage,
                az_gl_context_ptr_depth_func,
                az_gl_context_ptr_active_texture,
                az_gl_context_ptr_attach_shader,
                az_gl_context_ptr_bind_attrib_location,
                az_gl_context_ptr_get_uniform_iv,
                az_gl_context_ptr_get_uniform_fv,
                az_gl_context_ptr_get_uniform_block_index,
                az_gl_context_ptr_get_uniform_indices,
                az_gl_context_ptr_bind_buffer_base,
                az_gl_context_ptr_bind_buffer_range,
                az_gl_context_ptr_uniform_block_binding,
                az_gl_context_ptr_bind_buffer,
                az_gl_context_ptr_bind_vertex_array,
                az_gl_context_ptr_bind_renderbuffer,
                az_gl_context_ptr_bind_framebuffer,
                az_gl_context_ptr_bind_texture,
                az_gl_context_ptr_draw_buffers,
                az_gl_context_ptr_tex_image_2d,
                az_gl_context_ptr_compressed_tex_image_2d,
                az_gl_context_ptr_compressed_tex_sub_image_2d,
                az_gl_context_ptr_tex_image_3d,
                az_gl_context_ptr_copy_tex_image_2d,
                az_gl_context_ptr_copy_tex_sub_image_2d,
                az_gl_context_ptr_copy_tex_sub_image_3d,
                az_gl_context_ptr_tex_sub_image_2d,
                az_gl_context_ptr_tex_sub_image_2d_pbo,
                az_gl_context_ptr_tex_sub_image_3d,
                az_gl_context_ptr_tex_sub_image_3d_pbo,
                az_gl_context_ptr_tex_storage_2d,
                az_gl_context_ptr_tex_storage_3d,
                az_gl_context_ptr_get_tex_image_into_buffer,
                az_gl_context_ptr_copy_image_sub_data,
                az_gl_context_ptr_invalidate_framebuffer,
                az_gl_context_ptr_invalidate_sub_framebuffer,
                az_gl_context_ptr_get_integer_v,
                az_gl_context_ptr_get_integer_64v,
                az_gl_context_ptr_get_integer_iv,
                az_gl_context_ptr_get_integer_64iv,
                az_gl_context_ptr_get_boolean_v,
                az_gl_context_ptr_get_float_v,
                az_gl_context_ptr_get_framebuffer_attachment_parameter_iv,
                az_gl_context_ptr_get_renderbuffer_parameter_iv,
                az_gl_context_ptr_get_tex_parameter_iv,
                az_gl_context_ptr_get_tex_parameter_fv,
                az_gl_context_ptr_tex_parameter_i,
                az_gl_context_ptr_tex_parameter_f,
                az_gl_context_ptr_framebuffer_texture_2d,
                az_gl_context_ptr_framebuffer_texture_layer,
                az_gl_context_ptr_blit_framebuffer,
                az_gl_context_ptr_vertex_attrib_4f,
                az_gl_context_ptr_vertex_attrib_pointer_f32,
                az_gl_context_ptr_vertex_attrib_pointer,
                az_gl_context_ptr_vertex_attrib_i_pointer,
                az_gl_context_ptr_vertex_attrib_divisor,
                az_gl_context_ptr_viewport,
                az_gl_context_ptr_scissor,
                az_gl_context_ptr_line_width,
                az_gl_context_ptr_use_program,
                az_gl_context_ptr_validate_program,
                az_gl_context_ptr_draw_arrays,
                az_gl_context_ptr_draw_arrays_instanced,
                az_gl_context_ptr_draw_elements,
                az_gl_context_ptr_draw_elements_instanced,
                az_gl_context_ptr_blend_color,
                az_gl_context_ptr_blend_func,
                az_gl_context_ptr_blend_func_separate,
                az_gl_context_ptr_blend_equation,
                az_gl_context_ptr_blend_equation_separate,
                az_gl_context_ptr_color_mask,
                az_gl_context_ptr_cull_face,
                az_gl_context_ptr_front_face,
                az_gl_context_ptr_enable,
                az_gl_context_ptr_disable,
                az_gl_context_ptr_hint,
                az_gl_context_ptr_is_enabled,
                az_gl_context_ptr_is_shader,
                az_gl_context_ptr_is_texture,
                az_gl_context_ptr_is_framebuffer,
                az_gl_context_ptr_is_renderbuffer,
                az_gl_context_ptr_check_frame_buffer_status,
                az_gl_context_ptr_enable_vertex_attrib_array,
                az_gl_context_ptr_disable_vertex_attrib_array,
                az_gl_context_ptr_uniform_1f,
                az_gl_context_ptr_uniform_1fv,
                az_gl_context_ptr_uniform_1i,
                az_gl_context_ptr_uniform_1iv,
                az_gl_context_ptr_uniform_1ui,
                az_gl_context_ptr_uniform_2f,
                az_gl_context_ptr_uniform_2fv,
                az_gl_context_ptr_uniform_2i,
                az_gl_context_ptr_uniform_2iv,
                az_gl_context_ptr_uniform_2ui,
                az_gl_context_ptr_uniform_3f,
                az_gl_context_ptr_uniform_3fv,
                az_gl_context_ptr_uniform_3i,
                az_gl_context_ptr_uniform_3iv,
                az_gl_context_ptr_uniform_3ui,
                az_gl_context_ptr_uniform_4f,
                az_gl_context_ptr_uniform_4i,
                az_gl_context_ptr_uniform_4iv,
                az_gl_context_ptr_uniform_4ui,
                az_gl_context_ptr_uniform_4fv,
                az_gl_context_ptr_uniform_matrix_2fv,
                az_gl_context_ptr_uniform_matrix_3fv,
                az_gl_context_ptr_uniform_matrix_4fv,
                az_gl_context_ptr_depth_mask,
                az_gl_context_ptr_depth_range,
                az_gl_context_ptr_get_active_attrib,
                az_gl_context_ptr_get_active_uniform,
                az_gl_context_ptr_get_active_uniforms_iv,
                az_gl_context_ptr_get_active_uniform_block_i,
                az_gl_context_ptr_get_active_uniform_block_iv,
                az_gl_context_ptr_get_active_uniform_block_name,
                az_gl_context_ptr_get_attrib_location,
                az_gl_context_ptr_get_frag_data_location,
                az_gl_context_ptr_get_uniform_location,
                az_gl_context_ptr_get_program_info_log,
                az_gl_context_ptr_get_program_iv,
                az_gl_context_ptr_get_program_binary,
                az_gl_context_ptr_program_binary,
                az_gl_context_ptr_program_parameter_i,
                az_gl_context_ptr_get_vertex_attrib_iv,
                az_gl_context_ptr_get_vertex_attrib_fv,
                az_gl_context_ptr_get_vertex_attrib_pointer_v,
                az_gl_context_ptr_get_buffer_parameter_iv,
                az_gl_context_ptr_get_shader_info_log,
                az_gl_context_ptr_get_string,
                az_gl_context_ptr_get_string_i,
                az_gl_context_ptr_get_shader_iv,
                az_gl_context_ptr_get_shader_precision_format,
                az_gl_context_ptr_compile_shader,
                az_gl_context_ptr_create_program,
                az_gl_context_ptr_delete_program,
                az_gl_context_ptr_create_shader,
                az_gl_context_ptr_delete_shader,
                az_gl_context_ptr_detach_shader,
                az_gl_context_ptr_link_program,
                az_gl_context_ptr_clear_color,
                az_gl_context_ptr_clear,
                az_gl_context_ptr_clear_depth,
                az_gl_context_ptr_clear_stencil,
                az_gl_context_ptr_flush,
                az_gl_context_ptr_finish,
                az_gl_context_ptr_get_error,
                az_gl_context_ptr_stencil_mask,
                az_gl_context_ptr_stencil_mask_separate,
                az_gl_context_ptr_stencil_func,
                az_gl_context_ptr_stencil_func_separate,
                az_gl_context_ptr_stencil_op,
                az_gl_context_ptr_stencil_op_separate,
                az_gl_context_ptr_egl_image_target_texture2d_oes,
                az_gl_context_ptr_generate_mipmap,
                az_gl_context_ptr_insert_event_marker_ext,
                az_gl_context_ptr_push_group_marker_ext,
                az_gl_context_ptr_pop_group_marker_ext,
                az_gl_context_ptr_debug_message_insert_khr,
                az_gl_context_ptr_push_debug_group_khr,
                az_gl_context_ptr_pop_debug_group_khr,
                az_gl_context_ptr_fence_sync,
                az_gl_context_ptr_client_wait_sync,
                az_gl_context_ptr_wait_sync,
                az_gl_context_ptr_delete_sync,
                az_gl_context_ptr_texture_range_apple,
                az_gl_context_ptr_gen_fences_apple,
                az_gl_context_ptr_delete_fences_apple,
                az_gl_context_ptr_set_fence_apple,
                az_gl_context_ptr_finish_fence_apple,
                az_gl_context_ptr_test_fence_apple,
                az_gl_context_ptr_test_object_apple,
                az_gl_context_ptr_finish_object_apple,
                az_gl_context_ptr_get_frag_data_index,
                az_gl_context_ptr_blend_barrier_khr,
                az_gl_context_ptr_bind_frag_data_location_indexed,
                az_gl_context_ptr_get_debug_messages,
                az_gl_context_ptr_provoking_vertex_angle,
                az_gl_context_ptr_gen_vertex_arrays_apple,
                az_gl_context_ptr_bind_vertex_array_apple,
                az_gl_context_ptr_delete_vertex_arrays_apple,
                az_gl_context_ptr_copy_texture_chromium,
                az_gl_context_ptr_copy_sub_texture_chromium,
                az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes,
                az_gl_context_ptr_copy_texture_3d_angle,
                az_gl_context_ptr_copy_sub_texture_3d_angle,
                az_gl_context_ptr_delete,
                az_gl_context_ptr_deep_copy,
                az_gl_context_ptr_fmt_debug,
                az_texture_delete,
                az_texture_fmt_debug,
                az_texture_flags_delete,
                az_texture_flags_deep_copy,
                az_texture_flags_fmt_debug,
                az_text_id_new,
                az_text_id_delete,
                az_text_id_deep_copy,
                az_text_id_fmt_debug,
                az_image_id_new,
                az_image_id_delete,
                az_image_id_deep_copy,
                az_image_id_fmt_debug,
                az_image_id_partial_eq,
                az_image_id_partial_cmp,
                az_image_id_cmp,
                az_image_id_hash,
                az_font_id_new,
                az_font_id_delete,
                az_font_id_deep_copy,
                az_font_id_fmt_debug,
                az_image_source_delete,
                az_image_source_deep_copy,
                az_image_source_fmt_debug,
                az_font_source_delete,
                az_font_source_deep_copy,
                az_font_source_fmt_debug,
                az_raw_image_new,
                az_raw_image_delete,
                az_raw_image_deep_copy,
                az_raw_image_fmt_debug,
                az_raw_image_format_delete,
                az_raw_image_format_deep_copy,
                az_raw_image_format_fmt_debug,
                az_drop_check_ptr_ptr_delete,
                az_drop_check_ptr_ptr_fmt_debug,
                az_arc_mutex_ref_any_ptr_delete,
                az_arc_mutex_ref_any_ptr_fmt_debug,
                az_timer_delete,
                az_timer_deep_copy,
                az_timer_fmt_debug,
                az_task_ptr_new,
                az_task_ptr_then,
                az_task_ptr_delete,
                az_task_ptr_fmt_debug,
                az_thread_ptr_new,
                az_thread_ptr_block,
                az_thread_ptr_delete,
                az_thread_ptr_fmt_debug,
                az_drop_check_ptr_delete,
                az_drop_check_ptr_fmt_debug,
                az_timer_id_delete,
                az_timer_id_deep_copy,
                az_timer_id_fmt_debug,
                az_terminate_timer_delete,
                az_terminate_timer_deep_copy,
                az_terminate_timer_fmt_debug,
                az_block_error_delete,
                az_block_error_deep_copy,
                az_block_error_fmt_debug,
                az_task_bar_icon_delete,
                az_task_bar_icon_deep_copy,
                az_task_bar_icon_fmt_debug,
                az_x_window_type_delete,
                az_x_window_type_deep_copy,
                az_x_window_type_fmt_debug,
                az_physical_position_i32_delete,
                az_physical_position_i32_deep_copy,
                az_physical_position_i32_fmt_debug,
                az_physical_position_i32_partial_eq,
                az_physical_position_i32_partial_cmp,
                az_physical_position_i32_cmp,
                az_physical_position_i32_hash,
                az_physical_size_u32_delete,
                az_physical_size_u32_deep_copy,
                az_physical_size_u32_fmt_debug,
                az_physical_size_u32_partial_eq,
                az_physical_size_u32_partial_cmp,
                az_physical_size_u32_cmp,
                az_physical_size_u32_hash,
                az_logical_position_delete,
                az_logical_position_deep_copy,
                az_logical_position_fmt_debug,
                az_icon_key_delete,
                az_icon_key_deep_copy,
                az_icon_key_fmt_debug,
                az_small_window_icon_bytes_delete,
                az_small_window_icon_bytes_deep_copy,
                az_small_window_icon_bytes_fmt_debug,
                az_large_window_icon_bytes_delete,
                az_large_window_icon_bytes_deep_copy,
                az_large_window_icon_bytes_fmt_debug,
                az_window_icon_delete,
                az_window_icon_deep_copy,
                az_window_icon_fmt_debug,
                az_virtual_key_code_delete,
                az_virtual_key_code_deep_copy,
                az_virtual_key_code_fmt_debug,
                az_accelerator_key_delete,
                az_accelerator_key_deep_copy,
                az_accelerator_key_fmt_debug,
                az_window_size_delete,
                az_window_size_deep_copy,
                az_window_size_fmt_debug,
                az_window_flags_delete,
                az_window_flags_deep_copy,
                az_window_flags_fmt_debug,
                az_debug_state_delete,
                az_debug_state_deep_copy,
                az_debug_state_fmt_debug,
                az_keyboard_state_delete,
                az_keyboard_state_deep_copy,
                az_keyboard_state_fmt_debug,
                az_mouse_cursor_type_delete,
                az_mouse_cursor_type_deep_copy,
                az_mouse_cursor_type_fmt_debug,
                az_cursor_position_delete,
                az_cursor_position_deep_copy,
                az_cursor_position_fmt_debug,
                az_mouse_state_delete,
                az_mouse_state_deep_copy,
                az_mouse_state_fmt_debug,
                az_platform_specific_options_delete,
                az_platform_specific_options_deep_copy,
                az_platform_specific_options_fmt_debug,
                az_windows_window_options_delete,
                az_windows_window_options_deep_copy,
                az_windows_window_options_fmt_debug,
                az_wayland_theme_delete,
                az_wayland_theme_deep_copy,
                az_wayland_theme_fmt_debug,
                az_renderer_type_delete,
                az_renderer_type_deep_copy,
                az_renderer_type_fmt_debug,
                az_string_pair_delete,
                az_string_pair_deep_copy,
                az_string_pair_fmt_debug,
                az_linux_window_options_delete,
                az_linux_window_options_deep_copy,
                az_linux_window_options_fmt_debug,
                az_mac_window_options_delete,
                az_mac_window_options_deep_copy,
                az_mac_window_options_fmt_debug,
                az_wasm_window_options_delete,
                az_wasm_window_options_deep_copy,
                az_wasm_window_options_fmt_debug,
                az_full_screen_mode_delete,
                az_full_screen_mode_deep_copy,
                az_full_screen_mode_fmt_debug,
                az_window_state_delete,
                az_window_state_deep_copy,
                az_window_state_fmt_debug,
                az_logical_size_delete,
                az_logical_size_deep_copy,
                az_logical_size_fmt_debug,
                az_hot_reload_options_delete,
                az_hot_reload_options_deep_copy,
                az_hot_reload_options_fmt_debug,
                az_window_create_options_new,
                az_window_create_options_delete,
                az_window_create_options_deep_copy,
                az_window_create_options_fmt_debug,
            })
        }

    }

    #[cfg(unix)]
    const LIB_BYTES: &[u8] = include_bytes!(concat!(env!("CARGO_HOME"), "/lib/", "azul-dll-", env!("CARGO_PKG_VERSION"), "/target/release/libazul.so")); /* !!! IF THIS LINE SHOWS AN ERROR, IT MEANS YOU FORGOT TO RUN "cargo install --version 0.1.0 azul-dll" */
    #[cfg(windows)]
    const LIB_BYTES: &[u8] = include_bytes!(concat!(env!("CARGO_HOME"), "/lib/", "azul-dll-", env!("CARGO_PKG_VERSION", "/target/release/azul.dll"))); /* !!! IF THIS LINE SHOWS AN ERROR, IT MEANS YOU FORGOT TO RUN "cargo install --version 0.1.0 azul-dll" */

    use std::{mem::MaybeUninit, sync::atomic::{AtomicBool, Ordering}};

    static LIBRARY_IS_INITIALIZED: AtomicBool = AtomicBool::new(false);
    static mut AZUL_DLL: MaybeUninit<AzulDll> = MaybeUninit::<AzulDll>::uninit();

    #[cfg(unix)]
    const DLL_FILE_NAME: &str = "azul.so";
    #[cfg(windows)]
    const DLL_FILE_NAME: &str = "azul.dll";

    fn load_library_inner() -> Result<AzulDll, &'static str> {

        let current_exe_path = std::env::current_exe().map_err(|_| "current exe has no current dir (?!)")?;
        let mut library_path = current_exe_path.parent().ok_or("current exe has no parent (?!)")?.to_path_buf();
        library_path.push(DLL_FILE_NAME);

        if !library_path.exists() {
           std::fs::write(&library_path, LIB_BYTES).map_err(|_| "could not unpack DLL")?;
        }

        initialize_library(&library_path).ok_or("could not initialize library")
    }

    pub(crate) fn get_azul_dll() -> &'static AzulDll { 
        if !LIBRARY_IS_INITIALIZED.load(Ordering::SeqCst) {
           match load_library_inner() {
               Ok(s) => {
                   unsafe { AZUL_DLL = MaybeUninit::new(s) };
                   LIBRARY_IS_INITIALIZED.store(true, Ordering::SeqCst);
               },
               Err(e) => { println!("failed to initialize libazul dll: missing function {}", e); std::process::exit(-1); }
           }
        }

        unsafe { &*AZUL_DLL.as_ptr() }
    }
