#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]//! Auto-generated public Rust API for the Azul GUI toolkit version 0.1.0
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

    #[no_mangle]
    #[repr(C)]
    pub struct AzRefAny {
        pub _internal_ptr: *const c_void,
        pub _internal_len: usize,
        pub _internal_layout_size: usize,
        pub _internal_layout_align: usize,
        pub type_id: u64,
        pub type_name: AzString,
        pub strong_count: usize,
        pub is_currently_mutable: bool,
        pub custom_destructor: fn(AzRefAny),
    }

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

    pub type AzTimerCallbackType = fn(AzTimerCallbackInfoPtr) -> AzTimerCallbackReturn;
    pub type AzThreadCallbackType = fn(AzRefAny) -> AzRefAny;
    pub type AzTaskCallbackType= fn(AzArcMutexRefAnyPtr, AzDropCheckPtr) -> AzUpdateScreen;

    impl From<AzOn> for AzEventFilter {
        fn from(on: AzOn) -> AzEventFilter {
            on.into_event_filter()
        }
    }

    /// Re-export of rust-allocated (stack based) `String` struct
    #[repr(C)] pub struct AzString {
        pub vec: AzU8Vec,
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
    /// Wrapper over a Rust-allocated `GradientStopPreVec`
    #[repr(C)] pub struct AzGradientStopPreVec {
        pub(crate) ptr: *const AzGradientStopPre,
        pub len: usize,
        pub cap: usize,
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
    /// Re-export of rust-allocated (stack based) `OptionInstant` struct
    #[repr(C, u8)] pub enum AzOptionInstant {
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
    /// Re-export of rust-allocated (stack based) `TimerCallbackReturn` struct
    #[repr(C)] pub struct AzTimerCallbackReturn {
        pub should_update: AzUpdateScreen,
        pub should_terminate: AzTerminateTimer,
    }
    /// Pointer to rust-allocated `Box<LayoutInfo>` struct
    #[repr(C)] pub struct AzLayoutInfoPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Pointer to rust-allocated `Box<Css>` struct
    #[repr(C)] pub struct AzCssPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Pointer to rust-allocated `Box<CssHotReloader>` struct
    #[repr(C)] pub struct AzCssHotReloaderPtr {
        pub(crate) ptr: *mut c_void,
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
        pub(crate) ptr: *mut u32,
        pub len: usize,
    }
    /// C-ABI stable reexport of `&[GLenum]` aka `&[u32]`
    #[repr(C)] pub struct AzGLenumVecRef {
        pub(crate) ptr: *mut u32,
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
        pub(crate) ptr: *mut AzRefstr,
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
    /// C-ABI stable reexport of `(i32, u32, AzString)`
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
    /// Pointer to rust-allocated `Box<TimerCallbackInfo>` struct
    #[repr(C)] pub struct AzTimerCallbackInfoPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `Timer` struct
    #[repr(C)] pub struct AzTimer {
        pub created: AzInstantPtr,
        pub last_run: AzOptionInstant,
        pub delay: AzOptionInstant,
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
    /// Pointer to rust-allocated `Box<WindowCreateOptions>` struct
    #[repr(C)] pub struct AzWindowCreateOptionsPtr {
        pub(crate) ptr: *mut c_void,
    }
    /// Re-export of rust-allocated (stack based) `LogicalSize` struct
    #[repr(C)] pub struct AzLogicalSize {
        pub width: f32,
        pub height: f32,
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
        pub az_gradient_stop_pre_vec_copy_from: Symbol<extern fn(_:  *const AzGradientStopPre, _:  usize) -> AzGradientStopPreVec>,
        pub az_gradient_stop_pre_vec_delete: Symbol<extern fn(_:  &mut AzGradientStopPreVec)>,
        pub az_gradient_stop_pre_vec_deep_copy: Symbol<extern fn(_:  &AzGradientStopPreVec) -> AzGradientStopPreVec>,
        pub az_option_percentage_value_delete: Symbol<extern fn(_:  &mut AzOptionPercentageValue)>,
        pub az_option_percentage_value_deep_copy: Symbol<extern fn(_:  &AzOptionPercentageValue) -> AzOptionPercentageValue>,
        pub az_option_dom_delete: Symbol<extern fn(_:  &mut AzOptionDom)>,
        pub az_option_dom_deep_copy: Symbol<extern fn(_:  &AzOptionDom) -> AzOptionDom>,
        pub az_option_texture_delete: Symbol<extern fn(_:  &mut AzOptionTexture)>,
        pub az_option_tab_index_delete: Symbol<extern fn(_:  &mut AzOptionTabIndex)>,
        pub az_option_tab_index_deep_copy: Symbol<extern fn(_:  &AzOptionTabIndex) -> AzOptionTabIndex>,
        pub az_option_duration_delete: Symbol<extern fn(_:  &mut AzOptionDuration)>,
        pub az_option_duration_deep_copy: Symbol<extern fn(_:  &AzOptionDuration) -> AzOptionDuration>,
        pub az_option_instant_delete: Symbol<extern fn(_:  &mut AzOptionInstant)>,
        pub az_option_instant_deep_copy: Symbol<extern fn(_:  &AzOptionInstant) -> AzOptionInstant>,
        pub az_option_usize_delete: Symbol<extern fn(_:  &mut AzOptionUsize)>,
        pub az_option_usize_deep_copy: Symbol<extern fn(_:  &AzOptionUsize) -> AzOptionUsize>,
        pub az_option_u8_vec_ref_delete: Symbol<extern fn(_:  &mut AzOptionU8VecRef)>,
        pub az_result_ref_any_block_error_delete: Symbol<extern fn(_:  &mut AzResultRefAnyBlockError)>,
        pub az_result_ref_any_block_error_deep_copy: Symbol<extern fn(_:  &AzResultRefAnyBlockError) -> AzResultRefAnyBlockError>,
        pub az_instant_now: Symbol<extern fn() -> AzInstantPtr>,
        pub az_instant_delete: Symbol<extern fn(_:  &mut AzInstantPtr)>,
        pub az_instant_shallow_copy: Symbol<extern fn(_:  &AzInstantPtr) -> AzInstantPtr>,
        pub az_duration_delete: Symbol<extern fn(_:  &mut AzDuration)>,
        pub az_duration_deep_copy: Symbol<extern fn(_:  &AzDuration) -> AzDuration>,
        pub az_app_config_default: Symbol<extern fn() -> AzAppConfigPtr>,
        pub az_app_config_delete: Symbol<extern fn(_:  &mut AzAppConfigPtr)>,
        pub az_app_config_shallow_copy: Symbol<extern fn(_:  &AzAppConfigPtr) -> AzAppConfigPtr>,
        pub az_app_new: Symbol<extern fn(_:  AzRefAny, _:  AzAppConfigPtr, _:  AzLayoutCallbackType) -> AzAppPtr>,
        pub az_app_run: Symbol<extern fn(_:  AzAppPtr, _:  AzWindowCreateOptionsPtr)>,
        pub az_app_delete: Symbol<extern fn(_:  &mut AzAppPtr)>,
        pub az_app_shallow_copy: Symbol<extern fn(_:  &AzAppPtr) -> AzAppPtr>,
        pub az_layout_callback_delete: Symbol<extern fn(_:  &mut AzLayoutCallback)>,
        pub az_layout_callback_deep_copy: Symbol<extern fn(_:  &AzLayoutCallback) -> AzLayoutCallback>,
        pub az_callback_delete: Symbol<extern fn(_:  &mut AzCallback)>,
        pub az_callback_deep_copy: Symbol<extern fn(_:  &AzCallback) -> AzCallback>,
        pub az_callback_info_delete: Symbol<extern fn(_:  &mut AzCallbackInfoPtr)>,
        pub az_callback_info_shallow_copy: Symbol<extern fn(_:  &AzCallbackInfoPtr) -> AzCallbackInfoPtr>,
        pub az_update_screen_delete: Symbol<extern fn(_:  &mut AzUpdateScreen)>,
        pub az_update_screen_deep_copy: Symbol<extern fn(_:  &AzUpdateScreen) -> AzUpdateScreen>,
        pub az_i_frame_callback_delete: Symbol<extern fn(_:  &mut AzIFrameCallback)>,
        pub az_i_frame_callback_deep_copy: Symbol<extern fn(_:  &AzIFrameCallback) -> AzIFrameCallback>,
        pub az_i_frame_callback_info_delete: Symbol<extern fn(_:  &mut AzIFrameCallbackInfoPtr)>,
        pub az_i_frame_callback_info_shallow_copy: Symbol<extern fn(_:  &AzIFrameCallbackInfoPtr) -> AzIFrameCallbackInfoPtr>,
        pub az_i_frame_callback_return_delete: Symbol<extern fn(_:  &mut AzIFrameCallbackReturn)>,
        pub az_i_frame_callback_return_deep_copy: Symbol<extern fn(_:  &AzIFrameCallbackReturn) -> AzIFrameCallbackReturn>,
        pub az_gl_callback_delete: Symbol<extern fn(_:  &mut AzGlCallback)>,
        pub az_gl_callback_deep_copy: Symbol<extern fn(_:  &AzGlCallback) -> AzGlCallback>,
        pub az_gl_callback_info_delete: Symbol<extern fn(_:  &mut AzGlCallbackInfoPtr)>,
        pub az_gl_callback_info_shallow_copy: Symbol<extern fn(_:  &AzGlCallbackInfoPtr) -> AzGlCallbackInfoPtr>,
        pub az_gl_callback_return_delete: Symbol<extern fn(_:  &mut AzGlCallbackReturn)>,
        pub az_timer_callback_delete: Symbol<extern fn(_:  &mut AzTimerCallback)>,
        pub az_timer_callback_deep_copy: Symbol<extern fn(_:  &AzTimerCallback) -> AzTimerCallback>,
        pub az_timer_callback_type_delete: Symbol<extern fn(_:  &mut AzTimerCallbackTypePtr)>,
        pub az_timer_callback_type_shallow_copy: Symbol<extern fn(_:  &AzTimerCallbackTypePtr) -> AzTimerCallbackTypePtr>,
        pub az_timer_callback_return_delete: Symbol<extern fn(_:  &mut AzTimerCallbackReturn)>,
        pub az_timer_callback_return_deep_copy: Symbol<extern fn(_:  &AzTimerCallbackReturn) -> AzTimerCallbackReturn>,
        pub az_layout_info_delete: Symbol<extern fn(_:  &mut AzLayoutInfoPtr)>,
        pub az_layout_info_shallow_copy: Symbol<extern fn(_:  &AzLayoutInfoPtr) -> AzLayoutInfoPtr>,
        pub az_css_native: Symbol<extern fn() -> AzCssPtr>,
        pub az_css_empty: Symbol<extern fn() -> AzCssPtr>,
        pub az_css_from_string: Symbol<extern fn(_:  AzString) -> AzCssPtr>,
        pub az_css_override_native: Symbol<extern fn(_:  AzString) -> AzCssPtr>,
        pub az_css_delete: Symbol<extern fn(_:  &mut AzCssPtr)>,
        pub az_css_shallow_copy: Symbol<extern fn(_:  &AzCssPtr) -> AzCssPtr>,
        pub az_css_hot_reloader_new: Symbol<extern fn(_:  AzString, _:  u64) -> AzCssHotReloaderPtr>,
        pub az_css_hot_reloader_override_native: Symbol<extern fn(_:  AzString, _:  u64) -> AzCssHotReloaderPtr>,
        pub az_css_hot_reloader_delete: Symbol<extern fn(_:  &mut AzCssHotReloaderPtr)>,
        pub az_css_hot_reloader_shallow_copy: Symbol<extern fn(_:  &AzCssHotReloaderPtr) -> AzCssHotReloaderPtr>,
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
        pub az_dom_get_html_string: Symbol<extern fn(_:  &mut AzDom) -> AzString>,
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
        pub az_drop_check_ptr_shallow_copy: Symbol<extern fn(_:  &AzDropCheckPtrPtr) -> AzDropCheckPtrPtr>,
        pub az_arc_mutex_ref_any_delete: Symbol<extern fn(_:  &mut AzArcMutexRefAnyPtr)>,
        pub az_arc_mutex_ref_any_shallow_copy: Symbol<extern fn(_:  &AzArcMutexRefAnyPtr) -> AzArcMutexRefAnyPtr>,
        pub az_timer_callback_info_delete: Symbol<extern fn(_:  &mut AzTimerCallbackInfoPtr)>,
        pub az_timer_callback_info_shallow_copy: Symbol<extern fn(_:  &AzTimerCallbackInfoPtr) -> AzTimerCallbackInfoPtr>,
        pub az_timer_delete: Symbol<extern fn(_:  &mut AzTimer)>,
        pub az_timer_deep_copy: Symbol<extern fn(_:  &AzTimer) -> AzTimer>,
        pub az_task_new: Symbol<extern fn(_:  AzArcMutexRefAnyPtr, _:  AzTaskCallbackType) -> AzTaskPtr>,
        pub az_task_then: Symbol<extern fn(_:  AzTaskPtr, _:  AzTimer) -> AzTaskPtr>,
        pub az_task_delete: Symbol<extern fn(_:  &mut AzTaskPtr)>,
        pub az_task_shallow_copy: Symbol<extern fn(_:  &AzTaskPtr) -> AzTaskPtr>,
        pub az_thread_new: Symbol<extern fn(_:  AzRefAny, _:  AzThreadCallbackType) -> AzThreadPtr>,
        pub az_thread_block: Symbol<extern fn(_:  AzThreadPtr) -> AzResultRefAnyBlockError>,
        pub az_thread_delete: Symbol<extern fn(_:  &mut AzThreadPtr)>,
        pub az_thread_shallow_copy: Symbol<extern fn(_:  &AzThreadPtr) -> AzThreadPtr>,
        pub az_drop_check_delete: Symbol<extern fn(_:  &mut AzDropCheckPtr)>,
        pub az_drop_check_shallow_copy: Symbol<extern fn(_:  &AzDropCheckPtr) -> AzDropCheckPtr>,
        pub az_timer_id_delete: Symbol<extern fn(_:  &mut AzTimerId)>,
        pub az_timer_id_deep_copy: Symbol<extern fn(_:  &AzTimerId) -> AzTimerId>,
        pub az_terminate_timer_delete: Symbol<extern fn(_:  &mut AzTerminateTimer)>,
        pub az_terminate_timer_deep_copy: Symbol<extern fn(_:  &AzTerminateTimer) -> AzTerminateTimer>,
        pub az_block_error_delete: Symbol<extern fn(_:  &mut AzBlockError)>,
        pub az_block_error_deep_copy: Symbol<extern fn(_:  &AzBlockError) -> AzBlockError>,
        pub az_window_create_options_new: Symbol<extern fn(_:  AzCssPtr) -> AzWindowCreateOptionsPtr>,
        pub az_window_create_options_delete: Symbol<extern fn(_:  &mut AzWindowCreateOptionsPtr)>,
        pub az_window_create_options_shallow_copy: Symbol<extern fn(_:  &AzWindowCreateOptionsPtr) -> AzWindowCreateOptionsPtr>,
        pub az_logical_size_delete: Symbol<extern fn(_:  &mut AzLogicalSize)>,
        pub az_logical_size_deep_copy: Symbol<extern fn(_:  &AzLogicalSize) -> AzLogicalSize>,
        pub az_ref_any_new: Symbol<extern fn(_:  *const u8, _:  usize, _:  u64, _:  AzString, _:  fn(AzRefAny)) -> AzRefAny>,
        pub az_ref_any_get_ptr: Symbol<extern fn(_:  &AzRefAny, _:  usize, _:  u64) -> *const c_void>,
        pub az_ref_any_get_mut_ptr: Symbol<extern fn(_:  &AzRefAny, _:  usize, _:  u64) -> *mut c_void>,
        pub az_ref_any_shallow_copy: Symbol<extern fn(_:  &AzRefAny) -> AzRefAny>,
        pub az_ref_any_delete: Symbol<extern fn(_:  &mut AzRefAny)>,
        pub az_ref_any_core_copy: Symbol<extern fn(_:  &AzRefAny) -> AzRefAny>,
    }

    pub fn initialize_library(path: &std::path::Path) -> Option<AzulDll> {
        let lib = Library::new(path).ok()?;
        let az_string_from_utf8_unchecked = unsafe { lib.get::<extern fn(_:  *const u8, _:  usize) -> AzString>(b"az_string_from_utf8_unchecked").ok()? };
        let az_string_from_utf8_lossy = unsafe { lib.get::<extern fn(_:  *const u8, _:  usize) -> AzString>(b"az_string_from_utf8_lossy").ok()? };
        let az_string_into_bytes = unsafe { lib.get::<extern fn(_:  AzString) -> AzU8Vec>(b"az_string_into_bytes").ok()? };
        let az_string_delete = unsafe { lib.get::<extern fn(_:  &mut AzString)>(b"az_string_delete").ok()? };
        let az_string_deep_copy = unsafe { lib.get::<extern fn(_:  &AzString) -> AzString>(b"az_string_deep_copy").ok()? };
        let az_u8_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const u8, _:  usize) -> AzU8Vec>(b"az_u8_vec_copy_from").ok()? };
        let az_u8_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzU8Vec)>(b"az_u8_vec_delete").ok()? };
        let az_u8_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzU8Vec) -> AzU8Vec>(b"az_u8_vec_deep_copy").ok()? };
        let az_callback_data_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzCallbackData, _:  usize) -> AzCallbackDataVec>(b"az_callback_data_vec_copy_from").ok()? };
        let az_callback_data_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzCallbackDataVec)>(b"az_callback_data_vec_delete").ok()? };
        let az_callback_data_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCallbackDataVec) -> AzCallbackDataVec>(b"az_callback_data_vec_deep_copy").ok()? };
        let az_debug_message_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzDebugMessage, _:  usize) -> AzDebugMessageVec>(b"az_debug_message_vec_copy_from").ok()? };
        let az_debug_message_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzDebugMessageVec)>(b"az_debug_message_vec_delete").ok()? };
        let az_debug_message_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDebugMessageVec) -> AzDebugMessageVec>(b"az_debug_message_vec_deep_copy").ok()? };
        let az_g_luint_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const u32, _:  usize) -> AzGLuintVec>(b"az_g_luint_vec_copy_from").ok()? };
        let az_g_luint_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLuintVec)>(b"az_g_luint_vec_delete").ok()? };
        let az_g_luint_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGLuintVec) -> AzGLuintVec>(b"az_g_luint_vec_deep_copy").ok()? };
        let az_g_lint_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const i32, _:  usize) -> AzGLintVec>(b"az_g_lint_vec_copy_from").ok()? };
        let az_g_lint_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLintVec)>(b"az_g_lint_vec_delete").ok()? };
        let az_g_lint_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGLintVec) -> AzGLintVec>(b"az_g_lint_vec_deep_copy").ok()? };
        let az_override_property_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzOverrideProperty, _:  usize) -> AzOverridePropertyVec>(b"az_override_property_vec_copy_from").ok()? };
        let az_override_property_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzOverridePropertyVec)>(b"az_override_property_vec_delete").ok()? };
        let az_override_property_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOverridePropertyVec) -> AzOverridePropertyVec>(b"az_override_property_vec_deep_copy").ok()? };
        let az_dom_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzDom, _:  usize) -> AzDomVec>(b"az_dom_vec_copy_from").ok()? };
        let az_dom_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzDomVec)>(b"az_dom_vec_delete").ok()? };
        let az_dom_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDomVec) -> AzDomVec>(b"az_dom_vec_deep_copy").ok()? };
        let az_string_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzString, _:  usize) -> AzStringVec>(b"az_string_vec_copy_from").ok()? };
        let az_string_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzStringVec)>(b"az_string_vec_delete").ok()? };
        let az_string_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStringVec) -> AzStringVec>(b"az_string_vec_deep_copy").ok()? };
        let az_gradient_stop_pre_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzGradientStopPre, _:  usize) -> AzGradientStopPreVec>(b"az_gradient_stop_pre_vec_copy_from").ok()? };
        let az_gradient_stop_pre_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzGradientStopPreVec)>(b"az_gradient_stop_pre_vec_delete").ok()? };
        let az_gradient_stop_pre_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGradientStopPreVec) -> AzGradientStopPreVec>(b"az_gradient_stop_pre_vec_deep_copy").ok()? };
        let az_option_percentage_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionPercentageValue)>(b"az_option_percentage_value_delete").ok()? };
        let az_option_percentage_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionPercentageValue) -> AzOptionPercentageValue>(b"az_option_percentage_value_deep_copy").ok()? };
        let az_option_dom_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionDom)>(b"az_option_dom_delete").ok()? };
        let az_option_dom_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionDom) -> AzOptionDom>(b"az_option_dom_deep_copy").ok()? };
        let az_option_texture_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionTexture)>(b"az_option_texture_delete").ok()? };
        let az_option_tab_index_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionTabIndex)>(b"az_option_tab_index_delete").ok()? };
        let az_option_tab_index_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionTabIndex) -> AzOptionTabIndex>(b"az_option_tab_index_deep_copy").ok()? };
        let az_option_duration_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionDuration)>(b"az_option_duration_delete").ok()? };
        let az_option_duration_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionDuration) -> AzOptionDuration>(b"az_option_duration_deep_copy").ok()? };
        let az_option_instant_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionInstant)>(b"az_option_instant_delete").ok()? };
        let az_option_instant_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionInstant) -> AzOptionInstant>(b"az_option_instant_deep_copy").ok()? };
        let az_option_usize_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionUsize)>(b"az_option_usize_delete").ok()? };
        let az_option_usize_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionUsize) -> AzOptionUsize>(b"az_option_usize_deep_copy").ok()? };
        let az_option_u8_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionU8VecRef)>(b"az_option_u8_vec_ref_delete").ok()? };
        let az_result_ref_any_block_error_delete = unsafe { lib.get::<extern fn(_:  &mut AzResultRefAnyBlockError)>(b"az_result_ref_any_block_error_delete").ok()? };
        let az_result_ref_any_block_error_deep_copy = unsafe { lib.get::<extern fn(_:  &AzResultRefAnyBlockError) -> AzResultRefAnyBlockError>(b"az_result_ref_any_block_error_deep_copy").ok()? };
        let az_instant_now = unsafe { lib.get::<extern fn() -> AzInstantPtr>(b"az_instant_now").ok()? };
        let az_instant_delete = unsafe { lib.get::<extern fn(_:  &mut AzInstantPtr)>(b"az_instant_delete").ok()? };
        let az_instant_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzInstantPtr) -> AzInstantPtr>(b"az_instant_shallow_copy").ok()? };
        let az_duration_delete = unsafe { lib.get::<extern fn(_:  &mut AzDuration)>(b"az_duration_delete").ok()? };
        let az_duration_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDuration) -> AzDuration>(b"az_duration_deep_copy").ok()? };
        let az_app_config_default = unsafe { lib.get::<extern fn() -> AzAppConfigPtr>(b"az_app_config_default").ok()? };
        let az_app_config_delete = unsafe { lib.get::<extern fn(_:  &mut AzAppConfigPtr)>(b"az_app_config_delete").ok()? };
        let az_app_config_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzAppConfigPtr) -> AzAppConfigPtr>(b"az_app_config_shallow_copy").ok()? };
        let az_app_new = unsafe { lib.get::<extern fn(_:  AzRefAny, _:  AzAppConfigPtr, _:  AzLayoutCallbackType) -> AzAppPtr>(b"az_app_new").ok()? };
        let az_app_run = unsafe { lib.get::<extern fn(_:  AzAppPtr, _:  AzWindowCreateOptionsPtr)>(b"az_app_run").ok()? };
        let az_app_delete = unsafe { lib.get::<extern fn(_:  &mut AzAppPtr)>(b"az_app_delete").ok()? };
        let az_app_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzAppPtr) -> AzAppPtr>(b"az_app_shallow_copy").ok()? };
        let az_layout_callback_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutCallback)>(b"az_layout_callback_delete").ok()? };
        let az_layout_callback_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutCallback) -> AzLayoutCallback>(b"az_layout_callback_deep_copy").ok()? };
        let az_callback_delete = unsafe { lib.get::<extern fn(_:  &mut AzCallback)>(b"az_callback_delete").ok()? };
        let az_callback_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCallback) -> AzCallback>(b"az_callback_deep_copy").ok()? };
        let az_callback_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzCallbackInfoPtr)>(b"az_callback_info_delete").ok()? };
        let az_callback_info_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzCallbackInfoPtr) -> AzCallbackInfoPtr>(b"az_callback_info_shallow_copy").ok()? };
        let az_update_screen_delete = unsafe { lib.get::<extern fn(_:  &mut AzUpdateScreen)>(b"az_update_screen_delete").ok()? };
        let az_update_screen_deep_copy = unsafe { lib.get::<extern fn(_:  &AzUpdateScreen) -> AzUpdateScreen>(b"az_update_screen_deep_copy").ok()? };
        let az_i_frame_callback_delete = unsafe { lib.get::<extern fn(_:  &mut AzIFrameCallback)>(b"az_i_frame_callback_delete").ok()? };
        let az_i_frame_callback_deep_copy = unsafe { lib.get::<extern fn(_:  &AzIFrameCallback) -> AzIFrameCallback>(b"az_i_frame_callback_deep_copy").ok()? };
        let az_i_frame_callback_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzIFrameCallbackInfoPtr)>(b"az_i_frame_callback_info_delete").ok()? };
        let az_i_frame_callback_info_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzIFrameCallbackInfoPtr) -> AzIFrameCallbackInfoPtr>(b"az_i_frame_callback_info_shallow_copy").ok()? };
        let az_i_frame_callback_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzIFrameCallbackReturn)>(b"az_i_frame_callback_return_delete").ok()? };
        let az_i_frame_callback_return_deep_copy = unsafe { lib.get::<extern fn(_:  &AzIFrameCallbackReturn) -> AzIFrameCallbackReturn>(b"az_i_frame_callback_return_deep_copy").ok()? };
        let az_gl_callback_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlCallback)>(b"az_gl_callback_delete").ok()? };
        let az_gl_callback_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGlCallback) -> AzGlCallback>(b"az_gl_callback_deep_copy").ok()? };
        let az_gl_callback_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlCallbackInfoPtr)>(b"az_gl_callback_info_delete").ok()? };
        let az_gl_callback_info_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzGlCallbackInfoPtr) -> AzGlCallbackInfoPtr>(b"az_gl_callback_info_shallow_copy").ok()? };
        let az_gl_callback_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlCallbackReturn)>(b"az_gl_callback_return_delete").ok()? };
        let az_timer_callback_delete = unsafe { lib.get::<extern fn(_:  &mut AzTimerCallback)>(b"az_timer_callback_delete").ok()? };
        let az_timer_callback_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTimerCallback) -> AzTimerCallback>(b"az_timer_callback_deep_copy").ok()? };
        let az_timer_callback_type_delete = unsafe { lib.get::<extern fn(_:  &mut AzTimerCallbackTypePtr)>(b"az_timer_callback_type_delete").ok()? };
        let az_timer_callback_type_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzTimerCallbackTypePtr) -> AzTimerCallbackTypePtr>(b"az_timer_callback_type_shallow_copy").ok()? };
        let az_timer_callback_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzTimerCallbackReturn)>(b"az_timer_callback_return_delete").ok()? };
        let az_timer_callback_return_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTimerCallbackReturn) -> AzTimerCallbackReturn>(b"az_timer_callback_return_deep_copy").ok()? };
        let az_layout_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutInfoPtr)>(b"az_layout_info_delete").ok()? };
        let az_layout_info_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutInfoPtr) -> AzLayoutInfoPtr>(b"az_layout_info_shallow_copy").ok()? };
        let az_css_native = unsafe { lib.get::<extern fn() -> AzCssPtr>(b"az_css_native").ok()? };
        let az_css_empty = unsafe { lib.get::<extern fn() -> AzCssPtr>(b"az_css_empty").ok()? };
        let az_css_from_string = unsafe { lib.get::<extern fn(_:  AzString) -> AzCssPtr>(b"az_css_from_string").ok()? };
        let az_css_override_native = unsafe { lib.get::<extern fn(_:  AzString) -> AzCssPtr>(b"az_css_override_native").ok()? };
        let az_css_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssPtr)>(b"az_css_delete").ok()? };
        let az_css_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzCssPtr) -> AzCssPtr>(b"az_css_shallow_copy").ok()? };
        let az_css_hot_reloader_new = unsafe { lib.get::<extern fn(_:  AzString, _:  u64) -> AzCssHotReloaderPtr>(b"az_css_hot_reloader_new").ok()? };
        let az_css_hot_reloader_override_native = unsafe { lib.get::<extern fn(_:  AzString, _:  u64) -> AzCssHotReloaderPtr>(b"az_css_hot_reloader_override_native").ok()? };
        let az_css_hot_reloader_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssHotReloaderPtr)>(b"az_css_hot_reloader_delete").ok()? };
        let az_css_hot_reloader_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzCssHotReloaderPtr) -> AzCssHotReloaderPtr>(b"az_css_hot_reloader_shallow_copy").ok()? };
        let az_color_u_delete = unsafe { lib.get::<extern fn(_:  &mut AzColorU)>(b"az_color_u_delete").ok()? };
        let az_color_u_deep_copy = unsafe { lib.get::<extern fn(_:  &AzColorU) -> AzColorU>(b"az_color_u_deep_copy").ok()? };
        let az_size_metric_delete = unsafe { lib.get::<extern fn(_:  &mut AzSizeMetric)>(b"az_size_metric_delete").ok()? };
        let az_size_metric_deep_copy = unsafe { lib.get::<extern fn(_:  &AzSizeMetric) -> AzSizeMetric>(b"az_size_metric_deep_copy").ok()? };
        let az_float_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzFloatValue)>(b"az_float_value_delete").ok()? };
        let az_float_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzFloatValue) -> AzFloatValue>(b"az_float_value_deep_copy").ok()? };
        let az_pixel_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzPixelValue)>(b"az_pixel_value_delete").ok()? };
        let az_pixel_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzPixelValue) -> AzPixelValue>(b"az_pixel_value_deep_copy").ok()? };
        let az_pixel_value_no_percent_delete = unsafe { lib.get::<extern fn(_:  &mut AzPixelValueNoPercent)>(b"az_pixel_value_no_percent_delete").ok()? };
        let az_pixel_value_no_percent_deep_copy = unsafe { lib.get::<extern fn(_:  &AzPixelValueNoPercent) -> AzPixelValueNoPercent>(b"az_pixel_value_no_percent_deep_copy").ok()? };
        let az_box_shadow_clip_mode_delete = unsafe { lib.get::<extern fn(_:  &mut AzBoxShadowClipMode)>(b"az_box_shadow_clip_mode_delete").ok()? };
        let az_box_shadow_clip_mode_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBoxShadowClipMode) -> AzBoxShadowClipMode>(b"az_box_shadow_clip_mode_deep_copy").ok()? };
        let az_box_shadow_pre_display_item_delete = unsafe { lib.get::<extern fn(_:  &mut AzBoxShadowPreDisplayItem)>(b"az_box_shadow_pre_display_item_delete").ok()? };
        let az_box_shadow_pre_display_item_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBoxShadowPreDisplayItem) -> AzBoxShadowPreDisplayItem>(b"az_box_shadow_pre_display_item_deep_copy").ok()? };
        let az_layout_align_content_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutAlignContent)>(b"az_layout_align_content_delete").ok()? };
        let az_layout_align_content_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutAlignContent) -> AzLayoutAlignContent>(b"az_layout_align_content_deep_copy").ok()? };
        let az_layout_align_items_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutAlignItems)>(b"az_layout_align_items_delete").ok()? };
        let az_layout_align_items_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutAlignItems) -> AzLayoutAlignItems>(b"az_layout_align_items_deep_copy").ok()? };
        let az_layout_bottom_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutBottom)>(b"az_layout_bottom_delete").ok()? };
        let az_layout_bottom_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutBottom) -> AzLayoutBottom>(b"az_layout_bottom_deep_copy").ok()? };
        let az_layout_box_sizing_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutBoxSizing)>(b"az_layout_box_sizing_delete").ok()? };
        let az_layout_box_sizing_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutBoxSizing) -> AzLayoutBoxSizing>(b"az_layout_box_sizing_deep_copy").ok()? };
        let az_layout_direction_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutDirection)>(b"az_layout_direction_delete").ok()? };
        let az_layout_direction_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutDirection) -> AzLayoutDirection>(b"az_layout_direction_deep_copy").ok()? };
        let az_layout_display_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutDisplay)>(b"az_layout_display_delete").ok()? };
        let az_layout_display_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutDisplay) -> AzLayoutDisplay>(b"az_layout_display_deep_copy").ok()? };
        let az_layout_flex_grow_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutFlexGrow)>(b"az_layout_flex_grow_delete").ok()? };
        let az_layout_flex_grow_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutFlexGrow) -> AzLayoutFlexGrow>(b"az_layout_flex_grow_deep_copy").ok()? };
        let az_layout_flex_shrink_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutFlexShrink)>(b"az_layout_flex_shrink_delete").ok()? };
        let az_layout_flex_shrink_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutFlexShrink) -> AzLayoutFlexShrink>(b"az_layout_flex_shrink_deep_copy").ok()? };
        let az_layout_float_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutFloat)>(b"az_layout_float_delete").ok()? };
        let az_layout_float_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutFloat) -> AzLayoutFloat>(b"az_layout_float_deep_copy").ok()? };
        let az_layout_height_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutHeight)>(b"az_layout_height_delete").ok()? };
        let az_layout_height_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutHeight) -> AzLayoutHeight>(b"az_layout_height_deep_copy").ok()? };
        let az_layout_justify_content_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutJustifyContent)>(b"az_layout_justify_content_delete").ok()? };
        let az_layout_justify_content_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutJustifyContent) -> AzLayoutJustifyContent>(b"az_layout_justify_content_deep_copy").ok()? };
        let az_layout_left_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutLeft)>(b"az_layout_left_delete").ok()? };
        let az_layout_left_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutLeft) -> AzLayoutLeft>(b"az_layout_left_deep_copy").ok()? };
        let az_layout_margin_bottom_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginBottom)>(b"az_layout_margin_bottom_delete").ok()? };
        let az_layout_margin_bottom_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginBottom) -> AzLayoutMarginBottom>(b"az_layout_margin_bottom_deep_copy").ok()? };
        let az_layout_margin_left_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginLeft)>(b"az_layout_margin_left_delete").ok()? };
        let az_layout_margin_left_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginLeft) -> AzLayoutMarginLeft>(b"az_layout_margin_left_deep_copy").ok()? };
        let az_layout_margin_right_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginRight)>(b"az_layout_margin_right_delete").ok()? };
        let az_layout_margin_right_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginRight) -> AzLayoutMarginRight>(b"az_layout_margin_right_deep_copy").ok()? };
        let az_layout_margin_top_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginTop)>(b"az_layout_margin_top_delete").ok()? };
        let az_layout_margin_top_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginTop) -> AzLayoutMarginTop>(b"az_layout_margin_top_deep_copy").ok()? };
        let az_layout_max_height_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMaxHeight)>(b"az_layout_max_height_delete").ok()? };
        let az_layout_max_height_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMaxHeight) -> AzLayoutMaxHeight>(b"az_layout_max_height_deep_copy").ok()? };
        let az_layout_max_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMaxWidth)>(b"az_layout_max_width_delete").ok()? };
        let az_layout_max_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMaxWidth) -> AzLayoutMaxWidth>(b"az_layout_max_width_deep_copy").ok()? };
        let az_layout_min_height_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMinHeight)>(b"az_layout_min_height_delete").ok()? };
        let az_layout_min_height_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMinHeight) -> AzLayoutMinHeight>(b"az_layout_min_height_deep_copy").ok()? };
        let az_layout_min_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMinWidth)>(b"az_layout_min_width_delete").ok()? };
        let az_layout_min_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMinWidth) -> AzLayoutMinWidth>(b"az_layout_min_width_deep_copy").ok()? };
        let az_layout_padding_bottom_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingBottom)>(b"az_layout_padding_bottom_delete").ok()? };
        let az_layout_padding_bottom_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingBottom) -> AzLayoutPaddingBottom>(b"az_layout_padding_bottom_deep_copy").ok()? };
        let az_layout_padding_left_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingLeft)>(b"az_layout_padding_left_delete").ok()? };
        let az_layout_padding_left_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingLeft) -> AzLayoutPaddingLeft>(b"az_layout_padding_left_deep_copy").ok()? };
        let az_layout_padding_right_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingRight)>(b"az_layout_padding_right_delete").ok()? };
        let az_layout_padding_right_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingRight) -> AzLayoutPaddingRight>(b"az_layout_padding_right_deep_copy").ok()? };
        let az_layout_padding_top_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingTop)>(b"az_layout_padding_top_delete").ok()? };
        let az_layout_padding_top_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingTop) -> AzLayoutPaddingTop>(b"az_layout_padding_top_deep_copy").ok()? };
        let az_layout_position_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPosition)>(b"az_layout_position_delete").ok()? };
        let az_layout_position_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPosition) -> AzLayoutPosition>(b"az_layout_position_deep_copy").ok()? };
        let az_layout_right_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutRight)>(b"az_layout_right_delete").ok()? };
        let az_layout_right_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutRight) -> AzLayoutRight>(b"az_layout_right_deep_copy").ok()? };
        let az_layout_top_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutTop)>(b"az_layout_top_delete").ok()? };
        let az_layout_top_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutTop) -> AzLayoutTop>(b"az_layout_top_deep_copy").ok()? };
        let az_layout_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutWidth)>(b"az_layout_width_delete").ok()? };
        let az_layout_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutWidth) -> AzLayoutWidth>(b"az_layout_width_deep_copy").ok()? };
        let az_layout_wrap_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutWrap)>(b"az_layout_wrap_delete").ok()? };
        let az_layout_wrap_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutWrap) -> AzLayoutWrap>(b"az_layout_wrap_deep_copy").ok()? };
        let az_overflow_delete = unsafe { lib.get::<extern fn(_:  &mut AzOverflow)>(b"az_overflow_delete").ok()? };
        let az_overflow_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOverflow) -> AzOverflow>(b"az_overflow_deep_copy").ok()? };
        let az_percentage_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzPercentageValue)>(b"az_percentage_value_delete").ok()? };
        let az_percentage_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzPercentageValue) -> AzPercentageValue>(b"az_percentage_value_deep_copy").ok()? };
        let az_gradient_stop_pre_delete = unsafe { lib.get::<extern fn(_:  &mut AzGradientStopPre)>(b"az_gradient_stop_pre_delete").ok()? };
        let az_gradient_stop_pre_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGradientStopPre) -> AzGradientStopPre>(b"az_gradient_stop_pre_deep_copy").ok()? };
        let az_direction_corner_delete = unsafe { lib.get::<extern fn(_:  &mut AzDirectionCorner)>(b"az_direction_corner_delete").ok()? };
        let az_direction_corner_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDirectionCorner) -> AzDirectionCorner>(b"az_direction_corner_deep_copy").ok()? };
        let az_direction_corners_delete = unsafe { lib.get::<extern fn(_:  &mut AzDirectionCorners)>(b"az_direction_corners_delete").ok()? };
        let az_direction_corners_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDirectionCorners) -> AzDirectionCorners>(b"az_direction_corners_deep_copy").ok()? };
        let az_direction_delete = unsafe { lib.get::<extern fn(_:  &mut AzDirection)>(b"az_direction_delete").ok()? };
        let az_direction_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDirection) -> AzDirection>(b"az_direction_deep_copy").ok()? };
        let az_extend_mode_delete = unsafe { lib.get::<extern fn(_:  &mut AzExtendMode)>(b"az_extend_mode_delete").ok()? };
        let az_extend_mode_deep_copy = unsafe { lib.get::<extern fn(_:  &AzExtendMode) -> AzExtendMode>(b"az_extend_mode_deep_copy").ok()? };
        let az_linear_gradient_delete = unsafe { lib.get::<extern fn(_:  &mut AzLinearGradient)>(b"az_linear_gradient_delete").ok()? };
        let az_linear_gradient_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLinearGradient) -> AzLinearGradient>(b"az_linear_gradient_deep_copy").ok()? };
        let az_shape_delete = unsafe { lib.get::<extern fn(_:  &mut AzShape)>(b"az_shape_delete").ok()? };
        let az_shape_deep_copy = unsafe { lib.get::<extern fn(_:  &AzShape) -> AzShape>(b"az_shape_deep_copy").ok()? };
        let az_radial_gradient_delete = unsafe { lib.get::<extern fn(_:  &mut AzRadialGradient)>(b"az_radial_gradient_delete").ok()? };
        let az_radial_gradient_deep_copy = unsafe { lib.get::<extern fn(_:  &AzRadialGradient) -> AzRadialGradient>(b"az_radial_gradient_deep_copy").ok()? };
        let az_css_image_id_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssImageId)>(b"az_css_image_id_delete").ok()? };
        let az_css_image_id_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssImageId) -> AzCssImageId>(b"az_css_image_id_deep_copy").ok()? };
        let az_style_background_content_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundContent)>(b"az_style_background_content_delete").ok()? };
        let az_style_background_content_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundContent) -> AzStyleBackgroundContent>(b"az_style_background_content_deep_copy").ok()? };
        let az_background_position_horizontal_delete = unsafe { lib.get::<extern fn(_:  &mut AzBackgroundPositionHorizontal)>(b"az_background_position_horizontal_delete").ok()? };
        let az_background_position_horizontal_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBackgroundPositionHorizontal) -> AzBackgroundPositionHorizontal>(b"az_background_position_horizontal_deep_copy").ok()? };
        let az_background_position_vertical_delete = unsafe { lib.get::<extern fn(_:  &mut AzBackgroundPositionVertical)>(b"az_background_position_vertical_delete").ok()? };
        let az_background_position_vertical_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBackgroundPositionVertical) -> AzBackgroundPositionVertical>(b"az_background_position_vertical_deep_copy").ok()? };
        let az_style_background_position_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundPosition)>(b"az_style_background_position_delete").ok()? };
        let az_style_background_position_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundPosition) -> AzStyleBackgroundPosition>(b"az_style_background_position_deep_copy").ok()? };
        let az_style_background_repeat_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundRepeat)>(b"az_style_background_repeat_delete").ok()? };
        let az_style_background_repeat_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundRepeat) -> AzStyleBackgroundRepeat>(b"az_style_background_repeat_deep_copy").ok()? };
        let az_style_background_size_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundSize)>(b"az_style_background_size_delete").ok()? };
        let az_style_background_size_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundSize) -> AzStyleBackgroundSize>(b"az_style_background_size_deep_copy").ok()? };
        let az_style_border_bottom_color_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomColor)>(b"az_style_border_bottom_color_delete").ok()? };
        let az_style_border_bottom_color_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomColor) -> AzStyleBorderBottomColor>(b"az_style_border_bottom_color_deep_copy").ok()? };
        let az_style_border_bottom_left_radius_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomLeftRadius)>(b"az_style_border_bottom_left_radius_delete").ok()? };
        let az_style_border_bottom_left_radius_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadius>(b"az_style_border_bottom_left_radius_deep_copy").ok()? };
        let az_style_border_bottom_right_radius_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomRightRadius)>(b"az_style_border_bottom_right_radius_delete").ok()? };
        let az_style_border_bottom_right_radius_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadius>(b"az_style_border_bottom_right_radius_deep_copy").ok()? };
        let az_border_style_delete = unsafe { lib.get::<extern fn(_:  &mut AzBorderStyle)>(b"az_border_style_delete").ok()? };
        let az_border_style_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBorderStyle) -> AzBorderStyle>(b"az_border_style_deep_copy").ok()? };
        let az_style_border_bottom_style_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomStyle)>(b"az_style_border_bottom_style_delete").ok()? };
        let az_style_border_bottom_style_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomStyle) -> AzStyleBorderBottomStyle>(b"az_style_border_bottom_style_deep_copy").ok()? };
        let az_style_border_bottom_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomWidth)>(b"az_style_border_bottom_width_delete").ok()? };
        let az_style_border_bottom_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomWidth) -> AzStyleBorderBottomWidth>(b"az_style_border_bottom_width_deep_copy").ok()? };
        let az_style_border_left_color_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderLeftColor)>(b"az_style_border_left_color_delete").ok()? };
        let az_style_border_left_color_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderLeftColor) -> AzStyleBorderLeftColor>(b"az_style_border_left_color_deep_copy").ok()? };
        let az_style_border_left_style_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderLeftStyle)>(b"az_style_border_left_style_delete").ok()? };
        let az_style_border_left_style_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderLeftStyle) -> AzStyleBorderLeftStyle>(b"az_style_border_left_style_deep_copy").ok()? };
        let az_style_border_left_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderLeftWidth)>(b"az_style_border_left_width_delete").ok()? };
        let az_style_border_left_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderLeftWidth) -> AzStyleBorderLeftWidth>(b"az_style_border_left_width_deep_copy").ok()? };
        let az_style_border_right_color_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderRightColor)>(b"az_style_border_right_color_delete").ok()? };
        let az_style_border_right_color_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderRightColor) -> AzStyleBorderRightColor>(b"az_style_border_right_color_deep_copy").ok()? };
        let az_style_border_right_style_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderRightStyle)>(b"az_style_border_right_style_delete").ok()? };
        let az_style_border_right_style_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderRightStyle) -> AzStyleBorderRightStyle>(b"az_style_border_right_style_deep_copy").ok()? };
        let az_style_border_right_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderRightWidth)>(b"az_style_border_right_width_delete").ok()? };
        let az_style_border_right_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderRightWidth) -> AzStyleBorderRightWidth>(b"az_style_border_right_width_deep_copy").ok()? };
        let az_style_border_top_color_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopColor)>(b"az_style_border_top_color_delete").ok()? };
        let az_style_border_top_color_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopColor) -> AzStyleBorderTopColor>(b"az_style_border_top_color_deep_copy").ok()? };
        let az_style_border_top_left_radius_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopLeftRadius)>(b"az_style_border_top_left_radius_delete").ok()? };
        let az_style_border_top_left_radius_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopLeftRadius) -> AzStyleBorderTopLeftRadius>(b"az_style_border_top_left_radius_deep_copy").ok()? };
        let az_style_border_top_right_radius_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopRightRadius)>(b"az_style_border_top_right_radius_delete").ok()? };
        let az_style_border_top_right_radius_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopRightRadius) -> AzStyleBorderTopRightRadius>(b"az_style_border_top_right_radius_deep_copy").ok()? };
        let az_style_border_top_style_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopStyle)>(b"az_style_border_top_style_delete").ok()? };
        let az_style_border_top_style_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopStyle) -> AzStyleBorderTopStyle>(b"az_style_border_top_style_deep_copy").ok()? };
        let az_style_border_top_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopWidth)>(b"az_style_border_top_width_delete").ok()? };
        let az_style_border_top_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopWidth) -> AzStyleBorderTopWidth>(b"az_style_border_top_width_deep_copy").ok()? };
        let az_style_cursor_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleCursor)>(b"az_style_cursor_delete").ok()? };
        let az_style_cursor_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleCursor) -> AzStyleCursor>(b"az_style_cursor_deep_copy").ok()? };
        let az_style_font_family_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleFontFamily)>(b"az_style_font_family_delete").ok()? };
        let az_style_font_family_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleFontFamily) -> AzStyleFontFamily>(b"az_style_font_family_deep_copy").ok()? };
        let az_style_font_size_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleFontSize)>(b"az_style_font_size_delete").ok()? };
        let az_style_font_size_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleFontSize) -> AzStyleFontSize>(b"az_style_font_size_deep_copy").ok()? };
        let az_style_letter_spacing_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleLetterSpacing)>(b"az_style_letter_spacing_delete").ok()? };
        let az_style_letter_spacing_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleLetterSpacing) -> AzStyleLetterSpacing>(b"az_style_letter_spacing_deep_copy").ok()? };
        let az_style_line_height_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleLineHeight)>(b"az_style_line_height_delete").ok()? };
        let az_style_line_height_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleLineHeight) -> AzStyleLineHeight>(b"az_style_line_height_deep_copy").ok()? };
        let az_style_tab_width_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleTabWidth)>(b"az_style_tab_width_delete").ok()? };
        let az_style_tab_width_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleTabWidth) -> AzStyleTabWidth>(b"az_style_tab_width_deep_copy").ok()? };
        let az_style_text_alignment_horz_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleTextAlignmentHorz)>(b"az_style_text_alignment_horz_delete").ok()? };
        let az_style_text_alignment_horz_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleTextAlignmentHorz) -> AzStyleTextAlignmentHorz>(b"az_style_text_alignment_horz_deep_copy").ok()? };
        let az_style_text_color_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleTextColor)>(b"az_style_text_color_delete").ok()? };
        let az_style_text_color_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleTextColor) -> AzStyleTextColor>(b"az_style_text_color_deep_copy").ok()? };
        let az_style_word_spacing_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleWordSpacing)>(b"az_style_word_spacing_delete").ok()? };
        let az_style_word_spacing_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleWordSpacing) -> AzStyleWordSpacing>(b"az_style_word_spacing_deep_copy").ok()? };
        let az_box_shadow_pre_display_item_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzBoxShadowPreDisplayItemValue)>(b"az_box_shadow_pre_display_item_value_delete").ok()? };
        let az_box_shadow_pre_display_item_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBoxShadowPreDisplayItemValue) -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_deep_copy").ok()? };
        let az_layout_align_content_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutAlignContentValue)>(b"az_layout_align_content_value_delete").ok()? };
        let az_layout_align_content_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutAlignContentValue) -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_deep_copy").ok()? };
        let az_layout_align_items_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutAlignItemsValue)>(b"az_layout_align_items_value_delete").ok()? };
        let az_layout_align_items_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutAlignItemsValue) -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_deep_copy").ok()? };
        let az_layout_bottom_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutBottomValue)>(b"az_layout_bottom_value_delete").ok()? };
        let az_layout_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutBottomValue) -> AzLayoutBottomValue>(b"az_layout_bottom_value_deep_copy").ok()? };
        let az_layout_box_sizing_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutBoxSizingValue)>(b"az_layout_box_sizing_value_delete").ok()? };
        let az_layout_box_sizing_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutBoxSizingValue) -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_deep_copy").ok()? };
        let az_layout_direction_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutDirectionValue)>(b"az_layout_direction_value_delete").ok()? };
        let az_layout_direction_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutDirectionValue) -> AzLayoutDirectionValue>(b"az_layout_direction_value_deep_copy").ok()? };
        let az_layout_display_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutDisplayValue)>(b"az_layout_display_value_delete").ok()? };
        let az_layout_display_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutDisplayValue) -> AzLayoutDisplayValue>(b"az_layout_display_value_deep_copy").ok()? };
        let az_layout_flex_grow_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutFlexGrowValue)>(b"az_layout_flex_grow_value_delete").ok()? };
        let az_layout_flex_grow_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutFlexGrowValue) -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_deep_copy").ok()? };
        let az_layout_flex_shrink_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutFlexShrinkValue)>(b"az_layout_flex_shrink_value_delete").ok()? };
        let az_layout_flex_shrink_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutFlexShrinkValue) -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_deep_copy").ok()? };
        let az_layout_float_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutFloatValue)>(b"az_layout_float_value_delete").ok()? };
        let az_layout_float_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutFloatValue) -> AzLayoutFloatValue>(b"az_layout_float_value_deep_copy").ok()? };
        let az_layout_height_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutHeightValue)>(b"az_layout_height_value_delete").ok()? };
        let az_layout_height_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutHeightValue) -> AzLayoutHeightValue>(b"az_layout_height_value_deep_copy").ok()? };
        let az_layout_justify_content_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutJustifyContentValue)>(b"az_layout_justify_content_value_delete").ok()? };
        let az_layout_justify_content_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutJustifyContentValue) -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_deep_copy").ok()? };
        let az_layout_left_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutLeftValue)>(b"az_layout_left_value_delete").ok()? };
        let az_layout_left_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutLeftValue) -> AzLayoutLeftValue>(b"az_layout_left_value_deep_copy").ok()? };
        let az_layout_margin_bottom_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginBottomValue)>(b"az_layout_margin_bottom_value_delete").ok()? };
        let az_layout_margin_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginBottomValue) -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_deep_copy").ok()? };
        let az_layout_margin_left_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginLeftValue)>(b"az_layout_margin_left_value_delete").ok()? };
        let az_layout_margin_left_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginLeftValue) -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_deep_copy").ok()? };
        let az_layout_margin_right_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginRightValue)>(b"az_layout_margin_right_value_delete").ok()? };
        let az_layout_margin_right_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginRightValue) -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_deep_copy").ok()? };
        let az_layout_margin_top_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMarginTopValue)>(b"az_layout_margin_top_value_delete").ok()? };
        let az_layout_margin_top_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMarginTopValue) -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_deep_copy").ok()? };
        let az_layout_max_height_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMaxHeightValue)>(b"az_layout_max_height_value_delete").ok()? };
        let az_layout_max_height_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMaxHeightValue) -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_deep_copy").ok()? };
        let az_layout_max_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMaxWidthValue)>(b"az_layout_max_width_value_delete").ok()? };
        let az_layout_max_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMaxWidthValue) -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_deep_copy").ok()? };
        let az_layout_min_height_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMinHeightValue)>(b"az_layout_min_height_value_delete").ok()? };
        let az_layout_min_height_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMinHeightValue) -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_deep_copy").ok()? };
        let az_layout_min_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutMinWidthValue)>(b"az_layout_min_width_value_delete").ok()? };
        let az_layout_min_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutMinWidthValue) -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_deep_copy").ok()? };
        let az_layout_padding_bottom_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingBottomValue)>(b"az_layout_padding_bottom_value_delete").ok()? };
        let az_layout_padding_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingBottomValue) -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_deep_copy").ok()? };
        let az_layout_padding_left_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingLeftValue)>(b"az_layout_padding_left_value_delete").ok()? };
        let az_layout_padding_left_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingLeftValue) -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_deep_copy").ok()? };
        let az_layout_padding_right_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingRightValue)>(b"az_layout_padding_right_value_delete").ok()? };
        let az_layout_padding_right_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingRightValue) -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_deep_copy").ok()? };
        let az_layout_padding_top_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPaddingTopValue)>(b"az_layout_padding_top_value_delete").ok()? };
        let az_layout_padding_top_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPaddingTopValue) -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_deep_copy").ok()? };
        let az_layout_position_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutPositionValue)>(b"az_layout_position_value_delete").ok()? };
        let az_layout_position_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutPositionValue) -> AzLayoutPositionValue>(b"az_layout_position_value_deep_copy").ok()? };
        let az_layout_right_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutRightValue)>(b"az_layout_right_value_delete").ok()? };
        let az_layout_right_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutRightValue) -> AzLayoutRightValue>(b"az_layout_right_value_deep_copy").ok()? };
        let az_layout_top_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutTopValue)>(b"az_layout_top_value_delete").ok()? };
        let az_layout_top_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutTopValue) -> AzLayoutTopValue>(b"az_layout_top_value_deep_copy").ok()? };
        let az_layout_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutWidthValue)>(b"az_layout_width_value_delete").ok()? };
        let az_layout_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutWidthValue) -> AzLayoutWidthValue>(b"az_layout_width_value_deep_copy").ok()? };
        let az_layout_wrap_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzLayoutWrapValue)>(b"az_layout_wrap_value_delete").ok()? };
        let az_layout_wrap_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLayoutWrapValue) -> AzLayoutWrapValue>(b"az_layout_wrap_value_deep_copy").ok()? };
        let az_overflow_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzOverflowValue)>(b"az_overflow_value_delete").ok()? };
        let az_overflow_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOverflowValue) -> AzOverflowValue>(b"az_overflow_value_deep_copy").ok()? };
        let az_style_background_content_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundContentValue)>(b"az_style_background_content_value_delete").ok()? };
        let az_style_background_content_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_deep_copy").ok()? };
        let az_style_background_position_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundPositionValue)>(b"az_style_background_position_value_delete").ok()? };
        let az_style_background_position_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundPositionValue) -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_deep_copy").ok()? };
        let az_style_background_repeat_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundRepeatValue)>(b"az_style_background_repeat_value_delete").ok()? };
        let az_style_background_repeat_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundRepeatValue) -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_deep_copy").ok()? };
        let az_style_background_size_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBackgroundSizeValue)>(b"az_style_background_size_value_delete").ok()? };
        let az_style_background_size_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBackgroundSizeValue) -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_deep_copy").ok()? };
        let az_style_border_bottom_color_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomColorValue)>(b"az_style_border_bottom_color_value_delete").ok()? };
        let az_style_border_bottom_color_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomColorValue) -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_deep_copy").ok()? };
        let az_style_border_bottom_left_radius_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomLeftRadiusValue)>(b"az_style_border_bottom_left_radius_value_delete").ok()? };
        let az_style_border_bottom_left_radius_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomLeftRadiusValue) -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_deep_copy").ok()? };
        let az_style_border_bottom_right_radius_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomRightRadiusValue)>(b"az_style_border_bottom_right_radius_value_delete").ok()? };
        let az_style_border_bottom_right_radius_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomRightRadiusValue) -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_deep_copy").ok()? };
        let az_style_border_bottom_style_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomStyleValue)>(b"az_style_border_bottom_style_value_delete").ok()? };
        let az_style_border_bottom_style_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomStyleValue) -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_deep_copy").ok()? };
        let az_style_border_bottom_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderBottomWidthValue)>(b"az_style_border_bottom_width_value_delete").ok()? };
        let az_style_border_bottom_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderBottomWidthValue) -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_deep_copy").ok()? };
        let az_style_border_left_color_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderLeftColorValue)>(b"az_style_border_left_color_value_delete").ok()? };
        let az_style_border_left_color_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderLeftColorValue) -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_deep_copy").ok()? };
        let az_style_border_left_style_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderLeftStyleValue)>(b"az_style_border_left_style_value_delete").ok()? };
        let az_style_border_left_style_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderLeftStyleValue) -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_deep_copy").ok()? };
        let az_style_border_left_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderLeftWidthValue)>(b"az_style_border_left_width_value_delete").ok()? };
        let az_style_border_left_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderLeftWidthValue) -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_deep_copy").ok()? };
        let az_style_border_right_color_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderRightColorValue)>(b"az_style_border_right_color_value_delete").ok()? };
        let az_style_border_right_color_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderRightColorValue) -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_deep_copy").ok()? };
        let az_style_border_right_style_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderRightStyleValue)>(b"az_style_border_right_style_value_delete").ok()? };
        let az_style_border_right_style_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderRightStyleValue) -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_deep_copy").ok()? };
        let az_style_border_right_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderRightWidthValue)>(b"az_style_border_right_width_value_delete").ok()? };
        let az_style_border_right_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderRightWidthValue) -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_deep_copy").ok()? };
        let az_style_border_top_color_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopColorValue)>(b"az_style_border_top_color_value_delete").ok()? };
        let az_style_border_top_color_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopColorValue) -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_deep_copy").ok()? };
        let az_style_border_top_left_radius_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopLeftRadiusValue)>(b"az_style_border_top_left_radius_value_delete").ok()? };
        let az_style_border_top_left_radius_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopLeftRadiusValue) -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_deep_copy").ok()? };
        let az_style_border_top_right_radius_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopRightRadiusValue)>(b"az_style_border_top_right_radius_value_delete").ok()? };
        let az_style_border_top_right_radius_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopRightRadiusValue) -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_deep_copy").ok()? };
        let az_style_border_top_style_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopStyleValue)>(b"az_style_border_top_style_value_delete").ok()? };
        let az_style_border_top_style_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopStyleValue) -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_deep_copy").ok()? };
        let az_style_border_top_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleBorderTopWidthValue)>(b"az_style_border_top_width_value_delete").ok()? };
        let az_style_border_top_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleBorderTopWidthValue) -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_deep_copy").ok()? };
        let az_style_cursor_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleCursorValue)>(b"az_style_cursor_value_delete").ok()? };
        let az_style_cursor_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleCursorValue) -> AzStyleCursorValue>(b"az_style_cursor_value_deep_copy").ok()? };
        let az_style_font_family_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleFontFamilyValue)>(b"az_style_font_family_value_delete").ok()? };
        let az_style_font_family_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue>(b"az_style_font_family_value_deep_copy").ok()? };
        let az_style_font_size_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleFontSizeValue)>(b"az_style_font_size_value_delete").ok()? };
        let az_style_font_size_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleFontSizeValue) -> AzStyleFontSizeValue>(b"az_style_font_size_value_deep_copy").ok()? };
        let az_style_letter_spacing_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleLetterSpacingValue)>(b"az_style_letter_spacing_value_delete").ok()? };
        let az_style_letter_spacing_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleLetterSpacingValue) -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_deep_copy").ok()? };
        let az_style_line_height_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleLineHeightValue)>(b"az_style_line_height_value_delete").ok()? };
        let az_style_line_height_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleLineHeightValue) -> AzStyleLineHeightValue>(b"az_style_line_height_value_deep_copy").ok()? };
        let az_style_tab_width_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleTabWidthValue)>(b"az_style_tab_width_value_delete").ok()? };
        let az_style_tab_width_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleTabWidthValue) -> AzStyleTabWidthValue>(b"az_style_tab_width_value_deep_copy").ok()? };
        let az_style_text_alignment_horz_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleTextAlignmentHorzValue)>(b"az_style_text_alignment_horz_value_delete").ok()? };
        let az_style_text_alignment_horz_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleTextAlignmentHorzValue) -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_deep_copy").ok()? };
        let az_style_text_color_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleTextColorValue)>(b"az_style_text_color_value_delete").ok()? };
        let az_style_text_color_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleTextColorValue) -> AzStyleTextColorValue>(b"az_style_text_color_value_deep_copy").ok()? };
        let az_style_word_spacing_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzStyleWordSpacingValue)>(b"az_style_word_spacing_value_delete").ok()? };
        let az_style_word_spacing_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStyleWordSpacingValue) -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_deep_copy").ok()? };
        let az_css_property_delete = unsafe { lib.get::<extern fn(_:  &mut AzCssProperty)>(b"az_css_property_delete").ok()? };
        let az_css_property_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCssProperty) -> AzCssProperty>(b"az_css_property_deep_copy").ok()? };
        let az_dom_div = unsafe { lib.get::<extern fn() -> AzDom>(b"az_dom_div").ok()? };
        let az_dom_body = unsafe { lib.get::<extern fn() -> AzDom>(b"az_dom_body").ok()? };
        let az_dom_label = unsafe { lib.get::<extern fn(_:  AzString) -> AzDom>(b"az_dom_label").ok()? };
        let az_dom_text = unsafe { lib.get::<extern fn(_:  AzTextId) -> AzDom>(b"az_dom_text").ok()? };
        let az_dom_image = unsafe { lib.get::<extern fn(_:  AzImageId) -> AzDom>(b"az_dom_image").ok()? };
        let az_dom_gl_texture = unsafe { lib.get::<extern fn(_:  AzRefAny, _:  AzGlCallbackType) -> AzDom>(b"az_dom_gl_texture").ok()? };
        let az_dom_iframe = unsafe { lib.get::<extern fn(_:  AzRefAny, _:  AzIFrameCallbackType) -> AzDom>(b"az_dom_iframe").ok()? };
        let az_dom_add_id = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzString)>(b"az_dom_add_id").ok()? };
        let az_dom_with_id = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzString) -> AzDom>(b"az_dom_with_id").ok()? };
        let az_dom_set_ids = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzStringVec)>(b"az_dom_set_ids").ok()? };
        let az_dom_with_ids = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzStringVec) -> AzDom>(b"az_dom_with_ids").ok()? };
        let az_dom_add_class = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzString)>(b"az_dom_add_class").ok()? };
        let az_dom_with_class = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzString) -> AzDom>(b"az_dom_with_class").ok()? };
        let az_dom_set_classes = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzStringVec)>(b"az_dom_set_classes").ok()? };
        let az_dom_with_classes = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzStringVec) -> AzDom>(b"az_dom_with_classes").ok()? };
        let az_dom_add_callback = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType)>(b"az_dom_add_callback").ok()? };
        let az_dom_with_callback = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzEventFilter, _:  AzRefAny, _:  AzCallbackType) -> AzDom>(b"az_dom_with_callback").ok()? };
        let az_dom_add_css_override = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzString, _:  AzCssProperty)>(b"az_dom_add_css_override").ok()? };
        let az_dom_with_css_override = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzString, _:  AzCssProperty) -> AzDom>(b"az_dom_with_css_override").ok()? };
        let az_dom_set_is_draggable = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  bool)>(b"az_dom_set_is_draggable").ok()? };
        let az_dom_is_draggable = unsafe { lib.get::<extern fn(_:  AzDom, _:  bool) -> AzDom>(b"az_dom_is_draggable").ok()? };
        let az_dom_set_tab_index = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzTabIndex)>(b"az_dom_set_tab_index").ok()? };
        let az_dom_with_tab_index = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzTabIndex) -> AzDom>(b"az_dom_with_tab_index").ok()? };
        let az_dom_add_child = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzDom)>(b"az_dom_add_child").ok()? };
        let az_dom_with_child = unsafe { lib.get::<extern fn(_:  AzDom, _:  AzDom) -> AzDom>(b"az_dom_with_child").ok()? };
        let az_dom_has_id = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzString) -> bool>(b"az_dom_has_id").ok()? };
        let az_dom_has_class = unsafe { lib.get::<extern fn(_:  &mut AzDom, _:  AzString) -> bool>(b"az_dom_has_class").ok()? };
        let az_dom_get_html_string = unsafe { lib.get::<extern fn(_:  &mut AzDom) -> AzString>(b"az_dom_get_html_string").ok()? };
        let az_dom_delete = unsafe { lib.get::<extern fn(_:  &mut AzDom)>(b"az_dom_delete").ok()? };
        let az_dom_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDom) -> AzDom>(b"az_dom_deep_copy").ok()? };
        let az_gl_texture_node_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlTextureNode)>(b"az_gl_texture_node_delete").ok()? };
        let az_gl_texture_node_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGlTextureNode) -> AzGlTextureNode>(b"az_gl_texture_node_deep_copy").ok()? };
        let az_i_frame_node_delete = unsafe { lib.get::<extern fn(_:  &mut AzIFrameNode)>(b"az_i_frame_node_delete").ok()? };
        let az_i_frame_node_deep_copy = unsafe { lib.get::<extern fn(_:  &AzIFrameNode) -> AzIFrameNode>(b"az_i_frame_node_deep_copy").ok()? };
        let az_callback_data_delete = unsafe { lib.get::<extern fn(_:  &mut AzCallbackData)>(b"az_callback_data_delete").ok()? };
        let az_callback_data_deep_copy = unsafe { lib.get::<extern fn(_:  &AzCallbackData) -> AzCallbackData>(b"az_callback_data_deep_copy").ok()? };
        let az_override_property_delete = unsafe { lib.get::<extern fn(_:  &mut AzOverrideProperty)>(b"az_override_property_delete").ok()? };
        let az_override_property_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOverrideProperty) -> AzOverrideProperty>(b"az_override_property_deep_copy").ok()? };
        let az_node_data_new = unsafe { lib.get::<extern fn(_:  AzNodeType) -> AzNodeData>(b"az_node_data_new").ok()? };
        let az_node_data_default = unsafe { lib.get::<extern fn() -> AzNodeData>(b"az_node_data_default").ok()? };
        let az_node_data_delete = unsafe { lib.get::<extern fn(_:  &mut AzNodeData)>(b"az_node_data_delete").ok()? };
        let az_node_data_deep_copy = unsafe { lib.get::<extern fn(_:  &AzNodeData) -> AzNodeData>(b"az_node_data_deep_copy").ok()? };
        let az_node_type_delete = unsafe { lib.get::<extern fn(_:  &mut AzNodeType)>(b"az_node_type_delete").ok()? };
        let az_node_type_deep_copy = unsafe { lib.get::<extern fn(_:  &AzNodeType) -> AzNodeType>(b"az_node_type_deep_copy").ok()? };
        let az_on_into_event_filter = unsafe { lib.get::<extern fn(_:  AzOn) -> AzEventFilter>(b"az_on_into_event_filter").ok()? };
        let az_on_delete = unsafe { lib.get::<extern fn(_:  &mut AzOn)>(b"az_on_delete").ok()? };
        let az_on_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOn) -> AzOn>(b"az_on_deep_copy").ok()? };
        let az_event_filter_delete = unsafe { lib.get::<extern fn(_:  &mut AzEventFilter)>(b"az_event_filter_delete").ok()? };
        let az_event_filter_deep_copy = unsafe { lib.get::<extern fn(_:  &AzEventFilter) -> AzEventFilter>(b"az_event_filter_deep_copy").ok()? };
        let az_hover_event_filter_delete = unsafe { lib.get::<extern fn(_:  &mut AzHoverEventFilter)>(b"az_hover_event_filter_delete").ok()? };
        let az_hover_event_filter_deep_copy = unsafe { lib.get::<extern fn(_:  &AzHoverEventFilter) -> AzHoverEventFilter>(b"az_hover_event_filter_deep_copy").ok()? };
        let az_focus_event_filter_delete = unsafe { lib.get::<extern fn(_:  &mut AzFocusEventFilter)>(b"az_focus_event_filter_delete").ok()? };
        let az_focus_event_filter_deep_copy = unsafe { lib.get::<extern fn(_:  &AzFocusEventFilter) -> AzFocusEventFilter>(b"az_focus_event_filter_deep_copy").ok()? };
        let az_not_event_filter_delete = unsafe { lib.get::<extern fn(_:  &mut AzNotEventFilter)>(b"az_not_event_filter_delete").ok()? };
        let az_not_event_filter_deep_copy = unsafe { lib.get::<extern fn(_:  &AzNotEventFilter) -> AzNotEventFilter>(b"az_not_event_filter_deep_copy").ok()? };
        let az_window_event_filter_delete = unsafe { lib.get::<extern fn(_:  &mut AzWindowEventFilter)>(b"az_window_event_filter_delete").ok()? };
        let az_window_event_filter_deep_copy = unsafe { lib.get::<extern fn(_:  &AzWindowEventFilter) -> AzWindowEventFilter>(b"az_window_event_filter_deep_copy").ok()? };
        let az_tab_index_delete = unsafe { lib.get::<extern fn(_:  &mut AzTabIndex)>(b"az_tab_index_delete").ok()? };
        let az_tab_index_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTabIndex) -> AzTabIndex>(b"az_tab_index_deep_copy").ok()? };
        let az_gl_type_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlType)>(b"az_gl_type_delete").ok()? };
        let az_gl_type_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGlType) -> AzGlType>(b"az_gl_type_deep_copy").ok()? };
        let az_debug_message_delete = unsafe { lib.get::<extern fn(_:  &mut AzDebugMessage)>(b"az_debug_message_delete").ok()? };
        let az_debug_message_deep_copy = unsafe { lib.get::<extern fn(_:  &AzDebugMessage) -> AzDebugMessage>(b"az_debug_message_deep_copy").ok()? };
        let az_u8_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzU8VecRef)>(b"az_u8_vec_ref_delete").ok()? };
        let az_u8_vec_ref_mut_delete = unsafe { lib.get::<extern fn(_:  &mut AzU8VecRefMut)>(b"az_u8_vec_ref_mut_delete").ok()? };
        let az_f32_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzF32VecRef)>(b"az_f32_vec_ref_delete").ok()? };
        let az_i32_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzI32VecRef)>(b"az_i32_vec_ref_delete").ok()? };
        let az_g_luint_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLuintVecRef)>(b"az_g_luint_vec_ref_delete").ok()? };
        let az_g_lenum_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLenumVecRef)>(b"az_g_lenum_vec_ref_delete").ok()? };
        let az_g_lint_vec_ref_mut_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLintVecRefMut)>(b"az_g_lint_vec_ref_mut_delete").ok()? };
        let az_g_lint64_vec_ref_mut_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLint64VecRefMut)>(b"az_g_lint64_vec_ref_mut_delete").ok()? };
        let az_g_lboolean_vec_ref_mut_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLbooleanVecRefMut)>(b"az_g_lboolean_vec_ref_mut_delete").ok()? };
        let az_g_lfloat_vec_ref_mut_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLfloatVecRefMut)>(b"az_g_lfloat_vec_ref_mut_delete").ok()? };
        let az_refstr_vec_ref_delete = unsafe { lib.get::<extern fn(_:  &mut AzRefstrVecRef)>(b"az_refstr_vec_ref_delete").ok()? };
        let az_refstr_delete = unsafe { lib.get::<extern fn(_:  &mut AzRefstr)>(b"az_refstr_delete").ok()? };
        let az_get_program_binary_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzGetProgramBinaryReturn)>(b"az_get_program_binary_return_delete").ok()? };
        let az_get_program_binary_return_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGetProgramBinaryReturn) -> AzGetProgramBinaryReturn>(b"az_get_program_binary_return_deep_copy").ok()? };
        let az_get_active_attrib_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzGetActiveAttribReturn)>(b"az_get_active_attrib_return_delete").ok()? };
        let az_get_active_attrib_return_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGetActiveAttribReturn) -> AzGetActiveAttribReturn>(b"az_get_active_attrib_return_deep_copy").ok()? };
        let az_g_lsync_ptr_delete = unsafe { lib.get::<extern fn(_:  &mut AzGLsyncPtr)>(b"az_g_lsync_ptr_delete").ok()? };
        let az_get_active_uniform_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzGetActiveUniformReturn)>(b"az_get_active_uniform_return_delete").ok()? };
        let az_get_active_uniform_return_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGetActiveUniformReturn) -> AzGetActiveUniformReturn>(b"az_get_active_uniform_return_deep_copy").ok()? };
        let az_gl_context_ptr_get_type = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr) -> AzGlType>(b"az_gl_context_ptr_get_type").ok()? };
        let az_gl_context_ptr_buffer_data_untyped = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  *const c_void, _:  u32)>(b"az_gl_context_ptr_buffer_data_untyped").ok()? };
        let az_gl_context_ptr_buffer_sub_data_untyped = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  *const c_void)>(b"az_gl_context_ptr_buffer_sub_data_untyped").ok()? };
        let az_gl_context_ptr_map_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> *mut c_void>(b"az_gl_context_ptr_map_buffer").ok()? };
        let az_gl_context_ptr_map_buffer_range = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  isize, _:  isize, _:  u32) -> *mut c_void>(b"az_gl_context_ptr_map_buffer_range").ok()? };
        let az_gl_context_ptr_unmap_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>(b"az_gl_context_ptr_unmap_buffer").ok()? };
        let az_gl_context_ptr_tex_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_tex_buffer").ok()? };
        let az_gl_context_ptr_shader_source = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzStringVec)>(b"az_gl_context_ptr_shader_source").ok()? };
        let az_gl_context_ptr_read_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_read_buffer").ok()? };
        let az_gl_context_ptr_read_pixels_into_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut)>(b"az_gl_context_ptr_read_pixels_into_buffer").ok()? };
        let az_gl_context_ptr_read_pixels = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32) -> AzU8Vec>(b"az_gl_context_ptr_read_pixels").ok()? };
        let az_gl_context_ptr_read_pixels_into_pbo = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32)>(b"az_gl_context_ptr_read_pixels_into_pbo").ok()? };
        let az_gl_context_ptr_sample_coverage = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f32, _:  bool)>(b"az_gl_context_ptr_sample_coverage").ok()? };
        let az_gl_context_ptr_polygon_offset = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f32, _:  f32)>(b"az_gl_context_ptr_polygon_offset").ok()? };
        let az_gl_context_ptr_pixel_store_i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32)>(b"az_gl_context_ptr_pixel_store_i").ok()? };
        let az_gl_context_ptr_gen_buffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_buffers").ok()? };
        let az_gl_context_ptr_gen_renderbuffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_renderbuffers").ok()? };
        let az_gl_context_ptr_gen_framebuffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_framebuffers").ok()? };
        let az_gl_context_ptr_gen_textures = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_textures").ok()? };
        let az_gl_context_ptr_gen_vertex_arrays = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_vertex_arrays").ok()? };
        let az_gl_context_ptr_gen_queries = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_queries").ok()? };
        let az_gl_context_ptr_begin_query = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_begin_query").ok()? };
        let az_gl_context_ptr_end_query = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_end_query").ok()? };
        let az_gl_context_ptr_query_counter = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_query_counter").ok()? };
        let az_gl_context_ptr_get_query_object_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32>(b"az_gl_context_ptr_get_query_object_iv").ok()? };
        let az_gl_context_ptr_get_query_object_uiv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u32>(b"az_gl_context_ptr_get_query_object_uiv").ok()? };
        let az_gl_context_ptr_get_query_object_i64v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i64>(b"az_gl_context_ptr_get_query_object_i64v").ok()? };
        let az_gl_context_ptr_get_query_object_ui64v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u64>(b"az_gl_context_ptr_get_query_object_ui64v").ok()? };
        let az_gl_context_ptr_delete_queries = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_queries").ok()? };
        let az_gl_context_ptr_delete_vertex_arrays = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_vertex_arrays").ok()? };
        let az_gl_context_ptr_delete_buffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_buffers").ok()? };
        let az_gl_context_ptr_delete_renderbuffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_renderbuffers").ok()? };
        let az_gl_context_ptr_delete_framebuffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_framebuffers").ok()? };
        let az_gl_context_ptr_delete_textures = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_textures").ok()? };
        let az_gl_context_ptr_framebuffer_renderbuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_framebuffer_renderbuffer").ok()? };
        let az_gl_context_ptr_renderbuffer_storage = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32)>(b"az_gl_context_ptr_renderbuffer_storage").ok()? };
        let az_gl_context_ptr_depth_func = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_depth_func").ok()? };
        let az_gl_context_ptr_active_texture = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_active_texture").ok()? };
        let az_gl_context_ptr_attach_shader = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_attach_shader").ok()? };
        let az_gl_context_ptr_bind_attrib_location = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr)>(b"az_gl_context_ptr_bind_attrib_location").ok()? };
        let az_gl_context_ptr_get_uniform_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLintVecRefMut)>(b"az_gl_context_ptr_get_uniform_iv").ok()? };
        let az_gl_context_ptr_get_uniform_fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  AzGLfloatVecRefMut)>(b"az_gl_context_ptr_get_uniform_fv").ok()? };
        let az_gl_context_ptr_get_uniform_block_index = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> u32>(b"az_gl_context_ptr_get_uniform_block_index").ok()? };
        let az_gl_context_ptr_get_uniform_indices = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstrVecRef) -> AzGLuintVec>(b"az_gl_context_ptr_get_uniform_indices").ok()? };
        let az_gl_context_ptr_bind_buffer_base = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_bind_buffer_base").ok()? };
        let az_gl_context_ptr_bind_buffer_range = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  isize, _:  isize)>(b"az_gl_context_ptr_bind_buffer_range").ok()? };
        let az_gl_context_ptr_uniform_block_binding = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_uniform_block_binding").ok()? };
        let az_gl_context_ptr_bind_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_bind_buffer").ok()? };
        let az_gl_context_ptr_bind_vertex_array = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_bind_vertex_array").ok()? };
        let az_gl_context_ptr_bind_renderbuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_bind_renderbuffer").ok()? };
        let az_gl_context_ptr_bind_framebuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_bind_framebuffer").ok()? };
        let az_gl_context_ptr_bind_texture = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_bind_texture").ok()? };
        let az_gl_context_ptr_draw_buffers = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLenumVecRef)>(b"az_gl_context_ptr_draw_buffers").ok()? };
        let az_gl_context_ptr_tex_image_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef)>(b"az_gl_context_ptr_tex_image_2d").ok()? };
        let az_gl_context_ptr_compressed_tex_image_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  AzU8VecRef)>(b"az_gl_context_ptr_compressed_tex_image_2d").ok()? };
        let az_gl_context_ptr_compressed_tex_sub_image_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  AzU8VecRef)>(b"az_gl_context_ptr_compressed_tex_sub_image_2d").ok()? };
        let az_gl_context_ptr_tex_image_3d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzOptionU8VecRef)>(b"az_gl_context_ptr_tex_image_3d").ok()? };
        let az_gl_context_ptr_copy_tex_image_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_copy_tex_image_2d").ok()? };
        let az_gl_context_ptr_copy_tex_sub_image_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_copy_tex_sub_image_2d").ok()? };
        let az_gl_context_ptr_copy_tex_sub_image_3d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_copy_tex_sub_image_3d").ok()? };
        let az_gl_context_ptr_tex_sub_image_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef)>(b"az_gl_context_ptr_tex_sub_image_2d").ok()? };
        let az_gl_context_ptr_tex_sub_image_2d_pbo = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize)>(b"az_gl_context_ptr_tex_sub_image_2d_pbo").ok()? };
        let az_gl_context_ptr_tex_sub_image_3d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRef)>(b"az_gl_context_ptr_tex_sub_image_3d").ok()? };
        let az_gl_context_ptr_tex_sub_image_3d_pbo = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  usize)>(b"az_gl_context_ptr_tex_sub_image_3d_pbo").ok()? };
        let az_gl_context_ptr_tex_storage_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32)>(b"az_gl_context_ptr_tex_storage_2d").ok()? };
        let az_gl_context_ptr_tex_storage_3d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_tex_storage_3d").ok()? };
        let az_gl_context_ptr_get_tex_image_into_buffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  AzU8VecRefMut)>(b"az_gl_context_ptr_get_tex_image_into_buffer").ok()? };
        let az_gl_context_ptr_copy_image_sub_data = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_copy_image_sub_data").ok()? };
        let az_gl_context_ptr_invalidate_framebuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef)>(b"az_gl_context_ptr_invalidate_framebuffer").ok()? };
        let az_gl_context_ptr_invalidate_sub_framebuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLenumVecRef, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_invalidate_sub_framebuffer").ok()? };
        let az_gl_context_ptr_get_integer_v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLintVecRefMut)>(b"az_gl_context_ptr_get_integer_v").ok()? };
        let az_gl_context_ptr_get_integer_64v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLint64VecRefMut)>(b"az_gl_context_ptr_get_integer_64v").ok()? };
        let az_gl_context_ptr_get_integer_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut)>(b"az_gl_context_ptr_get_integer_iv").ok()? };
        let az_gl_context_ptr_get_integer_64iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLint64VecRefMut)>(b"az_gl_context_ptr_get_integer_64iv").ok()? };
        let az_gl_context_ptr_get_boolean_v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLbooleanVecRefMut)>(b"az_gl_context_ptr_get_boolean_v").ok()? };
        let az_gl_context_ptr_get_float_v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLfloatVecRefMut)>(b"az_gl_context_ptr_get_float_v").ok()? };
        let az_gl_context_ptr_get_framebuffer_attachment_parameter_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32>(b"az_gl_context_ptr_get_framebuffer_attachment_parameter_iv").ok()? };
        let az_gl_context_ptr_get_renderbuffer_parameter_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32>(b"az_gl_context_ptr_get_renderbuffer_parameter_iv").ok()? };
        let az_gl_context_ptr_get_tex_parameter_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32>(b"az_gl_context_ptr_get_tex_parameter_iv").ok()? };
        let az_gl_context_ptr_get_tex_parameter_fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> f32>(b"az_gl_context_ptr_get_tex_parameter_fv").ok()? };
        let az_gl_context_ptr_tex_parameter_i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32)>(b"az_gl_context_ptr_tex_parameter_i").ok()? };
        let az_gl_context_ptr_tex_parameter_f = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  f32)>(b"az_gl_context_ptr_tex_parameter_f").ok()? };
        let az_gl_context_ptr_framebuffer_texture_2d = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  i32)>(b"az_gl_context_ptr_framebuffer_texture_2d").ok()? };
        let az_gl_context_ptr_framebuffer_texture_layer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  i32, _:  i32)>(b"az_gl_context_ptr_framebuffer_texture_layer").ok()? };
        let az_gl_context_ptr_blit_framebuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u32, _:  u32)>(b"az_gl_context_ptr_blit_framebuffer").ok()? };
        let az_gl_context_ptr_vertex_attrib_4f = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  f32, _:  f32, _:  f32, _:  f32)>(b"az_gl_context_ptr_vertex_attrib_4f").ok()? };
        let az_gl_context_ptr_vertex_attrib_pointer_f32 = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  bool, _:  i32, _:  u32)>(b"az_gl_context_ptr_vertex_attrib_pointer_f32").ok()? };
        let az_gl_context_ptr_vertex_attrib_pointer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  bool, _:  i32, _:  u32)>(b"az_gl_context_ptr_vertex_attrib_pointer").ok()? };
        let az_gl_context_ptr_vertex_attrib_i_pointer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  i32, _:  u32)>(b"az_gl_context_ptr_vertex_attrib_i_pointer").ok()? };
        let az_gl_context_ptr_vertex_attrib_divisor = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_vertex_attrib_divisor").ok()? };
        let az_gl_context_ptr_viewport = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_viewport").ok()? };
        let az_gl_context_ptr_scissor = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_scissor").ok()? };
        let az_gl_context_ptr_line_width = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f32)>(b"az_gl_context_ptr_line_width").ok()? };
        let az_gl_context_ptr_use_program = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_use_program").ok()? };
        let az_gl_context_ptr_validate_program = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_validate_program").ok()? };
        let az_gl_context_ptr_draw_arrays = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32)>(b"az_gl_context_ptr_draw_arrays").ok()? };
        let az_gl_context_ptr_draw_arrays_instanced = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_draw_arrays_instanced").ok()? };
        let az_gl_context_ptr_draw_elements = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32)>(b"az_gl_context_ptr_draw_elements").ok()? };
        let az_gl_context_ptr_draw_elements_instanced = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32)>(b"az_gl_context_ptr_draw_elements_instanced").ok()? };
        let az_gl_context_ptr_blend_color = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32)>(b"az_gl_context_ptr_blend_color").ok()? };
        let az_gl_context_ptr_blend_func = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_blend_func").ok()? };
        let az_gl_context_ptr_blend_func_separate = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_blend_func_separate").ok()? };
        let az_gl_context_ptr_blend_equation = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_blend_equation").ok()? };
        let az_gl_context_ptr_blend_equation_separate = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_blend_equation_separate").ok()? };
        let az_gl_context_ptr_color_mask = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  bool, _:  bool, _:  bool, _:  bool)>(b"az_gl_context_ptr_color_mask").ok()? };
        let az_gl_context_ptr_cull_face = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_cull_face").ok()? };
        let az_gl_context_ptr_front_face = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_front_face").ok()? };
        let az_gl_context_ptr_enable = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_enable").ok()? };
        let az_gl_context_ptr_disable = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_disable").ok()? };
        let az_gl_context_ptr_hint = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_hint").ok()? };
        let az_gl_context_ptr_is_enabled = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>(b"az_gl_context_ptr_is_enabled").ok()? };
        let az_gl_context_ptr_is_shader = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>(b"az_gl_context_ptr_is_shader").ok()? };
        let az_gl_context_ptr_is_texture = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>(b"az_gl_context_ptr_is_texture").ok()? };
        let az_gl_context_ptr_is_framebuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>(b"az_gl_context_ptr_is_framebuffer").ok()? };
        let az_gl_context_ptr_is_renderbuffer = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u8>(b"az_gl_context_ptr_is_renderbuffer").ok()? };
        let az_gl_context_ptr_check_frame_buffer_status = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u32>(b"az_gl_context_ptr_check_frame_buffer_status").ok()? };
        let az_gl_context_ptr_enable_vertex_attrib_array = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_enable_vertex_attrib_array").ok()? };
        let az_gl_context_ptr_disable_vertex_attrib_array = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_disable_vertex_attrib_array").ok()? };
        let az_gl_context_ptr_uniform_1f = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  f32)>(b"az_gl_context_ptr_uniform_1f").ok()? };
        let az_gl_context_ptr_uniform_1fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_1fv").ok()? };
        let az_gl_context_ptr_uniform_1i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32)>(b"az_gl_context_ptr_uniform_1i").ok()? };
        let az_gl_context_ptr_uniform_1iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef)>(b"az_gl_context_ptr_uniform_1iv").ok()? };
        let az_gl_context_ptr_uniform_1ui = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  u32)>(b"az_gl_context_ptr_uniform_1ui").ok()? };
        let az_gl_context_ptr_uniform_2f = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32)>(b"az_gl_context_ptr_uniform_2f").ok()? };
        let az_gl_context_ptr_uniform_2fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_2fv").ok()? };
        let az_gl_context_ptr_uniform_2i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_uniform_2i").ok()? };
        let az_gl_context_ptr_uniform_2iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef)>(b"az_gl_context_ptr_uniform_2iv").ok()? };
        let az_gl_context_ptr_uniform_2ui = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32)>(b"az_gl_context_ptr_uniform_2ui").ok()? };
        let az_gl_context_ptr_uniform_3f = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32)>(b"az_gl_context_ptr_uniform_3f").ok()? };
        let az_gl_context_ptr_uniform_3fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_3fv").ok()? };
        let az_gl_context_ptr_uniform_3i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_uniform_3i").ok()? };
        let az_gl_context_ptr_uniform_3iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef)>(b"az_gl_context_ptr_uniform_3iv").ok()? };
        let az_gl_context_ptr_uniform_3ui = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_uniform_3ui").ok()? };
        let az_gl_context_ptr_uniform_4f = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  f32, _:  f32, _:  f32, _:  f32)>(b"az_gl_context_ptr_uniform_4f").ok()? };
        let az_gl_context_ptr_uniform_4i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32)>(b"az_gl_context_ptr_uniform_4i").ok()? };
        let az_gl_context_ptr_uniform_4iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzI32VecRef)>(b"az_gl_context_ptr_uniform_4iv").ok()? };
        let az_gl_context_ptr_uniform_4ui = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  u32, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_uniform_4ui").ok()? };
        let az_gl_context_ptr_uniform_4fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_4fv").ok()? };
        let az_gl_context_ptr_uniform_matrix_2fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_matrix_2fv").ok()? };
        let az_gl_context_ptr_uniform_matrix_3fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_matrix_3fv").ok()? };
        let az_gl_context_ptr_uniform_matrix_4fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32, _:  bool, _:  AzF32VecRef)>(b"az_gl_context_ptr_uniform_matrix_4fv").ok()? };
        let az_gl_context_ptr_depth_mask = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  bool)>(b"az_gl_context_ptr_depth_mask").ok()? };
        let az_gl_context_ptr_depth_range = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f64, _:  f64)>(b"az_gl_context_ptr_depth_range").ok()? };
        let az_gl_context_ptr_get_active_attrib = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveAttribReturn>(b"az_gl_context_ptr_get_active_attrib").ok()? };
        let az_gl_context_ptr_get_active_uniform = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGetActiveUniformReturn>(b"az_gl_context_ptr_get_active_uniform").ok()? };
        let az_gl_context_ptr_get_active_uniforms_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzGLuintVec, _:  u32) -> AzGLintVec>(b"az_gl_context_ptr_get_active_uniforms_iv").ok()? };
        let az_gl_context_ptr_get_active_uniform_block_i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> i32>(b"az_gl_context_ptr_get_active_uniform_block_i").ok()? };
        let az_gl_context_ptr_get_active_uniform_block_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32) -> AzGLintVec>(b"az_gl_context_ptr_get_active_uniform_block_iv").ok()? };
        let az_gl_context_ptr_get_active_uniform_block_name = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString>(b"az_gl_context_ptr_get_active_uniform_block_name").ok()? };
        let az_gl_context_ptr_get_attrib_location = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32>(b"az_gl_context_ptr_get_attrib_location").ok()? };
        let az_gl_context_ptr_get_frag_data_location = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32>(b"az_gl_context_ptr_get_frag_data_location").ok()? };
        let az_gl_context_ptr_get_uniform_location = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32>(b"az_gl_context_ptr_get_uniform_location").ok()? };
        let az_gl_context_ptr_get_program_info_log = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> AzString>(b"az_gl_context_ptr_get_program_info_log").ok()? };
        let az_gl_context_ptr_get_program_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut)>(b"az_gl_context_ptr_get_program_iv").ok()? };
        let az_gl_context_ptr_get_program_binary = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> AzGetProgramBinaryReturn>(b"az_gl_context_ptr_get_program_binary").ok()? };
        let az_gl_context_ptr_program_binary = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzU8VecRef)>(b"az_gl_context_ptr_program_binary").ok()? };
        let az_gl_context_ptr_program_parameter_i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32)>(b"az_gl_context_ptr_program_parameter_i").ok()? };
        let az_gl_context_ptr_get_vertex_attrib_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut)>(b"az_gl_context_ptr_get_vertex_attrib_iv").ok()? };
        let az_gl_context_ptr_get_vertex_attrib_fv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLfloatVecRefMut)>(b"az_gl_context_ptr_get_vertex_attrib_fv").ok()? };
        let az_gl_context_ptr_get_vertex_attrib_pointer_v = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> isize>(b"az_gl_context_ptr_get_vertex_attrib_pointer_v").ok()? };
        let az_gl_context_ptr_get_buffer_parameter_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> i32>(b"az_gl_context_ptr_get_buffer_parameter_iv").ok()? };
        let az_gl_context_ptr_get_shader_info_log = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> AzString>(b"az_gl_context_ptr_get_shader_info_log").ok()? };
        let az_gl_context_ptr_get_string = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> AzString>(b"az_gl_context_ptr_get_string").ok()? };
        let az_gl_context_ptr_get_string_i = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzString>(b"az_gl_context_ptr_get_string_i").ok()? };
        let az_gl_context_ptr_get_shader_iv = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzGLintVecRefMut)>(b"az_gl_context_ptr_get_shader_iv").ok()? };
        let az_gl_context_ptr_get_shader_precision_format = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> [i32;3]>(b"az_gl_context_ptr_get_shader_precision_format").ok()? };
        let az_gl_context_ptr_compile_shader = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_compile_shader").ok()? };
        let az_gl_context_ptr_create_program = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr) -> u32>(b"az_gl_context_ptr_create_program").ok()? };
        let az_gl_context_ptr_delete_program = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_delete_program").ok()? };
        let az_gl_context_ptr_create_shader = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32) -> u32>(b"az_gl_context_ptr_create_shader").ok()? };
        let az_gl_context_ptr_delete_shader = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_delete_shader").ok()? };
        let az_gl_context_ptr_detach_shader = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_detach_shader").ok()? };
        let az_gl_context_ptr_link_program = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_link_program").ok()? };
        let az_gl_context_ptr_clear_color = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f32, _:  f32, _:  f32, _:  f32)>(b"az_gl_context_ptr_clear_color").ok()? };
        let az_gl_context_ptr_clear = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_clear").ok()? };
        let az_gl_context_ptr_clear_depth = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  f64)>(b"az_gl_context_ptr_clear_depth").ok()? };
        let az_gl_context_ptr_clear_stencil = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32)>(b"az_gl_context_ptr_clear_stencil").ok()? };
        let az_gl_context_ptr_flush = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr)>(b"az_gl_context_ptr_flush").ok()? };
        let az_gl_context_ptr_finish = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr)>(b"az_gl_context_ptr_finish").ok()? };
        let az_gl_context_ptr_get_error = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr) -> u32>(b"az_gl_context_ptr_get_error").ok()? };
        let az_gl_context_ptr_stencil_mask = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_stencil_mask").ok()? };
        let az_gl_context_ptr_stencil_mask_separate = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_stencil_mask_separate").ok()? };
        let az_gl_context_ptr_stencil_func = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32)>(b"az_gl_context_ptr_stencil_func").ok()? };
        let az_gl_context_ptr_stencil_func_separate = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  i32, _:  u32)>(b"az_gl_context_ptr_stencil_func_separate").ok()? };
        let az_gl_context_ptr_stencil_op = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_stencil_op").ok()? };
        let az_gl_context_ptr_stencil_op_separate = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32)>(b"az_gl_context_ptr_stencil_op_separate").ok()? };
        let az_gl_context_ptr_egl_image_target_texture2d_oes = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void)>(b"az_gl_context_ptr_egl_image_target_texture2d_oes").ok()? };
        let az_gl_context_ptr_generate_mipmap = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_generate_mipmap").ok()? };
        let az_gl_context_ptr_insert_event_marker_ext = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzRefstr)>(b"az_gl_context_ptr_insert_event_marker_ext").ok()? };
        let az_gl_context_ptr_push_group_marker_ext = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzRefstr)>(b"az_gl_context_ptr_push_group_marker_ext").ok()? };
        let az_gl_context_ptr_pop_group_marker_ext = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr)>(b"az_gl_context_ptr_pop_group_marker_ext").ok()? };
        let az_gl_context_ptr_debug_message_insert_khr = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  u32, _:  AzRefstr)>(b"az_gl_context_ptr_debug_message_insert_khr").ok()? };
        let az_gl_context_ptr_push_debug_group_khr = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  AzRefstr)>(b"az_gl_context_ptr_push_debug_group_khr").ok()? };
        let az_gl_context_ptr_pop_debug_group_khr = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr)>(b"az_gl_context_ptr_pop_debug_group_khr").ok()? };
        let az_gl_context_ptr_fence_sync = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> AzGLsyncPtr>(b"az_gl_context_ptr_fence_sync").ok()? };
        let az_gl_context_ptr_client_wait_sync = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64)>(b"az_gl_context_ptr_client_wait_sync").ok()? };
        let az_gl_context_ptr_wait_sync = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr, _:  u32, _:  u64)>(b"az_gl_context_ptr_wait_sync").ok()? };
        let az_gl_context_ptr_delete_sync = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLsyncPtr)>(b"az_gl_context_ptr_delete_sync").ok()? };
        let az_gl_context_ptr_texture_range_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzU8VecRef)>(b"az_gl_context_ptr_texture_range_apple").ok()? };
        let az_gl_context_ptr_gen_fences_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_fences_apple").ok()? };
        let az_gl_context_ptr_delete_fences_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_fences_apple").ok()? };
        let az_gl_context_ptr_set_fence_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_set_fence_apple").ok()? };
        let az_gl_context_ptr_finish_fence_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_finish_fence_apple").ok()? };
        let az_gl_context_ptr_test_fence_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_test_fence_apple").ok()? };
        let az_gl_context_ptr_test_object_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32) -> u8>(b"az_gl_context_ptr_test_object_apple").ok()? };
        let az_gl_context_ptr_finish_object_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32)>(b"az_gl_context_ptr_finish_object_apple").ok()? };
        let az_gl_context_ptr_get_frag_data_index = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  AzRefstr) -> i32>(b"az_gl_context_ptr_get_frag_data_index").ok()? };
        let az_gl_context_ptr_blend_barrier_khr = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr)>(b"az_gl_context_ptr_blend_barrier_khr").ok()? };
        let az_gl_context_ptr_bind_frag_data_location_indexed = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  u32, _:  u32, _:  AzRefstr)>(b"az_gl_context_ptr_bind_frag_data_location_indexed").ok()? };
        let az_gl_context_ptr_get_debug_messages = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr) -> AzDebugMessageVec>(b"az_gl_context_ptr_get_debug_messages").ok()? };
        let az_gl_context_ptr_provoking_vertex_angle = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_provoking_vertex_angle").ok()? };
        let az_gl_context_ptr_gen_vertex_arrays_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  i32) -> AzGLuintVec>(b"az_gl_context_ptr_gen_vertex_arrays_apple").ok()? };
        let az_gl_context_ptr_bind_vertex_array_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32)>(b"az_gl_context_ptr_bind_vertex_array_apple").ok()? };
        let az_gl_context_ptr_delete_vertex_arrays_apple = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  AzGLuintVecRef)>(b"az_gl_context_ptr_delete_vertex_arrays_apple").ok()? };
        let az_gl_context_ptr_copy_texture_chromium = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8)>(b"az_gl_context_ptr_copy_texture_chromium").ok()? };
        let az_gl_context_ptr_copy_sub_texture_chromium = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8)>(b"az_gl_context_ptr_copy_sub_texture_chromium").ok()? };
        let az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  *const c_void)>(b"az_gl_context_ptr_egl_image_target_renderbuffer_storage_oes").ok()? };
        let az_gl_context_ptr_copy_texture_3d_angle = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  u32, _:  u8, _:  u8, _:  u8)>(b"az_gl_context_ptr_copy_texture_3d_angle").ok()? };
        let az_gl_context_ptr_copy_sub_texture_3d_angle = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr, _:  u32, _:  i32, _:  u32, _:  u32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  i32, _:  u8, _:  u8, _:  u8)>(b"az_gl_context_ptr_copy_sub_texture_3d_angle").ok()? };
        let az_gl_context_ptr_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlContextPtr)>(b"az_gl_context_ptr_delete").ok()? };
        let az_gl_context_ptr_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGlContextPtr) -> AzGlContextPtr>(b"az_gl_context_ptr_deep_copy").ok()? };
        let az_texture_delete = unsafe { lib.get::<extern fn(_:  &mut AzTexture)>(b"az_texture_delete").ok()? };
        let az_texture_flags_delete = unsafe { lib.get::<extern fn(_:  &mut AzTextureFlags)>(b"az_texture_flags_delete").ok()? };
        let az_texture_flags_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTextureFlags) -> AzTextureFlags>(b"az_texture_flags_deep_copy").ok()? };
        let az_text_id_new = unsafe { lib.get::<extern fn() -> AzTextId>(b"az_text_id_new").ok()? };
        let az_text_id_delete = unsafe { lib.get::<extern fn(_:  &mut AzTextId)>(b"az_text_id_delete").ok()? };
        let az_text_id_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTextId) -> AzTextId>(b"az_text_id_deep_copy").ok()? };
        let az_image_id_new = unsafe { lib.get::<extern fn() -> AzImageId>(b"az_image_id_new").ok()? };
        let az_image_id_delete = unsafe { lib.get::<extern fn(_:  &mut AzImageId)>(b"az_image_id_delete").ok()? };
        let az_image_id_deep_copy = unsafe { lib.get::<extern fn(_:  &AzImageId) -> AzImageId>(b"az_image_id_deep_copy").ok()? };
        let az_font_id_new = unsafe { lib.get::<extern fn() -> AzFontId>(b"az_font_id_new").ok()? };
        let az_font_id_delete = unsafe { lib.get::<extern fn(_:  &mut AzFontId)>(b"az_font_id_delete").ok()? };
        let az_font_id_deep_copy = unsafe { lib.get::<extern fn(_:  &AzFontId) -> AzFontId>(b"az_font_id_deep_copy").ok()? };
        let az_image_source_delete = unsafe { lib.get::<extern fn(_:  &mut AzImageSource)>(b"az_image_source_delete").ok()? };
        let az_image_source_deep_copy = unsafe { lib.get::<extern fn(_:  &AzImageSource) -> AzImageSource>(b"az_image_source_deep_copy").ok()? };
        let az_font_source_delete = unsafe { lib.get::<extern fn(_:  &mut AzFontSource)>(b"az_font_source_delete").ok()? };
        let az_font_source_deep_copy = unsafe { lib.get::<extern fn(_:  &AzFontSource) -> AzFontSource>(b"az_font_source_deep_copy").ok()? };
        let az_raw_image_new = unsafe { lib.get::<extern fn(_:  AzU8Vec, _:  usize, _:  usize, _:  AzRawImageFormat) -> AzRawImage>(b"az_raw_image_new").ok()? };
        let az_raw_image_delete = unsafe { lib.get::<extern fn(_:  &mut AzRawImage)>(b"az_raw_image_delete").ok()? };
        let az_raw_image_deep_copy = unsafe { lib.get::<extern fn(_:  &AzRawImage) -> AzRawImage>(b"az_raw_image_deep_copy").ok()? };
        let az_raw_image_format_delete = unsafe { lib.get::<extern fn(_:  &mut AzRawImageFormat)>(b"az_raw_image_format_delete").ok()? };
        let az_raw_image_format_deep_copy = unsafe { lib.get::<extern fn(_:  &AzRawImageFormat) -> AzRawImageFormat>(b"az_raw_image_format_deep_copy").ok()? };
        let az_drop_check_ptr_delete = unsafe { lib.get::<extern fn(_:  &mut AzDropCheckPtrPtr)>(b"az_drop_check_ptr_delete").ok()? };
        let az_drop_check_ptr_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzDropCheckPtrPtr) -> AzDropCheckPtrPtr>(b"az_drop_check_ptr_shallow_copy").ok()? };
        let az_arc_mutex_ref_any_delete = unsafe { lib.get::<extern fn(_:  &mut AzArcMutexRefAnyPtr)>(b"az_arc_mutex_ref_any_delete").ok()? };
        let az_arc_mutex_ref_any_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzArcMutexRefAnyPtr) -> AzArcMutexRefAnyPtr>(b"az_arc_mutex_ref_any_shallow_copy").ok()? };
        let az_timer_callback_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzTimerCallbackInfoPtr)>(b"az_timer_callback_info_delete").ok()? };
        let az_timer_callback_info_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzTimerCallbackInfoPtr) -> AzTimerCallbackInfoPtr>(b"az_timer_callback_info_shallow_copy").ok()? };
        let az_timer_delete = unsafe { lib.get::<extern fn(_:  &mut AzTimer)>(b"az_timer_delete").ok()? };
        let az_timer_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTimer) -> AzTimer>(b"az_timer_deep_copy").ok()? };
        let az_task_new = unsafe { lib.get::<extern fn(_:  AzArcMutexRefAnyPtr, _:  AzTaskCallbackType) -> AzTaskPtr>(b"az_task_new").ok()? };
        let az_task_then = unsafe { lib.get::<extern fn(_:  AzTaskPtr, _:  AzTimer) -> AzTaskPtr>(b"az_task_then").ok()? };
        let az_task_delete = unsafe { lib.get::<extern fn(_:  &mut AzTaskPtr)>(b"az_task_delete").ok()? };
        let az_task_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzTaskPtr) -> AzTaskPtr>(b"az_task_shallow_copy").ok()? };
        let az_thread_new = unsafe { lib.get::<extern fn(_:  AzRefAny, _:  AzThreadCallbackType) -> AzThreadPtr>(b"az_thread_new").ok()? };
        let az_thread_block = unsafe { lib.get::<extern fn(_:  AzThreadPtr) -> AzResultRefAnyBlockError>(b"az_thread_block").ok()? };
        let az_thread_delete = unsafe { lib.get::<extern fn(_:  &mut AzThreadPtr)>(b"az_thread_delete").ok()? };
        let az_thread_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzThreadPtr) -> AzThreadPtr>(b"az_thread_shallow_copy").ok()? };
        let az_drop_check_delete = unsafe { lib.get::<extern fn(_:  &mut AzDropCheckPtr)>(b"az_drop_check_delete").ok()? };
        let az_drop_check_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzDropCheckPtr) -> AzDropCheckPtr>(b"az_drop_check_shallow_copy").ok()? };
        let az_timer_id_delete = unsafe { lib.get::<extern fn(_:  &mut AzTimerId)>(b"az_timer_id_delete").ok()? };
        let az_timer_id_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTimerId) -> AzTimerId>(b"az_timer_id_deep_copy").ok()? };
        let az_terminate_timer_delete = unsafe { lib.get::<extern fn(_:  &mut AzTerminateTimer)>(b"az_terminate_timer_delete").ok()? };
        let az_terminate_timer_deep_copy = unsafe { lib.get::<extern fn(_:  &AzTerminateTimer) -> AzTerminateTimer>(b"az_terminate_timer_deep_copy").ok()? };
        let az_block_error_delete = unsafe { lib.get::<extern fn(_:  &mut AzBlockError)>(b"az_block_error_delete").ok()? };
        let az_block_error_deep_copy = unsafe { lib.get::<extern fn(_:  &AzBlockError) -> AzBlockError>(b"az_block_error_deep_copy").ok()? };
        let az_window_create_options_new = unsafe { lib.get::<extern fn(_:  AzCssPtr) -> AzWindowCreateOptionsPtr>(b"az_window_create_options_new").ok()? };
        let az_window_create_options_delete = unsafe { lib.get::<extern fn(_:  &mut AzWindowCreateOptionsPtr)>(b"az_window_create_options_delete").ok()? };
        let az_window_create_options_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzWindowCreateOptionsPtr) -> AzWindowCreateOptionsPtr>(b"az_window_create_options_shallow_copy").ok()? };
        let az_logical_size_delete = unsafe { lib.get::<extern fn(_:  &mut AzLogicalSize)>(b"az_logical_size_delete").ok()? };
        let az_logical_size_deep_copy = unsafe { lib.get::<extern fn(_:  &AzLogicalSize) -> AzLogicalSize>(b"az_logical_size_deep_copy").ok()? };
        let az_ref_any_new = unsafe { lib.get::<extern fn(_:  *const u8, _:  usize, _:  u64, _:  AzString, _:  fn(AzRefAny)) -> AzRefAny>(b"az_ref_any_new").ok()? };
        let az_ref_any_get_ptr = unsafe { lib.get::<extern fn(_:  &AzRefAny, _:  usize, _:  u64) -> *const c_void>(b"az_ref_any_get_ptr").ok()? };
        let az_ref_any_get_mut_ptr = unsafe { lib.get::<extern fn(_:  &AzRefAny, _:  usize, _:  u64) -> *mut c_void>(b"az_ref_any_get_mut_ptr").ok()? };
        let az_ref_any_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzRefAny) -> AzRefAny>(b"az_ref_any_shallow_copy").ok()? };
        let az_ref_any_delete = unsafe { lib.get::<extern fn(_:  &mut AzRefAny)>(b"az_ref_any_delete").ok()? };
        let az_ref_any_core_copy = unsafe { lib.get::<extern fn(_:  &AzRefAny) -> AzRefAny>(b"az_ref_any_core_copy").ok()? };
        Some(AzulDll {
            lib: Box::new(lib),
            az_string_from_utf8_unchecked,
            az_string_from_utf8_lossy,
            az_string_into_bytes,
            az_string_delete,
            az_string_deep_copy,
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
            az_gradient_stop_pre_vec_copy_from,
            az_gradient_stop_pre_vec_delete,
            az_gradient_stop_pre_vec_deep_copy,
            az_option_percentage_value_delete,
            az_option_percentage_value_deep_copy,
            az_option_dom_delete,
            az_option_dom_deep_copy,
            az_option_texture_delete,
            az_option_tab_index_delete,
            az_option_tab_index_deep_copy,
            az_option_duration_delete,
            az_option_duration_deep_copy,
            az_option_instant_delete,
            az_option_instant_deep_copy,
            az_option_usize_delete,
            az_option_usize_deep_copy,
            az_option_u8_vec_ref_delete,
            az_result_ref_any_block_error_delete,
            az_result_ref_any_block_error_deep_copy,
            az_instant_now,
            az_instant_delete,
            az_instant_shallow_copy,
            az_duration_delete,
            az_duration_deep_copy,
            az_app_config_default,
            az_app_config_delete,
            az_app_config_shallow_copy,
            az_app_new,
            az_app_run,
            az_app_delete,
            az_app_shallow_copy,
            az_layout_callback_delete,
            az_layout_callback_deep_copy,
            az_callback_delete,
            az_callback_deep_copy,
            az_callback_info_delete,
            az_callback_info_shallow_copy,
            az_update_screen_delete,
            az_update_screen_deep_copy,
            az_i_frame_callback_delete,
            az_i_frame_callback_deep_copy,
            az_i_frame_callback_info_delete,
            az_i_frame_callback_info_shallow_copy,
            az_i_frame_callback_return_delete,
            az_i_frame_callback_return_deep_copy,
            az_gl_callback_delete,
            az_gl_callback_deep_copy,
            az_gl_callback_info_delete,
            az_gl_callback_info_shallow_copy,
            az_gl_callback_return_delete,
            az_timer_callback_delete,
            az_timer_callback_deep_copy,
            az_timer_callback_type_delete,
            az_timer_callback_type_shallow_copy,
            az_timer_callback_return_delete,
            az_timer_callback_return_deep_copy,
            az_layout_info_delete,
            az_layout_info_shallow_copy,
            az_css_native,
            az_css_empty,
            az_css_from_string,
            az_css_override_native,
            az_css_delete,
            az_css_shallow_copy,
            az_css_hot_reloader_new,
            az_css_hot_reloader_override_native,
            az_css_hot_reloader_delete,
            az_css_hot_reloader_shallow_copy,
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
            az_drop_check_ptr_shallow_copy,
            az_arc_mutex_ref_any_delete,
            az_arc_mutex_ref_any_shallow_copy,
            az_timer_callback_info_delete,
            az_timer_callback_info_shallow_copy,
            az_timer_delete,
            az_timer_deep_copy,
            az_task_new,
            az_task_then,
            az_task_delete,
            az_task_shallow_copy,
            az_thread_new,
            az_thread_block,
            az_thread_delete,
            az_thread_shallow_copy,
            az_drop_check_delete,
            az_drop_check_shallow_copy,
            az_timer_id_delete,
            az_timer_id_deep_copy,
            az_terminate_timer_delete,
            az_terminate_timer_deep_copy,
            az_block_error_delete,
            az_block_error_deep_copy,
            az_window_create_options_new,
            az_window_create_options_delete,
            az_window_create_options_shallow_copy,
            az_logical_size_delete,
            az_logical_size_deep_copy,
            az_ref_any_new,
            az_ref_any_get_ptr,
            az_ref_any_get_mut_ptr,
            az_ref_any_shallow_copy,
            az_ref_any_delete,
            az_ref_any_core_copy,
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

    fn load_library_inner() -> Option<AzulDll> {

        let current_exe_path = std::env::current_exe().ok()?;
        let mut library_path = current_exe_path.parent()?.to_path_buf();
        library_path.push(DLL_FILE_NAME);

        if !library_path.exists() {
           std::fs::write(&library_path, LIB_BYTES).ok()?;
        }

        initialize_library(&library_path)
    }

    pub(crate) fn get_azul_dll() -> &'static AzulDll { 
        if !LIBRARY_IS_INITIALIZED.load(Ordering::SeqCst) {
           match load_library_inner() {
               Some(s) => {
                   unsafe { AZUL_DLL = MaybeUninit::new(s) };
                   LIBRARY_IS_INITIALIZED.store(true, Ordering::SeqCst);
               },
               None => { println!("failed to initialize libazul dll"); std::process::exit(-1); }
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

    impl From<crate::str::String> for std::string::String {
        fn from(s: crate::str::String) -> std::string::String {
            let s_bytes = s.into_bytes();
            unsafe { std::string::String::from_utf8_unchecked(s_bytes.into()) } // - copies s into a new String
            // - s_bytes is deallocated here
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
    }    use crate::dom::{CallbackData, Dom, OverrideProperty};
    use crate::gl::DebugMessage;
    use crate::str::String;
    use crate::css::GradientStopPre;


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


    /// `OptionInstant` struct
    pub use crate::dll::AzOptionInstant as OptionInstant;

    impl Clone for OptionInstant { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_option_instant_deep_copy)(self) } }
    impl Drop for OptionInstant { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_instant_delete)(self); } }


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



    /// `TimerCallbackReturn` struct
    pub use crate::dll::AzTimerCallbackReturn as TimerCallbackReturn;

    impl Clone for TimerCallbackReturn { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_timer_callback_return_deep_copy)(self) } }
    impl Drop for TimerCallbackReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_callback_return_delete)(self); } }


    pub use crate::dll::AzThreadCallbackType as ThreadCallbackType;

    pub use crate::dll::AzTaskCallbackType as TaskCallbackType;

    pub use crate::dll::AzRefAny as RefAny;

    impl Clone for RefAny {
        fn clone(&self) -> Self {
            (crate::dll::get_azul_dll().az_ref_any_shallow_copy)(&self)
        }
    }

    impl RefAny {

        /// Creates a new, type-erased pointer by casting the `T` value into a `Vec<u8>` and saving the length + type ID
        pub fn new<T: 'static>(value: T) -> Self {
            use crate::dll::*;

            fn default_custom_destructor<U: 'static>(ptr: RefAny) {
                use std::{mem, ptr};

                // note: in the default constructor, we do not need to check whether U == T

                unsafe {
                    // copy the struct from the heap to the stack and call mem::drop on U to run the destructor
                    let mut stack_mem = mem::MaybeUninit::<U>::uninit().assume_init();
                    ptr::copy_nonoverlapping(ptr._internal_ptr as *const u8, &mut stack_mem as *mut U as *mut u8, mem::size_of::<U>().min(ptr._internal_len));
                    mem::drop(stack_mem);
                }
            }

            let type_name_str = ::std::any::type_name::<T>();
            let s = (crate::dll::get_azul_dll().az_ref_any_new)(
                (&value as *const T) as *const u8,
                ::std::mem::size_of::<T>(),
                Self::get_type_id::<T>() as u64,
                crate::str::String::from_utf8_unchecked(type_name_str.as_ptr(), type_name_str.len()),
                default_custom_destructor::<T>,
            );
            ::std::mem::forget(value); // do not run the destructor of T here!
            s
        }

        /// Returns the inner `RefAny`
        pub fn leak(self) -> RefAny {
            use std::mem;
            let s = (crate::dll::get_azul_dll().az_ref_any_core_copy)(&self);
            mem::forget(self); // do not run destructor
            s
        }

        /// Downcasts the type-erased pointer to a type `&U`, returns `None` if the types don't match
        #[inline]
        pub fn downcast_ref<'a, U: 'static>(&'a self) -> Option<&'a U> {
            use std::ptr;
            let ptr = (crate::dll::get_azul_dll().az_ref_any_get_ptr)(&self, self._internal_len, Self::get_type_id::<U>());
            if ptr == ptr::null() { None } else { Some(unsafe { &*(self._internal_ptr as *const U) as &'a U }) }
        }

        /// Downcasts the type-erased pointer to a type `&mut U`, returns `None` if the types don't match
        #[inline]
        pub fn downcast_mut<'a, U: 'static>(&'a mut self) -> Option<&'a mut U> {
            use std::ptr;
            let ptr = (crate::dll::get_azul_dll().az_ref_any_get_mut_ptr)(&self, self._internal_len, Self::get_type_id::<U>());
            if ptr == ptr::null_mut() { None } else { Some(unsafe { &mut *(self._internal_ptr as *mut U) as &'a mut U }) }
        }

        #[inline]
        fn get_type_id<T: 'static>() -> u64 {
            use std::any::TypeId;
            use std::mem;

            // fast method to serialize the type id into a u64
            let t_id = TypeId::of::<T>();
            let struct_as_bytes = unsafe { ::std::slice::from_raw_parts((&t_id as *const TypeId) as *const u8, mem::size_of::<TypeId>()) };
            struct_as_bytes.into_iter().enumerate().map(|(s_pos, s)| ((*s as u64) << s_pos)).sum()
        }
    }

    impl Drop for RefAny {
        fn drop(&mut self) {
            (crate::dll::get_azul_dll().az_ref_any_delete)(self);
        }
    }


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


    /// `Css` struct
    pub use crate::dll::AzCssPtr as Css;

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

    impl Drop for Css { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_delete)(self); } }


    /// `CssHotReloader` struct
    pub use crate::dll::AzCssHotReloaderPtr as CssHotReloader;

    impl CssHotReloader {
        /// Creates a `HotReloadHandler` that hot-reloads a CSS file every X milliseconds
        pub fn new(path: String, reload_ms: u64) -> Self { (crate::dll::get_azul_dll().az_css_hot_reloader_new)(path, reload_ms) }
        /// Creates a `HotReloadHandler` that overrides the `Css::native()` stylesheet with a CSS file, reloaded every X milliseconds
        pub fn override_native(path: String, reload_ms: u64) -> Self { (crate::dll::get_azul_dll().az_css_hot_reloader_override_native)(path, reload_ms) }
    }

    impl Drop for CssHotReloader { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_hot_reloader_delete)(self); } }


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
        pub fn get_html_string(&mut self)  -> crate::str::String { { (crate::dll::get_azul_dll().az_dom_get_html_string)(self)} }
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


    /// C-ABI stable reexport of `(i32, u32, AzString)`
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


    /// `TimerCallbackInfo` struct
    pub use crate::dll::AzTimerCallbackInfoPtr as TimerCallbackInfo;

    impl Drop for TimerCallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_timer_callback_info_delete)(self); } }


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


    /// `WindowCreateOptions` struct
    pub use crate::dll::AzWindowCreateOptionsPtr as WindowCreateOptions;

    impl WindowCreateOptions {
        /// Creates a new `WindowCreateOptions` instance.
        pub fn new(css: Css) -> Self { (crate::dll::get_azul_dll().az_window_create_options_new)(css) }
    }

    impl Drop for WindowCreateOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_create_options_delete)(self); } }


    /// `LogicalSize` struct
    pub use crate::dll::AzLogicalSize as LogicalSize;

    impl Clone for LogicalSize { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_logical_size_deep_copy)(self) } }
    impl Drop for LogicalSize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_logical_size_delete)(self); } }
}

