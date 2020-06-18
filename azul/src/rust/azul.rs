#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]


//! Auto-generated public Rust API for the Azul GUI toolkit version 0.1.0
//!

// Copyright 2017 Maps4Print Einzelunternehmung
// 
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
// 
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
// 
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.


extern crate libloading;
pub(crate) mod dll {

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
            impl $struct_name {
                pub fn into_option(self) -> Option<$struct_type> {
                    match self {
                        $struct_name::None => None,
                        $struct_name::Some(t) => Some(t),
                    }
                }
            }

            impl_option_inner!($struct_type, $struct_name);
        );
        ($struct_type:ident, $struct_name:ident, copy = false, [$($derive:meta),* ]) => (
            impl $struct_name {
                pub fn into_option(&self) -> Option<$struct_type> {
                    match self {
                        $struct_name::None => None,
                        $struct_name::Some(t) => Some(t.clone()),
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

            impl_option_inner!($struct_type, $struct_name);
        );
    }

    impl_option!(AzTabIndex, AzOptionTabIndex, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzDom, AzOptionDom, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

    impl_option!(AzU8VecRef, AzOptionU8VecRef, copy = false, clone = false, [Debug, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzTexture, AzOptionTexture, copy = false, clone = false, [Debug, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(usize, AzOptionUsize, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

    impl_option!(AzInstantPtr, AzOptionInstantPtr, copy = false, clone = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzDuration, AzOptionDuration, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

    impl_option!(char, AzOptionChar, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzVirtualKeyCode, AzOptionVirtualKeyCode, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(i32, AzOptionI32, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(f32, AzOptionF32, [Debug, Copy, Clone, PartialEq, PartialOrd]);
    impl_option!(AzMouseCursorType, AzOptionMouseCursorType, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzString, AzOptionString, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    pub type AzHwndHandle = *mut c_void;
    impl_option!(AzHwndHandle, AzOptionHwndHandle, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    pub type AzX11Visual = *const c_void;
    impl_option!(AzX11Visual, AzOptionX11Visual, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzWaylandTheme, AzOptionWaylandTheme, [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzHotReloadOptions, AzOptionHotReloadOptions, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
    impl_option!(AzLogicalPosition, AzOptionLogicalPosition, [Debug, Copy, Clone, PartialEq, PartialOrd]);
    impl_option!(AzLogicalSize, AzOptionLogicalSize, [Debug, Copy, Clone, PartialEq, PartialOrd]);
    impl_option!(AzPhysicalPositionI32, AzOptionPhysicalPositionI32, [Debug, Copy, Clone, PartialEq, PartialOrd]);
    impl_option!(AzWindowIcon, AzOptionWindowIcon, copy = false, [Debug, Clone, PartialOrd, PartialEq, Eq, Hash, Ord]);
    impl_option!(AzTaskBarIcon, AzOptionTaskBarIcon, copy = false, [Debug, Clone, PartialOrd, PartialEq, Eq, Hash, Ord]);
    /// Re-export of rust-allocated (stack based) `String` struct
    #[repr(C)] pub struct AzString {
        pub vec: AzU8Vec,
    }
    /// Wrapper over a Rust-allocated `XWindowType`
    #[repr(C)] pub struct AzXWindowTypeVec {
        pub(crate) ptr: *const AzXWindowType,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `VirtualKeyCode`
    #[repr(C)] pub struct AzVirtualKeyCodeVec {
        pub(crate) ptr: *const AzVirtualKeyCode,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `ScanCode`
    #[repr(C)] pub struct AzScanCodeVec {
        pub(crate) ptr: *const u32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CssDeclaration`
    #[repr(C)] pub struct AzCssDeclarationVec {
        pub(crate) ptr: *const AzCssDeclaration,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CssPathSelector`
    #[repr(C)] pub struct AzCssPathSelectorVec {
        pub(crate) ptr: *const AzCssPathSelector,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Stylesheet`
    #[repr(C)] pub struct AzStylesheetVec {
        pub(crate) ptr: *const AzStylesheet,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CssRuleBlock`
    #[repr(C)] pub struct AzCssRuleBlockVec {
        pub(crate) ptr: *const AzCssRuleBlock,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `U8Vec`
    #[repr(C)] pub struct AzU8Vec {
        pub(crate) ptr: *const u8,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `CallbackData`
    #[repr(C)] pub struct AzCallbackDataVec {
        pub(crate) ptr: *const AzCallbackData,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `Vec<DebugMessage>`
    #[repr(C)] pub struct AzDebugMessageVec {
        pub(crate) ptr: *const AzDebugMessage,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `U32Vec`
    #[repr(C)] pub struct AzGLuintVec {
        pub(crate) ptr: *const u32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `GLintVec`
    #[repr(C)] pub struct AzGLintVec {
        pub(crate) ptr: *const i32,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `OverridePropertyVec`
    #[repr(C)] pub struct AzOverridePropertyVec {
        pub(crate) ptr: *const AzOverrideProperty,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `DomVec`
    #[repr(C)] pub struct AzDomVec {
        pub(crate) ptr: *const AzDom,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `StringVec`
    #[repr(C)] pub struct AzStringVec {
        pub(crate) ptr: *const AzString,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `StringPairVec`
    #[repr(C)] pub struct AzStringPairVec {
        pub(crate) ptr: *const AzStringPair,
        pub len: usize,
        pub cap: usize,
    }
    /// Wrapper over a Rust-allocated `GradientStopPreVec`
    #[repr(C)] pub struct AzGradientStopPreVec {
        pub(crate) ptr: *const AzGradientStopPre,
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
    /// Re-export of rust-allocated (stack based) `LayoutCallback` struct
    #[repr(C)] pub struct AzLayoutCallback {
        pub cb: AzLayoutCallbackType,
    }
    /// Re-export of rust-allocated (stack based) `Callback` struct
    #[repr(C)] pub struct AzCallback {
        pub cb: AzCallbackType,
    }
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


    #[cfg(unix)]
    use libloading::os::unix::{Library, Symbol};
    #[cfg(windows)]
    use libloading::os::windows::{Library, Symbol};

    pub struct AzulDll {
        pub lib: Box<Library>,
        pub az_string_from_utf8_unchecked: Symbol<extern fn(_:  *const u8, _:  usize) -> AzString>,
        pub az_string_from_utf8_lossy: Symbol<extern fn(_:  *const u8, _:  usize) -> AzString>,
        pub az_string_into_bytes: Symbol<extern fn(_:  AzString) -> AzU8Vec>,
        pub az_string_delete: Symbol<extern fn(_:  &mut AzString)>,
        pub az_string_deep_copy: Symbol<extern fn(_:  &AzString) -> AzString>,
        pub az_x_window_type_vec_copy_from: Symbol<extern fn(_:  *const AzXWindowType, _:  usize) -> AzXWindowTypeVec>,
        pub az_x_window_type_vec_delete: Symbol<extern fn(_:  &mut AzXWindowTypeVec)>,
        pub az_x_window_type_vec_deep_copy: Symbol<extern fn(_:  &AzXWindowTypeVec) -> AzXWindowTypeVec>,
        pub az_virtual_key_code_vec_copy_from: Symbol<extern fn(_:  *const AzVirtualKeyCode, _:  usize) -> AzVirtualKeyCodeVec>,
        pub az_virtual_key_code_vec_delete: Symbol<extern fn(_:  &mut AzVirtualKeyCodeVec)>,
        pub az_virtual_key_code_vec_deep_copy: Symbol<extern fn(_:  &AzVirtualKeyCodeVec) -> AzVirtualKeyCodeVec>,
        pub az_scan_code_vec_copy_from: Symbol<extern fn(_:  *const u32, _:  usize) -> AzScanCodeVec>,
        pub az_scan_code_vec_delete: Symbol<extern fn(_:  &mut AzScanCodeVec)>,
        pub az_scan_code_vec_deep_copy: Symbol<extern fn(_:  &AzScanCodeVec) -> AzScanCodeVec>,
        pub az_css_declaration_vec_copy_from: Symbol<extern fn(_:  *const AzCssDeclaration, _:  usize) -> AzCssDeclarationVec>,
        pub az_css_declaration_vec_delete: Symbol<extern fn(_:  &mut AzCssDeclarationVec)>,
        pub az_css_declaration_vec_deep_copy: Symbol<extern fn(_:  &AzCssDeclarationVec) -> AzCssDeclarationVec>,
        pub az_css_path_selector_vec_copy_from: Symbol<extern fn(_:  *const AzCssPathSelector, _:  usize) -> AzCssPathSelectorVec>,
        pub az_css_path_selector_vec_delete: Symbol<extern fn(_:  &mut AzCssPathSelectorVec)>,
        pub az_css_path_selector_vec_deep_copy: Symbol<extern fn(_:  &AzCssPathSelectorVec) -> AzCssPathSelectorVec>,
        pub az_stylesheet_vec_copy_from: Symbol<extern fn(_:  *const AzStylesheet, _:  usize) -> AzStylesheetVec>,
        pub az_stylesheet_vec_delete: Symbol<extern fn(_:  &mut AzStylesheetVec)>,
        pub az_stylesheet_vec_deep_copy: Symbol<extern fn(_:  &AzStylesheetVec) -> AzStylesheetVec>,
        pub az_css_rule_block_vec_copy_from: Symbol<extern fn(_:  *const AzCssRuleBlock, _:  usize) -> AzCssRuleBlockVec>,
        pub az_css_rule_block_vec_delete: Symbol<extern fn(_:  &mut AzCssRuleBlockVec)>,
        pub az_css_rule_block_vec_deep_copy: Symbol<extern fn(_:  &AzCssRuleBlockVec) -> AzCssRuleBlockVec>,
        pub az_u8_vec_copy_from: Symbol<extern fn(_:  *const u8, _:  usize) -> AzU8Vec>,
        pub az_u8_vec_delete: Symbol<extern fn(_:  &mut AzU8Vec)>,
        pub az_u8_vec_deep_copy: Symbol<extern fn(_:  &AzU8Vec) -> AzU8Vec>,
        pub az_callback_data_vec_copy_from: Symbol<extern fn(_:  *const AzCallbackData, _:  usize) -> AzCallbackDataVec>,
        pub az_callback_data_vec_delete: Symbol<extern fn(_:  &mut AzCallbackDataVec)>,
        pub az_callback_data_vec_deep_copy: Symbol<extern fn(_:  &AzCallbackDataVec) -> AzCallbackDataVec>,
        pub az_debug_message_vec_copy_from: Symbol<extern fn(_:  *const AzDebugMessage, _:  usize) -> AzDebugMessageVec>,
        pub az_debug_message_vec_delete: Symbol<extern fn(_:  &mut AzDebugMessageVec)>,
        pub az_debug_message_vec_deep_copy: Symbol<extern fn(_:  &AzDebugMessageVec) -> AzDebugMessageVec>,
        pub az_g_luint_vec_copy_from: Symbol<extern fn(_:  *const u32, _:  usize) -> AzGLuintVec>,
        pub az_g_luint_vec_delete: Symbol<extern fn(_:  &mut AzGLuintVec)>,
        pub az_g_luint_vec_deep_copy: Symbol<extern fn(_:  &AzGLuintVec) -> AzGLuintVec>,
        pub az_g_lint_vec_copy_from: Symbol<extern fn(_:  *const i32, _:  usize) -> AzGLintVec>,
        pub az_g_lint_vec_delete: Symbol<extern fn(_:  &mut AzGLintVec)>,
        pub az_g_lint_vec_deep_copy: Symbol<extern fn(_:  &AzGLintVec) -> AzGLintVec>,
        pub az_override_property_vec_copy_from: Symbol<extern fn(_:  *const AzOverrideProperty, _:  usize) -> AzOverridePropertyVec>,
        pub az_override_property_vec_delete: Symbol<extern fn(_:  &mut AzOverridePropertyVec)>,
        pub az_override_property_vec_deep_copy: Symbol<extern fn(_:  &AzOverridePropertyVec) -> AzOverridePropertyVec>,
        pub az_dom_vec_copy_from: Symbol<extern fn(_:  *const AzDom, _:  usize) -> AzDomVec>,
        pub az_dom_vec_delete: Symbol<extern fn(_:  &mut AzDomVec)>,
        pub az_dom_vec_deep_copy: Symbol<extern fn(_:  &AzDomVec) -> AzDomVec>,
        pub az_string_vec_copy_from: Symbol<extern fn(_:  *const AzString, _:  usize) -> AzStringVec>,
        pub az_string_vec_delete: Symbol<extern fn(_:  &mut AzStringVec)>,
        pub az_string_vec_deep_copy: Symbol<extern fn(_:  &AzStringVec) -> AzStringVec>,
        pub az_string_pair_vec_copy_from: Symbol<extern fn(_:  *const AzStringPair, _:  usize) -> AzStringPairVec>,
        pub az_string_pair_vec_delete: Symbol<extern fn(_:  &mut AzStringPairVec)>,
        pub az_string_pair_vec_deep_copy: Symbol<extern fn(_:  &AzStringPairVec) -> AzStringPairVec>,
        pub az_gradient_stop_pre_vec_copy_from: Symbol<extern fn(_:  *const AzGradientStopPre, _:  usize) -> AzGradientStopPreVec>,
        pub az_gradient_stop_pre_vec_delete: Symbol<extern fn(_:  &mut AzGradientStopPreVec)>,
        pub az_gradient_stop_pre_vec_deep_copy: Symbol<extern fn(_:  &AzGradientStopPreVec) -> AzGradientStopPreVec>,
        pub az_option_wayland_theme_delete: Symbol<extern fn(_:  &mut AzOptionWaylandTheme)>,
        pub az_option_wayland_theme_deep_copy: Symbol<extern fn(_:  &AzOptionWaylandTheme) -> AzOptionWaylandTheme>,
        pub az_option_task_bar_icon_delete: Symbol<extern fn(_:  &mut AzOptionTaskBarIcon)>,
        pub az_option_task_bar_icon_deep_copy: Symbol<extern fn(_:  &AzOptionTaskBarIcon) -> AzOptionTaskBarIcon>,
        pub az_option_hwnd_handle_delete: Symbol<extern fn(_:  &mut AzOptionHwndHandle)>,
        pub az_option_hwnd_handle_deep_copy: Symbol<extern fn(_:  &AzOptionHwndHandle) -> AzOptionHwndHandle>,
        pub az_option_logical_position_delete: Symbol<extern fn(_:  &mut AzOptionLogicalPosition)>,
        pub az_option_logical_position_deep_copy: Symbol<extern fn(_:  &AzOptionLogicalPosition) -> AzOptionLogicalPosition>,
        pub az_option_hot_reload_options_delete: Symbol<extern fn(_:  &mut AzOptionHotReloadOptions)>,
        pub az_option_hot_reload_options_deep_copy: Symbol<extern fn(_:  &AzOptionHotReloadOptions) -> AzOptionHotReloadOptions>,
        pub az_option_physical_position_i32_delete: Symbol<extern fn(_:  &mut AzOptionPhysicalPositionI32)>,
        pub az_option_physical_position_i32_deep_copy: Symbol<extern fn(_:  &AzOptionPhysicalPositionI32) -> AzOptionPhysicalPositionI32>,
        pub az_option_window_icon_delete: Symbol<extern fn(_:  &mut AzOptionWindowIcon)>,
        pub az_option_window_icon_deep_copy: Symbol<extern fn(_:  &AzOptionWindowIcon) -> AzOptionWindowIcon>,
        pub az_option_string_delete: Symbol<extern fn(_:  &mut AzOptionString)>,
        pub az_option_string_deep_copy: Symbol<extern fn(_:  &AzOptionString) -> AzOptionString>,
        pub az_option_x11_visual_delete: Symbol<extern fn(_:  &mut AzOptionX11Visual)>,
        pub az_option_x11_visual_deep_copy: Symbol<extern fn(_:  &AzOptionX11Visual) -> AzOptionX11Visual>,
        pub az_option_i32_delete: Symbol<extern fn(_:  &mut AzOptionI32)>,
        pub az_option_i32_deep_copy: Symbol<extern fn(_:  &AzOptionI32) -> AzOptionI32>,
        pub az_option_f32_delete: Symbol<extern fn(_:  &mut AzOptionF32)>,
        pub az_option_f32_deep_copy: Symbol<extern fn(_:  &AzOptionF32) -> AzOptionF32>,
        pub az_option_mouse_cursor_type_delete: Symbol<extern fn(_:  &mut AzOptionMouseCursorType)>,
        pub az_option_mouse_cursor_type_deep_copy: Symbol<extern fn(_:  &AzOptionMouseCursorType) -> AzOptionMouseCursorType>,
        pub az_option_logical_size_delete: Symbol<extern fn(_:  &mut AzOptionLogicalSize)>,
        pub az_option_logical_size_deep_copy: Symbol<extern fn(_:  &AzOptionLogicalSize) -> AzOptionLogicalSize>,
        pub az_option_char_delete: Symbol<extern fn(_:  &mut AzOptionChar)>,
        pub az_option_char_deep_copy: Symbol<extern fn(_:  &AzOptionChar) -> AzOptionChar>,
        pub az_option_virtual_key_code_delete: Symbol<extern fn(_:  &mut AzOptionVirtualKeyCode)>,
        pub az_option_virtual_key_code_deep_copy: Symbol<extern fn(_:  &AzOptionVirtualKeyCode) -> AzOptionVirtualKeyCode>,
        pub az_option_percentage_value_delete: Symbol<extern fn(_:  &mut AzOptionPercentageValue)>,
        pub az_option_percentage_value_deep_copy: Symbol<extern fn(_:  &AzOptionPercentageValue) -> AzOptionPercentageValue>,
        pub az_option_dom_delete: Symbol<extern fn(_:  &mut AzOptionDom)>,
        pub az_option_dom_deep_copy: Symbol<extern fn(_:  &AzOptionDom) -> AzOptionDom>,
        pub az_option_texture_delete: Symbol<extern fn(_:  &mut AzOptionTexture)>,
        pub az_option_tab_index_delete: Symbol<extern fn(_:  &mut AzOptionTabIndex)>,
        pub az_option_tab_index_deep_copy: Symbol<extern fn(_:  &AzOptionTabIndex) -> AzOptionTabIndex>,
        pub az_option_duration_delete: Symbol<extern fn(_:  &mut AzOptionDuration)>,
        pub az_option_duration_deep_copy: Symbol<extern fn(_:  &AzOptionDuration) -> AzOptionDuration>,
        pub az_option_instant_ptr_delete: Symbol<extern fn(_:  &mut AzOptionInstantPtr)>,
        pub az_option_instant_ptr_deep_copy: Symbol<extern fn(_:  &AzOptionInstantPtr) -> AzOptionInstantPtr>,
        pub az_option_usize_delete: Symbol<extern fn(_:  &mut AzOptionUsize)>,
        pub az_option_usize_deep_copy: Symbol<extern fn(_:  &AzOptionUsize) -> AzOptionUsize>,
        pub az_option_u8_vec_ref_delete: Symbol<extern fn(_:  &mut AzOptionU8VecRef)>,
        pub az_result_ref_any_block_error_delete: Symbol<extern fn(_:  &mut AzResultRefAnyBlockError)>,
        pub az_result_ref_any_block_error_deep_copy: Symbol<extern fn(_:  &AzResultRefAnyBlockError) -> AzResultRefAnyBlockError>,
        pub az_instant_now: Symbol<extern fn() -> AzInstantPtr>,
        pub az_instant_delete: Symbol<extern fn(_:  &mut AzInstantPtr)>,
        pub az_duration_delete: Symbol<extern fn(_:  &mut AzDuration)>,
        pub az_duration_deep_copy: Symbol<extern fn(_:  &AzDuration) -> AzDuration>,
        pub az_app_config_default: Symbol<extern fn() -> AzAppConfigPtr>,
        pub az_app_config_delete: Symbol<extern fn(_:  &mut AzAppConfigPtr)>,
        pub az_app_new: Symbol<extern fn(_:  AzRefAny, _:  AzAppConfigPtr, _:  AzLayoutCallbackType) -> AzAppPtr>,
        pub az_app_run: Symbol<extern fn(_:  AzAppPtr, _:  AzWindowCreateOptions)>,
        pub az_app_delete: Symbol<extern fn(_:  &mut AzAppPtr)>,
        pub az_layout_callback_delete: Symbol<extern fn(_:  &mut AzLayoutCallback)>,
        pub az_layout_callback_deep_copy: Symbol<extern fn(_:  &AzLayoutCallback) -> AzLayoutCallback>,
        pub az_callback_delete: Symbol<extern fn(_:  &mut AzCallback)>,
        pub az_callback_deep_copy: Symbol<extern fn(_:  &AzCallback) -> AzCallback>,
        pub az_callback_info_delete: Symbol<extern fn(_:  &mut AzCallbackInfoPtr)>,
        pub az_update_screen_delete: Symbol<extern fn(_:  &mut AzUpdateScreen)>,
        pub az_update_screen_deep_copy: Symbol<extern fn(_:  &AzUpdateScreen) -> AzUpdateScreen>,
        pub az_i_frame_callback_delete: Symbol<extern fn(_:  &mut AzIFrameCallback)>,
        pub az_i_frame_callback_deep_copy: Symbol<extern fn(_:  &AzIFrameCallback) -> AzIFrameCallback>,
        pub az_i_frame_callback_info_get_state: Symbol<extern fn(_:  &AzIFrameCallbackInfoPtr) -> AzRefAny>,
        pub az_i_frame_callback_info_delete: Symbol<extern fn(_:  &mut AzIFrameCallbackInfoPtr)>,
        pub az_i_frame_callback_return_delete: Symbol<extern fn(_:  &mut AzIFrameCallbackReturn)>,
        pub az_i_frame_callback_return_deep_copy: Symbol<extern fn(_:  &AzIFrameCallbackReturn) -> AzIFrameCallbackReturn>,
        pub az_gl_callback_delete: Symbol<extern fn(_:  &mut AzGlCallback)>,
        pub az_gl_callback_deep_copy: Symbol<extern fn(_:  &AzGlCallback) -> AzGlCallback>,
        pub az_gl_callback_info_delete: Symbol<extern fn(_:  &mut AzGlCallbackInfoPtr)>,
        pub az_gl_callback_return_delete: Symbol<extern fn(_:  &mut AzGlCallbackReturn)>,
        pub az_timer_callback_delete: Symbol<extern fn(_:  &mut AzTimerCallback)>,
        pub az_timer_callback_deep_copy: Symbol<extern fn(_:  &AzTimerCallback) -> AzTimerCallback>,
        pub az_timer_callback_type_delete: Symbol<extern fn(_:  &mut AzTimerCallbackTypePtr)>,
        pub az_timer_callback_info_delete: Symbol<extern fn(_:  &mut AzTimerCallbackInfoPtr)>,
        pub az_timer_callback_return_delete: Symbol<extern fn(_:  &mut AzTimerCallbackReturn)>,
        pub az_timer_callback_return_deep_copy: Symbol<extern fn(_:  &AzTimerCallbackReturn) -> AzTimerCallbackReturn>,
        pub az_ref_any_sharing_info_can_be_shared: Symbol<extern fn(_:  &AzRefAnySharingInfo) -> bool>,
        pub az_ref_any_sharing_info_can_be_shared_mut: Symbol<extern fn(_:  &AzRefAnySharingInfo) -> bool>,
        pub az_ref_any_sharing_info_increase_ref: Symbol<extern fn(_:  &mut AzRefAnySharingInfo)>,
        pub az_ref_any_sharing_info_decrease_ref: Symbol<extern fn(_:  &mut AzRefAnySharingInfo)>,
        pub az_ref_any_sharing_info_increase_refmut: Symbol<extern fn(_:  &mut AzRefAnySharingInfo)>,
        pub az_ref_any_sharing_info_decrease_refmut: Symbol<extern fn(_:  &mut AzRefAnySharingInfo)>,
        pub az_ref_any_sharing_info_delete: Symbol<extern fn(_:  &mut AzRefAnySharingInfo)>,
        pub az_ref_any_new_c: Symbol<extern fn(_:  *const c_void, _:  usize, _:  u64, _:  AzString, _:  AzRefAnyDestructorType) -> AzRefAny>,
        pub az_ref_any_is_type: Symbol<extern fn(_:  &AzRefAny, _:  u64) -> bool>,
        pub az_ref_any_get_type_name: Symbol<extern fn(_:  &AzRefAny) -> AzString>,
        pub az_ref_any_can_be_shared: Symbol<extern fn(_:  &AzRefAny) -> bool>,
        pub az_ref_any_can_be_shared_mut: Symbol<extern fn(_:  &AzRefAny) -> bool>,
        pub az_ref_any_increase_ref: Symbol<extern fn(_:  &AzRefAny)>,
        pub az_ref_any_decrease_ref: Symbol<extern fn(_:  &AzRefAny)>,
        pub az_ref_any_increase_refmut: Symbol<extern fn(_:  &AzRefAny)>,
        pub az_ref_any_decrease_refmut: Symbol<extern fn(_:  &AzRefAny)>,
        pub az_ref_any_delete: Symbol<extern fn(_:  &mut AzRefAny)>,
        pub az_ref_any_deep_copy: Symbol<extern fn(_:  &AzRefAny) -> AzRefAny>,
        pub az_layout_info_delete: Symbol<extern fn(_:  &mut AzLayoutInfoPtr)>,
        pub az_css_rule_block_delete: Symbol<extern fn(_:  &mut AzCssRuleBlock)>,
        pub az_css_rule_block_deep_copy: Symbol<extern fn(_:  &AzCssRuleBlock) -> AzCssRuleBlock>,
        pub az_css_declaration_delete: Symbol<extern fn(_:  &mut AzCssDeclaration)>,
        pub az_css_declaration_deep_copy: Symbol<extern fn(_:  &AzCssDeclaration) -> AzCssDeclaration>,
        pub az_dynamic_css_property_delete: Symbol<extern fn(_:  &mut AzDynamicCssProperty)>,
        pub az_dynamic_css_property_deep_copy: Symbol<extern fn(_:  &AzDynamicCssProperty) -> AzDynamicCssProperty>,
        pub az_css_path_delete: Symbol<extern fn(_:  &mut AzCssPath)>,
        pub az_css_path_deep_copy: Symbol<extern fn(_:  &AzCssPath) -> AzCssPath>,
        pub az_css_path_selector_delete: Symbol<extern fn(_:  &mut AzCssPathSelector)>,
        pub az_css_path_selector_deep_copy: Symbol<extern fn(_:  &AzCssPathSelector) -> AzCssPathSelector>,
        pub az_node_type_path_delete: Symbol<extern fn(_:  &mut AzNodeTypePath)>,
        pub az_node_type_path_deep_copy: Symbol<extern fn(_:  &AzNodeTypePath) -> AzNodeTypePath>,
        pub az_css_path_pseudo_selector_delete: Symbol<extern fn(_:  &mut AzCssPathPseudoSelector)>,
        pub az_css_path_pseudo_selector_deep_copy: Symbol<extern fn(_:  &AzCssPathPseudoSelector) -> AzCssPathPseudoSelector>,
        pub az_css_nth_child_selector_delete: Symbol<extern fn(_:  &mut AzCssNthChildSelector)>,
        pub az_css_nth_child_selector_deep_copy: Symbol<extern fn(_:  &AzCssNthChildSelector) -> AzCssNthChildSelector>,
        pub az_css_nth_child_pattern_delete: Symbol<extern fn(_:  &mut AzCssNthChildPattern)>,
        pub az_css_nth_child_pattern_deep_copy: Symbol<extern fn(_:  &AzCssNthChildPattern) -> AzCssNthChildPattern>,
        pub az_stylesheet_delete: Symbol<extern fn(_:  &mut AzStylesheet)>,
        pub az_stylesheet_deep_copy: Symbol<extern fn(_:  &AzStylesheet) -> AzStylesheet>,
        pub az_css_native: Symbol<extern fn() -> AzCss>,
        pub az_css_empty: Symbol<extern fn() -> AzCss>,
        pub az_css_from_string: Symbol<extern fn(_:  AzString) -> AzCss>,
        pub az_css_override_native: Symbol<extern fn(_:  AzString) -> AzCss>,
        pub az_css_delete: Symbol<extern fn(_:  &mut AzCss)>,
        pub az_css_deep_copy: Symbol<extern fn(_:  &AzCss) -> AzCss>,
        pub az_color_u_delete: Symbol<extern fn(_:  &mut AzColorU)>,
        pub az_color_u_deep_copy: Symbol<extern fn(_:  &AzColorU) -> AzColorU>,
        pub az_size_metric_delete: Symbol<extern fn(_:  &mut AzSizeMetric)>,
        pub az_size_metric_deep_copy: Symbol<extern fn(_:  &AzSizeMetric) -> AzSizeMetric>,
        pub az_float_value_delete: Symbol<extern fn(_:  &mut AzFloatValue)>,
        pub az_float_value_deep_copy: Symbol<extern fn(_:  &AzFloatValue) -> AzFloatValue>,
        pub az_pixel_value_delete: Symbol<extern fn(_:  &mut AzPixelValue)>,
        pub az_pixel_value_deep_copy: Symbol<extern fn(_:  &AzPixelValue) -> AzPixelValue>,
        pub az_pixel_value_no_percent_delete: Symbol<extern fn(_:  &mut AzPixelValueNoPercent)>,
        pub az_pixel_value_no_percent_deep_copy: Symbol<extern fn(_:  &AzPixelValueNoPercent) -> AzPixelValueNoPercent>,
        pub az_box_shadow_clip_mode_delete: Symbol<extern fn(_:  &mut AzBoxShadowClipMode)>,
        pub az_box_shadow_clip_mode_deep_copy: Symbol<extern fn(_:  &AzBoxShadowClipMode) -> AzBoxShadowClipMode>,
        pub az_box_shadow_pre_display_item_delete: Symbol<extern fn(_:  &mut AzBoxShadowPreDisplayItem)>,
        pub az_box_shadow_pre_display_item_deep_copy: Symbol<extern fn(_:  &AzBoxShadowPreDisplayItem) -> AzBoxShadowPreDisplayItem>,
        pub az_layout_align_content_delete: Symbol<extern fn(_:  &mut AzLayoutAlignContent)>,
        pub az_layout_align_content_deep_copy: Symbol<extern fn(_:  &AzLayoutAlignContent) -> AzLayoutAlignContent>,
        pub az_layout_align_items_delete: Symbol<extern fn(_:  &mut AzLayoutAlignItems)>,
        pub az_layout_align_items_deep_copy: Symbol<extern fn(_:  &AzLayoutAlignItems) -> AzLayoutAlignItems>,
        pub az_layout_bottom_delete: Symbol<extern fn(_:  &mut AzLayoutBottom)>,
        pub az_layout_bottom_deep_copy: Symbol<extern fn(_:  &AzLayoutBottom) -> AzLayoutBottom>,
        pub az_layout_box_sizing_delete: Symbol<extern fn(_:  &mut AzLayoutBoxSizing)>,
        pub az_layout_box_sizing_deep_copy: Symbol<extern fn(_:  &AzLayoutBoxSizing) -> AzLayoutBoxSizing>,
        pub az_layout_direction_delete: Symbol<extern fn(_:  &mut AzLayoutDirection)>,
        pub az_layout_direction_deep_copy: Symbol<extern fn(_:  &AzLayoutDirection) -> AzLayoutDirection>,
        pub az_layout_display_delete: Symbol<extern fn(_:  &mut AzLayoutDisplay)>,
        pub az_layout_display_deep_copy: Symbol<extern fn(_:  &AzLayoutDisplay) -> AzLayoutDisplay>,
        pub az_layout_flex_grow_delete: Symbol<extern fn(_:  &mut AzLayoutFlexGrow)>,
        pub az_layout_flex_grow_deep_copy: Symbol<extern fn(_:  &AzLayoutFlexGrow) -> AzLayoutFlexGrow>,
        pub az_layout_flex_shrink_delete: Symbol<extern fn(_:  &mut AzLayoutFlexShrink)>,
        pub az_layout_flex_shrink_deep_copy: Symbol<extern fn(_:  &AzLayoutFlexShrink) -> AzLayoutFlexShrink>,
        pub az_layout_float_delete: Symbol<extern fn(_:  &mut AzLayoutFloat)>,
        pub az_layout_float_deep_copy: Symbol<extern fn(_:  &AzLayoutFloat) -> AzLayoutFloat>,
        pub az_layout_height_delete: Symbol<extern fn(_:  &mut AzLayoutHeight)>,
        pub az_layout_height_deep_copy: Symbol<extern fn(_:  &AzLayoutHeight) -> AzLayoutHeight>,
        pub az_layout_justify_content_delete: Symbol<extern fn(_:  &mut AzLayoutJustifyContent)>,
        pub az_layout_justify_content_deep_copy: Symbol<extern fn(_:  &AzLayoutJustifyContent) -> AzLayoutJustifyContent>,
        pub az_layout_left_delete: Symbol<extern fn(_:  &mut AzLayoutLeft)>,
        pub az_layout_left_deep_copy: Symbol<extern fn(_:  &AzLayoutLeft) -> AzLayoutLeft>,
        pub az_layout_margin_bottom_delete: Symbol<extern fn(_:  &mut AzLayoutMarginBottom)>,
        pub az_layout_margin_bottom_deep_copy: Symbol<extern fn(_:  &AzLayoutMarginBottom) -> AzLayoutMarginBottom>,
        pub az_layout_margin_left_delete: Symbol<extern fn(_:  &mut AzLayoutMarginLeft)>,
        pub az_layout_margin_left_deep_copy: Symbol<extern fn(_:  &AzLayoutMarginLeft) -> AzLayoutMarginLeft>,
        pub az_layout_margin_right_delete: Symbol<extern fn(_:  &mut AzLayoutMarginRight)>,
        pub az_layout_margin_right_deep_copy: Symbol<extern fn(_:  &AzLayoutMarginRight) -> AzLayoutMarginRight>,
        pub az_layout_margin_top_delete: Symbol<extern fn(_:  &mut AzLayoutMarginTop)>,
        pub az_layout_margin_top_deep_copy: Symbol<extern fn(_:  &AzLayoutMarginTop) -> AzLayoutMarginTop>,
        pub az_layout_max_height_delete: Symbol<extern fn(_:  &mut AzLayoutMaxHeight)>,
        pub az_layout_max_height_deep_copy: Symbol<extern fn(_:  &AzLayoutMaxHeight) -> AzLayoutMaxHeight>,
        pub az_layout_max_width_delete: Symbol<extern fn(_:  &mut AzLayoutMaxWidth)>,
        pub az_layout_max_width_deep_copy: Symbol<extern fn(_:  &AzLayoutMaxWidth) -> AzLayoutMaxWidth>,
        pub az_layout_min_height_delete: Symbol<extern fn(_:  &mut AzLayoutMinHeight)>,
        pub az_layout_min_height_deep_copy: Symbol<extern fn(_:  &AzLayoutMinHeight) -> AzLayoutMinHeight>,
        pub az_layout_min_width_delete: Symbol<extern fn(_:  &mut AzLayoutMinWidth)>,
        pub az_layout_min_width_deep_copy: Symbol<extern fn(_:  &AzLayoutMinWidth) -> AzLayoutMinWidth>,
        pub az_layout_padding_bottom_delete: Symbol<extern fn(_:  &mut AzLayoutPaddingBottom)>,
        pub az_layout_padding_bottom_deep_copy: Symbol<extern fn(_:  &AzLayoutPaddingBottom) -> AzLayoutPaddingBottom>,
        pub az_layout_padding_left_delete: Symbol<extern fn(_:  &mut AzLayoutPaddingLeft)>,
        pub az_layout_padding_left_deep_copy: Symbol<extern fn(_:  &AzLayoutPaddingLeft) -> AzLayoutPaddingLeft>,
        pub az_layout_padding_right_delete: Symbol<extern fn(_:  &mut AzLayoutPaddingRight)>,
        pub az_layout_padding_right_deep_copy: Symbol<extern fn(_:  &AzLayoutPaddingRight) -> AzLayoutPaddingRight>,
        pub az_layout_padding_top_delete: Symbol<extern fn(_:  &mut AzLayoutPaddingTop)>,
        pub az_layout_padding_top_deep_copy: Symbol<extern fn(_:  &AzLayoutPaddingTop) -> AzLayoutPaddingTop>,
        pub az_layout_position_delete: Symbol<extern fn(_:  &mut AzLayoutPosition)>,
        pub az_layout_position_deep_copy: Symbol<extern fn(_:  &AzLayoutPosition) -> AzLayoutPosition>,
        pub az_layout_right_delete: Symbol<extern fn(_:  &mut AzLayoutRight)>,
        pub az_layout_right_deep_copy: Symbol<extern fn(_:  &AzLayoutRight) -> AzLayoutRight>,
        pub az_layout_top_delete: Symbol<extern fn(_:  &mut AzLayoutTop)>,
        pub az_layout_top_deep_copy: Symbol<extern fn(_:  &AzLayoutTop) -> AzLayoutTop>,
        pub az_layout_width_delete: Symbol<extern fn(_:  &mut AzLayoutWidth)>,
        pub az_layout_width_deep_copy: Symbol<extern fn(_:  &AzLayoutWidth) -> AzLayoutWidth>,
        pub az_layout_wrap_delete: Symbol<extern fn(_:  &mut AzLayoutWrap)>,
        pub az_layout_wrap_deep_copy: Symbol<extern fn(_:  &AzLayoutWrap) -> AzLayoutWrap>,
        pub az_overflow_delete: Symbol<extern fn(_:  &mut AzOverflow)>,
        pub az_overflow_deep_copy: Symbol<extern fn(_:  &AzOverflow) -> AzOverflow>,
        pub az_percentage_value_delete: Symbol<extern fn(_:  &mut AzPercentageValue)>,
        pub az_percentage_value_deep_copy: Symbol<extern fn(_:  &AzPercentageValue) -> AzPercentageValue>,
        pub az_gradient_stop_pre_delete: Symbol<extern fn(_:  &mut AzGradientStopPre)>,
        pub az_gradient_stop_pre_deep_copy: Symbol<extern fn(_:  &AzGradientStopPre) -> AzGradientStopPre>,
        pub az_direction_corner_delete: Symbol<extern fn(_:  &mut AzDirectionCorner)>,
        pub az_direction_corner_deep_copy: Symbol<extern fn(_:  &AzDirectionCorner) -> AzDirectionCorner>,
        pub az_direction_corners_delete: Symbol<extern fn(_:  &mut AzDirectionCorners)>,
        pub az_direction_corners_deep_copy: Symbol<extern fn(_:  &AzDirectionCorners) -> AzDirectionCorners>,
        pub az_direction_delete: Symbol<extern fn(_:  &mut AzDirection)>,
        pub az_direction_deep_copy: Symbol<extern fn(_:  &AzDirection) -> AzDirection>,
        pub az_extend_mode_delete: Symbol<extern fn(_:  &mut AzExtendMode)>,
        pub az_extend_mode_deep_copy: Symbol<extern fn(_:  &AzExtendMode) -> AzExtendMode>,
        pub az_linear_gradient_delete: Symbol<extern fn(_:  &mut AzLinearGradient)>,
        pub az_linear_gradient_deep_copy: Symbol<extern fn(_:  &AzLinearGradient) -> AzLinearGradient>,
        pub az_shape_delete: Symbol<extern fn(_:  &mut AzShape)>,
        pub az_shape_deep_copy: Symbol<extern fn(_:  &AzShape) -> AzShape>,
        pub az_radial_gradient_delete: Symbol<extern fn(_:  &mut AzRadialGradient)>,
        pub az_radial_gradient_deep_copy: Symbol<extern fn(_:  &AzRadialGradient) -> AzRadialGradient>,
        pub az_css_image_id_delete: Symbol<extern fn(_:  &mut AzCssImageId)>,
        pub az_css_image_id_deep_copy: Symbol<extern fn(_:  &AzCssImageId) -> AzCssImageId>,
        pub az_style_background_content_delete: Symbol<extern fn(_:  &mut AzStyleBackgroundContent)>,
        pub az_style_background_content_deep_copy: Symbol<extern fn(_:  &AzStyleBackgroundContent) -> AzStyleBackgroundContent>,
        pub az_background_position_horizontal_delete: Symbol<extern fn(_:  &mut AzBackgroundPositionHorizontal)>,
        pub az_background_position_horizontal_deep_copy: Symbol<extern fn(_:  &AzBackgroundPositionHorizontal) -> AzBackgroundPositionHorizontal>,
        pub az_background_position_vertical_delete: Symbol<extern fn(_:  &mut AzBackgroundPositionVertical)>,
        pub az_background_position_vertical_deep_copy: Symbol<extern fn(_:  &AzBackgroundPositionVertical) -> AzBackgroundPositionVertical>,
        pub az_style_background_position_delete: Symbol<extern fn(_:  &mut AzStyleBackgroundPosition)>,
        pub az_style_background_position_deep_copy: Symbol<extern fn(_:  &AzStyleBackgroundPosition) -> AzStyleBackgroundPosition>,
        pub az_style_background_repeat_delete: Symbol<extern fn(_:  &mut AzStyleBackgroundRepeat)>,
        pub az_style_background_repeat_deep_copy: Symbol<extern fn(_:  &AzStyleBackgroundRepeat) -> AzStyleBackgroundRepeat>,
        pub az_style_background_size_delete: Symbol<extern fn(_:  &mut AzStyleBackgroundSize)>,
        pub az_style_background_size_deep_copy: Symbol<extern fn(_:  &AzStyleBackgroundSize) -> AzStyleBackgroundSize>,
        pub az_style_border_bottom_color_delete: Symbol<extern fn(_:  &mut AzStyleBorderBottomColor)>,
        pub az_style_border_bottom_color_deep_copy: Symbol<extern fn(_:  &AzStyleBorderBottomColor) -> AzStyleBorderBottomColor>,
        pub az_style_border_bottom_left_radius_delete: Symbol<extern fn(_:  &mut AzStyleBorderBottomLeftRadius)>,
        pub az_style_border_bottom_left_radius_deep_copy: Symbol<extern fn(_:  &AzStyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadius>,
        pub az_style_border_bottom_right_radius_delete: Symbol<extern fn(_:  &mut AzStyleBorderBottomRightRadius)>,
        pub az_style_border_bottom_right_radius_deep_copy: Symbol<extern fn(_:  &AzStyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadius>,
        pub az_border_style_delete: Symbol<extern fn(_:  &mut AzBorderStyle)>,
        pub az_border_style_deep_copy: Symbol<extern fn(_:  &AzBorderStyle) -> AzBorderStyle>,
        pub az_style_border_bottom_style_delete: Symbol<extern fn(_:  &mut AzStyleBorderBottomStyle)>,
        pub az_style_border_bottom_style_deep_copy: Symbol<extern fn(_:  &AzStyleBorderBottomStyle) -> AzStyleBorderBottomStyle>,
        pub az_style_border_bottom_width_delete: Symbol<extern fn(_:  &mut AzStyleBorderBottomWidth)>,
        pub az_style_border_bottom_width_deep_copy: Symbol<extern fn(_:  &AzStyleBorderBottomWidth) -> AzStyleBorderBottomWidth>,
        pub az_style_border_left_color_delete: Symbol<extern fn(_:  &mut AzStyleBorderLeftColor)>,
        pub az_style_border_left_color_deep_copy: Symbol<extern fn(_:  &AzStyleBorderLeftColor) -> AzStyleBorderLeftColor>,
        pub az_style_border_left_style_delete: Symbol<extern fn(_:  &mut AzStyleBorderLeftStyle)>,
        pub az_style_border_left_style_deep_copy: Symbol<extern fn(_:  &AzStyleBorderLeftStyle) -> AzStyleBorderLeftStyle>,
        pub az_style_border_left_width_delete: Symbol<extern fn(_:  &mut AzStyleBorderLeftWidth)>,
        pub az_style_border_left_width_deep_copy: Symbol<extern fn(_:  &AzStyleBorderLeftWidth) -> AzStyleBorderLeftWidth>,
        pub az_style_border_right_color_delete: Symbol<extern fn(_:  &mut AzStyleBorderRightColor)>,
        pub az_style_border_right_color_deep_copy: Symbol<extern fn(_:  &AzStyleBorderRightColor) -> AzStyleBorderRightColor>,
        pub az_style_border_right_style_delete: Symbol<extern fn(_:  &mut AzStyleBorderRightStyle)>,
        pub az_style_border_right_style_deep_copy: Symbol<extern fn(_:  &AzStyleBorderRightStyle) -> AzStyleBorderRightStyle>,
        pub az_style_border_right_width_delete: Symbol<extern fn(_:  &mut AzStyleBorderRightWidth)>,
        pub az_style_border_right_width_deep_copy: Symbol<extern fn(_:  &AzStyleBorderRightWidth) -> AzStyleBorderRightWidth>,
        pub az_style_border_top_color_delete: Symbol<extern fn(_:  &mut AzStyleBorderTopColor)>,
        pub az_style_border_top_color_deep_copy: Symbol<extern fn(_:  &AzStyleBorderTopColor) -> AzStyleBorderTopColor>,
        pub az_style_border_top_left_radius_delete: Symbol<extern fn(_:  &mut AzStyleBorderTopLeftRadius)>,
        pub az_style_border_top_left_radius_deep_copy: Symbol<extern fn(_:  &AzStyleBorderTopLeftRadius) -> AzStyleBorderTopLeftRadius>,
        pub az_style_border_top_right_radius_delete: Symbol<extern fn(_:  &mut AzStyleBorderTopRightRadius)>,
        pub az_style_border_top_right_radius_deep_copy: Symbol<extern fn(_:  &AzStyleBorderTopRightRadius) -> AzStyleBorderTopRightRadius>,
        pub az_style_border_top_style_delete: Symbol<extern fn(_:  &mut AzStyleBorderTopStyle)>,
        pub az_style_border_top_style_deep_copy: Symbol<extern fn(_:  &AzStyleBorderTopStyle) -> AzStyleBorderTopStyle>,
        pub az_style_border_top_width_delete: Symbol<extern fn(_:  &mut AzStyleBorderTopWidth)>,
        pub az_style_border_top_width_deep_copy: Symbol<extern fn(_:  &AzStyleBorderTopWidth) -> AzStyleBorderTopWidth>,
        pub az_style_cursor_delete: Symbol<extern fn(_:  &mut AzStyleCursor)>,
        pub az_style_cursor_deep_copy: Symbol<extern fn(_:  &AzStyleCursor) -> AzStyleCursor>,
        pub az_style_font_family_delete: Symbol<extern fn(_:  &mut AzStyleFontFamily)>,
        pub az_style_font_family_deep_copy: Symbol<extern fn(_:  &AzStyleFontFamily) -> AzStyleFontFamily>,
        pub az_style_font_size_delete: Symbol<extern fn(_:  &mut AzStyleFontSize)>,
        pub az_style_font_size_deep_copy: Symbol<extern fn(_:  &AzStyleFontSize) -> AzStyleFontSize>,
        pub az_style_letter_spacing_delete: Symbol<extern fn(_:  &mut AzStyleLetterSpacing)>,
        pub az_style_letter_spacing_deep_copy: Symbol<extern fn(_:  &AzStyleLetterSpacing) -> AzStyleLetterSpacing>,
        pub az_style_line_height_delete: Symbol<extern fn(_:  &mut AzStyleLineHeight)>,
        pub az_style_line_height_deep_copy: Symbol<extern fn(_:  &AzStyleLineHeight) -> AzStyleLineHeight>,
        pub az_style_tab_width_delete: Symbol<extern fn(_:  &mut AzStyleTabWidth)>,
        pub az_style_tab_width_deep_copy: Symbol<extern fn(_:  &AzStyleTabWidth) -> AzStyleTabWidth>,
        pub az_style_text_alignment_horz_delete: Symbol<extern fn(_:  &mut AzStyleTextAlignmentHorz)>,
        pub az_style_text_alignment_horz_deep_copy: Symbol<extern fn(_:  &AzStyleTextAlignmentHorz) -> AzStyleTextAlignmentHorz>,
        pub az_style_text_color_delete: Symbol<extern fn(_:  &mut AzStyleTextColor)>,
        pub az_style_text_color_deep_copy: Symbol<extern fn(_:  &AzStyleTextColor) -> AzStyleTextColor>,
        pub az_style_word_spacing_delete: Symbol<extern fn(_:  &mut AzStyleWordSpacing)>,
        pub az_style_word_spacing_deep_copy: Symbol<extern fn(_:  &AzStyleWordSpacing) -> AzStyleWordSpacing>,
        pub az_box_shadow_pre_display_item_value_delete: Symbol<extern fn(_:  &mut AzBoxShadowPreDisplayItemValue)>,
        pub az_box_shadow_pre_display_item_value_deep_copy: Symbol<extern fn(_:  &AzBoxShadowPreDisplayItemValue) -> AzBoxShadowPreDisplayItemValue>,
        pub az_layout_align_content_value_delete: Symbol<extern fn(_:  &mut AzLayoutAlignContentValue)>,
        pub az_layout_align_content_value_deep_copy: Symbol<extern fn(_:  &AzLayoutAlignContentValue) -> AzLayoutAlignContentValue>,
        pub az_layout_align_items_value_delete: Symbol<extern fn(_:  &mut AzLayoutAlignItemsValue)>,
        pub az_layout_align_items_value_deep_copy: Symbol<extern fn(_:  &AzLayoutAlignItemsValue) -> AzLayoutAlignItemsValue>,
        pub az_layout_bottom_value_delete: Symbol<extern fn(_:  &mut AzLayoutBottomValue)>,
        pub az_layout_bottom_value_deep_copy: Symbol<extern fn(_:  &AzLayoutBottomValue) -> AzLayoutBottomValue>,
        pub az_layout_box_sizing_value_delete: Symbol<extern fn(_:  &mut AzLayoutBoxSizingValue)>,
        pub az_layout_box_sizing_value_deep_copy: Symbol<extern fn(_:  &AzLayoutBoxSizingValue) -> AzLayoutBoxSizingValue>,
        pub az_layout_direction_value_delete: Symbol<extern fn(_:  &mut AzLayoutDirectionValue)>,
        pub az_layout_direction_value_deep_copy: Symbol<extern fn(_:  &AzLayoutDirectionValue) -> AzLayoutDirectionValue>,
        pub az_layout_display_value_delete: Symbol<extern fn(_:  &mut AzLayoutDisplayValue)>,
        pub az_layout_display_value_deep_copy: Symbol<extern fn(_:  &AzLayoutDisplayValue) -> AzLayoutDisplayValue>,
        pub az_layout_flex_grow_value_delete: Symbol<extern fn(_:  &mut AzLayoutFlexGrowValue)>,
        pub az_layout_flex_grow_value_deep_copy: Symbol<extern fn(_:  &AzLayoutFlexGrowValue) -> AzLayoutFlexGrowValue>,
        pub az_layout_flex_shrink_value_delete: Symbol<extern fn(_:  &mut AzLayoutFlexShrinkValue)>,
        pub az_layout_flex_shrink_value_deep_copy: Symbol<extern fn(_:  &AzLayoutFlexShrinkValue) -> AzLayoutFlexShrinkValue>,
        pub az_layout_float_value_delete: Symbol<extern fn(_:  &mut AzLayoutFloatValue)>,
        pub az_layout_float_value_deep_copy: Symbol<extern fn(_:  &AzLayoutFloatValue) -> AzLayoutFloatValue>,
        pub az_layout_height_value_delete: Symbol<extern fn(_:  &mut AzLayoutHeightValue)>,
        pub az_layout_height_value_deep_copy: Symbol<extern fn(_:  &AzLayoutHeightValue) -> AzLayoutHeightValue>,
        pub az_layout_justify_content_value_delete: Symbol<extern fn(_:  &mut AzLayoutJustifyContentValue)>,
        pub az_layout_justify_content_value_deep_copy: Symbol<extern fn(_:  &AzLayoutJustifyContentValue) -> AzLayoutJustifyContentValue>,
        pub az_layout_left_value_delete: Symbol<extern fn(_:  &mut AzLayoutLeftValue)>,
        pub az_layout_left_value_deep_copy: Symbol<extern fn(_:  &AzLayoutLeftValue) -> AzLayoutLeftValue>,
        pub az_layout_margin_bottom_value_delete: Symbol<extern fn(_:  &mut AzLayoutMarginBottomValue)>,
        pub az_layout_margin_bottom_value_deep_copy: Symbol<extern fn(_:  &AzLayoutMarginBottomValue) -> AzLayoutMarginBottomValue>,
        pub az_layout_margin_left_value_delete: Symbol<extern fn(_:  &mut AzLayoutMarginLeftValue)>,
        pub az_layout_margin_left_value_deep_copy: Symbol<extern fn(_:  &AzLayoutMarginLeftValue) -> AzLayoutMarginLeftValue>,
        pub az_layout_margin_right_value_delete: Symbol<extern fn(_:  &mut AzLayoutMarginRightValue)>,
        pub az_layout_margin_right_value_deep_copy: Symbol<extern fn(_:  &AzLayoutMarginRightValue) -> AzLayoutMarginRightValue>,
        pub az_layout_margin_top_value_delete: Symbol<extern fn(_:  &mut AzLayoutMarginTopValue)>,
        pub az_layout_margin_top_value_deep_copy: Symbol<extern fn(_:  &AzLayoutMarginTopValue) -> AzLayoutMarginTopValue>,
        pub az_layout_max_height_value_delete: Symbol<extern fn(_:  &mut AzLayoutMaxHeightValue)>,
        pub az_layout_max_height_value_deep_copy: Symbol<extern fn(_:  &AzLayoutMaxHeightValue) -> AzLayoutMaxHeightValue>,
        pub az_layout_max_width_value_delete: Symbol<extern fn(_:  &mut AzLayoutMaxWidthValue)>,
        pub az_layout_max_width_value_deep_copy: Symbol<extern fn(_:  &AzLayoutMaxWidthValue) -> AzLayoutMaxWidthValue>,
        pub az_layout_min_height_value_delete: Symbol<extern fn(_:  &mut AzLayoutMinHeightValue)>,
        pub az_layout_min_height_value_deep_copy: Symbol<extern fn(_:  &AzLayoutMinHeightValue) -> AzLayoutMinHeightValue>,
        pub az_layout_min_width_value_delete: Symbol<extern fn(_:  &mut AzLayoutMinWidthValue)>,
        pub az_layout_min_width_value_deep_copy: Symbol<extern fn(_:  &AzLayoutMinWidthValue) -> AzLayoutMinWidthValue>,
        pub az_layout_padding_bottom_value_delete: Symbol<extern fn(_:  &mut AzLayoutPaddingBottomValue)>,
        pub az_layout_padding_bottom_value_deep_copy: Symbol<extern fn(_:  &AzLayoutPaddingBottomValue) -> AzLayoutPaddingBottomValue>,
        pub az_layout_padding_left_value_delete: Symbol<extern fn(_:  &mut AzLayoutPaddingLeftValue)>,
        pub az_layout_padding_left_value_deep_copy: Symbol<extern fn(_:  &AzLayoutPaddingLeftValue) -> AzLayoutPaddingLeftValue>,
        pub az_layout_padding_right_value_delete: Symbol<extern fn(_:  &mut AzLayoutPaddingRightValue)>,
        pub az_layout_padding_right_value_deep_copy: Symbol<extern fn(_:  &AzLayoutPaddingRightValue) -> AzLayoutPaddingRightValue>,
        pub az_layout_padding_top_value_delete: Symbol<extern fn(_:  &mut AzLayoutPaddingTopValue)>,
        pub az_layout_padding_top_value_deep_copy: Symbol<extern fn(_:  &AzLayoutPaddingTopValue) -> AzLayoutPaddingTopValue>,
        pub az_layout_position_value_delete: Symbol<extern fn(_:  &mut AzLayoutPositionValue)>,
        pub az_layout_position_value_deep_copy: Symbol<extern fn(_:  &AzLayoutPositionValue) -> AzLayoutPositionValue>,
        pub az_layout_right_value_delete: Symbol<extern fn(_:  &mut AzLayoutRightValue)>,
        pub az_layout_right_value_deep_copy: Symbol<extern fn(_:  &AzLayoutRightValue) -> AzLayoutRightValue>,
        pub az_layout_top_value_delete: Symbol<extern fn(_:  &mut AzLayoutTopValue)>,
        pub az_layout_top_value_deep_copy: Symbol<extern fn(_:  &AzLayoutTopValue) -> AzLayoutTopValue>,
        pub az_layout_width_value_delete: Symbol<extern fn(_:  &mut AzLayoutWidthValue)>,
        pub az_layout_width_value_deep_copy: Symbol<extern fn(_:  &AzLayoutWidthValue) -> AzLayoutWidthValue>,
        pub az_layout_wrap_value_delete: Symbol<extern fn(_:  &mut AzLayoutWrapValue)>,
        pub az_layout_wrap_value_deep_copy: Symbol<extern fn(_:  &AzLayoutWrapValue) -> AzLayoutWrapValue>,
        pub az_overflow_value_delete: Symbol<extern fn(_:  &mut AzOverflowValue)>,
        pub az_overflow_value_deep_copy: Symbol<extern fn(_:  &AzOverflowValue) -> AzOverflowValue>,
        pub az_style_background_content_value_delete: Symbol<extern fn(_:  &mut AzStyleBackgroundContentValue)>,
        pub az_style_background_content_value_deep_copy: Symbol<extern fn(_:  &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue>,
        pub az_style_background_position_value_delete: Symbol<extern fn(_:  &mut AzStyleBackgroundPositionValue)>,
        pub az_style_background_position_value_deep_copy: Symbol<extern fn(_:  &AzStyleBackgroundPositionValue) -> AzStyleBackgroundPositionValue>,
        pub az_style_background_repeat_value_delete: Symbol<extern fn(_:  &mut AzStyleBackgroundRepeatValue)>,
        pub az_style_background_repeat_value_deep_copy: Symbol<extern fn(_:  &AzStyleBackgroundRepeatValue) -> AzStyleBackgroundRepeatValue>,
        pub az_style_background_size_value_delete: Symbol<extern fn(_:  &mut AzStyleBackgroundSizeValue)>,
        pub az_style_background_size_value_deep_copy: Symbol<extern fn(_:  &AzStyleBackgroundSizeValue) -> AzStyleBackgroundSizeValue>,
        pub az_style_border_bottom_color_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderBottomColorValue)>,
        pub az_style_border_bottom_color_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderBottomColorValue) -> AzStyleBorderBottomColorValue>,
        pub az_style_border_bottom_left_radius_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderBottomLeftRadiusValue)>,
        pub az_style_border_bottom_left_radius_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderBottomLeftRadiusValue) -> AzStyleBorderBottomLeftRadiusValue>,
        pub az_style_border_bottom_right_radius_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderBottomRightRadiusValue)>,
        pub az_style_border_bottom_right_radius_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderBottomRightRadiusValue) -> AzStyleBorderBottomRightRadiusValue>,
        pub az_style_border_bottom_style_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderBottomStyleValue)>,
        pub az_style_border_bottom_style_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderBottomStyleValue) -> AzStyleBorderBottomStyleValue>,
        pub az_style_border_bottom_width_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderBottomWidthValue)>,
        pub az_style_border_bottom_width_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderBottomWidthValue) -> AzStyleBorderBottomWidthValue>,
        pub az_style_border_left_color_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderLeftColorValue)>,
        pub az_style_border_left_color_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderLeftColorValue) -> AzStyleBorderLeftColorValue>,
        pub az_style_border_left_style_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderLeftStyleValue)>,
        pub az_style_border_left_style_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderLeftStyleValue) -> AzStyleBorderLeftStyleValue>,
        pub az_style_border_left_width_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderLeftWidthValue)>,
        pub az_style_border_left_width_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderLeftWidthValue) -> AzStyleBorderLeftWidthValue>,
        pub az_style_border_right_color_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderRightColorValue)>,
        pub az_style_border_right_color_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderRightColorValue) -> AzStyleBorderRightColorValue>,
        pub az_style_border_right_style_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderRightStyleValue)>,
        pub az_style_border_right_style_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderRightStyleValue) -> AzStyleBorderRightStyleValue>,
        pub az_style_border_right_width_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderRightWidthValue)>,
        pub az_style_border_right_width_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderRightWidthValue) -> AzStyleBorderRightWidthValue>,
        pub az_style_border_top_color_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderTopColorValue)>,
        pub az_style_border_top_color_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderTopColorValue) -> AzStyleBorderTopColorValue>,
        pub az_style_border_top_left_radius_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderTopLeftRadiusValue)>,
        pub az_style_border_top_left_radius_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderTopLeftRadiusValue) -> AzStyleBorderTopLeftRadiusValue>,
        pub az_style_border_top_right_radius_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderTopRightRadiusValue)>,
        pub az_style_border_top_right_radius_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderTopRightRadiusValue) -> AzStyleBorderTopRightRadiusValue>,
        pub az_style_border_top_style_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderTopStyleValue)>,
        pub az_style_border_top_style_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderTopStyleValue) -> AzStyleBorderTopStyleValue>,
        pub az_style_border_top_width_value_delete: Symbol<extern fn(_:  &mut AzStyleBorderTopWidthValue)>,
        pub az_style_border_top_width_value_deep_copy: Symbol<extern fn(_:  &AzStyleBorderTopWidthValue) -> AzStyleBorderTopWidthValue>,
        pub az_style_cursor_value_delete: Symbol<extern fn(_:  &mut AzStyleCursorValue)>,
        pub az_style_cursor_value_deep_copy: Symbol<extern fn(_:  &AzStyleCursorValue) -> AzStyleCursorValue>,
        pub az_style_font_family_value_delete: Symbol<extern fn(_:  &mut AzStyleFontFamilyValue)>,
        pub az_style_font_family_value_deep_copy: Symbol<extern fn(_:  &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue>,
        pub az_style_font_size_value_delete: Symbol<extern fn(_:  &mut AzStyleFontSizeValue)>,
        pub az_style_font_size_value_deep_copy: Symbol<extern fn(_:  &AzStyleFontSizeValue) -> AzStyleFontSizeValue>,
        pub az_style_letter_spacing_value_delete: Symbol<extern fn(_:  &mut AzStyleLetterSpacingValue)>,
        pub az_style_letter_spacing_value_deep_copy: Symbol<extern fn(_:  &AzStyleLetterSpacingValue) -> AzStyleLetterSpacingValue>,
        pub az_style_line_height_value_delete: Symbol<extern fn(_:  &mut AzStyleLineHeightValue)>,
        pub az_style_line_height_value_deep_copy: Symbol<extern fn(_:  &AzStyleLineHeightValue) -> AzStyleLineHeightValue>,
        pub az_style_tab_width_value_delete: Symbol<extern fn(_:  &mut AzStyleTabWidthValue)>,
        pub az_style_tab_width_value_deep_copy: Symbol<extern fn(_:  &AzStyleTabWidthValue) -> AzStyleTabWidthValue>,
        pub az_style_text_alignment_horz_value_delete: Symbol<extern fn(_:  &mut AzStyleTextAlignmentHorzValue)>,
        pub az_style_text_alignment_horz_value_deep_copy: Symbol<extern fn(_:  &AzStyleTextAlignmentHorzValue) -> AzStyleTextAlignmentHorzValue>,
        pub az_style_text_color_value_delete: Symbol<extern fn(_:  &mut AzStyleTextColorValue)>,
        pub az_style_text_color_value_deep_copy: Symbol<extern fn(_:  &AzStyleTextColorValue) -> AzStyleTextColorValue>,
        pub az_style_word_spacing_value_delete: Symbol<extern fn(_:  &mut AzStyleWordSpacingValue)>,
        pub az_style_word_spacing_value_deep_copy: Symbol<extern fn(_:  &AzStyleWordSpacingValue) -> AzStyleWordSpacingValue>,
        pub az_css_property_delete: Symbol<extern fn(_:  &mut AzCssProperty)>,
        pub az_css_property_deep_copy: Symbol<extern fn(_:  &AzCssProperty) -> AzCssProperty>,
        pub az_dom_div: Symbol<extern fn() -> AzDom>,
        pub az_dom_body: Symbol<extern fn() -> AzDom>,
        pub az_dom_label: Symbol<extern fn(_:  AzString) -> AzDom>,
        pub az_dom_text: Symbol<extern fn(_:  AzTextId) -> AzDom>,
        pub az_dom_image: Symbol<extern fn(_:  AzImageId) -> AzDom>,
        pub az_dom_gl_texture: Symbol<extern fn(_:  AzRefAny, _:  AzGlCallbackType) -> AzDom>,
        pub az_dom_iframe: Symbol<extern fn(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzDom>,
        pub az_dom_add_id: Symbol<extern fn(_:  &mut AzDom, _:  AzString)>,
        pub az_dom_with_id: Symbol<extern fn(_:  AzDom, _:  AzString) -> AzDom>,
        pub az_dom_set_ids: Symbol<extern fn(_:  &mut AzDom, _:  AzStringVec)>,
        pub az_dom_with_ids: Symbol<extern fn(_:  AzDom, _:  AzStringVec) -> AzDom>,
        pub az_dom_add_class: Symbol<extern fn(_:  &mut AzDom, _:  AzString)>,
        pub az_dom_with_class: Symbol<extern fn(_:  AzDom, _:  AzString) -> AzDom>,
        pub az_dom_set_classes: Symbol<extern fn(_:  &mut AzDom, _:  AzStringVec)>,
        pub az_dom_with_classes: Symbol<extern fn(_:  AzDom, _:  AzStringVec) -> AzDom>,
        pub az_dom_add_callback: Symbol<extern fn(_:  &mut AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType)>,
        pub az_dom_with_callback: Symbol<extern fn(_:  AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzDom>,
        pub az_dom_add_css_override: Symbol<extern fn(_:  &mut AzDom, _:  AzString, _:  AzCssProperty)>,
        pub az_dom_with_css_override: Symbol<extern fn(_:  AzDom, _:  AzString, _:  AzCssProperty) -> AzDom>,
        pub az_dom_set_is_draggable: Symbol<extern fn(_:  &mut AzDom, _:  bool)>,
        pub az_dom_is_draggable: Symbol<extern fn(_:  AzDom, _:  bool) -> AzDom>,
        pub az_dom_set_tab_index: Symbol<extern fn(_:  &mut AzDom, _:  AzTabIndex)>,
        pub az_dom_with_tab_index: Symbol<extern fn(_:  AzDom, _:  AzTabIndex) -> AzDom>,
        pub az_dom_add_child: Symbol<extern fn(_:  &mut AzDom, _:  AzDom)>,
        pub az_dom_with_child: Symbol<extern fn(_:  AzDom, _:  AzDom) -> AzDom>,
        pub az_dom_has_id: Symbol<extern fn(_:  &mut AzDom, _:  AzString) -> bool>,
        pub az_dom_has_class: Symbol<extern fn(_:  &mut AzDom, _:  AzString) -> bool>,
        pub az_dom_get_html_string: Symbol<extern fn(_:  &AzDom) -> AzString>,
        pub az_dom_delete: Symbol<extern fn(_:  &mut AzDom)>,
        pub az_dom_deep_copy: Symbol<extern fn(_:  &AzDom) -> AzDom>,
        pub az_gl_texture_node_delete: Symbol<extern fn(_:  &mut AzGlTextureNode)>,
        pub az_gl_texture_node_deep_copy: Symbol<extern fn(_:  &AzGlTextureNode) -> AzGlTextureNode>,
        pub az_i_frame_node_delete: Symbol<extern fn(_:  &mut AzIFrameNode)>,
        pub az_i_frame_node_deep_copy: Symbol<extern fn(_:  &AzIFrameNode) -> AzIFrameNode>,
        pub az_callback_data_delete: Symbol<extern fn(_:  &mut AzCallbackData)>,
        pub az_callback_data_deep_copy: Symbol<extern fn(_:  &AzCallbackData) -> AzCallbackData>,
        pub az_override_property_delete: Symbol<extern fn(_:  &mut AzOverrideProperty)>,
        pub az_override_property_deep_copy: Symbol<extern fn(_:  &AzOverrideProperty) -> AzOverrideProperty>,
        pub az_node_data_new: Symbol<extern fn(_:  AzNodeType) -> AzNodeData>,
        pub az_node_data_default: Symbol<extern fn() -> AzNodeData>,
        pub az_node_data_delete: Symbol<extern fn(_:  &mut AzNodeData)>,
        pub az_node_data_deep_copy: Symbol<extern fn(_:  &AzNodeData) -> AzNodeData>,
        pub az_node_type_delete: Symbol<extern fn(_:  &mut AzNodeType)>,
        pub az_node_type_deep_copy: Symbol<extern fn(_:  &AzNodeType) -> AzNodeType>,
        pub az_on_into_event_filter: Symbol<extern fn(_:  AzOn) -> AzEventFilter>,
        pub az_on_delete: Symbol<extern fn(_:  &mut AzOn)>,
        pub az_on_deep_copy: Symbol<extern fn(_:  &AzOn) -> AzOn>,
        pub az_event_filter_delete: Symbol<extern fn(_:  &mut AzEventFilter)>,
        pub az_event_filter_deep_copy: Symbol<extern fn(_:  &AzEventFilter) -> AzEventFilter>,
        pub az_hover_event_filter_delete: Symbol<extern fn(_:  &mut AzHoverEventFilter)>,
        pub az_hover_event_filter_deep_copy: Symbol<extern fn(_:  &AzHoverEventFilter) -> AzHoverEventFilter>,
        pub az_focus_event_filter_delete: Symbol<extern fn(_:  &mut AzFocusEventFilter)>,
        pub az_focus_event_filter_deep_copy: Symbol<extern fn(_:  &AzFocusEventFilter) -> AzFocusEventFilter>,
        pub az_not_event_filter_delete: Symbol<extern fn(_:  &mut AzNotEventFilter)>,
        pub az_not_event_filter_deep_copy: Symbol<extern fn(_:  &AzNotEventFilter) -> AzNotEventFilter>,
        pub az_window_event_filter_delete: Symbol<extern fn(_:  &mut AzWindowEventFilter)>,
        pub az_window_event_filter_deep_copy: Symbol<extern fn(_:  &AzWindowEventFilter) -> AzWindowEventFilter>,
        pub az_tab_index_delete: Symbol<extern fn(_:  &mut AzTabIndex)>,
        pub az_tab_index_deep_copy: Symbol<extern fn(_:  &AzTabIndex) -> AzTabIndex>,
        pub az_gl_type_delete: Symbol<extern fn(_:  &mut AzGlType)>,
        pub az_gl_type_deep_copy: Symbol<extern fn(_:  &AzGlType) -> AzGlType>,
        pub az_debug_message_delete: Symbol<extern fn(_:  &mut AzDebugMessage)>,
        pub az_debug_message_deep_copy: Symbol<extern fn(_:  &AzDebugMessage) -> AzDebugMessage>,
        pub az_u8_vec_ref_delete: Symbol<extern fn(_:  &mut AzU8VecRef)>,
        pub az_u8_vec_ref_mut_delete: Symbol<extern fn(_:  &mut AzU8VecRefMut)>,
        pub az_f32_vec_ref_delete: Symbol<extern fn(_:  &mut AzF32VecRef)>,
        pub az_i32_vec_ref_delete: Symbol<extern fn(_:  &mut AzI32VecRef)>,
        pub az_g_luint_vec_ref_delete: Symbol<extern fn(_:  &mut AzGLuintVecRef)>,
        pub az_g_lenum_vec_ref_delete: Symbol<extern fn(_:  &mut AzGLenumVecRef)>,
        pub az_g_lint_vec_ref_mut_delete: Symbol<extern fn(_:  &mut AzGLintVecRefMut)>,
        pub az_g_lint64_vec_ref_mut_delete: Symbol<extern fn(_:  &mut AzGLint64VecRefMut)>,
        pub az_g_lboolean_vec_ref_mut_delete: Symbol<extern fn(_:  &mut AzGLbooleanVecRefMut)>,
        pub az_g_lfloat_vec_ref_mut_delete: Symbol<extern fn(_:  &mut AzGLfloatVecRefMut)>,
        pub az_refstr_vec_ref_delete: Symbol<extern fn(_:  &mut AzRefstrVecRef)>,
        pub az_refstr_delete: Symbol<extern fn(_:  &mut AzRefstr)>,
        pub az_get_program_binary_return_delete: Symbol<extern fn(_:  &mut AzGetProgramBinaryReturn)>,
        pub az_get_program_binary_return_deep_copy: Symbol<extern fn(_:  &AzGetProgramBinaryReturn) -> AzGetProgramBinaryReturn>,
        pub az_get_active_attrib_return_delete: Symbol<extern fn(_:  &mut AzGetActiveAttribReturn)>,
        pub az_get_active_attrib_return_deep_copy: Symbol<extern fn(_:  &AzGetActiveAttribReturn) -> AzGetActiveAttribReturn>,
        pub az_g_lsync_ptr_delete: Symbol<extern fn(_:  &mut AzGLsyncPtr)>,
        pub az_get_active_uniform_return_delete: Symbol<extern fn(_:  &mut AzGetActiveUniformReturn)>,
        pub az_get_active_uniform_return_deep_copy: Symbol<extern fn(_:  &AzGetActiveUniformReturn) -> AzGetActiveUniformReturn>,
        pub az_gl_context_ptr_get_type: Symbol<extern fn(_:  &AzGlContextPtr) -> AzGlType>,
        pub az_gl_context_ptr_buffer_data_untyped: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  *const c_void, _:  u32)>,
        pub az_gl_context_ptr_buffer_sub_data_untyped: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  *const c_void)>,
        pub az_gl_context_ptr_map_buffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> *mut c_void>,
        pub az_gl_context_ptr_map_buffer_range: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  u32) -> *mut c_void>,
        pub az_gl_context_ptr_unmap_buffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>,
        pub az_gl_context_ptr_tex_buffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_shader_source: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzStringVec)>,
        pub az_gl_context_ptr_read_buffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_read_pixels_into_buffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut)>,
        pub az_gl_context_ptr_read_pixels: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32) -> AzU8Vec>,
        pub az_gl_context_ptr_read_pixels_into_pbo: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_sample_coverage: Symbol<extern fn(_:  &AzGlContextPtr, _:  f32, _:  bool)>,
        pub az_gl_context_ptr_polygon_offset: Symbol<extern fn(_:  &AzGlContextPtr, _:  f32, _:  f32)>,
        pub az_gl_context_ptr_pixel_store_i: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32)>,
        pub az_gl_context_ptr_gen_buffers: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>,
        pub az_gl_context_ptr_gen_renderbuffers: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>,
        pub az_gl_context_ptr_gen_framebuffers: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>,
        pub az_gl_context_ptr_gen_textures: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>,
        pub az_gl_context_ptr_gen_vertex_arrays: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>,
        pub az_gl_context_ptr_gen_queries: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>,
        pub az_gl_context_ptr_begin_query: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_end_query: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_query_counter: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_get_query_object_iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32>,
        pub az_gl_context_ptr_get_query_object_uiv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u32>,
        pub az_gl_context_ptr_get_query_object_i64v: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i64>,
        pub az_gl_context_ptr_get_query_object_ui64v: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u64>,
        pub az_gl_context_ptr_delete_queries: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>,
        pub az_gl_context_ptr_delete_vertex_arrays: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>,
        pub az_gl_context_ptr_delete_buffers: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>,
        pub az_gl_context_ptr_delete_renderbuffers: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>,
        pub az_gl_context_ptr_delete_framebuffers: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>,
        pub az_gl_context_ptr_delete_textures: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>,
        pub az_gl_context_ptr_framebuffer_renderbuffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_renderbuffer_storage: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_depth_func: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_active_texture: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_attach_shader: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_bind_attrib_location: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr)>,
        pub az_gl_context_ptr_get_uniform_iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLintVecRefMut)>,
        pub az_gl_context_ptr_get_uniform_fv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLfloatVecRefMut)>,
        pub az_gl_context_ptr_get_uniform_block_index: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> u32>,
        pub az_gl_context_ptr_get_uniform_indices: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstrVecRef) -> AzGLuintVec>,
        pub az_gl_context_ptr_bind_buffer_base: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_bind_buffer_range: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  isize, _:  isize)>,
        pub az_gl_context_ptr_uniform_block_binding: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_bind_buffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_bind_vertex_array: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_bind_renderbuffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_bind_framebuffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_bind_texture: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_draw_buffers: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzGLenumVecRef)>,
        pub az_gl_context_ptr_tex_image_2d: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef)>,
        pub az_gl_context_ptr_compressed_tex_image_2d: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  AzU8VecRef)>,
        pub az_gl_context_ptr_compressed_tex_sub_image_2d: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  AzU8VecRef)>,
        pub az_gl_context_ptr_tex_image_3d: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef)>,
        pub az_gl_context_ptr_copy_tex_image_2d: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_copy_tex_sub_image_2d: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_copy_tex_sub_image_3d: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_tex_sub_image_2d: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef)>,
        pub az_gl_context_ptr_tex_sub_image_2d_pbo: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize)>,
        pub az_gl_context_ptr_tex_sub_image_3d: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef)>,
        pub az_gl_context_ptr_tex_sub_image_3d_pbo: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize)>,
        pub az_gl_context_ptr_tex_storage_2d: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_tex_storage_3d: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_get_tex_image_into_buffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut)>,
        pub az_gl_context_ptr_copy_image_sub_data: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_invalidate_framebuffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef)>,
        pub az_gl_context_ptr_invalidate_sub_framebuffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef, _:  i32, _:  i32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_get_integer_v: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLintVecRefMut)>,
        pub az_gl_context_ptr_get_integer_64v: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLint64VecRefMut)>,
        pub az_gl_context_ptr_get_integer_iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut)>,
        pub az_gl_context_ptr_get_integer_64iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLint64VecRefMut)>,
        pub az_gl_context_ptr_get_boolean_v: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLbooleanVecRefMut)>,
        pub az_gl_context_ptr_get_float_v: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLfloatVecRefMut)>,
        pub az_gl_context_ptr_get_framebuffer_attachment_parameter_iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32>,
        pub az_gl_context_ptr_get_renderbuffer_parameter_iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32>,
        pub az_gl_context_ptr_get_tex_parameter_iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32>,
        pub az_gl_context_ptr_get_tex_parameter_fv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> f32>,
        pub az_gl_context_ptr_tex_parameter_i: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32)>,
        pub az_gl_context_ptr_tex_parameter_f: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  f32)>,
        pub az_gl_context_ptr_framebuffer_texture_2d: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  i32)>,
        pub az_gl_context_ptr_framebuffer_texture_layer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_blit_framebuffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_vertex_attrib_4f: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  f32, _:  f32, _:  f32, _:  f32)>,
        pub az_gl_context_ptr_vertex_attrib_pointer_f32: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  bool, _:  i32, _:  u32)>,
        pub az_gl_context_ptr_vertex_attrib_pointer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  bool, _:  i32, _:  u32)>,
        pub az_gl_context_ptr_vertex_attrib_i_pointer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  u32)>,
        pub az_gl_context_ptr_vertex_attrib_divisor: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_viewport: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_scissor: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_line_width: Symbol<extern fn(_:  &AzGlContextPtr, _:  f32)>,
        pub az_gl_context_ptr_use_program: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_validate_program: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_draw_arrays: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_draw_arrays_instanced: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_draw_elements: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_draw_elements_instanced: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32)>,
        pub az_gl_context_ptr_blend_color: Symbol<extern fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32)>,
        pub az_gl_context_ptr_blend_func: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_blend_func_separate: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_blend_equation: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_blend_equation_separate: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_color_mask: Symbol<extern fn(_:  &AzGlContextPtr, _:  bool, _:  bool, _:  bool, _:  bool)>,
        pub az_gl_context_ptr_cull_face: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_front_face: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_enable: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_disable: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_hint: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_is_enabled: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>,
        pub az_gl_context_ptr_is_shader: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>,
        pub az_gl_context_ptr_is_texture: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>,
        pub az_gl_context_ptr_is_framebuffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>,
        pub az_gl_context_ptr_is_renderbuffer: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>,
        pub az_gl_context_ptr_check_frame_buffer_status: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32) -> u32>,
        pub az_gl_context_ptr_enable_vertex_attrib_array: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_disable_vertex_attrib_array: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_uniform_1f: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  f32)>,
        pub az_gl_context_ptr_uniform_1fv: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef)>,
        pub az_gl_context_ptr_uniform_1i: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_uniform_1iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef)>,
        pub az_gl_context_ptr_uniform_1ui: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  u32)>,
        pub az_gl_context_ptr_uniform_2f: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32)>,
        pub az_gl_context_ptr_uniform_2fv: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef)>,
        pub az_gl_context_ptr_uniform_2i: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_uniform_2iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef)>,
        pub az_gl_context_ptr_uniform_2ui: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_uniform_3f: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32)>,
        pub az_gl_context_ptr_uniform_3fv: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef)>,
        pub az_gl_context_ptr_uniform_3i: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_uniform_3iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef)>,
        pub az_gl_context_ptr_uniform_3ui: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_uniform_4f: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32, _:  f32)>,
        pub az_gl_context_ptr_uniform_4i: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>,
        pub az_gl_context_ptr_uniform_4iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef)>,
        pub az_gl_context_ptr_uniform_4ui: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_uniform_4fv: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef)>,
        pub az_gl_context_ptr_uniform_matrix_2fv: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef)>,
        pub az_gl_context_ptr_uniform_matrix_3fv: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef)>,
        pub az_gl_context_ptr_uniform_matrix_4fv: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef)>,
        pub az_gl_context_ptr_depth_mask: Symbol<extern fn(_:  &AzGlContextPtr, _:  bool)>,
        pub az_gl_context_ptr_depth_range: Symbol<extern fn(_:  &AzGlContextPtr, _:  f64, _:  f64)>,
        pub az_gl_context_ptr_get_active_attrib: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveAttribReturn>,
        pub az_gl_context_ptr_get_active_uniform: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveUniformReturn>,
        pub az_gl_context_ptr_get_active_uniforms_iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLuintVec, _:  u32) -> AzGLintVec>,
        pub az_gl_context_ptr_get_active_uniform_block_i: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32>,
        pub az_gl_context_ptr_get_active_uniform_block_iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> AzGLintVec>,
        pub az_gl_context_ptr_get_active_uniform_block_name: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString>,
        pub az_gl_context_ptr_get_attrib_location: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32>,
        pub az_gl_context_ptr_get_frag_data_location: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32>,
        pub az_gl_context_ptr_get_uniform_location: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32>,
        pub az_gl_context_ptr_get_program_info_log: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32) -> AzString>,
        pub az_gl_context_ptr_get_program_iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut)>,
        pub az_gl_context_ptr_get_program_binary: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32) -> AzGetProgramBinaryReturn>,
        pub az_gl_context_ptr_program_binary: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzU8VecRef)>,
        pub az_gl_context_ptr_program_parameter_i: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32)>,
        pub az_gl_context_ptr_get_vertex_attrib_iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut)>,
        pub az_gl_context_ptr_get_vertex_attrib_fv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLfloatVecRefMut)>,
        pub az_gl_context_ptr_get_vertex_attrib_pointer_v: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> isize>,
        pub az_gl_context_ptr_get_buffer_parameter_iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32>,
        pub az_gl_context_ptr_get_shader_info_log: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32) -> AzString>,
        pub az_gl_context_ptr_get_string: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32) -> AzString>,
        pub az_gl_context_ptr_get_string_i: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString>,
        pub az_gl_context_ptr_get_shader_iv: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut)>,
        pub az_gl_context_ptr_get_shader_precision_format: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> [i32;3]>,
        pub az_gl_context_ptr_compile_shader: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_create_program: Symbol<extern fn(_:  &AzGlContextPtr) -> u32>,
        pub az_gl_context_ptr_delete_program: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_create_shader: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32) -> u32>,
        pub az_gl_context_ptr_delete_shader: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_detach_shader: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_link_program: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_clear_color: Symbol<extern fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32)>,
        pub az_gl_context_ptr_clear: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_clear_depth: Symbol<extern fn(_:  &AzGlContextPtr, _:  f64)>,
        pub az_gl_context_ptr_clear_stencil: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32)>,
        pub az_gl_context_ptr_flush: Symbol<extern fn(_:  &AzGlContextPtr)>,
        pub az_gl_context_ptr_finish: Symbol<extern fn(_:  &AzGlContextPtr)>,
        pub az_gl_context_ptr_get_error: Symbol<extern fn(_:  &AzGlContextPtr) -> u32>,
        pub az_gl_context_ptr_stencil_mask: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_stencil_mask_separate: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_stencil_func: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32)>,
        pub az_gl_context_ptr_stencil_func_separate: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  u32)>,
        pub az_gl_context_ptr_stencil_op: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_stencil_op_separate: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_egl_image_target_texture2d_oes: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void)>,
        pub az_gl_context_ptr_generate_mipmap: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_insert_event_marker_ext: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzRefstr)>,
        pub az_gl_context_ptr_push_group_marker_ext: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzRefstr)>,
        pub az_gl_context_ptr_pop_group_marker_ext: Symbol<extern fn(_:  &AzGlContextPtr)>,
        pub az_gl_context_ptr_debug_message_insert_khr: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  AzRefstr)>,
        pub az_gl_context_ptr_push_debug_group_khr: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr)>,
        pub az_gl_context_ptr_pop_debug_group_khr: Symbol<extern fn(_:  &AzGlContextPtr)>,
        pub az_gl_context_ptr_fence_sync: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGLsyncPtr>,
        pub az_gl_context_ptr_client_wait_sync: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64)>,
        pub az_gl_context_ptr_wait_sync: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64)>,
        pub az_gl_context_ptr_delete_sync: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr)>,
        pub az_gl_context_ptr_texture_range_apple: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzU8VecRef)>,
        pub az_gl_context_ptr_gen_fences_apple: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>,
        pub az_gl_context_ptr_delete_fences_apple: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>,
        pub az_gl_context_ptr_set_fence_apple: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_finish_fence_apple: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_test_fence_apple: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_test_object_apple: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u8>,
        pub az_gl_context_ptr_finish_object_apple: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>,
        pub az_gl_context_ptr_get_frag_data_index: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32>,
        pub az_gl_context_ptr_blend_barrier_khr: Symbol<extern fn(_:  &AzGlContextPtr)>,
        pub az_gl_context_ptr_bind_frag_data_location_indexed: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  AzRefstr)>,
        pub az_gl_context_ptr_get_debug_messages: Symbol<extern fn(_:  &AzGlContextPtr) -> AzDebugMessageVec>,
        pub az_gl_context_ptr_provoking_vertex_angle: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_gen_vertex_arrays_apple: Symbol<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>,
        pub az_gl_context_ptr_bind_vertex_array_apple: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32)>,
        pub az_gl_context_ptr_delete_vertex_arrays_apple: Symbol<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>,
        pub az_gl_context_ptr_copy_texture_chromium: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8)>,
        pub az_gl_context_ptr_copy_sub_texture_chromium: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8)>,
        pub az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void)>,
        pub az_gl_context_ptr_copy_texture_3d_angle: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8)>,
        pub az_gl_context_ptr_copy_sub_texture_3d_angle: Symbol<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8)>,
        pub az_gl_context_ptr_delete: Symbol<extern fn(_:  &mut AzGlContextPtr)>,
        pub az_gl_context_ptr_deep_copy: Symbol<extern fn(_:  &AzGlContextPtr) -> AzGlContextPtr>,
        pub az_texture_delete: Symbol<extern fn(_:  &mut AzTexture)>,
        pub az_texture_flags_delete: Symbol<extern fn(_:  &mut AzTextureFlags)>,
        pub az_texture_flags_deep_copy: Symbol<extern fn(_:  &AzTextureFlags) -> AzTextureFlags>,
        pub az_text_id_new: Symbol<extern fn() -> AzTextId>,
        pub az_text_id_delete: Symbol<extern fn(_:  &mut AzTextId)>,
        pub az_text_id_deep_copy: Symbol<extern fn(_:  &AzTextId) -> AzTextId>,
        pub az_image_id_new: Symbol<extern fn() -> AzImageId>,
        pub az_image_id_delete: Symbol<extern fn(_:  &mut AzImageId)>,
        pub az_image_id_deep_copy: Symbol<extern fn(_:  &AzImageId) -> AzImageId>,
        pub az_font_id_new: Symbol<extern fn() -> AzFontId>,
        pub az_font_id_delete: Symbol<extern fn(_:  &mut AzFontId)>,
        pub az_font_id_deep_copy: Symbol<extern fn(_:  &AzFontId) -> AzFontId>,
        pub az_image_source_delete: Symbol<extern fn(_:  &mut AzImageSource)>,
        pub az_image_source_deep_copy: Symbol<extern fn(_:  &AzImageSource) -> AzImageSource>,
        pub az_font_source_delete: Symbol<extern fn(_:  &mut AzFontSource)>,
        pub az_font_source_deep_copy: Symbol<extern fn(_:  &AzFontSource) -> AzFontSource>,
        pub az_raw_image_new: Symbol<extern fn(_:  AzU8Vec, _:  usize, _:  usize, _:  AzRawImageFormat) -> AzRawImage>,
        pub az_raw_image_delete: Symbol<extern fn(_:  &mut AzRawImage)>,
        pub az_raw_image_deep_copy: Symbol<extern fn(_:  &AzRawImage) -> AzRawImage>,
        pub az_raw_image_format_delete: Symbol<extern fn(_:  &mut AzRawImageFormat)>,
        pub az_raw_image_format_deep_copy: Symbol<extern fn(_:  &AzRawImageFormat) -> AzRawImageFormat>,
        pub az_drop_check_ptr_delete: Symbol<extern fn(_:  &mut AzDropCheckPtrPtr)>,
        pub az_arc_mutex_ref_any_delete: Symbol<extern fn(_:  &mut AzArcMutexRefAnyPtr)>,
        pub az_timer_delete: Symbol<extern fn(_:  &mut AzTimer)>,
        pub az_timer_deep_copy: Symbol<extern fn(_:  &AzTimer) -> AzTimer>,
        pub az_task_new: Symbol<extern fn(_:  AzArcMutexRefAnyPtr, _:  AzTaskCallbackType) -> AzTaskPtr>,
        pub az_task_then: Symbol<extern fn(_:  AzTaskPtr, _:  AzTimer) -> AzTaskPtr>,
        pub az_task_delete: Symbol<extern fn(_:  &mut AzTaskPtr)>,
        pub az_thread_new: Symbol<extern fn(_:  AzRefAny, _:  AzThreadCallbackType) -> AzThreadPtr>,
        pub az_thread_block: Symbol<extern fn(_:  AzThreadPtr) -> AzResultRefAnyBlockError>,
        pub az_thread_delete: Symbol<extern fn(_:  &mut AzThreadPtr)>,
        pub az_drop_check_delete: Symbol<extern fn(_:  &mut AzDropCheckPtr)>,
        pub az_timer_id_delete: Symbol<extern fn(_:  &mut AzTimerId)>,
        pub az_timer_id_deep_copy: Symbol<extern fn(_:  &AzTimerId) -> AzTimerId>,
        pub az_terminate_timer_delete: Symbol<extern fn(_:  &mut AzTerminateTimer)>,
        pub az_terminate_timer_deep_copy: Symbol<extern fn(_:  &AzTerminateTimer) -> AzTerminateTimer>,
        pub az_block_error_delete: Symbol<extern fn(_:  &mut AzBlockError)>,
        pub az_block_error_deep_copy: Symbol<extern fn(_:  &AzBlockError) -> AzBlockError>,
        pub az_task_bar_icon_delete: Symbol<extern fn(_:  &mut AzTaskBarIcon)>,
        pub az_task_bar_icon_deep_copy: Symbol<extern fn(_:  &AzTaskBarIcon) -> AzTaskBarIcon>,
        pub az_x_window_type_delete: Symbol<extern fn(_:  &mut AzXWindowType)>,
        pub az_x_window_type_deep_copy: Symbol<extern fn(_:  &AzXWindowType) -> AzXWindowType>,
        pub az_physical_position_i32_delete: Symbol<extern fn(_:  &mut AzPhysicalPositionI32)>,
        pub az_physical_position_i32_deep_copy: Symbol<extern fn(_:  &AzPhysicalPositionI32) -> AzPhysicalPositionI32>,
        pub az_logical_position_delete: Symbol<extern fn(_:  &mut AzLogicalPosition)>,
        pub az_logical_position_deep_copy: Symbol<extern fn(_:  &AzLogicalPosition) -> AzLogicalPosition>,
        pub az_icon_key_delete: Symbol<extern fn(_:  &mut AzIconKey)>,
        pub az_icon_key_deep_copy: Symbol<extern fn(_:  &AzIconKey) -> AzIconKey>,
        pub az_small_window_icon_bytes_delete: Symbol<extern fn(_:  &mut AzSmallWindowIconBytes)>,
        pub az_small_window_icon_bytes_deep_copy: Symbol<extern fn(_:  &AzSmallWindowIconBytes) -> AzSmallWindowIconBytes>,
        pub az_large_window_icon_bytes_delete: Symbol<extern fn(_:  &mut AzLargeWindowIconBytes)>,
        pub az_large_window_icon_bytes_deep_copy: Symbol<extern fn(_:  &AzLargeWindowIconBytes) -> AzLargeWindowIconBytes>,
        pub az_window_icon_delete: Symbol<extern fn(_:  &mut AzWindowIcon)>,
        pub az_window_icon_deep_copy: Symbol<extern fn(_:  &AzWindowIcon) -> AzWindowIcon>,
        pub az_virtual_key_code_delete: Symbol<extern fn(_:  &mut AzVirtualKeyCode)>,
        pub az_virtual_key_code_deep_copy: Symbol<extern fn(_:  &AzVirtualKeyCode) -> AzVirtualKeyCode>,
        pub az_accelerator_key_delete: Symbol<extern fn(_:  &mut AzAcceleratorKey)>,
        pub az_accelerator_key_deep_copy: Symbol<extern fn(_:  &AzAcceleratorKey) -> AzAcceleratorKey>,
        pub az_window_size_delete: Symbol<extern fn(_:  &mut AzWindowSize)>,
        pub az_window_size_deep_copy: Symbol<extern fn(_:  &AzWindowSize) -> AzWindowSize>,
        pub az_window_flags_delete: Symbol<extern fn(_:  &mut AzWindowFlags)>,
        pub az_window_flags_deep_copy: Symbol<extern fn(_:  &AzWindowFlags) -> AzWindowFlags>,
        pub az_debug_state_delete: Symbol<extern fn(_:  &mut AzDebugState)>,
        pub az_debug_state_deep_copy: Symbol<extern fn(_:  &AzDebugState) -> AzDebugState>,
        pub az_keyboard_state_delete: Symbol<extern fn(_:  &mut AzKeyboardState)>,
        pub az_keyboard_state_deep_copy: Symbol<extern fn(_:  &AzKeyboardState) -> AzKeyboardState>,
        pub az_mouse_cursor_type_delete: Symbol<extern fn(_:  &mut AzMouseCursorType)>,
        pub az_mouse_cursor_type_deep_copy: Symbol<extern fn(_:  &AzMouseCursorType) -> AzMouseCursorType>,
        pub az_cursor_position_delete: Symbol<extern fn(_:  &mut AzCursorPosition)>,
        pub az_cursor_position_deep_copy: Symbol<extern fn(_:  &AzCursorPosition) -> AzCursorPosition>,
        pub az_mouse_state_delete: Symbol<extern fn(_:  &mut AzMouseState)>,
        pub az_mouse_state_deep_copy: Symbol<extern fn(_:  &AzMouseState) -> AzMouseState>,
        pub az_platform_specific_options_delete: Symbol<extern fn(_:  &mut AzPlatformSpecificOptions)>,
        pub az_platform_specific_options_deep_copy: Symbol<extern fn(_:  &AzPlatformSpecificOptions) -> AzPlatformSpecificOptions>,
        pub az_windows_window_options_delete: Symbol<extern fn(_:  &mut AzWindowsWindowOptions)>,
        pub az_windows_window_options_deep_copy: Symbol<extern fn(_:  &AzWindowsWindowOptions) -> AzWindowsWindowOptions>,
        pub az_wayland_theme_delete: Symbol<extern fn(_:  &mut AzWaylandTheme)>,
        pub az_wayland_theme_deep_copy: Symbol<extern fn(_:  &AzWaylandTheme) -> AzWaylandTheme>,
        pub az_renderer_type_delete: Symbol<extern fn(_:  &mut AzRendererType)>,
        pub az_renderer_type_deep_copy: Symbol<extern fn(_:  &AzRendererType) -> AzRendererType>,
        pub az_string_pair_delete: Symbol<extern fn(_:  &mut AzStringPair)>,
        pub az_string_pair_deep_copy: Symbol<extern fn(_:  &AzStringPair) -> AzStringPair>,
        pub az_linux_window_options_delete: Symbol<extern fn(_:  &mut AzLinuxWindowOptions)>,
        pub az_linux_window_options_deep_copy: Symbol<extern fn(_:  &AzLinuxWindowOptions) -> AzLinuxWindowOptions>,
        pub az_mac_window_options_delete: Symbol<extern fn(_:  &mut AzMacWindowOptions)>,
        pub az_mac_window_options_deep_copy: Symbol<extern fn(_:  &AzMacWindowOptions) -> AzMacWindowOptions>,
        pub az_wasm_window_options_delete: Symbol<extern fn(_:  &mut AzWasmWindowOptions)>,
        pub az_wasm_window_options_deep_copy: Symbol<extern fn(_:  &AzWasmWindowOptions) -> AzWasmWindowOptions>,
        pub az_full_screen_mode_delete: Symbol<extern fn(_:  &mut AzFullScreenMode)>,
        pub az_full_screen_mode_deep_copy: Symbol<extern fn(_:  &AzFullScreenMode) -> AzFullScreenMode>,
        pub az_window_state_delete: Symbol<extern fn(_:  &mut AzWindowState)>,
        pub az_window_state_deep_copy: Symbol<extern fn(_:  &AzWindowState) -> AzWindowState>,
        pub az_logical_size_delete: Symbol<extern fn(_:  &mut AzLogicalSize)>,
        pub az_logical_size_deep_copy: Symbol<extern fn(_:  &AzLogicalSize) -> AzLogicalSize>,
        pub az_hot_reload_options_delete: Symbol<extern fn(_:  &mut AzHotReloadOptions)>,
        pub az_hot_reload_options_deep_copy: Symbol<extern fn(_:  &AzHotReloadOptions) -> AzHotReloadOptions>,
        pub az_window_create_options_new: Symbol<extern fn(_:  AzCss) -> AzWindowCreateOptions>,
        pub az_window_create_options_delete: Symbol<extern fn(_:  &mut AzWindowCreateOptions)>,
        pub az_window_create_options_deep_copy: Symbol<extern fn(_:  &AzWindowCreateOptions) -> AzWindowCreateOptions>,
    }

    pub fn initialize_library(path: &std::path::Path) -> Result<AzulDll, &'static str> {
        let lib = Library::new(path).map_err(|_| "library is not a DLL file (?!)")?;
        let az_string_from_utf8_unchecked = unsafe { lib.get::<extern fn(_:  *const u8, _:  usize) -> AzString>(b"az_string_from_utf8_unchecked").map_err(|_| "az_string_from_utf8_unchecked")? };
        let az_string_from_utf8_lossy = unsafe { lib.get::<extern fn(_:  *const u8, _:  usize) -> AzString>(b"az_string_from_utf8_lossy").map_err(|_| "az_string_from_utf8_lossy")? };
        let az_string_into_bytes = unsafe { lib.get::<extern fn(_:  AzString) -> AzU8Vec>(b"az_string_into_bytes").map_err(|_| "az_string_into_bytes")? };
        let az_string_delete = unsafe { lib.get::<extern fn(_:  &mut AzString)>(b"az_string_delete").map_err(|_| "az_string_delete")? };
        let az_string_deep_copy = unsafe { lib.get::<extern fn(_:  &AzString) -> AzString>(b"az_string_deep_copy").map_err(|_| "az_string_deep_copy")? };
        let az_x_window_type_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzXWindowType, _:  usize) -> AzXWindowTypeVec>(b"az_x_window_type_vec_copy_from").map_err(|_| "az_x_window_type_vec_copy_from")? };
        let az_x_window_type_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzXWindowTypeVec)>(b"az_x_window_type_vec_delete").map_err(|_| "az_x_window_type_vec_delete")? };
        let az_x_window_type_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzXWindowTypeVec) -> AzXWindowTypeVec>(b"az_x_window_type_vec_deep_copy").map_err(|_| "az_x_window_type_vec_deep_copy")? };
        let az_virtual_key_code_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzVirtualKeyCode, _:  usize) -> AzVirtualKeyCodeVec>(b"az_virtual_key_code_vec_copy_from").map_err(|_| "az_virtual_key_code_vec_copy_from")? };
        let az_virtual_key_code_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzVirtualKeyCodeVec)>(b"az_virtual_key_code_vec_delete").map_err(|_| "az_virtual_key_code_vec_delete")? };
        let az_virtual_key_code_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzVirtualKeyCodeVec) -> AzVirtualKeyCodeVec>(b"az_virtual_key_code_vec_deep_copy").map_err(|_| "az_virtual_key_code_vec_deep_copy")? };
        let az_scan_code_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const u32, _:  usize) -> AzScanCodeVec>(b"az_scan_code_vec_copy_from").map_err(|_| "az_scan_code_vec_copy_from")? };
        let az_scan_code_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzScanCodeVec)>(b"az_scan_code_vec_delete").map_err(|_| "az_scan_code_vec_delete")? };
        let az_scan_code_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzScanCodeVec) -> AzScanCodeVec>(b"az_scan_code_vec_deep_copy").map_err(|_| "az_scan_code_vec_deep_copy")? };
        let az_css_declaration_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzCssDeclaration, _:  usize) -> AzCssDeclarationVec>(b"az_css_declaration_vec_copy_from").map_err(|_| "az_css_declaration_vec_copy_from")? };
        let az_css_declaration_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssDeclarationVec)>(b"az_css_declaration_vec_delete").map_err(|_| "az_css_declaration_vec_delete")? };
        let az_css_declaration_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssDeclarationVec) -> AzCssDeclarationVec>(b"az_css_declaration_vec_deep_copy").map_err(|_| "az_css_declaration_vec_deep_copy")? };
        let az_css_path_selector_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzCssPathSelector, _:  usize) -> AzCssPathSelectorVec>(b"az_css_path_selector_vec_copy_from").map_err(|_| "az_css_path_selector_vec_copy_from")? };
        let az_css_path_selector_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssPathSelectorVec)>(b"az_css_path_selector_vec_delete").map_err(|_| "az_css_path_selector_vec_delete")? };
        let az_css_path_selector_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssPathSelectorVec) -> AzCssPathSelectorVec>(b"az_css_path_selector_vec_deep_copy").map_err(|_| "az_css_path_selector_vec_deep_copy")? };
        let az_stylesheet_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzStylesheet, _:  usize) -> AzStylesheetVec>(b"az_stylesheet_vec_copy_from").map_err(|_| "az_stylesheet_vec_copy_from")? };
        let az_stylesheet_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzStylesheetVec)>(b"az_stylesheet_vec_delete").map_err(|_| "az_stylesheet_vec_delete")? };
        let az_stylesheet_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStylesheetVec) -> AzStylesheetVec>(b"az_stylesheet_vec_deep_copy").map_err(|_| "az_stylesheet_vec_deep_copy")? };
        let az_css_rule_block_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzCssRuleBlock, _:  usize) -> AzCssRuleBlockVec>(b"az_css_rule_block_vec_copy_from").map_err(|_| "az_css_rule_block_vec_copy_from")? };
        let az_css_rule_block_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssRuleBlockVec)>(b"az_css_rule_block_vec_delete").map_err(|_| "az_css_rule_block_vec_delete")? };
        let az_css_rule_block_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssRuleBlockVec) -> AzCssRuleBlockVec>(b"az_css_rule_block_vec_deep_copy").map_err(|_| "az_css_rule_block_vec_deep_copy")? };
        let az_u8_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const u8, _:  usize) -> AzU8Vec>(b"az_u8_vec_copy_from").map_err(|_| "az_u8_vec_copy_from")? };
        let az_u8_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzU8Vec)>(b"az_u8_vec_delete").map_err(|_| "az_u8_vec_delete")? };
        let az_u8_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzU8Vec) -> AzU8Vec>(b"az_u8_vec_deep_copy").map_err(|_| "az_u8_vec_deep_copy")? };
        let az_callback_data_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzCallbackData, _:  usize) -> AzCallbackDataVec>(b"az_callback_data_vec_copy_from").map_err(|_| "az_callback_data_vec_copy_from")? };
        let az_callback_data_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzCallbackDataVec)>(b"az_callback_data_vec_delete").map_err(|_| "az_callback_data_vec_delete")? };
        let az_callback_data_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCallbackDataVec) -> AzCallbackDataVec>(b"az_callback_data_vec_deep_copy").map_err(|_| "az_callback_data_vec_deep_copy")? };
        let az_debug_message_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzDebugMessage, _:  usize) -> AzDebugMessageVec>(b"az_debug_message_vec_copy_from").map_err(|_| "az_debug_message_vec_copy_from")? };
        let az_debug_message_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzDebugMessageVec)>(b"az_debug_message_vec_delete").map_err(|_| "az_debug_message_vec_delete")? };
        let az_debug_message_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDebugMessageVec) -> AzDebugMessageVec>(b"az_debug_message_vec_deep_copy").map_err(|_| "az_debug_message_vec_deep_copy")? };
        let az_g_luint_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const u32, _:  usize) -> AzGLuintVec>(b"az_g_luint_vec_copy_from").map_err(|_| "az_g_luint_vec_copy_from")? };
        let az_g_luint_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLuintVec)>(b"az_g_luint_vec_delete").map_err(|_| "az_g_luint_vec_delete")? };
        let az_g_luint_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGLuintVec) -> AzGLuintVec>(b"az_g_luint_vec_deep_copy").map_err(|_| "az_g_luint_vec_deep_copy")? };
        let az_g_lint_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const i32, _:  usize) -> AzGLintVec>(b"az_g_lint_vec_copy_from").map_err(|_| "az_g_lint_vec_copy_from")? };
        let az_g_lint_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLintVec)>(b"az_g_lint_vec_delete").map_err(|_| "az_g_lint_vec_delete")? };
        let az_g_lint_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGLintVec) -> AzGLintVec>(b"az_g_lint_vec_deep_copy").map_err(|_| "az_g_lint_vec_deep_copy")? };
        let az_override_property_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzOverrideProperty, _:  usize) -> AzOverridePropertyVec>(b"az_override_property_vec_copy_from").map_err(|_| "az_override_property_vec_copy_from")? };
        let az_override_property_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzOverridePropertyVec)>(b"az_override_property_vec_delete").map_err(|_| "az_override_property_vec_delete")? };
        let az_override_property_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOverridePropertyVec) -> AzOverridePropertyVec>(b"az_override_property_vec_deep_copy").map_err(|_| "az_override_property_vec_deep_copy")? };
        let az_dom_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzDom, _:  usize) -> AzDomVec>(b"az_dom_vec_copy_from").map_err(|_| "az_dom_vec_copy_from")? };
        let az_dom_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzDomVec)>(b"az_dom_vec_delete").map_err(|_| "az_dom_vec_delete")? };
        let az_dom_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDomVec) -> AzDomVec>(b"az_dom_vec_deep_copy").map_err(|_| "az_dom_vec_deep_copy")? };
        let az_string_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzString, _:  usize) -> AzStringVec>(b"az_string_vec_copy_from").map_err(|_| "az_string_vec_copy_from")? };
        let az_string_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzStringVec)>(b"az_string_vec_delete").map_err(|_| "az_string_vec_delete")? };
        let az_string_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStringVec) -> AzStringVec>(b"az_string_vec_deep_copy").map_err(|_| "az_string_vec_deep_copy")? };
        let az_string_pair_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzStringPair, _:  usize) -> AzStringPairVec>(b"az_string_pair_vec_copy_from").map_err(|_| "az_string_pair_vec_copy_from")? };
        let az_string_pair_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzStringPairVec)>(b"az_string_pair_vec_delete").map_err(|_| "az_string_pair_vec_delete")? };
        let az_string_pair_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStringPairVec) -> AzStringPairVec>(b"az_string_pair_vec_deep_copy").map_err(|_| "az_string_pair_vec_deep_copy")? };
        let az_gradient_stop_pre_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzGradientStopPre, _:  usize) -> AzGradientStopPreVec>(b"az_gradient_stop_pre_vec_copy_from").map_err(|_| "az_gradient_stop_pre_vec_copy_from")? };
        let az_gradient_stop_pre_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzGradientStopPreVec)>(b"az_gradient_stop_pre_vec_delete").map_err(|_| "az_gradient_stop_pre_vec_delete")? };
        let az_gradient_stop_pre_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGradientStopPreVec) -> AzGradientStopPreVec>(b"az_gradient_stop_pre_vec_deep_copy").map_err(|_| "az_gradient_stop_pre_vec_deep_copy")? };
        let az_option_wayland_theme_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionWaylandTheme)>(b"az_option_wayland_theme_delete").map_err(|_| "az_option_wayland_theme_delete")? };
        let az_option_wayland_theme_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionWaylandTheme) -> AzOptionWaylandTheme>(b"az_option_wayland_theme_deep_copy").map_err(|_| "az_option_wayland_theme_deep_copy")? };
        let az_option_task_bar_icon_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionTaskBarIcon)>(b"az_option_task_bar_icon_delete").map_err(|_| "az_option_task_bar_icon_delete")? };
        let az_option_task_bar_icon_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionTaskBarIcon) -> AzOptionTaskBarIcon>(b"az_option_task_bar_icon_deep_copy").map_err(|_| "az_option_task_bar_icon_deep_copy")? };
        let az_option_hwnd_handle_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionHwndHandle)>(b"az_option_hwnd_handle_delete").map_err(|_| "az_option_hwnd_handle_delete")? };
        let az_option_hwnd_handle_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionHwndHandle) -> AzOptionHwndHandle>(b"az_option_hwnd_handle_deep_copy").map_err(|_| "az_option_hwnd_handle_deep_copy")? };
        let az_option_logical_position_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionLogicalPosition)>(b"az_option_logical_position_delete").map_err(|_| "az_option_logical_position_delete")? };
        let az_option_logical_position_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionLogicalPosition) -> AzOptionLogicalPosition>(b"az_option_logical_position_deep_copy").map_err(|_| "az_option_logical_position_deep_copy")? };
        let az_option_hot_reload_options_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionHotReloadOptions)>(b"az_option_hot_reload_options_delete").map_err(|_| "az_option_hot_reload_options_delete")? };
        let az_option_hot_reload_options_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionHotReloadOptions) -> AzOptionHotReloadOptions>(b"az_option_hot_reload_options_deep_copy").map_err(|_| "az_option_hot_reload_options_deep_copy")? };
        let az_option_physical_position_i32_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionPhysicalPositionI32)>(b"az_option_physical_position_i32_delete").map_err(|_| "az_option_physical_position_i32_delete")? };
        let az_option_physical_position_i32_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionPhysicalPositionI32) -> AzOptionPhysicalPositionI32>(b"az_option_physical_position_i32_deep_copy").map_err(|_| "az_option_physical_position_i32_deep_copy")? };
        let az_option_window_icon_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionWindowIcon)>(b"az_option_window_icon_delete").map_err(|_| "az_option_window_icon_delete")? };
        let az_option_window_icon_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionWindowIcon) -> AzOptionWindowIcon>(b"az_option_window_icon_deep_copy").map_err(|_| "az_option_window_icon_deep_copy")? };
        let az_option_string_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionString)>(b"az_option_string_delete").map_err(|_| "az_option_string_delete")? };
        let az_option_string_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionString) -> AzOptionString>(b"az_option_string_deep_copy").map_err(|_| "az_option_string_deep_copy")? };
        let az_option_x11_visual_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionX11Visual)>(b"az_option_x11_visual_delete").map_err(|_| "az_option_x11_visual_delete")? };
        let az_option_x11_visual_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionX11Visual) -> AzOptionX11Visual>(b"az_option_x11_visual_deep_copy").map_err(|_| "az_option_x11_visual_deep_copy")? };
        let az_option_i32_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionI32)>(b"az_option_i32_delete").map_err(|_| "az_option_i32_delete")? };
        let az_option_i32_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionI32) -> AzOptionI32>(b"az_option_i32_deep_copy").map_err(|_| "az_option_i32_deep_copy")? };
        let az_option_f32_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionF32)>(b"az_option_f32_delete").map_err(|_| "az_option_f32_delete")? };
        let az_option_f32_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionF32) -> AzOptionF32>(b"az_option_f32_deep_copy").map_err(|_| "az_option_f32_deep_copy")? };
        let az_option_mouse_cursor_type_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionMouseCursorType)>(b"az_option_mouse_cursor_type_delete").map_err(|_| "az_option_mouse_cursor_type_delete")? };
        let az_option_mouse_cursor_type_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionMouseCursorType) -> AzOptionMouseCursorType>(b"az_option_mouse_cursor_type_deep_copy").map_err(|_| "az_option_mouse_cursor_type_deep_copy")? };
        let az_option_logical_size_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionLogicalSize)>(b"az_option_logical_size_delete").map_err(|_| "az_option_logical_size_delete")? };
        let az_option_logical_size_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionLogicalSize) -> AzOptionLogicalSize>(b"az_option_logical_size_deep_copy").map_err(|_| "az_option_logical_size_deep_copy")? };
        let az_option_char_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionChar)>(b"az_option_char_delete").map_err(|_| "az_option_char_delete")? };
        let az_option_char_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionChar) -> AzOptionChar>(b"az_option_char_deep_copy").map_err(|_| "az_option_char_deep_copy")? };
        let az_option_virtual_key_code_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionVirtualKeyCode)>(b"az_option_virtual_key_code_delete").map_err(|_| "az_option_virtual_key_code_delete")? };
        let az_option_virtual_key_code_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionVirtualKeyCode) -> AzOptionVirtualKeyCode>(b"az_option_virtual_key_code_deep_copy").map_err(|_| "az_option_virtual_key_code_deep_copy")? };
        let az_option_percentage_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionPercentageValue)>(b"az_option_percentage_value_delete").map_err(|_| "az_option_percentage_value_delete")? };
        let az_option_percentage_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionPercentageValue) -> AzOptionPercentageValue>(b"az_option_percentage_value_deep_copy").map_err(|_| "az_option_percentage_value_deep_copy")? };
        let az_option_dom_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionDom)>(b"az_option_dom_delete").map_err(|_| "az_option_dom_delete")? };
        let az_option_dom_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionDom) -> AzOptionDom>(b"az_option_dom_deep_copy").map_err(|_| "az_option_dom_deep_copy")? };
        let az_option_texture_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionTexture)>(b"az_option_texture_delete").map_err(|_| "az_option_texture_delete")? };
        let az_option_tab_index_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionTabIndex)>(b"az_option_tab_index_delete").map_err(|_| "az_option_tab_index_delete")? };
        let az_option_tab_index_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionTabIndex) -> AzOptionTabIndex>(b"az_option_tab_index_deep_copy").map_err(|_| "az_option_tab_index_deep_copy")? };
        let az_option_duration_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionDuration)>(b"az_option_duration_delete").map_err(|_| "az_option_duration_delete")? };
        let az_option_duration_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionDuration) -> AzOptionDuration>(b"az_option_duration_deep_copy").map_err(|_| "az_option_duration_deep_copy")? };
        let az_option_instant_ptr_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionInstantPtr)>(b"az_option_instant_ptr_delete").map_err(|_| "az_option_instant_ptr_delete")? };
        let az_option_instant_ptr_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionInstantPtr) -> AzOptionInstantPtr>(b"az_option_instant_ptr_deep_copy").map_err(|_| "az_option_instant_ptr_deep_copy")? };
        let az_option_usize_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionUsize)>(b"az_option_usize_delete").map_err(|_| "az_option_usize_delete")? };
        let az_option_usize_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionUsize) -> AzOptionUsize>(b"az_option_usize_deep_copy").map_err(|_| "az_option_usize_deep_copy")? };
        let az_option_u8_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionU8VecRef)>(b"az_option_u8_vec_ref_delete").map_err(|_| "az_option_u8_vec_ref_delete")? };
        let az_result_ref_any_block_error_delete = unsafe { lib.get::<extern fn(_:  &mut AzResultRefAnyBlockError)>(b"az_result_ref_any_block_error_delete").map_err(|_| "az_result_ref_any_block_error_delete")? };
        let az_result_ref_any_block_error_deep_copy = unsafe { lib.get::<extern fn(_:  &AzResultRefAnyBlockError) -> AzResultRefAnyBlockError>(b"az_result_ref_any_block_error_deep_copy").map_err(|_| "az_result_ref_any_block_error_deep_copy")? };
        let az_instant_now = unsafe { lib.get::<extern fn() -> AzInstantPtr>(b"az_instant_now").map_err(|_| "az_instant_now")? };
        let az_instant_delete = unsafe { lib.get::<extern fn(_:  &mut AzInstantPtr)>(b"az_instant_delete").map_err(|_| "az_instant_delete")? };
        let az_duration_delete = unsafe { lib.get::<extern fn(_:  &mut AzDuration)>(b"az_duration_delete").map_err(|_| "az_duration_delete")? };
        let az_duration_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDuration) -> AzDuration>(b"az_duration_deep_copy").map_err(|_| "az_duration_deep_copy")? };
        let az_app_config_default = unsafe { lib.get::<extern fn() -> AzAppConfigPtr>(b"az_app_config_default").map_err(|_| "az_app_config_default")? };
        let az_app_config_delete = unsafe { lib.get::<extern fn(_:  &mut AzAppConfigPtr)>(b"az_app_config_delete").map_err(|_| "az_app_config_delete")? };
        let az_app_new = unsafe { lib.get::<extern fn(_:  AzRefAny, _:  AzAppConfigPtr, _:  AzLayoutCallbackType) -> AzAppPtr>(b"az_app_new").map_err(|_| "az_app_new")? };
        let az_app_run = unsafe { lib.get::<extern fn(_:  AzAppPtr, _:  AzWindowCreateOptions)>(b"az_app_run").map_err(|_| "az_app_run")? };
        let az_app_delete = unsafe { lib.get::<extern fn(_:  &mut AzAppPtr)>(b"az_app_delete").map_err(|_| "az_app_delete")? };
        let az_layout_callback_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutCallback)>(b"az_layout_callback_delete").map_err(|_| "az_layout_callback_delete")? };
        let az_layout_callback_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutCallback) -> AzLayoutCallback>(b"az_layout_callback_deep_copy").map_err(|_| "az_layout_callback_deep_copy")? };
        let az_callback_delete = unsafe { lib.get::<extern fn(_:  &mut AzCallback)>(b"az_callback_delete").map_err(|_| "az_callback_delete")? };
        let az_callback_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCallback) -> AzCallback>(b"az_callback_deep_copy").map_err(|_| "az_callback_deep_copy")? };
        let az_callback_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzCallbackInfoPtr)>(b"az_callback_info_delete").map_err(|_| "az_callback_info_delete")? };
        let az_update_screen_delete = unsafe { lib.get::<extern fn(_:  &mut AzUpdateScreen)>(b"az_update_screen_delete").map_err(|_| "az_update_screen_delete")? };
        let az_update_screen_deep_copy = unsafe { lib.get::<extern fn(_:  &AzUpdateScreen) -> AzUpdateScreen>(b"az_update_screen_deep_copy").map_err(|_| "az_update_screen_deep_copy")? };
        let az_i_frame_callback_delete = unsafe { lib.get::<extern fn(_:  &mut AzIFrameCallback)>(b"az_i_frame_callback_delete").map_err(|_| "az_i_frame_callback_delete")? };
        let az_i_frame_callback_deep_copy = unsafe { lib.get::<extern fn(_:  &AzIFrameCallback) -> AzIFrameCallback>(b"az_i_frame_callback_deep_copy").map_err(|_| "az_i_frame_callback_deep_copy")? };
        let az_i_frame_callback_info_get_state = unsafe { lib.get::<extern fn(_:  &AzIFrameCallbackInfoPtr) -> AzRefAny>(b"az_i_frame_callback_info_get_state").map_err(|_| "az_i_frame_callback_info_get_state")? };
        let az_i_frame_callback_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzIFrameCallbackInfoPtr)>(b"az_i_frame_callback_info_delete").map_err(|_| "az_i_frame_callback_info_delete")? };
        let az_i_frame_callback_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzIFrameCallbackReturn)>(b"az_i_frame_callback_return_delete").map_err(|_| "az_i_frame_callback_return_delete")? };
        let az_i_frame_callback_return_deep_copy = unsafe { lib.get::<extern fn(_:  &AzIFrameCallbackReturn) -> AzIFrameCallbackReturn>(b"az_i_frame_callback_return_deep_copy").map_err(|_| "az_i_frame_callback_return_deep_copy")? };
        let az_gl_callback_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlCallback)>(b"az_gl_callback_delete").map_err(|_| "az_gl_callback_delete")? };
        let az_gl_callback_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGlCallback) -> AzGlCallback>(b"az_gl_callback_deep_copy").map_err(|_| "az_gl_callback_deep_copy")? };
        let az_gl_callback_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlCallbackInfoPtr)>(b"az_gl_callback_info_delete").map_err(|_| "az_gl_callback_info_delete")? };
        let az_gl_callback_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlCallbackReturn)>(b"az_gl_callback_return_delete").map_err(|_| "az_gl_callback_return_delete")? };
        let az_timer_callback_delete = unsafe { lib.get::<extern fn(_:  &mut AzTimerCallback)>(b"az_timer_callback_delete").map_err(|_| "az_timer_callback_delete")? };
        let az_timer_callback_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTimerCallback) -> AzTimerCallback>(b"az_timer_callback_deep_copy").map_err(|_| "az_timer_callback_deep_copy")? };
        let az_timer_callback_type_delete = unsafe { lib.get::<extern fn(_:  &mut AzTimerCallbackTypePtr)>(b"az_timer_callback_type_delete").map_err(|_| "az_timer_callback_type_delete")? };
        let az_timer_callback_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzTimerCallbackInfoPtr)>(b"az_timer_callback_info_delete").map_err(|_| "az_timer_callback_info_delete")? };
        let az_timer_callback_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzTimerCallbackReturn)>(b"az_timer_callback_return_delete").map_err(|_| "az_timer_callback_return_delete")? };
        let az_timer_callback_return_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTimerCallbackReturn) -> AzTimerCallbackReturn>(b"az_timer_callback_return_deep_copy").map_err(|_| "az_timer_callback_return_deep_copy")? };
        let az_ref_any_sharing_info_can_be_shared = unsafe { lib.get::<extern fn(_:  &AzRefAnySharingInfo) -> bool>(b"az_ref_any_sharing_info_can_be_shared").map_err(|_| "az_ref_any_sharing_info_can_be_shared")? };
        let az_ref_any_sharing_info_can_be_shared_mut = unsafe { lib.get::<extern fn(_:  &AzRefAnySharingInfo) -> bool>(b"az_ref_any_sharing_info_can_be_shared_mut").map_err(|_| "az_ref_any_sharing_info_can_be_shared_mut")? };
        let az_ref_any_sharing_info_increase_ref = unsafe { lib.get::<extern fn(_:  &mut AzRefAnySharingInfo)>(b"az_ref_any_sharing_info_increase_ref").map_err(|_| "az_ref_any_sharing_info_increase_ref")? };
        let az_ref_any_sharing_info_decrease_ref = unsafe { lib.get::<extern fn(_:  &mut AzRefAnySharingInfo)>(b"az_ref_any_sharing_info_decrease_ref").map_err(|_| "az_ref_any_sharing_info_decrease_ref")? };
        let az_ref_any_sharing_info_increase_refmut = unsafe { lib.get::<extern fn(_:  &mut AzRefAnySharingInfo)>(b"az_ref_any_sharing_info_increase_refmut").map_err(|_| "az_ref_any_sharing_info_increase_refmut")? };
        let az_ref_any_sharing_info_decrease_refmut = unsafe { lib.get::<extern fn(_:  &mut AzRefAnySharingInfo)>(b"az_ref_any_sharing_info_decrease_refmut").map_err(|_| "az_ref_any_sharing_info_decrease_refmut")? };
        let az_ref_any_sharing_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzRefAnySharingInfo)>(b"az_ref_any_sharing_info_delete").map_err(|_| "az_ref_any_sharing_info_delete")? };
        let az_ref_any_new_c = unsafe { lib.get::<extern fn(_:  *const c_void, _:  usize, _:  u64, _:  AzString, _:  AzRefAnyDestructorType) -> AzRefAny>(b"az_ref_any_new_c").map_err(|_| "az_ref_any_new_c")? };
        let az_ref_any_is_type = unsafe { lib.get::<extern fn(_:  &AzRefAny, _:  u64) -> bool>(b"az_ref_any_is_type").map_err(|_| "az_ref_any_is_type")? };
        let az_ref_any_get_type_name = unsafe { lib.get::<extern fn(_:  &AzRefAny) -> AzString>(b"az_ref_any_get_type_name").map_err(|_| "az_ref_any_get_type_name")? };
        let az_ref_any_can_be_shared = unsafe { lib.get::<extern fn(_:  &AzRefAny) -> bool>(b"az_ref_any_can_be_shared").map_err(|_| "az_ref_any_can_be_shared")? };
        let az_ref_any_can_be_shared_mut = unsafe { lib.get::<extern fn(_:  &AzRefAny) -> bool>(b"az_ref_any_can_be_shared_mut").map_err(|_| "az_ref_any_can_be_shared_mut")? };
        let az_ref_any_increase_ref = unsafe { lib.get::<extern fn(_:  &AzRefAny)>(b"az_ref_any_increase_ref").map_err(|_| "az_ref_any_increase_ref")? };
        let az_ref_any_decrease_ref = unsafe { lib.get::<extern fn(_:  &AzRefAny)>(b"az_ref_any_decrease_ref").map_err(|_| "az_ref_any_decrease_ref")? };
        let az_ref_any_increase_refmut = unsafe { lib.get::<extern fn(_:  &AzRefAny)>(b"az_ref_any_increase_refmut").map_err(|_| "az_ref_any_increase_refmut")? };
        let az_ref_any_decrease_refmut = unsafe { lib.get::<extern fn(_:  &AzRefAny)>(b"az_ref_any_decrease_refmut").map_err(|_| "az_ref_any_decrease_refmut")? };
        let az_ref_any_delete = unsafe { lib.get::<extern fn(_:  &mut AzRefAny)>(b"az_ref_any_delete").map_err(|_| "az_ref_any_delete")? };
        let az_ref_any_deep_copy = unsafe { lib.get::<extern fn(_:  &AzRefAny) -> AzRefAny>(b"az_ref_any_deep_copy").map_err(|_| "az_ref_any_deep_copy")? };
        let az_layout_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutInfoPtr)>(b"az_layout_info_delete").map_err(|_| "az_layout_info_delete")? };
        let az_css_rule_block_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssRuleBlock)>(b"az_css_rule_block_delete").map_err(|_| "az_css_rule_block_delete")? };
        let az_css_rule_block_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssRuleBlock) -> AzCssRuleBlock>(b"az_css_rule_block_deep_copy").map_err(|_| "az_css_rule_block_deep_copy")? };
        let az_css_declaration_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssDeclaration)>(b"az_css_declaration_delete").map_err(|_| "az_css_declaration_delete")? };
        let az_css_declaration_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssDeclaration) -> AzCssDeclaration>(b"az_css_declaration_deep_copy").map_err(|_| "az_css_declaration_deep_copy")? };
        let az_dynamic_css_property_delete = unsafe { lib.get::<extern fn(_:  &mut AzDynamicCssProperty)>(b"az_dynamic_css_property_delete").map_err(|_| "az_dynamic_css_property_delete")? };
        let az_dynamic_css_property_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDynamicCssProperty) -> AzDynamicCssProperty>(b"az_dynamic_css_property_deep_copy").map_err(|_| "az_dynamic_css_property_deep_copy")? };
        let az_css_path_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssPath)>(b"az_css_path_delete").map_err(|_| "az_css_path_delete")? };
        let az_css_path_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssPath) -> AzCssPath>(b"az_css_path_deep_copy").map_err(|_| "az_css_path_deep_copy")? };
        let az_css_path_selector_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssPathSelector)>(b"az_css_path_selector_delete").map_err(|_| "az_css_path_selector_delete")? };
        let az_css_path_selector_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssPathSelector) -> AzCssPathSelector>(b"az_css_path_selector_deep_copy").map_err(|_| "az_css_path_selector_deep_copy")? };
        let az_node_type_path_delete = unsafe { lib.get::<extern fn(_:  &mut AzNodeTypePath)>(b"az_node_type_path_delete").map_err(|_| "az_node_type_path_delete")? };
        let az_node_type_path_deep_copy = unsafe { lib.get::<extern fn(_:  &AzNodeTypePath) -> AzNodeTypePath>(b"az_node_type_path_deep_copy").map_err(|_| "az_node_type_path_deep_copy")? };
        let az_css_path_pseudo_selector_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssPathPseudoSelector)>(b"az_css_path_pseudo_selector_delete").map_err(|_| "az_css_path_pseudo_selector_delete")? };
        let az_css_path_pseudo_selector_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssPathPseudoSelector) -> AzCssPathPseudoSelector>(b"az_css_path_pseudo_selector_deep_copy").map_err(|_| "az_css_path_pseudo_selector_deep_copy")? };
        let az_css_nth_child_selector_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssNthChildSelector)>(b"az_css_nth_child_selector_delete").map_err(|_| "az_css_nth_child_selector_delete")? };
        let az_css_nth_child_selector_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssNthChildSelector) -> AzCssNthChildSelector>(b"az_css_nth_child_selector_deep_copy").map_err(|_| "az_css_nth_child_selector_deep_copy")? };
        let az_css_nth_child_pattern_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssNthChildPattern)>(b"az_css_nth_child_pattern_delete").map_err(|_| "az_css_nth_child_pattern_delete")? };
        let az_css_nth_child_pattern_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssNthChildPattern) -> AzCssNthChildPattern>(b"az_css_nth_child_pattern_deep_copy").map_err(|_| "az_css_nth_child_pattern_deep_copy")? };
        let az_stylesheet_delete = unsafe { lib.get::<extern fn(_:  &mut AzStylesheet)>(b"az_stylesheet_delete").map_err(|_| "az_stylesheet_delete")? };
        let az_stylesheet_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStylesheet) -> AzStylesheet>(b"az_stylesheet_deep_copy").map_err(|_| "az_stylesheet_deep_copy")? };
        let az_css_native = unsafe { lib.get::<extern fn() -> AzCss>(b"az_css_native").map_err(|_| "az_css_native")? };
        let az_css_empty = unsafe { lib.get::<extern fn() -> AzCss>(b"az_css_empty").map_err(|_| "az_css_empty")? };
        let az_css_from_string = unsafe { lib.get::<extern fn(_:  AzString) -> AzCss>(b"az_css_from_string").map_err(|_| "az_css_from_string")? };
        let az_css_override_native = unsafe { lib.get::<extern fn(_:  AzString) -> AzCss>(b"az_css_override_native").map_err(|_| "az_css_override_native")? };
        let az_css_delete = unsafe { lib.get::<extern fn(_:  &mut AzCss)>(b"az_css_delete").map_err(|_| "az_css_delete")? };
        let az_css_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCss) -> AzCss>(b"az_css_deep_copy").map_err(|_| "az_css_deep_copy")? };
        let az_color_u_delete = unsafe { lib.get::<extern fn(_:  &mut AzColorU)>(b"az_color_u_delete").map_err(|_| "az_color_u_delete")? };
        let az_color_u_deep_copy = unsafe { lib.get::<extern fn(_:  &AzColorU) -> AzColorU>(b"az_color_u_deep_copy").map_err(|_| "az_color_u_deep_copy")? };
        let az_size_metric_delete = unsafe { lib.get::<extern fn(_:  &mut AzSizeMetric)>(b"az_size_metric_delete").map_err(|_| "az_size_metric_delete")? };
        let az_size_metric_deep_copy = unsafe { lib.get::<extern fn(_:  &AzSizeMetric) -> AzSizeMetric>(b"az_size_metric_deep_copy").map_err(|_| "az_size_metric_deep_copy")? };
        let az_float_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzFloatValue)>(b"az_float_value_delete").map_err(|_| "az_float_value_delete")? };
        let az_float_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzFloatValue) -> AzFloatValue>(b"az_float_value_deep_copy").map_err(|_| "az_float_value_deep_copy")? };
        let az_pixel_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzPixelValue)>(b"az_pixel_value_delete").map_err(|_| "az_pixel_value_delete")? };
        let az_pixel_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzPixelValue) -> AzPixelValue>(b"az_pixel_value_deep_copy").map_err(|_| "az_pixel_value_deep_copy")? };
        let az_pixel_value_no_percent_delete = unsafe { lib.get::<extern fn(_:  &mut AzPixelValueNoPercent)>(b"az_pixel_value_no_percent_delete").map_err(|_| "az_pixel_value_no_percent_delete")? };
        let az_pixel_value_no_percent_deep_copy = unsafe { lib.get::<extern fn(_:  &AzPixelValueNoPercent) -> AzPixelValueNoPercent>(b"az_pixel_value_no_percent_deep_copy").map_err(|_| "az_pixel_value_no_percent_deep_copy")? };
        let az_box_shadow_clip_mode_delete = unsafe { lib.get::<extern fn(_:  &mut AzBoxShadowClipMode)>(b"az_box_shadow_clip_mode_delete").map_err(|_| "az_box_shadow_clip_mode_delete")? };
        let az_box_shadow_clip_mode_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBoxShadowClipMode) -> AzBoxShadowClipMode>(b"az_box_shadow_clip_mode_deep_copy").map_err(|_| "az_box_shadow_clip_mode_deep_copy")? };
        let az_box_shadow_pre_display_item_delete = unsafe { lib.get::<extern fn(_:  &mut AzBoxShadowPreDisplayItem)>(b"az_box_shadow_pre_display_item_delete").map_err(|_| "az_box_shadow_pre_display_item_delete")? };
        let az_box_shadow_pre_display_item_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBoxShadowPreDisplayItem) -> AzBoxShadowPreDisplayItem>(b"az_box_shadow_pre_display_item_deep_copy").map_err(|_| "az_box_shadow_pre_display_item_deep_copy")? };
        let az_layout_align_content_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutAlignContent)>(b"az_layout_align_content_delete").map_err(|_| "az_layout_align_content_delete")? };
        let az_layout_align_content_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutAlignContent) -> AzLayoutAlignContent>(b"az_layout_align_content_deep_copy").map_err(|_| "az_layout_align_content_deep_copy")? };
        let az_layout_align_items_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutAlignItems)>(b"az_layout_align_items_delete").map_err(|_| "az_layout_align_items_delete")? };
        let az_layout_align_items_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutAlignItems) -> AzLayoutAlignItems>(b"az_layout_align_items_deep_copy").map_err(|_| "az_layout_align_items_deep_copy")? };
        let az_layout_bottom_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutBottom)>(b"az_layout_bottom_delete").map_err(|_| "az_layout_bottom_delete")? };
        let az_layout_bottom_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutBottom) -> AzLayoutBottom>(b"az_layout_bottom_deep_copy").map_err(|_| "az_layout_bottom_deep_copy")? };
        let az_layout_box_sizing_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutBoxSizing)>(b"az_layout_box_sizing_delete").map_err(|_| "az_layout_box_sizing_delete")? };
        let az_layout_box_sizing_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutBoxSizing) -> AzLayoutBoxSizing>(b"az_layout_box_sizing_deep_copy").map_err(|_| "az_layout_box_sizing_deep_copy")? };
        let az_layout_direction_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutDirection)>(b"az_layout_direction_delete").map_err(|_| "az_layout_direction_delete")? };
        let az_layout_direction_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutDirection) -> AzLayoutDirection>(b"az_layout_direction_deep_copy").map_err(|_| "az_layout_direction_deep_copy")? };
        let az_layout_display_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutDisplay)>(b"az_layout_display_delete").map_err(|_| "az_layout_display_delete")? };
        let az_layout_display_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutDisplay) -> AzLayoutDisplay>(b"az_layout_display_deep_copy").map_err(|_| "az_layout_display_deep_copy")? };
        let az_layout_flex_grow_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutFlexGrow)>(b"az_layout_flex_grow_delete").map_err(|_| "az_layout_flex_grow_delete")? };
        let az_layout_flex_grow_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutFlexGrow) -> AzLayoutFlexGrow>(b"az_layout_flex_grow_deep_copy").map_err(|_| "az_layout_flex_grow_deep_copy")? };
        let az_layout_flex_shrink_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutFlexShrink)>(b"az_layout_flex_shrink_delete").map_err(|_| "az_layout_flex_shrink_delete")? };
        let az_layout_flex_shrink_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutFlexShrink) -> AzLayoutFlexShrink>(b"az_layout_flex_shrink_deep_copy").map_err(|_| "az_layout_flex_shrink_deep_copy")? };
        let az_layout_float_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutFloat)>(b"az_layout_float_delete").map_err(|_| "az_layout_float_delete")? };
        let az_layout_float_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutFloat) -> AzLayoutFloat>(b"az_layout_float_deep_copy").map_err(|_| "az_layout_float_deep_copy")? };
        let az_layout_height_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutHeight)>(b"az_layout_height_delete").map_err(|_| "az_layout_height_delete")? };
        let az_layout_height_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutHeight) -> AzLayoutHeight>(b"az_layout_height_deep_copy").map_err(|_| "az_layout_height_deep_copy")? };
        let az_layout_justify_content_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutJustifyContent)>(b"az_layout_justify_content_delete").map_err(|_| "az_layout_justify_content_delete")? };
        let az_layout_justify_content_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutJustifyContent) -> AzLayoutJustifyContent>(b"az_layout_justify_content_deep_copy").map_err(|_| "az_layout_justify_content_deep_copy")? };
        let az_layout_left_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutLeft)>(b"az_layout_left_delete").map_err(|_| "az_layout_left_delete")? };
        let az_layout_left_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutLeft) -> AzLayoutLeft>(b"az_layout_left_deep_copy").map_err(|_| "az_layout_left_deep_copy")? };
        let az_layout_margin_bottom_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginBottom)>(b"az_layout_margin_bottom_delete").map_err(|_| "az_layout_margin_bottom_delete")? };
        let az_layout_margin_bottom_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginBottom) -> AzLayoutMarginBottom>(b"az_layout_margin_bottom_deep_copy").map_err(|_| "az_layout_margin_bottom_deep_copy")? };
        let az_layout_margin_left_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginLeft)>(b"az_layout_margin_left_delete").map_err(|_| "az_layout_margin_left_delete")? };
        let az_layout_margin_left_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginLeft) -> AzLayoutMarginLeft>(b"az_layout_margin_left_deep_copy").map_err(|_| "az_layout_margin_left_deep_copy")? };
        let az_layout_margin_right_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginRight)>(b"az_layout_margin_right_delete").map_err(|_| "az_layout_margin_right_delete")? };
        let az_layout_margin_right_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginRight) -> AzLayoutMarginRight>(b"az_layout_margin_right_deep_copy").map_err(|_| "az_layout_margin_right_deep_copy")? };
        let az_layout_margin_top_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginTop)>(b"az_layout_margin_top_delete").map_err(|_| "az_layout_margin_top_delete")? };
        let az_layout_margin_top_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginTop) -> AzLayoutMarginTop>(b"az_layout_margin_top_deep_copy").map_err(|_| "az_layout_margin_top_deep_copy")? };
        let az_layout_max_height_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMaxHeight)>(b"az_layout_max_height_delete").map_err(|_| "az_layout_max_height_delete")? };
        let az_layout_max_height_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMaxHeight) -> AzLayoutMaxHeight>(b"az_layout_max_height_deep_copy").map_err(|_| "az_layout_max_height_deep_copy")? };
        let az_layout_max_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMaxWidth)>(b"az_layout_max_width_delete").map_err(|_| "az_layout_max_width_delete")? };
        let az_layout_max_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMaxWidth) -> AzLayoutMaxWidth>(b"az_layout_max_width_deep_copy").map_err(|_| "az_layout_max_width_deep_copy")? };
        let az_layout_min_height_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMinHeight)>(b"az_layout_min_height_delete").map_err(|_| "az_layout_min_height_delete")? };
        let az_layout_min_height_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMinHeight) -> AzLayoutMinHeight>(b"az_layout_min_height_deep_copy").map_err(|_| "az_layout_min_height_deep_copy")? };
        let az_layout_min_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMinWidth)>(b"az_layout_min_width_delete").map_err(|_| "az_layout_min_width_delete")? };
        let az_layout_min_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMinWidth) -> AzLayoutMinWidth>(b"az_layout_min_width_deep_copy").map_err(|_| "az_layout_min_width_deep_copy")? };
        let az_layout_padding_bottom_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingBottom)>(b"az_layout_padding_bottom_delete").map_err(|_| "az_layout_padding_bottom_delete")? };
        let az_layout_padding_bottom_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingBottom) -> AzLayoutPaddingBottom>(b"az_layout_padding_bottom_deep_copy").map_err(|_| "az_layout_padding_bottom_deep_copy")? };
        let az_layout_padding_left_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingLeft)>(b"az_layout_padding_left_delete").map_err(|_| "az_layout_padding_left_delete")? };
        let az_layout_padding_left_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingLeft) -> AzLayoutPaddingLeft>(b"az_layout_padding_left_deep_copy").map_err(|_| "az_layout_padding_left_deep_copy")? };
        let az_layout_padding_right_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingRight)>(b"az_layout_padding_right_delete").map_err(|_| "az_layout_padding_right_delete")? };
        let az_layout_padding_right_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingRight) -> AzLayoutPaddingRight>(b"az_layout_padding_right_deep_copy").map_err(|_| "az_layout_padding_right_deep_copy")? };
        let az_layout_padding_top_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingTop)>(b"az_layout_padding_top_delete").map_err(|_| "az_layout_padding_top_delete")? };
        let az_layout_padding_top_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingTop) -> AzLayoutPaddingTop>(b"az_layout_padding_top_deep_copy").map_err(|_| "az_layout_padding_top_deep_copy")? };
        let az_layout_position_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPosition)>(b"az_layout_position_delete").map_err(|_| "az_layout_position_delete")? };
        let az_layout_position_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPosition) -> AzLayoutPosition>(b"az_layout_position_deep_copy").map_err(|_| "az_layout_position_deep_copy")? };
        let az_layout_right_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutRight)>(b"az_layout_right_delete").map_err(|_| "az_layout_right_delete")? };
        let az_layout_right_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutRight) -> AzLayoutRight>(b"az_layout_right_deep_copy").map_err(|_| "az_layout_right_deep_copy")? };
        let az_layout_top_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutTop)>(b"az_layout_top_delete").map_err(|_| "az_layout_top_delete")? };
        let az_layout_top_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutTop) -> AzLayoutTop>(b"az_layout_top_deep_copy").map_err(|_| "az_layout_top_deep_copy")? };
        let az_layout_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutWidth)>(b"az_layout_width_delete").map_err(|_| "az_layout_width_delete")? };
        let az_layout_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutWidth) -> AzLayoutWidth>(b"az_layout_width_deep_copy").map_err(|_| "az_layout_width_deep_copy")? };
        let az_layout_wrap_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutWrap)>(b"az_layout_wrap_delete").map_err(|_| "az_layout_wrap_delete")? };
        let az_layout_wrap_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutWrap) -> AzLayoutWrap>(b"az_layout_wrap_deep_copy").map_err(|_| "az_layout_wrap_deep_copy")? };
        let az_overflow_delete = unsafe { lib.get::<extern fn(_:  &mut AzOverflow)>(b"az_overflow_delete").map_err(|_| "az_overflow_delete")? };
        let az_overflow_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOverflow) -> AzOverflow>(b"az_overflow_deep_copy").map_err(|_| "az_overflow_deep_copy")? };
        let az_percentage_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzPercentageValue)>(b"az_percentage_value_delete").map_err(|_| "az_percentage_value_delete")? };
        let az_percentage_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzPercentageValue) -> AzPercentageValue>(b"az_percentage_value_deep_copy").map_err(|_| "az_percentage_value_deep_copy")? };
        let az_gradient_stop_pre_delete = unsafe { lib.get::<extern fn(_:  &mut AzGradientStopPre)>(b"az_gradient_stop_pre_delete").map_err(|_| "az_gradient_stop_pre_delete")? };
        let az_gradient_stop_pre_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGradientStopPre) -> AzGradientStopPre>(b"az_gradient_stop_pre_deep_copy").map_err(|_| "az_gradient_stop_pre_deep_copy")? };
        let az_direction_corner_delete = unsafe { lib.get::<extern fn(_:  &mut AzDirectionCorner)>(b"az_direction_corner_delete").map_err(|_| "az_direction_corner_delete")? };
        let az_direction_corner_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDirectionCorner) -> AzDirectionCorner>(b"az_direction_corner_deep_copy").map_err(|_| "az_direction_corner_deep_copy")? };
        let az_direction_corners_delete = unsafe { lib.get::<extern fn(_:  &mut AzDirectionCorners)>(b"az_direction_corners_delete").map_err(|_| "az_direction_corners_delete")? };
        let az_direction_corners_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDirectionCorners) -> AzDirectionCorners>(b"az_direction_corners_deep_copy").map_err(|_| "az_direction_corners_deep_copy")? };
        let az_direction_delete = unsafe { lib.get::<extern fn(_:  &mut AzDirection)>(b"az_direction_delete").map_err(|_| "az_direction_delete")? };
        let az_direction_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDirection) -> AzDirection>(b"az_direction_deep_copy").map_err(|_| "az_direction_deep_copy")? };
        let az_extend_mode_delete = unsafe { lib.get::<extern fn(_:  &mut AzExtendMode)>(b"az_extend_mode_delete").map_err(|_| "az_extend_mode_delete")? };
        let az_extend_mode_deep_copy = unsafe { lib.get::<extern fn(_:  &AzExtendMode) -> AzExtendMode>(b"az_extend_mode_deep_copy").map_err(|_| "az_extend_mode_deep_copy")? };
        let az_linear_gradient_delete = unsafe { lib.get::<extern fn(_:  &mut AzLinearGradient)>(b"az_linear_gradient_delete").map_err(|_| "az_linear_gradient_delete")? };
        let az_linear_gradient_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLinearGradient) -> AzLinearGradient>(b"az_linear_gradient_deep_copy").map_err(|_| "az_linear_gradient_deep_copy")? };
        let az_shape_delete = unsafe { lib.get::<extern fn(_:  &mut AzShape)>(b"az_shape_delete").map_err(|_| "az_shape_delete")? };
        let az_shape_deep_copy = unsafe { lib.get::<extern fn(_:  &AzShape) -> AzShape>(b"az_shape_deep_copy").map_err(|_| "az_shape_deep_copy")? };
        let az_radial_gradient_delete = unsafe { lib.get::<extern fn(_:  &mut AzRadialGradient)>(b"az_radial_gradient_delete").map_err(|_| "az_radial_gradient_delete")? };
        let az_radial_gradient_deep_copy = unsafe { lib.get::<extern fn(_:  &AzRadialGradient) -> AzRadialGradient>(b"az_radial_gradient_deep_copy").map_err(|_| "az_radial_gradient_deep_copy")? };
        let az_css_image_id_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssImageId)>(b"az_css_image_id_delete").map_err(|_| "az_css_image_id_delete")? };
        let az_css_image_id_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssImageId) -> AzCssImageId>(b"az_css_image_id_deep_copy").map_err(|_| "az_css_image_id_deep_copy")? };
        let az_style_background_content_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundContent)>(b"az_style_background_content_delete").map_err(|_| "az_style_background_content_delete")? };
        let az_style_background_content_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundContent) -> AzStyleBackgroundContent>(b"az_style_background_content_deep_copy").map_err(|_| "az_style_background_content_deep_copy")? };
        let az_background_position_horizontal_delete = unsafe { lib.get::<extern fn(_:  &mut AzBackgroundPositionHorizontal)>(b"az_background_position_horizontal_delete").map_err(|_| "az_background_position_horizontal_delete")? };
        let az_background_position_horizontal_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBackgroundPositionHorizontal) -> AzBackgroundPositionHorizontal>(b"az_background_position_horizontal_deep_copy").map_err(|_| "az_background_position_horizontal_deep_copy")? };
        let az_background_position_vertical_delete = unsafe { lib.get::<extern fn(_:  &mut AzBackgroundPositionVertical)>(b"az_background_position_vertical_delete").map_err(|_| "az_background_position_vertical_delete")? };
        let az_background_position_vertical_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBackgroundPositionVertical) -> AzBackgroundPositionVertical>(b"az_background_position_vertical_deep_copy").map_err(|_| "az_background_position_vertical_deep_copy")? };
        let az_style_background_position_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundPosition)>(b"az_style_background_position_delete").map_err(|_| "az_style_background_position_delete")? };
        let az_style_background_position_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundPosition) -> AzStyleBackgroundPosition>(b"az_style_background_position_deep_copy").map_err(|_| "az_style_background_position_deep_copy")? };
        let az_style_background_repeat_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundRepeat)>(b"az_style_background_repeat_delete").map_err(|_| "az_style_background_repeat_delete")? };
        let az_style_background_repeat_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundRepeat) -> AzStyleBackgroundRepeat>(b"az_style_background_repeat_deep_copy").map_err(|_| "az_style_background_repeat_deep_copy")? };
        let az_style_background_size_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundSize)>(b"az_style_background_size_delete").map_err(|_| "az_style_background_size_delete")? };
        let az_style_background_size_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundSize) -> AzStyleBackgroundSize>(b"az_style_background_size_deep_copy").map_err(|_| "az_style_background_size_deep_copy")? };
        let az_style_border_bottom_color_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomColor)>(b"az_style_border_bottom_color_delete").map_err(|_| "az_style_border_bottom_color_delete")? };
        let az_style_border_bottom_color_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomColor) -> AzStyleBorderBottomColor>(b"az_style_border_bottom_color_deep_copy").map_err(|_| "az_style_border_bottom_color_deep_copy")? };
        let az_style_border_bottom_left_radius_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomLeftRadius)>(b"az_style_border_bottom_left_radius_delete").map_err(|_| "az_style_border_bottom_left_radius_delete")? };
        let az_style_border_bottom_left_radius_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadius>(b"az_style_border_bottom_left_radius_deep_copy").map_err(|_| "az_style_border_bottom_left_radius_deep_copy")? };
        let az_style_border_bottom_right_radius_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomRightRadius)>(b"az_style_border_bottom_right_radius_delete").map_err(|_| "az_style_border_bottom_right_radius_delete")? };
        let az_style_border_bottom_right_radius_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadius>(b"az_style_border_bottom_right_radius_deep_copy").map_err(|_| "az_style_border_bottom_right_radius_deep_copy")? };
        let az_border_style_delete = unsafe { lib.get::<extern fn(_:  &mut AzBorderStyle)>(b"az_border_style_delete").map_err(|_| "az_border_style_delete")? };
        let az_border_style_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBorderStyle) -> AzBorderStyle>(b"az_border_style_deep_copy").map_err(|_| "az_border_style_deep_copy")? };
        let az_style_border_bottom_style_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomStyle)>(b"az_style_border_bottom_style_delete").map_err(|_| "az_style_border_bottom_style_delete")? };
        let az_style_border_bottom_style_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomStyle) -> AzStyleBorderBottomStyle>(b"az_style_border_bottom_style_deep_copy").map_err(|_| "az_style_border_bottom_style_deep_copy")? };
        let az_style_border_bottom_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomWidth)>(b"az_style_border_bottom_width_delete").map_err(|_| "az_style_border_bottom_width_delete")? };
        let az_style_border_bottom_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomWidth) -> AzStyleBorderBottomWidth>(b"az_style_border_bottom_width_deep_copy").map_err(|_| "az_style_border_bottom_width_deep_copy")? };
        let az_style_border_left_color_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderLeftColor)>(b"az_style_border_left_color_delete").map_err(|_| "az_style_border_left_color_delete")? };
        let az_style_border_left_color_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderLeftColor) -> AzStyleBorderLeftColor>(b"az_style_border_left_color_deep_copy").map_err(|_| "az_style_border_left_color_deep_copy")? };
        let az_style_border_left_style_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderLeftStyle)>(b"az_style_border_left_style_delete").map_err(|_| "az_style_border_left_style_delete")? };
        let az_style_border_left_style_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderLeftStyle) -> AzStyleBorderLeftStyle>(b"az_style_border_left_style_deep_copy").map_err(|_| "az_style_border_left_style_deep_copy")? };
        let az_style_border_left_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderLeftWidth)>(b"az_style_border_left_width_delete").map_err(|_| "az_style_border_left_width_delete")? };
        let az_style_border_left_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderLeftWidth) -> AzStyleBorderLeftWidth>(b"az_style_border_left_width_deep_copy").map_err(|_| "az_style_border_left_width_deep_copy")? };
        let az_style_border_right_color_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderRightColor)>(b"az_style_border_right_color_delete").map_err(|_| "az_style_border_right_color_delete")? };
        let az_style_border_right_color_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderRightColor) -> AzStyleBorderRightColor>(b"az_style_border_right_color_deep_copy").map_err(|_| "az_style_border_right_color_deep_copy")? };
        let az_style_border_right_style_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderRightStyle)>(b"az_style_border_right_style_delete").map_err(|_| "az_style_border_right_style_delete")? };
        let az_style_border_right_style_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderRightStyle) -> AzStyleBorderRightStyle>(b"az_style_border_right_style_deep_copy").map_err(|_| "az_style_border_right_style_deep_copy")? };
        let az_style_border_right_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderRightWidth)>(b"az_style_border_right_width_delete").map_err(|_| "az_style_border_right_width_delete")? };
        let az_style_border_right_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderRightWidth) -> AzStyleBorderRightWidth>(b"az_style_border_right_width_deep_copy").map_err(|_| "az_style_border_right_width_deep_copy")? };
        let az_style_border_top_color_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopColor)>(b"az_style_border_top_color_delete").map_err(|_| "az_style_border_top_color_delete")? };
        let az_style_border_top_color_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopColor) -> AzStyleBorderTopColor>(b"az_style_border_top_color_deep_copy").map_err(|_| "az_style_border_top_color_deep_copy")? };
        let az_style_border_top_left_radius_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopLeftRadius)>(b"az_style_border_top_left_radius_delete").map_err(|_| "az_style_border_top_left_radius_delete")? };
        let az_style_border_top_left_radius_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopLeftRadius) -> AzStyleBorderTopLeftRadius>(b"az_style_border_top_left_radius_deep_copy").map_err(|_| "az_style_border_top_left_radius_deep_copy")? };
        let az_style_border_top_right_radius_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopRightRadius)>(b"az_style_border_top_right_radius_delete").map_err(|_| "az_style_border_top_right_radius_delete")? };
        let az_style_border_top_right_radius_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopRightRadius) -> AzStyleBorderTopRightRadius>(b"az_style_border_top_right_radius_deep_copy").map_err(|_| "az_style_border_top_right_radius_deep_copy")? };
        let az_style_border_top_style_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopStyle)>(b"az_style_border_top_style_delete").map_err(|_| "az_style_border_top_style_delete")? };
        let az_style_border_top_style_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopStyle) -> AzStyleBorderTopStyle>(b"az_style_border_top_style_deep_copy").map_err(|_| "az_style_border_top_style_deep_copy")? };
        let az_style_border_top_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopWidth)>(b"az_style_border_top_width_delete").map_err(|_| "az_style_border_top_width_delete")? };
        let az_style_border_top_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopWidth) -> AzStyleBorderTopWidth>(b"az_style_border_top_width_deep_copy").map_err(|_| "az_style_border_top_width_deep_copy")? };
        let az_style_cursor_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleCursor)>(b"az_style_cursor_delete").map_err(|_| "az_style_cursor_delete")? };
        let az_style_cursor_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleCursor) -> AzStyleCursor>(b"az_style_cursor_deep_copy").map_err(|_| "az_style_cursor_deep_copy")? };
        let az_style_font_family_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleFontFamily)>(b"az_style_font_family_delete").map_err(|_| "az_style_font_family_delete")? };
        let az_style_font_family_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleFontFamily) -> AzStyleFontFamily>(b"az_style_font_family_deep_copy").map_err(|_| "az_style_font_family_deep_copy")? };
        let az_style_font_size_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleFontSize)>(b"az_style_font_size_delete").map_err(|_| "az_style_font_size_delete")? };
        let az_style_font_size_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleFontSize) -> AzStyleFontSize>(b"az_style_font_size_deep_copy").map_err(|_| "az_style_font_size_deep_copy")? };
        let az_style_letter_spacing_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleLetterSpacing)>(b"az_style_letter_spacing_delete").map_err(|_| "az_style_letter_spacing_delete")? };
        let az_style_letter_spacing_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleLetterSpacing) -> AzStyleLetterSpacing>(b"az_style_letter_spacing_deep_copy").map_err(|_| "az_style_letter_spacing_deep_copy")? };
        let az_style_line_height_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleLineHeight)>(b"az_style_line_height_delete").map_err(|_| "az_style_line_height_delete")? };
        let az_style_line_height_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleLineHeight) -> AzStyleLineHeight>(b"az_style_line_height_deep_copy").map_err(|_| "az_style_line_height_deep_copy")? };
        let az_style_tab_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleTabWidth)>(b"az_style_tab_width_delete").map_err(|_| "az_style_tab_width_delete")? };
        let az_style_tab_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleTabWidth) -> AzStyleTabWidth>(b"az_style_tab_width_deep_copy").map_err(|_| "az_style_tab_width_deep_copy")? };
        let az_style_text_alignment_horz_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleTextAlignmentHorz)>(b"az_style_text_alignment_horz_delete").map_err(|_| "az_style_text_alignment_horz_delete")? };
        let az_style_text_alignment_horz_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleTextAlignmentHorz) -> AzStyleTextAlignmentHorz>(b"az_style_text_alignment_horz_deep_copy").map_err(|_| "az_style_text_alignment_horz_deep_copy")? };
        let az_style_text_color_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleTextColor)>(b"az_style_text_color_delete").map_err(|_| "az_style_text_color_delete")? };
        let az_style_text_color_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleTextColor) -> AzStyleTextColor>(b"az_style_text_color_deep_copy").map_err(|_| "az_style_text_color_deep_copy")? };
        let az_style_word_spacing_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleWordSpacing)>(b"az_style_word_spacing_delete").map_err(|_| "az_style_word_spacing_delete")? };
        let az_style_word_spacing_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleWordSpacing) -> AzStyleWordSpacing>(b"az_style_word_spacing_deep_copy").map_err(|_| "az_style_word_spacing_deep_copy")? };
        let az_box_shadow_pre_display_item_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzBoxShadowPreDisplayItemValue)>(b"az_box_shadow_pre_display_item_value_delete").map_err(|_| "az_box_shadow_pre_display_item_value_delete")? };
        let az_box_shadow_pre_display_item_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBoxShadowPreDisplayItemValue) -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_deep_copy").map_err(|_| "az_box_shadow_pre_display_item_value_deep_copy")? };
        let az_layout_align_content_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutAlignContentValue)>(b"az_layout_align_content_value_delete").map_err(|_| "az_layout_align_content_value_delete")? };
        let az_layout_align_content_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutAlignContentValue) -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_deep_copy").map_err(|_| "az_layout_align_content_value_deep_copy")? };
        let az_layout_align_items_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutAlignItemsValue)>(b"az_layout_align_items_value_delete").map_err(|_| "az_layout_align_items_value_delete")? };
        let az_layout_align_items_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutAlignItemsValue) -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_deep_copy").map_err(|_| "az_layout_align_items_value_deep_copy")? };
        let az_layout_bottom_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutBottomValue)>(b"az_layout_bottom_value_delete").map_err(|_| "az_layout_bottom_value_delete")? };
        let az_layout_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutBottomValue) -> AzLayoutBottomValue>(b"az_layout_bottom_value_deep_copy").map_err(|_| "az_layout_bottom_value_deep_copy")? };
        let az_layout_box_sizing_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutBoxSizingValue)>(b"az_layout_box_sizing_value_delete").map_err(|_| "az_layout_box_sizing_value_delete")? };
        let az_layout_box_sizing_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutBoxSizingValue) -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_deep_copy").map_err(|_| "az_layout_box_sizing_value_deep_copy")? };
        let az_layout_direction_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutDirectionValue)>(b"az_layout_direction_value_delete").map_err(|_| "az_layout_direction_value_delete")? };
        let az_layout_direction_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutDirectionValue) -> AzLayoutDirectionValue>(b"az_layout_direction_value_deep_copy").map_err(|_| "az_layout_direction_value_deep_copy")? };
        let az_layout_display_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutDisplayValue)>(b"az_layout_display_value_delete").map_err(|_| "az_layout_display_value_delete")? };
        let az_layout_display_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutDisplayValue) -> AzLayoutDisplayValue>(b"az_layout_display_value_deep_copy").map_err(|_| "az_layout_display_value_deep_copy")? };
        let az_layout_flex_grow_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutFlexGrowValue)>(b"az_layout_flex_grow_value_delete").map_err(|_| "az_layout_flex_grow_value_delete")? };
        let az_layout_flex_grow_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutFlexGrowValue) -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_deep_copy").map_err(|_| "az_layout_flex_grow_value_deep_copy")? };
        let az_layout_flex_shrink_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutFlexShrinkValue)>(b"az_layout_flex_shrink_value_delete").map_err(|_| "az_layout_flex_shrink_value_delete")? };
        let az_layout_flex_shrink_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutFlexShrinkValue) -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_deep_copy").map_err(|_| "az_layout_flex_shrink_value_deep_copy")? };
        let az_layout_float_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutFloatValue)>(b"az_layout_float_value_delete").map_err(|_| "az_layout_float_value_delete")? };
        let az_layout_float_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutFloatValue) -> AzLayoutFloatValue>(b"az_layout_float_value_deep_copy").map_err(|_| "az_layout_float_value_deep_copy")? };
        let az_layout_height_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutHeightValue)>(b"az_layout_height_value_delete").map_err(|_| "az_layout_height_value_delete")? };
        let az_layout_height_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutHeightValue) -> AzLayoutHeightValue>(b"az_layout_height_value_deep_copy").map_err(|_| "az_layout_height_value_deep_copy")? };
        let az_layout_justify_content_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutJustifyContentValue)>(b"az_layout_justify_content_value_delete").map_err(|_| "az_layout_justify_content_value_delete")? };
        let az_layout_justify_content_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutJustifyContentValue) -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_deep_copy").map_err(|_| "az_layout_justify_content_value_deep_copy")? };
        let az_layout_left_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutLeftValue)>(b"az_layout_left_value_delete").map_err(|_| "az_layout_left_value_delete")? };
        let az_layout_left_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutLeftValue) -> AzLayoutLeftValue>(b"az_layout_left_value_deep_copy").map_err(|_| "az_layout_left_value_deep_copy")? };
        let az_layout_margin_bottom_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginBottomValue)>(b"az_layout_margin_bottom_value_delete").map_err(|_| "az_layout_margin_bottom_value_delete")? };
        let az_layout_margin_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginBottomValue) -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_deep_copy").map_err(|_| "az_layout_margin_bottom_value_deep_copy")? };
        let az_layout_margin_left_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginLeftValue)>(b"az_layout_margin_left_value_delete").map_err(|_| "az_layout_margin_left_value_delete")? };
        let az_layout_margin_left_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginLeftValue) -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_deep_copy").map_err(|_| "az_layout_margin_left_value_deep_copy")? };
        let az_layout_margin_right_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginRightValue)>(b"az_layout_margin_right_value_delete").map_err(|_| "az_layout_margin_right_value_delete")? };
        let az_layout_margin_right_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginRightValue) -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_deep_copy").map_err(|_| "az_layout_margin_right_value_deep_copy")? };
        let az_layout_margin_top_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginTopValue)>(b"az_layout_margin_top_value_delete").map_err(|_| "az_layout_margin_top_value_delete")? };
        let az_layout_margin_top_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginTopValue) -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_deep_copy").map_err(|_| "az_layout_margin_top_value_deep_copy")? };
        let az_layout_max_height_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMaxHeightValue)>(b"az_layout_max_height_value_delete").map_err(|_| "az_layout_max_height_value_delete")? };
        let az_layout_max_height_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMaxHeightValue) -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_deep_copy").map_err(|_| "az_layout_max_height_value_deep_copy")? };
        let az_layout_max_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMaxWidthValue)>(b"az_layout_max_width_value_delete").map_err(|_| "az_layout_max_width_value_delete")? };
        let az_layout_max_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMaxWidthValue) -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_deep_copy").map_err(|_| "az_layout_max_width_value_deep_copy")? };
        let az_layout_min_height_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMinHeightValue)>(b"az_layout_min_height_value_delete").map_err(|_| "az_layout_min_height_value_delete")? };
        let az_layout_min_height_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMinHeightValue) -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_deep_copy").map_err(|_| "az_layout_min_height_value_deep_copy")? };
        let az_layout_min_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMinWidthValue)>(b"az_layout_min_width_value_delete").map_err(|_| "az_layout_min_width_value_delete")? };
        let az_layout_min_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMinWidthValue) -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_deep_copy").map_err(|_| "az_layout_min_width_value_deep_copy")? };
        let az_layout_padding_bottom_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingBottomValue)>(b"az_layout_padding_bottom_value_delete").map_err(|_| "az_layout_padding_bottom_value_delete")? };
        let az_layout_padding_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingBottomValue) -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_deep_copy").map_err(|_| "az_layout_padding_bottom_value_deep_copy")? };
        let az_layout_padding_left_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingLeftValue)>(b"az_layout_padding_left_value_delete").map_err(|_| "az_layout_padding_left_value_delete")? };
        let az_layout_padding_left_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingLeftValue) -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_deep_copy").map_err(|_| "az_layout_padding_left_value_deep_copy")? };
        let az_layout_padding_right_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingRightValue)>(b"az_layout_padding_right_value_delete").map_err(|_| "az_layout_padding_right_value_delete")? };
        let az_layout_padding_right_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingRightValue) -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_deep_copy").map_err(|_| "az_layout_padding_right_value_deep_copy")? };
        let az_layout_padding_top_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingTopValue)>(b"az_layout_padding_top_value_delete").map_err(|_| "az_layout_padding_top_value_delete")? };
        let az_layout_padding_top_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingTopValue) -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_deep_copy").map_err(|_| "az_layout_padding_top_value_deep_copy")? };
        let az_layout_position_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPositionValue)>(b"az_layout_position_value_delete").map_err(|_| "az_layout_position_value_delete")? };
        let az_layout_position_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPositionValue) -> AzLayoutPositionValue>(b"az_layout_position_value_deep_copy").map_err(|_| "az_layout_position_value_deep_copy")? };
        let az_layout_right_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutRightValue)>(b"az_layout_right_value_delete").map_err(|_| "az_layout_right_value_delete")? };
        let az_layout_right_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutRightValue) -> AzLayoutRightValue>(b"az_layout_right_value_deep_copy").map_err(|_| "az_layout_right_value_deep_copy")? };
        let az_layout_top_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutTopValue)>(b"az_layout_top_value_delete").map_err(|_| "az_layout_top_value_delete")? };
        let az_layout_top_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutTopValue) -> AzLayoutTopValue>(b"az_layout_top_value_deep_copy").map_err(|_| "az_layout_top_value_deep_copy")? };
        let az_layout_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutWidthValue)>(b"az_layout_width_value_delete").map_err(|_| "az_layout_width_value_delete")? };
        let az_layout_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutWidthValue) -> AzLayoutWidthValue>(b"az_layout_width_value_deep_copy").map_err(|_| "az_layout_width_value_deep_copy")? };
        let az_layout_wrap_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutWrapValue)>(b"az_layout_wrap_value_delete").map_err(|_| "az_layout_wrap_value_delete")? };
        let az_layout_wrap_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutWrapValue) -> AzLayoutWrapValue>(b"az_layout_wrap_value_deep_copy").map_err(|_| "az_layout_wrap_value_deep_copy")? };
        let az_overflow_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzOverflowValue)>(b"az_overflow_value_delete").map_err(|_| "az_overflow_value_delete")? };
        let az_overflow_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOverflowValue) -> AzOverflowValue>(b"az_overflow_value_deep_copy").map_err(|_| "az_overflow_value_deep_copy")? };
        let az_style_background_content_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundContentValue)>(b"az_style_background_content_value_delete").map_err(|_| "az_style_background_content_value_delete")? };
        let az_style_background_content_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_deep_copy").map_err(|_| "az_style_background_content_value_deep_copy")? };
        let az_style_background_position_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundPositionValue)>(b"az_style_background_position_value_delete").map_err(|_| "az_style_background_position_value_delete")? };
        let az_style_background_position_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundPositionValue) -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_deep_copy").map_err(|_| "az_style_background_position_value_deep_copy")? };
        let az_style_background_repeat_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundRepeatValue)>(b"az_style_background_repeat_value_delete").map_err(|_| "az_style_background_repeat_value_delete")? };
        let az_style_background_repeat_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundRepeatValue) -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_deep_copy").map_err(|_| "az_style_background_repeat_value_deep_copy")? };
        let az_style_background_size_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundSizeValue)>(b"az_style_background_size_value_delete").map_err(|_| "az_style_background_size_value_delete")? };
        let az_style_background_size_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundSizeValue) -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_deep_copy").map_err(|_| "az_style_background_size_value_deep_copy")? };
        let az_style_border_bottom_color_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomColorValue)>(b"az_style_border_bottom_color_value_delete").map_err(|_| "az_style_border_bottom_color_value_delete")? };
        let az_style_border_bottom_color_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomColorValue) -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_deep_copy").map_err(|_| "az_style_border_bottom_color_value_deep_copy")? };
        let az_style_border_bottom_left_radius_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomLeftRadiusValue)>(b"az_style_border_bottom_left_radius_value_delete").map_err(|_| "az_style_border_bottom_left_radius_value_delete")? };
        let az_style_border_bottom_left_radius_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomLeftRadiusValue) -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_deep_copy").map_err(|_| "az_style_border_bottom_left_radius_value_deep_copy")? };
        let az_style_border_bottom_right_radius_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomRightRadiusValue)>(b"az_style_border_bottom_right_radius_value_delete").map_err(|_| "az_style_border_bottom_right_radius_value_delete")? };
        let az_style_border_bottom_right_radius_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomRightRadiusValue) -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_deep_copy").map_err(|_| "az_style_border_bottom_right_radius_value_deep_copy")? };
        let az_style_border_bottom_style_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomStyleValue)>(b"az_style_border_bottom_style_value_delete").map_err(|_| "az_style_border_bottom_style_value_delete")? };
        let az_style_border_bottom_style_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomStyleValue) -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_deep_copy").map_err(|_| "az_style_border_bottom_style_value_deep_copy")? };
        let az_style_border_bottom_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomWidthValue)>(b"az_style_border_bottom_width_value_delete").map_err(|_| "az_style_border_bottom_width_value_delete")? };
        let az_style_border_bottom_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomWidthValue) -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_deep_copy").map_err(|_| "az_style_border_bottom_width_value_deep_copy")? };
        let az_style_border_left_color_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderLeftColorValue)>(b"az_style_border_left_color_value_delete").map_err(|_| "az_style_border_left_color_value_delete")? };
        let az_style_border_left_color_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderLeftColorValue) -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_deep_copy").map_err(|_| "az_style_border_left_color_value_deep_copy")? };
        let az_style_border_left_style_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderLeftStyleValue)>(b"az_style_border_left_style_value_delete").map_err(|_| "az_style_border_left_style_value_delete")? };
        let az_style_border_left_style_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderLeftStyleValue) -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_deep_copy").map_err(|_| "az_style_border_left_style_value_deep_copy")? };
        let az_style_border_left_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderLeftWidthValue)>(b"az_style_border_left_width_value_delete").map_err(|_| "az_style_border_left_width_value_delete")? };
        let az_style_border_left_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderLeftWidthValue) -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_deep_copy").map_err(|_| "az_style_border_left_width_value_deep_copy")? };
        let az_style_border_right_color_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderRightColorValue)>(b"az_style_border_right_color_value_delete").map_err(|_| "az_style_border_right_color_value_delete")? };
        let az_style_border_right_color_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderRightColorValue) -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_deep_copy").map_err(|_| "az_style_border_right_color_value_deep_copy")? };
        let az_style_border_right_style_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderRightStyleValue)>(b"az_style_border_right_style_value_delete").map_err(|_| "az_style_border_right_style_value_delete")? };
        let az_style_border_right_style_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderRightStyleValue) -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_deep_copy").map_err(|_| "az_style_border_right_style_value_deep_copy")? };
        let az_style_border_right_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderRightWidthValue)>(b"az_style_border_right_width_value_delete").map_err(|_| "az_style_border_right_width_value_delete")? };
        let az_style_border_right_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderRightWidthValue) -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_deep_copy").map_err(|_| "az_style_border_right_width_value_deep_copy")? };
        let az_style_border_top_color_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopColorValue)>(b"az_style_border_top_color_value_delete").map_err(|_| "az_style_border_top_color_value_delete")? };
        let az_style_border_top_color_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopColorValue) -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_deep_copy").map_err(|_| "az_style_border_top_color_value_deep_copy")? };
        let az_style_border_top_left_radius_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopLeftRadiusValue)>(b"az_style_border_top_left_radius_value_delete").map_err(|_| "az_style_border_top_left_radius_value_delete")? };
        let az_style_border_top_left_radius_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopLeftRadiusValue) -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_deep_copy").map_err(|_| "az_style_border_top_left_radius_value_deep_copy")? };
        let az_style_border_top_right_radius_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopRightRadiusValue)>(b"az_style_border_top_right_radius_value_delete").map_err(|_| "az_style_border_top_right_radius_value_delete")? };
        let az_style_border_top_right_radius_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopRightRadiusValue) -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_deep_copy").map_err(|_| "az_style_border_top_right_radius_value_deep_copy")? };
        let az_style_border_top_style_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopStyleValue)>(b"az_style_border_top_style_value_delete").map_err(|_| "az_style_border_top_style_value_delete")? };
        let az_style_border_top_style_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopStyleValue) -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_deep_copy").map_err(|_| "az_style_border_top_style_value_deep_copy")? };
        let az_style_border_top_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopWidthValue)>(b"az_style_border_top_width_value_delete").map_err(|_| "az_style_border_top_width_value_delete")? };
        let az_style_border_top_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopWidthValue) -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_deep_copy").map_err(|_| "az_style_border_top_width_value_deep_copy")? };
        let az_style_cursor_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleCursorValue)>(b"az_style_cursor_value_delete").map_err(|_| "az_style_cursor_value_delete")? };
        let az_style_cursor_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleCursorValue) -> AzStyleCursorValue>(b"az_style_cursor_value_deep_copy").map_err(|_| "az_style_cursor_value_deep_copy")? };
        let az_style_font_family_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleFontFamilyValue)>(b"az_style_font_family_value_delete").map_err(|_| "az_style_font_family_value_delete")? };
        let az_style_font_family_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue>(b"az_style_font_family_value_deep_copy").map_err(|_| "az_style_font_family_value_deep_copy")? };
        let az_style_font_size_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleFontSizeValue)>(b"az_style_font_size_value_delete").map_err(|_| "az_style_font_size_value_delete")? };
        let az_style_font_size_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleFontSizeValue) -> AzStyleFontSizeValue>(b"az_style_font_size_value_deep_copy").map_err(|_| "az_style_font_size_value_deep_copy")? };
        let az_style_letter_spacing_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleLetterSpacingValue)>(b"az_style_letter_spacing_value_delete").map_err(|_| "az_style_letter_spacing_value_delete")? };
        let az_style_letter_spacing_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleLetterSpacingValue) -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_deep_copy").map_err(|_| "az_style_letter_spacing_value_deep_copy")? };
        let az_style_line_height_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleLineHeightValue)>(b"az_style_line_height_value_delete").map_err(|_| "az_style_line_height_value_delete")? };
        let az_style_line_height_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleLineHeightValue) -> AzStyleLineHeightValue>(b"az_style_line_height_value_deep_copy").map_err(|_| "az_style_line_height_value_deep_copy")? };
        let az_style_tab_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleTabWidthValue)>(b"az_style_tab_width_value_delete").map_err(|_| "az_style_tab_width_value_delete")? };
        let az_style_tab_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleTabWidthValue) -> AzStyleTabWidthValue>(b"az_style_tab_width_value_deep_copy").map_err(|_| "az_style_tab_width_value_deep_copy")? };
        let az_style_text_alignment_horz_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleTextAlignmentHorzValue)>(b"az_style_text_alignment_horz_value_delete").map_err(|_| "az_style_text_alignment_horz_value_delete")? };
        let az_style_text_alignment_horz_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleTextAlignmentHorzValue) -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_deep_copy").map_err(|_| "az_style_text_alignment_horz_value_deep_copy")? };
        let az_style_text_color_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleTextColorValue)>(b"az_style_text_color_value_delete").map_err(|_| "az_style_text_color_value_delete")? };
        let az_style_text_color_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleTextColorValue) -> AzStyleTextColorValue>(b"az_style_text_color_value_deep_copy").map_err(|_| "az_style_text_color_value_deep_copy")? };
        let az_style_word_spacing_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleWordSpacingValue)>(b"az_style_word_spacing_value_delete").map_err(|_| "az_style_word_spacing_value_delete")? };
        let az_style_word_spacing_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleWordSpacingValue) -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_deep_copy").map_err(|_| "az_style_word_spacing_value_deep_copy")? };
        let az_css_property_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssProperty)>(b"az_css_property_delete").map_err(|_| "az_css_property_delete")? };
        let az_css_property_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssProperty) -> AzCssProperty>(b"az_css_property_deep_copy").map_err(|_| "az_css_property_deep_copy")? };
        let az_dom_div = unsafe { lib.get::<extern fn() -> AzDom>(b"az_dom_div").map_err(|_| "az_dom_div")? };
        let az_dom_body = unsafe { lib.get::<extern fn() -> AzDom>(b"az_dom_body").map_err(|_| "az_dom_body")? };
        let az_dom_label = unsafe { lib.get::<extern fn(_:  AzString) -> AzDom>(b"az_dom_label").map_err(|_| "az_dom_label")? };
        let az_dom_text = unsafe { lib.get::<extern fn(_:  AzTextId) -> AzDom>(b"az_dom_text").map_err(|_| "az_dom_text")? };
        let az_dom_image = unsafe { lib.get::<extern fn(_:  AzImageId) -> AzDom>(b"az_dom_image").map_err(|_| "az_dom_image")? };
        let az_dom_gl_texture = unsafe { lib.get::<extern fn(_:  AzRefAny, _:  AzGlCallbackType) -> AzDom>(b"az_dom_gl_texture").map_err(|_| "az_dom_gl_texture")? };
        let az_dom_iframe = unsafe { lib.get::<extern fn(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzDom>(b"az_dom_iframe").map_err(|_| "az_dom_iframe")? };
        let az_dom_add_id = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzString)>(b"az_dom_add_id").map_err(|_| "az_dom_add_id")? };
        let az_dom_with_id = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzString) -> AzDom>(b"az_dom_with_id").map_err(|_| "az_dom_with_id")? };
        let az_dom_set_ids = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzStringVec)>(b"az_dom_set_ids").map_err(|_| "az_dom_set_ids")? };
        let az_dom_with_ids = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzStringVec) -> AzDom>(b"az_dom_with_ids").map_err(|_| "az_dom_with_ids")? };
        let az_dom_add_class = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzString)>(b"az_dom_add_class").map_err(|_| "az_dom_add_class")? };
        let az_dom_with_class = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzString) -> AzDom>(b"az_dom_with_class").map_err(|_| "az_dom_with_class")? };
        let az_dom_set_classes = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzStringVec)>(b"az_dom_set_classes").map_err(|_| "az_dom_set_classes")? };
        let az_dom_with_classes = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzStringVec) -> AzDom>(b"az_dom_with_classes").map_err(|_| "az_dom_with_classes")? };
        let az_dom_add_callback = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType)>(b"az_dom_add_callback").map_err(|_| "az_dom_add_callback")? };
        let az_dom_with_callback = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzDom>(b"az_dom_with_callback").map_err(|_| "az_dom_with_callback")? };
        let az_dom_add_css_override = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzString, _:  AzCssProperty)>(b"az_dom_add_css_override").map_err(|_| "az_dom_add_css_override")? };
        let az_dom_with_css_override = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzString, _:  AzCssProperty) -> AzDom>(b"az_dom_with_css_override").map_err(|_| "az_dom_with_css_override")? };
        let az_dom_set_is_draggable = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  bool)>(b"az_dom_set_is_draggable").map_err(|_| "az_dom_set_is_draggable")? };
        let az_dom_is_draggable = unsafe { lib.get::<extern fn(_:  AzDom, _:  bool) -> AzDom>(b"az_dom_is_draggable").map_err(|_| "az_dom_is_draggable")? };
        let az_dom_set_tab_index = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzTabIndex)>(b"az_dom_set_tab_index").map_err(|_| "az_dom_set_tab_index")? };
        let az_dom_with_tab_index = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzTabIndex) -> AzDom>(b"az_dom_with_tab_index").map_err(|_| "az_dom_with_tab_index")? };
        let az_dom_add_child = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzDom)>(b"az_dom_add_child").map_err(|_| "az_dom_add_child")? };
        let az_dom_with_child = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzDom) -> AzDom>(b"az_dom_with_child").map_err(|_| "az_dom_with_child")? };
        let az_dom_has_id = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzString) -> bool>(b"az_dom_has_id").map_err(|_| "az_dom_has_id")? };
        let az_dom_has_class = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzString) -> bool>(b"az_dom_has_class").map_err(|_| "az_dom_has_class")? };
        let az_dom_get_html_string = unsafe { lib.get::<extern fn(_:  &AzDom) -> AzString>(b"az_dom_get_html_string").map_err(|_| "az_dom_get_html_string")? };
        let az_dom_delete = unsafe { lib.get::<extern fn(_:  &mut AzDom)>(b"az_dom_delete").map_err(|_| "az_dom_delete")? };
        let az_dom_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDom) -> AzDom>(b"az_dom_deep_copy").map_err(|_| "az_dom_deep_copy")? };
        let az_gl_texture_node_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlTextureNode)>(b"az_gl_texture_node_delete").map_err(|_| "az_gl_texture_node_delete")? };
        let az_gl_texture_node_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGlTextureNode) -> AzGlTextureNode>(b"az_gl_texture_node_deep_copy").map_err(|_| "az_gl_texture_node_deep_copy")? };
        let az_i_frame_node_delete = unsafe { lib.get::<extern fn(_:  &mut AzIFrameNode)>(b"az_i_frame_node_delete").map_err(|_| "az_i_frame_node_delete")? };
        let az_i_frame_node_deep_copy = unsafe { lib.get::<extern fn(_:  &AzIFrameNode) -> AzIFrameNode>(b"az_i_frame_node_deep_copy").map_err(|_| "az_i_frame_node_deep_copy")? };
        let az_callback_data_delete = unsafe { lib.get::<extern fn(_:  &mut AzCallbackData)>(b"az_callback_data_delete").map_err(|_| "az_callback_data_delete")? };
        let az_callback_data_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCallbackData) -> AzCallbackData>(b"az_callback_data_deep_copy").map_err(|_| "az_callback_data_deep_copy")? };
        let az_override_property_delete = unsafe { lib.get::<extern fn(_:  &mut AzOverrideProperty)>(b"az_override_property_delete").map_err(|_| "az_override_property_delete")? };
        let az_override_property_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOverrideProperty) -> AzOverrideProperty>(b"az_override_property_deep_copy").map_err(|_| "az_override_property_deep_copy")? };
        let az_node_data_new = unsafe { lib.get::<extern fn(_:  AzNodeType) -> AzNodeData>(b"az_node_data_new").map_err(|_| "az_node_data_new")? };
        let az_node_data_default = unsafe { lib.get::<extern fn() -> AzNodeData>(b"az_node_data_default").map_err(|_| "az_node_data_default")? };
        let az_node_data_delete = unsafe { lib.get::<extern fn(_:  &mut AzNodeData)>(b"az_node_data_delete").map_err(|_| "az_node_data_delete")? };
        let az_node_data_deep_copy = unsafe { lib.get::<extern fn(_:  &AzNodeData) -> AzNodeData>(b"az_node_data_deep_copy").map_err(|_| "az_node_data_deep_copy")? };
        let az_node_type_delete = unsafe { lib.get::<extern fn(_:  &mut AzNodeType)>(b"az_node_type_delete").map_err(|_| "az_node_type_delete")? };
        let az_node_type_deep_copy = unsafe { lib.get::<extern fn(_:  &AzNodeType) -> AzNodeType>(b"az_node_type_deep_copy").map_err(|_| "az_node_type_deep_copy")? };
        let az_on_into_event_filter = unsafe { lib.get::<extern fn(_:  AzOn) -> AzEventFilter>(b"az_on_into_event_filter").map_err(|_| "az_on_into_event_filter")? };
        let az_on_delete = unsafe { lib.get::<extern fn(_:  &mut AzOn)>(b"az_on_delete").map_err(|_| "az_on_delete")? };
        let az_on_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOn) -> AzOn>(b"az_on_deep_copy").map_err(|_| "az_on_deep_copy")? };
        let az_event_filter_delete = unsafe { lib.get::<extern fn(_:  &mut AzEventFilter)>(b"az_event_filter_delete").map_err(|_| "az_event_filter_delete")? };
        let az_event_filter_deep_copy = unsafe { lib.get::<extern fn(_:  &AzEventFilter) -> AzEventFilter>(b"az_event_filter_deep_copy").map_err(|_| "az_event_filter_deep_copy")? };
        let az_hover_event_filter_delete = unsafe { lib.get::<extern fn(_:  &mut AzHoverEventFilter)>(b"az_hover_event_filter_delete").map_err(|_| "az_hover_event_filter_delete")? };
        let az_hover_event_filter_deep_copy = unsafe { lib.get::<extern fn(_:  &AzHoverEventFilter) -> AzHoverEventFilter>(b"az_hover_event_filter_deep_copy").map_err(|_| "az_hover_event_filter_deep_copy")? };
        let az_focus_event_filter_delete = unsafe { lib.get::<extern fn(_:  &mut AzFocusEventFilter)>(b"az_focus_event_filter_delete").map_err(|_| "az_focus_event_filter_delete")? };
        let az_focus_event_filter_deep_copy = unsafe { lib.get::<extern fn(_:  &AzFocusEventFilter) -> AzFocusEventFilter>(b"az_focus_event_filter_deep_copy").map_err(|_| "az_focus_event_filter_deep_copy")? };
        let az_not_event_filter_delete = unsafe { lib.get::<extern fn(_:  &mut AzNotEventFilter)>(b"az_not_event_filter_delete").map_err(|_| "az_not_event_filter_delete")? };
        let az_not_event_filter_deep_copy = unsafe { lib.get::<extern fn(_:  &AzNotEventFilter) -> AzNotEventFilter>(b"az_not_event_filter_deep_copy").map_err(|_| "az_not_event_filter_deep_copy")? };
        let az_window_event_filter_delete = unsafe { lib.get::<extern fn(_:  &mut AzWindowEventFilter)>(b"az_window_event_filter_delete").map_err(|_| "az_window_event_filter_delete")? };
        let az_window_event_filter_deep_copy = unsafe { lib.get::<extern fn(_:  &AzWindowEventFilter) -> AzWindowEventFilter>(b"az_window_event_filter_deep_copy").map_err(|_| "az_window_event_filter_deep_copy")? };
        let az_tab_index_delete = unsafe { lib.get::<extern fn(_:  &mut AzTabIndex)>(b"az_tab_index_delete").map_err(|_| "az_tab_index_delete")? };
        let az_tab_index_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTabIndex) -> AzTabIndex>(b"az_tab_index_deep_copy").map_err(|_| "az_tab_index_deep_copy")? };
        let az_gl_type_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlType)>(b"az_gl_type_delete").map_err(|_| "az_gl_type_delete")? };
        let az_gl_type_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGlType) -> AzGlType>(b"az_gl_type_deep_copy").map_err(|_| "az_gl_type_deep_copy")? };
        let az_debug_message_delete = unsafe { lib.get::<extern fn(_:  &mut AzDebugMessage)>(b"az_debug_message_delete").map_err(|_| "az_debug_message_delete")? };
        let az_debug_message_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDebugMessage) -> AzDebugMessage>(b"az_debug_message_deep_copy").map_err(|_| "az_debug_message_deep_copy")? };
        let az_u8_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzU8VecRef)>(b"az_u8_vec_ref_delete").map_err(|_| "az_u8_vec_ref_delete")? };
        let az_u8_vec_ref_mut_delete = unsafe { lib.get::<extern fn(_:  &mut AzU8VecRefMut)>(b"az_u8_vec_ref_mut_delete").map_err(|_| "az_u8_vec_ref_mut_delete")? };
        let az_f32_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzF32VecRef)>(b"az_f32_vec_ref_delete").map_err(|_| "az_f32_vec_ref_delete")? };
        let az_i32_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzI32VecRef)>(b"az_i32_vec_ref_delete").map_err(|_| "az_i32_vec_ref_delete")? };
        let az_g_luint_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLuintVecRef)>(b"az_g_luint_vec_ref_delete").map_err(|_| "az_g_luint_vec_ref_delete")? };
        let az_g_lenum_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLenumVecRef)>(b"az_g_lenum_vec_ref_delete").map_err(|_| "az_g_lenum_vec_ref_delete")? };
        let az_g_lint_vec_ref_mut_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLintVecRefMut)>(b"az_g_lint_vec_ref_mut_delete").map_err(|_| "az_g_lint_vec_ref_mut_delete")? };
        let az_g_lint64_vec_ref_mut_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLint64VecRefMut)>(b"az_g_lint64_vec_ref_mut_delete").map_err(|_| "az_g_lint64_vec_ref_mut_delete")? };
        let az_g_lboolean_vec_ref_mut_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLbooleanVecRefMut)>(b"az_g_lboolean_vec_ref_mut_delete").map_err(|_| "az_g_lboolean_vec_ref_mut_delete")? };
        let az_g_lfloat_vec_ref_mut_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLfloatVecRefMut)>(b"az_g_lfloat_vec_ref_mut_delete").map_err(|_| "az_g_lfloat_vec_ref_mut_delete")? };
        let az_refstr_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzRefstrVecRef)>(b"az_refstr_vec_ref_delete").map_err(|_| "az_refstr_vec_ref_delete")? };
        let az_refstr_delete = unsafe { lib.get::<extern fn(_:  &mut AzRefstr)>(b"az_refstr_delete").map_err(|_| "az_refstr_delete")? };
        let az_get_program_binary_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzGetProgramBinaryReturn)>(b"az_get_program_binary_return_delete").map_err(|_| "az_get_program_binary_return_delete")? };
        let az_get_program_binary_return_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGetProgramBinaryReturn) -> AzGetProgramBinaryReturn>(b"az_get_program_binary_return_deep_copy").map_err(|_| "az_get_program_binary_return_deep_copy")? };
        let az_get_active_attrib_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzGetActiveAttribReturn)>(b"az_get_active_attrib_return_delete").map_err(|_| "az_get_active_attrib_return_delete")? };
        let az_get_active_attrib_return_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGetActiveAttribReturn) -> AzGetActiveAttribReturn>(b"az_get_active_attrib_return_deep_copy").map_err(|_| "az_get_active_attrib_return_deep_copy")? };
        let az_g_lsync_ptr_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLsyncPtr)>(b"az_g_lsync_ptr_delete").map_err(|_| "az_g_lsync_ptr_delete")? };
        let az_get_active_uniform_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzGetActiveUniformReturn)>(b"az_get_active_uniform_return_delete").map_err(|_| "az_get_active_uniform_return_delete")? };
        let az_get_active_uniform_return_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGetActiveUniformReturn) -> AzGetActiveUniformReturn>(b"az_get_active_uniform_return_deep_copy").map_err(|_| "az_get_active_uniform_return_deep_copy")? };
        let az_gl_context_ptr_get_type = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr) -> AzGlType>(b"az_gl_context_ptr_get_type").map_err(|_| "az_gl_context_ptr_get_type")? };
        let az_gl_context_ptr_buffer_data_untyped = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  *const c_void, _:  u32)>(b"az_gl_context_ptr_buffer_data_untyped").map_err(|_| "az_gl_context_ptr_buffer_data_untyped")? };
        let az_gl_context_ptr_buffer_sub_data_untyped = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  *const c_void)>(b"az_gl_context_ptr_buffer_sub_data_untyped").map_err(|_| "az_gl_context_ptr_buffer_sub_data_untyped")? };
        let az_gl_context_ptr_map_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> *mut c_void>(b"az_gl_context_ptr_map_buffer").map_err(|_| "az_gl_context_ptr_map_buffer")? };
        let az_gl_context_ptr_map_buffer_range = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  u32) -> *mut c_void>(b"az_gl_context_ptr_map_buffer_range").map_err(|_| "az_gl_context_ptr_map_buffer_range")? };
        let az_gl_context_ptr_unmap_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>(b"az_gl_context_ptr_unmap_buffer").map_err(|_| "az_gl_context_ptr_unmap_buffer")? };
        let az_gl_context_ptr_tex_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_tex_buffer").map_err(|_| "az_gl_context_ptr_tex_buffer")? };
        let az_gl_context_ptr_shader_source = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzStringVec)>(b"az_gl_context_ptr_shader_source").map_err(|_| "az_gl_context_ptr_shader_source")? };
        let az_gl_context_ptr_read_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_read_buffer").map_err(|_| "az_gl_context_ptr_read_buffer")? };
        let az_gl_context_ptr_read_pixels_into_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut)>(b"az_gl_context_ptr_read_pixels_into_buffer").map_err(|_| "az_gl_context_ptr_read_pixels_into_buffer")? };
        let az_gl_context_ptr_read_pixels = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32) -> AzU8Vec>(b"az_gl_context_ptr_read_pixels").map_err(|_| "az_gl_context_ptr_read_pixels")? };
        let az_gl_context_ptr_read_pixels_into_pbo = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32)>(b"az_gl_context_ptr_read_pixels_into_pbo").map_err(|_| "az_gl_context_ptr_read_pixels_into_pbo")? };
        let az_gl_context_ptr_sample_coverage = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f32, _:  bool)>(b"az_gl_context_ptr_sample_coverage").map_err(|_| "az_gl_context_ptr_sample_coverage")? };
        let az_gl_context_ptr_polygon_offset = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f32, _:  f32)>(b"az_gl_context_ptr_polygon_offset").map_err(|_| "az_gl_context_ptr_polygon_offset")? };
        let az_gl_context_ptr_pixel_store_i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32)>(b"az_gl_context_ptr_pixel_store_i").map_err(|_| "az_gl_context_ptr_pixel_store_i")? };
        let az_gl_context_ptr_gen_buffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_buffers").map_err(|_| "az_gl_context_ptr_gen_buffers")? };
        let az_gl_context_ptr_gen_renderbuffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_renderbuffers").map_err(|_| "az_gl_context_ptr_gen_renderbuffers")? };
        let az_gl_context_ptr_gen_framebuffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_framebuffers").map_err(|_| "az_gl_context_ptr_gen_framebuffers")? };
        let az_gl_context_ptr_gen_textures = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_textures").map_err(|_| "az_gl_context_ptr_gen_textures")? };
        let az_gl_context_ptr_gen_vertex_arrays = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_vertex_arrays").map_err(|_| "az_gl_context_ptr_gen_vertex_arrays")? };
        let az_gl_context_ptr_gen_queries = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_queries").map_err(|_| "az_gl_context_ptr_gen_queries")? };
        let az_gl_context_ptr_begin_query = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_begin_query").map_err(|_| "az_gl_context_ptr_begin_query")? };
        let az_gl_context_ptr_end_query = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_end_query").map_err(|_| "az_gl_context_ptr_end_query")? };
        let az_gl_context_ptr_query_counter = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_query_counter").map_err(|_| "az_gl_context_ptr_query_counter")? };
        let az_gl_context_ptr_get_query_object_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32>(b"az_gl_context_ptr_get_query_object_iv").map_err(|_| "az_gl_context_ptr_get_query_object_iv")? };
        let az_gl_context_ptr_get_query_object_uiv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u32>(b"az_gl_context_ptr_get_query_object_uiv").map_err(|_| "az_gl_context_ptr_get_query_object_uiv")? };
        let az_gl_context_ptr_get_query_object_i64v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i64>(b"az_gl_context_ptr_get_query_object_i64v").map_err(|_| "az_gl_context_ptr_get_query_object_i64v")? };
        let az_gl_context_ptr_get_query_object_ui64v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u64>(b"az_gl_context_ptr_get_query_object_ui64v").map_err(|_| "az_gl_context_ptr_get_query_object_ui64v")? };
        let az_gl_context_ptr_delete_queries = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_queries").map_err(|_| "az_gl_context_ptr_delete_queries")? };
        let az_gl_context_ptr_delete_vertex_arrays = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_vertex_arrays").map_err(|_| "az_gl_context_ptr_delete_vertex_arrays")? };
        let az_gl_context_ptr_delete_buffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_buffers").map_err(|_| "az_gl_context_ptr_delete_buffers")? };
        let az_gl_context_ptr_delete_renderbuffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_renderbuffers").map_err(|_| "az_gl_context_ptr_delete_renderbuffers")? };
        let az_gl_context_ptr_delete_framebuffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_framebuffers").map_err(|_| "az_gl_context_ptr_delete_framebuffers")? };
        let az_gl_context_ptr_delete_textures = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_textures").map_err(|_| "az_gl_context_ptr_delete_textures")? };
        let az_gl_context_ptr_framebuffer_renderbuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_framebuffer_renderbuffer").map_err(|_| "az_gl_context_ptr_framebuffer_renderbuffer")? };
        let az_gl_context_ptr_renderbuffer_storage = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32)>(b"az_gl_context_ptr_renderbuffer_storage").map_err(|_| "az_gl_context_ptr_renderbuffer_storage")? };
        let az_gl_context_ptr_depth_func = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_depth_func").map_err(|_| "az_gl_context_ptr_depth_func")? };
        let az_gl_context_ptr_active_texture = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_active_texture").map_err(|_| "az_gl_context_ptr_active_texture")? };
        let az_gl_context_ptr_attach_shader = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_attach_shader").map_err(|_| "az_gl_context_ptr_attach_shader")? };
        let az_gl_context_ptr_bind_attrib_location = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr)>(b"az_gl_context_ptr_bind_attrib_location").map_err(|_| "az_gl_context_ptr_bind_attrib_location")? };
        let az_gl_context_ptr_get_uniform_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLintVecRefMut)>(b"az_gl_context_ptr_get_uniform_iv").map_err(|_| "az_gl_context_ptr_get_uniform_iv")? };
        let az_gl_context_ptr_get_uniform_fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLfloatVecRefMut)>(b"az_gl_context_ptr_get_uniform_fv").map_err(|_| "az_gl_context_ptr_get_uniform_fv")? };
        let az_gl_context_ptr_get_uniform_block_index = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> u32>(b"az_gl_context_ptr_get_uniform_block_index").map_err(|_| "az_gl_context_ptr_get_uniform_block_index")? };
        let az_gl_context_ptr_get_uniform_indices = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstrVecRef) -> AzGLuintVec>(b"az_gl_context_ptr_get_uniform_indices").map_err(|_| "az_gl_context_ptr_get_uniform_indices")? };
        let az_gl_context_ptr_bind_buffer_base = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_bind_buffer_base").map_err(|_| "az_gl_context_ptr_bind_buffer_base")? };
        let az_gl_context_ptr_bind_buffer_range = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  isize, _:  isize)>(b"az_gl_context_ptr_bind_buffer_range").map_err(|_| "az_gl_context_ptr_bind_buffer_range")? };
        let az_gl_context_ptr_uniform_block_binding = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_uniform_block_binding").map_err(|_| "az_gl_context_ptr_uniform_block_binding")? };
        let az_gl_context_ptr_bind_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_bind_buffer").map_err(|_| "az_gl_context_ptr_bind_buffer")? };
        let az_gl_context_ptr_bind_vertex_array = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_bind_vertex_array").map_err(|_| "az_gl_context_ptr_bind_vertex_array")? };
        let az_gl_context_ptr_bind_renderbuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_bind_renderbuffer").map_err(|_| "az_gl_context_ptr_bind_renderbuffer")? };
        let az_gl_context_ptr_bind_framebuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_bind_framebuffer").map_err(|_| "az_gl_context_ptr_bind_framebuffer")? };
        let az_gl_context_ptr_bind_texture = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_bind_texture").map_err(|_| "az_gl_context_ptr_bind_texture")? };
        let az_gl_context_ptr_draw_buffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLenumVecRef)>(b"az_gl_context_ptr_draw_buffers").map_err(|_| "az_gl_context_ptr_draw_buffers")? };
        let az_gl_context_ptr_tex_image_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef)>(b"az_gl_context_ptr_tex_image_2d").map_err(|_| "az_gl_context_ptr_tex_image_2d")? };
        let az_gl_context_ptr_compressed_tex_image_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  AzU8VecRef)>(b"az_gl_context_ptr_compressed_tex_image_2d").map_err(|_| "az_gl_context_ptr_compressed_tex_image_2d")? };
        let az_gl_context_ptr_compressed_tex_sub_image_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  AzU8VecRef)>(b"az_gl_context_ptr_compressed_tex_sub_image_2d").map_err(|_| "az_gl_context_ptr_compressed_tex_sub_image_2d")? };
        let az_gl_context_ptr_tex_image_3d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef)>(b"az_gl_context_ptr_tex_image_3d").map_err(|_| "az_gl_context_ptr_tex_image_3d")? };
        let az_gl_context_ptr_copy_tex_image_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_copy_tex_image_2d").map_err(|_| "az_gl_context_ptr_copy_tex_image_2d")? };
        let az_gl_context_ptr_copy_tex_sub_image_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_copy_tex_sub_image_2d").map_err(|_| "az_gl_context_ptr_copy_tex_sub_image_2d")? };
        let az_gl_context_ptr_copy_tex_sub_image_3d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_copy_tex_sub_image_3d").map_err(|_| "az_gl_context_ptr_copy_tex_sub_image_3d")? };
        let az_gl_context_ptr_tex_sub_image_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef)>(b"az_gl_context_ptr_tex_sub_image_2d").map_err(|_| "az_gl_context_ptr_tex_sub_image_2d")? };
        let az_gl_context_ptr_tex_sub_image_2d_pbo = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize)>(b"az_gl_context_ptr_tex_sub_image_2d_pbo").map_err(|_| "az_gl_context_ptr_tex_sub_image_2d_pbo")? };
        let az_gl_context_ptr_tex_sub_image_3d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef)>(b"az_gl_context_ptr_tex_sub_image_3d").map_err(|_| "az_gl_context_ptr_tex_sub_image_3d")? };
        let az_gl_context_ptr_tex_sub_image_3d_pbo = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize)>(b"az_gl_context_ptr_tex_sub_image_3d_pbo").map_err(|_| "az_gl_context_ptr_tex_sub_image_3d_pbo")? };
        let az_gl_context_ptr_tex_storage_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32)>(b"az_gl_context_ptr_tex_storage_2d").map_err(|_| "az_gl_context_ptr_tex_storage_2d")? };
        let az_gl_context_ptr_tex_storage_3d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_tex_storage_3d").map_err(|_| "az_gl_context_ptr_tex_storage_3d")? };
        let az_gl_context_ptr_get_tex_image_into_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut)>(b"az_gl_context_ptr_get_tex_image_into_buffer").map_err(|_| "az_gl_context_ptr_get_tex_image_into_buffer")? };
        let az_gl_context_ptr_copy_image_sub_data = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_copy_image_sub_data").map_err(|_| "az_gl_context_ptr_copy_image_sub_data")? };
        let az_gl_context_ptr_invalidate_framebuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef)>(b"az_gl_context_ptr_invalidate_framebuffer").map_err(|_| "az_gl_context_ptr_invalidate_framebuffer")? };
        let az_gl_context_ptr_invalidate_sub_framebuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_invalidate_sub_framebuffer").map_err(|_| "az_gl_context_ptr_invalidate_sub_framebuffer")? };
        let az_gl_context_ptr_get_integer_v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLintVecRefMut)>(b"az_gl_context_ptr_get_integer_v").map_err(|_| "az_gl_context_ptr_get_integer_v")? };
        let az_gl_context_ptr_get_integer_64v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLint64VecRefMut)>(b"az_gl_context_ptr_get_integer_64v").map_err(|_| "az_gl_context_ptr_get_integer_64v")? };
        let az_gl_context_ptr_get_integer_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut)>(b"az_gl_context_ptr_get_integer_iv").map_err(|_| "az_gl_context_ptr_get_integer_iv")? };
        let az_gl_context_ptr_get_integer_64iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLint64VecRefMut)>(b"az_gl_context_ptr_get_integer_64iv").map_err(|_| "az_gl_context_ptr_get_integer_64iv")? };
        let az_gl_context_ptr_get_boolean_v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLbooleanVecRefMut)>(b"az_gl_context_ptr_get_boolean_v").map_err(|_| "az_gl_context_ptr_get_boolean_v")? };
        let az_gl_context_ptr_get_float_v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLfloatVecRefMut)>(b"az_gl_context_ptr_get_float_v").map_err(|_| "az_gl_context_ptr_get_float_v")? };
        let az_gl_context_ptr_get_framebuffer_attachment_parameter_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32>(b"az_gl_context_ptr_get_framebuffer_attachment_parameter_iv").map_err(|_| "az_gl_context_ptr_get_framebuffer_attachment_parameter_iv")? };
        let az_gl_context_ptr_get_renderbuffer_parameter_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32>(b"az_gl_context_ptr_get_renderbuffer_parameter_iv").map_err(|_| "az_gl_context_ptr_get_renderbuffer_parameter_iv")? };
        let az_gl_context_ptr_get_tex_parameter_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32>(b"az_gl_context_ptr_get_tex_parameter_iv").map_err(|_| "az_gl_context_ptr_get_tex_parameter_iv")? };
        let az_gl_context_ptr_get_tex_parameter_fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> f32>(b"az_gl_context_ptr_get_tex_parameter_fv").map_err(|_| "az_gl_context_ptr_get_tex_parameter_fv")? };
        let az_gl_context_ptr_tex_parameter_i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32)>(b"az_gl_context_ptr_tex_parameter_i").map_err(|_| "az_gl_context_ptr_tex_parameter_i")? };
        let az_gl_context_ptr_tex_parameter_f = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  f32)>(b"az_gl_context_ptr_tex_parameter_f").map_err(|_| "az_gl_context_ptr_tex_parameter_f")? };
        let az_gl_context_ptr_framebuffer_texture_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  i32)>(b"az_gl_context_ptr_framebuffer_texture_2d").map_err(|_| "az_gl_context_ptr_framebuffer_texture_2d")? };
        let az_gl_context_ptr_framebuffer_texture_layer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  i32, _:  i32)>(b"az_gl_context_ptr_framebuffer_texture_layer").map_err(|_| "az_gl_context_ptr_framebuffer_texture_layer")? };
        let az_gl_context_ptr_blit_framebuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32)>(b"az_gl_context_ptr_blit_framebuffer").map_err(|_| "az_gl_context_ptr_blit_framebuffer")? };
        let az_gl_context_ptr_vertex_attrib_4f = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  f32, _:  f32, _:  f32, _:  f32)>(b"az_gl_context_ptr_vertex_attrib_4f").map_err(|_| "az_gl_context_ptr_vertex_attrib_4f")? };
        let az_gl_context_ptr_vertex_attrib_pointer_f32 = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  bool, _:  i32, _:  u32)>(b"az_gl_context_ptr_vertex_attrib_pointer_f32").map_err(|_| "az_gl_context_ptr_vertex_attrib_pointer_f32")? };
        let az_gl_context_ptr_vertex_attrib_pointer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  bool, _:  i32, _:  u32)>(b"az_gl_context_ptr_vertex_attrib_pointer").map_err(|_| "az_gl_context_ptr_vertex_attrib_pointer")? };
        let az_gl_context_ptr_vertex_attrib_i_pointer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  u32)>(b"az_gl_context_ptr_vertex_attrib_i_pointer").map_err(|_| "az_gl_context_ptr_vertex_attrib_i_pointer")? };
        let az_gl_context_ptr_vertex_attrib_divisor = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_vertex_attrib_divisor").map_err(|_| "az_gl_context_ptr_vertex_attrib_divisor")? };
        let az_gl_context_ptr_viewport = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_viewport").map_err(|_| "az_gl_context_ptr_viewport")? };
        let az_gl_context_ptr_scissor = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_scissor").map_err(|_| "az_gl_context_ptr_scissor")? };
        let az_gl_context_ptr_line_width = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f32)>(b"az_gl_context_ptr_line_width").map_err(|_| "az_gl_context_ptr_line_width")? };
        let az_gl_context_ptr_use_program = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_use_program").map_err(|_| "az_gl_context_ptr_use_program")? };
        let az_gl_context_ptr_validate_program = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_validate_program").map_err(|_| "az_gl_context_ptr_validate_program")? };
        let az_gl_context_ptr_draw_arrays = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32)>(b"az_gl_context_ptr_draw_arrays").map_err(|_| "az_gl_context_ptr_draw_arrays")? };
        let az_gl_context_ptr_draw_arrays_instanced = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_draw_arrays_instanced").map_err(|_| "az_gl_context_ptr_draw_arrays_instanced")? };
        let az_gl_context_ptr_draw_elements = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32)>(b"az_gl_context_ptr_draw_elements").map_err(|_| "az_gl_context_ptr_draw_elements")? };
        let az_gl_context_ptr_draw_elements_instanced = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32)>(b"az_gl_context_ptr_draw_elements_instanced").map_err(|_| "az_gl_context_ptr_draw_elements_instanced")? };
        let az_gl_context_ptr_blend_color = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32)>(b"az_gl_context_ptr_blend_color").map_err(|_| "az_gl_context_ptr_blend_color")? };
        let az_gl_context_ptr_blend_func = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_blend_func").map_err(|_| "az_gl_context_ptr_blend_func")? };
        let az_gl_context_ptr_blend_func_separate = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_blend_func_separate").map_err(|_| "az_gl_context_ptr_blend_func_separate")? };
        let az_gl_context_ptr_blend_equation = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_blend_equation").map_err(|_| "az_gl_context_ptr_blend_equation")? };
        let az_gl_context_ptr_blend_equation_separate = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_blend_equation_separate").map_err(|_| "az_gl_context_ptr_blend_equation_separate")? };
        let az_gl_context_ptr_color_mask = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  bool, _:  bool, _:  bool, _:  bool)>(b"az_gl_context_ptr_color_mask").map_err(|_| "az_gl_context_ptr_color_mask")? };
        let az_gl_context_ptr_cull_face = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_cull_face").map_err(|_| "az_gl_context_ptr_cull_face")? };
        let az_gl_context_ptr_front_face = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_front_face").map_err(|_| "az_gl_context_ptr_front_face")? };
        let az_gl_context_ptr_enable = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_enable").map_err(|_| "az_gl_context_ptr_enable")? };
        let az_gl_context_ptr_disable = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_disable").map_err(|_| "az_gl_context_ptr_disable")? };
        let az_gl_context_ptr_hint = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_hint").map_err(|_| "az_gl_context_ptr_hint")? };
        let az_gl_context_ptr_is_enabled = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>(b"az_gl_context_ptr_is_enabled").map_err(|_| "az_gl_context_ptr_is_enabled")? };
        let az_gl_context_ptr_is_shader = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>(b"az_gl_context_ptr_is_shader").map_err(|_| "az_gl_context_ptr_is_shader")? };
        let az_gl_context_ptr_is_texture = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>(b"az_gl_context_ptr_is_texture").map_err(|_| "az_gl_context_ptr_is_texture")? };
        let az_gl_context_ptr_is_framebuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>(b"az_gl_context_ptr_is_framebuffer").map_err(|_| "az_gl_context_ptr_is_framebuffer")? };
        let az_gl_context_ptr_is_renderbuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>(b"az_gl_context_ptr_is_renderbuffer").map_err(|_| "az_gl_context_ptr_is_renderbuffer")? };
        let az_gl_context_ptr_check_frame_buffer_status = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u32>(b"az_gl_context_ptr_check_frame_buffer_status").map_err(|_| "az_gl_context_ptr_check_frame_buffer_status")? };
        let az_gl_context_ptr_enable_vertex_attrib_array = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_enable_vertex_attrib_array").map_err(|_| "az_gl_context_ptr_enable_vertex_attrib_array")? };
        let az_gl_context_ptr_disable_vertex_attrib_array = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_disable_vertex_attrib_array").map_err(|_| "az_gl_context_ptr_disable_vertex_attrib_array")? };
        let az_gl_context_ptr_uniform_1f = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  f32)>(b"az_gl_context_ptr_uniform_1f").map_err(|_| "az_gl_context_ptr_uniform_1f")? };
        let az_gl_context_ptr_uniform_1fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_1fv").map_err(|_| "az_gl_context_ptr_uniform_1fv")? };
        let az_gl_context_ptr_uniform_1i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32)>(b"az_gl_context_ptr_uniform_1i").map_err(|_| "az_gl_context_ptr_uniform_1i")? };
        let az_gl_context_ptr_uniform_1iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef)>(b"az_gl_context_ptr_uniform_1iv").map_err(|_| "az_gl_context_ptr_uniform_1iv")? };
        let az_gl_context_ptr_uniform_1ui = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  u32)>(b"az_gl_context_ptr_uniform_1ui").map_err(|_| "az_gl_context_ptr_uniform_1ui")? };
        let az_gl_context_ptr_uniform_2f = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32)>(b"az_gl_context_ptr_uniform_2f").map_err(|_| "az_gl_context_ptr_uniform_2f")? };
        let az_gl_context_ptr_uniform_2fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_2fv").map_err(|_| "az_gl_context_ptr_uniform_2fv")? };
        let az_gl_context_ptr_uniform_2i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_uniform_2i").map_err(|_| "az_gl_context_ptr_uniform_2i")? };
        let az_gl_context_ptr_uniform_2iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef)>(b"az_gl_context_ptr_uniform_2iv").map_err(|_| "az_gl_context_ptr_uniform_2iv")? };
        let az_gl_context_ptr_uniform_2ui = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32)>(b"az_gl_context_ptr_uniform_2ui").map_err(|_| "az_gl_context_ptr_uniform_2ui")? };
        let az_gl_context_ptr_uniform_3f = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32)>(b"az_gl_context_ptr_uniform_3f").map_err(|_| "az_gl_context_ptr_uniform_3f")? };
        let az_gl_context_ptr_uniform_3fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_3fv").map_err(|_| "az_gl_context_ptr_uniform_3fv")? };
        let az_gl_context_ptr_uniform_3i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_uniform_3i").map_err(|_| "az_gl_context_ptr_uniform_3i")? };
        let az_gl_context_ptr_uniform_3iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef)>(b"az_gl_context_ptr_uniform_3iv").map_err(|_| "az_gl_context_ptr_uniform_3iv")? };
        let az_gl_context_ptr_uniform_3ui = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_uniform_3ui").map_err(|_| "az_gl_context_ptr_uniform_3ui")? };
        let az_gl_context_ptr_uniform_4f = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32, _:  f32)>(b"az_gl_context_ptr_uniform_4f").map_err(|_| "az_gl_context_ptr_uniform_4f")? };
        let az_gl_context_ptr_uniform_4i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_uniform_4i").map_err(|_| "az_gl_context_ptr_uniform_4i")? };
        let az_gl_context_ptr_uniform_4iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef)>(b"az_gl_context_ptr_uniform_4iv").map_err(|_| "az_gl_context_ptr_uniform_4iv")? };
        let az_gl_context_ptr_uniform_4ui = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_uniform_4ui").map_err(|_| "az_gl_context_ptr_uniform_4ui")? };
        let az_gl_context_ptr_uniform_4fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_4fv").map_err(|_| "az_gl_context_ptr_uniform_4fv")? };
        let az_gl_context_ptr_uniform_matrix_2fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_matrix_2fv").map_err(|_| "az_gl_context_ptr_uniform_matrix_2fv")? };
        let az_gl_context_ptr_uniform_matrix_3fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_matrix_3fv").map_err(|_| "az_gl_context_ptr_uniform_matrix_3fv")? };
        let az_gl_context_ptr_uniform_matrix_4fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_matrix_4fv").map_err(|_| "az_gl_context_ptr_uniform_matrix_4fv")? };
        let az_gl_context_ptr_depth_mask = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  bool)>(b"az_gl_context_ptr_depth_mask").map_err(|_| "az_gl_context_ptr_depth_mask")? };
        let az_gl_context_ptr_depth_range = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f64, _:  f64)>(b"az_gl_context_ptr_depth_range").map_err(|_| "az_gl_context_ptr_depth_range")? };
        let az_gl_context_ptr_get_active_attrib = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveAttribReturn>(b"az_gl_context_ptr_get_active_attrib").map_err(|_| "az_gl_context_ptr_get_active_attrib")? };
        let az_gl_context_ptr_get_active_uniform = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveUniformReturn>(b"az_gl_context_ptr_get_active_uniform").map_err(|_| "az_gl_context_ptr_get_active_uniform")? };
        let az_gl_context_ptr_get_active_uniforms_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLuintVec, _:  u32) -> AzGLintVec>(b"az_gl_context_ptr_get_active_uniforms_iv").map_err(|_| "az_gl_context_ptr_get_active_uniforms_iv")? };
        let az_gl_context_ptr_get_active_uniform_block_i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32>(b"az_gl_context_ptr_get_active_uniform_block_i").map_err(|_| "az_gl_context_ptr_get_active_uniform_block_i")? };
        let az_gl_context_ptr_get_active_uniform_block_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> AzGLintVec>(b"az_gl_context_ptr_get_active_uniform_block_iv").map_err(|_| "az_gl_context_ptr_get_active_uniform_block_iv")? };
        let az_gl_context_ptr_get_active_uniform_block_name = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString>(b"az_gl_context_ptr_get_active_uniform_block_name").map_err(|_| "az_gl_context_ptr_get_active_uniform_block_name")? };
        let az_gl_context_ptr_get_attrib_location = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32>(b"az_gl_context_ptr_get_attrib_location").map_err(|_| "az_gl_context_ptr_get_attrib_location")? };
        let az_gl_context_ptr_get_frag_data_location = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32>(b"az_gl_context_ptr_get_frag_data_location").map_err(|_| "az_gl_context_ptr_get_frag_data_location")? };
        let az_gl_context_ptr_get_uniform_location = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32>(b"az_gl_context_ptr_get_uniform_location").map_err(|_| "az_gl_context_ptr_get_uniform_location")? };
        let az_gl_context_ptr_get_program_info_log = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> AzString>(b"az_gl_context_ptr_get_program_info_log").map_err(|_| "az_gl_context_ptr_get_program_info_log")? };
        let az_gl_context_ptr_get_program_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut)>(b"az_gl_context_ptr_get_program_iv").map_err(|_| "az_gl_context_ptr_get_program_iv")? };
        let az_gl_context_ptr_get_program_binary = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> AzGetProgramBinaryReturn>(b"az_gl_context_ptr_get_program_binary").map_err(|_| "az_gl_context_ptr_get_program_binary")? };
        let az_gl_context_ptr_program_binary = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzU8VecRef)>(b"az_gl_context_ptr_program_binary").map_err(|_| "az_gl_context_ptr_program_binary")? };
        let az_gl_context_ptr_program_parameter_i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32)>(b"az_gl_context_ptr_program_parameter_i").map_err(|_| "az_gl_context_ptr_program_parameter_i")? };
        let az_gl_context_ptr_get_vertex_attrib_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut)>(b"az_gl_context_ptr_get_vertex_attrib_iv").map_err(|_| "az_gl_context_ptr_get_vertex_attrib_iv")? };
        let az_gl_context_ptr_get_vertex_attrib_fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLfloatVecRefMut)>(b"az_gl_context_ptr_get_vertex_attrib_fv").map_err(|_| "az_gl_context_ptr_get_vertex_attrib_fv")? };
        let az_gl_context_ptr_get_vertex_attrib_pointer_v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> isize>(b"az_gl_context_ptr_get_vertex_attrib_pointer_v").map_err(|_| "az_gl_context_ptr_get_vertex_attrib_pointer_v")? };
        let az_gl_context_ptr_get_buffer_parameter_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32>(b"az_gl_context_ptr_get_buffer_parameter_iv").map_err(|_| "az_gl_context_ptr_get_buffer_parameter_iv")? };
        let az_gl_context_ptr_get_shader_info_log = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> AzString>(b"az_gl_context_ptr_get_shader_info_log").map_err(|_| "az_gl_context_ptr_get_shader_info_log")? };
        let az_gl_context_ptr_get_string = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> AzString>(b"az_gl_context_ptr_get_string").map_err(|_| "az_gl_context_ptr_get_string")? };
        let az_gl_context_ptr_get_string_i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString>(b"az_gl_context_ptr_get_string_i").map_err(|_| "az_gl_context_ptr_get_string_i")? };
        let az_gl_context_ptr_get_shader_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut)>(b"az_gl_context_ptr_get_shader_iv").map_err(|_| "az_gl_context_ptr_get_shader_iv")? };
        let az_gl_context_ptr_get_shader_precision_format = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> [i32;3]>(b"az_gl_context_ptr_get_shader_precision_format").map_err(|_| "az_gl_context_ptr_get_shader_precision_format")? };
        let az_gl_context_ptr_compile_shader = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_compile_shader").map_err(|_| "az_gl_context_ptr_compile_shader")? };
        let az_gl_context_ptr_create_program = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr) -> u32>(b"az_gl_context_ptr_create_program").map_err(|_| "az_gl_context_ptr_create_program")? };
        let az_gl_context_ptr_delete_program = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_delete_program").map_err(|_| "az_gl_context_ptr_delete_program")? };
        let az_gl_context_ptr_create_shader = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u32>(b"az_gl_context_ptr_create_shader").map_err(|_| "az_gl_context_ptr_create_shader")? };
        let az_gl_context_ptr_delete_shader = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_delete_shader").map_err(|_| "az_gl_context_ptr_delete_shader")? };
        let az_gl_context_ptr_detach_shader = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_detach_shader").map_err(|_| "az_gl_context_ptr_detach_shader")? };
        let az_gl_context_ptr_link_program = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_link_program").map_err(|_| "az_gl_context_ptr_link_program")? };
        let az_gl_context_ptr_clear_color = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32)>(b"az_gl_context_ptr_clear_color").map_err(|_| "az_gl_context_ptr_clear_color")? };
        let az_gl_context_ptr_clear = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_clear").map_err(|_| "az_gl_context_ptr_clear")? };
        let az_gl_context_ptr_clear_depth = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f64)>(b"az_gl_context_ptr_clear_depth").map_err(|_| "az_gl_context_ptr_clear_depth")? };
        let az_gl_context_ptr_clear_stencil = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32)>(b"az_gl_context_ptr_clear_stencil").map_err(|_| "az_gl_context_ptr_clear_stencil")? };
        let az_gl_context_ptr_flush = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr)>(b"az_gl_context_ptr_flush").map_err(|_| "az_gl_context_ptr_flush")? };
        let az_gl_context_ptr_finish = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr)>(b"az_gl_context_ptr_finish").map_err(|_| "az_gl_context_ptr_finish")? };
        let az_gl_context_ptr_get_error = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr) -> u32>(b"az_gl_context_ptr_get_error").map_err(|_| "az_gl_context_ptr_get_error")? };
        let az_gl_context_ptr_stencil_mask = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_stencil_mask").map_err(|_| "az_gl_context_ptr_stencil_mask")? };
        let az_gl_context_ptr_stencil_mask_separate = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_stencil_mask_separate").map_err(|_| "az_gl_context_ptr_stencil_mask_separate")? };
        let az_gl_context_ptr_stencil_func = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32)>(b"az_gl_context_ptr_stencil_func").map_err(|_| "az_gl_context_ptr_stencil_func")? };
        let az_gl_context_ptr_stencil_func_separate = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  u32)>(b"az_gl_context_ptr_stencil_func_separate").map_err(|_| "az_gl_context_ptr_stencil_func_separate")? };
        let az_gl_context_ptr_stencil_op = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_stencil_op").map_err(|_| "az_gl_context_ptr_stencil_op")? };
        let az_gl_context_ptr_stencil_op_separate = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_stencil_op_separate").map_err(|_| "az_gl_context_ptr_stencil_op_separate")? };
        let az_gl_context_ptr_egl_image_target_texture2d_oes = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void)>(b"az_gl_context_ptr_egl_image_target_texture2d_oes").map_err(|_| "az_gl_context_ptr_egl_image_target_texture2d_oes")? };
        let az_gl_context_ptr_generate_mipmap = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_generate_mipmap").map_err(|_| "az_gl_context_ptr_generate_mipmap")? };
        let az_gl_context_ptr_insert_event_marker_ext = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzRefstr)>(b"az_gl_context_ptr_insert_event_marker_ext").map_err(|_| "az_gl_context_ptr_insert_event_marker_ext")? };
        let az_gl_context_ptr_push_group_marker_ext = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzRefstr)>(b"az_gl_context_ptr_push_group_marker_ext").map_err(|_| "az_gl_context_ptr_push_group_marker_ext")? };
        let az_gl_context_ptr_pop_group_marker_ext = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr)>(b"az_gl_context_ptr_pop_group_marker_ext").map_err(|_| "az_gl_context_ptr_pop_group_marker_ext")? };
        let az_gl_context_ptr_debug_message_insert_khr = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  AzRefstr)>(b"az_gl_context_ptr_debug_message_insert_khr").map_err(|_| "az_gl_context_ptr_debug_message_insert_khr")? };
        let az_gl_context_ptr_push_debug_group_khr = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr)>(b"az_gl_context_ptr_push_debug_group_khr").map_err(|_| "az_gl_context_ptr_push_debug_group_khr")? };
        let az_gl_context_ptr_pop_debug_group_khr = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr)>(b"az_gl_context_ptr_pop_debug_group_khr").map_err(|_| "az_gl_context_ptr_pop_debug_group_khr")? };
        let az_gl_context_ptr_fence_sync = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGLsyncPtr>(b"az_gl_context_ptr_fence_sync").map_err(|_| "az_gl_context_ptr_fence_sync")? };
        let az_gl_context_ptr_client_wait_sync = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64)>(b"az_gl_context_ptr_client_wait_sync").map_err(|_| "az_gl_context_ptr_client_wait_sync")? };
        let az_gl_context_ptr_wait_sync = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64)>(b"az_gl_context_ptr_wait_sync").map_err(|_| "az_gl_context_ptr_wait_sync")? };
        let az_gl_context_ptr_delete_sync = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr)>(b"az_gl_context_ptr_delete_sync").map_err(|_| "az_gl_context_ptr_delete_sync")? };
        let az_gl_context_ptr_texture_range_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzU8VecRef)>(b"az_gl_context_ptr_texture_range_apple").map_err(|_| "az_gl_context_ptr_texture_range_apple")? };
        let az_gl_context_ptr_gen_fences_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_fences_apple").map_err(|_| "az_gl_context_ptr_gen_fences_apple")? };
        let az_gl_context_ptr_delete_fences_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_fences_apple").map_err(|_| "az_gl_context_ptr_delete_fences_apple")? };
        let az_gl_context_ptr_set_fence_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_set_fence_apple").map_err(|_| "az_gl_context_ptr_set_fence_apple")? };
        let az_gl_context_ptr_finish_fence_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_finish_fence_apple").map_err(|_| "az_gl_context_ptr_finish_fence_apple")? };
        let az_gl_context_ptr_test_fence_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_test_fence_apple").map_err(|_| "az_gl_context_ptr_test_fence_apple")? };
        let az_gl_context_ptr_test_object_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u8>(b"az_gl_context_ptr_test_object_apple").map_err(|_| "az_gl_context_ptr_test_object_apple")? };
        let az_gl_context_ptr_finish_object_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_finish_object_apple").map_err(|_| "az_gl_context_ptr_finish_object_apple")? };
        let az_gl_context_ptr_get_frag_data_index = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32>(b"az_gl_context_ptr_get_frag_data_index").map_err(|_| "az_gl_context_ptr_get_frag_data_index")? };
        let az_gl_context_ptr_blend_barrier_khr = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr)>(b"az_gl_context_ptr_blend_barrier_khr").map_err(|_| "az_gl_context_ptr_blend_barrier_khr")? };
        let az_gl_context_ptr_bind_frag_data_location_indexed = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  AzRefstr)>(b"az_gl_context_ptr_bind_frag_data_location_indexed").map_err(|_| "az_gl_context_ptr_bind_frag_data_location_indexed")? };
        let az_gl_context_ptr_get_debug_messages = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr) -> AzDebugMessageVec>(b"az_gl_context_ptr_get_debug_messages").map_err(|_| "az_gl_context_ptr_get_debug_messages")? };
        let az_gl_context_ptr_provoking_vertex_angle = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_provoking_vertex_angle").map_err(|_| "az_gl_context_ptr_provoking_vertex_angle")? };
        let az_gl_context_ptr_gen_vertex_arrays_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_vertex_arrays_apple").map_err(|_| "az_gl_context_ptr_gen_vertex_arrays_apple")? };
        let az_gl_context_ptr_bind_vertex_array_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_bind_vertex_array_apple").map_err(|_| "az_gl_context_ptr_bind_vertex_array_apple")? };
        let az_gl_context_ptr_delete_vertex_arrays_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_vertex_arrays_apple").map_err(|_| "az_gl_context_ptr_delete_vertex_arrays_apple")? };
        let az_gl_context_ptr_copy_texture_chromium = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8)>(b"az_gl_context_ptr_copy_texture_chromium").map_err(|_| "az_gl_context_ptr_copy_texture_chromium")? };
        let az_gl_context_ptr_copy_sub_texture_chromium = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8)>(b"az_gl_context_ptr_copy_sub_texture_chromium").map_err(|_| "az_gl_context_ptr_copy_sub_texture_chromium")? };
        let az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void)>(b"az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes").map_err(|_| "az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes")? };
        let az_gl_context_ptr_copy_texture_3d_angle = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8)>(b"az_gl_context_ptr_copy_texture_3d_angle").map_err(|_| "az_gl_context_ptr_copy_texture_3d_angle")? };
        let az_gl_context_ptr_copy_sub_texture_3d_angle = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8)>(b"az_gl_context_ptr_copy_sub_texture_3d_angle").map_err(|_| "az_gl_context_ptr_copy_sub_texture_3d_angle")? };
        let az_gl_context_ptr_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlContextPtr)>(b"az_gl_context_ptr_delete").map_err(|_| "az_gl_context_ptr_delete")? };
        let az_gl_context_ptr_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr) -> AzGlContextPtr>(b"az_gl_context_ptr_deep_copy").map_err(|_| "az_gl_context_ptr_deep_copy")? };
        let az_texture_delete = unsafe { lib.get::<extern fn(_:  &mut AzTexture)>(b"az_texture_delete").map_err(|_| "az_texture_delete")? };
        let az_texture_flags_delete = unsafe { lib.get::<extern fn(_:  &mut AzTextureFlags)>(b"az_texture_flags_delete").map_err(|_| "az_texture_flags_delete")? };
        let az_texture_flags_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTextureFlags) -> AzTextureFlags>(b"az_texture_flags_deep_copy").map_err(|_| "az_texture_flags_deep_copy")? };
        let az_text_id_new = unsafe { lib.get::<extern fn() -> AzTextId>(b"az_text_id_new").map_err(|_| "az_text_id_new")? };
        let az_text_id_delete = unsafe { lib.get::<extern fn(_:  &mut AzTextId)>(b"az_text_id_delete").map_err(|_| "az_text_id_delete")? };
        let az_text_id_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTextId) -> AzTextId>(b"az_text_id_deep_copy").map_err(|_| "az_text_id_deep_copy")? };
        let az_image_id_new = unsafe { lib.get::<extern fn() -> AzImageId>(b"az_image_id_new").map_err(|_| "az_image_id_new")? };
        let az_image_id_delete = unsafe { lib.get::<extern fn(_:  &mut AzImageId)>(b"az_image_id_delete").map_err(|_| "az_image_id_delete")? };
        let az_image_id_deep_copy = unsafe { lib.get::<extern fn(_:  &AzImageId) -> AzImageId>(b"az_image_id_deep_copy").map_err(|_| "az_image_id_deep_copy")? };
        let az_font_id_new = unsafe { lib.get::<extern fn() -> AzFontId>(b"az_font_id_new").map_err(|_| "az_font_id_new")? };
        let az_font_id_delete = unsafe { lib.get::<extern fn(_:  &mut AzFontId)>(b"az_font_id_delete").map_err(|_| "az_font_id_delete")? };
        let az_font_id_deep_copy = unsafe { lib.get::<extern fn(_:  &AzFontId) -> AzFontId>(b"az_font_id_deep_copy").map_err(|_| "az_font_id_deep_copy")? };
        let az_image_source_delete = unsafe { lib.get::<extern fn(_:  &mut AzImageSource)>(b"az_image_source_delete").map_err(|_| "az_image_source_delete")? };
        let az_image_source_deep_copy = unsafe { lib.get::<extern fn(_:  &AzImageSource) -> AzImageSource>(b"az_image_source_deep_copy").map_err(|_| "az_image_source_deep_copy")? };
        let az_font_source_delete = unsafe { lib.get::<extern fn(_:  &mut AzFontSource)>(b"az_font_source_delete").map_err(|_| "az_font_source_delete")? };
        let az_font_source_deep_copy = unsafe { lib.get::<extern fn(_:  &AzFontSource) -> AzFontSource>(b"az_font_source_deep_copy").map_err(|_| "az_font_source_deep_copy")? };
        let az_raw_image_new = unsafe { lib.get::<extern fn(_:  AzU8Vec, _:  usize, _:  usize, _:  AzRawImageFormat) -> AzRawImage>(b"az_raw_image_new").map_err(|_| "az_raw_image_new")? };
        let az_raw_image_delete = unsafe { lib.get::<extern fn(_:  &mut AzRawImage)>(b"az_raw_image_delete").map_err(|_| "az_raw_image_delete")? };
        let az_raw_image_deep_copy = unsafe { lib.get::<extern fn(_:  &AzRawImage) -> AzRawImage>(b"az_raw_image_deep_copy").map_err(|_| "az_raw_image_deep_copy")? };
        let az_raw_image_format_delete = unsafe { lib.get::<extern fn(_:  &mut AzRawImageFormat)>(b"az_raw_image_format_delete").map_err(|_| "az_raw_image_format_delete")? };
        let az_raw_image_format_deep_copy = unsafe { lib.get::<extern fn(_:  &AzRawImageFormat) -> AzRawImageFormat>(b"az_raw_image_format_deep_copy").map_err(|_| "az_raw_image_format_deep_copy")? };
        let az_drop_check_ptr_delete = unsafe { lib.get::<extern fn(_:  &mut AzDropCheckPtrPtr)>(b"az_drop_check_ptr_delete").map_err(|_| "az_drop_check_ptr_delete")? };
        let az_arc_mutex_ref_any_delete = unsafe { lib.get::<extern fn(_:  &mut AzArcMutexRefAnyPtr)>(b"az_arc_mutex_ref_any_delete").map_err(|_| "az_arc_mutex_ref_any_delete")? };
        let az_timer_delete = unsafe { lib.get::<extern fn(_:  &mut AzTimer)>(b"az_timer_delete").map_err(|_| "az_timer_delete")? };
        let az_timer_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTimer) -> AzTimer>(b"az_timer_deep_copy").map_err(|_| "az_timer_deep_copy")? };
        let az_task_new = unsafe { lib.get::<extern fn(_:  AzArcMutexRefAnyPtr, _:  AzTaskCallbackType) -> AzTaskPtr>(b"az_task_new").map_err(|_| "az_task_new")? };
        let az_task_then = unsafe { lib.get::<extern fn(_:  AzTaskPtr, _:  AzTimer) -> AzTaskPtr>(b"az_task_then").map_err(|_| "az_task_then")? };
        let az_task_delete = unsafe { lib.get::<extern fn(_:  &mut AzTaskPtr)>(b"az_task_delete").map_err(|_| "az_task_delete")? };
        let az_thread_new = unsafe { lib.get::<extern fn(_:  AzRefAny, _:  AzThreadCallbackType) -> AzThreadPtr>(b"az_thread_new").map_err(|_| "az_thread_new")? };
        let az_thread_block = unsafe { lib.get::<extern fn(_:  AzThreadPtr) -> AzResultRefAnyBlockError>(b"az_thread_block").map_err(|_| "az_thread_block")? };
        let az_thread_delete = unsafe { lib.get::<extern fn(_:  &mut AzThreadPtr)>(b"az_thread_delete").map_err(|_| "az_thread_delete")? };
        let az_drop_check_delete = unsafe { lib.get::<extern fn(_:  &mut AzDropCheckPtr)>(b"az_drop_check_delete").map_err(|_| "az_drop_check_delete")? };
        let az_timer_id_delete = unsafe { lib.get::<extern fn(_:  &mut AzTimerId)>(b"az_timer_id_delete").map_err(|_| "az_timer_id_delete")? };
        let az_timer_id_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTimerId) -> AzTimerId>(b"az_timer_id_deep_copy").map_err(|_| "az_timer_id_deep_copy")? };
        let az_terminate_timer_delete = unsafe { lib.get::<extern fn(_:  &mut AzTerminateTimer)>(b"az_terminate_timer_delete").map_err(|_| "az_terminate_timer_delete")? };
        let az_terminate_timer_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTerminateTimer) -> AzTerminateTimer>(b"az_terminate_timer_deep_copy").map_err(|_| "az_terminate_timer_deep_copy")? };
        let az_block_error_delete = unsafe { lib.get::<extern fn(_:  &mut AzBlockError)>(b"az_block_error_delete").map_err(|_| "az_block_error_delete")? };
        let az_block_error_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBlockError) -> AzBlockError>(b"az_block_error_deep_copy").map_err(|_| "az_block_error_deep_copy")? };
        let az_task_bar_icon_delete = unsafe { lib.get::<extern fn(_:  &mut AzTaskBarIcon)>(b"az_task_bar_icon_delete").map_err(|_| "az_task_bar_icon_delete")? };
        let az_task_bar_icon_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTaskBarIcon) -> AzTaskBarIcon>(b"az_task_bar_icon_deep_copy").map_err(|_| "az_task_bar_icon_deep_copy")? };
        let az_x_window_type_delete = unsafe { lib.get::<extern fn(_:  &mut AzXWindowType)>(b"az_x_window_type_delete").map_err(|_| "az_x_window_type_delete")? };
        let az_x_window_type_deep_copy = unsafe { lib.get::<extern fn(_:  &AzXWindowType) -> AzXWindowType>(b"az_x_window_type_deep_copy").map_err(|_| "az_x_window_type_deep_copy")? };
        let az_physical_position_i32_delete = unsafe { lib.get::<extern fn(_:  &mut AzPhysicalPositionI32)>(b"az_physical_position_i32_delete").map_err(|_| "az_physical_position_i32_delete")? };
        let az_physical_position_i32_deep_copy = unsafe { lib.get::<extern fn(_:  &AzPhysicalPositionI32) -> AzPhysicalPositionI32>(b"az_physical_position_i32_deep_copy").map_err(|_| "az_physical_position_i32_deep_copy")? };
        let az_logical_position_delete = unsafe { lib.get::<extern fn(_:  &mut AzLogicalPosition)>(b"az_logical_position_delete").map_err(|_| "az_logical_position_delete")? };
        let az_logical_position_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLogicalPosition) -> AzLogicalPosition>(b"az_logical_position_deep_copy").map_err(|_| "az_logical_position_deep_copy")? };
        let az_icon_key_delete = unsafe { lib.get::<extern fn(_:  &mut AzIconKey)>(b"az_icon_key_delete").map_err(|_| "az_icon_key_delete")? };
        let az_icon_key_deep_copy = unsafe { lib.get::<extern fn(_:  &AzIconKey) -> AzIconKey>(b"az_icon_key_deep_copy").map_err(|_| "az_icon_key_deep_copy")? };
        let az_small_window_icon_bytes_delete = unsafe { lib.get::<extern fn(_:  &mut AzSmallWindowIconBytes)>(b"az_small_window_icon_bytes_delete").map_err(|_| "az_small_window_icon_bytes_delete")? };
        let az_small_window_icon_bytes_deep_copy = unsafe { lib.get::<extern fn(_:  &AzSmallWindowIconBytes) -> AzSmallWindowIconBytes>(b"az_small_window_icon_bytes_deep_copy").map_err(|_| "az_small_window_icon_bytes_deep_copy")? };
        let az_large_window_icon_bytes_delete = unsafe { lib.get::<extern fn(_:  &mut AzLargeWindowIconBytes)>(b"az_large_window_icon_bytes_delete").map_err(|_| "az_large_window_icon_bytes_delete")? };
        let az_large_window_icon_bytes_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLargeWindowIconBytes) -> AzLargeWindowIconBytes>(b"az_large_window_icon_bytes_deep_copy").map_err(|_| "az_large_window_icon_bytes_deep_copy")? };
        let az_window_icon_delete = unsafe { lib.get::<extern fn(_:  &mut AzWindowIcon)>(b"az_window_icon_delete").map_err(|_| "az_window_icon_delete")? };
        let az_window_icon_deep_copy = unsafe { lib.get::<extern fn(_:  &AzWindowIcon) -> AzWindowIcon>(b"az_window_icon_deep_copy").map_err(|_| "az_window_icon_deep_copy")? };
        let az_virtual_key_code_delete = unsafe { lib.get::<extern fn(_:  &mut AzVirtualKeyCode)>(b"az_virtual_key_code_delete").map_err(|_| "az_virtual_key_code_delete")? };
        let az_virtual_key_code_deep_copy = unsafe { lib.get::<extern fn(_:  &AzVirtualKeyCode) -> AzVirtualKeyCode>(b"az_virtual_key_code_deep_copy").map_err(|_| "az_virtual_key_code_deep_copy")? };
        let az_accelerator_key_delete = unsafe { lib.get::<extern fn(_:  &mut AzAcceleratorKey)>(b"az_accelerator_key_delete").map_err(|_| "az_accelerator_key_delete")? };
        let az_accelerator_key_deep_copy = unsafe { lib.get::<extern fn(_:  &AzAcceleratorKey) -> AzAcceleratorKey>(b"az_accelerator_key_deep_copy").map_err(|_| "az_accelerator_key_deep_copy")? };
        let az_window_size_delete = unsafe { lib.get::<extern fn(_:  &mut AzWindowSize)>(b"az_window_size_delete").map_err(|_| "az_window_size_delete")? };
        let az_window_size_deep_copy = unsafe { lib.get::<extern fn(_:  &AzWindowSize) -> AzWindowSize>(b"az_window_size_deep_copy").map_err(|_| "az_window_size_deep_copy")? };
        let az_window_flags_delete = unsafe { lib.get::<extern fn(_:  &mut AzWindowFlags)>(b"az_window_flags_delete").map_err(|_| "az_window_flags_delete")? };
        let az_window_flags_deep_copy = unsafe { lib.get::<extern fn(_:  &AzWindowFlags) -> AzWindowFlags>(b"az_window_flags_deep_copy").map_err(|_| "az_window_flags_deep_copy")? };
        let az_debug_state_delete = unsafe { lib.get::<extern fn(_:  &mut AzDebugState)>(b"az_debug_state_delete").map_err(|_| "az_debug_state_delete")? };
        let az_debug_state_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDebugState) -> AzDebugState>(b"az_debug_state_deep_copy").map_err(|_| "az_debug_state_deep_copy")? };
        let az_keyboard_state_delete = unsafe { lib.get::<extern fn(_:  &mut AzKeyboardState)>(b"az_keyboard_state_delete").map_err(|_| "az_keyboard_state_delete")? };
        let az_keyboard_state_deep_copy = unsafe { lib.get::<extern fn(_:  &AzKeyboardState) -> AzKeyboardState>(b"az_keyboard_state_deep_copy").map_err(|_| "az_keyboard_state_deep_copy")? };
        let az_mouse_cursor_type_delete = unsafe { lib.get::<extern fn(_:  &mut AzMouseCursorType)>(b"az_mouse_cursor_type_delete").map_err(|_| "az_mouse_cursor_type_delete")? };
        let az_mouse_cursor_type_deep_copy = unsafe { lib.get::<extern fn(_:  &AzMouseCursorType) -> AzMouseCursorType>(b"az_mouse_cursor_type_deep_copy").map_err(|_| "az_mouse_cursor_type_deep_copy")? };
        let az_cursor_position_delete = unsafe { lib.get::<extern fn(_:  &mut AzCursorPosition)>(b"az_cursor_position_delete").map_err(|_| "az_cursor_position_delete")? };
        let az_cursor_position_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCursorPosition) -> AzCursorPosition>(b"az_cursor_position_deep_copy").map_err(|_| "az_cursor_position_deep_copy")? };
        let az_mouse_state_delete = unsafe { lib.get::<extern fn(_:  &mut AzMouseState)>(b"az_mouse_state_delete").map_err(|_| "az_mouse_state_delete")? };
        let az_mouse_state_deep_copy = unsafe { lib.get::<extern fn(_:  &AzMouseState) -> AzMouseState>(b"az_mouse_state_deep_copy").map_err(|_| "az_mouse_state_deep_copy")? };
        let az_platform_specific_options_delete = unsafe { lib.get::<extern fn(_:  &mut AzPlatformSpecificOptions)>(b"az_platform_specific_options_delete").map_err(|_| "az_platform_specific_options_delete")? };
        let az_platform_specific_options_deep_copy = unsafe { lib.get::<extern fn(_:  &AzPlatformSpecificOptions) -> AzPlatformSpecificOptions>(b"az_platform_specific_options_deep_copy").map_err(|_| "az_platform_specific_options_deep_copy")? };
        let az_windows_window_options_delete = unsafe { lib.get::<extern fn(_:  &mut AzWindowsWindowOptions)>(b"az_windows_window_options_delete").map_err(|_| "az_windows_window_options_delete")? };
        let az_windows_window_options_deep_copy = unsafe { lib.get::<extern fn(_:  &AzWindowsWindowOptions) -> AzWindowsWindowOptions>(b"az_windows_window_options_deep_copy").map_err(|_| "az_windows_window_options_deep_copy")? };
        let az_wayland_theme_delete = unsafe { lib.get::<extern fn(_:  &mut AzWaylandTheme)>(b"az_wayland_theme_delete").map_err(|_| "az_wayland_theme_delete")? };
        let az_wayland_theme_deep_copy = unsafe { lib.get::<extern fn(_:  &AzWaylandTheme) -> AzWaylandTheme>(b"az_wayland_theme_deep_copy").map_err(|_| "az_wayland_theme_deep_copy")? };
        let az_renderer_type_delete = unsafe { lib.get::<extern fn(_:  &mut AzRendererType)>(b"az_renderer_type_delete").map_err(|_| "az_renderer_type_delete")? };
        let az_renderer_type_deep_copy = unsafe { lib.get::<extern fn(_:  &AzRendererType) -> AzRendererType>(b"az_renderer_type_deep_copy").map_err(|_| "az_renderer_type_deep_copy")? };
        let az_string_pair_delete = unsafe { lib.get::<extern fn(_:  &mut AzStringPair)>(b"az_string_pair_delete").map_err(|_| "az_string_pair_delete")? };
        let az_string_pair_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStringPair) -> AzStringPair>(b"az_string_pair_deep_copy").map_err(|_| "az_string_pair_deep_copy")? };
        let az_linux_window_options_delete = unsafe { lib.get::<extern fn(_:  &mut AzLinuxWindowOptions)>(b"az_linux_window_options_delete").map_err(|_| "az_linux_window_options_delete")? };
        let az_linux_window_options_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLinuxWindowOptions) -> AzLinuxWindowOptions>(b"az_linux_window_options_deep_copy").map_err(|_| "az_linux_window_options_deep_copy")? };
        let az_mac_window_options_delete = unsafe { lib.get::<extern fn(_:  &mut AzMacWindowOptions)>(b"az_mac_window_options_delete").map_err(|_| "az_mac_window_options_delete")? };
        let az_mac_window_options_deep_copy = unsafe { lib.get::<extern fn(_:  &AzMacWindowOptions) -> AzMacWindowOptions>(b"az_mac_window_options_deep_copy").map_err(|_| "az_mac_window_options_deep_copy")? };
        let az_wasm_window_options_delete = unsafe { lib.get::<extern fn(_:  &mut AzWasmWindowOptions)>(b"az_wasm_window_options_delete").map_err(|_| "az_wasm_window_options_delete")? };
        let az_wasm_window_options_deep_copy = unsafe { lib.get::<extern fn(_:  &AzWasmWindowOptions) -> AzWasmWindowOptions>(b"az_wasm_window_options_deep_copy").map_err(|_| "az_wasm_window_options_deep_copy")? };
        let az_full_screen_mode_delete = unsafe { lib.get::<extern fn(_:  &mut AzFullScreenMode)>(b"az_full_screen_mode_delete").map_err(|_| "az_full_screen_mode_delete")? };
        let az_full_screen_mode_deep_copy = unsafe { lib.get::<extern fn(_:  &AzFullScreenMode) -> AzFullScreenMode>(b"az_full_screen_mode_deep_copy").map_err(|_| "az_full_screen_mode_deep_copy")? };
        let az_window_state_delete = unsafe { lib.get::<extern fn(_:  &mut AzWindowState)>(b"az_window_state_delete").map_err(|_| "az_window_state_delete")? };
        let az_window_state_deep_copy = unsafe { lib.get::<extern fn(_:  &AzWindowState) -> AzWindowState>(b"az_window_state_deep_copy").map_err(|_| "az_window_state_deep_copy")? };
        let az_logical_size_delete = unsafe { lib.get::<extern fn(_:  &mut AzLogicalSize)>(b"az_logical_size_delete").map_err(|_| "az_logical_size_delete")? };
        let az_logical_size_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLogicalSize) -> AzLogicalSize>(b"az_logical_size_deep_copy").map_err(|_| "az_logical_size_deep_copy")? };
        let az_hot_reload_options_delete = unsafe { lib.get::<extern fn(_:  &mut AzHotReloadOptions)>(b"az_hot_reload_options_delete").map_err(|_| "az_hot_reload_options_delete")? };
        let az_hot_reload_options_deep_copy = unsafe { lib.get::<extern fn(_:  &AzHotReloadOptions) -> AzHotReloadOptions>(b"az_hot_reload_options_deep_copy").map_err(|_| "az_hot_reload_options_deep_copy")? };
        let az_window_create_options_new = unsafe { lib.get::<extern fn(_:  AzCss) -> AzWindowCreateOptions>(b"az_window_create_options_new").map_err(|_| "az_window_create_options_new")? };
        let az_window_create_options_delete = unsafe { lib.get::<extern fn(_:  &mut AzWindowCreateOptions)>(b"az_window_create_options_delete").map_err(|_| "az_window_create_options_delete")? };
        let az_window_create_options_deep_copy = unsafe { lib.get::<extern fn(_:  &AzWindowCreateOptions) -> AzWindowCreateOptions>(b"az_window_create_options_deep_copy").map_err(|_| "az_window_create_options_deep_copy")? };
        Ok(AzulDll {
            lib: Box::new(lib),
            az_string_from_utf8_unchecked,
            az_string_from_utf8_lossy,
            az_string_into_bytes,
            az_string_delete,
            az_string_deep_copy,
            az_x_window_type_vec_copy_from,
            az_x_window_type_vec_delete,
            az_x_window_type_vec_deep_copy,
            az_virtual_key_code_vec_copy_from,
            az_virtual_key_code_vec_delete,
            az_virtual_key_code_vec_deep_copy,
            az_scan_code_vec_copy_from,
            az_scan_code_vec_delete,
            az_scan_code_vec_deep_copy,
            az_css_declaration_vec_copy_from,
            az_css_declaration_vec_delete,
            az_css_declaration_vec_deep_copy,
            az_css_path_selector_vec_copy_from,
            az_css_path_selector_vec_delete,
            az_css_path_selector_vec_deep_copy,
            az_stylesheet_vec_copy_from,
            az_stylesheet_vec_delete,
            az_stylesheet_vec_deep_copy,
            az_css_rule_block_vec_copy_from,
            az_css_rule_block_vec_delete,
            az_css_rule_block_vec_deep_copy,
            az_u8_vec_copy_from,
            az_u8_vec_delete,
            az_u8_vec_deep_copy,
            az_callback_data_vec_copy_from,
            az_callback_data_vec_delete,
            az_callback_data_vec_deep_copy,
            az_debug_message_vec_copy_from,
            az_debug_message_vec_delete,
            az_debug_message_vec_deep_copy,
            az_g_luint_vec_copy_from,
            az_g_luint_vec_delete,
            az_g_luint_vec_deep_copy,
            az_g_lint_vec_copy_from,
            az_g_lint_vec_delete,
            az_g_lint_vec_deep_copy,
            az_override_property_vec_copy_from,
            az_override_property_vec_delete,
            az_override_property_vec_deep_copy,
            az_dom_vec_copy_from,
            az_dom_vec_delete,
            az_dom_vec_deep_copy,
            az_string_vec_copy_from,
            az_string_vec_delete,
            az_string_vec_deep_copy,
            az_string_pair_vec_copy_from,
            az_string_pair_vec_delete,
            az_string_pair_vec_deep_copy,
            az_gradient_stop_pre_vec_copy_from,
            az_gradient_stop_pre_vec_delete,
            az_gradient_stop_pre_vec_deep_copy,
            az_option_wayland_theme_delete,
            az_option_wayland_theme_deep_copy,
            az_option_task_bar_icon_delete,
            az_option_task_bar_icon_deep_copy,
            az_option_hwnd_handle_delete,
            az_option_hwnd_handle_deep_copy,
            az_option_logical_position_delete,
            az_option_logical_position_deep_copy,
            az_option_hot_reload_options_delete,
            az_option_hot_reload_options_deep_copy,
            az_option_physical_position_i32_delete,
            az_option_physical_position_i32_deep_copy,
            az_option_window_icon_delete,
            az_option_window_icon_deep_copy,
            az_option_string_delete,
            az_option_string_deep_copy,
            az_option_x11_visual_delete,
            az_option_x11_visual_deep_copy,
            az_option_i32_delete,
            az_option_i32_deep_copy,
            az_option_f32_delete,
            az_option_f32_deep_copy,
            az_option_mouse_cursor_type_delete,
            az_option_mouse_cursor_type_deep_copy,
            az_option_logical_size_delete,
            az_option_logical_size_deep_copy,
            az_option_char_delete,
            az_option_char_deep_copy,
            az_option_virtual_key_code_delete,
            az_option_virtual_key_code_deep_copy,
            az_option_percentage_value_delete,
            az_option_percentage_value_deep_copy,
            az_option_dom_delete,
            az_option_dom_deep_copy,
            az_option_texture_delete,
            az_option_tab_index_delete,
            az_option_tab_index_deep_copy,
            az_option_duration_delete,
            az_option_duration_deep_copy,
            az_option_instant_ptr_delete,
            az_option_instant_ptr_deep_copy,
            az_option_usize_delete,
            az_option_usize_deep_copy,
            az_option_u8_vec_ref_delete,
            az_result_ref_any_block_error_delete,
            az_result_ref_any_block_error_deep_copy,
            az_instant_now,
            az_instant_delete,
            az_duration_delete,
            az_duration_deep_copy,
            az_app_config_default,
            az_app_config_delete,
            az_app_new,
            az_app_run,
            az_app_delete,
            az_layout_callback_delete,
            az_layout_callback_deep_copy,
            az_callback_delete,
            az_callback_deep_copy,
            az_callback_info_delete,
            az_update_screen_delete,
            az_update_screen_deep_copy,
            az_i_frame_callback_delete,
            az_i_frame_callback_deep_copy,
            az_i_frame_callback_info_get_state,
            az_i_frame_callback_info_delete,
            az_i_frame_callback_return_delete,
            az_i_frame_callback_return_deep_copy,
            az_gl_callback_delete,
            az_gl_callback_deep_copy,
            az_gl_callback_info_delete,
            az_gl_callback_return_delete,
            az_timer_callback_delete,
            az_timer_callback_deep_copy,
            az_timer_callback_type_delete,
            az_timer_callback_info_delete,
            az_timer_callback_return_delete,
            az_timer_callback_return_deep_copy,
            az_ref_any_sharing_info_can_be_shared,
            az_ref_any_sharing_info_can_be_shared_mut,
            az_ref_any_sharing_info_increase_ref,
            az_ref_any_sharing_info_decrease_ref,
            az_ref_any_sharing_info_increase_refmut,
            az_ref_any_sharing_info_decrease_refmut,
            az_ref_any_sharing_info_delete,
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
            az_layout_info_delete,
            az_css_rule_block_delete,
            az_css_rule_block_deep_copy,
            az_css_declaration_delete,
            az_css_declaration_deep_copy,
            az_dynamic_css_property_delete,
            az_dynamic_css_property_deep_copy,
            az_css_path_delete,
            az_css_path_deep_copy,
            az_css_path_selector_delete,
            az_css_path_selector_deep_copy,
            az_node_type_path_delete,
            az_node_type_path_deep_copy,
            az_css_path_pseudo_selector_delete,
            az_css_path_pseudo_selector_deep_copy,
            az_css_nth_child_selector_delete,
            az_css_nth_child_selector_deep_copy,
            az_css_nth_child_pattern_delete,
            az_css_nth_child_pattern_deep_copy,
            az_stylesheet_delete,
            az_stylesheet_deep_copy,
            az_css_native,
            az_css_empty,
            az_css_from_string,
            az_css_override_native,
            az_css_delete,
            az_css_deep_copy,
            az_color_u_delete,
            az_color_u_deep_copy,
            az_size_metric_delete,
            az_size_metric_deep_copy,
            az_float_value_delete,
            az_float_value_deep_copy,
            az_pixel_value_delete,
            az_pixel_value_deep_copy,
            az_pixel_value_no_percent_delete,
            az_pixel_value_no_percent_deep_copy,
            az_box_shadow_clip_mode_delete,
            az_box_shadow_clip_mode_deep_copy,
            az_box_shadow_pre_display_item_delete,
            az_box_shadow_pre_display_item_deep_copy,
            az_layout_align_content_delete,
            az_layout_align_content_deep_copy,
            az_layout_align_items_delete,
            az_layout_align_items_deep_copy,
            az_layout_bottom_delete,
            az_layout_bottom_deep_copy,
            az_layout_box_sizing_delete,
            az_layout_box_sizing_deep_copy,
            az_layout_direction_delete,
            az_layout_direction_deep_copy,
            az_layout_display_delete,
            az_layout_display_deep_copy,
            az_layout_flex_grow_delete,
            az_layout_flex_grow_deep_copy,
            az_layout_flex_shrink_delete,
            az_layout_flex_shrink_deep_copy,
            az_layout_float_delete,
            az_layout_float_deep_copy,
            az_layout_height_delete,
            az_layout_height_deep_copy,
            az_layout_justify_content_delete,
            az_layout_justify_content_deep_copy,
            az_layout_left_delete,
            az_layout_left_deep_copy,
            az_layout_margin_bottom_delete,
            az_layout_margin_bottom_deep_copy,
            az_layout_margin_left_delete,
            az_layout_margin_left_deep_copy,
            az_layout_margin_right_delete,
            az_layout_margin_right_deep_copy,
            az_layout_margin_top_delete,
            az_layout_margin_top_deep_copy,
            az_layout_max_height_delete,
            az_layout_max_height_deep_copy,
            az_layout_max_width_delete,
            az_layout_max_width_deep_copy,
            az_layout_min_height_delete,
            az_layout_min_height_deep_copy,
            az_layout_min_width_delete,
            az_layout_min_width_deep_copy,
            az_layout_padding_bottom_delete,
            az_layout_padding_bottom_deep_copy,
            az_layout_padding_left_delete,
            az_layout_padding_left_deep_copy,
            az_layout_padding_right_delete,
            az_layout_padding_right_deep_copy,
            az_layout_padding_top_delete,
            az_layout_padding_top_deep_copy,
            az_layout_position_delete,
            az_layout_position_deep_copy,
            az_layout_right_delete,
            az_layout_right_deep_copy,
            az_layout_top_delete,
            az_layout_top_deep_copy,
            az_layout_width_delete,
            az_layout_width_deep_copy,
            az_layout_wrap_delete,
            az_layout_wrap_deep_copy,
            az_overflow_delete,
            az_overflow_deep_copy,
            az_percentage_value_delete,
            az_percentage_value_deep_copy,
            az_gradient_stop_pre_delete,
            az_gradient_stop_pre_deep_copy,
            az_direction_corner_delete,
            az_direction_corner_deep_copy,
            az_direction_corners_delete,
            az_direction_corners_deep_copy,
            az_direction_delete,
            az_direction_deep_copy,
            az_extend_mode_delete,
            az_extend_mode_deep_copy,
            az_linear_gradient_delete,
            az_linear_gradient_deep_copy,
            az_shape_delete,
            az_shape_deep_copy,
            az_radial_gradient_delete,
            az_radial_gradient_deep_copy,
            az_css_image_id_delete,
            az_css_image_id_deep_copy,
            az_style_background_content_delete,
            az_style_background_content_deep_copy,
            az_background_position_horizontal_delete,
            az_background_position_horizontal_deep_copy,
            az_background_position_vertical_delete,
            az_background_position_vertical_deep_copy,
            az_style_background_position_delete,
            az_style_background_position_deep_copy,
            az_style_background_repeat_delete,
            az_style_background_repeat_deep_copy,
            az_style_background_size_delete,
            az_style_background_size_deep_copy,
            az_style_border_bottom_color_delete,
            az_style_border_bottom_color_deep_copy,
            az_style_border_bottom_left_radius_delete,
            az_style_border_bottom_left_radius_deep_copy,
            az_style_border_bottom_right_radius_delete,
            az_style_border_bottom_right_radius_deep_copy,
            az_border_style_delete,
            az_border_style_deep_copy,
            az_style_border_bottom_style_delete,
            az_style_border_bottom_style_deep_copy,
            az_style_border_bottom_width_delete,
            az_style_border_bottom_width_deep_copy,
            az_style_border_left_color_delete,
            az_style_border_left_color_deep_copy,
            az_style_border_left_style_delete,
            az_style_border_left_style_deep_copy,
            az_style_border_left_width_delete,
            az_style_border_left_width_deep_copy,
            az_style_border_right_color_delete,
            az_style_border_right_color_deep_copy,
            az_style_border_right_style_delete,
            az_style_border_right_style_deep_copy,
            az_style_border_right_width_delete,
            az_style_border_right_width_deep_copy,
            az_style_border_top_color_delete,
            az_style_border_top_color_deep_copy,
            az_style_border_top_left_radius_delete,
            az_style_border_top_left_radius_deep_copy,
            az_style_border_top_right_radius_delete,
            az_style_border_top_right_radius_deep_copy,
            az_style_border_top_style_delete,
            az_style_border_top_style_deep_copy,
            az_style_border_top_width_delete,
            az_style_border_top_width_deep_copy,
            az_style_cursor_delete,
            az_style_cursor_deep_copy,
            az_style_font_family_delete,
            az_style_font_family_deep_copy,
            az_style_font_size_delete,
            az_style_font_size_deep_copy,
            az_style_letter_spacing_delete,
            az_style_letter_spacing_deep_copy,
            az_style_line_height_delete,
            az_style_line_height_deep_copy,
            az_style_tab_width_delete,
            az_style_tab_width_deep_copy,
            az_style_text_alignment_horz_delete,
            az_style_text_alignment_horz_deep_copy,
            az_style_text_color_delete,
            az_style_text_color_deep_copy,
            az_style_word_spacing_delete,
            az_style_word_spacing_deep_copy,
            az_box_shadow_pre_display_item_value_delete,
            az_box_shadow_pre_display_item_value_deep_copy,
            az_layout_align_content_value_delete,
            az_layout_align_content_value_deep_copy,
            az_layout_align_items_value_delete,
            az_layout_align_items_value_deep_copy,
            az_layout_bottom_value_delete,
            az_layout_bottom_value_deep_copy,
            az_layout_box_sizing_value_delete,
            az_layout_box_sizing_value_deep_copy,
            az_layout_direction_value_delete,
            az_layout_direction_value_deep_copy,
            az_layout_display_value_delete,
            az_layout_display_value_deep_copy,
            az_layout_flex_grow_value_delete,
            az_layout_flex_grow_value_deep_copy,
            az_layout_flex_shrink_value_delete,
            az_layout_flex_shrink_value_deep_copy,
            az_layout_float_value_delete,
            az_layout_float_value_deep_copy,
            az_layout_height_value_delete,
            az_layout_height_value_deep_copy,
            az_layout_justify_content_value_delete,
            az_layout_justify_content_value_deep_copy,
            az_layout_left_value_delete,
            az_layout_left_value_deep_copy,
            az_layout_margin_bottom_value_delete,
            az_layout_margin_bottom_value_deep_copy,
            az_layout_margin_left_value_delete,
            az_layout_margin_left_value_deep_copy,
            az_layout_margin_right_value_delete,
            az_layout_margin_right_value_deep_copy,
            az_layout_margin_top_value_delete,
            az_layout_margin_top_value_deep_copy,
            az_layout_max_height_value_delete,
            az_layout_max_height_value_deep_copy,
            az_layout_max_width_value_delete,
            az_layout_max_width_value_deep_copy,
            az_layout_min_height_value_delete,
            az_layout_min_height_value_deep_copy,
            az_layout_min_width_value_delete,
            az_layout_min_width_value_deep_copy,
            az_layout_padding_bottom_value_delete,
            az_layout_padding_bottom_value_deep_copy,
            az_layout_padding_left_value_delete,
            az_layout_padding_left_value_deep_copy,
            az_layout_padding_right_value_delete,
            az_layout_padding_right_value_deep_copy,
            az_layout_padding_top_value_delete,
            az_layout_padding_top_value_deep_copy,
            az_layout_position_value_delete,
            az_layout_position_value_deep_copy,
            az_layout_right_value_delete,
            az_layout_right_value_deep_copy,
            az_layout_top_value_delete,
            az_layout_top_value_deep_copy,
            az_layout_width_value_delete,
            az_layout_width_value_deep_copy,
            az_layout_wrap_value_delete,
            az_layout_wrap_value_deep_copy,
            az_overflow_value_delete,
            az_overflow_value_deep_copy,
            az_style_background_content_value_delete,
            az_style_background_content_value_deep_copy,
            az_style_background_position_value_delete,
            az_style_background_position_value_deep_copy,
            az_style_background_repeat_value_delete,
            az_style_background_repeat_value_deep_copy,
            az_style_background_size_value_delete,
            az_style_background_size_value_deep_copy,
            az_style_border_bottom_color_value_delete,
            az_style_border_bottom_color_value_deep_copy,
            az_style_border_bottom_left_radius_value_delete,
            az_style_border_bottom_left_radius_value_deep_copy,
            az_style_border_bottom_right_radius_value_delete,
            az_style_border_bottom_right_radius_value_deep_copy,
            az_style_border_bottom_style_value_delete,
            az_style_border_bottom_style_value_deep_copy,
            az_style_border_bottom_width_value_delete,
            az_style_border_bottom_width_value_deep_copy,
            az_style_border_left_color_value_delete,
            az_style_border_left_color_value_deep_copy,
            az_style_border_left_style_value_delete,
            az_style_border_left_style_value_deep_copy,
            az_style_border_left_width_value_delete,
            az_style_border_left_width_value_deep_copy,
            az_style_border_right_color_value_delete,
            az_style_border_right_color_value_deep_copy,
            az_style_border_right_style_value_delete,
            az_style_border_right_style_value_deep_copy,
            az_style_border_right_width_value_delete,
            az_style_border_right_width_value_deep_copy,
            az_style_border_top_color_value_delete,
            az_style_border_top_color_value_deep_copy,
            az_style_border_top_left_radius_value_delete,
            az_style_border_top_left_radius_value_deep_copy,
            az_style_border_top_right_radius_value_delete,
            az_style_border_top_right_radius_value_deep_copy,
            az_style_border_top_style_value_delete,
            az_style_border_top_style_value_deep_copy,
            az_style_border_top_width_value_delete,
            az_style_border_top_width_value_deep_copy,
            az_style_cursor_value_delete,
            az_style_cursor_value_deep_copy,
            az_style_font_family_value_delete,
            az_style_font_family_value_deep_copy,
            az_style_font_size_value_delete,
            az_style_font_size_value_deep_copy,
            az_style_letter_spacing_value_delete,
            az_style_letter_spacing_value_deep_copy,
            az_style_line_height_value_delete,
            az_style_line_height_value_deep_copy,
            az_style_tab_width_value_delete,
            az_style_tab_width_value_deep_copy,
            az_style_text_alignment_horz_value_delete,
            az_style_text_alignment_horz_value_deep_copy,
            az_style_text_color_value_delete,
            az_style_text_color_value_deep_copy,
            az_style_word_spacing_value_delete,
            az_style_word_spacing_value_deep_copy,
            az_css_property_delete,
            az_css_property_deep_copy,
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
            az_dom_add_child,
            az_dom_with_child,
            az_dom_has_id,
            az_dom_has_class,
            az_dom_get_html_string,
            az_dom_delete,
            az_dom_deep_copy,
            az_gl_texture_node_delete,
            az_gl_texture_node_deep_copy,
            az_i_frame_node_delete,
            az_i_frame_node_deep_copy,
            az_callback_data_delete,
            az_callback_data_deep_copy,
            az_override_property_delete,
            az_override_property_deep_copy,
            az_node_data_new,
            az_node_data_default,
            az_node_data_delete,
            az_node_data_deep_copy,
            az_node_type_delete,
            az_node_type_deep_copy,
            az_on_into_event_filter,
            az_on_delete,
            az_on_deep_copy,
            az_event_filter_delete,
            az_event_filter_deep_copy,
            az_hover_event_filter_delete,
            az_hover_event_filter_deep_copy,
            az_focus_event_filter_delete,
            az_focus_event_filter_deep_copy,
            az_not_event_filter_delete,
            az_not_event_filter_deep_copy,
            az_window_event_filter_delete,
            az_window_event_filter_deep_copy,
            az_tab_index_delete,
            az_tab_index_deep_copy,
            az_gl_type_delete,
            az_gl_type_deep_copy,
            az_debug_message_delete,
            az_debug_message_deep_copy,
            az_u8_vec_ref_delete,
            az_u8_vec_ref_mut_delete,
            az_f32_vec_ref_delete,
            az_i32_vec_ref_delete,
            az_g_luint_vec_ref_delete,
            az_g_lenum_vec_ref_delete,
            az_g_lint_vec_ref_mut_delete,
            az_g_lint64_vec_ref_mut_delete,
            az_g_lboolean_vec_ref_mut_delete,
            az_g_lfloat_vec_ref_mut_delete,
            az_refstr_vec_ref_delete,
            az_refstr_delete,
            az_get_program_binary_return_delete,
            az_get_program_binary_return_deep_copy,
            az_get_active_attrib_return_delete,
            az_get_active_attrib_return_deep_copy,
            az_g_lsync_ptr_delete,
            az_get_active_uniform_return_delete,
            az_get_active_uniform_return_deep_copy,
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
            az_texture_delete,
            az_texture_flags_delete,
            az_texture_flags_deep_copy,
            az_text_id_new,
            az_text_id_delete,
            az_text_id_deep_copy,
            az_image_id_new,
            az_image_id_delete,
            az_image_id_deep_copy,
            az_font_id_new,
            az_font_id_delete,
            az_font_id_deep_copy,
            az_image_source_delete,
            az_image_source_deep_copy,
            az_font_source_delete,
            az_font_source_deep_copy,
            az_raw_image_new,
            az_raw_image_delete,
            az_raw_image_deep_copy,
            az_raw_image_format_delete,
            az_raw_image_format_deep_copy,
            az_drop_check_ptr_delete,
            az_arc_mutex_ref_any_delete,
            az_timer_delete,
            az_timer_deep_copy,
            az_task_new,
            az_task_then,
            az_task_delete,
            az_thread_new,
            az_thread_block,
            az_thread_delete,
            az_drop_check_delete,
            az_timer_id_delete,
            az_timer_id_deep_copy,
            az_terminate_timer_delete,
            az_terminate_timer_deep_copy,
            az_block_error_delete,
            az_block_error_deep_copy,
            az_task_bar_icon_delete,
            az_task_bar_icon_deep_copy,
            az_x_window_type_delete,
            az_x_window_type_deep_copy,
            az_physical_position_i32_delete,
            az_physical_position_i32_deep_copy,
            az_logical_position_delete,
            az_logical_position_deep_copy,
            az_icon_key_delete,
            az_icon_key_deep_copy,
            az_small_window_icon_bytes_delete,
            az_small_window_icon_bytes_deep_copy,
            az_large_window_icon_bytes_delete,
            az_large_window_icon_bytes_deep_copy,
            az_window_icon_delete,
            az_window_icon_deep_copy,
            az_virtual_key_code_delete,
            az_virtual_key_code_deep_copy,
            az_accelerator_key_delete,
            az_accelerator_key_deep_copy,
            az_window_size_delete,
            az_window_size_deep_copy,
            az_window_flags_delete,
            az_window_flags_deep_copy,
            az_debug_state_delete,
            az_debug_state_deep_copy,
            az_keyboard_state_delete,
            az_keyboard_state_deep_copy,
            az_mouse_cursor_type_delete,
            az_mouse_cursor_type_deep_copy,
            az_cursor_position_delete,
            az_cursor_position_deep_copy,
            az_mouse_state_delete,
            az_mouse_state_deep_copy,
            az_platform_specific_options_delete,
            az_platform_specific_options_deep_copy,
            az_windows_window_options_delete,
            az_windows_window_options_deep_copy,
            az_wayland_theme_delete,
            az_wayland_theme_deep_copy,
            az_renderer_type_delete,
            az_renderer_type_deep_copy,
            az_string_pair_delete,
            az_string_pair_deep_copy,
            az_linux_window_options_delete,
            az_linux_window_options_deep_copy,
            az_mac_window_options_delete,
            az_mac_window_options_deep_copy,
            az_wasm_window_options_delete,
            az_wasm_window_options_deep_copy,
            az_full_screen_mode_delete,
            az_full_screen_mode_deep_copy,
            az_window_state_delete,
            az_window_state_deep_copy,
            az_logical_size_delete,
            az_logical_size_deep_copy,
            az_hot_reload_options_delete,
            az_hot_reload_options_deep_copy,
            az_window_create_options_new,
            az_window_create_options_delete,
            az_window_create_options_deep_copy,
        })
    }

    #[cfg(unix)]
    const LIB_BYTES: &[u8] = include_bytes!(concat!(env!("CARGO_HOME"), "/lib/", "azul-dll-", env!("CARGO_PKG_VERSION"), "/target/release/libazul.so")); /* !!! IF THIS LINE SHOWS AN ERROR, IT MEANS YOU FORGOT TO RUN "cargo install --version 0.1.0 azul-dll" */
    #[cfg(windows)]
    const LIB_BYTES: &[u8] = include_bytes!(concat!(env!("CARGO_HOME"), "/lib/", "azul-dll-", env!("CARGO_PKG_VERSION", "/target/release/azul.dll"))); /* !!! IF THIS LINE SHOWS AN ERROR, IT MEANS YOU FORGOT TO RUN "cargo install --version 0.1.0 azul-dll" */

    use std::{mem::MaybeUninit, sync::atomic::{AtomicBool, Ordering}};

    static LIBRARY_IS_INITIALIZED: AtomicBool = AtomicBool::new(false);
    static mut AZUL_DLL: MaybeUninit<AzulDll> = MaybeUninit::<AzulDll>::uninit();

    #[cfg(unix)]
    const DLL_FILE_NAME: &str = "./azul.so";
    #[cfg(windows)]
    const DLL_FILE_NAME: &str = "./azul.dll";

    fn load_library_inner() -> Result<AzulDll, &'static str> {

        let current_exe_path = std::env::current_exe().map_err(|_| "current exe has no current dir (?!)")?;
        let mut library_path = current_exe_path.parent().ok_or("current exe has no parent (?!)")?.to_path_buf();
        library_path.push(DLL_FILE_NAME);

        if !library_path.exists() {
           std::fs::write(&library_path, LIB_BYTES).map_err(|_| "could not unpack DLL")?;
        }

        initialize_library(&library_path)
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
}

/// Module to re-export common structs (`App`, `AppConfig`, `Css`, `Dom`, `WindowCreateOptions`, `RefAny`, `LayoutInfo`)
pub mod prelude {
    pub use crate::{
        app::{App, AppConfig},
        css::Css,
        dom::Dom,
        window::WindowCreateOptions,
        callbacks::{RefAny, LayoutInfo},
    };
}

/// Definition of azuls internal `String` wrappers
#[allow(dead_code, unused_imports)]
pub mod str {

    use crate::dll::*;
    use std::ffi::c_void;

    impl From<std::string::String> for crate::str::String {
        fn from(s: std::string::String) -> crate::str::String {
            crate::str::String::from_utf8_unchecked(s.as_ptr(), s.len()) // - copies s into a new String
            // - s is deallocated here
        }
    }

    impl From<&str> for crate::str::String {
        fn from(s: &str) -> crate::str::String {
            crate::str::String::from_utf8_unchecked(s.as_ptr(), s.len()) // - copies s into a new String
        }
    }

    impl From<crate::str::String> for std::string::String {
        fn from(s: crate::str::String) -> std::string::String {
            let s_bytes = s.into_bytes();
            unsafe { std::string::String::from_utf8_unchecked(s_bytes.into()) } // - copies s into a new String
            // - s_bytes is deallocated here
        }
    }

    impl AsRef<str> for crate::str::String {
        fn as_ref(&self) -> &str {
            self.as_str()
        }
    }

    impl std::fmt::Display for crate::str::String {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            self.as_str().fmt(f)
        }
    }

    impl std::fmt::Debug for crate::str::String {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            self.as_str().fmt(f)
        }
    }

    impl crate::str::String {
        #[inline]
        pub fn as_str(&self) -> &str {
            unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.vec.ptr, self.vec.len)) }
        }
        #[inline]
        pub fn as_bytes(&self) -> &[u8] {
            self.vec.as_ref()
        }
        #[inline]
        pub fn into_string(self) -> String {
            String::from(self)
        }
    }

    /// `String` struct
    pub use crate::dll::AzString as String;

    impl String {
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn from_utf8_unchecked(ptr: *const u8, len: usize) -> Self { (crate::dll::get_azul_dll().az_string_from_utf8_unchecked)(ptr, len) }
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn from_utf8_lossy(ptr: *const u8, len: usize) -> Self { (crate::dll::get_azul_dll().az_string_from_utf8_lossy)(ptr, len) }
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn into_bytes(self)  -> crate::vec::U8Vec { { (crate::dll::get_azul_dll().az_string_into_bytes)(self)} }
    }

    impl Clone for String { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_string_deep_copy)(self) } }
    impl Drop for String { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_delete)(self); } }
}

/// Definition of azuls internal `Vec<*>` wrappers
#[allow(dead_code, unused_imports)]
pub mod vec {

    use crate::dll::*;
    use std::ffi::c_void;
    macro_rules! impl_vec {($struct_type:ident, $struct_name:ident) => (

        impl $struct_name {

            pub fn new() -> Self {
                Vec::<$struct_type>::new().into()
            }

            pub fn sort_by<F: FnMut(&$struct_type, &$struct_type) -> std::cmp::Ordering>(&mut self, compare: F) {
                let v1: &mut [$struct_type] = unsafe { std::slice::from_raw_parts_mut(self.ptr as *mut $struct_type, self.len) };
                v1.sort_by(compare);
            }

            pub fn with_capacity(cap: usize) -> Self {
                Vec::<$struct_type>::with_capacity(cap).into()
            }

            pub fn push(&mut self, val: $struct_type) {
                let mut v: Vec<$struct_type> = unsafe { Vec::from_raw_parts(self.ptr as *mut $struct_type, self.len, self.cap) };
                v.push(val);
                let (ptr, len, cap) = Self::into_raw_parts(v);
                self.ptr = ptr;
                self.len = len;
                self.cap = cap;
            }

            pub fn iter(&self) -> std::slice::Iter<$struct_type> {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                v1.iter()
            }

            pub fn iter_mut(&mut self) -> std::slice::IterMut<$struct_type> {
                let v1: &mut [$struct_type] = unsafe { std::slice::from_raw_parts_mut(self.ptr as *mut $struct_type, self.len) };
                v1.iter_mut()
            }

            pub fn into_iter(self) -> std::vec::IntoIter<$struct_type> {
                let v1: Vec<$struct_type> = unsafe { std::vec::Vec::from_raw_parts(self.ptr as *mut $struct_type, self.len, self.cap) };
                std::mem::forget(self); // do not run destructor of self
                v1.into_iter()
            }

            pub fn as_ptr(&self) -> *const $struct_type {
                self.ptr as *const $struct_type
            }

            pub fn len(&self) -> usize {
                self.len
            }

            pub fn is_empty(&self) -> bool {
                self.len == 0
            }

            pub fn cap(&self) -> usize {
                self.cap
            }

            pub fn get(&self, index: usize) -> Option<&$struct_type> {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                let res = v1.get(index);
                std::mem::forget(v1);
                res
            }

            pub fn foreach<U, F: FnMut(&$struct_type) -> Result<(), U>>(&self, mut closure: F) -> Result<(), U> {
                let v1: &[$struct_type] = unsafe { std::slice::from_raw_parts(self.ptr, self.len) };
                for i in v1.iter() { closure(i)?; }
                std::mem::forget(v1);
                Ok(())
            }

            /// Same as Vec::into_raw_parts(self), prevents destructor from running
            fn into_raw_parts(mut v: Vec<$struct_type>) -> (*mut $struct_type, usize, usize) {
                let ptr = v.as_mut_ptr();
                let len = v.len();
                let cap = v.capacity();
                std::mem::forget(v);
                (ptr, len, cap)
            }
        }

        impl Default for $struct_name {
            fn default() -> Self {
                Vec::<$struct_type>::default().into()
            }
        }

        impl std::iter::FromIterator<$struct_type> for $struct_name {
            fn from_iter<T>(iter: T) -> Self where T: IntoIterator<Item = $struct_type> {
                let v: Vec<$struct_type> = Vec::from_iter(iter);
                v.into()
            }
        }

        impl AsRef<[$struct_type]> for $struct_name {
            fn as_ref(&self) -> &[$struct_type] {
                unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
            }
        }

        impl From<Vec<$struct_type>> for $struct_name {
            fn from(v: Vec<$struct_type>) -> $struct_name {
                $struct_name::copy_from(v.as_ptr(), v.len())
            }
        }

        impl From<$struct_name> for Vec<$struct_type> {
            fn from(v: $struct_name) -> Vec<$struct_type> {
                unsafe { std::slice::from_raw_parts(v.as_ptr(), v.len()) }.to_vec()
            }
        }
    )}

    impl_vec!(u8, U8Vec);
    impl_vec!(CallbackData, CallbackDataVec);
    impl_vec!(OverrideProperty, OverridePropertyVec);
    impl_vec!(Dom, DomVec);
    impl_vec!(AzString, StringVec);
    impl_vec!(GradientStopPre, GradientStopPreVec);
    impl_vec!(DebugMessage, DebugMessageVec);

    impl From<std::vec::Vec<std::string::String>> for crate::vec::StringVec {
        fn from(v: std::vec::Vec<std::string::String>) -> crate::vec::StringVec {
            let vec: Vec<AzString> = v.into_iter().map(|i| {
                let i: std::vec::Vec<u8> = i.into_bytes();
                (crate::dll::get_azul_dll().az_string_from_utf8_unchecked)(i.as_ptr(), i.len())
            }).collect();

            (crate::dll::get_azul_dll().az_string_vec_copy_from)(vec.as_ptr(), vec.len())
        }
    }

    impl From<crate::vec::StringVec> for std::vec::Vec<std::string::String> {
        fn from(v: crate::vec::StringVec) -> std::vec::Vec<std::string::String> {
            unsafe { std::slice::from_raw_parts(v.ptr, v.len) }
            .iter()
            .map(|s| unsafe {
                let s: AzString = (crate::dll::get_azul_dll().az_string_deep_copy)(s);
                let s_vec: std::vec::Vec<u8> = s.into_bytes().into();
                std::string::String::from_utf8_unchecked(s_vec)
            })
            .collect()

            // delete() not necessary because StringVec is stack-allocated
        }
    }    use crate::window::{StringPair, VirtualKeyCode, XWindowType};
    use crate::css::{CssDeclaration, CssPathSelector, CssRuleBlock, GradientStopPre, Stylesheet};
    use crate::dom::{CallbackData, Dom, OverrideProperty};
    use crate::gl::DebugMessage;
    use crate::str::String;


    /// Wrapper over a Rust-allocated `XWindowType`
    pub use crate::dll::AzXWindowTypeVec as XWindowTypeVec;

    impl XWindowTypeVec {
        /// Creates + allocates a Rust `Vec<XWindowType>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzXWindowType, len: usize) -> Self { (crate::dll::get_azul_dll().az_x_window_type_vec_copy_from)(ptr, len) }
    }

    impl Clone for XWindowTypeVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_x_window_type_vec_deep_copy)(self) } }
    impl Drop for XWindowTypeVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_x_window_type_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `VirtualKeyCode`
    pub use crate::dll::AzVirtualKeyCodeVec as VirtualKeyCodeVec;

    impl VirtualKeyCodeVec {
        /// Creates + allocates a Rust `Vec<VirtualKeyCode>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzVirtualKeyCode, len: usize) -> Self { (crate::dll::get_azul_dll().az_virtual_key_code_vec_copy_from)(ptr, len) }
    }

    impl Clone for VirtualKeyCodeVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_virtual_key_code_vec_deep_copy)(self) } }
    impl Drop for VirtualKeyCodeVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_virtual_key_code_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `ScanCode`
    pub use crate::dll::AzScanCodeVec as ScanCodeVec;

    impl ScanCodeVec {
        /// Creates + allocates a Rust `Vec<ScanCode>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u32, len: usize) -> Self { (crate::dll::get_azul_dll().az_scan_code_vec_copy_from)(ptr, len) }
    }

    impl Clone for ScanCodeVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_scan_code_vec_deep_copy)(self) } }
    impl Drop for ScanCodeVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_scan_code_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CssDeclaration`
    pub use crate::dll::AzCssDeclarationVec as CssDeclarationVec;

    impl CssDeclarationVec {
        /// Creates + allocates a Rust `Vec<CssDeclaration>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzCssDeclaration, len: usize) -> Self { (crate::dll::get_azul_dll().az_css_declaration_vec_copy_from)(ptr, len) }
    }

    impl Clone for CssDeclarationVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_declaration_vec_deep_copy)(self) } }
    impl Drop for CssDeclarationVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_declaration_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CssPathSelector`
    pub use crate::dll::AzCssPathSelectorVec as CssPathSelectorVec;

    impl CssPathSelectorVec {
        /// Creates + allocates a Rust `Vec<CssPathSelector>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzCssPathSelector, len: usize) -> Self { (crate::dll::get_azul_dll().az_css_path_selector_vec_copy_from)(ptr, len) }
    }

    impl Clone for CssPathSelectorVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_path_selector_vec_deep_copy)(self) } }
    impl Drop for CssPathSelectorVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_path_selector_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `Stylesheet`
    pub use crate::dll::AzStylesheetVec as StylesheetVec;

    impl StylesheetVec {
        /// Creates + allocates a Rust `Vec<Stylesheet>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzStylesheet, len: usize) -> Self { (crate::dll::get_azul_dll().az_stylesheet_vec_copy_from)(ptr, len) }
    }

    impl Clone for StylesheetVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_stylesheet_vec_deep_copy)(self) } }
    impl Drop for StylesheetVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_stylesheet_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CssRuleBlock`
    pub use crate::dll::AzCssRuleBlockVec as CssRuleBlockVec;

    impl CssRuleBlockVec {
        /// Creates + allocates a Rust `Vec<CssRuleBlock>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzCssRuleBlock, len: usize) -> Self { (crate::dll::get_azul_dll().az_css_rule_block_vec_copy_from)(ptr, len) }
    }

    impl Clone for CssRuleBlockVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_rule_block_vec_deep_copy)(self) } }
    impl Drop for CssRuleBlockVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_rule_block_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `U8Vec`
    pub use crate::dll::AzU8Vec as U8Vec;

    impl U8Vec {
        /// Creates + allocates a Rust `Vec<u8>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u8, len: usize) -> Self { (crate::dll::get_azul_dll().az_u8_vec_copy_from)(ptr, len) }
    }

    impl Clone for U8Vec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_u8_vec_deep_copy)(self) } }
    impl Drop for U8Vec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_u8_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `CallbackData`
    pub use crate::dll::AzCallbackDataVec as CallbackDataVec;

    impl CallbackDataVec {
        /// Creates + allocates a Rust `Vec<CallbackData>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzCallbackData, len: usize) -> Self { (crate::dll::get_azul_dll().az_callback_data_vec_copy_from)(ptr, len) }
    }

    impl Clone for CallbackDataVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_callback_data_vec_deep_copy)(self) } }
    impl Drop for CallbackDataVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_callback_data_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `Vec<DebugMessage>`
    pub use crate::dll::AzDebugMessageVec as DebugMessageVec;

    impl DebugMessageVec {
        /// Creates + allocates a Rust `Vec<DebugMessage>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzDebugMessage, len: usize) -> Self { (crate::dll::get_azul_dll().az_debug_message_vec_copy_from)(ptr, len) }
    }

    impl Clone for DebugMessageVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_debug_message_vec_deep_copy)(self) } }
    impl Drop for DebugMessageVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_debug_message_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `U32Vec`
    pub use crate::dll::AzGLuintVec as GLuintVec;

    impl GLuintVec {
        /// Creates + allocates a Rust `Vec<u32>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u32, len: usize) -> Self { (crate::dll::get_azul_dll().az_g_luint_vec_copy_from)(ptr, len) }
    }

    impl Clone for GLuintVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_g_luint_vec_deep_copy)(self) } }
    impl Drop for GLuintVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_luint_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `GLintVec`
    pub use crate::dll::AzGLintVec as GLintVec;

    impl GLintVec {
        /// Creates + allocates a Rust `Vec<u32>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const i32, len: usize) -> Self { (crate::dll::get_azul_dll().az_g_lint_vec_copy_from)(ptr, len) }
    }

    impl Clone for GLintVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_g_lint_vec_deep_copy)(self) } }
    impl Drop for GLintVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lint_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `OverridePropertyVec`
    pub use crate::dll::AzOverridePropertyVec as OverridePropertyVec;

    impl OverridePropertyVec {
        /// Creates + allocates a Rust `Vec<OverrideProperty>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzOverrideProperty, len: usize) -> Self { (crate::dll::get_azul_dll().az_override_property_vec_copy_from)(ptr, len) }
    }

    impl Clone for OverridePropertyVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_override_property_vec_deep_copy)(self) } }
    impl Drop for OverridePropertyVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_override_property_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `DomVec`
    pub use crate::dll::AzDomVec as DomVec;

    impl DomVec {
        /// Creates + allocates a Rust `Vec<Dom>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzDom, len: usize) -> Self { (crate::dll::get_azul_dll().az_dom_vec_copy_from)(ptr, len) }
    }

    impl Clone for DomVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_dom_vec_deep_copy)(self) } }
    impl Drop for DomVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_dom_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `StringVec`
    pub use crate::dll::AzStringVec as StringVec;

    impl StringVec {
        /// Creates + allocates a Rust `Vec<String>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzString, len: usize) -> Self { (crate::dll::get_azul_dll().az_string_vec_copy_from)(ptr, len) }
    }

    impl Clone for StringVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_string_vec_deep_copy)(self) } }
    impl Drop for StringVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `StringPairVec`
    pub use crate::dll::AzStringPairVec as StringPairVec;

    impl StringPairVec {
        /// Creates + allocates a Rust `Vec<StringPair>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzStringPair, len: usize) -> Self { (crate::dll::get_azul_dll().az_string_pair_vec_copy_from)(ptr, len) }
    }

    impl Clone for StringPairVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_string_pair_vec_deep_copy)(self) } }
    impl Drop for StringPairVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_pair_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `GradientStopPreVec`
    pub use crate::dll::AzGradientStopPreVec as GradientStopPreVec;

    impl GradientStopPreVec {
        /// Creates + allocates a Rust `Vec<GradientStopPre>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzGradientStopPre, len: usize) -> Self { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_copy_from)(ptr, len) }
    }

    impl Clone for GradientStopPreVec { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_deep_copy)(self) } }
    impl Drop for GradientStopPreVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_delete)(self); } }
}

/// Definition of azuls internal `Option<*>` wrappers
#[allow(dead_code, unused_imports)]
pub mod option {

    use crate::dll::*;
    use std::ffi::c_void;


    /// `OptionWaylandTheme` struct
    pub use crate::dll::AzOptionWaylandTheme as OptionWaylandTheme;

    impl Clone for OptionWaylandTheme { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_wayland_theme_deep_copy)(self) } }
    impl Drop for OptionWaylandTheme { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_wayland_theme_delete)(self); } }


    /// `OptionTaskBarIcon` struct
    pub use crate::dll::AzOptionTaskBarIcon as OptionTaskBarIcon;

    impl Clone for OptionTaskBarIcon { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_task_bar_icon_deep_copy)(self) } }
    impl Drop for OptionTaskBarIcon { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_task_bar_icon_delete)(self); } }


    /// `OptionHwndHandle` struct
    pub use crate::dll::AzOptionHwndHandle as OptionHwndHandle;

    impl Clone for OptionHwndHandle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_hwnd_handle_deep_copy)(self) } }
    impl Drop for OptionHwndHandle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_hwnd_handle_delete)(self); } }


    /// `OptionLogicalPosition` struct
    pub use crate::dll::AzOptionLogicalPosition as OptionLogicalPosition;

    impl Clone for OptionLogicalPosition { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_logical_position_deep_copy)(self) } }
    impl Drop for OptionLogicalPosition { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_logical_position_delete)(self); } }


    /// `OptionHotReloadOptions` struct
    pub use crate::dll::AzOptionHotReloadOptions as OptionHotReloadOptions;

    impl Clone for OptionHotReloadOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_hot_reload_options_deep_copy)(self) } }
    impl Drop for OptionHotReloadOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_hot_reload_options_delete)(self); } }


    /// `OptionPhysicalPositionI32` struct
    pub use crate::dll::AzOptionPhysicalPositionI32 as OptionPhysicalPositionI32;

    impl Clone for OptionPhysicalPositionI32 { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_physical_position_i32_deep_copy)(self) } }
    impl Drop for OptionPhysicalPositionI32 { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_physical_position_i32_delete)(self); } }


    /// `OptionWindowIcon` struct
    pub use crate::dll::AzOptionWindowIcon as OptionWindowIcon;

    impl Clone for OptionWindowIcon { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_window_icon_deep_copy)(self) } }
    impl Drop for OptionWindowIcon { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_window_icon_delete)(self); } }


    /// `OptionString` struct
    pub use crate::dll::AzOptionString as OptionString;

    impl Clone for OptionString { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_string_deep_copy)(self) } }
    impl Drop for OptionString { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_string_delete)(self); } }


    /// `OptionX11Visual` struct
    pub use crate::dll::AzOptionX11Visual as OptionX11Visual;

    impl Clone for OptionX11Visual { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_x11_visual_deep_copy)(self) } }
    impl Drop for OptionX11Visual { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_x11_visual_delete)(self); } }


    /// `OptionI32` struct
    pub use crate::dll::AzOptionI32 as OptionI32;

    impl Clone for OptionI32 { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_i32_deep_copy)(self) } }
    impl Drop for OptionI32 { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_i32_delete)(self); } }


    /// `OptionF32` struct
    pub use crate::dll::AzOptionF32 as OptionF32;

    impl Clone for OptionF32 { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_f32_deep_copy)(self) } }
    impl Drop for OptionF32 { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_f32_delete)(self); } }


    /// `OptionMouseCursorType` struct
    pub use crate::dll::AzOptionMouseCursorType as OptionMouseCursorType;

    impl Clone for OptionMouseCursorType { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_mouse_cursor_type_deep_copy)(self) } }
    impl Drop for OptionMouseCursorType { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_mouse_cursor_type_delete)(self); } }


    /// `OptionLogicalSize` struct
    pub use crate::dll::AzOptionLogicalSize as OptionLogicalSize;

    impl Clone for OptionLogicalSize { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_logical_size_deep_copy)(self) } }
    impl Drop for OptionLogicalSize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_logical_size_delete)(self); } }


    /// `OptionChar` struct
    pub use crate::dll::AzOptionChar as OptionChar;

    impl Clone for OptionChar { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_char_deep_copy)(self) } }
    impl Drop for OptionChar { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_char_delete)(self); } }


    /// `OptionVirtualKeyCode` struct
    pub use crate::dll::AzOptionVirtualKeyCode as OptionVirtualKeyCode;

    impl Clone for OptionVirtualKeyCode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_virtual_key_code_deep_copy)(self) } }
    impl Drop for OptionVirtualKeyCode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_virtual_key_code_delete)(self); } }


    /// `OptionPercentageValue` struct
    pub use crate::dll::AzOptionPercentageValue as OptionPercentageValue;

    impl Clone for OptionPercentageValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_percentage_value_deep_copy)(self) } }
    impl Drop for OptionPercentageValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_percentage_value_delete)(self); } }


    /// `OptionDom` struct
    pub use crate::dll::AzOptionDom as OptionDom;

    impl Clone for OptionDom { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_dom_deep_copy)(self) } }
    impl Drop for OptionDom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_dom_delete)(self); } }


    /// `OptionTexture` struct
    pub use crate::dll::AzOptionTexture as OptionTexture;

    impl Drop for OptionTexture { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_texture_delete)(self); } }


    /// `OptionTabIndex` struct
    pub use crate::dll::AzOptionTabIndex as OptionTabIndex;

    impl Clone for OptionTabIndex { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_tab_index_deep_copy)(self) } }
    impl Drop for OptionTabIndex { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_tab_index_delete)(self); } }


    /// `OptionDuration` struct
    pub use crate::dll::AzOptionDuration as OptionDuration;

    impl Clone for OptionDuration { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_duration_deep_copy)(self) } }
    impl Drop for OptionDuration { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_duration_delete)(self); } }


    /// `OptionInstantPtr` struct
    pub use crate::dll::AzOptionInstantPtr as OptionInstantPtr;

    impl Clone for OptionInstantPtr { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_instant_ptr_deep_copy)(self) } }
    impl Drop for OptionInstantPtr { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_instant_ptr_delete)(self); } }


    /// `OptionUsize` struct
    pub use crate::dll::AzOptionUsize as OptionUsize;

    impl Clone for OptionUsize { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_usize_deep_copy)(self) } }
    impl Drop for OptionUsize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_usize_delete)(self); } }


    /// `OptionU8VecRef` struct
    pub use crate::dll::AzOptionU8VecRef as OptionU8VecRef;

    impl Drop for OptionU8VecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_u8_vec_ref_delete)(self); } }
}

/// Definition of azuls internal `Option<*>` wrappers
#[allow(dead_code, unused_imports)]
pub mod result {

    use crate::dll::*;
    use std::ffi::c_void;


    /// `ResultRefAnyBlockError` struct
    pub use crate::dll::AzResultRefAnyBlockError as ResultRefAnyBlockError;

    impl Clone for ResultRefAnyBlockError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_result_ref_any_block_error_deep_copy)(self) } }
    impl Drop for ResultRefAnyBlockError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_result_ref_any_block_error_delete)(self); } }
}

/// Rust wrappers for `Instant` / `Duration` classes
#[allow(dead_code, unused_imports)]
pub mod time {

    use crate::dll::*;
    use std::ffi::c_void;


    /// `Instant` struct
    pub use crate::dll::AzInstantPtr as Instant;

    impl Instant {
        /// Creates a new `Instant` instance.
        pub fn now() -> Self { (crate::dll::get_azul_dll().az_instant_now)() }
    }

    impl Drop for Instant { fn drop(&mut self) { (crate::dll::get_azul_dll().az_instant_delete)(self); } }


    /// `Duration` struct
    pub use crate::dll::AzDuration as Duration;

    impl Clone for Duration { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_duration_deep_copy)(self) } }
    impl Drop for Duration { fn drop(&mut self) { (crate::dll::get_azul_dll().az_duration_delete)(self); } }
}

/// `App` construction and configuration
#[allow(dead_code, unused_imports)]
pub mod app {

    use crate::dll::*;
    use std::ffi::c_void;
    use crate::callbacks::{LayoutCallbackType, RefAny};
    use crate::window::WindowCreateOptions;


    /// `AppConfig` struct
    pub use crate::dll::AzAppConfigPtr as AppConfig;

    impl AppConfig {
        /// Creates a new AppConfig with default values
        pub fn default() -> Self { (crate::dll::get_azul_dll().az_app_config_default)() }
    }

    impl Drop for AppConfig { fn drop(&mut self) { (crate::dll::get_azul_dll().az_app_config_delete)(self); } }


    /// `App` struct
    pub use crate::dll::AzAppPtr as App;

    impl App {
        /// Creates a new App instance from the given `AppConfig`
        pub fn new(data: RefAny, config: AppConfig, callback: LayoutCallbackType) -> Self { (crate::dll::get_azul_dll().az_app_new)(data, config, callback) }
        /// Runs the application. Due to platform restrictions (specifically `WinMain` on Windows), this function never returns.
        pub fn run(self, window: WindowCreateOptions)  { (crate::dll::get_azul_dll().az_app_run)(self, window) }
    }

    impl Drop for App { fn drop(&mut self) { (crate::dll::get_azul_dll().az_app_delete)(self); } }
}

/// Callback type definitions + struct definitions of `CallbackInfo`s
#[allow(dead_code, unused_imports)]
pub mod callbacks {

    use crate::dll::*;
    use std::ffi::c_void;

    #[derive(Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
    #[repr(C)]
    pub struct Ref<'a, T> {
        ptr: &'a T,
        _sharing_info_ptr: *const RefAnySharingInfo,
    }

    impl<'a, T> Drop for Ref<'a, T> {
        fn drop(&mut self) {
            (crate::dll::get_azul_dll().az_ref_any_sharing_info_decrease_ref)(unsafe { &mut *(self._sharing_info_ptr as *mut RefAnySharingInfo) });
        }
    }

    impl<'a, T> std::ops::Deref for Ref<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            self.ptr
        }
    }

    #[derive(Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
    #[repr(C)]
    pub struct RefMut<'a, T> {
        ptr: &'a mut T,
        _sharing_info_ptr: *const RefAnySharingInfo,
    }

    impl<'a, T> Drop for RefMut<'a, T> {
        fn drop(&mut self) {
            (crate::dll::get_azul_dll().az_ref_any_sharing_info_decrease_refmut)(unsafe { &mut *(self._sharing_info_ptr as *mut RefAnySharingInfo) });
        }
    }

    impl<'a, T> std::ops::Deref for RefMut<'a, T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &*self.ptr
        }
    }

    impl<'a, T> std::ops::DerefMut for RefMut<'a, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.ptr
        }
    }

    impl RefAny {

        /// Creates a new, type-erased pointer by casting the `T` value into a `Vec<u8>` and saving the length + type ID
        pub fn new<T: 'static>(value: T) -> Self {
            use crate::dll::*;

            fn default_custom_destructor<U: 'static>(ptr: *const c_void) {
                use std::{mem, ptr};

                // note: in the default constructor, we do not need to check whether U == T

                unsafe {
                    // copy the struct from the heap to the stack and call mem::drop on U to run the destructor
                    let mut stack_mem = mem::MaybeUninit::<U>::uninit().assume_init();
                    ptr::copy_nonoverlapping(ptr as *const U, &mut stack_mem as *mut U, mem::size_of::<U>());
                    mem::drop(stack_mem);
                }
            }

            let type_name_str = ::std::any::type_name::<T>();
            let s = (crate::dll::get_azul_dll().az_ref_any_new_c)(
                (&value as *const T) as *const c_void,
                ::std::mem::size_of::<T>(),
                Self::get_type_id::<T>(),
                crate::str::String::from_utf8_unchecked(type_name_str.as_ptr(), type_name_str.len()),
                default_custom_destructor::<T>,
            );
            ::std::mem::forget(value); // do not run the destructor of T here!
            s
        }

        /// Downcasts the type-erased pointer to a type `&U`, returns `None` if the types don't match
        #[inline]
        pub fn borrow<'a, U: 'static>(&'a self) -> Option<Ref<'a, U>> {
            let is_same_type = (crate::dll::get_azul_dll().az_ref_any_is_type)(self, Self::get_type_id::<U>());
            if !is_same_type { return None; }

            let can_be_shared = (crate::dll::get_azul_dll().az_ref_any_can_be_shared)(self);
            if !can_be_shared { return None; }

            Some(Ref {
                ptr: unsafe { &*(self._internal_ptr as *const U) },
                _sharing_info_ptr: self._sharing_info_ptr,
            })
        }

        /// Downcasts the type-erased pointer to a type `&mut U`, returns `None` if the types don't match
        #[inline]
        pub fn borrow_mut<'a, U: 'static>(&'a mut self) -> Option<RefMut<'a, U>> {
            let is_same_type = (crate::dll::get_azul_dll().az_ref_any_is_type)(self, Self::get_type_id::<U>());
            if !is_same_type { return None; }

            let can_be_shared_mut = (crate::dll::get_azul_dll().az_ref_any_can_be_shared_mut)(self);
            if !can_be_shared_mut { return None; }

            Some(RefMut {
                ptr: unsafe { &mut *(self._internal_ptr as *mut U) },
                _sharing_info_ptr: self._sharing_info_ptr,
            })
        }

        // Returns the typeid of `T` as a u64 (necessary because `std::any::TypeId` is not C-ABI compatible)
        #[inline]
        pub fn get_type_id<T: 'static>() -> u64 {
            use std::any::TypeId;
            use std::mem;

            // fast method to serialize the type id into a u64
            let t_id = TypeId::of::<T>();
            let struct_as_bytes = unsafe { ::std::slice::from_raw_parts((&t_id as *const TypeId) as *const u8, mem::size_of::<TypeId>()) };
            struct_as_bytes.into_iter().enumerate().map(|(s_pos, s)| ((*s as u64) << s_pos)).sum()
        }
    }    use crate::str::String;


    /// `LayoutCallback` struct
    pub use crate::dll::AzLayoutCallback as LayoutCallback;

    impl Clone for LayoutCallback { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_callback_deep_copy)(self) } }
    impl Drop for LayoutCallback { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_callback_delete)(self); } }


    pub use crate::dll::AzLayoutCallbackType as LayoutCallbackType;

    /// `Callback` struct
    pub use crate::dll::AzCallback as Callback;

    impl Clone for Callback { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_callback_deep_copy)(self) } }
    impl Drop for Callback { fn drop(&mut self) { (crate::dll::get_azul_dll().az_callback_delete)(self); } }


    pub use crate::dll::AzCallbackReturn as CallbackReturn;
    pub use crate::dll::AzCallbackType as CallbackType;

    /// `CallbackInfo` struct
    pub use crate::dll::AzCallbackInfoPtr as CallbackInfo;

    impl Drop for CallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_callback_info_delete)(self); } }


    /// Specifies if the screen should be updated after the callback function has returned
    pub use crate::dll::AzUpdateScreen as UpdateScreen;

    impl Clone for UpdateScreen { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_update_screen_deep_copy)(self) } }
    impl Drop for UpdateScreen { fn drop(&mut self) { (crate::dll::get_azul_dll().az_update_screen_delete)(self); } }


    /// `IFrameCallback` struct
    pub use crate::dll::AzIFrameCallback as IFrameCallback;

    impl Clone for IFrameCallback { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_i_frame_callback_deep_copy)(self) } }
    impl Drop for IFrameCallback { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i_frame_callback_delete)(self); } }


    pub use crate::dll::AzIFrameCallbackType as IFrameCallbackType;

    /// `IFrameCallbackInfo` struct
    pub use crate::dll::AzIFrameCallbackInfoPtr as IFrameCallbackInfo;

    impl IFrameCallbackInfo {
        /// Returns a copy of the internal `RefAny`
        pub fn get_state(&self)  -> crate::callbacks::RefAny { { (crate::dll::get_azul_dll().az_i_frame_callback_info_get_state)(self)} }
    }

    impl Drop for IFrameCallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i_frame_callback_info_delete)(self); } }


    /// `IFrameCallbackReturn` struct
    pub use crate::dll::AzIFrameCallbackReturn as IFrameCallbackReturn;

    impl Clone for IFrameCallbackReturn { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_i_frame_callback_return_deep_copy)(self) } }
    impl Drop for IFrameCallbackReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i_frame_callback_return_delete)(self); } }


    /// `GlCallback` struct
    pub use crate::dll::AzGlCallback as GlCallback;

    impl Clone for GlCallback { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_gl_callback_deep_copy)(self) } }
    impl Drop for GlCallback { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_callback_delete)(self); } }


    pub use crate::dll::AzGlCallbackType as GlCallbackType;

    /// `GlCallbackInfo` struct
    pub use crate::dll::AzGlCallbackInfoPtr as GlCallbackInfo;

    impl Drop for GlCallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_callback_info_delete)(self); } }


    /// `GlCallbackReturn` struct
    pub use crate::dll::AzGlCallbackReturn as GlCallbackReturn;

    impl Drop for GlCallbackReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_callback_return_delete)(self); } }


    /// `TimerCallback` struct
    pub use crate::dll::AzTimerCallback as TimerCallback;

    impl Clone for TimerCallback { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_timer_callback_deep_copy)(self) } }
    impl Drop for TimerCallback { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_callback_delete)(self); } }


    /// `TimerCallbackType` struct
    pub use crate::dll::AzTimerCallbackTypePtr as TimerCallbackType;



    /// `TimerCallbackInfo` struct
    pub use crate::dll::AzTimerCallbackInfoPtr as TimerCallbackInfo;

    impl Drop for TimerCallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_callback_info_delete)(self); } }


    /// `TimerCallbackReturn` struct
    pub use crate::dll::AzTimerCallbackReturn as TimerCallbackReturn;

    impl Clone for TimerCallbackReturn { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_timer_callback_return_deep_copy)(self) } }
    impl Drop for TimerCallbackReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_callback_return_delete)(self); } }


    pub use crate::dll::AzThreadCallbackType as ThreadCallbackType;

    pub use crate::dll::AzTaskCallbackType as TaskCallbackType;

    pub use crate::dll::AzRefAnyDestructorType as RefAnyDestructorType;

    /// `RefAnySharingInfo` struct
    pub use crate::dll::AzRefAnySharingInfo as RefAnySharingInfo;

    impl RefAnySharingInfo {
        /// Calls the `RefAnySharingInfo::can_be_shared` function.
        pub fn can_be_shared(&self)  -> bool { (crate::dll::get_azul_dll().az_ref_any_sharing_info_can_be_shared)(self) }
        /// Calls the `RefAnySharingInfo::can_be_shared_mut` function.
        pub fn can_be_shared_mut(&self)  -> bool { (crate::dll::get_azul_dll().az_ref_any_sharing_info_can_be_shared_mut)(self) }
        /// Calls the `RefAnySharingInfo::increase_ref` function.
        pub fn increase_ref(&mut self)  { (crate::dll::get_azul_dll().az_ref_any_sharing_info_increase_ref)(self) }
        /// Calls the `RefAnySharingInfo::decrease_ref` function.
        pub fn decrease_ref(&mut self)  { (crate::dll::get_azul_dll().az_ref_any_sharing_info_decrease_ref)(self) }
        /// Calls the `RefAnySharingInfo::increase_refmut` function.
        pub fn increase_refmut(&mut self)  { (crate::dll::get_azul_dll().az_ref_any_sharing_info_increase_refmut)(self) }
        /// Calls the `RefAnySharingInfo::decrease_refmut` function.
        pub fn decrease_refmut(&mut self)  { (crate::dll::get_azul_dll().az_ref_any_sharing_info_decrease_refmut)(self) }
    }

    impl Drop for RefAnySharingInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_ref_any_sharing_info_delete)(self); } }


    /// RefAny is a reference-counted, type-erased pointer, which stores a reference to a struct. `RefAny` can be up- and downcasted (this usually done via generics and can't be expressed in the Rust API)
    pub use crate::dll::AzRefAny as RefAny;

    impl RefAny {
        /// Creates a new `RefAny` instance.
        pub fn new_c(ptr: *const c_void, len: usize, type_id: u64, type_name: String, destructor: RefAnyDestructorType) -> Self { (crate::dll::get_azul_dll().az_ref_any_new_c)(ptr, len, type_id, type_name, destructor) }
        /// Calls the `RefAny::is_type` function.
        pub fn is_type(&self, type_id: u64)  -> bool { (crate::dll::get_azul_dll().az_ref_any_is_type)(self, type_id) }
        /// Calls the `RefAny::get_type_name` function.
        pub fn get_type_name(&self)  -> crate::str::String { { (crate::dll::get_azul_dll().az_ref_any_get_type_name)(self)} }
        /// Calls the `RefAny::can_be_shared` function.
        pub fn can_be_shared(&self)  -> bool { (crate::dll::get_azul_dll().az_ref_any_can_be_shared)(self) }
        /// Calls the `RefAny::can_be_shared_mut` function.
        pub fn can_be_shared_mut(&self)  -> bool { (crate::dll::get_azul_dll().az_ref_any_can_be_shared_mut)(self) }
        /// Calls the `RefAny::increase_ref` function.
        pub fn increase_ref(&self)  { (crate::dll::get_azul_dll().az_ref_any_increase_ref)(self) }
        /// Calls the `RefAny::decrease_ref` function.
        pub fn decrease_ref(&self)  { (crate::dll::get_azul_dll().az_ref_any_decrease_ref)(self) }
        /// Calls the `RefAny::increase_refmut` function.
        pub fn increase_refmut(&self)  { (crate::dll::get_azul_dll().az_ref_any_increase_refmut)(self) }
        /// Calls the `RefAny::decrease_refmut` function.
        pub fn decrease_refmut(&self)  { (crate::dll::get_azul_dll().az_ref_any_decrease_refmut)(self) }
    }

    impl Clone for RefAny { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_ref_any_deep_copy)(self) } }
    impl Drop for RefAny { fn drop(&mut self) { (crate::dll::get_azul_dll().az_ref_any_delete)(self); } }


    /// `LayoutInfo` struct
    pub use crate::dll::AzLayoutInfoPtr as LayoutInfo;

    impl Drop for LayoutInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_info_delete)(self); } }
}

/// `Css` parsing module
#[allow(dead_code, unused_imports)]
pub mod css {

    use crate::dll::*;
    use std::ffi::c_void;
    use crate::str::String;


    /// `CssRuleBlock` struct
    pub use crate::dll::AzCssRuleBlock as CssRuleBlock;

    impl Clone for CssRuleBlock { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_rule_block_deep_copy)(self) } }
    impl Drop for CssRuleBlock { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_rule_block_delete)(self); } }


    /// `CssDeclaration` struct
    pub use crate::dll::AzCssDeclaration as CssDeclaration;

    impl Clone for CssDeclaration { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_declaration_deep_copy)(self) } }
    impl Drop for CssDeclaration { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_declaration_delete)(self); } }


    /// `DynamicCssProperty` struct
    pub use crate::dll::AzDynamicCssProperty as DynamicCssProperty;

    impl Clone for DynamicCssProperty { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_dynamic_css_property_deep_copy)(self) } }
    impl Drop for DynamicCssProperty { fn drop(&mut self) { (crate::dll::get_azul_dll().az_dynamic_css_property_delete)(self); } }


    /// `CssPath` struct
    pub use crate::dll::AzCssPath as CssPath;

    impl Clone for CssPath { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_path_deep_copy)(self) } }
    impl Drop for CssPath { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_path_delete)(self); } }


    /// `CssPathSelector` struct
    pub use crate::dll::AzCssPathSelector as CssPathSelector;

    impl Clone for CssPathSelector { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_path_selector_deep_copy)(self) } }
    impl Drop for CssPathSelector { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_path_selector_delete)(self); } }


    /// `NodeTypePath` struct
    pub use crate::dll::AzNodeTypePath as NodeTypePath;

    impl Clone for NodeTypePath { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_node_type_path_deep_copy)(self) } }
    impl Drop for NodeTypePath { fn drop(&mut self) { (crate::dll::get_azul_dll().az_node_type_path_delete)(self); } }


    /// `CssPathPseudoSelector` struct
    pub use crate::dll::AzCssPathPseudoSelector as CssPathPseudoSelector;

    impl Clone for CssPathPseudoSelector { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_path_pseudo_selector_deep_copy)(self) } }
    impl Drop for CssPathPseudoSelector { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_path_pseudo_selector_delete)(self); } }


    /// `CssNthChildSelector` struct
    pub use crate::dll::AzCssNthChildSelector as CssNthChildSelector;

    impl Clone for CssNthChildSelector { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_nth_child_selector_deep_copy)(self) } }
    impl Drop for CssNthChildSelector { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_nth_child_selector_delete)(self); } }


    /// `CssNthChildPattern` struct
    pub use crate::dll::AzCssNthChildPattern as CssNthChildPattern;

    impl Clone for CssNthChildPattern { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_nth_child_pattern_deep_copy)(self) } }
    impl Drop for CssNthChildPattern { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_nth_child_pattern_delete)(self); } }


    /// `Stylesheet` struct
    pub use crate::dll::AzStylesheet as Stylesheet;

    impl Clone for Stylesheet { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_stylesheet_deep_copy)(self) } }
    impl Drop for Stylesheet { fn drop(&mut self) { (crate::dll::get_azul_dll().az_stylesheet_delete)(self); } }


    /// `Css` struct
    pub use crate::dll::AzCss as Css;

    impl Css {
        /// Loads the native style for the given operating system
        pub fn native() -> Self { (crate::dll::get_azul_dll().az_css_native)() }
        /// Returns an empty CSS style
        pub fn empty() -> Self { (crate::dll::get_azul_dll().az_css_empty)() }
        /// Returns a CSS style parsed from a `String`
        pub fn from_string(s: String) -> Self { (crate::dll::get_azul_dll().az_css_from_string)(s) }
        /// Appends a parsed stylesheet to `Css::native()`
        pub fn override_native(s: String) -> Self { (crate::dll::get_azul_dll().az_css_override_native)(s) }
    }

    impl Clone for Css { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_deep_copy)(self) } }
    impl Drop for Css { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_delete)(self); } }


    /// `ColorU` struct
    pub use crate::dll::AzColorU as ColorU;

    impl Clone for ColorU { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_color_u_deep_copy)(self) } }
    impl Drop for ColorU { fn drop(&mut self) { (crate::dll::get_azul_dll().az_color_u_delete)(self); } }


    /// `SizeMetric` struct
    pub use crate::dll::AzSizeMetric as SizeMetric;

    impl Clone for SizeMetric { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_size_metric_deep_copy)(self) } }
    impl Drop for SizeMetric { fn drop(&mut self) { (crate::dll::get_azul_dll().az_size_metric_delete)(self); } }


    /// `FloatValue` struct
    pub use crate::dll::AzFloatValue as FloatValue;

    impl Clone for FloatValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_float_value_deep_copy)(self) } }
    impl Drop for FloatValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_float_value_delete)(self); } }


    /// `PixelValue` struct
    pub use crate::dll::AzPixelValue as PixelValue;

    impl Clone for PixelValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_pixel_value_deep_copy)(self) } }
    impl Drop for PixelValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_pixel_value_delete)(self); } }


    /// `PixelValueNoPercent` struct
    pub use crate::dll::AzPixelValueNoPercent as PixelValueNoPercent;

    impl Clone for PixelValueNoPercent { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_pixel_value_no_percent_deep_copy)(self) } }
    impl Drop for PixelValueNoPercent { fn drop(&mut self) { (crate::dll::get_azul_dll().az_pixel_value_no_percent_delete)(self); } }


    /// `BoxShadowClipMode` struct
    pub use crate::dll::AzBoxShadowClipMode as BoxShadowClipMode;

    impl Clone for BoxShadowClipMode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_box_shadow_clip_mode_deep_copy)(self) } }
    impl Drop for BoxShadowClipMode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_box_shadow_clip_mode_delete)(self); } }


    /// `BoxShadowPreDisplayItem` struct
    pub use crate::dll::AzBoxShadowPreDisplayItem as BoxShadowPreDisplayItem;

    impl Clone for BoxShadowPreDisplayItem { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_box_shadow_pre_display_item_deep_copy)(self) } }
    impl Drop for BoxShadowPreDisplayItem { fn drop(&mut self) { (crate::dll::get_azul_dll().az_box_shadow_pre_display_item_delete)(self); } }


    /// `LayoutAlignContent` struct
    pub use crate::dll::AzLayoutAlignContent as LayoutAlignContent;

    impl Clone for LayoutAlignContent { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_align_content_deep_copy)(self) } }
    impl Drop for LayoutAlignContent { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_align_content_delete)(self); } }


    /// `LayoutAlignItems` struct
    pub use crate::dll::AzLayoutAlignItems as LayoutAlignItems;

    impl Clone for LayoutAlignItems { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_align_items_deep_copy)(self) } }
    impl Drop for LayoutAlignItems { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_align_items_delete)(self); } }


    /// `LayoutBottom` struct
    pub use crate::dll::AzLayoutBottom as LayoutBottom;

    impl Clone for LayoutBottom { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_bottom_deep_copy)(self) } }
    impl Drop for LayoutBottom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_bottom_delete)(self); } }


    /// `LayoutBoxSizing` struct
    pub use crate::dll::AzLayoutBoxSizing as LayoutBoxSizing;

    impl Clone for LayoutBoxSizing { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_box_sizing_deep_copy)(self) } }
    impl Drop for LayoutBoxSizing { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_box_sizing_delete)(self); } }


    /// `LayoutDirection` struct
    pub use crate::dll::AzLayoutDirection as LayoutDirection;

    impl Clone for LayoutDirection { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_direction_deep_copy)(self) } }
    impl Drop for LayoutDirection { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_direction_delete)(self); } }


    /// `LayoutDisplay` struct
    pub use crate::dll::AzLayoutDisplay as LayoutDisplay;

    impl Clone for LayoutDisplay { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_display_deep_copy)(self) } }
    impl Drop for LayoutDisplay { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_display_delete)(self); } }


    /// `LayoutFlexGrow` struct
    pub use crate::dll::AzLayoutFlexGrow as LayoutFlexGrow;

    impl Clone for LayoutFlexGrow { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_flex_grow_deep_copy)(self) } }
    impl Drop for LayoutFlexGrow { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_flex_grow_delete)(self); } }


    /// `LayoutFlexShrink` struct
    pub use crate::dll::AzLayoutFlexShrink as LayoutFlexShrink;

    impl Clone for LayoutFlexShrink { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_flex_shrink_deep_copy)(self) } }
    impl Drop for LayoutFlexShrink { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_flex_shrink_delete)(self); } }


    /// `LayoutFloat` struct
    pub use crate::dll::AzLayoutFloat as LayoutFloat;

    impl Clone for LayoutFloat { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_float_deep_copy)(self) } }
    impl Drop for LayoutFloat { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_float_delete)(self); } }


    /// `LayoutHeight` struct
    pub use crate::dll::AzLayoutHeight as LayoutHeight;

    impl Clone for LayoutHeight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_height_deep_copy)(self) } }
    impl Drop for LayoutHeight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_height_delete)(self); } }


    /// `LayoutJustifyContent` struct
    pub use crate::dll::AzLayoutJustifyContent as LayoutJustifyContent;

    impl Clone for LayoutJustifyContent { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_justify_content_deep_copy)(self) } }
    impl Drop for LayoutJustifyContent { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_justify_content_delete)(self); } }


    /// `LayoutLeft` struct
    pub use crate::dll::AzLayoutLeft as LayoutLeft;

    impl Clone for LayoutLeft { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_left_deep_copy)(self) } }
    impl Drop for LayoutLeft { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_left_delete)(self); } }


    /// `LayoutMarginBottom` struct
    pub use crate::dll::AzLayoutMarginBottom as LayoutMarginBottom;

    impl Clone for LayoutMarginBottom { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_bottom_deep_copy)(self) } }
    impl Drop for LayoutMarginBottom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_bottom_delete)(self); } }


    /// `LayoutMarginLeft` struct
    pub use crate::dll::AzLayoutMarginLeft as LayoutMarginLeft;

    impl Clone for LayoutMarginLeft { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_left_deep_copy)(self) } }
    impl Drop for LayoutMarginLeft { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_left_delete)(self); } }


    /// `LayoutMarginRight` struct
    pub use crate::dll::AzLayoutMarginRight as LayoutMarginRight;

    impl Clone for LayoutMarginRight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_right_deep_copy)(self) } }
    impl Drop for LayoutMarginRight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_right_delete)(self); } }


    /// `LayoutMarginTop` struct
    pub use crate::dll::AzLayoutMarginTop as LayoutMarginTop;

    impl Clone for LayoutMarginTop { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_top_deep_copy)(self) } }
    impl Drop for LayoutMarginTop { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_top_delete)(self); } }


    /// `LayoutMaxHeight` struct
    pub use crate::dll::AzLayoutMaxHeight as LayoutMaxHeight;

    impl Clone for LayoutMaxHeight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_max_height_deep_copy)(self) } }
    impl Drop for LayoutMaxHeight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_max_height_delete)(self); } }


    /// `LayoutMaxWidth` struct
    pub use crate::dll::AzLayoutMaxWidth as LayoutMaxWidth;

    impl Clone for LayoutMaxWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_max_width_deep_copy)(self) } }
    impl Drop for LayoutMaxWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_max_width_delete)(self); } }


    /// `LayoutMinHeight` struct
    pub use crate::dll::AzLayoutMinHeight as LayoutMinHeight;

    impl Clone for LayoutMinHeight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_min_height_deep_copy)(self) } }
    impl Drop for LayoutMinHeight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_min_height_delete)(self); } }


    /// `LayoutMinWidth` struct
    pub use crate::dll::AzLayoutMinWidth as LayoutMinWidth;

    impl Clone for LayoutMinWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_min_width_deep_copy)(self) } }
    impl Drop for LayoutMinWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_min_width_delete)(self); } }


    /// `LayoutPaddingBottom` struct
    pub use crate::dll::AzLayoutPaddingBottom as LayoutPaddingBottom;

    impl Clone for LayoutPaddingBottom { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_bottom_deep_copy)(self) } }
    impl Drop for LayoutPaddingBottom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_bottom_delete)(self); } }


    /// `LayoutPaddingLeft` struct
    pub use crate::dll::AzLayoutPaddingLeft as LayoutPaddingLeft;

    impl Clone for LayoutPaddingLeft { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_left_deep_copy)(self) } }
    impl Drop for LayoutPaddingLeft { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_left_delete)(self); } }


    /// `LayoutPaddingRight` struct
    pub use crate::dll::AzLayoutPaddingRight as LayoutPaddingRight;

    impl Clone for LayoutPaddingRight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_right_deep_copy)(self) } }
    impl Drop for LayoutPaddingRight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_right_delete)(self); } }


    /// `LayoutPaddingTop` struct
    pub use crate::dll::AzLayoutPaddingTop as LayoutPaddingTop;

    impl Clone for LayoutPaddingTop { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_top_deep_copy)(self) } }
    impl Drop for LayoutPaddingTop { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_top_delete)(self); } }


    /// `LayoutPosition` struct
    pub use crate::dll::AzLayoutPosition as LayoutPosition;

    impl Clone for LayoutPosition { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_position_deep_copy)(self) } }
    impl Drop for LayoutPosition { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_position_delete)(self); } }


    /// `LayoutRight` struct
    pub use crate::dll::AzLayoutRight as LayoutRight;

    impl Clone for LayoutRight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_right_deep_copy)(self) } }
    impl Drop for LayoutRight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_right_delete)(self); } }


    /// `LayoutTop` struct
    pub use crate::dll::AzLayoutTop as LayoutTop;

    impl Clone for LayoutTop { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_top_deep_copy)(self) } }
    impl Drop for LayoutTop { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_top_delete)(self); } }


    /// `LayoutWidth` struct
    pub use crate::dll::AzLayoutWidth as LayoutWidth;

    impl Clone for LayoutWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_width_deep_copy)(self) } }
    impl Drop for LayoutWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_width_delete)(self); } }


    /// `LayoutWrap` struct
    pub use crate::dll::AzLayoutWrap as LayoutWrap;

    impl Clone for LayoutWrap { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_wrap_deep_copy)(self) } }
    impl Drop for LayoutWrap { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_wrap_delete)(self); } }


    /// `Overflow` struct
    pub use crate::dll::AzOverflow as Overflow;

    impl Clone for Overflow { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_overflow_deep_copy)(self) } }
    impl Drop for Overflow { fn drop(&mut self) { (crate::dll::get_azul_dll().az_overflow_delete)(self); } }


    /// `PercentageValue` struct
    pub use crate::dll::AzPercentageValue as PercentageValue;

    impl Clone for PercentageValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_percentage_value_deep_copy)(self) } }
    impl Drop for PercentageValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_percentage_value_delete)(self); } }


    /// `GradientStopPre` struct
    pub use crate::dll::AzGradientStopPre as GradientStopPre;

    impl Clone for GradientStopPre { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_gradient_stop_pre_deep_copy)(self) } }
    impl Drop for GradientStopPre { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gradient_stop_pre_delete)(self); } }


    /// `DirectionCorner` struct
    pub use crate::dll::AzDirectionCorner as DirectionCorner;

    impl Clone for DirectionCorner { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_direction_corner_deep_copy)(self) } }
    impl Drop for DirectionCorner { fn drop(&mut self) { (crate::dll::get_azul_dll().az_direction_corner_delete)(self); } }


    /// `DirectionCorners` struct
    pub use crate::dll::AzDirectionCorners as DirectionCorners;

    impl Clone for DirectionCorners { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_direction_corners_deep_copy)(self) } }
    impl Drop for DirectionCorners { fn drop(&mut self) { (crate::dll::get_azul_dll().az_direction_corners_delete)(self); } }


    /// `Direction` struct
    pub use crate::dll::AzDirection as Direction;

    impl Clone for Direction { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_direction_deep_copy)(self) } }
    impl Drop for Direction { fn drop(&mut self) { (crate::dll::get_azul_dll().az_direction_delete)(self); } }


    /// `ExtendMode` struct
    pub use crate::dll::AzExtendMode as ExtendMode;

    impl Clone for ExtendMode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_extend_mode_deep_copy)(self) } }
    impl Drop for ExtendMode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_extend_mode_delete)(self); } }


    /// `LinearGradient` struct
    pub use crate::dll::AzLinearGradient as LinearGradient;

    impl Clone for LinearGradient { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_linear_gradient_deep_copy)(self) } }
    impl Drop for LinearGradient { fn drop(&mut self) { (crate::dll::get_azul_dll().az_linear_gradient_delete)(self); } }


    /// `Shape` struct
    pub use crate::dll::AzShape as Shape;

    impl Clone for Shape { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_shape_deep_copy)(self) } }
    impl Drop for Shape { fn drop(&mut self) { (crate::dll::get_azul_dll().az_shape_delete)(self); } }


    /// `RadialGradient` struct
    pub use crate::dll::AzRadialGradient as RadialGradient;

    impl Clone for RadialGradient { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_radial_gradient_deep_copy)(self) } }
    impl Drop for RadialGradient { fn drop(&mut self) { (crate::dll::get_azul_dll().az_radial_gradient_delete)(self); } }


    /// `CssImageId` struct
    pub use crate::dll::AzCssImageId as CssImageId;

    impl Clone for CssImageId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_image_id_deep_copy)(self) } }
    impl Drop for CssImageId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_image_id_delete)(self); } }


    /// `StyleBackgroundContent` struct
    pub use crate::dll::AzStyleBackgroundContent as StyleBackgroundContent;

    impl Clone for StyleBackgroundContent { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_content_deep_copy)(self) } }
    impl Drop for StyleBackgroundContent { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_content_delete)(self); } }


    /// `BackgroundPositionHorizontal` struct
    pub use crate::dll::AzBackgroundPositionHorizontal as BackgroundPositionHorizontal;

    impl Clone for BackgroundPositionHorizontal { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_background_position_horizontal_deep_copy)(self) } }
    impl Drop for BackgroundPositionHorizontal { fn drop(&mut self) { (crate::dll::get_azul_dll().az_background_position_horizontal_delete)(self); } }


    /// `BackgroundPositionVertical` struct
    pub use crate::dll::AzBackgroundPositionVertical as BackgroundPositionVertical;

    impl Clone for BackgroundPositionVertical { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_background_position_vertical_deep_copy)(self) } }
    impl Drop for BackgroundPositionVertical { fn drop(&mut self) { (crate::dll::get_azul_dll().az_background_position_vertical_delete)(self); } }


    /// `StyleBackgroundPosition` struct
    pub use crate::dll::AzStyleBackgroundPosition as StyleBackgroundPosition;

    impl Clone for StyleBackgroundPosition { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_position_deep_copy)(self) } }
    impl Drop for StyleBackgroundPosition { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_position_delete)(self); } }


    /// `StyleBackgroundRepeat` struct
    pub use crate::dll::AzStyleBackgroundRepeat as StyleBackgroundRepeat;

    impl Clone for StyleBackgroundRepeat { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_repeat_deep_copy)(self) } }
    impl Drop for StyleBackgroundRepeat { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_repeat_delete)(self); } }


    /// `StyleBackgroundSize` struct
    pub use crate::dll::AzStyleBackgroundSize as StyleBackgroundSize;

    impl Clone for StyleBackgroundSize { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_size_deep_copy)(self) } }
    impl Drop for StyleBackgroundSize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_size_delete)(self); } }


    /// `StyleBorderBottomColor` struct
    pub use crate::dll::AzStyleBorderBottomColor as StyleBorderBottomColor;

    impl Clone for StyleBorderBottomColor { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_color_deep_copy)(self) } }
    impl Drop for StyleBorderBottomColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_color_delete)(self); } }


    /// `StyleBorderBottomLeftRadius` struct
    pub use crate::dll::AzStyleBorderBottomLeftRadius as StyleBorderBottomLeftRadius;

    impl Clone for StyleBorderBottomLeftRadius { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_left_radius_deep_copy)(self) } }
    impl Drop for StyleBorderBottomLeftRadius { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_left_radius_delete)(self); } }


    /// `StyleBorderBottomRightRadius` struct
    pub use crate::dll::AzStyleBorderBottomRightRadius as StyleBorderBottomRightRadius;

    impl Clone for StyleBorderBottomRightRadius { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_right_radius_deep_copy)(self) } }
    impl Drop for StyleBorderBottomRightRadius { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_right_radius_delete)(self); } }


    /// `BorderStyle` struct
    pub use crate::dll::AzBorderStyle as BorderStyle;

    impl Clone for BorderStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_border_style_deep_copy)(self) } }
    impl Drop for BorderStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_border_style_delete)(self); } }


    /// `StyleBorderBottomStyle` struct
    pub use crate::dll::AzStyleBorderBottomStyle as StyleBorderBottomStyle;

    impl Clone for StyleBorderBottomStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_style_deep_copy)(self) } }
    impl Drop for StyleBorderBottomStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_style_delete)(self); } }


    /// `StyleBorderBottomWidth` struct
    pub use crate::dll::AzStyleBorderBottomWidth as StyleBorderBottomWidth;

    impl Clone for StyleBorderBottomWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_width_deep_copy)(self) } }
    impl Drop for StyleBorderBottomWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_width_delete)(self); } }


    /// `StyleBorderLeftColor` struct
    pub use crate::dll::AzStyleBorderLeftColor as StyleBorderLeftColor;

    impl Clone for StyleBorderLeftColor { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_left_color_deep_copy)(self) } }
    impl Drop for StyleBorderLeftColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_color_delete)(self); } }


    /// `StyleBorderLeftStyle` struct
    pub use crate::dll::AzStyleBorderLeftStyle as StyleBorderLeftStyle;

    impl Clone for StyleBorderLeftStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_left_style_deep_copy)(self) } }
    impl Drop for StyleBorderLeftStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_style_delete)(self); } }


    /// `StyleBorderLeftWidth` struct
    pub use crate::dll::AzStyleBorderLeftWidth as StyleBorderLeftWidth;

    impl Clone for StyleBorderLeftWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_left_width_deep_copy)(self) } }
    impl Drop for StyleBorderLeftWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_width_delete)(self); } }


    /// `StyleBorderRightColor` struct
    pub use crate::dll::AzStyleBorderRightColor as StyleBorderRightColor;

    impl Clone for StyleBorderRightColor { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_right_color_deep_copy)(self) } }
    impl Drop for StyleBorderRightColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_color_delete)(self); } }


    /// `StyleBorderRightStyle` struct
    pub use crate::dll::AzStyleBorderRightStyle as StyleBorderRightStyle;

    impl Clone for StyleBorderRightStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_right_style_deep_copy)(self) } }
    impl Drop for StyleBorderRightStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_style_delete)(self); } }


    /// `StyleBorderRightWidth` struct
    pub use crate::dll::AzStyleBorderRightWidth as StyleBorderRightWidth;

    impl Clone for StyleBorderRightWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_right_width_deep_copy)(self) } }
    impl Drop for StyleBorderRightWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_width_delete)(self); } }


    /// `StyleBorderTopColor` struct
    pub use crate::dll::AzStyleBorderTopColor as StyleBorderTopColor;

    impl Clone for StyleBorderTopColor { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_color_deep_copy)(self) } }
    impl Drop for StyleBorderTopColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_color_delete)(self); } }


    /// `StyleBorderTopLeftRadius` struct
    pub use crate::dll::AzStyleBorderTopLeftRadius as StyleBorderTopLeftRadius;

    impl Clone for StyleBorderTopLeftRadius { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_left_radius_deep_copy)(self) } }
    impl Drop for StyleBorderTopLeftRadius { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_left_radius_delete)(self); } }


    /// `StyleBorderTopRightRadius` struct
    pub use crate::dll::AzStyleBorderTopRightRadius as StyleBorderTopRightRadius;

    impl Clone for StyleBorderTopRightRadius { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_right_radius_deep_copy)(self) } }
    impl Drop for StyleBorderTopRightRadius { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_right_radius_delete)(self); } }


    /// `StyleBorderTopStyle` struct
    pub use crate::dll::AzStyleBorderTopStyle as StyleBorderTopStyle;

    impl Clone for StyleBorderTopStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_style_deep_copy)(self) } }
    impl Drop for StyleBorderTopStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_style_delete)(self); } }


    /// `StyleBorderTopWidth` struct
    pub use crate::dll::AzStyleBorderTopWidth as StyleBorderTopWidth;

    impl Clone for StyleBorderTopWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_width_deep_copy)(self) } }
    impl Drop for StyleBorderTopWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_width_delete)(self); } }


    /// `StyleCursor` struct
    pub use crate::dll::AzStyleCursor as StyleCursor;

    impl Clone for StyleCursor { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_cursor_deep_copy)(self) } }
    impl Drop for StyleCursor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_cursor_delete)(self); } }


    /// `StyleFontFamily` struct
    pub use crate::dll::AzStyleFontFamily as StyleFontFamily;

    impl Clone for StyleFontFamily { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_font_family_deep_copy)(self) } }
    impl Drop for StyleFontFamily { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_family_delete)(self); } }


    /// `StyleFontSize` struct
    pub use crate::dll::AzStyleFontSize as StyleFontSize;

    impl Clone for StyleFontSize { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_font_size_deep_copy)(self) } }
    impl Drop for StyleFontSize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_size_delete)(self); } }


    /// `StyleLetterSpacing` struct
    pub use crate::dll::AzStyleLetterSpacing as StyleLetterSpacing;

    impl Clone for StyleLetterSpacing { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_letter_spacing_deep_copy)(self) } }
    impl Drop for StyleLetterSpacing { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_letter_spacing_delete)(self); } }


    /// `StyleLineHeight` struct
    pub use crate::dll::AzStyleLineHeight as StyleLineHeight;

    impl Clone for StyleLineHeight { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_line_height_deep_copy)(self) } }
    impl Drop for StyleLineHeight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_line_height_delete)(self); } }


    /// `StyleTabWidth` struct
    pub use crate::dll::AzStyleTabWidth as StyleTabWidth;

    impl Clone for StyleTabWidth { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_tab_width_deep_copy)(self) } }
    impl Drop for StyleTabWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_tab_width_delete)(self); } }


    /// `StyleTextAlignmentHorz` struct
    pub use crate::dll::AzStyleTextAlignmentHorz as StyleTextAlignmentHorz;

    impl Clone for StyleTextAlignmentHorz { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_text_alignment_horz_deep_copy)(self) } }
    impl Drop for StyleTextAlignmentHorz { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_text_alignment_horz_delete)(self); } }


    /// `StyleTextColor` struct
    pub use crate::dll::AzStyleTextColor as StyleTextColor;

    impl Clone for StyleTextColor { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_text_color_deep_copy)(self) } }
    impl Drop for StyleTextColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_text_color_delete)(self); } }


    /// `StyleWordSpacing` struct
    pub use crate::dll::AzStyleWordSpacing as StyleWordSpacing;

    impl Clone for StyleWordSpacing { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_word_spacing_deep_copy)(self) } }
    impl Drop for StyleWordSpacing { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_word_spacing_delete)(self); } }


    /// `BoxShadowPreDisplayItemValue` struct
    pub use crate::dll::AzBoxShadowPreDisplayItemValue as BoxShadowPreDisplayItemValue;

    impl Clone for BoxShadowPreDisplayItemValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_box_shadow_pre_display_item_value_deep_copy)(self) } }
    impl Drop for BoxShadowPreDisplayItemValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_box_shadow_pre_display_item_value_delete)(self); } }


    /// `LayoutAlignContentValue` struct
    pub use crate::dll::AzLayoutAlignContentValue as LayoutAlignContentValue;

    impl Clone for LayoutAlignContentValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_align_content_value_deep_copy)(self) } }
    impl Drop for LayoutAlignContentValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_align_content_value_delete)(self); } }


    /// `LayoutAlignItemsValue` struct
    pub use crate::dll::AzLayoutAlignItemsValue as LayoutAlignItemsValue;

    impl Clone for LayoutAlignItemsValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_align_items_value_deep_copy)(self) } }
    impl Drop for LayoutAlignItemsValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_align_items_value_delete)(self); } }


    /// `LayoutBottomValue` struct
    pub use crate::dll::AzLayoutBottomValue as LayoutBottomValue;

    impl Clone for LayoutBottomValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_bottom_value_deep_copy)(self) } }
    impl Drop for LayoutBottomValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_bottom_value_delete)(self); } }


    /// `LayoutBoxSizingValue` struct
    pub use crate::dll::AzLayoutBoxSizingValue as LayoutBoxSizingValue;

    impl Clone for LayoutBoxSizingValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_box_sizing_value_deep_copy)(self) } }
    impl Drop for LayoutBoxSizingValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_box_sizing_value_delete)(self); } }


    /// `LayoutDirectionValue` struct
    pub use crate::dll::AzLayoutDirectionValue as LayoutDirectionValue;

    impl Clone for LayoutDirectionValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_direction_value_deep_copy)(self) } }
    impl Drop for LayoutDirectionValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_direction_value_delete)(self); } }


    /// `LayoutDisplayValue` struct
    pub use crate::dll::AzLayoutDisplayValue as LayoutDisplayValue;

    impl Clone for LayoutDisplayValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_display_value_deep_copy)(self) } }
    impl Drop for LayoutDisplayValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_display_value_delete)(self); } }


    /// `LayoutFlexGrowValue` struct
    pub use crate::dll::AzLayoutFlexGrowValue as LayoutFlexGrowValue;

    impl Clone for LayoutFlexGrowValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_flex_grow_value_deep_copy)(self) } }
    impl Drop for LayoutFlexGrowValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_flex_grow_value_delete)(self); } }


    /// `LayoutFlexShrinkValue` struct
    pub use crate::dll::AzLayoutFlexShrinkValue as LayoutFlexShrinkValue;

    impl Clone for LayoutFlexShrinkValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_flex_shrink_value_deep_copy)(self) } }
    impl Drop for LayoutFlexShrinkValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_flex_shrink_value_delete)(self); } }


    /// `LayoutFloatValue` struct
    pub use crate::dll::AzLayoutFloatValue as LayoutFloatValue;

    impl Clone for LayoutFloatValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_float_value_deep_copy)(self) } }
    impl Drop for LayoutFloatValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_float_value_delete)(self); } }


    /// `LayoutHeightValue` struct
    pub use crate::dll::AzLayoutHeightValue as LayoutHeightValue;

    impl Clone for LayoutHeightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_height_value_deep_copy)(self) } }
    impl Drop for LayoutHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_height_value_delete)(self); } }


    /// `LayoutJustifyContentValue` struct
    pub use crate::dll::AzLayoutJustifyContentValue as LayoutJustifyContentValue;

    impl Clone for LayoutJustifyContentValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_justify_content_value_deep_copy)(self) } }
    impl Drop for LayoutJustifyContentValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_justify_content_value_delete)(self); } }


    /// `LayoutLeftValue` struct
    pub use crate::dll::AzLayoutLeftValue as LayoutLeftValue;

    impl Clone for LayoutLeftValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_left_value_deep_copy)(self) } }
    impl Drop for LayoutLeftValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_left_value_delete)(self); } }


    /// `LayoutMarginBottomValue` struct
    pub use crate::dll::AzLayoutMarginBottomValue as LayoutMarginBottomValue;

    impl Clone for LayoutMarginBottomValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_bottom_value_deep_copy)(self) } }
    impl Drop for LayoutMarginBottomValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_bottom_value_delete)(self); } }


    /// `LayoutMarginLeftValue` struct
    pub use crate::dll::AzLayoutMarginLeftValue as LayoutMarginLeftValue;

    impl Clone for LayoutMarginLeftValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_left_value_deep_copy)(self) } }
    impl Drop for LayoutMarginLeftValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_left_value_delete)(self); } }


    /// `LayoutMarginRightValue` struct
    pub use crate::dll::AzLayoutMarginRightValue as LayoutMarginRightValue;

    impl Clone for LayoutMarginRightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_right_value_deep_copy)(self) } }
    impl Drop for LayoutMarginRightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_right_value_delete)(self); } }


    /// `LayoutMarginTopValue` struct
    pub use crate::dll::AzLayoutMarginTopValue as LayoutMarginTopValue;

    impl Clone for LayoutMarginTopValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_margin_top_value_deep_copy)(self) } }
    impl Drop for LayoutMarginTopValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_top_value_delete)(self); } }


    /// `LayoutMaxHeightValue` struct
    pub use crate::dll::AzLayoutMaxHeightValue as LayoutMaxHeightValue;

    impl Clone for LayoutMaxHeightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_max_height_value_deep_copy)(self) } }
    impl Drop for LayoutMaxHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_max_height_value_delete)(self); } }


    /// `LayoutMaxWidthValue` struct
    pub use crate::dll::AzLayoutMaxWidthValue as LayoutMaxWidthValue;

    impl Clone for LayoutMaxWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_max_width_value_deep_copy)(self) } }
    impl Drop for LayoutMaxWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_max_width_value_delete)(self); } }


    /// `LayoutMinHeightValue` struct
    pub use crate::dll::AzLayoutMinHeightValue as LayoutMinHeightValue;

    impl Clone for LayoutMinHeightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_min_height_value_deep_copy)(self) } }
    impl Drop for LayoutMinHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_min_height_value_delete)(self); } }


    /// `LayoutMinWidthValue` struct
    pub use crate::dll::AzLayoutMinWidthValue as LayoutMinWidthValue;

    impl Clone for LayoutMinWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_min_width_value_deep_copy)(self) } }
    impl Drop for LayoutMinWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_min_width_value_delete)(self); } }


    /// `LayoutPaddingBottomValue` struct
    pub use crate::dll::AzLayoutPaddingBottomValue as LayoutPaddingBottomValue;

    impl Clone for LayoutPaddingBottomValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_bottom_value_deep_copy)(self) } }
    impl Drop for LayoutPaddingBottomValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_bottom_value_delete)(self); } }


    /// `LayoutPaddingLeftValue` struct
    pub use crate::dll::AzLayoutPaddingLeftValue as LayoutPaddingLeftValue;

    impl Clone for LayoutPaddingLeftValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_left_value_deep_copy)(self) } }
    impl Drop for LayoutPaddingLeftValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_left_value_delete)(self); } }


    /// `LayoutPaddingRightValue` struct
    pub use crate::dll::AzLayoutPaddingRightValue as LayoutPaddingRightValue;

    impl Clone for LayoutPaddingRightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_right_value_deep_copy)(self) } }
    impl Drop for LayoutPaddingRightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_right_value_delete)(self); } }


    /// `LayoutPaddingTopValue` struct
    pub use crate::dll::AzLayoutPaddingTopValue as LayoutPaddingTopValue;

    impl Clone for LayoutPaddingTopValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_padding_top_value_deep_copy)(self) } }
    impl Drop for LayoutPaddingTopValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_top_value_delete)(self); } }


    /// `LayoutPositionValue` struct
    pub use crate::dll::AzLayoutPositionValue as LayoutPositionValue;

    impl Clone for LayoutPositionValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_position_value_deep_copy)(self) } }
    impl Drop for LayoutPositionValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_position_value_delete)(self); } }


    /// `LayoutRightValue` struct
    pub use crate::dll::AzLayoutRightValue as LayoutRightValue;

    impl Clone for LayoutRightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_right_value_deep_copy)(self) } }
    impl Drop for LayoutRightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_right_value_delete)(self); } }


    /// `LayoutTopValue` struct
    pub use crate::dll::AzLayoutTopValue as LayoutTopValue;

    impl Clone for LayoutTopValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_top_value_deep_copy)(self) } }
    impl Drop for LayoutTopValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_top_value_delete)(self); } }


    /// `LayoutWidthValue` struct
    pub use crate::dll::AzLayoutWidthValue as LayoutWidthValue;

    impl Clone for LayoutWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_width_value_deep_copy)(self) } }
    impl Drop for LayoutWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_width_value_delete)(self); } }


    /// `LayoutWrapValue` struct
    pub use crate::dll::AzLayoutWrapValue as LayoutWrapValue;

    impl Clone for LayoutWrapValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_layout_wrap_value_deep_copy)(self) } }
    impl Drop for LayoutWrapValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_wrap_value_delete)(self); } }


    /// `OverflowValue` struct
    pub use crate::dll::AzOverflowValue as OverflowValue;

    impl Clone for OverflowValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_overflow_value_deep_copy)(self) } }
    impl Drop for OverflowValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_overflow_value_delete)(self); } }


    /// `StyleBackgroundContentValue` struct
    pub use crate::dll::AzStyleBackgroundContentValue as StyleBackgroundContentValue;

    impl Clone for StyleBackgroundContentValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_content_value_deep_copy)(self) } }
    impl Drop for StyleBackgroundContentValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_content_value_delete)(self); } }


    /// `StyleBackgroundPositionValue` struct
    pub use crate::dll::AzStyleBackgroundPositionValue as StyleBackgroundPositionValue;

    impl Clone for StyleBackgroundPositionValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_position_value_deep_copy)(self) } }
    impl Drop for StyleBackgroundPositionValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_position_value_delete)(self); } }


    /// `StyleBackgroundRepeatValue` struct
    pub use crate::dll::AzStyleBackgroundRepeatValue as StyleBackgroundRepeatValue;

    impl Clone for StyleBackgroundRepeatValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_repeat_value_deep_copy)(self) } }
    impl Drop for StyleBackgroundRepeatValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_repeat_value_delete)(self); } }


    /// `StyleBackgroundSizeValue` struct
    pub use crate::dll::AzStyleBackgroundSizeValue as StyleBackgroundSizeValue;

    impl Clone for StyleBackgroundSizeValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_size_value_deep_copy)(self) } }
    impl Drop for StyleBackgroundSizeValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_size_value_delete)(self); } }


    /// `StyleBorderBottomColorValue` struct
    pub use crate::dll::AzStyleBorderBottomColorValue as StyleBorderBottomColorValue;

    impl Clone for StyleBorderBottomColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_color_value_deep_copy)(self) } }
    impl Drop for StyleBorderBottomColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_color_value_delete)(self); } }


    /// `StyleBorderBottomLeftRadiusValue` struct
    pub use crate::dll::AzStyleBorderBottomLeftRadiusValue as StyleBorderBottomLeftRadiusValue;

    impl Clone for StyleBorderBottomLeftRadiusValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_left_radius_value_deep_copy)(self) } }
    impl Drop for StyleBorderBottomLeftRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_left_radius_value_delete)(self); } }


    /// `StyleBorderBottomRightRadiusValue` struct
    pub use crate::dll::AzStyleBorderBottomRightRadiusValue as StyleBorderBottomRightRadiusValue;

    impl Clone for StyleBorderBottomRightRadiusValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_right_radius_value_deep_copy)(self) } }
    impl Drop for StyleBorderBottomRightRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_right_radius_value_delete)(self); } }


    /// `StyleBorderBottomStyleValue` struct
    pub use crate::dll::AzStyleBorderBottomStyleValue as StyleBorderBottomStyleValue;

    impl Clone for StyleBorderBottomStyleValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_style_value_deep_copy)(self) } }
    impl Drop for StyleBorderBottomStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_style_value_delete)(self); } }


    /// `StyleBorderBottomWidthValue` struct
    pub use crate::dll::AzStyleBorderBottomWidthValue as StyleBorderBottomWidthValue;

    impl Clone for StyleBorderBottomWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_bottom_width_value_deep_copy)(self) } }
    impl Drop for StyleBorderBottomWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_width_value_delete)(self); } }


    /// `StyleBorderLeftColorValue` struct
    pub use crate::dll::AzStyleBorderLeftColorValue as StyleBorderLeftColorValue;

    impl Clone for StyleBorderLeftColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_left_color_value_deep_copy)(self) } }
    impl Drop for StyleBorderLeftColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_color_value_delete)(self); } }


    /// `StyleBorderLeftStyleValue` struct
    pub use crate::dll::AzStyleBorderLeftStyleValue as StyleBorderLeftStyleValue;

    impl Clone for StyleBorderLeftStyleValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_left_style_value_deep_copy)(self) } }
    impl Drop for StyleBorderLeftStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_style_value_delete)(self); } }


    /// `StyleBorderLeftWidthValue` struct
    pub use crate::dll::AzStyleBorderLeftWidthValue as StyleBorderLeftWidthValue;

    impl Clone for StyleBorderLeftWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_left_width_value_deep_copy)(self) } }
    impl Drop for StyleBorderLeftWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_width_value_delete)(self); } }


    /// `StyleBorderRightColorValue` struct
    pub use crate::dll::AzStyleBorderRightColorValue as StyleBorderRightColorValue;

    impl Clone for StyleBorderRightColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_right_color_value_deep_copy)(self) } }
    impl Drop for StyleBorderRightColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_color_value_delete)(self); } }


    /// `StyleBorderRightStyleValue` struct
    pub use crate::dll::AzStyleBorderRightStyleValue as StyleBorderRightStyleValue;

    impl Clone for StyleBorderRightStyleValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_right_style_value_deep_copy)(self) } }
    impl Drop for StyleBorderRightStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_style_value_delete)(self); } }


    /// `StyleBorderRightWidthValue` struct
    pub use crate::dll::AzStyleBorderRightWidthValue as StyleBorderRightWidthValue;

    impl Clone for StyleBorderRightWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_right_width_value_deep_copy)(self) } }
    impl Drop for StyleBorderRightWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_width_value_delete)(self); } }


    /// `StyleBorderTopColorValue` struct
    pub use crate::dll::AzStyleBorderTopColorValue as StyleBorderTopColorValue;

    impl Clone for StyleBorderTopColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_color_value_deep_copy)(self) } }
    impl Drop for StyleBorderTopColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_color_value_delete)(self); } }


    /// `StyleBorderTopLeftRadiusValue` struct
    pub use crate::dll::AzStyleBorderTopLeftRadiusValue as StyleBorderTopLeftRadiusValue;

    impl Clone for StyleBorderTopLeftRadiusValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_left_radius_value_deep_copy)(self) } }
    impl Drop for StyleBorderTopLeftRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_left_radius_value_delete)(self); } }


    /// `StyleBorderTopRightRadiusValue` struct
    pub use crate::dll::AzStyleBorderTopRightRadiusValue as StyleBorderTopRightRadiusValue;

    impl Clone for StyleBorderTopRightRadiusValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_right_radius_value_deep_copy)(self) } }
    impl Drop for StyleBorderTopRightRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_right_radius_value_delete)(self); } }


    /// `StyleBorderTopStyleValue` struct
    pub use crate::dll::AzStyleBorderTopStyleValue as StyleBorderTopStyleValue;

    impl Clone for StyleBorderTopStyleValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_style_value_deep_copy)(self) } }
    impl Drop for StyleBorderTopStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_style_value_delete)(self); } }


    /// `StyleBorderTopWidthValue` struct
    pub use crate::dll::AzStyleBorderTopWidthValue as StyleBorderTopWidthValue;

    impl Clone for StyleBorderTopWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_border_top_width_value_deep_copy)(self) } }
    impl Drop for StyleBorderTopWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_width_value_delete)(self); } }


    /// `StyleCursorValue` struct
    pub use crate::dll::AzStyleCursorValue as StyleCursorValue;

    impl Clone for StyleCursorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_cursor_value_deep_copy)(self) } }
    impl Drop for StyleCursorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_cursor_value_delete)(self); } }


    /// `StyleFontFamilyValue` struct
    pub use crate::dll::AzStyleFontFamilyValue as StyleFontFamilyValue;

    impl Clone for StyleFontFamilyValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_font_family_value_deep_copy)(self) } }
    impl Drop for StyleFontFamilyValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_family_value_delete)(self); } }


    /// `StyleFontSizeValue` struct
    pub use crate::dll::AzStyleFontSizeValue as StyleFontSizeValue;

    impl Clone for StyleFontSizeValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_font_size_value_deep_copy)(self) } }
    impl Drop for StyleFontSizeValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_size_value_delete)(self); } }


    /// `StyleLetterSpacingValue` struct
    pub use crate::dll::AzStyleLetterSpacingValue as StyleLetterSpacingValue;

    impl Clone for StyleLetterSpacingValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_letter_spacing_value_deep_copy)(self) } }
    impl Drop for StyleLetterSpacingValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_letter_spacing_value_delete)(self); } }


    /// `StyleLineHeightValue` struct
    pub use crate::dll::AzStyleLineHeightValue as StyleLineHeightValue;

    impl Clone for StyleLineHeightValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_line_height_value_deep_copy)(self) } }
    impl Drop for StyleLineHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_line_height_value_delete)(self); } }


    /// `StyleTabWidthValue` struct
    pub use crate::dll::AzStyleTabWidthValue as StyleTabWidthValue;

    impl Clone for StyleTabWidthValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_tab_width_value_deep_copy)(self) } }
    impl Drop for StyleTabWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_tab_width_value_delete)(self); } }


    /// `StyleTextAlignmentHorzValue` struct
    pub use crate::dll::AzStyleTextAlignmentHorzValue as StyleTextAlignmentHorzValue;

    impl Clone for StyleTextAlignmentHorzValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_text_alignment_horz_value_deep_copy)(self) } }
    impl Drop for StyleTextAlignmentHorzValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_text_alignment_horz_value_delete)(self); } }


    /// `StyleTextColorValue` struct
    pub use crate::dll::AzStyleTextColorValue as StyleTextColorValue;

    impl Clone for StyleTextColorValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_text_color_value_deep_copy)(self) } }
    impl Drop for StyleTextColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_text_color_value_delete)(self); } }


    /// `StyleWordSpacingValue` struct
    pub use crate::dll::AzStyleWordSpacingValue as StyleWordSpacingValue;

    impl Clone for StyleWordSpacingValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_word_spacing_value_deep_copy)(self) } }
    impl Drop for StyleWordSpacingValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_word_spacing_value_delete)(self); } }


    /// Parsed CSS key-value pair
    pub use crate::dll::AzCssProperty as CssProperty;

    impl Clone for CssProperty { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_property_deep_copy)(self) } }
    impl Drop for CssProperty { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_property_delete)(self); } }
}

/// `Dom` construction and configuration
#[allow(dead_code, unused_imports)]
pub mod dom {

    use crate::dll::*;
    use std::ffi::c_void;
    impl std::iter::FromIterator<Dom> for Dom {
        fn from_iter<I: IntoIterator<Item=Dom>>(iter: I) -> Self {

            let mut estimated_total_children = 0;
            let children = iter.into_iter().map(|c| {
                estimated_total_children += c.estimated_total_children + 1;
                c
            }).collect();

            Dom {
                root: NodeData::new(NodeType::Div),
                children,
                estimated_total_children,
            }
        }
    }

    impl std::iter::FromIterator<NodeData> for Dom {
        fn from_iter<I: IntoIterator<Item=NodeData>>(iter: I) -> Self {
            use crate::vec::DomVec;
            let children = iter.into_iter().map(|c| Dom { root: c, children: DomVec::new(), estimated_total_children: 0 }).collect::<DomVec>();
            let estimated_total_children = children.len();

            Dom {
                root: NodeData::new(NodeType::Div),
                children: children,
                estimated_total_children,
            }
        }
    }

    impl std::iter::FromIterator<NodeType> for Dom {
        fn from_iter<I: IntoIterator<Item=NodeType>>(iter: I) -> Self {
            iter.into_iter().map(|i| {
                let mut nd = NodeData::default();
                nd.node_type = i;
                nd
            }).collect()
        }
    }    use crate::str::String;
    use crate::resources::{ImageId, TextId};
    use crate::callbacks::{CallbackType, GlCallbackType, IFrameCallbackType, RefAny};
    use crate::vec::StringVec;
    use crate::css::CssProperty;


    /// `Dom` struct
    pub use crate::dll::AzDom as Dom;

    impl Dom {
        /// Creates a new `div` node
        pub fn div() -> Self { (crate::dll::get_azul_dll().az_dom_div)() }
        /// Creates a new `body` node
        pub fn body() -> Self { (crate::dll::get_azul_dll().az_dom_body)() }
        /// Creates a new `p` node with a given `String` as the text contents
        pub fn label(text: String) -> Self { (crate::dll::get_azul_dll().az_dom_label)(text) }
        /// Creates a new `p` node from a (cached) text referenced by a `TextId`
        pub fn text(text_id: TextId) -> Self { (crate::dll::get_azul_dll().az_dom_text)(text_id) }
        /// Creates a new `img` node from a (cached) text referenced by a `ImageId`
        pub fn image(image_id: ImageId) -> Self { (crate::dll::get_azul_dll().az_dom_image)(image_id) }
        /// Creates a new node which will render an OpenGL texture after the layout step is finished. See the documentation for [GlCallback]() for more info about OpenGL rendering callbacks.
        pub fn gl_texture(data: RefAny, callback: GlCallbackType) -> Self { (crate::dll::get_azul_dll().az_dom_gl_texture)(data, callback) }
        /// Creates a new node with a callback that will return a `Dom` after being layouted. See the documentation for [IFrameCallback]() for more info about iframe callbacks.
        pub fn iframe(data: RefAny, callback: IFrameCallbackType) -> Self { (crate::dll::get_azul_dll().az_dom_iframe)(data, callback) }
        /// Adds a CSS ID (`#something`) to the DOM node
        pub fn add_id(&mut self, id: String)  { (crate::dll::get_azul_dll().az_dom_add_id)(self, id) }
        /// Same as [`Dom::add_id`](#method.add_id), but as a builder method
        pub fn with_id(self, id: String)  -> crate::dom::Dom { { (crate::dll::get_azul_dll().az_dom_with_id)(self, id)} }
        /// Same as calling [`Dom::add_id`](#method.add_id) for each CSS ID, but this function **replaces** all current CSS IDs
        pub fn set_ids(&mut self, ids: StringVec)  { (crate::dll::get_azul_dll().az_dom_set_ids)(self, ids) }
        /// Same as [`Dom::set_ids`](#method.set_ids), but as a builder method
        pub fn with_ids(self, ids: StringVec)  -> crate::dom::Dom { { (crate::dll::get_azul_dll().az_dom_with_ids)(self, ids)} }
        /// Adds a CSS class (`.something`) to the DOM node
        pub fn add_class(&mut self, class: String)  { (crate::dll::get_azul_dll().az_dom_add_class)(self, class) }
        /// Same as [`Dom::add_class`](#method.add_class), but as a builder method
        pub fn with_class(self, class: String)  -> crate::dom::Dom { { (crate::dll::get_azul_dll().az_dom_with_class)(self, class)} }
        /// Same as calling [`Dom::add_class`](#method.add_class) for each class, but this function **replaces** all current classes
        pub fn set_classes(&mut self, classes: StringVec)  { (crate::dll::get_azul_dll().az_dom_set_classes)(self, classes) }
        /// Same as [`Dom::set_classes`](#method.set_classes), but as a builder method
        pub fn with_classes(self, classes: StringVec)  -> crate::dom::Dom { { (crate::dll::get_azul_dll().az_dom_with_classes)(self, classes)} }
        /// Adds a [`Callback`](callbacks/type.Callback) that acts on the `data` the `event` happens
        pub fn add_callback(&mut self, event: EventFilter, data: RefAny, callback: CallbackType)  { (crate::dll::get_azul_dll().az_dom_add_callback)(self, event, data, callback) }
        /// Same as [`Dom::add_callback`](#method.add_callback), but as a builder method
        pub fn with_callback(self, event: EventFilter, data: RefAny, callback: CallbackType)  -> crate::dom::Dom { { (crate::dll::get_azul_dll().az_dom_with_callback)(self, event, data, callback)} }
        /// Overrides the CSS property of this DOM node with a value (for example `"width = 200px"`)
        pub fn add_css_override(&mut self, id: String, prop: CssProperty)  { (crate::dll::get_azul_dll().az_dom_add_css_override)(self, id, prop) }
        /// Same as [`Dom::add_css_override`](#method.add_css_override), but as a builder method
        pub fn with_css_override(self, id: String, prop: CssProperty)  -> crate::dom::Dom { { (crate::dll::get_azul_dll().az_dom_with_css_override)(self, id, prop)} }
        /// Sets the `is_draggable` attribute of this DOM node (default: false)
        pub fn set_is_draggable(&mut self, is_draggable: bool)  { (crate::dll::get_azul_dll().az_dom_set_is_draggable)(self, is_draggable) }
        /// Same as [`Dom::set_is_draggable`](#method.set_is_draggable), but as a builder method
        pub fn is_draggable(self, is_draggable: bool)  -> crate::dom::Dom { { (crate::dll::get_azul_dll().az_dom_is_draggable)(self, is_draggable)} }
        /// Sets the `tabindex` attribute of this DOM node (makes an element focusable - default: None)
        pub fn set_tab_index(&mut self, tab_index: TabIndex)  { (crate::dll::get_azul_dll().az_dom_set_tab_index)(self, tab_index) }
        /// Same as [`Dom::set_tab_index`](#method.set_tab_index), but as a builder method
        pub fn with_tab_index(self, tab_index: TabIndex)  -> crate::dom::Dom { { (crate::dll::get_azul_dll().az_dom_with_tab_index)(self, tab_index)} }
        /// Reparents another `Dom` to be the child node of this `Dom`
        pub fn add_child(&mut self, child: Dom)  { (crate::dll::get_azul_dll().az_dom_add_child)(self, child) }
        /// Same as [`Dom::add_child`](#method.add_child), but as a builder method
        pub fn with_child(self, child: Dom)  -> crate::dom::Dom { { (crate::dll::get_azul_dll().az_dom_with_child)(self, child)} }
        /// Returns if the DOM node has a certain CSS ID
        pub fn has_id(&mut self, id: String)  -> bool { (crate::dll::get_azul_dll().az_dom_has_id)(self, id) }
        /// Returns if the DOM node has a certain CSS class
        pub fn has_class(&mut self, class: String)  -> bool { (crate::dll::get_azul_dll().az_dom_has_class)(self, class) }
        /// Returns the HTML String for this DOM
        pub fn get_html_string(&self)  -> crate::str::String { { (crate::dll::get_azul_dll().az_dom_get_html_string)(self)} }
    }

    impl Clone for Dom { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_dom_deep_copy)(self) } }
    impl Drop for Dom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_dom_delete)(self); } }


    /// `GlTextureNode` struct
    pub use crate::dll::AzGlTextureNode as GlTextureNode;

    impl Clone for GlTextureNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_gl_texture_node_deep_copy)(self) } }
    impl Drop for GlTextureNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_texture_node_delete)(self); } }


    /// `IFrameNode` struct
    pub use crate::dll::AzIFrameNode as IFrameNode;

    impl Clone for IFrameNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_i_frame_node_deep_copy)(self) } }
    impl Drop for IFrameNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i_frame_node_delete)(self); } }


    /// `CallbackData` struct
    pub use crate::dll::AzCallbackData as CallbackData;

    impl Clone for CallbackData { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_callback_data_deep_copy)(self) } }
    impl Drop for CallbackData { fn drop(&mut self) { (crate::dll::get_azul_dll().az_callback_data_delete)(self); } }


    /// `OverrideProperty` struct
    pub use crate::dll::AzOverrideProperty as OverrideProperty;

    impl Clone for OverrideProperty { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_override_property_deep_copy)(self) } }
    impl Drop for OverrideProperty { fn drop(&mut self) { (crate::dll::get_azul_dll().az_override_property_delete)(self); } }


    /// Represents one single DOM node (node type, classes, ids and callbacks are stored here)
    pub use crate::dll::AzNodeData as NodeData;

    impl NodeData {
        /// Creates a new node without any classes or ids from a NodeType
        pub fn new(node_type: NodeType) -> Self { (crate::dll::get_azul_dll().az_node_data_new)(node_type) }
        /// Creates a default (div) node without any classes
        pub fn default() -> Self { (crate::dll::get_azul_dll().az_node_data_default)() }
    }

    impl Clone for NodeData { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_node_data_deep_copy)(self) } }
    impl Drop for NodeData { fn drop(&mut self) { (crate::dll::get_azul_dll().az_node_data_delete)(self); } }


    /// List of core DOM node types built-into by `azul`
    pub use crate::dll::AzNodeType as NodeType;

    impl Clone for NodeType { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_node_type_deep_copy)(self) } }
    impl Drop for NodeType { fn drop(&mut self) { (crate::dll::get_azul_dll().az_node_type_delete)(self); } }


    /// When to call a callback action - `On::MouseOver`, `On::MouseOut`, etc.
    pub use crate::dll::AzOn as On;

    impl On {
        /// Converts the `On` shorthand into a `EventFilter`
        pub fn into_event_filter(self)  -> crate::dom::EventFilter { { (crate::dll::get_azul_dll().az_on_into_event_filter)(self)} }
    }

    impl Clone for On { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_on_deep_copy)(self) } }
    impl Drop for On { fn drop(&mut self) { (crate::dll::get_azul_dll().az_on_delete)(self); } }


    /// `EventFilter` struct
    pub use crate::dll::AzEventFilter as EventFilter;

    impl Clone for EventFilter { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_event_filter_deep_copy)(self) } }
    impl Drop for EventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_event_filter_delete)(self); } }


    /// `HoverEventFilter` struct
    pub use crate::dll::AzHoverEventFilter as HoverEventFilter;

    impl Clone for HoverEventFilter { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_hover_event_filter_deep_copy)(self) } }
    impl Drop for HoverEventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_hover_event_filter_delete)(self); } }


    /// `FocusEventFilter` struct
    pub use crate::dll::AzFocusEventFilter as FocusEventFilter;

    impl Clone for FocusEventFilter { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_focus_event_filter_deep_copy)(self) } }
    impl Drop for FocusEventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_focus_event_filter_delete)(self); } }


    /// `NotEventFilter` struct
    pub use crate::dll::AzNotEventFilter as NotEventFilter;

    impl Clone for NotEventFilter { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_not_event_filter_deep_copy)(self) } }
    impl Drop for NotEventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_not_event_filter_delete)(self); } }


    /// `WindowEventFilter` struct
    pub use crate::dll::AzWindowEventFilter as WindowEventFilter;

    impl Clone for WindowEventFilter { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_event_filter_deep_copy)(self) } }
    impl Drop for WindowEventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_event_filter_delete)(self); } }


    /// `TabIndex` struct
    pub use crate::dll::AzTabIndex as TabIndex;

    impl Clone for TabIndex { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_tab_index_deep_copy)(self) } }
    impl Drop for TabIndex { fn drop(&mut self) { (crate::dll::get_azul_dll().az_tab_index_delete)(self); } }
}

/// OpenGl helper types (`Texture`, `GlContext`, etc.)
#[allow(dead_code, unused_imports)]
pub mod gl {

    use crate::dll::*;
    use std::ffi::c_void;
    impl Refstr {
        fn as_str(&self) -> &str { unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.ptr, self.len)) } }
    }

    impl From<&str> for Refstr {
        fn from(s: &str) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl RefstrVecRef {
        fn as_slice(&self) -> &[Refstr] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&[Refstr]> for RefstrVecRef {
        fn from(s: &[Refstr]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl From<&mut [GLint64]> for GLint64VecRefMut {
        fn from(s: &mut [GLint64]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl GLint64VecRefMut {
        fn as_mut_slice(&mut self) -> &mut [GLint64] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    impl From<&mut [GLfloat]> for GLfloatVecRefMut {
        fn from(s: &mut [GLfloat]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl GLfloatVecRefMut {
        fn as_mut_slice(&mut self) -> &mut [GLfloat] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    impl From<&mut [GLint]> for GLintVecRefMut {
        fn from(s: &mut [GLint]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl GLintVecRefMut {
        fn as_mut_slice(&mut self) -> &mut [GLint] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    impl From<&[GLuint]> for GLuintVecRef {
        fn from(s: &[GLuint]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl GLuintVecRef {
        fn as_slice(&self) -> &[GLuint] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&[GLenum]> for GLenumVecRef {
        fn from(s: &[GLenum]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl GLenumVecRef {
        fn as_slice(&self) -> &[GLenum] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&[u8]> for U8VecRef {
        fn from(s: &[u8]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl U8VecRef {
        fn as_slice(&self) -> &[u8] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl std::fmt::Debug for U8VecRef {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            self.as_slice().fmt(f)
        }
    }

    impl PartialOrd for U8VecRef {
        fn partial_cmp(&self, rhs: &Self) -> Option<std::cmp::Ordering> {
            self.as_slice().partial_cmp(rhs.as_slice())
        }
    }

    impl Ord for U8VecRef {
        fn cmp(&self, rhs: &Self) -> std::cmp::Ordering {
            self.as_slice().cmp(rhs.as_slice())
        }
    }

    impl PartialEq for U8VecRef {
        fn eq(&self, rhs: &Self) -> bool {
            self.as_slice().eq(rhs.as_slice())
        }
    }

    impl Eq for U8VecRef { }

    impl std::hash::Hash for U8VecRef {
        fn hash<H>(&self, state: &mut H) where H: std::hash::Hasher {
            self.as_slice().hash(state)
        }
    }

    impl From<&[f32]> for F32VecRef {
        fn from(s: &[f32]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl F32VecRef {
        fn as_slice(&self) -> &[f32] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&[i32]> for I32VecRef {
        fn from(s: &[i32]) -> Self {
            Self { ptr: s.as_ptr(), len: s.len() }
        }
    }

    impl I32VecRef {
        fn as_slice(&self) -> &[i32] { unsafe { std::slice::from_raw_parts(self.ptr, self.len) } }
    }

    impl From<&mut [GLboolean]> for GLbooleanVecRefMut {
        fn from(s: &mut [GLboolean]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl GLbooleanVecRefMut {
        fn as_mut_slice(&mut self) -> &mut [GLboolean] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    impl From<&mut [u8]> for U8VecRefMut {
        fn from(s: &mut [u8]) -> Self {
            Self { ptr: s.as_mut_ptr(), len: s.len() }
        }
    }

    impl U8VecRefMut {
        fn as_mut_slice(&mut self) -> &mut [u8] { unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) } }
    }

    pub type GLenum = std::os::raw::c_uint;
    pub type GLboolean = std::os::raw::c_uchar;
    pub type GLbitfield = std::os::raw::c_uint;
    pub type GLvoid = std::os::raw::c_void;
    pub type GLbyte = std::os::raw::c_char;
    pub type GLshort = std::os::raw::c_short;
    pub type GLint = std::os::raw::c_int;
    pub type GLclampx = std::os::raw::c_int;
    pub type GLubyte = std::os::raw::c_uchar;
    pub type GLushort = std::os::raw::c_ushort;
    pub type GLuint = std::os::raw::c_uint;
    pub type GLsizei = std::os::raw::c_int;
    pub type GLfloat = std::os::raw::c_float;
    pub type GLclampf = std::os::raw::c_float;
    pub type GLdouble = std::os::raw::c_double;
    pub type GLclampd = std::os::raw::c_double;
    pub type GLeglImageOES = *const std::os::raw::c_void;
    pub type GLchar = std::os::raw::c_char;
    pub type GLcharARB = std::os::raw::c_char;

    #[cfg(target_os = "macos")]
    pub type GLhandleARB = *const std::os::raw::c_void;
    #[cfg(not(target_os = "macos"))]
    pub type GLhandleARB = std::os::raw::c_uint;

    pub type GLhalfARB = std::os::raw::c_ushort;
    pub type GLhalf = std::os::raw::c_ushort;

    // Must be 32 bits
    pub type GLfixed = GLint;
    pub type GLintptr = isize;
    pub type GLsizeiptr = isize;
    pub type GLint64 = i64;
    pub type GLuint64 = u64;
    pub type GLintptrARB = isize;
    pub type GLsizeiptrARB = isize;
    pub type GLint64EXT = i64;
    pub type GLuint64EXT = u64;

    pub type GLDEBUGPROC = Option<extern "system" fn(source: GLenum, gltype: GLenum, id: GLuint, severity: GLenum, length: GLsizei, message: *const GLchar, userParam: *mut std::os::raw::c_void)>;
    pub type GLDEBUGPROCARB = Option<extern "system" fn(source: GLenum, gltype: GLenum, id: GLuint, severity: GLenum, length: GLsizei, message: *const GLchar, userParam: *mut std::os::raw::c_void)>;
    pub type GLDEBUGPROCKHR = Option<extern "system" fn(source: GLenum, gltype: GLenum, id: GLuint, severity: GLenum, length: GLsizei, message: *const GLchar, userParam: *mut std::os::raw::c_void)>;

    // Vendor extension types
    pub type GLDEBUGPROCAMD = Option<extern "system" fn(id: GLuint, category: GLenum, severity: GLenum, length: GLsizei, message: *const GLchar, userParam: *mut std::os::raw::c_void)>;
    pub type GLhalfNV = std::os::raw::c_ushort;
    pub type GLvdpauSurfaceNV = GLintptr;

    pub const ACCUM: GLenum = 0x0100;
    pub const ACCUM_ALPHA_BITS: GLenum = 0x0D5B;
    pub const ACCUM_BLUE_BITS: GLenum = 0x0D5A;
    pub const ACCUM_BUFFER_BIT: GLenum = 0x00000200;
    pub const ACCUM_CLEAR_VALUE: GLenum = 0x0B80;
    pub const ACCUM_GREEN_BITS: GLenum = 0x0D59;
    pub const ACCUM_RED_BITS: GLenum = 0x0D58;
    pub const ACTIVE_ATTRIBUTES: GLenum = 0x8B89;
    pub const ACTIVE_ATTRIBUTE_MAX_LENGTH: GLenum = 0x8B8A;
    pub const ACTIVE_TEXTURE: GLenum = 0x84E0;
    pub const ACTIVE_UNIFORMS: GLenum = 0x8B86;
    pub const ACTIVE_UNIFORM_BLOCKS: GLenum = 0x8A36;
    pub const ACTIVE_UNIFORM_BLOCK_MAX_NAME_LENGTH: GLenum = 0x8A35;
    pub const ACTIVE_UNIFORM_MAX_LENGTH: GLenum = 0x8B87;
    pub const ADD: GLenum = 0x0104;
    pub const ADD_SIGNED: GLenum = 0x8574;
    pub const ALIASED_LINE_WIDTH_RANGE: GLenum = 0x846E;
    pub const ALIASED_POINT_SIZE_RANGE: GLenum = 0x846D;
    pub const ALL_ATTRIB_BITS: GLenum = 0xFFFFFFFF;
    pub const ALPHA: GLenum = 0x1906;
    pub const ALPHA12: GLenum = 0x803D;
    pub const ALPHA16: GLenum = 0x803E;
    pub const ALPHA16F_EXT: GLenum = 0x881C;
    pub const ALPHA32F_EXT: GLenum = 0x8816;
    pub const ALPHA4: GLenum = 0x803B;
    pub const ALPHA8: GLenum = 0x803C;
    pub const ALPHA8_EXT: GLenum = 0x803C;
    pub const ALPHA_BIAS: GLenum = 0x0D1D;
    pub const ALPHA_BITS: GLenum = 0x0D55;
    pub const ALPHA_INTEGER: GLenum = 0x8D97;
    pub const ALPHA_SCALE: GLenum = 0x0D1C;
    pub const ALPHA_TEST: GLenum = 0x0BC0;
    pub const ALPHA_TEST_FUNC: GLenum = 0x0BC1;
    pub const ALPHA_TEST_REF: GLenum = 0x0BC2;
    pub const ALREADY_SIGNALED: GLenum = 0x911A;
    pub const ALWAYS: GLenum = 0x0207;
    pub const AMBIENT: GLenum = 0x1200;
    pub const AMBIENT_AND_DIFFUSE: GLenum = 0x1602;
    pub const AND: GLenum = 0x1501;
    pub const AND_INVERTED: GLenum = 0x1504;
    pub const AND_REVERSE: GLenum = 0x1502;
    pub const ANY_SAMPLES_PASSED: GLenum = 0x8C2F;
    pub const ANY_SAMPLES_PASSED_CONSERVATIVE: GLenum = 0x8D6A;
    pub const ARRAY_BUFFER: GLenum = 0x8892;
    pub const ARRAY_BUFFER_BINDING: GLenum = 0x8894;
    pub const ATTACHED_SHADERS: GLenum = 0x8B85;
    pub const ATTRIB_STACK_DEPTH: GLenum = 0x0BB0;
    pub const AUTO_NORMAL: GLenum = 0x0D80;
    pub const AUX0: GLenum = 0x0409;
    pub const AUX1: GLenum = 0x040A;
    pub const AUX2: GLenum = 0x040B;
    pub const AUX3: GLenum = 0x040C;
    pub const AUX_BUFFERS: GLenum = 0x0C00;
    pub const BACK: GLenum = 0x0405;
    pub const BACK_LEFT: GLenum = 0x0402;
    pub const BACK_RIGHT: GLenum = 0x0403;
    pub const BGR: GLenum = 0x80E0;
    pub const BGRA: GLenum = 0x80E1;
    pub const BGRA8_EXT: GLenum = 0x93A1;
    pub const BGRA_EXT: GLenum = 0x80E1;
    pub const BGRA_INTEGER: GLenum = 0x8D9B;
    pub const BGR_INTEGER: GLenum = 0x8D9A;
    pub const BITMAP: GLenum = 0x1A00;
    pub const BITMAP_TOKEN: GLenum = 0x0704;
    pub const BLEND: GLenum = 0x0BE2;
    pub const BLEND_ADVANCED_COHERENT_KHR: GLenum = 0x9285;
    pub const BLEND_COLOR: GLenum = 0x8005;
    pub const BLEND_DST: GLenum = 0x0BE0;
    pub const BLEND_DST_ALPHA: GLenum = 0x80CA;
    pub const BLEND_DST_RGB: GLenum = 0x80C8;
    pub const BLEND_EQUATION: GLenum = 0x8009;
    pub const BLEND_EQUATION_ALPHA: GLenum = 0x883D;
    pub const BLEND_EQUATION_RGB: GLenum = 0x8009;
    pub const BLEND_SRC: GLenum = 0x0BE1;
    pub const BLEND_SRC_ALPHA: GLenum = 0x80CB;
    pub const BLEND_SRC_RGB: GLenum = 0x80C9;
    pub const BLUE: GLenum = 0x1905;
    pub const BLUE_BIAS: GLenum = 0x0D1B;
    pub const BLUE_BITS: GLenum = 0x0D54;
    pub const BLUE_INTEGER: GLenum = 0x8D96;
    pub const BLUE_SCALE: GLenum = 0x0D1A;
    pub const BOOL: GLenum = 0x8B56;
    pub const BOOL_VEC2: GLenum = 0x8B57;
    pub const BOOL_VEC3: GLenum = 0x8B58;
    pub const BOOL_VEC4: GLenum = 0x8B59;
    pub const BUFFER: GLenum = 0x82E0;
    pub const BUFFER_ACCESS: GLenum = 0x88BB;
    pub const BUFFER_ACCESS_FLAGS: GLenum = 0x911F;
    pub const BUFFER_KHR: GLenum = 0x82E0;
    pub const BUFFER_MAPPED: GLenum = 0x88BC;
    pub const BUFFER_MAP_LENGTH: GLenum = 0x9120;
    pub const BUFFER_MAP_OFFSET: GLenum = 0x9121;
    pub const BUFFER_MAP_POINTER: GLenum = 0x88BD;
    pub const BUFFER_SIZE: GLenum = 0x8764;
    pub const BUFFER_USAGE: GLenum = 0x8765;
    pub const BYTE: GLenum = 0x1400;
    pub const C3F_V3F: GLenum = 0x2A24;
    pub const C4F_N3F_V3F: GLenum = 0x2A26;
    pub const C4UB_V2F: GLenum = 0x2A22;
    pub const C4UB_V3F: GLenum = 0x2A23;
    pub const CCW: GLenum = 0x0901;
    pub const CLAMP: GLenum = 0x2900;
    pub const CLAMP_FRAGMENT_COLOR: GLenum = 0x891B;
    pub const CLAMP_READ_COLOR: GLenum = 0x891C;
    pub const CLAMP_TO_BORDER: GLenum = 0x812D;
    pub const CLAMP_TO_EDGE: GLenum = 0x812F;
    pub const CLAMP_VERTEX_COLOR: GLenum = 0x891A;
    pub const CLEAR: GLenum = 0x1500;
    pub const CLIENT_ACTIVE_TEXTURE: GLenum = 0x84E1;
    pub const CLIENT_ALL_ATTRIB_BITS: GLenum = 0xFFFFFFFF;
    pub const CLIENT_ATTRIB_STACK_DEPTH: GLenum = 0x0BB1;
    pub const CLIENT_PIXEL_STORE_BIT: GLenum = 0x00000001;
    pub const CLIENT_VERTEX_ARRAY_BIT: GLenum = 0x00000002;
    pub const CLIP_DISTANCE0: GLenum = 0x3000;
    pub const CLIP_DISTANCE1: GLenum = 0x3001;
    pub const CLIP_DISTANCE2: GLenum = 0x3002;
    pub const CLIP_DISTANCE3: GLenum = 0x3003;
    pub const CLIP_DISTANCE4: GLenum = 0x3004;
    pub const CLIP_DISTANCE5: GLenum = 0x3005;
    pub const CLIP_DISTANCE6: GLenum = 0x3006;
    pub const CLIP_DISTANCE7: GLenum = 0x3007;
    pub const CLIP_PLANE0: GLenum = 0x3000;
    pub const CLIP_PLANE1: GLenum = 0x3001;
    pub const CLIP_PLANE2: GLenum = 0x3002;
    pub const CLIP_PLANE3: GLenum = 0x3003;
    pub const CLIP_PLANE4: GLenum = 0x3004;
    pub const CLIP_PLANE5: GLenum = 0x3005;
    pub const COEFF: GLenum = 0x0A00;
    pub const COLOR: GLenum = 0x1800;
    pub const COLORBURN_KHR: GLenum = 0x929A;
    pub const COLORDODGE_KHR: GLenum = 0x9299;
    pub const COLOR_ARRAY: GLenum = 0x8076;
    pub const COLOR_ARRAY_BUFFER_BINDING: GLenum = 0x8898;
    pub const COLOR_ARRAY_POINTER: GLenum = 0x8090;
    pub const COLOR_ARRAY_SIZE: GLenum = 0x8081;
    pub const COLOR_ARRAY_STRIDE: GLenum = 0x8083;
    pub const COLOR_ARRAY_TYPE: GLenum = 0x8082;
    pub const COLOR_ATTACHMENT0: GLenum = 0x8CE0;
    pub const COLOR_ATTACHMENT1: GLenum = 0x8CE1;
    pub const COLOR_ATTACHMENT10: GLenum = 0x8CEA;
    pub const COLOR_ATTACHMENT11: GLenum = 0x8CEB;
    pub const COLOR_ATTACHMENT12: GLenum = 0x8CEC;
    pub const COLOR_ATTACHMENT13: GLenum = 0x8CED;
    pub const COLOR_ATTACHMENT14: GLenum = 0x8CEE;
    pub const COLOR_ATTACHMENT15: GLenum = 0x8CEF;
    pub const COLOR_ATTACHMENT16: GLenum = 0x8CF0;
    pub const COLOR_ATTACHMENT17: GLenum = 0x8CF1;
    pub const COLOR_ATTACHMENT18: GLenum = 0x8CF2;
    pub const COLOR_ATTACHMENT19: GLenum = 0x8CF3;
    pub const COLOR_ATTACHMENT2: GLenum = 0x8CE2;
    pub const COLOR_ATTACHMENT20: GLenum = 0x8CF4;
    pub const COLOR_ATTACHMENT21: GLenum = 0x8CF5;
    pub const COLOR_ATTACHMENT22: GLenum = 0x8CF6;
    pub const COLOR_ATTACHMENT23: GLenum = 0x8CF7;
    pub const COLOR_ATTACHMENT24: GLenum = 0x8CF8;
    pub const COLOR_ATTACHMENT25: GLenum = 0x8CF9;
    pub const COLOR_ATTACHMENT26: GLenum = 0x8CFA;
    pub const COLOR_ATTACHMENT27: GLenum = 0x8CFB;
    pub const COLOR_ATTACHMENT28: GLenum = 0x8CFC;
    pub const COLOR_ATTACHMENT29: GLenum = 0x8CFD;
    pub const COLOR_ATTACHMENT3: GLenum = 0x8CE3;
    pub const COLOR_ATTACHMENT30: GLenum = 0x8CFE;
    pub const COLOR_ATTACHMENT31: GLenum = 0x8CFF;
    pub const COLOR_ATTACHMENT4: GLenum = 0x8CE4;
    pub const COLOR_ATTACHMENT5: GLenum = 0x8CE5;
    pub const COLOR_ATTACHMENT6: GLenum = 0x8CE6;
    pub const COLOR_ATTACHMENT7: GLenum = 0x8CE7;
    pub const COLOR_ATTACHMENT8: GLenum = 0x8CE8;
    pub const COLOR_ATTACHMENT9: GLenum = 0x8CE9;
    pub const COLOR_BUFFER_BIT: GLenum = 0x00004000;
    pub const COLOR_CLEAR_VALUE: GLenum = 0x0C22;
    pub const COLOR_INDEX: GLenum = 0x1900;
    pub const COLOR_INDEXES: GLenum = 0x1603;
    pub const COLOR_LOGIC_OP: GLenum = 0x0BF2;
    pub const COLOR_MATERIAL: GLenum = 0x0B57;
    pub const COLOR_MATERIAL_FACE: GLenum = 0x0B55;
    pub const COLOR_MATERIAL_PARAMETER: GLenum = 0x0B56;
    pub const COLOR_SUM: GLenum = 0x8458;
    pub const COLOR_WRITEMASK: GLenum = 0x0C23;
    pub const COMBINE: GLenum = 0x8570;
    pub const COMBINE_ALPHA: GLenum = 0x8572;
    pub const COMBINE_RGB: GLenum = 0x8571;
    pub const COMPARE_REF_TO_TEXTURE: GLenum = 0x884E;
    pub const COMPARE_R_TO_TEXTURE: GLenum = 0x884E;
    pub const COMPILE: GLenum = 0x1300;
    pub const COMPILE_AND_EXECUTE: GLenum = 0x1301;
    pub const COMPILE_STATUS: GLenum = 0x8B81;
    pub const COMPRESSED_ALPHA: GLenum = 0x84E9;
    pub const COMPRESSED_INTENSITY: GLenum = 0x84EC;
    pub const COMPRESSED_LUMINANCE: GLenum = 0x84EA;
    pub const COMPRESSED_LUMINANCE_ALPHA: GLenum = 0x84EB;
    pub const COMPRESSED_R11_EAC: GLenum = 0x9270;
    pub const COMPRESSED_RED: GLenum = 0x8225;
    pub const COMPRESSED_RED_RGTC1: GLenum = 0x8DBB;
    pub const COMPRESSED_RG: GLenum = 0x8226;
    pub const COMPRESSED_RG11_EAC: GLenum = 0x9272;
    pub const COMPRESSED_RGB: GLenum = 0x84ED;
    pub const COMPRESSED_RGB8_ETC2: GLenum = 0x9274;
    pub const COMPRESSED_RGB8_PUNCHTHROUGH_ALPHA1_ETC2: GLenum = 0x9276;
    pub const COMPRESSED_RGBA: GLenum = 0x84EE;
    pub const COMPRESSED_RGBA8_ETC2_EAC: GLenum = 0x9278;
    pub const COMPRESSED_RG_RGTC2: GLenum = 0x8DBD;
    pub const COMPRESSED_SIGNED_R11_EAC: GLenum = 0x9271;
    pub const COMPRESSED_SIGNED_RED_RGTC1: GLenum = 0x8DBC;
    pub const COMPRESSED_SIGNED_RG11_EAC: GLenum = 0x9273;
    pub const COMPRESSED_SIGNED_RG_RGTC2: GLenum = 0x8DBE;
    pub const COMPRESSED_SLUMINANCE: GLenum = 0x8C4A;
    pub const COMPRESSED_SLUMINANCE_ALPHA: GLenum = 0x8C4B;
    pub const COMPRESSED_SRGB: GLenum = 0x8C48;
    pub const COMPRESSED_SRGB8_ALPHA8_ETC2_EAC: GLenum = 0x9279;
    pub const COMPRESSED_SRGB8_ETC2: GLenum = 0x9275;
    pub const COMPRESSED_SRGB8_PUNCHTHROUGH_ALPHA1_ETC2: GLenum = 0x9277;
    pub const COMPRESSED_SRGB_ALPHA: GLenum = 0x8C49;
    pub const COMPRESSED_TEXTURE_FORMATS: GLenum = 0x86A3;
    pub const CONDITION_SATISFIED: GLenum = 0x911C;
    pub const CONSTANT: GLenum = 0x8576;
    pub const CONSTANT_ALPHA: GLenum = 0x8003;
    pub const CONSTANT_ATTENUATION: GLenum = 0x1207;
    pub const CONSTANT_COLOR: GLenum = 0x8001;
    pub const CONTEXT_COMPATIBILITY_PROFILE_BIT: GLenum = 0x00000002;
    pub const CONTEXT_CORE_PROFILE_BIT: GLenum = 0x00000001;
    pub const CONTEXT_FLAGS: GLenum = 0x821E;
    pub const CONTEXT_FLAG_DEBUG_BIT: GLenum = 0x00000002;
    pub const CONTEXT_FLAG_DEBUG_BIT_KHR: GLenum = 0x00000002;
    pub const CONTEXT_FLAG_FORWARD_COMPATIBLE_BIT: GLenum = 0x00000001;
    pub const CONTEXT_PROFILE_MASK: GLenum = 0x9126;
    pub const COORD_REPLACE: GLenum = 0x8862;
    pub const COPY: GLenum = 0x1503;
    pub const COPY_INVERTED: GLenum = 0x150C;
    pub const COPY_PIXEL_TOKEN: GLenum = 0x0706;
    pub const COPY_READ_BUFFER: GLenum = 0x8F36;
    pub const COPY_READ_BUFFER_BINDING: GLenum = 0x8F36;
    pub const COPY_WRITE_BUFFER: GLenum = 0x8F37;
    pub const COPY_WRITE_BUFFER_BINDING: GLenum = 0x8F37;
    pub const CULL_FACE: GLenum = 0x0B44;
    pub const CULL_FACE_MODE: GLenum = 0x0B45;
    pub const CURRENT_BIT: GLenum = 0x00000001;
    pub const CURRENT_COLOR: GLenum = 0x0B00;
    pub const CURRENT_FOG_COORD: GLenum = 0x8453;
    pub const CURRENT_FOG_COORDINATE: GLenum = 0x8453;
    pub const CURRENT_INDEX: GLenum = 0x0B01;
    pub const CURRENT_NORMAL: GLenum = 0x0B02;
    pub const CURRENT_PROGRAM: GLenum = 0x8B8D;
    pub const CURRENT_QUERY: GLenum = 0x8865;
    pub const CURRENT_QUERY_EXT: GLenum = 0x8865;
    pub const CURRENT_RASTER_COLOR: GLenum = 0x0B04;
    pub const CURRENT_RASTER_DISTANCE: GLenum = 0x0B09;
    pub const CURRENT_RASTER_INDEX: GLenum = 0x0B05;
    pub const CURRENT_RASTER_POSITION: GLenum = 0x0B07;
    pub const CURRENT_RASTER_POSITION_VALID: GLenum = 0x0B08;
    pub const CURRENT_RASTER_SECONDARY_COLOR: GLenum = 0x845F;
    pub const CURRENT_RASTER_TEXTURE_COORDS: GLenum = 0x0B06;
    pub const CURRENT_SECONDARY_COLOR: GLenum = 0x8459;
    pub const CURRENT_TEXTURE_COORDS: GLenum = 0x0B03;
    pub const CURRENT_VERTEX_ATTRIB: GLenum = 0x8626;
    pub const CW: GLenum = 0x0900;
    pub const DARKEN_KHR: GLenum = 0x9297;
    pub const DEBUG_CALLBACK_FUNCTION: GLenum = 0x8244;
    pub const DEBUG_CALLBACK_FUNCTION_KHR: GLenum = 0x8244;
    pub const DEBUG_CALLBACK_USER_PARAM: GLenum = 0x8245;
    pub const DEBUG_CALLBACK_USER_PARAM_KHR: GLenum = 0x8245;
    pub const DEBUG_GROUP_STACK_DEPTH: GLenum = 0x826D;
    pub const DEBUG_GROUP_STACK_DEPTH_KHR: GLenum = 0x826D;
    pub const DEBUG_LOGGED_MESSAGES: GLenum = 0x9145;
    pub const DEBUG_LOGGED_MESSAGES_KHR: GLenum = 0x9145;
    pub const DEBUG_NEXT_LOGGED_MESSAGE_LENGTH: GLenum = 0x8243;
    pub const DEBUG_NEXT_LOGGED_MESSAGE_LENGTH_KHR: GLenum = 0x8243;
    pub const DEBUG_OUTPUT: GLenum = 0x92E0;
    pub const DEBUG_OUTPUT_KHR: GLenum = 0x92E0;
    pub const DEBUG_OUTPUT_SYNCHRONOUS: GLenum = 0x8242;
    pub const DEBUG_OUTPUT_SYNCHRONOUS_KHR: GLenum = 0x8242;
    pub const DEBUG_SEVERITY_HIGH: GLenum = 0x9146;
    pub const DEBUG_SEVERITY_HIGH_KHR: GLenum = 0x9146;
    pub const DEBUG_SEVERITY_LOW: GLenum = 0x9148;
    pub const DEBUG_SEVERITY_LOW_KHR: GLenum = 0x9148;
    pub const DEBUG_SEVERITY_MEDIUM: GLenum = 0x9147;
    pub const DEBUG_SEVERITY_MEDIUM_KHR: GLenum = 0x9147;
    pub const DEBUG_SEVERITY_NOTIFICATION: GLenum = 0x826B;
    pub const DEBUG_SEVERITY_NOTIFICATION_KHR: GLenum = 0x826B;
    pub const DEBUG_SOURCE_API: GLenum = 0x8246;
    pub const DEBUG_SOURCE_API_KHR: GLenum = 0x8246;
    pub const DEBUG_SOURCE_APPLICATION: GLenum = 0x824A;
    pub const DEBUG_SOURCE_APPLICATION_KHR: GLenum = 0x824A;
    pub const DEBUG_SOURCE_OTHER: GLenum = 0x824B;
    pub const DEBUG_SOURCE_OTHER_KHR: GLenum = 0x824B;
    pub const DEBUG_SOURCE_SHADER_COMPILER: GLenum = 0x8248;
    pub const DEBUG_SOURCE_SHADER_COMPILER_KHR: GLenum = 0x8248;
    pub const DEBUG_SOURCE_THIRD_PARTY: GLenum = 0x8249;
    pub const DEBUG_SOURCE_THIRD_PARTY_KHR: GLenum = 0x8249;
    pub const DEBUG_SOURCE_WINDOW_SYSTEM: GLenum = 0x8247;
    pub const DEBUG_SOURCE_WINDOW_SYSTEM_KHR: GLenum = 0x8247;
    pub const DEBUG_TYPE_DEPRECATED_BEHAVIOR: GLenum = 0x824D;
    pub const DEBUG_TYPE_DEPRECATED_BEHAVIOR_KHR: GLenum = 0x824D;
    pub const DEBUG_TYPE_ERROR: GLenum = 0x824C;
    pub const DEBUG_TYPE_ERROR_KHR: GLenum = 0x824C;
    pub const DEBUG_TYPE_MARKER: GLenum = 0x8268;
    pub const DEBUG_TYPE_MARKER_KHR: GLenum = 0x8268;
    pub const DEBUG_TYPE_OTHER: GLenum = 0x8251;
    pub const DEBUG_TYPE_OTHER_KHR: GLenum = 0x8251;
    pub const DEBUG_TYPE_PERFORMANCE: GLenum = 0x8250;
    pub const DEBUG_TYPE_PERFORMANCE_KHR: GLenum = 0x8250;
    pub const DEBUG_TYPE_POP_GROUP: GLenum = 0x826A;
    pub const DEBUG_TYPE_POP_GROUP_KHR: GLenum = 0x826A;
    pub const DEBUG_TYPE_PORTABILITY: GLenum = 0x824F;
    pub const DEBUG_TYPE_PORTABILITY_KHR: GLenum = 0x824F;
    pub const DEBUG_TYPE_PUSH_GROUP: GLenum = 0x8269;
    pub const DEBUG_TYPE_PUSH_GROUP_KHR: GLenum = 0x8269;
    pub const DEBUG_TYPE_UNDEFINED_BEHAVIOR: GLenum = 0x824E;
    pub const DEBUG_TYPE_UNDEFINED_BEHAVIOR_KHR: GLenum = 0x824E;
    pub const DECAL: GLenum = 0x2101;
    pub const DECR: GLenum = 0x1E03;
    pub const DECR_WRAP: GLenum = 0x8508;
    pub const DELETE_STATUS: GLenum = 0x8B80;
    pub const DEPTH: GLenum = 0x1801;
    pub const DEPTH24_STENCIL8: GLenum = 0x88F0;
    pub const DEPTH32F_STENCIL8: GLenum = 0x8CAD;
    pub const DEPTH_ATTACHMENT: GLenum = 0x8D00;
    pub const DEPTH_BIAS: GLenum = 0x0D1F;
    pub const DEPTH_BITS: GLenum = 0x0D56;
    pub const DEPTH_BUFFER_BIT: GLenum = 0x00000100;
    pub const DEPTH_CLAMP: GLenum = 0x864F;
    pub const DEPTH_CLEAR_VALUE: GLenum = 0x0B73;
    pub const DEPTH_COMPONENT: GLenum = 0x1902;
    pub const DEPTH_COMPONENT16: GLenum = 0x81A5;
    pub const DEPTH_COMPONENT24: GLenum = 0x81A6;
    pub const DEPTH_COMPONENT32: GLenum = 0x81A7;
    pub const DEPTH_COMPONENT32F: GLenum = 0x8CAC;
    pub const DEPTH_FUNC: GLenum = 0x0B74;
    pub const DEPTH_RANGE: GLenum = 0x0B70;
    pub const DEPTH_SCALE: GLenum = 0x0D1E;
    pub const DEPTH_STENCIL: GLenum = 0x84F9;
    pub const DEPTH_STENCIL_ATTACHMENT: GLenum = 0x821A;
    pub const DEPTH_TEST: GLenum = 0x0B71;
    pub const DEPTH_TEXTURE_MODE: GLenum = 0x884B;
    pub const DEPTH_WRITEMASK: GLenum = 0x0B72;
    pub const DIFFERENCE_KHR: GLenum = 0x929E;
    pub const DIFFUSE: GLenum = 0x1201;
    pub const DISPLAY_LIST: GLenum = 0x82E7;
    pub const DITHER: GLenum = 0x0BD0;
    pub const DOMAIN: GLenum = 0x0A02;
    pub const DONT_CARE: GLenum = 0x1100;
    pub const DOT3_RGB: GLenum = 0x86AE;
    pub const DOT3_RGBA: GLenum = 0x86AF;
    pub const DOUBLE: GLenum = 0x140A;
    pub const DOUBLEBUFFER: GLenum = 0x0C32;
    pub const DRAW_BUFFER: GLenum = 0x0C01;
    pub const DRAW_BUFFER0: GLenum = 0x8825;
    pub const DRAW_BUFFER1: GLenum = 0x8826;
    pub const DRAW_BUFFER10: GLenum = 0x882F;
    pub const DRAW_BUFFER11: GLenum = 0x8830;
    pub const DRAW_BUFFER12: GLenum = 0x8831;
    pub const DRAW_BUFFER13: GLenum = 0x8832;
    pub const DRAW_BUFFER14: GLenum = 0x8833;
    pub const DRAW_BUFFER15: GLenum = 0x8834;
    pub const DRAW_BUFFER2: GLenum = 0x8827;
    pub const DRAW_BUFFER3: GLenum = 0x8828;
    pub const DRAW_BUFFER4: GLenum = 0x8829;
    pub const DRAW_BUFFER5: GLenum = 0x882A;
    pub const DRAW_BUFFER6: GLenum = 0x882B;
    pub const DRAW_BUFFER7: GLenum = 0x882C;
    pub const DRAW_BUFFER8: GLenum = 0x882D;
    pub const DRAW_BUFFER9: GLenum = 0x882E;
    pub const DRAW_FRAMEBUFFER: GLenum = 0x8CA9;
    pub const DRAW_FRAMEBUFFER_BINDING: GLenum = 0x8CA6;
    pub const DRAW_PIXELS_APPLE: GLenum = 0x8A0A;
    pub const DRAW_PIXEL_TOKEN: GLenum = 0x0705;
    pub const DST_ALPHA: GLenum = 0x0304;
    pub const DST_COLOR: GLenum = 0x0306;
    pub const DYNAMIC_COPY: GLenum = 0x88EA;
    pub const DYNAMIC_DRAW: GLenum = 0x88E8;
    pub const DYNAMIC_READ: GLenum = 0x88E9;
    pub const EDGE_FLAG: GLenum = 0x0B43;
    pub const EDGE_FLAG_ARRAY: GLenum = 0x8079;
    pub const EDGE_FLAG_ARRAY_BUFFER_BINDING: GLenum = 0x889B;
    pub const EDGE_FLAG_ARRAY_POINTER: GLenum = 0x8093;
    pub const EDGE_FLAG_ARRAY_STRIDE: GLenum = 0x808C;
    pub const ELEMENT_ARRAY_BUFFER: GLenum = 0x8893;
    pub const ELEMENT_ARRAY_BUFFER_BINDING: GLenum = 0x8895;
    pub const EMISSION: GLenum = 0x1600;
    pub const ENABLE_BIT: GLenum = 0x00002000;
    pub const EQUAL: GLenum = 0x0202;
    pub const EQUIV: GLenum = 0x1509;
    pub const EVAL_BIT: GLenum = 0x00010000;
    pub const EXCLUSION_KHR: GLenum = 0x92A0;
    pub const EXP: GLenum = 0x0800;
    pub const EXP2: GLenum = 0x0801;
    pub const EXTENSIONS: GLenum = 0x1F03;
    pub const EYE_LINEAR: GLenum = 0x2400;
    pub const EYE_PLANE: GLenum = 0x2502;
    pub const FALSE: GLboolean = 0;
    pub const FASTEST: GLenum = 0x1101;
    pub const FEEDBACK: GLenum = 0x1C01;
    pub const FEEDBACK_BUFFER_POINTER: GLenum = 0x0DF0;
    pub const FEEDBACK_BUFFER_SIZE: GLenum = 0x0DF1;
    pub const FEEDBACK_BUFFER_TYPE: GLenum = 0x0DF2;
    pub const FENCE_APPLE: GLenum = 0x8A0B;
    pub const FILL: GLenum = 0x1B02;
    pub const FIRST_VERTEX_CONVENTION: GLenum = 0x8E4D;
    pub const FIXED: GLenum = 0x140C;
    pub const FIXED_ONLY: GLenum = 0x891D;
    pub const FLAT: GLenum = 0x1D00;
    pub const FLOAT: GLenum = 0x1406;
    pub const FLOAT_32_UNSIGNED_INT_24_8_REV: GLenum = 0x8DAD;
    pub const FLOAT_MAT2: GLenum = 0x8B5A;
    #[allow(non_upper_case_globals)]
    pub const FLOAT_MAT2x3: GLenum = 0x8B65;
    #[allow(non_upper_case_globals)]
    pub const FLOAT_MAT2x4: GLenum = 0x8B66;
    pub const FLOAT_MAT3: GLenum = 0x8B5B;
    #[allow(non_upper_case_globals)]
    pub const FLOAT_MAT3x2: GLenum = 0x8B67;
    #[allow(non_upper_case_globals)]
    pub const FLOAT_MAT3x4: GLenum = 0x8B68;
    pub const FLOAT_MAT4: GLenum = 0x8B5C;
    #[allow(non_upper_case_globals)]
    pub const FLOAT_MAT4x2: GLenum = 0x8B69;
    #[allow(non_upper_case_globals)]
    pub const FLOAT_MAT4x3: GLenum = 0x8B6A;
    pub const FLOAT_VEC2: GLenum = 0x8B50;
    pub const FLOAT_VEC3: GLenum = 0x8B51;
    pub const FLOAT_VEC4: GLenum = 0x8B52;
    pub const FOG: GLenum = 0x0B60;
    pub const FOG_BIT: GLenum = 0x00000080;
    pub const FOG_COLOR: GLenum = 0x0B66;
    pub const FOG_COORD: GLenum = 0x8451;
    pub const FOG_COORDINATE: GLenum = 0x8451;
    pub const FOG_COORDINATE_ARRAY: GLenum = 0x8457;
    pub const FOG_COORDINATE_ARRAY_BUFFER_BINDING: GLenum = 0x889D;
    pub const FOG_COORDINATE_ARRAY_POINTER: GLenum = 0x8456;
    pub const FOG_COORDINATE_ARRAY_STRIDE: GLenum = 0x8455;
    pub const FOG_COORDINATE_ARRAY_TYPE: GLenum = 0x8454;
    pub const FOG_COORDINATE_SOURCE: GLenum = 0x8450;
    pub const FOG_COORD_ARRAY: GLenum = 0x8457;
    pub const FOG_COORD_ARRAY_BUFFER_BINDING: GLenum = 0x889D;
    pub const FOG_COORD_ARRAY_POINTER: GLenum = 0x8456;
    pub const FOG_COORD_ARRAY_STRIDE: GLenum = 0x8455;
    pub const FOG_COORD_ARRAY_TYPE: GLenum = 0x8454;
    pub const FOG_COORD_SRC: GLenum = 0x8450;
    pub const FOG_DENSITY: GLenum = 0x0B62;
    pub const FOG_END: GLenum = 0x0B64;
    pub const FOG_HINT: GLenum = 0x0C54;
    pub const FOG_INDEX: GLenum = 0x0B61;
    pub const FOG_MODE: GLenum = 0x0B65;
    pub const FOG_START: GLenum = 0x0B63;
    pub const FRAGMENT_DEPTH: GLenum = 0x8452;
    pub const FRAGMENT_SHADER: GLenum = 0x8B30;
    pub const FRAGMENT_SHADER_DERIVATIVE_HINT: GLenum = 0x8B8B;
    pub const FRAMEBUFFER: GLenum = 0x8D40;
    pub const FRAMEBUFFER_ATTACHMENT_ALPHA_SIZE: GLenum = 0x8215;
    pub const FRAMEBUFFER_ATTACHMENT_ANGLE: GLenum = 0x93A3;
    pub const FRAMEBUFFER_ATTACHMENT_BLUE_SIZE: GLenum = 0x8214;
    pub const FRAMEBUFFER_ATTACHMENT_COLOR_ENCODING: GLenum = 0x8210;
    pub const FRAMEBUFFER_ATTACHMENT_COMPONENT_TYPE: GLenum = 0x8211;
    pub const FRAMEBUFFER_ATTACHMENT_DEPTH_SIZE: GLenum = 0x8216;
    pub const FRAMEBUFFER_ATTACHMENT_GREEN_SIZE: GLenum = 0x8213;
    pub const FRAMEBUFFER_ATTACHMENT_LAYERED: GLenum = 0x8DA7;
    pub const FRAMEBUFFER_ATTACHMENT_OBJECT_NAME: GLenum = 0x8CD1;
    pub const FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE: GLenum = 0x8CD0;
    pub const FRAMEBUFFER_ATTACHMENT_RED_SIZE: GLenum = 0x8212;
    pub const FRAMEBUFFER_ATTACHMENT_STENCIL_SIZE: GLenum = 0x8217;
    pub const FRAMEBUFFER_ATTACHMENT_TEXTURE_CUBE_MAP_FACE: GLenum = 0x8CD3;
    pub const FRAMEBUFFER_ATTACHMENT_TEXTURE_LAYER: GLenum = 0x8CD4;
    pub const FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL: GLenum = 0x8CD2;
    pub const FRAMEBUFFER_BINDING: GLenum = 0x8CA6;
    pub const FRAMEBUFFER_COMPLETE: GLenum = 0x8CD5;
    pub const FRAMEBUFFER_DEFAULT: GLenum = 0x8218;
    pub const FRAMEBUFFER_INCOMPLETE_ATTACHMENT: GLenum = 0x8CD6;
    pub const FRAMEBUFFER_INCOMPLETE_DIMENSIONS: GLenum = 0x8CD9;
    pub const FRAMEBUFFER_INCOMPLETE_DRAW_BUFFER: GLenum = 0x8CDB;
    pub const FRAMEBUFFER_INCOMPLETE_LAYER_TARGETS: GLenum = 0x8DA8;
    pub const FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT: GLenum = 0x8CD7;
    pub const FRAMEBUFFER_INCOMPLETE_MULTISAMPLE: GLenum = 0x8D56;
    pub const FRAMEBUFFER_INCOMPLETE_READ_BUFFER: GLenum = 0x8CDC;
    pub const FRAMEBUFFER_SRGB: GLenum = 0x8DB9;
    pub const FRAMEBUFFER_UNDEFINED: GLenum = 0x8219;
    pub const FRAMEBUFFER_UNSUPPORTED: GLenum = 0x8CDD;
    pub const FRONT: GLenum = 0x0404;
    pub const FRONT_AND_BACK: GLenum = 0x0408;
    pub const FRONT_FACE: GLenum = 0x0B46;
    pub const FRONT_LEFT: GLenum = 0x0400;
    pub const FRONT_RIGHT: GLenum = 0x0401;
    pub const FUNC_ADD: GLenum = 0x8006;
    pub const FUNC_REVERSE_SUBTRACT: GLenum = 0x800B;
    pub const FUNC_SUBTRACT: GLenum = 0x800A;
    pub const GENERATE_MIPMAP: GLenum = 0x8191;
    pub const GENERATE_MIPMAP_HINT: GLenum = 0x8192;
    pub const GEOMETRY_INPUT_TYPE: GLenum = 0x8917;
    pub const GEOMETRY_OUTPUT_TYPE: GLenum = 0x8918;
    pub const GEOMETRY_SHADER: GLenum = 0x8DD9;
    pub const GEOMETRY_VERTICES_OUT: GLenum = 0x8916;
    pub const GEQUAL: GLenum = 0x0206;
    pub const GPU_DISJOINT_EXT: GLenum = 0x8FBB;
    pub const GREATER: GLenum = 0x0204;
    pub const GREEN: GLenum = 0x1904;
    pub const GREEN_BIAS: GLenum = 0x0D19;
    pub const GREEN_BITS: GLenum = 0x0D53;
    pub const GREEN_INTEGER: GLenum = 0x8D95;
    pub const GREEN_SCALE: GLenum = 0x0D18;
    pub const HALF_FLOAT: GLenum = 0x140B;
    pub const HALF_FLOAT_OES: GLenum = 0x8D61;
    pub const HARDLIGHT_KHR: GLenum = 0x929B;
    pub const HIGH_FLOAT: GLenum = 0x8DF2;
    pub const HIGH_INT: GLenum = 0x8DF5;
    pub const HINT_BIT: GLenum = 0x00008000;
    pub const HSL_COLOR_KHR: GLenum = 0x92AF;
    pub const HSL_HUE_KHR: GLenum = 0x92AD;
    pub const HSL_LUMINOSITY_KHR: GLenum = 0x92B0;
    pub const HSL_SATURATION_KHR: GLenum = 0x92AE;
    pub const IMPLEMENTATION_COLOR_READ_FORMAT: GLenum = 0x8B9B;
    pub const IMPLEMENTATION_COLOR_READ_TYPE: GLenum = 0x8B9A;
    pub const INCR: GLenum = 0x1E02;
    pub const INCR_WRAP: GLenum = 0x8507;
    pub const INDEX: GLenum = 0x8222;
    pub const INDEX_ARRAY: GLenum = 0x8077;
    pub const INDEX_ARRAY_BUFFER_BINDING: GLenum = 0x8899;
    pub const INDEX_ARRAY_POINTER: GLenum = 0x8091;
    pub const INDEX_ARRAY_STRIDE: GLenum = 0x8086;
    pub const INDEX_ARRAY_TYPE: GLenum = 0x8085;
    pub const INDEX_BITS: GLenum = 0x0D51;
    pub const INDEX_CLEAR_VALUE: GLenum = 0x0C20;
    pub const INDEX_LOGIC_OP: GLenum = 0x0BF1;
    pub const INDEX_MODE: GLenum = 0x0C30;
    pub const INDEX_OFFSET: GLenum = 0x0D13;
    pub const INDEX_SHIFT: GLenum = 0x0D12;
    pub const INDEX_WRITEMASK: GLenum = 0x0C21;
    pub const INFO_LOG_LENGTH: GLenum = 0x8B84;
    pub const INT: GLenum = 0x1404;
    pub const INTENSITY: GLenum = 0x8049;
    pub const INTENSITY12: GLenum = 0x804C;
    pub const INTENSITY16: GLenum = 0x804D;
    pub const INTENSITY4: GLenum = 0x804A;
    pub const INTENSITY8: GLenum = 0x804B;
    pub const INTERLEAVED_ATTRIBS: GLenum = 0x8C8C;
    pub const INTERPOLATE: GLenum = 0x8575;
    pub const INT_2_10_10_10_REV: GLenum = 0x8D9F;
    pub const INT_SAMPLER_1D: GLenum = 0x8DC9;
    pub const INT_SAMPLER_1D_ARRAY: GLenum = 0x8DCE;
    pub const INT_SAMPLER_2D: GLenum = 0x8DCA;
    pub const INT_SAMPLER_2D_ARRAY: GLenum = 0x8DCF;
    pub const INT_SAMPLER_2D_MULTISAMPLE: GLenum = 0x9109;
    pub const INT_SAMPLER_2D_MULTISAMPLE_ARRAY: GLenum = 0x910C;
    pub const INT_SAMPLER_2D_RECT: GLenum = 0x8DCD;
    pub const INT_SAMPLER_3D: GLenum = 0x8DCB;
    pub const INT_SAMPLER_BUFFER: GLenum = 0x8DD0;
    pub const INT_SAMPLER_CUBE: GLenum = 0x8DCC;
    pub const INT_VEC2: GLenum = 0x8B53;
    pub const INT_VEC3: GLenum = 0x8B54;
    pub const INT_VEC4: GLenum = 0x8B55;
    pub const INVALID_ENUM: GLenum = 0x0500;
    pub const INVALID_FRAMEBUFFER_OPERATION: GLenum = 0x0506;
    pub const INVALID_INDEX: GLuint = 0xFFFFFFFF;
    pub const INVALID_OPERATION: GLenum = 0x0502;
    pub const INVALID_VALUE: GLenum = 0x0501;
    pub const INVERT: GLenum = 0x150A;
    pub const KEEP: GLenum = 0x1E00;
    pub const LAST_VERTEX_CONVENTION: GLenum = 0x8E4E;
    pub const LEFT: GLenum = 0x0406;
    pub const LEQUAL: GLenum = 0x0203;
    pub const LESS: GLenum = 0x0201;
    pub const LIGHT0: GLenum = 0x4000;
    pub const LIGHT1: GLenum = 0x4001;
    pub const LIGHT2: GLenum = 0x4002;
    pub const LIGHT3: GLenum = 0x4003;
    pub const LIGHT4: GLenum = 0x4004;
    pub const LIGHT5: GLenum = 0x4005;
    pub const LIGHT6: GLenum = 0x4006;
    pub const LIGHT7: GLenum = 0x4007;
    pub const LIGHTEN_KHR: GLenum = 0x9298;
    pub const LIGHTING: GLenum = 0x0B50;
    pub const LIGHTING_BIT: GLenum = 0x00000040;
    pub const LIGHT_MODEL_AMBIENT: GLenum = 0x0B53;
    pub const LIGHT_MODEL_COLOR_CONTROL: GLenum = 0x81F8;
    pub const LIGHT_MODEL_LOCAL_VIEWER: GLenum = 0x0B51;
    pub const LIGHT_MODEL_TWO_SIDE: GLenum = 0x0B52;
    pub const LINE: GLenum = 0x1B01;
    pub const LINEAR: GLenum = 0x2601;
    pub const LINEAR_ATTENUATION: GLenum = 0x1208;
    pub const LINEAR_MIPMAP_LINEAR: GLenum = 0x2703;
    pub const LINEAR_MIPMAP_NEAREST: GLenum = 0x2701;
    pub const LINES: GLenum = 0x0001;
    pub const LINES_ADJACENCY: GLenum = 0x000A;
    pub const LINE_BIT: GLenum = 0x00000004;
    pub const LINE_LOOP: GLenum = 0x0002;
    pub const LINE_RESET_TOKEN: GLenum = 0x0707;
    pub const LINE_SMOOTH: GLenum = 0x0B20;
    pub const LINE_SMOOTH_HINT: GLenum = 0x0C52;
    pub const LINE_STIPPLE: GLenum = 0x0B24;
    pub const LINE_STIPPLE_PATTERN: GLenum = 0x0B25;
    pub const LINE_STIPPLE_REPEAT: GLenum = 0x0B26;
    pub const LINE_STRIP: GLenum = 0x0003;
    pub const LINE_STRIP_ADJACENCY: GLenum = 0x000B;
    pub const LINE_TOKEN: GLenum = 0x0702;
    pub const LINE_WIDTH: GLenum = 0x0B21;
    pub const LINE_WIDTH_GRANULARITY: GLenum = 0x0B23;
    pub const LINE_WIDTH_RANGE: GLenum = 0x0B22;
    pub const LINK_STATUS: GLenum = 0x8B82;
    pub const LIST_BASE: GLenum = 0x0B32;
    pub const LIST_BIT: GLenum = 0x00020000;
    pub const LIST_INDEX: GLenum = 0x0B33;
    pub const LIST_MODE: GLenum = 0x0B30;
    pub const LOAD: GLenum = 0x0101;
    pub const LOGIC_OP: GLenum = 0x0BF1;
    pub const LOGIC_OP_MODE: GLenum = 0x0BF0;
    pub const LOWER_LEFT: GLenum = 0x8CA1;
    pub const LOW_FLOAT: GLenum = 0x8DF0;
    pub const LOW_INT: GLenum = 0x8DF3;
    pub const LUMINANCE: GLenum = 0x1909;
    pub const LUMINANCE12: GLenum = 0x8041;
    pub const LUMINANCE12_ALPHA12: GLenum = 0x8047;
    pub const LUMINANCE12_ALPHA4: GLenum = 0x8046;
    pub const LUMINANCE16: GLenum = 0x8042;
    pub const LUMINANCE16F_EXT: GLenum = 0x881E;
    pub const LUMINANCE16_ALPHA16: GLenum = 0x8048;
    pub const LUMINANCE32F_EXT: GLenum = 0x8818;
    pub const LUMINANCE4: GLenum = 0x803F;
    pub const LUMINANCE4_ALPHA4: GLenum = 0x8043;
    pub const LUMINANCE6_ALPHA2: GLenum = 0x8044;
    pub const LUMINANCE8: GLenum = 0x8040;
    pub const LUMINANCE8_ALPHA8: GLenum = 0x8045;
    pub const LUMINANCE8_ALPHA8_EXT: GLenum = 0x8045;
    pub const LUMINANCE8_EXT: GLenum = 0x8040;
    pub const LUMINANCE_ALPHA: GLenum = 0x190A;
    pub const LUMINANCE_ALPHA16F_EXT: GLenum = 0x881F;
    pub const LUMINANCE_ALPHA32F_EXT: GLenum = 0x8819;
    pub const MAJOR_VERSION: GLenum = 0x821B;
    pub const MAP1_COLOR_4: GLenum = 0x0D90;
    pub const MAP1_GRID_DOMAIN: GLenum = 0x0DD0;
    pub const MAP1_GRID_SEGMENTS: GLenum = 0x0DD1;
    pub const MAP1_INDEX: GLenum = 0x0D91;
    pub const MAP1_NORMAL: GLenum = 0x0D92;
    pub const MAP1_TEXTURE_COORD_1: GLenum = 0x0D93;
    pub const MAP1_TEXTURE_COORD_2: GLenum = 0x0D94;
    pub const MAP1_TEXTURE_COORD_3: GLenum = 0x0D95;
    pub const MAP1_TEXTURE_COORD_4: GLenum = 0x0D96;
    pub const MAP1_VERTEX_3: GLenum = 0x0D97;
    pub const MAP1_VERTEX_4: GLenum = 0x0D98;
    pub const MAP2_COLOR_4: GLenum = 0x0DB0;
    pub const MAP2_GRID_DOMAIN: GLenum = 0x0DD2;
    pub const MAP2_GRID_SEGMENTS: GLenum = 0x0DD3;
    pub const MAP2_INDEX: GLenum = 0x0DB1;
    pub const MAP2_NORMAL: GLenum = 0x0DB2;
    pub const MAP2_TEXTURE_COORD_1: GLenum = 0x0DB3;
    pub const MAP2_TEXTURE_COORD_2: GLenum = 0x0DB4;
    pub const MAP2_TEXTURE_COORD_3: GLenum = 0x0DB5;
    pub const MAP2_TEXTURE_COORD_4: GLenum = 0x0DB6;
    pub const MAP2_VERTEX_3: GLenum = 0x0DB7;
    pub const MAP2_VERTEX_4: GLenum = 0x0DB8;
    pub const MAP_COLOR: GLenum = 0x0D10;
    pub const MAP_FLUSH_EXPLICIT_BIT: GLenum = 0x0010;
    pub const MAP_INVALIDATE_BUFFER_BIT: GLenum = 0x0008;
    pub const MAP_INVALIDATE_RANGE_BIT: GLenum = 0x0004;
    pub const MAP_READ_BIT: GLenum = 0x0001;
    pub const MAP_STENCIL: GLenum = 0x0D11;
    pub const MAP_UNSYNCHRONIZED_BIT: GLenum = 0x0020;
    pub const MAP_WRITE_BIT: GLenum = 0x0002;
    pub const MATRIX_MODE: GLenum = 0x0BA0;
    pub const MAX: GLenum = 0x8008;
    pub const MAX_3D_TEXTURE_SIZE: GLenum = 0x8073;
    pub const MAX_ARRAY_TEXTURE_LAYERS: GLenum = 0x88FF;
    pub const MAX_ATTRIB_STACK_DEPTH: GLenum = 0x0D35;
    pub const MAX_CLIENT_ATTRIB_STACK_DEPTH: GLenum = 0x0D3B;
    pub const MAX_CLIP_DISTANCES: GLenum = 0x0D32;
    pub const MAX_CLIP_PLANES: GLenum = 0x0D32;
    pub const MAX_COLOR_ATTACHMENTS: GLenum = 0x8CDF;
    pub const MAX_COLOR_TEXTURE_SAMPLES: GLenum = 0x910E;
    pub const MAX_COMBINED_FRAGMENT_UNIFORM_COMPONENTS: GLenum = 0x8A33;
    pub const MAX_COMBINED_GEOMETRY_UNIFORM_COMPONENTS: GLenum = 0x8A32;
    pub const MAX_COMBINED_TEXTURE_IMAGE_UNITS: GLenum = 0x8B4D;
    pub const MAX_COMBINED_UNIFORM_BLOCKS: GLenum = 0x8A2E;
    pub const MAX_COMBINED_VERTEX_UNIFORM_COMPONENTS: GLenum = 0x8A31;
    pub const MAX_CUBE_MAP_TEXTURE_SIZE: GLenum = 0x851C;
    pub const MAX_DEBUG_GROUP_STACK_DEPTH: GLenum = 0x826C;
    pub const MAX_DEBUG_GROUP_STACK_DEPTH_KHR: GLenum = 0x826C;
    pub const MAX_DEBUG_LOGGED_MESSAGES: GLenum = 0x9144;
    pub const MAX_DEBUG_LOGGED_MESSAGES_KHR: GLenum = 0x9144;
    pub const MAX_DEBUG_MESSAGE_LENGTH: GLenum = 0x9143;
    pub const MAX_DEBUG_MESSAGE_LENGTH_KHR: GLenum = 0x9143;
    pub const MAX_DEPTH_TEXTURE_SAMPLES: GLenum = 0x910F;
    pub const MAX_DRAW_BUFFERS: GLenum = 0x8824;
    pub const MAX_DUAL_SOURCE_DRAW_BUFFERS: GLenum = 0x88FC;
    pub const MAX_ELEMENTS_INDICES: GLenum = 0x80E9;
    pub const MAX_ELEMENTS_VERTICES: GLenum = 0x80E8;
    pub const MAX_ELEMENT_INDEX: GLenum = 0x8D6B;
    pub const MAX_EVAL_ORDER: GLenum = 0x0D30;
    pub const MAX_FRAGMENT_INPUT_COMPONENTS: GLenum = 0x9125;
    pub const MAX_FRAGMENT_UNIFORM_BLOCKS: GLenum = 0x8A2D;
    pub const MAX_FRAGMENT_UNIFORM_COMPONENTS: GLenum = 0x8B49;
    pub const MAX_FRAGMENT_UNIFORM_VECTORS: GLenum = 0x8DFD;
    pub const MAX_GEOMETRY_INPUT_COMPONENTS: GLenum = 0x9123;
    pub const MAX_GEOMETRY_OUTPUT_COMPONENTS: GLenum = 0x9124;
    pub const MAX_GEOMETRY_OUTPUT_VERTICES: GLenum = 0x8DE0;
    pub const MAX_GEOMETRY_TEXTURE_IMAGE_UNITS: GLenum = 0x8C29;
    pub const MAX_GEOMETRY_TOTAL_OUTPUT_COMPONENTS: GLenum = 0x8DE1;
    pub const MAX_GEOMETRY_UNIFORM_BLOCKS: GLenum = 0x8A2C;
    pub const MAX_GEOMETRY_UNIFORM_COMPONENTS: GLenum = 0x8DDF;
    pub const MAX_INTEGER_SAMPLES: GLenum = 0x9110;
    pub const MAX_LABEL_LENGTH: GLenum = 0x82E8;
    pub const MAX_LABEL_LENGTH_KHR: GLenum = 0x82E8;
    pub const MAX_LIGHTS: GLenum = 0x0D31;
    pub const MAX_LIST_NESTING: GLenum = 0x0B31;
    pub const MAX_MODELVIEW_STACK_DEPTH: GLenum = 0x0D36;
    pub const MAX_NAME_STACK_DEPTH: GLenum = 0x0D37;
    pub const MAX_PIXEL_MAP_TABLE: GLenum = 0x0D34;
    pub const MAX_PROGRAM_TEXEL_OFFSET: GLenum = 0x8905;
    pub const MAX_PROJECTION_STACK_DEPTH: GLenum = 0x0D38;
    pub const MAX_RECTANGLE_TEXTURE_SIZE: GLenum = 0x84F8;
    pub const MAX_RECTANGLE_TEXTURE_SIZE_ARB: GLenum = 0x84F8;
    pub const MAX_RENDERBUFFER_SIZE: GLenum = 0x84E8;
    pub const MAX_SAMPLES: GLenum = 0x8D57;
    pub const MAX_SAMPLE_MASK_WORDS: GLenum = 0x8E59;
    pub const MAX_SERVER_WAIT_TIMEOUT: GLenum = 0x9111;
    pub const MAX_SHADER_PIXEL_LOCAL_STORAGE_FAST_SIZE_EXT: GLenum = 0x8F63;
    pub const MAX_SHADER_PIXEL_LOCAL_STORAGE_SIZE_EXT: GLenum = 0x8F67;
    pub const MAX_TEXTURE_BUFFER_SIZE: GLenum = 0x8C2B;
    pub const MAX_TEXTURE_COORDS: GLenum = 0x8871;
    pub const MAX_TEXTURE_IMAGE_UNITS: GLenum = 0x8872;
    pub const MAX_TEXTURE_LOD_BIAS: GLenum = 0x84FD;
    pub const MAX_TEXTURE_MAX_ANISOTROPY_EXT: GLenum = 0x84FF;
    pub const MAX_TEXTURE_SIZE: GLenum = 0x0D33;
    pub const MAX_TEXTURE_STACK_DEPTH: GLenum = 0x0D39;
    pub const MAX_TEXTURE_UNITS: GLenum = 0x84E2;
    pub const MAX_TRANSFORM_FEEDBACK_INTERLEAVED_COMPONENTS: GLenum = 0x8C8A;
    pub const MAX_TRANSFORM_FEEDBACK_SEPARATE_ATTRIBS: GLenum = 0x8C8B;
    pub const MAX_TRANSFORM_FEEDBACK_SEPARATE_COMPONENTS: GLenum = 0x8C80;
    pub const MAX_UNIFORM_BLOCK_SIZE: GLenum = 0x8A30;
    pub const MAX_UNIFORM_BUFFER_BINDINGS: GLenum = 0x8A2F;
    pub const MAX_VARYING_COMPONENTS: GLenum = 0x8B4B;
    pub const MAX_VARYING_FLOATS: GLenum = 0x8B4B;
    pub const MAX_VARYING_VECTORS: GLenum = 0x8DFC;
    pub const MAX_VERTEX_ATTRIBS: GLenum = 0x8869;
    pub const MAX_VERTEX_OUTPUT_COMPONENTS: GLenum = 0x9122;
    pub const MAX_VERTEX_TEXTURE_IMAGE_UNITS: GLenum = 0x8B4C;
    pub const MAX_VERTEX_UNIFORM_BLOCKS: GLenum = 0x8A2B;
    pub const MAX_VERTEX_UNIFORM_COMPONENTS: GLenum = 0x8B4A;
    pub const MAX_VERTEX_UNIFORM_VECTORS: GLenum = 0x8DFB;
    pub const MAX_VIEWPORT_DIMS: GLenum = 0x0D3A;
    pub const MEDIUM_FLOAT: GLenum = 0x8DF1;
    pub const MEDIUM_INT: GLenum = 0x8DF4;
    pub const MIN: GLenum = 0x8007;
    pub const MINOR_VERSION: GLenum = 0x821C;
    pub const MIN_PROGRAM_TEXEL_OFFSET: GLenum = 0x8904;
    pub const MIRRORED_REPEAT: GLenum = 0x8370;
    pub const MODELVIEW: GLenum = 0x1700;
    pub const MODELVIEW_MATRIX: GLenum = 0x0BA6;
    pub const MODELVIEW_STACK_DEPTH: GLenum = 0x0BA3;
    pub const MODULATE: GLenum = 0x2100;
    pub const MULT: GLenum = 0x0103;
    pub const MULTIPLY_KHR: GLenum = 0x9294;
    pub const MULTISAMPLE: GLenum = 0x809D;
    pub const MULTISAMPLE_BIT: GLenum = 0x20000000;
    pub const N3F_V3F: GLenum = 0x2A25;
    pub const NAME_STACK_DEPTH: GLenum = 0x0D70;
    pub const NAND: GLenum = 0x150E;
    pub const NEAREST: GLenum = 0x2600;
    pub const NEAREST_MIPMAP_LINEAR: GLenum = 0x2702;
    pub const NEAREST_MIPMAP_NEAREST: GLenum = 0x2700;
    pub const NEVER: GLenum = 0x0200;
    pub const NICEST: GLenum = 0x1102;
    pub const NONE: GLenum = 0;
    pub const NOOP: GLenum = 0x1505;
    pub const NOR: GLenum = 0x1508;
    pub const NORMALIZE: GLenum = 0x0BA1;
    pub const NORMAL_ARRAY: GLenum = 0x8075;
    pub const NORMAL_ARRAY_BUFFER_BINDING: GLenum = 0x8897;
    pub const NORMAL_ARRAY_POINTER: GLenum = 0x808F;
    pub const NORMAL_ARRAY_STRIDE: GLenum = 0x807F;
    pub const NORMAL_ARRAY_TYPE: GLenum = 0x807E;
    pub const NORMAL_MAP: GLenum = 0x8511;
    pub const NOTEQUAL: GLenum = 0x0205;
    pub const NO_ERROR: GLenum = 0;
    pub const NUM_COMPRESSED_TEXTURE_FORMATS: GLenum = 0x86A2;
    pub const NUM_EXTENSIONS: GLenum = 0x821D;
    pub const NUM_PROGRAM_BINARY_FORMATS: GLenum = 0x87FE;
    pub const NUM_SAMPLE_COUNTS: GLenum = 0x9380;
    pub const NUM_SHADER_BINARY_FORMATS: GLenum = 0x8DF9;
    pub const OBJECT_LINEAR: GLenum = 0x2401;
    pub const OBJECT_PLANE: GLenum = 0x2501;
    pub const OBJECT_TYPE: GLenum = 0x9112;
    pub const ONE: GLenum = 1;
    pub const ONE_MINUS_CONSTANT_ALPHA: GLenum = 0x8004;
    pub const ONE_MINUS_CONSTANT_COLOR: GLenum = 0x8002;
    pub const ONE_MINUS_DST_ALPHA: GLenum = 0x0305;
    pub const ONE_MINUS_DST_COLOR: GLenum = 0x0307;
    pub const ONE_MINUS_SRC1_ALPHA: GLenum = 0x88FB;
    pub const ONE_MINUS_SRC1_COLOR: GLenum = 0x88FA;
    pub const ONE_MINUS_SRC_ALPHA: GLenum = 0x0303;
    pub const ONE_MINUS_SRC_COLOR: GLenum = 0x0301;
    pub const OPERAND0_ALPHA: GLenum = 0x8598;
    pub const OPERAND0_RGB: GLenum = 0x8590;
    pub const OPERAND1_ALPHA: GLenum = 0x8599;
    pub const OPERAND1_RGB: GLenum = 0x8591;
    pub const OPERAND2_ALPHA: GLenum = 0x859A;
    pub const OPERAND2_RGB: GLenum = 0x8592;
    pub const OR: GLenum = 0x1507;
    pub const ORDER: GLenum = 0x0A01;
    pub const OR_INVERTED: GLenum = 0x150D;
    pub const OR_REVERSE: GLenum = 0x150B;
    pub const OUT_OF_MEMORY: GLenum = 0x0505;
    pub const OVERLAY_KHR: GLenum = 0x9296;
    pub const PACK_ALIGNMENT: GLenum = 0x0D05;
    pub const PACK_IMAGE_HEIGHT: GLenum = 0x806C;
    pub const PACK_LSB_FIRST: GLenum = 0x0D01;
    pub const PACK_ROW_LENGTH: GLenum = 0x0D02;
    pub const PACK_SKIP_IMAGES: GLenum = 0x806B;
    pub const PACK_SKIP_PIXELS: GLenum = 0x0D04;
    pub const PACK_SKIP_ROWS: GLenum = 0x0D03;
    pub const PACK_SWAP_BYTES: GLenum = 0x0D00;
    pub const PASS_THROUGH_TOKEN: GLenum = 0x0700;
    pub const PERSPECTIVE_CORRECTION_HINT: GLenum = 0x0C50;
    pub const PIXEL_MAP_A_TO_A: GLenum = 0x0C79;
    pub const PIXEL_MAP_A_TO_A_SIZE: GLenum = 0x0CB9;
    pub const PIXEL_MAP_B_TO_B: GLenum = 0x0C78;
    pub const PIXEL_MAP_B_TO_B_SIZE: GLenum = 0x0CB8;
    pub const PIXEL_MAP_G_TO_G: GLenum = 0x0C77;
    pub const PIXEL_MAP_G_TO_G_SIZE: GLenum = 0x0CB7;
    pub const PIXEL_MAP_I_TO_A: GLenum = 0x0C75;
    pub const PIXEL_MAP_I_TO_A_SIZE: GLenum = 0x0CB5;
    pub const PIXEL_MAP_I_TO_B: GLenum = 0x0C74;
    pub const PIXEL_MAP_I_TO_B_SIZE: GLenum = 0x0CB4;
    pub const PIXEL_MAP_I_TO_G: GLenum = 0x0C73;
    pub const PIXEL_MAP_I_TO_G_SIZE: GLenum = 0x0CB3;
    pub const PIXEL_MAP_I_TO_I: GLenum = 0x0C70;
    pub const PIXEL_MAP_I_TO_I_SIZE: GLenum = 0x0CB0;
    pub const PIXEL_MAP_I_TO_R: GLenum = 0x0C72;
    pub const PIXEL_MAP_I_TO_R_SIZE: GLenum = 0x0CB2;
    pub const PIXEL_MAP_R_TO_R: GLenum = 0x0C76;
    pub const PIXEL_MAP_R_TO_R_SIZE: GLenum = 0x0CB6;
    pub const PIXEL_MAP_S_TO_S: GLenum = 0x0C71;
    pub const PIXEL_MAP_S_TO_S_SIZE: GLenum = 0x0CB1;
    pub const PIXEL_MODE_BIT: GLenum = 0x00000020;
    pub const PIXEL_PACK_BUFFER: GLenum = 0x88EB;
    pub const PIXEL_PACK_BUFFER_BINDING: GLenum = 0x88ED;
    pub const PIXEL_UNPACK_BUFFER: GLenum = 0x88EC;
    pub const PIXEL_UNPACK_BUFFER_BINDING: GLenum = 0x88EF;
    pub const POINT: GLenum = 0x1B00;
    pub const POINTS: GLenum = 0x0000;
    pub const POINT_BIT: GLenum = 0x00000002;
    pub const POINT_DISTANCE_ATTENUATION: GLenum = 0x8129;
    pub const POINT_FADE_THRESHOLD_SIZE: GLenum = 0x8128;
    pub const POINT_SIZE: GLenum = 0x0B11;
    pub const POINT_SIZE_GRANULARITY: GLenum = 0x0B13;
    pub const POINT_SIZE_MAX: GLenum = 0x8127;
    pub const POINT_SIZE_MIN: GLenum = 0x8126;
    pub const POINT_SIZE_RANGE: GLenum = 0x0B12;
    pub const POINT_SMOOTH: GLenum = 0x0B10;
    pub const POINT_SMOOTH_HINT: GLenum = 0x0C51;
    pub const POINT_SPRITE: GLenum = 0x8861;
    pub const POINT_SPRITE_COORD_ORIGIN: GLenum = 0x8CA0;
    pub const POINT_TOKEN: GLenum = 0x0701;
    pub const POLYGON: GLenum = 0x0009;
    pub const POLYGON_BIT: GLenum = 0x00000008;
    pub const POLYGON_MODE: GLenum = 0x0B40;
    pub const POLYGON_OFFSET_FACTOR: GLenum = 0x8038;
    pub const POLYGON_OFFSET_FILL: GLenum = 0x8037;
    pub const POLYGON_OFFSET_LINE: GLenum = 0x2A02;
    pub const POLYGON_OFFSET_POINT: GLenum = 0x2A01;
    pub const POLYGON_OFFSET_UNITS: GLenum = 0x2A00;
    pub const POLYGON_SMOOTH: GLenum = 0x0B41;
    pub const POLYGON_SMOOTH_HINT: GLenum = 0x0C53;
    pub const POLYGON_STIPPLE: GLenum = 0x0B42;
    pub const POLYGON_STIPPLE_BIT: GLenum = 0x00000010;
    pub const POLYGON_TOKEN: GLenum = 0x0703;
    pub const POSITION: GLenum = 0x1203;
    pub const PREVIOUS: GLenum = 0x8578;
    pub const PRIMARY_COLOR: GLenum = 0x8577;
    pub const PRIMITIVES_GENERATED: GLenum = 0x8C87;
    pub const PRIMITIVE_RESTART: GLenum = 0x8F9D;
    pub const PRIMITIVE_RESTART_FIXED_INDEX: GLenum = 0x8D69;
    pub const PRIMITIVE_RESTART_INDEX: GLenum = 0x8F9E;
    pub const PROGRAM: GLenum = 0x82E2;
    pub const PROGRAM_BINARY_FORMATS: GLenum = 0x87FF;
    pub const PROGRAM_BINARY_LENGTH: GLenum = 0x8741;
    pub const PROGRAM_BINARY_RETRIEVABLE_HINT: GLenum = 0x8257;
    pub const PROGRAM_KHR: GLenum = 0x82E2;
    pub const PROGRAM_PIPELINE: GLenum = 0x82E4;
    pub const PROGRAM_PIPELINE_KHR: GLenum = 0x82E4;
    pub const PROGRAM_POINT_SIZE: GLenum = 0x8642;
    pub const PROJECTION: GLenum = 0x1701;
    pub const PROJECTION_MATRIX: GLenum = 0x0BA7;
    pub const PROJECTION_STACK_DEPTH: GLenum = 0x0BA4;
    pub const PROVOKING_VERTEX: GLenum = 0x8E4F;
    pub const PROXY_TEXTURE_1D: GLenum = 0x8063;
    pub const PROXY_TEXTURE_1D_ARRAY: GLenum = 0x8C19;
    pub const PROXY_TEXTURE_2D: GLenum = 0x8064;
    pub const PROXY_TEXTURE_2D_ARRAY: GLenum = 0x8C1B;
    pub const PROXY_TEXTURE_2D_MULTISAMPLE: GLenum = 0x9101;
    pub const PROXY_TEXTURE_2D_MULTISAMPLE_ARRAY: GLenum = 0x9103;
    pub const PROXY_TEXTURE_3D: GLenum = 0x8070;
    pub const PROXY_TEXTURE_CUBE_MAP: GLenum = 0x851B;
    pub const PROXY_TEXTURE_RECTANGLE: GLenum = 0x84F7;
    pub const PROXY_TEXTURE_RECTANGLE_ARB: GLenum = 0x84F7;
    pub const Q: GLenum = 0x2003;
    pub const QUADRATIC_ATTENUATION: GLenum = 0x1209;
    pub const QUADS: GLenum = 0x0007;
    pub const QUADS_FOLLOW_PROVOKING_VERTEX_CONVENTION: GLenum = 0x8E4C;
    pub const QUAD_STRIP: GLenum = 0x0008;
    pub const QUERY: GLenum = 0x82E3;
    pub const QUERY_BY_REGION_NO_WAIT: GLenum = 0x8E16;
    pub const QUERY_BY_REGION_WAIT: GLenum = 0x8E15;
    pub const QUERY_COUNTER_BITS: GLenum = 0x8864;
    pub const QUERY_COUNTER_BITS_EXT: GLenum = 0x8864;
    pub const QUERY_KHR: GLenum = 0x82E3;
    pub const QUERY_NO_WAIT: GLenum = 0x8E14;
    pub const QUERY_RESULT: GLenum = 0x8866;
    pub const QUERY_RESULT_AVAILABLE: GLenum = 0x8867;
    pub const QUERY_RESULT_AVAILABLE_EXT: GLenum = 0x8867;
    pub const QUERY_RESULT_EXT: GLenum = 0x8866;
    pub const QUERY_WAIT: GLenum = 0x8E13;
    pub const R: GLenum = 0x2002;
    pub const R11F_G11F_B10F: GLenum = 0x8C3A;
    pub const R16: GLenum = 0x822A;
    pub const R16F: GLenum = 0x822D;
    pub const R16F_EXT: GLenum = 0x822D;
    pub const R16I: GLenum = 0x8233;
    pub const R16UI: GLenum = 0x8234;
    pub const R16_SNORM: GLenum = 0x8F98;
    pub const R32F: GLenum = 0x822E;
    pub const R32F_EXT: GLenum = 0x822E;
    pub const R32I: GLenum = 0x8235;
    pub const R32UI: GLenum = 0x8236;
    pub const R3_G3_B2: GLenum = 0x2A10;
    pub const R8: GLenum = 0x8229;
    pub const R8I: GLenum = 0x8231;
    pub const R8UI: GLenum = 0x8232;
    pub const R8_EXT: GLenum = 0x8229;
    pub const R8_SNORM: GLenum = 0x8F94;
    pub const RASTERIZER_DISCARD: GLenum = 0x8C89;
    pub const READ_BUFFER: GLenum = 0x0C02;
    pub const READ_FRAMEBUFFER: GLenum = 0x8CA8;
    pub const READ_FRAMEBUFFER_BINDING: GLenum = 0x8CAA;
    pub const READ_ONLY: GLenum = 0x88B8;
    pub const READ_WRITE: GLenum = 0x88BA;
    pub const RED: GLenum = 0x1903;
    pub const RED_BIAS: GLenum = 0x0D15;
    pub const RED_BITS: GLenum = 0x0D52;
    pub const RED_INTEGER: GLenum = 0x8D94;
    pub const RED_SCALE: GLenum = 0x0D14;
    pub const REFLECTION_MAP: GLenum = 0x8512;
    pub const RENDER: GLenum = 0x1C00;
    pub const RENDERBUFFER: GLenum = 0x8D41;
    pub const RENDERBUFFER_ALPHA_SIZE: GLenum = 0x8D53;
    pub const RENDERBUFFER_BINDING: GLenum = 0x8CA7;
    pub const RENDERBUFFER_BLUE_SIZE: GLenum = 0x8D52;
    pub const RENDERBUFFER_DEPTH_SIZE: GLenum = 0x8D54;
    pub const RENDERBUFFER_GREEN_SIZE: GLenum = 0x8D51;
    pub const RENDERBUFFER_HEIGHT: GLenum = 0x8D43;
    pub const RENDERBUFFER_INTERNAL_FORMAT: GLenum = 0x8D44;
    pub const RENDERBUFFER_RED_SIZE: GLenum = 0x8D50;
    pub const RENDERBUFFER_SAMPLES: GLenum = 0x8CAB;
    pub const RENDERBUFFER_STENCIL_SIZE: GLenum = 0x8D55;
    pub const RENDERBUFFER_WIDTH: GLenum = 0x8D42;
    pub const RENDERER: GLenum = 0x1F01;
    pub const RENDER_MODE: GLenum = 0x0C40;
    pub const REPEAT: GLenum = 0x2901;
    pub const REPLACE: GLenum = 0x1E01;
    pub const REQUIRED_TEXTURE_IMAGE_UNITS_OES: GLenum = 0x8D68;
    pub const RESCALE_NORMAL: GLenum = 0x803A;
    pub const RETURN: GLenum = 0x0102;
    pub const RG: GLenum = 0x8227;
    pub const RG16: GLenum = 0x822C;
    pub const RG16F: GLenum = 0x822F;
    pub const RG16F_EXT: GLenum = 0x822F;
    pub const RG16I: GLenum = 0x8239;
    pub const RG16UI: GLenum = 0x823A;
    pub const RG16_SNORM: GLenum = 0x8F99;
    pub const RG32F: GLenum = 0x8230;
    pub const RG32F_EXT: GLenum = 0x8230;
    pub const RG32I: GLenum = 0x823B;
    pub const RG32UI: GLenum = 0x823C;
    pub const RG8: GLenum = 0x822B;
    pub const RG8I: GLenum = 0x8237;
    pub const RG8UI: GLenum = 0x8238;
    pub const RG8_EXT: GLenum = 0x822B;
    pub const RG8_SNORM: GLenum = 0x8F95;
    pub const RGB: GLenum = 0x1907;
    pub const RGB10: GLenum = 0x8052;
    pub const RGB10_A2: GLenum = 0x8059;
    pub const RGB10_A2UI: GLenum = 0x906F;
    pub const RGB10_A2_EXT: GLenum = 0x8059;
    pub const RGB10_EXT: GLenum = 0x8052;
    pub const RGB12: GLenum = 0x8053;
    pub const RGB16: GLenum = 0x8054;
    pub const RGB16F: GLenum = 0x881B;
    pub const RGB16F_EXT: GLenum = 0x881B;
    pub const RGB16I: GLenum = 0x8D89;
    pub const RGB16UI: GLenum = 0x8D77;
    pub const RGB16_SNORM: GLenum = 0x8F9A;
    pub const RGB32F: GLenum = 0x8815;
    pub const RGB32F_EXT: GLenum = 0x8815;
    pub const RGB32I: GLenum = 0x8D83;
    pub const RGB32UI: GLenum = 0x8D71;
    pub const RGB4: GLenum = 0x804F;
    pub const RGB5: GLenum = 0x8050;
    pub const RGB565: GLenum = 0x8D62;
    pub const RGB5_A1: GLenum = 0x8057;
    pub const RGB8: GLenum = 0x8051;
    pub const RGB8I: GLenum = 0x8D8F;
    pub const RGB8UI: GLenum = 0x8D7D;
    pub const RGB8_SNORM: GLenum = 0x8F96;
    pub const RGB9_E5: GLenum = 0x8C3D;
    pub const RGBA: GLenum = 0x1908;
    pub const RGBA12: GLenum = 0x805A;
    pub const RGBA16: GLenum = 0x805B;
    pub const RGBA16F: GLenum = 0x881A;
    pub const RGBA16F_EXT: GLenum = 0x881A;
    pub const RGBA16I: GLenum = 0x8D88;
    pub const RGBA16UI: GLenum = 0x8D76;
    pub const RGBA16_SNORM: GLenum = 0x8F9B;
    pub const RGBA2: GLenum = 0x8055;
    pub const RGBA32F: GLenum = 0x8814;
    pub const RGBA32F_EXT: GLenum = 0x8814;
    pub const RGBA32I: GLenum = 0x8D82;
    pub const RGBA32UI: GLenum = 0x8D70;
    pub const RGBA4: GLenum = 0x8056;
    pub const RGBA8: GLenum = 0x8058;
    pub const RGBA8I: GLenum = 0x8D8E;
    pub const RGBA8UI: GLenum = 0x8D7C;
    pub const RGBA8_SNORM: GLenum = 0x8F97;
    pub const RGBA_INTEGER: GLenum = 0x8D99;
    pub const RGBA_MODE: GLenum = 0x0C31;
    pub const RGB_INTEGER: GLenum = 0x8D98;
    pub const RGB_SCALE: GLenum = 0x8573;
    pub const RG_INTEGER: GLenum = 0x8228;
    pub const RIGHT: GLenum = 0x0407;
    pub const S: GLenum = 0x2000;
    pub const SAMPLER: GLenum = 0x82E6;
    pub const SAMPLER_1D: GLenum = 0x8B5D;
    pub const SAMPLER_1D_ARRAY: GLenum = 0x8DC0;
    pub const SAMPLER_1D_ARRAY_SHADOW: GLenum = 0x8DC3;
    pub const SAMPLER_1D_SHADOW: GLenum = 0x8B61;
    pub const SAMPLER_2D: GLenum = 0x8B5E;
    pub const SAMPLER_2D_ARRAY: GLenum = 0x8DC1;
    pub const SAMPLER_2D_ARRAY_SHADOW: GLenum = 0x8DC4;
    pub const SAMPLER_2D_MULTISAMPLE: GLenum = 0x9108;
    pub const SAMPLER_2D_MULTISAMPLE_ARRAY: GLenum = 0x910B;
    pub const SAMPLER_2D_RECT: GLenum = 0x8B63;
    pub const SAMPLER_2D_RECT_SHADOW: GLenum = 0x8B64;
    pub const SAMPLER_2D_SHADOW: GLenum = 0x8B62;
    pub const SAMPLER_3D: GLenum = 0x8B5F;
    pub const SAMPLER_BINDING: GLenum = 0x8919;
    pub const SAMPLER_BUFFER: GLenum = 0x8DC2;
    pub const SAMPLER_CUBE: GLenum = 0x8B60;
    pub const SAMPLER_CUBE_SHADOW: GLenum = 0x8DC5;
    pub const SAMPLER_EXTERNAL_OES: GLenum = 0x8D66;
    pub const SAMPLER_KHR: GLenum = 0x82E6;
    pub const SAMPLES: GLenum = 0x80A9;
    pub const SAMPLES_PASSED: GLenum = 0x8914;
    pub const SAMPLE_ALPHA_TO_COVERAGE: GLenum = 0x809E;
    pub const SAMPLE_ALPHA_TO_ONE: GLenum = 0x809F;
    pub const SAMPLE_BUFFERS: GLenum = 0x80A8;
    pub const SAMPLE_COVERAGE: GLenum = 0x80A0;
    pub const SAMPLE_COVERAGE_INVERT: GLenum = 0x80AB;
    pub const SAMPLE_COVERAGE_VALUE: GLenum = 0x80AA;
    pub const SAMPLE_MASK: GLenum = 0x8E51;
    pub const SAMPLE_MASK_VALUE: GLenum = 0x8E52;
    pub const SAMPLE_POSITION: GLenum = 0x8E50;
    pub const SCISSOR_BIT: GLenum = 0x00080000;
    pub const SCISSOR_BOX: GLenum = 0x0C10;
    pub const SCISSOR_TEST: GLenum = 0x0C11;
    pub const SCREEN_KHR: GLenum = 0x9295;
    pub const SECONDARY_COLOR_ARRAY: GLenum = 0x845E;
    pub const SECONDARY_COLOR_ARRAY_BUFFER_BINDING: GLenum = 0x889C;
    pub const SECONDARY_COLOR_ARRAY_POINTER: GLenum = 0x845D;
    pub const SECONDARY_COLOR_ARRAY_SIZE: GLenum = 0x845A;
    pub const SECONDARY_COLOR_ARRAY_STRIDE: GLenum = 0x845C;
    pub const SECONDARY_COLOR_ARRAY_TYPE: GLenum = 0x845B;
    pub const SELECT: GLenum = 0x1C02;
    pub const SELECTION_BUFFER_POINTER: GLenum = 0x0DF3;
    pub const SELECTION_BUFFER_SIZE: GLenum = 0x0DF4;
    pub const SEPARATE_ATTRIBS: GLenum = 0x8C8D;
    pub const SEPARATE_SPECULAR_COLOR: GLenum = 0x81FA;
    pub const SET: GLenum = 0x150F;
    pub const SHADER: GLenum = 0x82E1;
    pub const SHADER_BINARY_FORMATS: GLenum = 0x8DF8;
    pub const SHADER_COMPILER: GLenum = 0x8DFA;
    pub const SHADER_KHR: GLenum = 0x82E1;
    pub const SHADER_PIXEL_LOCAL_STORAGE_EXT: GLenum = 0x8F64;
    pub const SHADER_SOURCE_LENGTH: GLenum = 0x8B88;
    pub const SHADER_TYPE: GLenum = 0x8B4F;
    pub const SHADE_MODEL: GLenum = 0x0B54;
    pub const SHADING_LANGUAGE_VERSION: GLenum = 0x8B8C;
    pub const SHININESS: GLenum = 0x1601;
    pub const SHORT: GLenum = 0x1402;
    pub const SIGNALED: GLenum = 0x9119;
    pub const SIGNED_NORMALIZED: GLenum = 0x8F9C;
    pub const SINGLE_COLOR: GLenum = 0x81F9;
    pub const SLUMINANCE: GLenum = 0x8C46;
    pub const SLUMINANCE8: GLenum = 0x8C47;
    pub const SLUMINANCE8_ALPHA8: GLenum = 0x8C45;
    pub const SLUMINANCE_ALPHA: GLenum = 0x8C44;
    pub const SMOOTH: GLenum = 0x1D01;
    pub const SMOOTH_LINE_WIDTH_GRANULARITY: GLenum = 0x0B23;
    pub const SMOOTH_LINE_WIDTH_RANGE: GLenum = 0x0B22;
    pub const SMOOTH_POINT_SIZE_GRANULARITY: GLenum = 0x0B13;
    pub const SMOOTH_POINT_SIZE_RANGE: GLenum = 0x0B12;
    pub const SOFTLIGHT_KHR: GLenum = 0x929C;
    pub const SOURCE0_ALPHA: GLenum = 0x8588;
    pub const SOURCE0_RGB: GLenum = 0x8580;
    pub const SOURCE1_ALPHA: GLenum = 0x8589;
    pub const SOURCE1_RGB: GLenum = 0x8581;
    pub const SOURCE2_ALPHA: GLenum = 0x858A;
    pub const SOURCE2_RGB: GLenum = 0x8582;
    pub const SPECULAR: GLenum = 0x1202;
    pub const SPHERE_MAP: GLenum = 0x2402;
    pub const SPOT_CUTOFF: GLenum = 0x1206;
    pub const SPOT_DIRECTION: GLenum = 0x1204;
    pub const SPOT_EXPONENT: GLenum = 0x1205;
    pub const SRC0_ALPHA: GLenum = 0x8588;
    pub const SRC0_RGB: GLenum = 0x8580;
    pub const SRC1_ALPHA: GLenum = 0x8589;
    pub const SRC1_COLOR: GLenum = 0x88F9;
    pub const SRC1_RGB: GLenum = 0x8581;
    pub const SRC2_ALPHA: GLenum = 0x858A;
    pub const SRC2_RGB: GLenum = 0x8582;
    pub const SRC_ALPHA: GLenum = 0x0302;
    pub const SRC_ALPHA_SATURATE: GLenum = 0x0308;
    pub const SRC_COLOR: GLenum = 0x0300;
    pub const SRGB: GLenum = 0x8C40;
    pub const SRGB8: GLenum = 0x8C41;
    pub const SRGB8_ALPHA8: GLenum = 0x8C43;
    pub const SRGB_ALPHA: GLenum = 0x8C42;
    pub const STACK_OVERFLOW: GLenum = 0x0503;
    pub const STACK_OVERFLOW_KHR: GLenum = 0x0503;
    pub const STACK_UNDERFLOW: GLenum = 0x0504;
    pub const STACK_UNDERFLOW_KHR: GLenum = 0x0504;
    pub const STATIC_COPY: GLenum = 0x88E6;
    pub const STATIC_DRAW: GLenum = 0x88E4;
    pub const STATIC_READ: GLenum = 0x88E5;
    pub const STENCIL: GLenum = 0x1802;
    pub const STENCIL_ATTACHMENT: GLenum = 0x8D20;
    pub const STENCIL_BACK_FAIL: GLenum = 0x8801;
    pub const STENCIL_BACK_FUNC: GLenum = 0x8800;
    pub const STENCIL_BACK_PASS_DEPTH_FAIL: GLenum = 0x8802;
    pub const STENCIL_BACK_PASS_DEPTH_PASS: GLenum = 0x8803;
    pub const STENCIL_BACK_REF: GLenum = 0x8CA3;
    pub const STENCIL_BACK_VALUE_MASK: GLenum = 0x8CA4;
    pub const STENCIL_BACK_WRITEMASK: GLenum = 0x8CA5;
    pub const STENCIL_BITS: GLenum = 0x0D57;
    pub const STENCIL_BUFFER_BIT: GLenum = 0x00000400;
    pub const STENCIL_CLEAR_VALUE: GLenum = 0x0B91;
    pub const STENCIL_FAIL: GLenum = 0x0B94;
    pub const STENCIL_FUNC: GLenum = 0x0B92;
    pub const STENCIL_INDEX: GLenum = 0x1901;
    pub const STENCIL_INDEX1: GLenum = 0x8D46;
    pub const STENCIL_INDEX16: GLenum = 0x8D49;
    pub const STENCIL_INDEX4: GLenum = 0x8D47;
    pub const STENCIL_INDEX8: GLenum = 0x8D48;
    pub const STENCIL_PASS_DEPTH_FAIL: GLenum = 0x0B95;
    pub const STENCIL_PASS_DEPTH_PASS: GLenum = 0x0B96;
    pub const STENCIL_REF: GLenum = 0x0B97;
    pub const STENCIL_TEST: GLenum = 0x0B90;
    pub const STENCIL_VALUE_MASK: GLenum = 0x0B93;
    pub const STENCIL_WRITEMASK: GLenum = 0x0B98;
    pub const STEREO: GLenum = 0x0C33;
    pub const STORAGE_CACHED_APPLE: GLenum = 0x85BE;
    pub const STORAGE_PRIVATE_APPLE: GLenum = 0x85BD;
    pub const STORAGE_SHARED_APPLE: GLenum = 0x85BF;
    pub const STREAM_COPY: GLenum = 0x88E2;
    pub const STREAM_DRAW: GLenum = 0x88E0;
    pub const STREAM_READ: GLenum = 0x88E1;
    pub const SUBPIXEL_BITS: GLenum = 0x0D50;
    pub const SUBTRACT: GLenum = 0x84E7;
    pub const SYNC_CONDITION: GLenum = 0x9113;
    pub const SYNC_FENCE: GLenum = 0x9116;
    pub const SYNC_FLAGS: GLenum = 0x9115;
    pub const SYNC_FLUSH_COMMANDS_BIT: GLenum = 0x00000001;
    pub const SYNC_GPU_COMMANDS_COMPLETE: GLenum = 0x9117;
    pub const SYNC_STATUS: GLenum = 0x9114;
    pub const T: GLenum = 0x2001;
    pub const T2F_C3F_V3F: GLenum = 0x2A2A;
    pub const T2F_C4F_N3F_V3F: GLenum = 0x2A2C;
    pub const T2F_C4UB_V3F: GLenum = 0x2A29;
    pub const T2F_N3F_V3F: GLenum = 0x2A2B;
    pub const T2F_V3F: GLenum = 0x2A27;
    pub const T4F_C4F_N3F_V4F: GLenum = 0x2A2D;
    pub const T4F_V4F: GLenum = 0x2A28;
    pub const TEXTURE: GLenum = 0x1702;
    pub const TEXTURE0: GLenum = 0x84C0;
    pub const TEXTURE1: GLenum = 0x84C1;
    pub const TEXTURE10: GLenum = 0x84CA;
    pub const TEXTURE11: GLenum = 0x84CB;
    pub const TEXTURE12: GLenum = 0x84CC;
    pub const TEXTURE13: GLenum = 0x84CD;
    pub const TEXTURE14: GLenum = 0x84CE;
    pub const TEXTURE15: GLenum = 0x84CF;
    pub const TEXTURE16: GLenum = 0x84D0;
    pub const TEXTURE17: GLenum = 0x84D1;
    pub const TEXTURE18: GLenum = 0x84D2;
    pub const TEXTURE19: GLenum = 0x84D3;
    pub const TEXTURE2: GLenum = 0x84C2;
    pub const TEXTURE20: GLenum = 0x84D4;
    pub const TEXTURE21: GLenum = 0x84D5;
    pub const TEXTURE22: GLenum = 0x84D6;
    pub const TEXTURE23: GLenum = 0x84D7;
    pub const TEXTURE24: GLenum = 0x84D8;
    pub const TEXTURE25: GLenum = 0x84D9;
    pub const TEXTURE26: GLenum = 0x84DA;
    pub const TEXTURE27: GLenum = 0x84DB;
    pub const TEXTURE28: GLenum = 0x84DC;
    pub const TEXTURE29: GLenum = 0x84DD;
    pub const TEXTURE3: GLenum = 0x84C3;
    pub const TEXTURE30: GLenum = 0x84DE;
    pub const TEXTURE31: GLenum = 0x84DF;
    pub const TEXTURE4: GLenum = 0x84C4;
    pub const TEXTURE5: GLenum = 0x84C5;
    pub const TEXTURE6: GLenum = 0x84C6;
    pub const TEXTURE7: GLenum = 0x84C7;
    pub const TEXTURE8: GLenum = 0x84C8;
    pub const TEXTURE9: GLenum = 0x84C9;
    pub const TEXTURE_1D: GLenum = 0x0DE0;
    pub const TEXTURE_1D_ARRAY: GLenum = 0x8C18;
    pub const TEXTURE_2D: GLenum = 0x0DE1;
    pub const TEXTURE_2D_ARRAY: GLenum = 0x8C1A;
    pub const TEXTURE_2D_MULTISAMPLE: GLenum = 0x9100;
    pub const TEXTURE_2D_MULTISAMPLE_ARRAY: GLenum = 0x9102;
    pub const TEXTURE_3D: GLenum = 0x806F;
    pub const TEXTURE_ALPHA_SIZE: GLenum = 0x805F;
    pub const TEXTURE_ALPHA_TYPE: GLenum = 0x8C13;
    pub const TEXTURE_BASE_LEVEL: GLenum = 0x813C;
    pub const TEXTURE_BINDING_1D: GLenum = 0x8068;
    pub const TEXTURE_BINDING_1D_ARRAY: GLenum = 0x8C1C;
    pub const TEXTURE_BINDING_2D: GLenum = 0x8069;
    pub const TEXTURE_BINDING_2D_ARRAY: GLenum = 0x8C1D;
    pub const TEXTURE_BINDING_2D_MULTISAMPLE: GLenum = 0x9104;
    pub const TEXTURE_BINDING_2D_MULTISAMPLE_ARRAY: GLenum = 0x9105;
    pub const TEXTURE_BINDING_3D: GLenum = 0x806A;
    pub const TEXTURE_BINDING_BUFFER: GLenum = 0x8C2C;
    pub const TEXTURE_BINDING_CUBE_MAP: GLenum = 0x8514;
    pub const TEXTURE_BINDING_EXTERNAL_OES: GLenum = 0x8D67;
    pub const TEXTURE_BINDING_RECTANGLE: GLenum = 0x84F6;
    pub const TEXTURE_BINDING_RECTANGLE_ARB: GLenum = 0x84F6;
    pub const TEXTURE_BIT: GLenum = 0x00040000;
    pub const TEXTURE_BLUE_SIZE: GLenum = 0x805E;
    pub const TEXTURE_BLUE_TYPE: GLenum = 0x8C12;
    pub const TEXTURE_BORDER: GLenum = 0x1005;
    pub const TEXTURE_BORDER_COLOR: GLenum = 0x1004;
    pub const TEXTURE_BUFFER: GLenum = 0x8C2A;
    pub const TEXTURE_BUFFER_DATA_STORE_BINDING: GLenum = 0x8C2D;
    pub const TEXTURE_COMPARE_FUNC: GLenum = 0x884D;
    pub const TEXTURE_COMPARE_MODE: GLenum = 0x884C;
    pub const TEXTURE_COMPONENTS: GLenum = 0x1003;
    pub const TEXTURE_COMPRESSED: GLenum = 0x86A1;
    pub const TEXTURE_COMPRESSED_IMAGE_SIZE: GLenum = 0x86A0;
    pub const TEXTURE_COMPRESSION_HINT: GLenum = 0x84EF;
    pub const TEXTURE_COORD_ARRAY: GLenum = 0x8078;
    pub const TEXTURE_COORD_ARRAY_BUFFER_BINDING: GLenum = 0x889A;
    pub const TEXTURE_COORD_ARRAY_POINTER: GLenum = 0x8092;
    pub const TEXTURE_COORD_ARRAY_SIZE: GLenum = 0x8088;
    pub const TEXTURE_COORD_ARRAY_STRIDE: GLenum = 0x808A;
    pub const TEXTURE_COORD_ARRAY_TYPE: GLenum = 0x8089;
    pub const TEXTURE_CUBE_MAP: GLenum = 0x8513;
    pub const TEXTURE_CUBE_MAP_NEGATIVE_X: GLenum = 0x8516;
    pub const TEXTURE_CUBE_MAP_NEGATIVE_Y: GLenum = 0x8518;
    pub const TEXTURE_CUBE_MAP_NEGATIVE_Z: GLenum = 0x851A;
    pub const TEXTURE_CUBE_MAP_POSITIVE_X: GLenum = 0x8515;
    pub const TEXTURE_CUBE_MAP_POSITIVE_Y: GLenum = 0x8517;
    pub const TEXTURE_CUBE_MAP_POSITIVE_Z: GLenum = 0x8519;
    pub const TEXTURE_CUBE_MAP_SEAMLESS: GLenum = 0x884F;
    pub const TEXTURE_DEPTH: GLenum = 0x8071;
    pub const TEXTURE_DEPTH_SIZE: GLenum = 0x884A;
    pub const TEXTURE_DEPTH_TYPE: GLenum = 0x8C16;
    pub const TEXTURE_ENV: GLenum = 0x2300;
    pub const TEXTURE_ENV_COLOR: GLenum = 0x2201;
    pub const TEXTURE_ENV_MODE: GLenum = 0x2200;
    pub const TEXTURE_EXTERNAL_OES: GLenum = 0x8D65;
    pub const TEXTURE_FILTER_CONTROL: GLenum = 0x8500;
    pub const TEXTURE_FIXED_SAMPLE_LOCATIONS: GLenum = 0x9107;
    pub const TEXTURE_GEN_MODE: GLenum = 0x2500;
    pub const TEXTURE_GEN_Q: GLenum = 0x0C63;
    pub const TEXTURE_GEN_R: GLenum = 0x0C62;
    pub const TEXTURE_GEN_S: GLenum = 0x0C60;
    pub const TEXTURE_GEN_T: GLenum = 0x0C61;
    pub const TEXTURE_GREEN_SIZE: GLenum = 0x805D;
    pub const TEXTURE_GREEN_TYPE: GLenum = 0x8C11;
    pub const TEXTURE_HEIGHT: GLenum = 0x1001;
    pub const TEXTURE_IMMUTABLE_FORMAT: GLenum = 0x912F;
    pub const TEXTURE_IMMUTABLE_FORMAT_EXT: GLenum = 0x912F;
    pub const TEXTURE_IMMUTABLE_LEVELS: GLenum = 0x82DF;
    pub const TEXTURE_INTENSITY_SIZE: GLenum = 0x8061;
    pub const TEXTURE_INTENSITY_TYPE: GLenum = 0x8C15;
    pub const TEXTURE_INTERNAL_FORMAT: GLenum = 0x1003;
    pub const TEXTURE_LOD_BIAS: GLenum = 0x8501;
    pub const TEXTURE_LUMINANCE_SIZE: GLenum = 0x8060;
    pub const TEXTURE_LUMINANCE_TYPE: GLenum = 0x8C14;
    pub const TEXTURE_MAG_FILTER: GLenum = 0x2800;
    pub const TEXTURE_MATRIX: GLenum = 0x0BA8;
    pub const TEXTURE_MAX_ANISOTROPY_EXT: GLenum = 0x84FE;
    pub const TEXTURE_MAX_LEVEL: GLenum = 0x813D;
    pub const TEXTURE_MAX_LOD: GLenum = 0x813B;
    pub const TEXTURE_MIN_FILTER: GLenum = 0x2801;
    pub const TEXTURE_MIN_LOD: GLenum = 0x813A;
    pub const TEXTURE_PRIORITY: GLenum = 0x8066;
    pub const TEXTURE_RANGE_LENGTH_APPLE: GLenum = 0x85B7;
    pub const TEXTURE_RANGE_POINTER_APPLE: GLenum = 0x85B8;
    pub const TEXTURE_RECTANGLE: GLenum = 0x84F5;
    pub const TEXTURE_RECTANGLE_ARB: GLenum = 0x84F5;
    pub const TEXTURE_RED_SIZE: GLenum = 0x805C;
    pub const TEXTURE_RED_TYPE: GLenum = 0x8C10;
    pub const TEXTURE_RESIDENT: GLenum = 0x8067;
    pub const TEXTURE_SAMPLES: GLenum = 0x9106;
    pub const TEXTURE_SHARED_SIZE: GLenum = 0x8C3F;
    pub const TEXTURE_STACK_DEPTH: GLenum = 0x0BA5;
    pub const TEXTURE_STENCIL_SIZE: GLenum = 0x88F1;
    pub const TEXTURE_STORAGE_HINT_APPLE: GLenum = 0x85BC;
    pub const TEXTURE_SWIZZLE_A: GLenum = 0x8E45;
    pub const TEXTURE_SWIZZLE_B: GLenum = 0x8E44;
    pub const TEXTURE_SWIZZLE_G: GLenum = 0x8E43;
    pub const TEXTURE_SWIZZLE_R: GLenum = 0x8E42;
    pub const TEXTURE_SWIZZLE_RGBA: GLenum = 0x8E46;
    pub const TEXTURE_USAGE_ANGLE: GLenum = 0x93A2;
    pub const TEXTURE_WIDTH: GLenum = 0x1000;
    pub const TEXTURE_WRAP_R: GLenum = 0x8072;
    pub const TEXTURE_WRAP_S: GLenum = 0x2802;
    pub const TEXTURE_WRAP_T: GLenum = 0x2803;
    pub const TIMEOUT_EXPIRED: GLenum = 0x911B;
    pub const TIMEOUT_IGNORED: GLuint64 = 0xFFFFFFFFFFFFFFFF;
    pub const TIMESTAMP: GLenum = 0x8E28;
    pub const TIMESTAMP_EXT: GLenum = 0x8E28;
    pub const TIME_ELAPSED: GLenum = 0x88BF;
    pub const TIME_ELAPSED_EXT: GLenum = 0x88BF;
    pub const TRANSFORM_BIT: GLenum = 0x00001000;
    pub const TRANSFORM_FEEDBACK: GLenum = 0x8E22;
    pub const TRANSFORM_FEEDBACK_ACTIVE: GLenum = 0x8E24;
    pub const TRANSFORM_FEEDBACK_BINDING: GLenum = 0x8E25;
    pub const TRANSFORM_FEEDBACK_BUFFER: GLenum = 0x8C8E;
    pub const TRANSFORM_FEEDBACK_BUFFER_BINDING: GLenum = 0x8C8F;
    pub const TRANSFORM_FEEDBACK_BUFFER_MODE: GLenum = 0x8C7F;
    pub const TRANSFORM_FEEDBACK_BUFFER_SIZE: GLenum = 0x8C85;
    pub const TRANSFORM_FEEDBACK_BUFFER_START: GLenum = 0x8C84;
    pub const TRANSFORM_FEEDBACK_PAUSED: GLenum = 0x8E23;
    pub const TRANSFORM_FEEDBACK_PRIMITIVES_WRITTEN: GLenum = 0x8C88;
    pub const TRANSFORM_FEEDBACK_VARYINGS: GLenum = 0x8C83;
    pub const TRANSFORM_FEEDBACK_VARYING_MAX_LENGTH: GLenum = 0x8C76;
    pub const TRANSPOSE_COLOR_MATRIX: GLenum = 0x84E6;
    pub const TRANSPOSE_MODELVIEW_MATRIX: GLenum = 0x84E3;
    pub const TRANSPOSE_PROJECTION_MATRIX: GLenum = 0x84E4;
    pub const TRANSPOSE_TEXTURE_MATRIX: GLenum = 0x84E5;
    pub const TRIANGLES: GLenum = 0x0004;
    pub const TRIANGLES_ADJACENCY: GLenum = 0x000C;
    pub const TRIANGLE_FAN: GLenum = 0x0006;
    pub const TRIANGLE_STRIP: GLenum = 0x0005;
    pub const TRIANGLE_STRIP_ADJACENCY: GLenum = 0x000D;
    pub const TRUE: GLboolean = 1;
    pub const UNIFORM_ARRAY_STRIDE: GLenum = 0x8A3C;
    pub const UNIFORM_BLOCK_ACTIVE_UNIFORMS: GLenum = 0x8A42;
    pub const UNIFORM_BLOCK_ACTIVE_UNIFORM_INDICES: GLenum = 0x8A43;
    pub const UNIFORM_BLOCK_BINDING: GLenum = 0x8A3F;
    pub const UNIFORM_BLOCK_DATA_SIZE: GLenum = 0x8A40;
    pub const UNIFORM_BLOCK_INDEX: GLenum = 0x8A3A;
    pub const UNIFORM_BLOCK_NAME_LENGTH: GLenum = 0x8A41;
    pub const UNIFORM_BLOCK_REFERENCED_BY_FRAGMENT_SHADER: GLenum = 0x8A46;
    pub const UNIFORM_BLOCK_REFERENCED_BY_GEOMETRY_SHADER: GLenum = 0x8A45;
    pub const UNIFORM_BLOCK_REFERENCED_BY_VERTEX_SHADER: GLenum = 0x8A44;
    pub const UNIFORM_BUFFER: GLenum = 0x8A11;
    pub const UNIFORM_BUFFER_BINDING: GLenum = 0x8A28;
    pub const UNIFORM_BUFFER_OFFSET_ALIGNMENT: GLenum = 0x8A34;
    pub const UNIFORM_BUFFER_SIZE: GLenum = 0x8A2A;
    pub const UNIFORM_BUFFER_START: GLenum = 0x8A29;
    pub const UNIFORM_IS_ROW_MAJOR: GLenum = 0x8A3E;
    pub const UNIFORM_MATRIX_STRIDE: GLenum = 0x8A3D;
    pub const UNIFORM_NAME_LENGTH: GLenum = 0x8A39;
    pub const UNIFORM_OFFSET: GLenum = 0x8A3B;
    pub const UNIFORM_SIZE: GLenum = 0x8A38;
    pub const UNIFORM_TYPE: GLenum = 0x8A37;
    pub const UNPACK_ALIGNMENT: GLenum = 0x0CF5;
    pub const UNPACK_CLIENT_STORAGE_APPLE: GLenum = 0x85B2;
    pub const UNPACK_IMAGE_HEIGHT: GLenum = 0x806E;
    pub const UNPACK_LSB_FIRST: GLenum = 0x0CF1;
    pub const UNPACK_ROW_LENGTH: GLenum = 0x0CF2;
    pub const UNPACK_SKIP_IMAGES: GLenum = 0x806D;
    pub const UNPACK_SKIP_PIXELS: GLenum = 0x0CF4;
    pub const UNPACK_SKIP_ROWS: GLenum = 0x0CF3;
    pub const UNPACK_SWAP_BYTES: GLenum = 0x0CF0;
    pub const UNSIGNALED: GLenum = 0x9118;
    pub const UNSIGNED_BYTE: GLenum = 0x1401;
    pub const UNSIGNED_BYTE_2_3_3_REV: GLenum = 0x8362;
    pub const UNSIGNED_BYTE_3_3_2: GLenum = 0x8032;
    pub const UNSIGNED_INT: GLenum = 0x1405;
    pub const UNSIGNED_INT_10F_11F_11F_REV: GLenum = 0x8C3B;
    pub const UNSIGNED_INT_10_10_10_2: GLenum = 0x8036;
    pub const UNSIGNED_INT_24_8: GLenum = 0x84FA;
    pub const UNSIGNED_INT_2_10_10_10_REV: GLenum = 0x8368;
    pub const UNSIGNED_INT_5_9_9_9_REV: GLenum = 0x8C3E;
    pub const UNSIGNED_INT_8_8_8_8: GLenum = 0x8035;
    pub const UNSIGNED_INT_8_8_8_8_REV: GLenum = 0x8367;
    pub const UNSIGNED_INT_SAMPLER_1D: GLenum = 0x8DD1;
    pub const UNSIGNED_INT_SAMPLER_1D_ARRAY: GLenum = 0x8DD6;
    pub const UNSIGNED_INT_SAMPLER_2D: GLenum = 0x8DD2;
    pub const UNSIGNED_INT_SAMPLER_2D_ARRAY: GLenum = 0x8DD7;
    pub const UNSIGNED_INT_SAMPLER_2D_MULTISAMPLE: GLenum = 0x910A;
    pub const UNSIGNED_INT_SAMPLER_2D_MULTISAMPLE_ARRAY: GLenum = 0x910D;
    pub const UNSIGNED_INT_SAMPLER_2D_RECT: GLenum = 0x8DD5;
    pub const UNSIGNED_INT_SAMPLER_3D: GLenum = 0x8DD3;
    pub const UNSIGNED_INT_SAMPLER_BUFFER: GLenum = 0x8DD8;
    pub const UNSIGNED_INT_SAMPLER_CUBE: GLenum = 0x8DD4;
    pub const UNSIGNED_INT_VEC2: GLenum = 0x8DC6;
    pub const UNSIGNED_INT_VEC3: GLenum = 0x8DC7;
    pub const UNSIGNED_INT_VEC4: GLenum = 0x8DC8;
    pub const UNSIGNED_NORMALIZED: GLenum = 0x8C17;
    pub const UNSIGNED_SHORT: GLenum = 0x1403;
    pub const UNSIGNED_SHORT_1_5_5_5_REV: GLenum = 0x8366;
    pub const UNSIGNED_SHORT_4_4_4_4: GLenum = 0x8033;
    pub const UNSIGNED_SHORT_4_4_4_4_REV: GLenum = 0x8365;
    pub const UNSIGNED_SHORT_5_5_5_1: GLenum = 0x8034;
    pub const UNSIGNED_SHORT_5_6_5: GLenum = 0x8363;
    pub const UNSIGNED_SHORT_5_6_5_REV: GLenum = 0x8364;
    pub const UPPER_LEFT: GLenum = 0x8CA2;
    pub const V2F: GLenum = 0x2A20;
    pub const V3F: GLenum = 0x2A21;
    pub const VALIDATE_STATUS: GLenum = 0x8B83;
    pub const VENDOR: GLenum = 0x1F00;
    pub const VERSION: GLenum = 0x1F02;
    pub const VERTEX_ARRAY: GLenum = 0x8074;
    pub const VERTEX_ARRAY_BINDING: GLenum = 0x85B5;
    pub const VERTEX_ARRAY_BINDING_APPLE: GLenum = 0x85B5;
    pub const VERTEX_ARRAY_BUFFER_BINDING: GLenum = 0x8896;
    pub const VERTEX_ARRAY_KHR: GLenum = 0x8074;
    pub const VERTEX_ARRAY_POINTER: GLenum = 0x808E;
    pub const VERTEX_ARRAY_SIZE: GLenum = 0x807A;
    pub const VERTEX_ARRAY_STRIDE: GLenum = 0x807C;
    pub const VERTEX_ARRAY_TYPE: GLenum = 0x807B;
    pub const VERTEX_ATTRIB_ARRAY_BUFFER_BINDING: GLenum = 0x889F;
    pub const VERTEX_ATTRIB_ARRAY_DIVISOR: GLenum = 0x88FE;
    pub const VERTEX_ATTRIB_ARRAY_ENABLED: GLenum = 0x8622;
    pub const VERTEX_ATTRIB_ARRAY_INTEGER: GLenum = 0x88FD;
    pub const VERTEX_ATTRIB_ARRAY_NORMALIZED: GLenum = 0x886A;
    pub const VERTEX_ATTRIB_ARRAY_POINTER: GLenum = 0x8645;
    pub const VERTEX_ATTRIB_ARRAY_SIZE: GLenum = 0x8623;
    pub const VERTEX_ATTRIB_ARRAY_STRIDE: GLenum = 0x8624;
    pub const VERTEX_ATTRIB_ARRAY_TYPE: GLenum = 0x8625;
    pub const VERTEX_PROGRAM_POINT_SIZE: GLenum = 0x8642;
    pub const VERTEX_PROGRAM_TWO_SIDE: GLenum = 0x8643;
    pub const VERTEX_SHADER: GLenum = 0x8B31;
    pub const VIEWPORT: GLenum = 0x0BA2;
    pub const VIEWPORT_BIT: GLenum = 0x00000800;
    pub const WAIT_FAILED: GLenum = 0x911D;
    pub const WEIGHT_ARRAY_BUFFER_BINDING: GLenum = 0x889E;
    pub const WRITE_ONLY: GLenum = 0x88B9;
    pub const XOR: GLenum = 0x1506;
    pub const ZERO: GLenum = 0;
    pub const ZOOM_X: GLenum = 0x0D16;
    pub const ZOOM_Y: GLenum = 0x0D17;

    use crate::vec::{GLuintVec, StringVec};
    use crate::option::OptionU8VecRef;


    /// `GlType` struct
    pub use crate::dll::AzGlType as GlType;

    impl Clone for GlType { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_gl_type_deep_copy)(self) } }
    impl Drop for GlType { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_type_delete)(self); } }


    /// `DebugMessage` struct
    pub use crate::dll::AzDebugMessage as DebugMessage;

    impl Clone for DebugMessage { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_debug_message_deep_copy)(self) } }
    impl Drop for DebugMessage { fn drop(&mut self) { (crate::dll::get_azul_dll().az_debug_message_delete)(self); } }


    /// C-ABI stable reexport of `&[u8]`
    pub use crate::dll::AzU8VecRef as U8VecRef;

    impl Drop for U8VecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_u8_vec_ref_delete)(self); } }


    /// C-ABI stable reexport of `&mut [u8]`
    pub use crate::dll::AzU8VecRefMut as U8VecRefMut;

    impl Drop for U8VecRefMut { fn drop(&mut self) { (crate::dll::get_azul_dll().az_u8_vec_ref_mut_delete)(self); } }


    /// C-ABI stable reexport of `&[f32]`
    pub use crate::dll::AzF32VecRef as F32VecRef;

    impl Drop for F32VecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_f32_vec_ref_delete)(self); } }


    /// C-ABI stable reexport of `&[i32]`
    pub use crate::dll::AzI32VecRef as I32VecRef;

    impl Drop for I32VecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i32_vec_ref_delete)(self); } }


    /// C-ABI stable reexport of `&[GLuint]` aka `&[u32]`
    pub use crate::dll::AzGLuintVecRef as GLuintVecRef;

    impl Drop for GLuintVecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_luint_vec_ref_delete)(self); } }


    /// C-ABI stable reexport of `&[GLenum]` aka `&[u32]`
    pub use crate::dll::AzGLenumVecRef as GLenumVecRef;

    impl Drop for GLenumVecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lenum_vec_ref_delete)(self); } }


    /// C-ABI stable reexport of `&mut [GLint]` aka `&mut [i32]`
    pub use crate::dll::AzGLintVecRefMut as GLintVecRefMut;

    impl Drop for GLintVecRefMut { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lint_vec_ref_mut_delete)(self); } }


    /// C-ABI stable reexport of `&mut [GLint64]` aka `&mut [i64]`
    pub use crate::dll::AzGLint64VecRefMut as GLint64VecRefMut;

    impl Drop for GLint64VecRefMut { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lint64_vec_ref_mut_delete)(self); } }


    /// C-ABI stable reexport of `&mut [GLboolean]` aka `&mut [u8]`
    pub use crate::dll::AzGLbooleanVecRefMut as GLbooleanVecRefMut;

    impl Drop for GLbooleanVecRefMut { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lboolean_vec_ref_mut_delete)(self); } }


    /// C-ABI stable reexport of `&mut [GLfloat]` aka `&mut [f32]`
    pub use crate::dll::AzGLfloatVecRefMut as GLfloatVecRefMut;

    impl Drop for GLfloatVecRefMut { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lfloat_vec_ref_mut_delete)(self); } }


    /// C-ABI stable reexport of `&[Refstr]` aka `&mut [&str]`
    pub use crate::dll::AzRefstrVecRef as RefstrVecRef;

    impl Drop for RefstrVecRef { fn drop(&mut self) { (crate::dll::get_azul_dll().az_refstr_vec_ref_delete)(self); } }


    /// C-ABI stable reexport of `&str`
    pub use crate::dll::AzRefstr as Refstr;

    impl Drop for Refstr { fn drop(&mut self) { (crate::dll::get_azul_dll().az_refstr_delete)(self); } }


    /// C-ABI stable reexport of `(U8Vec, u32)`
    pub use crate::dll::AzGetProgramBinaryReturn as GetProgramBinaryReturn;

    impl Clone for GetProgramBinaryReturn { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_get_program_binary_return_deep_copy)(self) } }
    impl Drop for GetProgramBinaryReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_get_program_binary_return_delete)(self); } }


    /// C-ABI stable reexport of `(i32, u32, AzString)`
    pub use crate::dll::AzGetActiveAttribReturn as GetActiveAttribReturn;

    impl Clone for GetActiveAttribReturn { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_get_active_attrib_return_deep_copy)(self) } }
    impl Drop for GetActiveAttribReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_get_active_attrib_return_delete)(self); } }


    /// C-ABI stable reexport of `*const gleam::gl::GLsync`
    pub use crate::dll::AzGLsyncPtr as GLsyncPtr;

    impl Drop for GLsyncPtr { fn drop(&mut self) { (crate::dll::get_azul_dll().az_g_lsync_ptr_delete)(self); } }


    /// C-ABI stable reexport of `(i32, u32, AzString)`
    pub use crate::dll::AzGetActiveUniformReturn as GetActiveUniformReturn;

    impl Clone for GetActiveUniformReturn { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_get_active_uniform_return_deep_copy)(self) } }
    impl Drop for GetActiveUniformReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_get_active_uniform_return_delete)(self); } }


    /// `GlContextPtr` struct
    pub use crate::dll::AzGlContextPtr as GlContextPtr;

    impl GlContextPtr {
        /// Calls the `GlContextPtr::get_type` function.
        pub fn get_type(&self)  -> crate::gl::GlType { { (crate::dll::get_azul_dll().az_gl_context_ptr_get_type)(self)} }
        /// Calls the `GlContextPtr::buffer_data_untyped` function.
        pub fn buffer_data_untyped(&self, target: u32, size: isize, data: *const c_void, usage: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_buffer_data_untyped)(self, target, size, data, usage) }
        /// Calls the `GlContextPtr::buffer_sub_data_untyped` function.
        pub fn buffer_sub_data_untyped(&self, target: u32, offset: isize, size: isize, data: *const c_void)  { (crate::dll::get_azul_dll().az_gl_context_ptr_buffer_sub_data_untyped)(self, target, offset, size, data) }
        /// Calls the `GlContextPtr::map_buffer` function.
        pub fn map_buffer(&self, target: u32, access: u32)  -> *mut c_void { (crate::dll::get_azul_dll().az_gl_context_ptr_map_buffer)(self, target, access) }
        /// Calls the `GlContextPtr::map_buffer_range` function.
        pub fn map_buffer_range(&self, target: u32, offset: isize, length: isize, access: u32)  -> *mut c_void { (crate::dll::get_azul_dll().az_gl_context_ptr_map_buffer_range)(self, target, offset, length, access) }
        /// Calls the `GlContextPtr::unmap_buffer` function.
        pub fn unmap_buffer(&self, target: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_unmap_buffer)(self, target) }
        /// Calls the `GlContextPtr::tex_buffer` function.
        pub fn tex_buffer(&self, target: u32, internal_format: u32, buffer: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_buffer)(self, target, internal_format, buffer) }
        /// Calls the `GlContextPtr::shader_source` function.
        pub fn shader_source(&self, shader: u32, strings: StringVec)  { (crate::dll::get_azul_dll().az_gl_context_ptr_shader_source)(self, shader, strings) }
        /// Calls the `GlContextPtr::read_buffer` function.
        pub fn read_buffer(&self, mode: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_read_buffer)(self, mode) }
        /// Calls the `GlContextPtr::read_pixels_into_buffer` function.
        pub fn read_pixels_into_buffer(&self, x: i32, y: i32, width: i32, height: i32, format: u32, pixel_type: u32, dst_buffer: U8VecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_read_pixels_into_buffer)(self, x, y, width, height, format, pixel_type, dst_buffer) }
        /// Calls the `GlContextPtr::read_pixels` function.
        pub fn read_pixels(&self, x: i32, y: i32, width: i32, height: i32, format: u32, pixel_type: u32)  -> crate::vec::U8Vec { { (crate::dll::get_azul_dll().az_gl_context_ptr_read_pixels)(self, x, y, width, height, format, pixel_type)} }
        /// Calls the `GlContextPtr::read_pixels_into_pbo` function.
        pub fn read_pixels_into_pbo(&self, x: i32, y: i32, width: i32, height: i32, format: u32, pixel_type: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_read_pixels_into_pbo)(self, x, y, width, height, format, pixel_type) }
        /// Calls the `GlContextPtr::sample_coverage` function.
        pub fn sample_coverage(&self, value: f32, invert: bool)  { (crate::dll::get_azul_dll().az_gl_context_ptr_sample_coverage)(self, value, invert) }
        /// Calls the `GlContextPtr::polygon_offset` function.
        pub fn polygon_offset(&self, factor: f32, units: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_polygon_offset)(self, factor, units) }
        /// Calls the `GlContextPtr::pixel_store_i` function.
        pub fn pixel_store_i(&self, name: u32, param: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_pixel_store_i)(self, name, param) }
        /// Calls the `GlContextPtr::gen_buffers` function.
        pub fn gen_buffers(&self, n: i32)  -> crate::vec::GLuintVec { { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_buffers)(self, n)} }
        /// Calls the `GlContextPtr::gen_renderbuffers` function.
        pub fn gen_renderbuffers(&self, n: i32)  -> crate::vec::GLuintVec { { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_renderbuffers)(self, n)} }
        /// Calls the `GlContextPtr::gen_framebuffers` function.
        pub fn gen_framebuffers(&self, n: i32)  -> crate::vec::GLuintVec { { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_framebuffers)(self, n)} }
        /// Calls the `GlContextPtr::gen_textures` function.
        pub fn gen_textures(&self, n: i32)  -> crate::vec::GLuintVec { { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_textures)(self, n)} }
        /// Calls the `GlContextPtr::gen_vertex_arrays` function.
        pub fn gen_vertex_arrays(&self, n: i32)  -> crate::vec::GLuintVec { { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_vertex_arrays)(self, n)} }
        /// Calls the `GlContextPtr::gen_queries` function.
        pub fn gen_queries(&self, n: i32)  -> crate::vec::GLuintVec { { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_queries)(self, n)} }
        /// Calls the `GlContextPtr::begin_query` function.
        pub fn begin_query(&self, target: u32, id: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_begin_query)(self, target, id) }
        /// Calls the `GlContextPtr::end_query` function.
        pub fn end_query(&self, target: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_end_query)(self, target) }
        /// Calls the `GlContextPtr::query_counter` function.
        pub fn query_counter(&self, id: u32, target: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_query_counter)(self, id, target) }
        /// Calls the `GlContextPtr::get_query_object_iv` function.
        pub fn get_query_object_iv(&self, id: u32, pname: u32)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_query_object_iv)(self, id, pname) }
        /// Calls the `GlContextPtr::get_query_object_uiv` function.
        pub fn get_query_object_uiv(&self, id: u32, pname: u32)  -> u32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_query_object_uiv)(self, id, pname) }
        /// Calls the `GlContextPtr::get_query_object_i64v` function.
        pub fn get_query_object_i64v(&self, id: u32, pname: u32)  -> i64 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_query_object_i64v)(self, id, pname) }
        /// Calls the `GlContextPtr::get_query_object_ui64v` function.
        pub fn get_query_object_ui64v(&self, id: u32, pname: u32)  -> u64 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_query_object_ui64v)(self, id, pname) }
        /// Calls the `GlContextPtr::delete_queries` function.
        pub fn delete_queries(&self, queries: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_queries)(self, queries) }
        /// Calls the `GlContextPtr::delete_vertex_arrays` function.
        pub fn delete_vertex_arrays(&self, vertex_arrays: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_vertex_arrays)(self, vertex_arrays) }
        /// Calls the `GlContextPtr::delete_buffers` function.
        pub fn delete_buffers(&self, buffers: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_buffers)(self, buffers) }
        /// Calls the `GlContextPtr::delete_renderbuffers` function.
        pub fn delete_renderbuffers(&self, renderbuffers: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_renderbuffers)(self, renderbuffers) }
        /// Calls the `GlContextPtr::delete_framebuffers` function.
        pub fn delete_framebuffers(&self, framebuffers: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_framebuffers)(self, framebuffers) }
        /// Calls the `GlContextPtr::delete_textures` function.
        pub fn delete_textures(&self, textures: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_textures)(self, textures) }
        /// Calls the `GlContextPtr::framebuffer_renderbuffer` function.
        pub fn framebuffer_renderbuffer(&self, target: u32, attachment: u32, renderbuffertarget: u32, renderbuffer: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_framebuffer_renderbuffer)(self, target, attachment, renderbuffertarget, renderbuffer) }
        /// Calls the `GlContextPtr::renderbuffer_storage` function.
        pub fn renderbuffer_storage(&self, target: u32, internalformat: u32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_renderbuffer_storage)(self, target, internalformat, width, height) }
        /// Calls the `GlContextPtr::depth_func` function.
        pub fn depth_func(&self, func: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_depth_func)(self, func) }
        /// Calls the `GlContextPtr::active_texture` function.
        pub fn active_texture(&self, texture: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_active_texture)(self, texture) }
        /// Calls the `GlContextPtr::attach_shader` function.
        pub fn attach_shader(&self, program: u32, shader: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_attach_shader)(self, program, shader) }
        /// Calls the `GlContextPtr::bind_attrib_location` function.
        pub fn bind_attrib_location(&self, program: u32, index: u32, name: Refstr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_attrib_location)(self, program, index, name) }
        /// Calls the `GlContextPtr::get_uniform_iv` function.
        pub fn get_uniform_iv(&self, program: u32, location: i32, result: GLintVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_uniform_iv)(self, program, location, result) }
        /// Calls the `GlContextPtr::get_uniform_fv` function.
        pub fn get_uniform_fv(&self, program: u32, location: i32, result: GLfloatVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_uniform_fv)(self, program, location, result) }
        /// Calls the `GlContextPtr::get_uniform_block_index` function.
        pub fn get_uniform_block_index(&self, program: u32, name: Refstr)  -> u32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_uniform_block_index)(self, program, name) }
        /// Calls the `GlContextPtr::get_uniform_indices` function.
        pub fn get_uniform_indices(&self, program: u32, names: RefstrVecRef)  -> crate::vec::GLuintVec { { (crate::dll::get_azul_dll().az_gl_context_ptr_get_uniform_indices)(self, program, names)} }
        /// Calls the `GlContextPtr::bind_buffer_base` function.
        pub fn bind_buffer_base(&self, target: u32, index: u32, buffer: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_buffer_base)(self, target, index, buffer) }
        /// Calls the `GlContextPtr::bind_buffer_range` function.
        pub fn bind_buffer_range(&self, target: u32, index: u32, buffer: u32, offset: isize, size: isize)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_buffer_range)(self, target, index, buffer, offset, size) }
        /// Calls the `GlContextPtr::uniform_block_binding` function.
        pub fn uniform_block_binding(&self, program: u32, uniform_block_index: u32, uniform_block_binding: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_block_binding)(self, program, uniform_block_index, uniform_block_binding) }
        /// Calls the `GlContextPtr::bind_buffer` function.
        pub fn bind_buffer(&self, target: u32, buffer: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_buffer)(self, target, buffer) }
        /// Calls the `GlContextPtr::bind_vertex_array` function.
        pub fn bind_vertex_array(&self, vao: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_vertex_array)(self, vao) }
        /// Calls the `GlContextPtr::bind_renderbuffer` function.
        pub fn bind_renderbuffer(&self, target: u32, renderbuffer: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_renderbuffer)(self, target, renderbuffer) }
        /// Calls the `GlContextPtr::bind_framebuffer` function.
        pub fn bind_framebuffer(&self, target: u32, framebuffer: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_framebuffer)(self, target, framebuffer) }
        /// Calls the `GlContextPtr::bind_texture` function.
        pub fn bind_texture(&self, target: u32, texture: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_texture)(self, target, texture) }
        /// Calls the `GlContextPtr::draw_buffers` function.
        pub fn draw_buffers(&self, bufs: GLenumVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_draw_buffers)(self, bufs) }
        /// Calls the `GlContextPtr::tex_image_2d` function.
        pub fn tex_image_2d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, border: i32, format: u32, ty: u32, opt_data: OptionU8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_image_2d)(self, target, level, internal_format, width, height, border, format, ty, opt_data) }
        /// Calls the `GlContextPtr::compressed_tex_image_2d` function.
        pub fn compressed_tex_image_2d(&self, target: u32, level: i32, internal_format: u32, width: i32, height: i32, border: i32, data: U8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_compressed_tex_image_2d)(self, target, level, internal_format, width, height, border, data) }
        /// Calls the `GlContextPtr::compressed_tex_sub_image_2d` function.
        pub fn compressed_tex_sub_image_2d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, width: i32, height: i32, format: u32, data: U8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_compressed_tex_sub_image_2d)(self, target, level, xoffset, yoffset, width, height, format, data) }
        /// Calls the `GlContextPtr::tex_image_3d` function.
        pub fn tex_image_3d(&self, target: u32, level: i32, internal_format: i32, width: i32, height: i32, depth: i32, border: i32, format: u32, ty: u32, opt_data: OptionU8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_image_3d)(self, target, level, internal_format, width, height, depth, border, format, ty, opt_data) }
        /// Calls the `GlContextPtr::copy_tex_image_2d` function.
        pub fn copy_tex_image_2d(&self, target: u32, level: i32, internal_format: u32, x: i32, y: i32, width: i32, height: i32, border: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_tex_image_2d)(self, target, level, internal_format, x, y, width, height, border) }
        /// Calls the `GlContextPtr::copy_tex_sub_image_2d` function.
        pub fn copy_tex_sub_image_2d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, x: i32, y: i32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_tex_sub_image_2d)(self, target, level, xoffset, yoffset, x, y, width, height) }
        /// Calls the `GlContextPtr::copy_tex_sub_image_3d` function.
        pub fn copy_tex_sub_image_3d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, zoffset: i32, x: i32, y: i32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_tex_sub_image_3d)(self, target, level, xoffset, yoffset, zoffset, x, y, width, height) }
        /// Calls the `GlContextPtr::tex_sub_image_2d` function.
        pub fn tex_sub_image_2d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, width: i32, height: i32, format: u32, ty: u32, data: U8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_sub_image_2d)(self, target, level, xoffset, yoffset, width, height, format, ty, data) }
        /// Calls the `GlContextPtr::tex_sub_image_2d_pbo` function.
        pub fn tex_sub_image_2d_pbo(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, width: i32, height: i32, format: u32, ty: u32, offset: usize)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_sub_image_2d_pbo)(self, target, level, xoffset, yoffset, width, height, format, ty, offset) }
        /// Calls the `GlContextPtr::tex_sub_image_3d` function.
        pub fn tex_sub_image_3d(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, zoffset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, data: U8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_sub_image_3d)(self, target, level, xoffset, yoffset, zoffset, width, height, depth, format, ty, data) }
        /// Calls the `GlContextPtr::tex_sub_image_3d_pbo` function.
        pub fn tex_sub_image_3d_pbo(&self, target: u32, level: i32, xoffset: i32, yoffset: i32, zoffset: i32, width: i32, height: i32, depth: i32, format: u32, ty: u32, offset: usize)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_sub_image_3d_pbo)(self, target, level, xoffset, yoffset, zoffset, width, height, depth, format, ty, offset) }
        /// Calls the `GlContextPtr::tex_storage_2d` function.
        pub fn tex_storage_2d(&self, target: u32, levels: i32, internal_format: u32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_storage_2d)(self, target, levels, internal_format, width, height) }
        /// Calls the `GlContextPtr::tex_storage_3d` function.
        pub fn tex_storage_3d(&self, target: u32, levels: i32, internal_format: u32, width: i32, height: i32, depth: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_storage_3d)(self, target, levels, internal_format, width, height, depth) }
        /// Calls the `GlContextPtr::get_tex_image_into_buffer` function.
        pub fn get_tex_image_into_buffer(&self, target: u32, level: i32, format: u32, ty: u32, output: U8VecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_tex_image_into_buffer)(self, target, level, format, ty, output) }
        /// Calls the `GlContextPtr::copy_image_sub_data` function.
        pub fn copy_image_sub_data(&self, src_name: u32, src_target: u32, src_level: i32, src_x: i32, src_y: i32, src_z: i32, dst_name: u32, dst_target: u32, dst_level: i32, dst_x: i32, dst_y: i32, dst_z: i32, src_width: i32, src_height: i32, src_depth: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_image_sub_data)(self, src_name, src_target, src_level, src_x, src_y, src_z, dst_name, dst_target, dst_level, dst_x, dst_y, dst_z, src_width, src_height, src_depth) }
        /// Calls the `GlContextPtr::invalidate_framebuffer` function.
        pub fn invalidate_framebuffer(&self, target: u32, attachments: GLenumVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_invalidate_framebuffer)(self, target, attachments) }
        /// Calls the `GlContextPtr::invalidate_sub_framebuffer` function.
        pub fn invalidate_sub_framebuffer(&self, target: u32, attachments: GLenumVecRef, xoffset: i32, yoffset: i32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_invalidate_sub_framebuffer)(self, target, attachments, xoffset, yoffset, width, height) }
        /// Calls the `GlContextPtr::get_integer_v` function.
        pub fn get_integer_v(&self, name: u32, result: GLintVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_integer_v)(self, name, result) }
        /// Calls the `GlContextPtr::get_integer_64v` function.
        pub fn get_integer_64v(&self, name: u32, result: GLint64VecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_integer_64v)(self, name, result) }
        /// Calls the `GlContextPtr::get_integer_iv` function.
        pub fn get_integer_iv(&self, name: u32, index: u32, result: GLintVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_integer_iv)(self, name, index, result) }
        /// Calls the `GlContextPtr::get_integer_64iv` function.
        pub fn get_integer_64iv(&self, name: u32, index: u32, result: GLint64VecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_integer_64iv)(self, name, index, result) }
        /// Calls the `GlContextPtr::get_boolean_v` function.
        pub fn get_boolean_v(&self, name: u32, result: GLbooleanVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_boolean_v)(self, name, result) }
        /// Calls the `GlContextPtr::get_float_v` function.
        pub fn get_float_v(&self, name: u32, result: GLfloatVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_float_v)(self, name, result) }
        /// Calls the `GlContextPtr::get_framebuffer_attachment_parameter_iv` function.
        pub fn get_framebuffer_attachment_parameter_iv(&self, target: u32, attachment: u32, pname: u32)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_framebuffer_attachment_parameter_iv)(self, target, attachment, pname) }
        /// Calls the `GlContextPtr::get_renderbuffer_parameter_iv` function.
        pub fn get_renderbuffer_parameter_iv(&self, target: u32, pname: u32)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_renderbuffer_parameter_iv)(self, target, pname) }
        /// Calls the `GlContextPtr::get_tex_parameter_iv` function.
        pub fn get_tex_parameter_iv(&self, target: u32, name: u32)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_tex_parameter_iv)(self, target, name) }
        /// Calls the `GlContextPtr::get_tex_parameter_fv` function.
        pub fn get_tex_parameter_fv(&self, target: u32, name: u32)  -> f32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_tex_parameter_fv)(self, target, name) }
        /// Calls the `GlContextPtr::tex_parameter_i` function.
        pub fn tex_parameter_i(&self, target: u32, pname: u32, param: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_parameter_i)(self, target, pname, param) }
        /// Calls the `GlContextPtr::tex_parameter_f` function.
        pub fn tex_parameter_f(&self, target: u32, pname: u32, param: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_tex_parameter_f)(self, target, pname, param) }
        /// Calls the `GlContextPtr::framebuffer_texture_2d` function.
        pub fn framebuffer_texture_2d(&self, target: u32, attachment: u32, textarget: u32, texture: u32, level: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_framebuffer_texture_2d)(self, target, attachment, textarget, texture, level) }
        /// Calls the `GlContextPtr::framebuffer_texture_layer` function.
        pub fn framebuffer_texture_layer(&self, target: u32, attachment: u32, texture: u32, level: i32, layer: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_framebuffer_texture_layer)(self, target, attachment, texture, level, layer) }
        /// Calls the `GlContextPtr::blit_framebuffer` function.
        pub fn blit_framebuffer(&self, src_x0: i32, src_y0: i32, src_x1: i32, src_y1: i32, dst_x0: i32, dst_y0: i32, dst_x1: i32, dst_y1: i32, mask: u32, filter: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blit_framebuffer)(self, src_x0, src_y0, src_x1, src_y1, dst_x0, dst_y0, dst_x1, dst_y1, mask, filter) }
        /// Calls the `GlContextPtr::vertex_attrib_4f` function.
        pub fn vertex_attrib_4f(&self, index: u32, x: f32, y: f32, z: f32, w: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_vertex_attrib_4f)(self, index, x, y, z, w) }
        /// Calls the `GlContextPtr::vertex_attrib_pointer_f32` function.
        pub fn vertex_attrib_pointer_f32(&self, index: u32, size: i32, normalized: bool, stride: i32, offset: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_vertex_attrib_pointer_f32)(self, index, size, normalized, stride, offset) }
        /// Calls the `GlContextPtr::vertex_attrib_pointer` function.
        pub fn vertex_attrib_pointer(&self, index: u32, size: i32, type_: u32, normalized: bool, stride: i32, offset: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_vertex_attrib_pointer)(self, index, size, type_, normalized, stride, offset) }
        /// Calls the `GlContextPtr::vertex_attrib_i_pointer` function.
        pub fn vertex_attrib_i_pointer(&self, index: u32, size: i32, type_: u32, stride: i32, offset: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_vertex_attrib_i_pointer)(self, index, size, type_, stride, offset) }
        /// Calls the `GlContextPtr::vertex_attrib_divisor` function.
        pub fn vertex_attrib_divisor(&self, index: u32, divisor: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_vertex_attrib_divisor)(self, index, divisor) }
        /// Calls the `GlContextPtr::viewport` function.
        pub fn viewport(&self, x: i32, y: i32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_viewport)(self, x, y, width, height) }
        /// Calls the `GlContextPtr::scissor` function.
        pub fn scissor(&self, x: i32, y: i32, width: i32, height: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_scissor)(self, x, y, width, height) }
        /// Calls the `GlContextPtr::line_width` function.
        pub fn line_width(&self, width: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_line_width)(self, width) }
        /// Calls the `GlContextPtr::use_program` function.
        pub fn use_program(&self, program: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_use_program)(self, program) }
        /// Calls the `GlContextPtr::validate_program` function.
        pub fn validate_program(&self, program: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_validate_program)(self, program) }
        /// Calls the `GlContextPtr::draw_arrays` function.
        pub fn draw_arrays(&self, mode: u32, first: i32, count: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_draw_arrays)(self, mode, first, count) }
        /// Calls the `GlContextPtr::draw_arrays_instanced` function.
        pub fn draw_arrays_instanced(&self, mode: u32, first: i32, count: i32, primcount: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_draw_arrays_instanced)(self, mode, first, count, primcount) }
        /// Calls the `GlContextPtr::draw_elements` function.
        pub fn draw_elements(&self, mode: u32, count: i32, element_type: u32, indices_offset: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_draw_elements)(self, mode, count, element_type, indices_offset) }
        /// Calls the `GlContextPtr::draw_elements_instanced` function.
        pub fn draw_elements_instanced(&self, mode: u32, count: i32, element_type: u32, indices_offset: u32, primcount: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_draw_elements_instanced)(self, mode, count, element_type, indices_offset, primcount) }
        /// Calls the `GlContextPtr::blend_color` function.
        pub fn blend_color(&self, r: f32, g: f32, b: f32, a: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blend_color)(self, r, g, b, a) }
        /// Calls the `GlContextPtr::blend_func` function.
        pub fn blend_func(&self, sfactor: u32, dfactor: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blend_func)(self, sfactor, dfactor) }
        /// Calls the `GlContextPtr::blend_func_separate` function.
        pub fn blend_func_separate(&self, src_rgb: u32, dest_rgb: u32, src_alpha: u32, dest_alpha: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blend_func_separate)(self, src_rgb, dest_rgb, src_alpha, dest_alpha) }
        /// Calls the `GlContextPtr::blend_equation` function.
        pub fn blend_equation(&self, mode: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blend_equation)(self, mode) }
        /// Calls the `GlContextPtr::blend_equation_separate` function.
        pub fn blend_equation_separate(&self, mode_rgb: u32, mode_alpha: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blend_equation_separate)(self, mode_rgb, mode_alpha) }
        /// Calls the `GlContextPtr::color_mask` function.
        pub fn color_mask(&self, r: bool, g: bool, b: bool, a: bool)  { (crate::dll::get_azul_dll().az_gl_context_ptr_color_mask)(self, r, g, b, a) }
        /// Calls the `GlContextPtr::cull_face` function.
        pub fn cull_face(&self, mode: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_cull_face)(self, mode) }
        /// Calls the `GlContextPtr::front_face` function.
        pub fn front_face(&self, mode: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_front_face)(self, mode) }
        /// Calls the `GlContextPtr::enable` function.
        pub fn enable(&self, cap: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_enable)(self, cap) }
        /// Calls the `GlContextPtr::disable` function.
        pub fn disable(&self, cap: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_disable)(self, cap) }
        /// Calls the `GlContextPtr::hint` function.
        pub fn hint(&self, param_name: u32, param_val: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_hint)(self, param_name, param_val) }
        /// Calls the `GlContextPtr::is_enabled` function.
        pub fn is_enabled(&self, cap: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_is_enabled)(self, cap) }
        /// Calls the `GlContextPtr::is_shader` function.
        pub fn is_shader(&self, shader: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_is_shader)(self, shader) }
        /// Calls the `GlContextPtr::is_texture` function.
        pub fn is_texture(&self, texture: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_is_texture)(self, texture) }
        /// Calls the `GlContextPtr::is_framebuffer` function.
        pub fn is_framebuffer(&self, framebuffer: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_is_framebuffer)(self, framebuffer) }
        /// Calls the `GlContextPtr::is_renderbuffer` function.
        pub fn is_renderbuffer(&self, renderbuffer: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_is_renderbuffer)(self, renderbuffer) }
        /// Calls the `GlContextPtr::check_frame_buffer_status` function.
        pub fn check_frame_buffer_status(&self, target: u32)  -> u32 { (crate::dll::get_azul_dll().az_gl_context_ptr_check_frame_buffer_status)(self, target) }
        /// Calls the `GlContextPtr::enable_vertex_attrib_array` function.
        pub fn enable_vertex_attrib_array(&self, index: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_enable_vertex_attrib_array)(self, index) }
        /// Calls the `GlContextPtr::disable_vertex_attrib_array` function.
        pub fn disable_vertex_attrib_array(&self, index: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_disable_vertex_attrib_array)(self, index) }
        /// Calls the `GlContextPtr::uniform_1f` function.
        pub fn uniform_1f(&self, location: i32, v0: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_1f)(self, location, v0) }
        /// Calls the `GlContextPtr::uniform_1fv` function.
        pub fn uniform_1fv(&self, location: i32, values: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_1fv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_1i` function.
        pub fn uniform_1i(&self, location: i32, v0: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_1i)(self, location, v0) }
        /// Calls the `GlContextPtr::uniform_1iv` function.
        pub fn uniform_1iv(&self, location: i32, values: I32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_1iv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_1ui` function.
        pub fn uniform_1ui(&self, location: i32, v0: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_1ui)(self, location, v0) }
        /// Calls the `GlContextPtr::uniform_2f` function.
        pub fn uniform_2f(&self, location: i32, v0: f32, v1: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_2f)(self, location, v0, v1) }
        /// Calls the `GlContextPtr::uniform_2fv` function.
        pub fn uniform_2fv(&self, location: i32, values: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_2fv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_2i` function.
        pub fn uniform_2i(&self, location: i32, v0: i32, v1: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_2i)(self, location, v0, v1) }
        /// Calls the `GlContextPtr::uniform_2iv` function.
        pub fn uniform_2iv(&self, location: i32, values: I32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_2iv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_2ui` function.
        pub fn uniform_2ui(&self, location: i32, v0: u32, v1: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_2ui)(self, location, v0, v1) }
        /// Calls the `GlContextPtr::uniform_3f` function.
        pub fn uniform_3f(&self, location: i32, v0: f32, v1: f32, v2: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_3f)(self, location, v0, v1, v2) }
        /// Calls the `GlContextPtr::uniform_3fv` function.
        pub fn uniform_3fv(&self, location: i32, values: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_3fv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_3i` function.
        pub fn uniform_3i(&self, location: i32, v0: i32, v1: i32, v2: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_3i)(self, location, v0, v1, v2) }
        /// Calls the `GlContextPtr::uniform_3iv` function.
        pub fn uniform_3iv(&self, location: i32, values: I32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_3iv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_3ui` function.
        pub fn uniform_3ui(&self, location: i32, v0: u32, v1: u32, v2: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_3ui)(self, location, v0, v1, v2) }
        /// Calls the `GlContextPtr::uniform_4f` function.
        pub fn uniform_4f(&self, location: i32, x: f32, y: f32, z: f32, w: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_4f)(self, location, x, y, z, w) }
        /// Calls the `GlContextPtr::uniform_4i` function.
        pub fn uniform_4i(&self, location: i32, x: i32, y: i32, z: i32, w: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_4i)(self, location, x, y, z, w) }
        /// Calls the `GlContextPtr::uniform_4iv` function.
        pub fn uniform_4iv(&self, location: i32, values: I32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_4iv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_4ui` function.
        pub fn uniform_4ui(&self, location: i32, x: u32, y: u32, z: u32, w: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_4ui)(self, location, x, y, z, w) }
        /// Calls the `GlContextPtr::uniform_4fv` function.
        pub fn uniform_4fv(&self, location: i32, values: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_4fv)(self, location, values) }
        /// Calls the `GlContextPtr::uniform_matrix_2fv` function.
        pub fn uniform_matrix_2fv(&self, location: i32, transpose: bool, value: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_matrix_2fv)(self, location, transpose, value) }
        /// Calls the `GlContextPtr::uniform_matrix_3fv` function.
        pub fn uniform_matrix_3fv(&self, location: i32, transpose: bool, value: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_matrix_3fv)(self, location, transpose, value) }
        /// Calls the `GlContextPtr::uniform_matrix_4fv` function.
        pub fn uniform_matrix_4fv(&self, location: i32, transpose: bool, value: F32VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_uniform_matrix_4fv)(self, location, transpose, value) }
        /// Calls the `GlContextPtr::depth_mask` function.
        pub fn depth_mask(&self, flag: bool)  { (crate::dll::get_azul_dll().az_gl_context_ptr_depth_mask)(self, flag) }
        /// Calls the `GlContextPtr::depth_range` function.
        pub fn depth_range(&self, near: f64, far: f64)  { (crate::dll::get_azul_dll().az_gl_context_ptr_depth_range)(self, near, far) }
        /// Calls the `GlContextPtr::get_active_attrib` function.
        pub fn get_active_attrib(&self, program: u32, index: u32)  -> crate::gl::GetActiveAttribReturn { { (crate::dll::get_azul_dll().az_gl_context_ptr_get_active_attrib)(self, program, index)} }
        /// Calls the `GlContextPtr::get_active_uniform` function.
        pub fn get_active_uniform(&self, program: u32, index: u32)  -> crate::gl::GetActiveUniformReturn { { (crate::dll::get_azul_dll().az_gl_context_ptr_get_active_uniform)(self, program, index)} }
        /// Calls the `GlContextPtr::get_active_uniforms_iv` function.
        pub fn get_active_uniforms_iv(&self, program: u32, indices: GLuintVec, pname: u32)  -> crate::vec::GLintVec { { (crate::dll::get_azul_dll().az_gl_context_ptr_get_active_uniforms_iv)(self, program, indices, pname)} }
        /// Calls the `GlContextPtr::get_active_uniform_block_i` function.
        pub fn get_active_uniform_block_i(&self, program: u32, index: u32, pname: u32)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_active_uniform_block_i)(self, program, index, pname) }
        /// Calls the `GlContextPtr::get_active_uniform_block_iv` function.
        pub fn get_active_uniform_block_iv(&self, program: u32, index: u32, pname: u32)  -> crate::vec::GLintVec { { (crate::dll::get_azul_dll().az_gl_context_ptr_get_active_uniform_block_iv)(self, program, index, pname)} }
        /// Calls the `GlContextPtr::get_active_uniform_block_name` function.
        pub fn get_active_uniform_block_name(&self, program: u32, index: u32)  -> crate::str::String { { (crate::dll::get_azul_dll().az_gl_context_ptr_get_active_uniform_block_name)(self, program, index)} }
        /// Calls the `GlContextPtr::get_attrib_location` function.
        pub fn get_attrib_location(&self, program: u32, name: Refstr)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_attrib_location)(self, program, name) }
        /// Calls the `GlContextPtr::get_frag_data_location` function.
        pub fn get_frag_data_location(&self, program: u32, name: Refstr)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_frag_data_location)(self, program, name) }
        /// Calls the `GlContextPtr::get_uniform_location` function.
        pub fn get_uniform_location(&self, program: u32, name: Refstr)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_uniform_location)(self, program, name) }
        /// Calls the `GlContextPtr::get_program_info_log` function.
        pub fn get_program_info_log(&self, program: u32)  -> crate::str::String { { (crate::dll::get_azul_dll().az_gl_context_ptr_get_program_info_log)(self, program)} }
        /// Calls the `GlContextPtr::get_program_iv` function.
        pub fn get_program_iv(&self, program: u32, pname: u32, result: GLintVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_program_iv)(self, program, pname, result) }
        /// Calls the `GlContextPtr::get_program_binary` function.
        pub fn get_program_binary(&self, program: u32)  -> crate::gl::GetProgramBinaryReturn { { (crate::dll::get_azul_dll().az_gl_context_ptr_get_program_binary)(self, program)} }
        /// Calls the `GlContextPtr::program_binary` function.
        pub fn program_binary(&self, program: u32, format: u32, binary: U8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_program_binary)(self, program, format, binary) }
        /// Calls the `GlContextPtr::program_parameter_i` function.
        pub fn program_parameter_i(&self, program: u32, pname: u32, value: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_program_parameter_i)(self, program, pname, value) }
        /// Calls the `GlContextPtr::get_vertex_attrib_iv` function.
        pub fn get_vertex_attrib_iv(&self, index: u32, pname: u32, result: GLintVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_vertex_attrib_iv)(self, index, pname, result) }
        /// Calls the `GlContextPtr::get_vertex_attrib_fv` function.
        pub fn get_vertex_attrib_fv(&self, index: u32, pname: u32, result: GLfloatVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_vertex_attrib_fv)(self, index, pname, result) }
        /// Calls the `GlContextPtr::get_vertex_attrib_pointer_v` function.
        pub fn get_vertex_attrib_pointer_v(&self, index: u32, pname: u32)  -> isize { (crate::dll::get_azul_dll().az_gl_context_ptr_get_vertex_attrib_pointer_v)(self, index, pname) }
        /// Calls the `GlContextPtr::get_buffer_parameter_iv` function.
        pub fn get_buffer_parameter_iv(&self, target: u32, pname: u32)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_buffer_parameter_iv)(self, target, pname) }
        /// Calls the `GlContextPtr::get_shader_info_log` function.
        pub fn get_shader_info_log(&self, shader: u32)  -> crate::str::String { { (crate::dll::get_azul_dll().az_gl_context_ptr_get_shader_info_log)(self, shader)} }
        /// Calls the `GlContextPtr::get_string` function.
        pub fn get_string(&self, which: u32)  -> crate::str::String { { (crate::dll::get_azul_dll().az_gl_context_ptr_get_string)(self, which)} }
        /// Calls the `GlContextPtr::get_string_i` function.
        pub fn get_string_i(&self, which: u32, index: u32)  -> crate::str::String { { (crate::dll::get_azul_dll().az_gl_context_ptr_get_string_i)(self, which, index)} }
        /// Calls the `GlContextPtr::get_shader_iv` function.
        pub fn get_shader_iv(&self, shader: u32, pname: u32, result: GLintVecRefMut)  { (crate::dll::get_azul_dll().az_gl_context_ptr_get_shader_iv)(self, shader, pname, result) }
        /// Calls the `GlContextPtr::get_shader_precision_format` function.
        pub fn get_shader_precision_format(&self, shader_type: u32, precision_type: u32)  -> [i32;3] { (crate::dll::get_azul_dll().az_gl_context_ptr_get_shader_precision_format)(self, shader_type, precision_type) }
        /// Calls the `GlContextPtr::compile_shader` function.
        pub fn compile_shader(&self, shader: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_compile_shader)(self, shader) }
        /// Calls the `GlContextPtr::create_program` function.
        pub fn create_program(&self)  -> u32 { (crate::dll::get_azul_dll().az_gl_context_ptr_create_program)(self) }
        /// Calls the `GlContextPtr::delete_program` function.
        pub fn delete_program(&self, program: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_program)(self, program) }
        /// Calls the `GlContextPtr::create_shader` function.
        pub fn create_shader(&self, shader_type: u32)  -> u32 { (crate::dll::get_azul_dll().az_gl_context_ptr_create_shader)(self, shader_type) }
        /// Calls the `GlContextPtr::delete_shader` function.
        pub fn delete_shader(&self, shader: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_shader)(self, shader) }
        /// Calls the `GlContextPtr::detach_shader` function.
        pub fn detach_shader(&self, program: u32, shader: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_detach_shader)(self, program, shader) }
        /// Calls the `GlContextPtr::link_program` function.
        pub fn link_program(&self, program: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_link_program)(self, program) }
        /// Calls the `GlContextPtr::clear_color` function.
        pub fn clear_color(&self, r: f32, g: f32, b: f32, a: f32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_clear_color)(self, r, g, b, a) }
        /// Calls the `GlContextPtr::clear` function.
        pub fn clear(&self, buffer_mask: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_clear)(self, buffer_mask) }
        /// Calls the `GlContextPtr::clear_depth` function.
        pub fn clear_depth(&self, depth: f64)  { (crate::dll::get_azul_dll().az_gl_context_ptr_clear_depth)(self, depth) }
        /// Calls the `GlContextPtr::clear_stencil` function.
        pub fn clear_stencil(&self, s: i32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_clear_stencil)(self, s) }
        /// Calls the `GlContextPtr::flush` function.
        pub fn flush(&self)  { (crate::dll::get_azul_dll().az_gl_context_ptr_flush)(self) }
        /// Calls the `GlContextPtr::finish` function.
        pub fn finish(&self)  { (crate::dll::get_azul_dll().az_gl_context_ptr_finish)(self) }
        /// Calls the `GlContextPtr::get_error` function.
        pub fn get_error(&self)  -> u32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_error)(self) }
        /// Calls the `GlContextPtr::stencil_mask` function.
        pub fn stencil_mask(&self, mask: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_stencil_mask)(self, mask) }
        /// Calls the `GlContextPtr::stencil_mask_separate` function.
        pub fn stencil_mask_separate(&self, face: u32, mask: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_stencil_mask_separate)(self, face, mask) }
        /// Calls the `GlContextPtr::stencil_func` function.
        pub fn stencil_func(&self, func: u32, ref_: i32, mask: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_stencil_func)(self, func, ref_, mask) }
        /// Calls the `GlContextPtr::stencil_func_separate` function.
        pub fn stencil_func_separate(&self, face: u32, func: u32, ref_: i32, mask: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_stencil_func_separate)(self, face, func, ref_, mask) }
        /// Calls the `GlContextPtr::stencil_op` function.
        pub fn stencil_op(&self, sfail: u32, dpfail: u32, dppass: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_stencil_op)(self, sfail, dpfail, dppass) }
        /// Calls the `GlContextPtr::stencil_op_separate` function.
        pub fn stencil_op_separate(&self, face: u32, sfail: u32, dpfail: u32, dppass: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_stencil_op_separate)(self, face, sfail, dpfail, dppass) }
        /// Calls the `GlContextPtr::egl_image_target_texture2d_oes` function.
        pub fn egl_image_target_texture2d_oes(&self, target: u32, image: *const c_void)  { (crate::dll::get_azul_dll().az_gl_context_ptr_egl_image_target_texture2d_oes)(self, target, image) }
        /// Calls the `GlContextPtr::generate_mipmap` function.
        pub fn generate_mipmap(&self, target: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_generate_mipmap)(self, target) }
        /// Calls the `GlContextPtr::insert_event_marker_ext` function.
        pub fn insert_event_marker_ext(&self, message: Refstr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_insert_event_marker_ext)(self, message) }
        /// Calls the `GlContextPtr::push_group_marker_ext` function.
        pub fn push_group_marker_ext(&self, message: Refstr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_push_group_marker_ext)(self, message) }
        /// Calls the `GlContextPtr::pop_group_marker_ext` function.
        pub fn pop_group_marker_ext(&self)  { (crate::dll::get_azul_dll().az_gl_context_ptr_pop_group_marker_ext)(self) }
        /// Calls the `GlContextPtr::debug_message_insert_khr` function.
        pub fn debug_message_insert_khr(&self, source: u32, type_: u32, id: u32, severity: u32, message: Refstr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_debug_message_insert_khr)(self, source, type_, id, severity, message) }
        /// Calls the `GlContextPtr::push_debug_group_khr` function.
        pub fn push_debug_group_khr(&self, source: u32, id: u32, message: Refstr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_push_debug_group_khr)(self, source, id, message) }
        /// Calls the `GlContextPtr::pop_debug_group_khr` function.
        pub fn pop_debug_group_khr(&self)  { (crate::dll::get_azul_dll().az_gl_context_ptr_pop_debug_group_khr)(self) }
        /// Calls the `GlContextPtr::fence_sync` function.
        pub fn fence_sync(&self, condition: u32, flags: u32)  -> crate::gl::GLsyncPtr { { (crate::dll::get_azul_dll().az_gl_context_ptr_fence_sync)(self, condition, flags)} }
        /// Calls the `GlContextPtr::client_wait_sync` function.
        pub fn client_wait_sync(&self, sync: GLsyncPtr, flags: u32, timeout: u64)  { (crate::dll::get_azul_dll().az_gl_context_ptr_client_wait_sync)(self, sync, flags, timeout) }
        /// Calls the `GlContextPtr::wait_sync` function.
        pub fn wait_sync(&self, sync: GLsyncPtr, flags: u32, timeout: u64)  { (crate::dll::get_azul_dll().az_gl_context_ptr_wait_sync)(self, sync, flags, timeout) }
        /// Calls the `GlContextPtr::delete_sync` function.
        pub fn delete_sync(&self, sync: GLsyncPtr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_sync)(self, sync) }
        /// Calls the `GlContextPtr::texture_range_apple` function.
        pub fn texture_range_apple(&self, target: u32, data: U8VecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_texture_range_apple)(self, target, data) }
        /// Calls the `GlContextPtr::gen_fences_apple` function.
        pub fn gen_fences_apple(&self, n: i32)  -> crate::vec::GLuintVec { { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_fences_apple)(self, n)} }
        /// Calls the `GlContextPtr::delete_fences_apple` function.
        pub fn delete_fences_apple(&self, fences: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_fences_apple)(self, fences) }
        /// Calls the `GlContextPtr::set_fence_apple` function.
        pub fn set_fence_apple(&self, fence: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_set_fence_apple)(self, fence) }
        /// Calls the `GlContextPtr::finish_fence_apple` function.
        pub fn finish_fence_apple(&self, fence: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_finish_fence_apple)(self, fence) }
        /// Calls the `GlContextPtr::test_fence_apple` function.
        pub fn test_fence_apple(&self, fence: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_test_fence_apple)(self, fence) }
        /// Calls the `GlContextPtr::test_object_apple` function.
        pub fn test_object_apple(&self, object: u32, name: u32)  -> u8 { (crate::dll::get_azul_dll().az_gl_context_ptr_test_object_apple)(self, object, name) }
        /// Calls the `GlContextPtr::finish_object_apple` function.
        pub fn finish_object_apple(&self, object: u32, name: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_finish_object_apple)(self, object, name) }
        /// Calls the `GlContextPtr::get_frag_data_index` function.
        pub fn get_frag_data_index(&self, program: u32, name: Refstr)  -> i32 { (crate::dll::get_azul_dll().az_gl_context_ptr_get_frag_data_index)(self, program, name) }
        /// Calls the `GlContextPtr::blend_barrier_khr` function.
        pub fn blend_barrier_khr(&self)  { (crate::dll::get_azul_dll().az_gl_context_ptr_blend_barrier_khr)(self) }
        /// Calls the `GlContextPtr::bind_frag_data_location_indexed` function.
        pub fn bind_frag_data_location_indexed(&self, program: u32, color_number: u32, index: u32, name: Refstr)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_frag_data_location_indexed)(self, program, color_number, index, name) }
        /// Calls the `GlContextPtr::get_debug_messages` function.
        pub fn get_debug_messages(&self)  -> crate::vec::DebugMessageVec { { (crate::dll::get_azul_dll().az_gl_context_ptr_get_debug_messages)(self)} }
        /// Calls the `GlContextPtr::provoking_vertex_angle` function.
        pub fn provoking_vertex_angle(&self, mode: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_provoking_vertex_angle)(self, mode) }
        /// Calls the `GlContextPtr::gen_vertex_arrays_apple` function.
        pub fn gen_vertex_arrays_apple(&self, n: i32)  -> crate::vec::GLuintVec { { (crate::dll::get_azul_dll().az_gl_context_ptr_gen_vertex_arrays_apple)(self, n)} }
        /// Calls the `GlContextPtr::bind_vertex_array_apple` function.
        pub fn bind_vertex_array_apple(&self, vao: u32)  { (crate::dll::get_azul_dll().az_gl_context_ptr_bind_vertex_array_apple)(self, vao) }
        /// Calls the `GlContextPtr::delete_vertex_arrays_apple` function.
        pub fn delete_vertex_arrays_apple(&self, vertex_arrays: GLuintVecRef)  { (crate::dll::get_azul_dll().az_gl_context_ptr_delete_vertex_arrays_apple)(self, vertex_arrays) }
        /// Calls the `GlContextPtr::copy_texture_chromium` function.
        pub fn copy_texture_chromium(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, internal_format: i32, dest_type: u32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_texture_chromium)(self, source_id, source_level, dest_target, dest_id, dest_level, internal_format, dest_type, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) }
        /// Calls the `GlContextPtr::copy_sub_texture_chromium` function.
        pub fn copy_sub_texture_chromium(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, x_offset: i32, y_offset: i32, x: i32, y: i32, width: i32, height: i32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_sub_texture_chromium)(self, source_id, source_level, dest_target, dest_id, dest_level, x_offset, y_offset, x, y, width, height, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) }
        /// Calls the `GlContextPtr::egl_image_target_renderbuffer_storage_oes` function.
        pub fn egl_image_target_renderbuffer_storage_oes(&self, target: u32, image: *const c_void)  { (crate::dll::get_azul_dll().az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes)(self, target, image) }
        /// Calls the `GlContextPtr::copy_texture_3d_angle` function.
        pub fn copy_texture_3d_angle(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, internal_format: i32, dest_type: u32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_texture_3d_angle)(self, source_id, source_level, dest_target, dest_id, dest_level, internal_format, dest_type, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) }
        /// Calls the `GlContextPtr::copy_sub_texture_3d_angle` function.
        pub fn copy_sub_texture_3d_angle(&self, source_id: u32, source_level: i32, dest_target: u32, dest_id: u32, dest_level: i32, x_offset: i32, y_offset: i32, z_offset: i32, x: i32, y: i32, z: i32, width: i32, height: i32, depth: i32, unpack_flip_y: u8, unpack_premultiply_alpha: u8, unpack_unmultiply_alpha: u8)  { (crate::dll::get_azul_dll().az_gl_context_ptr_copy_sub_texture_3d_angle)(self, source_id, source_level, dest_target, dest_id, dest_level, x_offset, y_offset, z_offset, x, y, z, width, height, depth, unpack_flip_y, unpack_premultiply_alpha, unpack_unmultiply_alpha) }
    }

    impl Clone for GlContextPtr { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_gl_context_ptr_deep_copy)(self) } }
    impl Drop for GlContextPtr { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_context_ptr_delete)(self); } }


    /// `Texture` struct
    pub use crate::dll::AzTexture as Texture;

    impl Drop for Texture { fn drop(&mut self) { (crate::dll::get_azul_dll().az_texture_delete)(self); } }


    /// `TextureFlags` struct
    pub use crate::dll::AzTextureFlags as TextureFlags;

    impl Clone for TextureFlags { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_texture_flags_deep_copy)(self) } }
    impl Drop for TextureFlags { fn drop(&mut self) { (crate::dll::get_azul_dll().az_texture_flags_delete)(self); } }
}

/// Struct definition for image / font / text IDs
#[allow(dead_code, unused_imports)]
pub mod resources {

    use crate::dll::*;
    use std::ffi::c_void;
    use crate::vec::U8Vec;


    /// `TextId` struct
    pub use crate::dll::AzTextId as TextId;

    impl TextId {
        /// Creates a new, unique `TextId`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_text_id_new)() }
    }

    impl Clone for TextId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_text_id_deep_copy)(self) } }
    impl Drop for TextId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_text_id_delete)(self); } }


    /// `ImageId` struct
    pub use crate::dll::AzImageId as ImageId;

    impl ImageId {
        /// Creates a new, unique `ImageId`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_image_id_new)() }
    }

    impl Clone for ImageId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_image_id_deep_copy)(self) } }
    impl Drop for ImageId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_image_id_delete)(self); } }


    /// `FontId` struct
    pub use crate::dll::AzFontId as FontId;

    impl FontId {
        /// Creates a new, unique `FontId`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_font_id_new)() }
    }

    impl Clone for FontId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_font_id_deep_copy)(self) } }
    impl Drop for FontId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_font_id_delete)(self); } }


    /// `ImageSource` struct
    pub use crate::dll::AzImageSource as ImageSource;

    impl Clone for ImageSource { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_image_source_deep_copy)(self) } }
    impl Drop for ImageSource { fn drop(&mut self) { (crate::dll::get_azul_dll().az_image_source_delete)(self); } }


    /// `FontSource` struct
    pub use crate::dll::AzFontSource as FontSource;

    impl Clone for FontSource { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_font_source_deep_copy)(self) } }
    impl Drop for FontSource { fn drop(&mut self) { (crate::dll::get_azul_dll().az_font_source_delete)(self); } }


    /// `RawImage` struct
    pub use crate::dll::AzRawImage as RawImage;

    impl RawImage {
        /// Creates a new `RawImage` by loading the decoded bytes
        pub fn new(decoded_pixels: U8Vec, width: usize, height: usize, data_format: RawImageFormat) -> Self { (crate::dll::get_azul_dll().az_raw_image_new)(decoded_pixels, width, height, data_format) }
    }

    impl Clone for RawImage { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_raw_image_deep_copy)(self) } }
    impl Drop for RawImage { fn drop(&mut self) { (crate::dll::get_azul_dll().az_raw_image_delete)(self); } }


    /// `RawImageFormat` struct
    pub use crate::dll::AzRawImageFormat as RawImageFormat;

    impl Clone for RawImageFormat { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_raw_image_format_deep_copy)(self) } }
    impl Drop for RawImageFormat { fn drop(&mut self) { (crate::dll::get_azul_dll().az_raw_image_format_delete)(self); } }
}

/// Asyncronous timers / task / thread handlers for easy async loading
#[allow(dead_code, unused_imports)]
pub mod task {

    use crate::dll::*;
    use std::ffi::c_void;
    use crate::callbacks::{RefAny, TaskCallbackType, ThreadCallbackType};


    /// `DropCheckPtr` struct
    pub use crate::dll::AzDropCheckPtrPtr as DropCheckPtr;

    impl Drop for DropCheckPtr { fn drop(&mut self) { (crate::dll::get_azul_dll().az_drop_check_ptr_delete)(self); } }


    /// `ArcMutexRefAny` struct
    pub use crate::dll::AzArcMutexRefAnyPtr as ArcMutexRefAny;

    impl Drop for ArcMutexRefAny { fn drop(&mut self) { (crate::dll::get_azul_dll().az_arc_mutex_ref_any_delete)(self); } }


    /// `Timer` struct
    pub use crate::dll::AzTimer as Timer;

    impl Clone for Timer { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_timer_deep_copy)(self) } }
    impl Drop for Timer { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_delete)(self); } }


    /// `Task` struct
    pub use crate::dll::AzTaskPtr as Task;

    impl Task {
        /// Creates and starts a new `Task`
        pub fn new(data: ArcMutexRefAny, callback: TaskCallbackType) -> Self { (crate::dll::get_azul_dll().az_task_new)(data, callback) }
        /// Creates and starts a new `Task`
        pub fn then(self, timer: Timer)  -> crate::task::Task { { (crate::dll::get_azul_dll().az_task_then)(self, timer)} }
    }

    impl Drop for Task { fn drop(&mut self) { (crate::dll::get_azul_dll().az_task_delete)(self); } }


    /// `Thread` struct
    pub use crate::dll::AzThreadPtr as Thread;

    impl Thread {
        /// Creates and starts a new thread that calls the `callback` on the `data`.
        pub fn new(data: RefAny, callback: ThreadCallbackType) -> Self { (crate::dll::get_azul_dll().az_thread_new)(data, callback) }
        /// Blocks until the internal thread has finished and returns the result of the operation
        pub fn block(self)  -> crate::result::ResultRefAnyBlockError { { (crate::dll::get_azul_dll().az_thread_block)(self)} }
    }

    impl Drop for Thread { fn drop(&mut self) { (crate::dll::get_azul_dll().az_thread_delete)(self); } }


    /// `DropCheck` struct
    pub use crate::dll::AzDropCheckPtr as DropCheck;

    impl Drop for DropCheck { fn drop(&mut self) { (crate::dll::get_azul_dll().az_drop_check_delete)(self); } }


    /// `TimerId` struct
    pub use crate::dll::AzTimerId as TimerId;

    impl Clone for TimerId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_timer_id_deep_copy)(self) } }
    impl Drop for TimerId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_id_delete)(self); } }


    /// Should a timer terminate or not - used to remove active timers
    pub use crate::dll::AzTerminateTimer as TerminateTimer;

    impl Clone for TerminateTimer { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_terminate_timer_deep_copy)(self) } }
    impl Drop for TerminateTimer { fn drop(&mut self) { (crate::dll::get_azul_dll().az_terminate_timer_delete)(self); } }


    /// `BlockError` struct
    pub use crate::dll::AzBlockError as BlockError;

    impl Clone for BlockError { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_block_error_deep_copy)(self) } }
    impl Drop for BlockError { fn drop(&mut self) { (crate::dll::get_azul_dll().az_block_error_delete)(self); } }
}

/// Window creation / startup configuration
#[allow(dead_code, unused_imports)]
pub mod window {

    use crate::dll::*;
    use std::ffi::c_void;
    use crate::css::Css;


    /// `TaskBarIcon` struct
    pub use crate::dll::AzTaskBarIcon as TaskBarIcon;

    impl Clone for TaskBarIcon { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_task_bar_icon_deep_copy)(self) } }
    impl Drop for TaskBarIcon { fn drop(&mut self) { (crate::dll::get_azul_dll().az_task_bar_icon_delete)(self); } }


    /// `XWindowType` struct
    pub use crate::dll::AzXWindowType as XWindowType;

    impl Clone for XWindowType { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_x_window_type_deep_copy)(self) } }
    impl Drop for XWindowType { fn drop(&mut self) { (crate::dll::get_azul_dll().az_x_window_type_delete)(self); } }


    /// `PhysicalPositionI32` struct
    pub use crate::dll::AzPhysicalPositionI32 as PhysicalPositionI32;

    impl Clone for PhysicalPositionI32 { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_physical_position_i32_deep_copy)(self) } }
    impl Drop for PhysicalPositionI32 { fn drop(&mut self) { (crate::dll::get_azul_dll().az_physical_position_i32_delete)(self); } }


    /// `LogicalPosition` struct
    pub use crate::dll::AzLogicalPosition as LogicalPosition;

    impl Clone for LogicalPosition { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_logical_position_deep_copy)(self) } }
    impl Drop for LogicalPosition { fn drop(&mut self) { (crate::dll::get_azul_dll().az_logical_position_delete)(self); } }


    /// `IconKey` struct
    pub use crate::dll::AzIconKey as IconKey;

    impl Clone for IconKey { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_icon_key_deep_copy)(self) } }
    impl Drop for IconKey { fn drop(&mut self) { (crate::dll::get_azul_dll().az_icon_key_delete)(self); } }


    /// `SmallWindowIconBytes` struct
    pub use crate::dll::AzSmallWindowIconBytes as SmallWindowIconBytes;

    impl Clone for SmallWindowIconBytes { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_small_window_icon_bytes_deep_copy)(self) } }
    impl Drop for SmallWindowIconBytes { fn drop(&mut self) { (crate::dll::get_azul_dll().az_small_window_icon_bytes_delete)(self); } }


    /// `LargeWindowIconBytes` struct
    pub use crate::dll::AzLargeWindowIconBytes as LargeWindowIconBytes;

    impl Clone for LargeWindowIconBytes { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_large_window_icon_bytes_deep_copy)(self) } }
    impl Drop for LargeWindowIconBytes { fn drop(&mut self) { (crate::dll::get_azul_dll().az_large_window_icon_bytes_delete)(self); } }


    /// `WindowIcon` struct
    pub use crate::dll::AzWindowIcon as WindowIcon;

    impl Clone for WindowIcon { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_icon_deep_copy)(self) } }
    impl Drop for WindowIcon { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_icon_delete)(self); } }


    /// `VirtualKeyCode` struct
    pub use crate::dll::AzVirtualKeyCode as VirtualKeyCode;

    impl Clone for VirtualKeyCode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_virtual_key_code_deep_copy)(self) } }
    impl Drop for VirtualKeyCode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_virtual_key_code_delete)(self); } }


    /// `AcceleratorKey` struct
    pub use crate::dll::AzAcceleratorKey as AcceleratorKey;

    impl Clone for AcceleratorKey { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_accelerator_key_deep_copy)(self) } }
    impl Drop for AcceleratorKey { fn drop(&mut self) { (crate::dll::get_azul_dll().az_accelerator_key_delete)(self); } }


    /// `WindowSize` struct
    pub use crate::dll::AzWindowSize as WindowSize;

    impl Clone for WindowSize { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_size_deep_copy)(self) } }
    impl Drop for WindowSize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_size_delete)(self); } }


    /// `WindowFlags` struct
    pub use crate::dll::AzWindowFlags as WindowFlags;

    impl Clone for WindowFlags { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_flags_deep_copy)(self) } }
    impl Drop for WindowFlags { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_flags_delete)(self); } }


    /// `DebugState` struct
    pub use crate::dll::AzDebugState as DebugState;

    impl Clone for DebugState { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_debug_state_deep_copy)(self) } }
    impl Drop for DebugState { fn drop(&mut self) { (crate::dll::get_azul_dll().az_debug_state_delete)(self); } }


    /// `KeyboardState` struct
    pub use crate::dll::AzKeyboardState as KeyboardState;

    impl Clone for KeyboardState { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_keyboard_state_deep_copy)(self) } }
    impl Drop for KeyboardState { fn drop(&mut self) { (crate::dll::get_azul_dll().az_keyboard_state_delete)(self); } }


    /// `MouseCursorType` struct
    pub use crate::dll::AzMouseCursorType as MouseCursorType;

    impl Clone for MouseCursorType { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_mouse_cursor_type_deep_copy)(self) } }
    impl Drop for MouseCursorType { fn drop(&mut self) { (crate::dll::get_azul_dll().az_mouse_cursor_type_delete)(self); } }


    /// `CursorPosition` struct
    pub use crate::dll::AzCursorPosition as CursorPosition;

    impl Clone for CursorPosition { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_cursor_position_deep_copy)(self) } }
    impl Drop for CursorPosition { fn drop(&mut self) { (crate::dll::get_azul_dll().az_cursor_position_delete)(self); } }


    /// `MouseState` struct
    pub use crate::dll::AzMouseState as MouseState;

    impl Clone for MouseState { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_mouse_state_deep_copy)(self) } }
    impl Drop for MouseState { fn drop(&mut self) { (crate::dll::get_azul_dll().az_mouse_state_delete)(self); } }


    /// `PlatformSpecificOptions` struct
    pub use crate::dll::AzPlatformSpecificOptions as PlatformSpecificOptions;

    impl Clone for PlatformSpecificOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_platform_specific_options_deep_copy)(self) } }
    impl Drop for PlatformSpecificOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_platform_specific_options_delete)(self); } }


    /// `WindowsWindowOptions` struct
    pub use crate::dll::AzWindowsWindowOptions as WindowsWindowOptions;

    impl Clone for WindowsWindowOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_windows_window_options_deep_copy)(self) } }
    impl Drop for WindowsWindowOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_windows_window_options_delete)(self); } }


    /// `WaylandTheme` struct
    pub use crate::dll::AzWaylandTheme as WaylandTheme;

    impl Clone for WaylandTheme { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_wayland_theme_deep_copy)(self) } }
    impl Drop for WaylandTheme { fn drop(&mut self) { (crate::dll::get_azul_dll().az_wayland_theme_delete)(self); } }


    /// `RendererType` struct
    pub use crate::dll::AzRendererType as RendererType;

    impl Clone for RendererType { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_renderer_type_deep_copy)(self) } }
    impl Drop for RendererType { fn drop(&mut self) { (crate::dll::get_azul_dll().az_renderer_type_delete)(self); } }


    /// `StringPair` struct
    pub use crate::dll::AzStringPair as StringPair;

    impl Clone for StringPair { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_string_pair_deep_copy)(self) } }
    impl Drop for StringPair { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_pair_delete)(self); } }


    /// `LinuxWindowOptions` struct
    pub use crate::dll::AzLinuxWindowOptions as LinuxWindowOptions;

    impl Clone for LinuxWindowOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_linux_window_options_deep_copy)(self) } }
    impl Drop for LinuxWindowOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_linux_window_options_delete)(self); } }


    /// `MacWindowOptions` struct
    pub use crate::dll::AzMacWindowOptions as MacWindowOptions;

    impl Clone for MacWindowOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_mac_window_options_deep_copy)(self) } }
    impl Drop for MacWindowOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_mac_window_options_delete)(self); } }


    /// `WasmWindowOptions` struct
    pub use crate::dll::AzWasmWindowOptions as WasmWindowOptions;

    impl Clone for WasmWindowOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_wasm_window_options_deep_copy)(self) } }
    impl Drop for WasmWindowOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_wasm_window_options_delete)(self); } }


    /// `FullScreenMode` struct
    pub use crate::dll::AzFullScreenMode as FullScreenMode;

    impl Clone for FullScreenMode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_full_screen_mode_deep_copy)(self) } }
    impl Drop for FullScreenMode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_full_screen_mode_delete)(self); } }


    /// `WindowState` struct
    pub use crate::dll::AzWindowState as WindowState;

    impl Clone for WindowState { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_state_deep_copy)(self) } }
    impl Drop for WindowState { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_state_delete)(self); } }


    /// `LogicalSize` struct
    pub use crate::dll::AzLogicalSize as LogicalSize;

    impl Clone for LogicalSize { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_logical_size_deep_copy)(self) } }
    impl Drop for LogicalSize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_logical_size_delete)(self); } }


    /// `HotReloadOptions` struct
    pub use crate::dll::AzHotReloadOptions as HotReloadOptions;

    impl Clone for HotReloadOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_hot_reload_options_deep_copy)(self) } }
    impl Drop for HotReloadOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_hot_reload_options_delete)(self); } }


    /// `WindowCreateOptions` struct
    pub use crate::dll::AzWindowCreateOptions as WindowCreateOptions;

    impl WindowCreateOptions {
        /// Creates a new `WindowCreateOptions` instance.
        pub fn new(css: Css) -> Self { (crate::dll::get_azul_dll().az_window_create_options_new)(css) }
    }

    impl Clone for WindowCreateOptions { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_window_create_options_deep_copy)(self) } }
    impl Drop for WindowCreateOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_create_options_delete)(self); } }
}

