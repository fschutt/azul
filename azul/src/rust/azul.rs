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
    pub type AzCallback = fn(AzCallbackInfoPtr) -> AzCallbackReturn;
    /// Callback fn that returns the DOM of the app
    pub type AzLayoutCallback = fn(AzRefAny, AzLayoutInfoPtr) -> AzDomPtr;
    /// Callback for rendering to an OpenGL texture
    pub type AzGlCallback = fn(AzGlCallbackInfoPtr) -> AzGlCallbackReturnPtr;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzIFrameCallback = fn(AzIFrameCallbackInfoPtr) -> AzIFrameCallbackReturnPtr;
    #[repr(C)] pub struct AzString {
        pub vec: AzU8Vec,
    }
    #[repr(C)] pub struct AzU8Vec {
        pub ptr: *const u8,
        pub len: usize,
        pub cap: usize,
    }
    #[repr(C)] pub struct AzStringVec {
        pub ptr: *const AzString,
        pub len: usize,
        pub cap: usize,
    }
    #[repr(C)] pub struct AzGradientStopPreVec {
        pub ptr: *const AzGradientStopPre,
        pub len: usize,
        pub cap: usize,
    }
    #[repr(C, u8)] pub enum AzOptionPercentageValue {
        None,
        Some(AzPercentageValue),
    }
    #[repr(C)] pub struct AzAppConfigPtr {
        pub ptr: *mut c_void,
    }
    #[repr(C)] pub struct AzAppPtr {
        pub ptr: *mut c_void,
    }
    #[repr(C)] pub struct AzCallbackInfoPtr {
        pub ptr: *mut c_void,
    }
    #[repr(C)] pub enum AzUpdateScreen {
        Redraw,
        DontRedraw,
    }
    #[repr(C)] pub struct AzIFrameCallbackInfoPtr {
        pub ptr: *mut c_void,
    }
    #[repr(C)] pub struct AzIFrameCallbackReturnPtr {
        pub ptr: *mut c_void,
    }
    #[repr(C)] pub struct AzGlCallbackInfoPtr {
        pub ptr: *mut c_void,
    }
    #[repr(C)] pub struct AzGlCallbackReturnPtr {
        pub ptr: *mut c_void,
    }
    #[repr(C)] pub struct AzLayoutInfoPtr {
        pub ptr: *mut c_void,
    }
    #[repr(C)] pub struct AzCssPtr {
        pub ptr: *mut c_void,
    }
    #[repr(C)] pub struct AzCssHotReloaderPtr {
        pub ptr: *mut c_void,
    }
    #[repr(C)] pub struct AzColorU {
        pub r: u8,
        pub g: u8,
        pub b: u8,
        pub a: u8,
    }
    #[repr(C)] pub enum AzSizeMetric {
        Px,
        Pt,
        Em,
        Percent,
    }
    #[repr(C)] pub struct AzFloatValue {
        pub number: isize,
    }
    #[repr(C)] pub struct AzPixelValue {
        pub metric: AzSizeMetric,
        pub number: AzFloatValue,
    }
    #[repr(C)] pub struct AzPixelValueNoPercent {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub enum AzBoxShadowClipMode {
        Outset,
        Inset,
    }
    #[repr(C)] pub struct AzBoxShadowPreDisplayItem {
        pub offset: [AzPixelValueNoPercent;2],
        pub color: AzColorU,
        pub blur_radius: AzPixelValueNoPercent,
        pub spread_radius: AzPixelValueNoPercent,
        pub clip_mode: AzBoxShadowClipMode,
    }
    #[repr(C)] pub enum AzLayoutAlignContent {
        Stretch,
        Center,
        Start,
        End,
        SpaceBetween,
        SpaceAround,
    }
    #[repr(C)] pub enum AzLayoutAlignItems {
        Stretch,
        Center,
        Start,
        End,
    }
    #[repr(C)] pub struct AzLayoutBottom {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub enum AzLayoutBoxSizing {
        ContentBox,
        BorderBox,
    }
    #[repr(C)] pub enum AzLayoutDirection {
        Row,
        RowReverse,
        Column,
        ColumnReverse,
    }
    #[repr(C)] pub enum AzLayoutDisplay {
        Flex,
        Block,
        InlineBlock,
    }
    #[repr(C)] pub struct AzLayoutFlexGrow {
        pub inner: AzFloatValue,
    }
    #[repr(C)] pub struct AzLayoutFlexShrink {
        pub inner: AzFloatValue,
    }
    #[repr(C)] pub enum AzLayoutFloat {
        Left,
        Right,
    }
    #[repr(C)] pub struct AzLayoutHeight {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub enum AzLayoutJustifyContent {
        Start,
        End,
        Center,
        SpaceBetween,
        SpaceAround,
        SpaceEvenly,
    }
    #[repr(C)] pub struct AzLayoutLeft {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutMarginBottom {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutMarginLeft {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutMarginRight {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutMarginTop {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutMaxHeight {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutMaxWidth {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutMinHeight {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutMinWidth {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutPaddingBottom {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutPaddingLeft {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutPaddingRight {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutPaddingTop {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub enum AzLayoutPosition {
        Static,
        Relative,
        Absolute,
        Fixed,
    }
    #[repr(C)] pub struct AzLayoutRight {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutTop {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzLayoutWidth {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub enum AzLayoutWrap {
        Wrap,
        NoWrap,
    }
    #[repr(C)] pub enum AzOverflow {
        Scroll,
        Auto,
        Hidden,
        Visible,
    }
    #[repr(C)] pub struct AzPercentageValue {
        pub number: AzFloatValue,
    }
    #[repr(C)] pub struct AzGradientStopPre {
        pub offset: AzOptionPercentageValue,
        pub color: AzColorU,
    }
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
    #[repr(C)] pub struct AzDirectionCorners {
        pub from: AzDirectionCorner,
        pub to: AzDirectionCorner,
    }
    #[repr(C, u8)] pub enum AzDirection {
        Angle(AzFloatValue),
        FromTo(AzDirectionCorners),
    }
    #[repr(C)] pub enum AzExtendMode {
        Clamp,
        Repeat,
    }
    #[repr(C)] pub struct AzLinearGradient {
        pub direction: AzDirection,
        pub extend_mode: AzExtendMode,
        pub stops: AzGradientStopPreVec,
    }
    #[repr(C)] pub enum AzShape {
        Ellipse,
        Circle,
    }
    #[repr(C)] pub struct AzRadialGradient {
        pub shape: AzShape,
        pub extend_mode: AzExtendMode,
        pub stops: AzGradientStopPreVec,
    }
    #[repr(C)] pub struct AzCssImageId {
        pub inner: AzString,
    }
    #[repr(C, u8)] pub enum AzStyleBackgroundContent {
        LinearGradient(AzLinearGradient),
        RadialGradient(AzRadialGradient),
        Image(AzCssImageId),
        Color(AzColorU),
    }
    #[repr(C, u8)] pub enum AzBackgroundPositionHorizontal {
        Left,
        Center,
        Right,
        Exact(AzPixelValue),
    }
    #[repr(C, u8)] pub enum AzBackgroundPositionVertical {
        Top,
        Center,
        Bottom,
        Exact(AzPixelValue),
    }
    #[repr(C)] pub struct AzStyleBackgroundPosition {
        pub horizontal: AzBackgroundPositionHorizontal,
        pub vertical: AzBackgroundPositionVertical,
    }
    #[repr(C)] pub enum AzStyleBackgroundRepeat {
        NoRepeat,
        Repeat,
        RepeatX,
        RepeatY,
    }
    #[repr(C, u8)] pub enum AzStyleBackgroundSize {
        ExactSize([AzPixelValue;2]),
        Contain,
        Cover,
    }
    #[repr(C)] pub struct AzStyleBorderBottomColor {
        pub inner: AzColorU,
    }
    #[repr(C)] pub struct AzStyleBorderBottomLeftRadius {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzStyleBorderBottomRightRadius {
        pub inner: AzPixelValue,
    }
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
    #[repr(C)] pub struct AzStyleBorderBottomStyle {
        pub inner: AzBorderStyle,
    }
    #[repr(C)] pub struct AzStyleBorderBottomWidth {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzStyleBorderLeftColor {
        pub inner: AzColorU,
    }
    #[repr(C)] pub struct AzStyleBorderLeftStyle {
        pub inner: AzBorderStyle,
    }
    #[repr(C)] pub struct AzStyleBorderLeftWidth {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzStyleBorderRightColor {
        pub inner: AzColorU,
    }
    #[repr(C)] pub struct AzStyleBorderRightStyle {
        pub inner: AzBorderStyle,
    }
    #[repr(C)] pub struct AzStyleBorderRightWidth {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzStyleBorderTopColor {
        pub inner: AzColorU,
    }
    #[repr(C)] pub struct AzStyleBorderTopLeftRadius {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzStyleBorderTopRightRadius {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzStyleBorderTopStyle {
        pub inner: AzBorderStyle,
    }
    #[repr(C)] pub struct AzStyleBorderTopWidth {
        pub inner: AzPixelValue,
    }
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
    #[repr(C)] pub struct AzStyleFontFamily {
        pub fonts: AzStringVec,
    }
    #[repr(C)] pub struct AzStyleFontSize {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzStyleLetterSpacing {
        pub inner: AzPixelValue,
    }
    #[repr(C)] pub struct AzStyleLineHeight {
        pub inner: AzPercentageValue,
    }
    #[repr(C)] pub struct AzStyleTabWidth {
        pub inner: AzPercentageValue,
    }
    #[repr(C)] pub enum AzStyleTextAlignmentHorz {
        Left,
        Center,
        Right,
    }
    #[repr(C)] pub struct AzStyleTextColor {
        pub inner: AzColorU,
    }
    #[repr(C)] pub struct AzStyleWordSpacing {
        pub inner: AzPixelValue,
    }
    #[repr(C, u8)] pub enum AzBoxShadowPreDisplayItemValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzBoxShadowPreDisplayItem),
    }
    #[repr(C, u8)] pub enum AzLayoutAlignContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutAlignContent),
    }
    #[repr(C, u8)] pub enum AzLayoutAlignItemsValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutAlignItems),
    }
    #[repr(C, u8)] pub enum AzLayoutBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBottom),
    }
    #[repr(C, u8)] pub enum AzLayoutBoxSizingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutBoxSizing),
    }
    #[repr(C, u8)] pub enum AzLayoutDirectionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutDirection),
    }
    #[repr(C, u8)] pub enum AzLayoutDisplayValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutDisplay),
    }
    #[repr(C, u8)] pub enum AzLayoutFlexGrowValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexGrow),
    }
    #[repr(C, u8)] pub enum AzLayoutFlexShrinkValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFlexShrink),
    }
    #[repr(C, u8)] pub enum AzLayoutFloatValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutFloat),
    }
    #[repr(C, u8)] pub enum AzLayoutHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutHeight),
    }
    #[repr(C, u8)] pub enum AzLayoutJustifyContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutJustifyContent),
    }
    #[repr(C, u8)] pub enum AzLayoutLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutLeft),
    }
    #[repr(C, u8)] pub enum AzLayoutMarginBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginBottom),
    }
    #[repr(C, u8)] pub enum AzLayoutMarginLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginLeft),
    }
    #[repr(C, u8)] pub enum AzLayoutMarginRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginRight),
    }
    #[repr(C, u8)] pub enum AzLayoutMarginTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMarginTop),
    }
    #[repr(C, u8)] pub enum AzLayoutMaxHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMaxHeight),
    }
    #[repr(C, u8)] pub enum AzLayoutMaxWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMaxWidth),
    }
    #[repr(C, u8)] pub enum AzLayoutMinHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMinHeight),
    }
    #[repr(C, u8)] pub enum AzLayoutMinWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutMinWidth),
    }
    #[repr(C, u8)] pub enum AzLayoutPaddingBottomValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingBottom),
    }
    #[repr(C, u8)] pub enum AzLayoutPaddingLeftValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingLeft),
    }
    #[repr(C, u8)] pub enum AzLayoutPaddingRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingRight),
    }
    #[repr(C, u8)] pub enum AzLayoutPaddingTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPaddingTop),
    }
    #[repr(C, u8)] pub enum AzLayoutPositionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutPosition),
    }
    #[repr(C, u8)] pub enum AzLayoutRightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutRight),
    }
    #[repr(C, u8)] pub enum AzLayoutTopValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutTop),
    }
    #[repr(C, u8)] pub enum AzLayoutWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutWidth),
    }
    #[repr(C, u8)] pub enum AzLayoutWrapValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzLayoutWrap),
    }
    #[repr(C, u8)] pub enum AzOverflowValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzOverflow),
    }
    #[repr(C, u8)] pub enum AzStyleBackgroundContentValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundContent),
    }
    #[repr(C, u8)] pub enum AzStyleBackgroundPositionValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundPosition),
    }
    #[repr(C, u8)] pub enum AzStyleBackgroundRepeatValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundRepeat),
    }
    #[repr(C, u8)] pub enum AzStyleBackgroundSizeValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBackgroundSize),
    }
    #[repr(C, u8)] pub enum AzStyleBorderBottomColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomColor),
    }
    #[repr(C, u8)] pub enum AzStyleBorderBottomLeftRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomLeftRadius),
    }
    #[repr(C, u8)] pub enum AzStyleBorderBottomRightRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomRightRadius),
    }
    #[repr(C, u8)] pub enum AzStyleBorderBottomStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomStyle),
    }
    #[repr(C, u8)] pub enum AzStyleBorderBottomWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderBottomWidth),
    }
    #[repr(C, u8)] pub enum AzStyleBorderLeftColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftColor),
    }
    #[repr(C, u8)] pub enum AzStyleBorderLeftStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftStyle),
    }
    #[repr(C, u8)] pub enum AzStyleBorderLeftWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderLeftWidth),
    }
    #[repr(C, u8)] pub enum AzStyleBorderRightColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightColor),
    }
    #[repr(C, u8)] pub enum AzStyleBorderRightStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightStyle),
    }
    #[repr(C, u8)] pub enum AzStyleBorderRightWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderRightWidth),
    }
    #[repr(C, u8)] pub enum AzStyleBorderTopColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopColor),
    }
    #[repr(C, u8)] pub enum AzStyleBorderTopLeftRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopLeftRadius),
    }
    #[repr(C, u8)] pub enum AzStyleBorderTopRightRadiusValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopRightRadius),
    }
    #[repr(C, u8)] pub enum AzStyleBorderTopStyleValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopStyle),
    }
    #[repr(C, u8)] pub enum AzStyleBorderTopWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleBorderTopWidth),
    }
    #[repr(C, u8)] pub enum AzStyleCursorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleCursor),
    }
    #[repr(C, u8)] pub enum AzStyleFontFamilyValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleFontFamily),
    }
    #[repr(C, u8)] pub enum AzStyleFontSizeValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleFontSize),
    }
    #[repr(C, u8)] pub enum AzStyleLetterSpacingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleLetterSpacing),
    }
    #[repr(C, u8)] pub enum AzStyleLineHeightValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleLineHeight),
    }
    #[repr(C, u8)] pub enum AzStyleTabWidthValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTabWidth),
    }
    #[repr(C, u8)] pub enum AzStyleTextAlignmentHorzValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTextAlignmentHorz),
    }
    #[repr(C, u8)] pub enum AzStyleTextColorValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleTextColor),
    }
    #[repr(C, u8)] pub enum AzStyleWordSpacingValue {
        Auto,
        None,
        Inherit,
        Initial,
        Exact(AzStyleWordSpacing),
    }
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
    #[repr(C)] pub struct AzDomPtr {
        pub ptr: *mut c_void,
    }
    #[repr(C, u8)] pub enum AzEventFilter {
        Hover(AzHoverEventFilter),
        Not(AzNotEventFilter),
        Focus(AzFocusEventFilter),
        Window(AzWindowEventFilter),
    }
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
    #[repr(C, u8)] pub enum AzNotEventFilter {
        Hover(AzHoverEventFilter),
        Focus(AzFocusEventFilter),
    }
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
    #[repr(C, u8)] pub enum AzTabIndex {
        Auto,
        OverrideInParent(usize),
        NoKeyboardFocus,
    }
    #[repr(C)] pub struct AzTextId {
        pub id: usize,
    }
    #[repr(C)] pub struct AzImageId {
        pub id: usize,
    }
    #[repr(C)] pub struct AzFontId {
        pub id: usize,
    }
    #[repr(C, u8)] pub enum AzImageSource {
        Embedded(AzU8Vec),
        File(AzString),
        Raw(AzRawImage),
    }
    #[repr(C, u8)] pub enum AzFontSource {
        Embedded(AzU8Vec),
        File(AzString),
        System(AzString),
    }
    #[repr(C)] pub struct AzRawImage {
        pub pixels: AzU8Vec,
        pub width: usize,
        pub height: usize,
        pub data_format: AzRawImageFormat,
    }
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
    #[repr(C)] pub struct AzWindowCreateOptionsPtr {
        pub ptr: *mut c_void,
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
        pub az_u8_vec_as_ptr: Symbol<extern fn(_:  &AzU8Vec) -> *const u8>,
        pub az_u8_vec_len: Symbol<extern fn(_:  &AzU8Vec) -> usize>,
        pub az_u8_vec_capacity: Symbol<extern fn(_:  &AzU8Vec) -> usize>,
        pub az_u8_vec_delete: Symbol<extern fn(_:  &mut AzU8Vec)>,
        pub az_u8_vec_deep_copy: Symbol<extern fn(_:  &AzU8Vec) -> AzU8Vec>,
        pub az_string_vec_copy_from: Symbol<extern fn(_:  *const AzString, _:  usize) -> AzStringVec>,
        pub az_string_vec_as_ptr: Symbol<extern fn(_:  &AzStringVec) -> *const AzString>,
        pub az_string_vec_len: Symbol<extern fn(_:  &AzStringVec) -> usize>,
        pub az_string_vec_capacity: Symbol<extern fn(_:  &AzStringVec) -> usize>,
        pub az_string_vec_delete: Symbol<extern fn(_:  &mut AzStringVec)>,
        pub az_string_vec_deep_copy: Symbol<extern fn(_:  &AzStringVec) -> AzStringVec>,
        pub az_gradient_stop_pre_vec_copy_from: Symbol<extern fn(_:  *const AzGradientStopPre, _:  usize) -> AzGradientStopPreVec>,
        pub az_gradient_stop_pre_vec_as_ptr: Symbol<extern fn(_:  &AzGradientStopPreVec) -> *const AzGradientStopPre>,
        pub az_gradient_stop_pre_vec_len: Symbol<extern fn(_:  &AzGradientStopPreVec) -> usize>,
        pub az_gradient_stop_pre_vec_capacity: Symbol<extern fn(_:  &AzGradientStopPreVec) -> usize>,
        pub az_gradient_stop_pre_vec_delete: Symbol<extern fn(_:  &mut AzGradientStopPreVec)>,
        pub az_gradient_stop_pre_vec_deep_copy: Symbol<extern fn(_:  &AzGradientStopPreVec) -> AzGradientStopPreVec>,
        pub az_option_percentage_value_delete: Symbol<extern fn(_:  &mut AzOptionPercentageValue)>,
        pub az_option_percentage_value_deep_copy: Symbol<extern fn(_:  &AzOptionPercentageValue) -> AzOptionPercentageValue>,
        pub az_app_config_default: Symbol<extern fn() -> AzAppConfigPtr>,
        pub az_app_config_delete: Symbol<extern fn(_:  &mut AzAppConfigPtr)>,
        pub az_app_config_shallow_copy: Symbol<extern fn(_:  &AzAppConfigPtr) -> AzAppConfigPtr>,
        pub az_app_new: Symbol<extern fn(_:  AzRefAny, _:  AzAppConfigPtr, _:  AzLayoutCallback) -> AzAppPtr>,
        pub az_app_run: Symbol<extern fn(_:  AzAppPtr, _:  AzWindowCreateOptionsPtr)>,
        pub az_app_delete: Symbol<extern fn(_:  &mut AzAppPtr)>,
        pub az_app_shallow_copy: Symbol<extern fn(_:  &AzAppPtr) -> AzAppPtr>,
        pub az_callback_info_delete: Symbol<extern fn(_:  &mut AzCallbackInfoPtr)>,
        pub az_callback_info_shallow_copy: Symbol<extern fn(_:  &AzCallbackInfoPtr) -> AzCallbackInfoPtr>,
        pub az_update_screen_delete: Symbol<extern fn(_:  &mut AzUpdateScreen)>,
        pub az_update_screen_deep_copy: Symbol<extern fn(_:  &AzUpdateScreen) -> AzUpdateScreen>,
        pub az_i_frame_callback_info_delete: Symbol<extern fn(_:  &mut AzIFrameCallbackInfoPtr)>,
        pub az_i_frame_callback_info_shallow_copy: Symbol<extern fn(_:  &AzIFrameCallbackInfoPtr) -> AzIFrameCallbackInfoPtr>,
        pub az_i_frame_callback_return_delete: Symbol<extern fn(_:  &mut AzIFrameCallbackReturnPtr)>,
        pub az_i_frame_callback_return_shallow_copy: Symbol<extern fn(_:  &AzIFrameCallbackReturnPtr) -> AzIFrameCallbackReturnPtr>,
        pub az_gl_callback_info_delete: Symbol<extern fn(_:  &mut AzGlCallbackInfoPtr)>,
        pub az_gl_callback_info_shallow_copy: Symbol<extern fn(_:  &AzGlCallbackInfoPtr) -> AzGlCallbackInfoPtr>,
        pub az_gl_callback_return_delete: Symbol<extern fn(_:  &mut AzGlCallbackReturnPtr)>,
        pub az_gl_callback_return_shallow_copy: Symbol<extern fn(_:  &AzGlCallbackReturnPtr) -> AzGlCallbackReturnPtr>,
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
        pub az_dom_div: Symbol<extern fn() -> AzDomPtr>,
        pub az_dom_body: Symbol<extern fn() -> AzDomPtr>,
        pub az_dom_label: Symbol<extern fn(_:  AzString) -> AzDomPtr>,
        pub az_dom_text: Symbol<extern fn(_:  AzTextId) -> AzDomPtr>,
        pub az_dom_image: Symbol<extern fn(_:  AzImageId) -> AzDomPtr>,
        pub az_dom_gl_texture: Symbol<extern fn(_:  AzRefAny, _:  AzGlCallback) -> AzDomPtr>,
        pub az_dom_iframe_callback: Symbol<extern fn(_:  AzRefAny, _:  AzIFrameCallback) -> AzDomPtr>,
        pub az_dom_add_id: Symbol<extern fn(_:  &mut AzDomPtr, _:  AzString)>,
        pub az_dom_with_id: Symbol<extern fn(_:  AzDomPtr, _:  AzString) -> AzDomPtr>,
        pub az_dom_set_ids: Symbol<extern fn(_:  &mut AzDomPtr, _:  AzStringVec)>,
        pub az_dom_with_ids: Symbol<extern fn(_:  AzDomPtr, _:  AzStringVec) -> AzDomPtr>,
        pub az_dom_add_class: Symbol<extern fn(_:  &mut AzDomPtr, _:  AzString)>,
        pub az_dom_with_class: Symbol<extern fn(_:  AzDomPtr, _:  AzString) -> AzDomPtr>,
        pub az_dom_set_classes: Symbol<extern fn(_:  &mut AzDomPtr, _:  AzStringVec)>,
        pub az_dom_with_classes: Symbol<extern fn(_:  AzDomPtr, _:  AzStringVec) -> AzDomPtr>,
        pub az_dom_add_callback: Symbol<extern fn(_:  &mut AzDomPtr, _:  AzEventFilter, _:  AzRefAny, _:  AzCallback)>,
        pub az_dom_with_callback: Symbol<extern fn(_:  AzDomPtr, _:  AzEventFilter, _:  AzRefAny, _:  AzCallback) -> AzDomPtr>,
        pub az_dom_add_css_override: Symbol<extern fn(_:  &mut AzDomPtr, _:  AzString, _:  AzCssProperty)>,
        pub az_dom_with_css_override: Symbol<extern fn(_:  AzDomPtr, _:  AzString, _:  AzCssProperty) -> AzDomPtr>,
        pub az_dom_set_is_draggable: Symbol<extern fn(_:  &mut AzDomPtr, _:  bool)>,
        pub az_dom_is_draggable: Symbol<extern fn(_:  AzDomPtr, _:  bool) -> AzDomPtr>,
        pub az_dom_set_tab_index: Symbol<extern fn(_:  &mut AzDomPtr, _:  AzTabIndex)>,
        pub az_dom_with_tab_index: Symbol<extern fn(_:  AzDomPtr, _:  AzTabIndex) -> AzDomPtr>,
        pub az_dom_add_child: Symbol<extern fn(_:  &mut AzDomPtr, _:  AzDomPtr)>,
        pub az_dom_with_child: Symbol<extern fn(_:  AzDomPtr, _:  AzDomPtr) -> AzDomPtr>,
        pub az_dom_has_id: Symbol<extern fn(_:  &mut AzDomPtr, _:  AzString) -> bool>,
        pub az_dom_has_class: Symbol<extern fn(_:  &mut AzDomPtr, _:  AzString) -> bool>,
        pub az_dom_get_html_string: Symbol<extern fn(_:  &mut AzDomPtr) -> AzString>,
        pub az_dom_delete: Symbol<extern fn(_:  &mut AzDomPtr)>,
        pub az_dom_shallow_copy: Symbol<extern fn(_:  &AzDomPtr) -> AzDomPtr>,
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
        pub az_window_create_options_new: Symbol<extern fn(_:  AzCssPtr) -> AzWindowCreateOptionsPtr>,
        pub az_window_create_options_delete: Symbol<extern fn(_:  &mut AzWindowCreateOptionsPtr)>,
        pub az_window_create_options_shallow_copy: Symbol<extern fn(_:  &AzWindowCreateOptionsPtr) -> AzWindowCreateOptionsPtr>,
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
        let az_u8_vec_as_ptr = unsafe { lib.get::<extern fn(_:  &AzU8Vec) -> *const u8>(b"az_u8_vec_as_ptr").ok()? };
        let az_u8_vec_len = unsafe { lib.get::<extern fn(_:  &AzU8Vec) -> usize>(b"az_u8_vec_len").ok()? };
        let az_u8_vec_capacity = unsafe { lib.get::<extern fn(_:  &AzU8Vec) -> usize>(b"az_u8_vec_capacity").ok()? };
        let az_u8_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzU8Vec)>(b"az_u8_vec_delete").ok()? };
        let az_u8_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzU8Vec) -> AzU8Vec>(b"az_u8_vec_deep_copy").ok()? };
        let az_string_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzString, _:  usize) -> AzStringVec>(b"az_string_vec_copy_from").ok()? };
        let az_string_vec_as_ptr = unsafe { lib.get::<extern fn(_:  &AzStringVec) -> *const AzString>(b"az_string_vec_as_ptr").ok()? };
        let az_string_vec_len = unsafe { lib.get::<extern fn(_:  &AzStringVec) -> usize>(b"az_string_vec_len").ok()? };
        let az_string_vec_capacity = unsafe { lib.get::<extern fn(_:  &AzStringVec) -> usize>(b"az_string_vec_capacity").ok()? };
        let az_string_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzStringVec)>(b"az_string_vec_delete").ok()? };
        let az_string_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzStringVec) -> AzStringVec>(b"az_string_vec_deep_copy").ok()? };
        let az_gradient_stop_pre_vec_copy_from = unsafe { lib.get::<extern fn(_:  *const AzGradientStopPre, _:  usize) -> AzGradientStopPreVec>(b"az_gradient_stop_pre_vec_copy_from").ok()? };
        let az_gradient_stop_pre_vec_as_ptr = unsafe { lib.get::<extern fn(_:  &AzGradientStopPreVec) -> *const AzGradientStopPre>(b"az_gradient_stop_pre_vec_as_ptr").ok()? };
        let az_gradient_stop_pre_vec_len = unsafe { lib.get::<extern fn(_:  &AzGradientStopPreVec) -> usize>(b"az_gradient_stop_pre_vec_len").ok()? };
        let az_gradient_stop_pre_vec_capacity = unsafe { lib.get::<extern fn(_:  &AzGradientStopPreVec) -> usize>(b"az_gradient_stop_pre_vec_capacity").ok()? };
        let az_gradient_stop_pre_vec_delete = unsafe { lib.get::<extern fn(_:  &mut AzGradientStopPreVec)>(b"az_gradient_stop_pre_vec_delete").ok()? };
        let az_gradient_stop_pre_vec_deep_copy = unsafe { lib.get::<extern fn(_:  &AzGradientStopPreVec) -> AzGradientStopPreVec>(b"az_gradient_stop_pre_vec_deep_copy").ok()? };
        let az_option_percentage_value_delete = unsafe { lib.get::<extern fn(_:  &mut AzOptionPercentageValue)>(b"az_option_percentage_value_delete").ok()? };
        let az_option_percentage_value_deep_copy = unsafe { lib.get::<extern fn(_:  &AzOptionPercentageValue) -> AzOptionPercentageValue>(b"az_option_percentage_value_deep_copy").ok()? };
        let az_app_config_default = unsafe { lib.get::<extern fn() -> AzAppConfigPtr>(b"az_app_config_default").ok()? };
        let az_app_config_delete = unsafe { lib.get::<extern fn(_:  &mut AzAppConfigPtr)>(b"az_app_config_delete").ok()? };
        let az_app_config_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzAppConfigPtr) -> AzAppConfigPtr>(b"az_app_config_shallow_copy").ok()? };
        let az_app_new = unsafe { lib.get::<extern fn(_:  AzRefAny, _:  AzAppConfigPtr, _:  AzLayoutCallback) -> AzAppPtr>(b"az_app_new").ok()? };
        let az_app_run = unsafe { lib.get::<extern fn(_:  AzAppPtr, _:  AzWindowCreateOptionsPtr)>(b"az_app_run").ok()? };
        let az_app_delete = unsafe { lib.get::<extern fn(_:  &mut AzAppPtr)>(b"az_app_delete").ok()? };
        let az_app_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzAppPtr) -> AzAppPtr>(b"az_app_shallow_copy").ok()? };
        let az_callback_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzCallbackInfoPtr)>(b"az_callback_info_delete").ok()? };
        let az_callback_info_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzCallbackInfoPtr) -> AzCallbackInfoPtr>(b"az_callback_info_shallow_copy").ok()? };
        let az_update_screen_delete = unsafe { lib.get::<extern fn(_:  &mut AzUpdateScreen)>(b"az_update_screen_delete").ok()? };
        let az_update_screen_deep_copy = unsafe { lib.get::<extern fn(_:  &AzUpdateScreen) -> AzUpdateScreen>(b"az_update_screen_deep_copy").ok()? };
        let az_i_frame_callback_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzIFrameCallbackInfoPtr)>(b"az_i_frame_callback_info_delete").ok()? };
        let az_i_frame_callback_info_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzIFrameCallbackInfoPtr) -> AzIFrameCallbackInfoPtr>(b"az_i_frame_callback_info_shallow_copy").ok()? };
        let az_i_frame_callback_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzIFrameCallbackReturnPtr)>(b"az_i_frame_callback_return_delete").ok()? };
        let az_i_frame_callback_return_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzIFrameCallbackReturnPtr) -> AzIFrameCallbackReturnPtr>(b"az_i_frame_callback_return_shallow_copy").ok()? };
        let az_gl_callback_info_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlCallbackInfoPtr)>(b"az_gl_callback_info_delete").ok()? };
        let az_gl_callback_info_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzGlCallbackInfoPtr) -> AzGlCallbackInfoPtr>(b"az_gl_callback_info_shallow_copy").ok()? };
        let az_gl_callback_return_delete = unsafe { lib.get::<extern fn(_:  &mut AzGlCallbackReturnPtr)>(b"az_gl_callback_return_delete").ok()? };
        let az_gl_callback_return_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzGlCallbackReturnPtr) -> AzGlCallbackReturnPtr>(b"az_gl_callback_return_shallow_copy").ok()? };
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
        let az_dom_div = unsafe { lib.get::<extern fn() -> AzDomPtr>(b"az_dom_div").ok()? };
        let az_dom_body = unsafe { lib.get::<extern fn() -> AzDomPtr>(b"az_dom_body").ok()? };
        let az_dom_label = unsafe { lib.get::<extern fn(_:  AzString) -> AzDomPtr>(b"az_dom_label").ok()? };
        let az_dom_text = unsafe { lib.get::<extern fn(_:  AzTextId) -> AzDomPtr>(b"az_dom_text").ok()? };
        let az_dom_image = unsafe { lib.get::<extern fn(_:  AzImageId) -> AzDomPtr>(b"az_dom_image").ok()? };
        let az_dom_gl_texture = unsafe { lib.get::<extern fn(_:  AzRefAny, _:  AzGlCallback) -> AzDomPtr>(b"az_dom_gl_texture").ok()? };
        let az_dom_iframe_callback = unsafe { lib.get::<extern fn(_:  AzRefAny, _:  AzIFrameCallback) -> AzDomPtr>(b"az_dom_iframe_callback").ok()? };
        let az_dom_add_id = unsafe { lib.get::<extern fn(_:  &mut AzDomPtr, _:  AzString)>(b"az_dom_add_id").ok()? };
        let az_dom_with_id = unsafe { lib.get::<extern fn(_:  AzDomPtr, _:  AzString) -> AzDomPtr>(b"az_dom_with_id").ok()? };
        let az_dom_set_ids = unsafe { lib.get::<extern fn(_:  &mut AzDomPtr, _:  AzStringVec)>(b"az_dom_set_ids").ok()? };
        let az_dom_with_ids = unsafe { lib.get::<extern fn(_:  AzDomPtr, _:  AzStringVec) -> AzDomPtr>(b"az_dom_with_ids").ok()? };
        let az_dom_add_class = unsafe { lib.get::<extern fn(_:  &mut AzDomPtr, _:  AzString)>(b"az_dom_add_class").ok()? };
        let az_dom_with_class = unsafe { lib.get::<extern fn(_:  AzDomPtr, _:  AzString) -> AzDomPtr>(b"az_dom_with_class").ok()? };
        let az_dom_set_classes = unsafe { lib.get::<extern fn(_:  &mut AzDomPtr, _:  AzStringVec)>(b"az_dom_set_classes").ok()? };
        let az_dom_with_classes = unsafe { lib.get::<extern fn(_:  AzDomPtr, _:  AzStringVec) -> AzDomPtr>(b"az_dom_with_classes").ok()? };
        let az_dom_add_callback = unsafe { lib.get::<extern fn(_:  &mut AzDomPtr, _:  AzEventFilter, _:  AzRefAny, _:  AzCallback)>(b"az_dom_add_callback").ok()? };
        let az_dom_with_callback = unsafe { lib.get::<extern fn(_:  AzDomPtr, _:  AzEventFilter, _:  AzRefAny, _:  AzCallback) -> AzDomPtr>(b"az_dom_with_callback").ok()? };
        let az_dom_add_css_override = unsafe { lib.get::<extern fn(_:  &mut AzDomPtr, _:  AzString, _:  AzCssProperty)>(b"az_dom_add_css_override").ok()? };
        let az_dom_with_css_override = unsafe { lib.get::<extern fn(_:  AzDomPtr, _:  AzString, _:  AzCssProperty) -> AzDomPtr>(b"az_dom_with_css_override").ok()? };
        let az_dom_set_is_draggable = unsafe { lib.get::<extern fn(_:  &mut AzDomPtr, _:  bool)>(b"az_dom_set_is_draggable").ok()? };
        let az_dom_is_draggable = unsafe { lib.get::<extern fn(_:  AzDomPtr, _:  bool) -> AzDomPtr>(b"az_dom_is_draggable").ok()? };
        let az_dom_set_tab_index = unsafe { lib.get::<extern fn(_:  &mut AzDomPtr, _:  AzTabIndex)>(b"az_dom_set_tab_index").ok()? };
        let az_dom_with_tab_index = unsafe { lib.get::<extern fn(_:  AzDomPtr, _:  AzTabIndex) -> AzDomPtr>(b"az_dom_with_tab_index").ok()? };
        let az_dom_add_child = unsafe { lib.get::<extern fn(_:  &mut AzDomPtr, _:  AzDomPtr)>(b"az_dom_add_child").ok()? };
        let az_dom_with_child = unsafe { lib.get::<extern fn(_:  AzDomPtr, _:  AzDomPtr) -> AzDomPtr>(b"az_dom_with_child").ok()? };
        let az_dom_has_id = unsafe { lib.get::<extern fn(_:  &mut AzDomPtr, _:  AzString) -> bool>(b"az_dom_has_id").ok()? };
        let az_dom_has_class = unsafe { lib.get::<extern fn(_:  &mut AzDomPtr, _:  AzString) -> bool>(b"az_dom_has_class").ok()? };
        let az_dom_get_html_string = unsafe { lib.get::<extern fn(_:  &mut AzDomPtr) -> AzString>(b"az_dom_get_html_string").ok()? };
        let az_dom_delete = unsafe { lib.get::<extern fn(_:  &mut AzDomPtr)>(b"az_dom_delete").ok()? };
        let az_dom_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzDomPtr) -> AzDomPtr>(b"az_dom_shallow_copy").ok()? };
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
        let az_window_create_options_new = unsafe { lib.get::<extern fn(_:  AzCssPtr) -> AzWindowCreateOptionsPtr>(b"az_window_create_options_new").ok()? };
        let az_window_create_options_delete = unsafe { lib.get::<extern fn(_:  &mut AzWindowCreateOptionsPtr)>(b"az_window_create_options_delete").ok()? };
        let az_window_create_options_shallow_copy = unsafe { lib.get::<extern fn(_:  &AzWindowCreateOptionsPtr) -> AzWindowCreateOptionsPtr>(b"az_window_create_options_shallow_copy").ok()? };
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
            az_u8_vec_as_ptr,
            az_u8_vec_len,
            az_u8_vec_capacity,
            az_u8_vec_delete,
            az_u8_vec_deep_copy,
            az_string_vec_copy_from,
            az_string_vec_as_ptr,
            az_string_vec_len,
            az_string_vec_capacity,
            az_string_vec_delete,
            az_string_vec_deep_copy,
            az_gradient_stop_pre_vec_copy_from,
            az_gradient_stop_pre_vec_as_ptr,
            az_gradient_stop_pre_vec_len,
            az_gradient_stop_pre_vec_capacity,
            az_gradient_stop_pre_vec_delete,
            az_gradient_stop_pre_vec_deep_copy,
            az_option_percentage_value_delete,
            az_option_percentage_value_deep_copy,
            az_app_config_default,
            az_app_config_delete,
            az_app_config_shallow_copy,
            az_app_new,
            az_app_run,
            az_app_delete,
            az_app_shallow_copy,
            az_callback_info_delete,
            az_callback_info_shallow_copy,
            az_update_screen_delete,
            az_update_screen_deep_copy,
            az_i_frame_callback_info_delete,
            az_i_frame_callback_info_shallow_copy,
            az_i_frame_callback_return_delete,
            az_i_frame_callback_return_shallow_copy,
            az_gl_callback_info_delete,
            az_gl_callback_info_shallow_copy,
            az_gl_callback_return_delete,
            az_gl_callback_return_shallow_copy,
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
            az_dom_iframe_callback,
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
            az_dom_shallow_copy,
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
            az_window_create_options_new,
            az_window_create_options_delete,
            az_window_create_options_shallow_copy,
            az_ref_any_new,
            az_ref_any_get_ptr,
            az_ref_any_get_mut_ptr,
            az_ref_any_shallow_copy,
            az_ref_any_delete,
            az_ref_any_core_copy,
        })
    }

    const LIB_BYTES: &[u8] = include_bytes!("../../../target/debug/libazul.so");

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

    impl Drop for String { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_delete)(self); } }
}

/// Definition of azuls internal `Vec<*>` wrappers
#[allow(dead_code, unused_imports)]
pub mod vec {

    use crate::dll::*;
    impl From<std::vec::Vec<u8>> for crate::vec::U8Vec {
        fn from(v: std::vec::Vec<u8>) -> crate::vec::U8Vec {
            crate::vec::U8Vec::copy_from(v.as_ptr(), v.len())
        }
    }

    impl From<crate::vec::U8Vec> for std::vec::Vec<u8> {
        fn from(v: crate::vec::U8Vec) -> std::vec::Vec<u8> {
            unsafe { std::slice::from_raw_parts(v.as_ptr(), v.len()) }.to_vec()
        }
    }

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
    }    use crate::str::String;
    use crate::css::GradientStopPre;


    /// Wrapper over a Rust-allocated `U8Vec`
    pub use crate::dll::AzU8Vec as U8Vec;

    impl U8Vec {
        /// Creates + allocates a Rust `Vec<String>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u8, len: usize) -> Self { (crate::dll::get_azul_dll().az_u8_vec_copy_from)(ptr, len) }
        /// Returns the internal pointer to the (azul-dll allocated) [u8]
        pub fn as_ptr(&self)  -> *const u8 { (crate::dll::get_azul_dll().az_u8_vec_as_ptr)(self) }
        /// Returns the length of the internal `Vec<u8>`
        pub fn len(&self)  -> usize { (crate::dll::get_azul_dll().az_u8_vec_len)(self) }
        /// Returns the capacity of the internal `Vec<u8>`
        pub fn capacity(&self)  -> usize { (crate::dll::get_azul_dll().az_u8_vec_capacity)(self) }
    }

    impl Drop for U8Vec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_u8_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `StringVec`
    pub use crate::dll::AzStringVec as StringVec;

    impl StringVec {
        /// Creates + allocates a Rust `Vec<String>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzString, len: usize) -> Self { (crate::dll::get_azul_dll().az_string_vec_copy_from)(ptr, len) }
        /// Returns the internal pointer to the (azul-dll allocated) [AzString]
        pub fn as_ptr(&self)  ->*const  crate::str::String { { (crate::dll::get_azul_dll().az_string_vec_as_ptr)(self)} }
        /// Returns the length of the internal `Vec<AzString>`
        pub fn len(&self)  -> usize { (crate::dll::get_azul_dll().az_string_vec_len)(self) }
        /// Returns the capacity of the internal `Vec<AzString>`
        pub fn capacity(&self)  -> usize { (crate::dll::get_azul_dll().az_string_vec_capacity)(self) }
    }

    impl Drop for StringVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_string_vec_delete)(self); } }


    /// Wrapper over a Rust-allocated `GradientStopPreVec`
    pub use crate::dll::AzGradientStopPreVec as GradientStopPreVec;

    impl GradientStopPreVec {
        /// Creates + allocates a Rust `Vec<GradientStopPre>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzGradientStopPre, len: usize) -> Self { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_copy_from)(ptr, len) }
        /// Returns the internal pointer to the (azul-dll allocated) [GradientStopPre]
        pub fn as_ptr(&self)  ->*const  crate::css::GradientStopPre { { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_as_ptr)(self)} }
        /// Returns the length of the internal `Vec<GradientStopPre>`
        pub fn len(&self)  -> usize { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_len)(self) }
        /// Returns the capacity of the internal `Vec<GradientStopPre>`
        pub fn capacity(&self)  -> usize { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_capacity)(self) }
    }

    impl Drop for GradientStopPreVec { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gradient_stop_pre_vec_delete)(self); } }
}

/// Definition of azuls internal `Option<*>` wrappers
#[allow(dead_code, unused_imports)]
pub mod option {

    use crate::dll::*;


    /// `OptionPercentageValue` struct
    pub use crate::dll::AzOptionPercentageValue as OptionPercentageValue;

    impl Drop for OptionPercentageValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_option_percentage_value_delete)(self); } }
}

/// `App` construction and configuration
#[allow(dead_code, unused_imports)]
pub mod app {

    use crate::dll::*;
    use crate::callbacks::{LayoutCallback, RefAny};
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
        pub fn new(data: RefAny, config: AppConfig, callback: LayoutCallback) -> Self { (crate::dll::get_azul_dll().az_app_new)(data, config, callback) }
        /// Runs the application. Due to platform restrictions (specifically `WinMain` on Windows), this function never returns.
        pub fn run(self, window: WindowCreateOptions)  { (crate::dll::get_azul_dll().az_app_run)(self, window) }
    }

    impl Drop for App { fn drop(&mut self) { (crate::dll::get_azul_dll().az_app_delete)(self); } }
}

/// Callback type definitions + struct definitions of `CallbackInfo`s
#[allow(dead_code, unused_imports)]
pub mod callbacks {

    use crate::dll::*;


    pub use crate::dll::AzLayoutCallback as LayoutCallback;

    pub use crate::dll::AzCallbackReturn as CallbackReturn;
    pub use crate::dll::AzCallback as Callback;

    /// `CallbackInfo` struct
    pub use crate::dll::AzCallbackInfoPtr as CallbackInfo;

    impl Drop for CallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_callback_info_delete)(self); } }


    /// Specifies if the screen should be updated after the callback function has returned
    pub use crate::dll::AzUpdateScreen as UpdateScreen;

    impl Drop for UpdateScreen { fn drop(&mut self) { (crate::dll::get_azul_dll().az_update_screen_delete)(self); } }


    pub use crate::dll::AzIFrameCallback as IFrameCallback;

    /// `IFrameCallbackInfo` struct
    pub use crate::dll::AzIFrameCallbackInfoPtr as IFrameCallbackInfo;

    impl Drop for IFrameCallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i_frame_callback_info_delete)(self); } }


    /// `IFrameCallbackReturn` struct
    pub use crate::dll::AzIFrameCallbackReturnPtr as IFrameCallbackReturn;

    impl Drop for IFrameCallbackReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_i_frame_callback_return_delete)(self); } }


    pub use crate::dll::AzGlCallback as GlCallback;

    /// `GlCallbackInfo` struct
    pub use crate::dll::AzGlCallbackInfoPtr as GlCallbackInfo;

    impl Drop for GlCallbackInfo { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_callback_info_delete)(self); } }


    /// `GlCallbackReturn` struct
    pub use crate::dll::AzGlCallbackReturnPtr as GlCallbackReturn;

    impl Drop for GlCallbackReturn { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gl_callback_return_delete)(self); } }


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

    impl Drop for ColorU { fn drop(&mut self) { (crate::dll::get_azul_dll().az_color_u_delete)(self); } }


    /// `SizeMetric` struct
    pub use crate::dll::AzSizeMetric as SizeMetric;

    impl Drop for SizeMetric { fn drop(&mut self) { (crate::dll::get_azul_dll().az_size_metric_delete)(self); } }


    /// `FloatValue` struct
    pub use crate::dll::AzFloatValue as FloatValue;

    impl Drop for FloatValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_float_value_delete)(self); } }


    /// `PixelValue` struct
    pub use crate::dll::AzPixelValue as PixelValue;

    impl Drop for PixelValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_pixel_value_delete)(self); } }


    /// `PixelValueNoPercent` struct
    pub use crate::dll::AzPixelValueNoPercent as PixelValueNoPercent;

    impl Drop for PixelValueNoPercent { fn drop(&mut self) { (crate::dll::get_azul_dll().az_pixel_value_no_percent_delete)(self); } }


    /// `BoxShadowClipMode` struct
    pub use crate::dll::AzBoxShadowClipMode as BoxShadowClipMode;

    impl Drop for BoxShadowClipMode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_box_shadow_clip_mode_delete)(self); } }


    /// `BoxShadowPreDisplayItem` struct
    pub use crate::dll::AzBoxShadowPreDisplayItem as BoxShadowPreDisplayItem;

    impl Drop for BoxShadowPreDisplayItem { fn drop(&mut self) { (crate::dll::get_azul_dll().az_box_shadow_pre_display_item_delete)(self); } }


    /// `LayoutAlignContent` struct
    pub use crate::dll::AzLayoutAlignContent as LayoutAlignContent;

    impl Drop for LayoutAlignContent { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_align_content_delete)(self); } }


    /// `LayoutAlignItems` struct
    pub use crate::dll::AzLayoutAlignItems as LayoutAlignItems;

    impl Drop for LayoutAlignItems { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_align_items_delete)(self); } }


    /// `LayoutBottom` struct
    pub use crate::dll::AzLayoutBottom as LayoutBottom;

    impl Drop for LayoutBottom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_bottom_delete)(self); } }


    /// `LayoutBoxSizing` struct
    pub use crate::dll::AzLayoutBoxSizing as LayoutBoxSizing;

    impl Drop for LayoutBoxSizing { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_box_sizing_delete)(self); } }


    /// `LayoutDirection` struct
    pub use crate::dll::AzLayoutDirection as LayoutDirection;

    impl Drop for LayoutDirection { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_direction_delete)(self); } }


    /// `LayoutDisplay` struct
    pub use crate::dll::AzLayoutDisplay as LayoutDisplay;

    impl Drop for LayoutDisplay { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_display_delete)(self); } }


    /// `LayoutFlexGrow` struct
    pub use crate::dll::AzLayoutFlexGrow as LayoutFlexGrow;

    impl Drop for LayoutFlexGrow { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_flex_grow_delete)(self); } }


    /// `LayoutFlexShrink` struct
    pub use crate::dll::AzLayoutFlexShrink as LayoutFlexShrink;

    impl Drop for LayoutFlexShrink { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_flex_shrink_delete)(self); } }


    /// `LayoutFloat` struct
    pub use crate::dll::AzLayoutFloat as LayoutFloat;

    impl Drop for LayoutFloat { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_float_delete)(self); } }


    /// `LayoutHeight` struct
    pub use crate::dll::AzLayoutHeight as LayoutHeight;

    impl Drop for LayoutHeight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_height_delete)(self); } }


    /// `LayoutJustifyContent` struct
    pub use crate::dll::AzLayoutJustifyContent as LayoutJustifyContent;

    impl Drop for LayoutJustifyContent { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_justify_content_delete)(self); } }


    /// `LayoutLeft` struct
    pub use crate::dll::AzLayoutLeft as LayoutLeft;

    impl Drop for LayoutLeft { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_left_delete)(self); } }


    /// `LayoutMarginBottom` struct
    pub use crate::dll::AzLayoutMarginBottom as LayoutMarginBottom;

    impl Drop for LayoutMarginBottom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_bottom_delete)(self); } }


    /// `LayoutMarginLeft` struct
    pub use crate::dll::AzLayoutMarginLeft as LayoutMarginLeft;

    impl Drop for LayoutMarginLeft { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_left_delete)(self); } }


    /// `LayoutMarginRight` struct
    pub use crate::dll::AzLayoutMarginRight as LayoutMarginRight;

    impl Drop for LayoutMarginRight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_right_delete)(self); } }


    /// `LayoutMarginTop` struct
    pub use crate::dll::AzLayoutMarginTop as LayoutMarginTop;

    impl Drop for LayoutMarginTop { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_top_delete)(self); } }


    /// `LayoutMaxHeight` struct
    pub use crate::dll::AzLayoutMaxHeight as LayoutMaxHeight;

    impl Drop for LayoutMaxHeight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_max_height_delete)(self); } }


    /// `LayoutMaxWidth` struct
    pub use crate::dll::AzLayoutMaxWidth as LayoutMaxWidth;

    impl Drop for LayoutMaxWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_max_width_delete)(self); } }


    /// `LayoutMinHeight` struct
    pub use crate::dll::AzLayoutMinHeight as LayoutMinHeight;

    impl Drop for LayoutMinHeight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_min_height_delete)(self); } }


    /// `LayoutMinWidth` struct
    pub use crate::dll::AzLayoutMinWidth as LayoutMinWidth;

    impl Drop for LayoutMinWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_min_width_delete)(self); } }


    /// `LayoutPaddingBottom` struct
    pub use crate::dll::AzLayoutPaddingBottom as LayoutPaddingBottom;

    impl Drop for LayoutPaddingBottom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_bottom_delete)(self); } }


    /// `LayoutPaddingLeft` struct
    pub use crate::dll::AzLayoutPaddingLeft as LayoutPaddingLeft;

    impl Drop for LayoutPaddingLeft { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_left_delete)(self); } }


    /// `LayoutPaddingRight` struct
    pub use crate::dll::AzLayoutPaddingRight as LayoutPaddingRight;

    impl Drop for LayoutPaddingRight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_right_delete)(self); } }


    /// `LayoutPaddingTop` struct
    pub use crate::dll::AzLayoutPaddingTop as LayoutPaddingTop;

    impl Drop for LayoutPaddingTop { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_top_delete)(self); } }


    /// `LayoutPosition` struct
    pub use crate::dll::AzLayoutPosition as LayoutPosition;

    impl Drop for LayoutPosition { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_position_delete)(self); } }


    /// `LayoutRight` struct
    pub use crate::dll::AzLayoutRight as LayoutRight;

    impl Drop for LayoutRight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_right_delete)(self); } }


    /// `LayoutTop` struct
    pub use crate::dll::AzLayoutTop as LayoutTop;

    impl Drop for LayoutTop { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_top_delete)(self); } }


    /// `LayoutWidth` struct
    pub use crate::dll::AzLayoutWidth as LayoutWidth;

    impl Drop for LayoutWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_width_delete)(self); } }


    /// `LayoutWrap` struct
    pub use crate::dll::AzLayoutWrap as LayoutWrap;

    impl Drop for LayoutWrap { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_wrap_delete)(self); } }


    /// `Overflow` struct
    pub use crate::dll::AzOverflow as Overflow;

    impl Drop for Overflow { fn drop(&mut self) { (crate::dll::get_azul_dll().az_overflow_delete)(self); } }


    /// `PercentageValue` struct
    pub use crate::dll::AzPercentageValue as PercentageValue;

    impl Drop for PercentageValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_percentage_value_delete)(self); } }


    /// `GradientStopPre` struct
    pub use crate::dll::AzGradientStopPre as GradientStopPre;

    impl Drop for GradientStopPre { fn drop(&mut self) { (crate::dll::get_azul_dll().az_gradient_stop_pre_delete)(self); } }


    /// `DirectionCorner` struct
    pub use crate::dll::AzDirectionCorner as DirectionCorner;

    impl Drop for DirectionCorner { fn drop(&mut self) { (crate::dll::get_azul_dll().az_direction_corner_delete)(self); } }


    /// `DirectionCorners` struct
    pub use crate::dll::AzDirectionCorners as DirectionCorners;

    impl Drop for DirectionCorners { fn drop(&mut self) { (crate::dll::get_azul_dll().az_direction_corners_delete)(self); } }


    /// `Direction` struct
    pub use crate::dll::AzDirection as Direction;

    impl Drop for Direction { fn drop(&mut self) { (crate::dll::get_azul_dll().az_direction_delete)(self); } }


    /// `ExtendMode` struct
    pub use crate::dll::AzExtendMode as ExtendMode;

    impl Drop for ExtendMode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_extend_mode_delete)(self); } }


    /// `LinearGradient` struct
    pub use crate::dll::AzLinearGradient as LinearGradient;

    impl Drop for LinearGradient { fn drop(&mut self) { (crate::dll::get_azul_dll().az_linear_gradient_delete)(self); } }


    /// `Shape` struct
    pub use crate::dll::AzShape as Shape;

    impl Drop for Shape { fn drop(&mut self) { (crate::dll::get_azul_dll().az_shape_delete)(self); } }


    /// `RadialGradient` struct
    pub use crate::dll::AzRadialGradient as RadialGradient;

    impl Drop for RadialGradient { fn drop(&mut self) { (crate::dll::get_azul_dll().az_radial_gradient_delete)(self); } }


    /// `CssImageId` struct
    pub use crate::dll::AzCssImageId as CssImageId;

    impl Drop for CssImageId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_image_id_delete)(self); } }


    /// `StyleBackgroundContent` struct
    pub use crate::dll::AzStyleBackgroundContent as StyleBackgroundContent;

    impl Drop for StyleBackgroundContent { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_content_delete)(self); } }


    /// `BackgroundPositionHorizontal` struct
    pub use crate::dll::AzBackgroundPositionHorizontal as BackgroundPositionHorizontal;

    impl Drop for BackgroundPositionHorizontal { fn drop(&mut self) { (crate::dll::get_azul_dll().az_background_position_horizontal_delete)(self); } }


    /// `BackgroundPositionVertical` struct
    pub use crate::dll::AzBackgroundPositionVertical as BackgroundPositionVertical;

    impl Drop for BackgroundPositionVertical { fn drop(&mut self) { (crate::dll::get_azul_dll().az_background_position_vertical_delete)(self); } }


    /// `StyleBackgroundPosition` struct
    pub use crate::dll::AzStyleBackgroundPosition as StyleBackgroundPosition;

    impl Drop for StyleBackgroundPosition { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_position_delete)(self); } }


    /// `StyleBackgroundRepeat` struct
    pub use crate::dll::AzStyleBackgroundRepeat as StyleBackgroundRepeat;

    impl Drop for StyleBackgroundRepeat { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_repeat_delete)(self); } }


    /// `StyleBackgroundSize` struct
    pub use crate::dll::AzStyleBackgroundSize as StyleBackgroundSize;

    impl Drop for StyleBackgroundSize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_size_delete)(self); } }


    /// `StyleBorderBottomColor` struct
    pub use crate::dll::AzStyleBorderBottomColor as StyleBorderBottomColor;

    impl Drop for StyleBorderBottomColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_color_delete)(self); } }


    /// `StyleBorderBottomLeftRadius` struct
    pub use crate::dll::AzStyleBorderBottomLeftRadius as StyleBorderBottomLeftRadius;

    impl Drop for StyleBorderBottomLeftRadius { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_left_radius_delete)(self); } }


    /// `StyleBorderBottomRightRadius` struct
    pub use crate::dll::AzStyleBorderBottomRightRadius as StyleBorderBottomRightRadius;

    impl Drop for StyleBorderBottomRightRadius { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_right_radius_delete)(self); } }


    /// `BorderStyle` struct
    pub use crate::dll::AzBorderStyle as BorderStyle;

    impl Drop for BorderStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_border_style_delete)(self); } }


    /// `StyleBorderBottomStyle` struct
    pub use crate::dll::AzStyleBorderBottomStyle as StyleBorderBottomStyle;

    impl Drop for StyleBorderBottomStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_style_delete)(self); } }


    /// `StyleBorderBottomWidth` struct
    pub use crate::dll::AzStyleBorderBottomWidth as StyleBorderBottomWidth;

    impl Drop for StyleBorderBottomWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_width_delete)(self); } }


    /// `StyleBorderLeftColor` struct
    pub use crate::dll::AzStyleBorderLeftColor as StyleBorderLeftColor;

    impl Drop for StyleBorderLeftColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_color_delete)(self); } }


    /// `StyleBorderLeftStyle` struct
    pub use crate::dll::AzStyleBorderLeftStyle as StyleBorderLeftStyle;

    impl Drop for StyleBorderLeftStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_style_delete)(self); } }


    /// `StyleBorderLeftWidth` struct
    pub use crate::dll::AzStyleBorderLeftWidth as StyleBorderLeftWidth;

    impl Drop for StyleBorderLeftWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_width_delete)(self); } }


    /// `StyleBorderRightColor` struct
    pub use crate::dll::AzStyleBorderRightColor as StyleBorderRightColor;

    impl Drop for StyleBorderRightColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_color_delete)(self); } }


    /// `StyleBorderRightStyle` struct
    pub use crate::dll::AzStyleBorderRightStyle as StyleBorderRightStyle;

    impl Drop for StyleBorderRightStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_style_delete)(self); } }


    /// `StyleBorderRightWidth` struct
    pub use crate::dll::AzStyleBorderRightWidth as StyleBorderRightWidth;

    impl Drop for StyleBorderRightWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_width_delete)(self); } }


    /// `StyleBorderTopColor` struct
    pub use crate::dll::AzStyleBorderTopColor as StyleBorderTopColor;

    impl Drop for StyleBorderTopColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_color_delete)(self); } }


    /// `StyleBorderTopLeftRadius` struct
    pub use crate::dll::AzStyleBorderTopLeftRadius as StyleBorderTopLeftRadius;

    impl Drop for StyleBorderTopLeftRadius { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_left_radius_delete)(self); } }


    /// `StyleBorderTopRightRadius` struct
    pub use crate::dll::AzStyleBorderTopRightRadius as StyleBorderTopRightRadius;

    impl Drop for StyleBorderTopRightRadius { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_right_radius_delete)(self); } }


    /// `StyleBorderTopStyle` struct
    pub use crate::dll::AzStyleBorderTopStyle as StyleBorderTopStyle;

    impl Drop for StyleBorderTopStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_style_delete)(self); } }


    /// `StyleBorderTopWidth` struct
    pub use crate::dll::AzStyleBorderTopWidth as StyleBorderTopWidth;

    impl Drop for StyleBorderTopWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_width_delete)(self); } }


    /// `StyleCursor` struct
    pub use crate::dll::AzStyleCursor as StyleCursor;

    impl Drop for StyleCursor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_cursor_delete)(self); } }


    /// `StyleFontFamily` struct
    pub use crate::dll::AzStyleFontFamily as StyleFontFamily;

    impl Drop for StyleFontFamily { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_family_delete)(self); } }


    /// `StyleFontSize` struct
    pub use crate::dll::AzStyleFontSize as StyleFontSize;

    impl Drop for StyleFontSize { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_size_delete)(self); } }


    /// `StyleLetterSpacing` struct
    pub use crate::dll::AzStyleLetterSpacing as StyleLetterSpacing;

    impl Drop for StyleLetterSpacing { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_letter_spacing_delete)(self); } }


    /// `StyleLineHeight` struct
    pub use crate::dll::AzStyleLineHeight as StyleLineHeight;

    impl Drop for StyleLineHeight { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_line_height_delete)(self); } }


    /// `StyleTabWidth` struct
    pub use crate::dll::AzStyleTabWidth as StyleTabWidth;

    impl Drop for StyleTabWidth { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_tab_width_delete)(self); } }


    /// `StyleTextAlignmentHorz` struct
    pub use crate::dll::AzStyleTextAlignmentHorz as StyleTextAlignmentHorz;

    impl Drop for StyleTextAlignmentHorz { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_text_alignment_horz_delete)(self); } }


    /// `StyleTextColor` struct
    pub use crate::dll::AzStyleTextColor as StyleTextColor;

    impl Drop for StyleTextColor { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_text_color_delete)(self); } }


    /// `StyleWordSpacing` struct
    pub use crate::dll::AzStyleWordSpacing as StyleWordSpacing;

    impl Drop for StyleWordSpacing { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_word_spacing_delete)(self); } }


    /// `BoxShadowPreDisplayItemValue` struct
    pub use crate::dll::AzBoxShadowPreDisplayItemValue as BoxShadowPreDisplayItemValue;

    impl Drop for BoxShadowPreDisplayItemValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_box_shadow_pre_display_item_value_delete)(self); } }


    /// `LayoutAlignContentValue` struct
    pub use crate::dll::AzLayoutAlignContentValue as LayoutAlignContentValue;

    impl Drop for LayoutAlignContentValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_align_content_value_delete)(self); } }


    /// `LayoutAlignItemsValue` struct
    pub use crate::dll::AzLayoutAlignItemsValue as LayoutAlignItemsValue;

    impl Drop for LayoutAlignItemsValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_align_items_value_delete)(self); } }


    /// `LayoutBottomValue` struct
    pub use crate::dll::AzLayoutBottomValue as LayoutBottomValue;

    impl Drop for LayoutBottomValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_bottom_value_delete)(self); } }


    /// `LayoutBoxSizingValue` struct
    pub use crate::dll::AzLayoutBoxSizingValue as LayoutBoxSizingValue;

    impl Drop for LayoutBoxSizingValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_box_sizing_value_delete)(self); } }


    /// `LayoutDirectionValue` struct
    pub use crate::dll::AzLayoutDirectionValue as LayoutDirectionValue;

    impl Drop for LayoutDirectionValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_direction_value_delete)(self); } }


    /// `LayoutDisplayValue` struct
    pub use crate::dll::AzLayoutDisplayValue as LayoutDisplayValue;

    impl Drop for LayoutDisplayValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_display_value_delete)(self); } }


    /// `LayoutFlexGrowValue` struct
    pub use crate::dll::AzLayoutFlexGrowValue as LayoutFlexGrowValue;

    impl Drop for LayoutFlexGrowValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_flex_grow_value_delete)(self); } }


    /// `LayoutFlexShrinkValue` struct
    pub use crate::dll::AzLayoutFlexShrinkValue as LayoutFlexShrinkValue;

    impl Drop for LayoutFlexShrinkValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_flex_shrink_value_delete)(self); } }


    /// `LayoutFloatValue` struct
    pub use crate::dll::AzLayoutFloatValue as LayoutFloatValue;

    impl Drop for LayoutFloatValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_float_value_delete)(self); } }


    /// `LayoutHeightValue` struct
    pub use crate::dll::AzLayoutHeightValue as LayoutHeightValue;

    impl Drop for LayoutHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_height_value_delete)(self); } }


    /// `LayoutJustifyContentValue` struct
    pub use crate::dll::AzLayoutJustifyContentValue as LayoutJustifyContentValue;

    impl Drop for LayoutJustifyContentValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_justify_content_value_delete)(self); } }


    /// `LayoutLeftValue` struct
    pub use crate::dll::AzLayoutLeftValue as LayoutLeftValue;

    impl Drop for LayoutLeftValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_left_value_delete)(self); } }


    /// `LayoutMarginBottomValue` struct
    pub use crate::dll::AzLayoutMarginBottomValue as LayoutMarginBottomValue;

    impl Drop for LayoutMarginBottomValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_bottom_value_delete)(self); } }


    /// `LayoutMarginLeftValue` struct
    pub use crate::dll::AzLayoutMarginLeftValue as LayoutMarginLeftValue;

    impl Drop for LayoutMarginLeftValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_left_value_delete)(self); } }


    /// `LayoutMarginRightValue` struct
    pub use crate::dll::AzLayoutMarginRightValue as LayoutMarginRightValue;

    impl Drop for LayoutMarginRightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_right_value_delete)(self); } }


    /// `LayoutMarginTopValue` struct
    pub use crate::dll::AzLayoutMarginTopValue as LayoutMarginTopValue;

    impl Drop for LayoutMarginTopValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_margin_top_value_delete)(self); } }


    /// `LayoutMaxHeightValue` struct
    pub use crate::dll::AzLayoutMaxHeightValue as LayoutMaxHeightValue;

    impl Drop for LayoutMaxHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_max_height_value_delete)(self); } }


    /// `LayoutMaxWidthValue` struct
    pub use crate::dll::AzLayoutMaxWidthValue as LayoutMaxWidthValue;

    impl Drop for LayoutMaxWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_max_width_value_delete)(self); } }


    /// `LayoutMinHeightValue` struct
    pub use crate::dll::AzLayoutMinHeightValue as LayoutMinHeightValue;

    impl Drop for LayoutMinHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_min_height_value_delete)(self); } }


    /// `LayoutMinWidthValue` struct
    pub use crate::dll::AzLayoutMinWidthValue as LayoutMinWidthValue;

    impl Drop for LayoutMinWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_min_width_value_delete)(self); } }


    /// `LayoutPaddingBottomValue` struct
    pub use crate::dll::AzLayoutPaddingBottomValue as LayoutPaddingBottomValue;

    impl Drop for LayoutPaddingBottomValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_bottom_value_delete)(self); } }


    /// `LayoutPaddingLeftValue` struct
    pub use crate::dll::AzLayoutPaddingLeftValue as LayoutPaddingLeftValue;

    impl Drop for LayoutPaddingLeftValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_left_value_delete)(self); } }


    /// `LayoutPaddingRightValue` struct
    pub use crate::dll::AzLayoutPaddingRightValue as LayoutPaddingRightValue;

    impl Drop for LayoutPaddingRightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_right_value_delete)(self); } }


    /// `LayoutPaddingTopValue` struct
    pub use crate::dll::AzLayoutPaddingTopValue as LayoutPaddingTopValue;

    impl Drop for LayoutPaddingTopValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_padding_top_value_delete)(self); } }


    /// `LayoutPositionValue` struct
    pub use crate::dll::AzLayoutPositionValue as LayoutPositionValue;

    impl Drop for LayoutPositionValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_position_value_delete)(self); } }


    /// `LayoutRightValue` struct
    pub use crate::dll::AzLayoutRightValue as LayoutRightValue;

    impl Drop for LayoutRightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_right_value_delete)(self); } }


    /// `LayoutTopValue` struct
    pub use crate::dll::AzLayoutTopValue as LayoutTopValue;

    impl Drop for LayoutTopValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_top_value_delete)(self); } }


    /// `LayoutWidthValue` struct
    pub use crate::dll::AzLayoutWidthValue as LayoutWidthValue;

    impl Drop for LayoutWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_width_value_delete)(self); } }


    /// `LayoutWrapValue` struct
    pub use crate::dll::AzLayoutWrapValue as LayoutWrapValue;

    impl Drop for LayoutWrapValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_layout_wrap_value_delete)(self); } }


    /// `OverflowValue` struct
    pub use crate::dll::AzOverflowValue as OverflowValue;

    impl Drop for OverflowValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_overflow_value_delete)(self); } }


    /// `StyleBackgroundContentValue` struct
    pub use crate::dll::AzStyleBackgroundContentValue as StyleBackgroundContentValue;

    impl Drop for StyleBackgroundContentValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_content_value_delete)(self); } }


    /// `StyleBackgroundPositionValue` struct
    pub use crate::dll::AzStyleBackgroundPositionValue as StyleBackgroundPositionValue;

    impl Drop for StyleBackgroundPositionValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_position_value_delete)(self); } }


    /// `StyleBackgroundRepeatValue` struct
    pub use crate::dll::AzStyleBackgroundRepeatValue as StyleBackgroundRepeatValue;

    impl Drop for StyleBackgroundRepeatValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_repeat_value_delete)(self); } }


    /// `StyleBackgroundSizeValue` struct
    pub use crate::dll::AzStyleBackgroundSizeValue as StyleBackgroundSizeValue;

    impl Drop for StyleBackgroundSizeValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_size_value_delete)(self); } }


    /// `StyleBorderBottomColorValue` struct
    pub use crate::dll::AzStyleBorderBottomColorValue as StyleBorderBottomColorValue;

    impl Drop for StyleBorderBottomColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_color_value_delete)(self); } }


    /// `StyleBorderBottomLeftRadiusValue` struct
    pub use crate::dll::AzStyleBorderBottomLeftRadiusValue as StyleBorderBottomLeftRadiusValue;

    impl Drop for StyleBorderBottomLeftRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_left_radius_value_delete)(self); } }


    /// `StyleBorderBottomRightRadiusValue` struct
    pub use crate::dll::AzStyleBorderBottomRightRadiusValue as StyleBorderBottomRightRadiusValue;

    impl Drop for StyleBorderBottomRightRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_right_radius_value_delete)(self); } }


    /// `StyleBorderBottomStyleValue` struct
    pub use crate::dll::AzStyleBorderBottomStyleValue as StyleBorderBottomStyleValue;

    impl Drop for StyleBorderBottomStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_style_value_delete)(self); } }


    /// `StyleBorderBottomWidthValue` struct
    pub use crate::dll::AzStyleBorderBottomWidthValue as StyleBorderBottomWidthValue;

    impl Drop for StyleBorderBottomWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_bottom_width_value_delete)(self); } }


    /// `StyleBorderLeftColorValue` struct
    pub use crate::dll::AzStyleBorderLeftColorValue as StyleBorderLeftColorValue;

    impl Drop for StyleBorderLeftColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_color_value_delete)(self); } }


    /// `StyleBorderLeftStyleValue` struct
    pub use crate::dll::AzStyleBorderLeftStyleValue as StyleBorderLeftStyleValue;

    impl Drop for StyleBorderLeftStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_style_value_delete)(self); } }


    /// `StyleBorderLeftWidthValue` struct
    pub use crate::dll::AzStyleBorderLeftWidthValue as StyleBorderLeftWidthValue;

    impl Drop for StyleBorderLeftWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_left_width_value_delete)(self); } }


    /// `StyleBorderRightColorValue` struct
    pub use crate::dll::AzStyleBorderRightColorValue as StyleBorderRightColorValue;

    impl Drop for StyleBorderRightColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_color_value_delete)(self); } }


    /// `StyleBorderRightStyleValue` struct
    pub use crate::dll::AzStyleBorderRightStyleValue as StyleBorderRightStyleValue;

    impl Drop for StyleBorderRightStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_style_value_delete)(self); } }


    /// `StyleBorderRightWidthValue` struct
    pub use crate::dll::AzStyleBorderRightWidthValue as StyleBorderRightWidthValue;

    impl Drop for StyleBorderRightWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_right_width_value_delete)(self); } }


    /// `StyleBorderTopColorValue` struct
    pub use crate::dll::AzStyleBorderTopColorValue as StyleBorderTopColorValue;

    impl Drop for StyleBorderTopColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_color_value_delete)(self); } }


    /// `StyleBorderTopLeftRadiusValue` struct
    pub use crate::dll::AzStyleBorderTopLeftRadiusValue as StyleBorderTopLeftRadiusValue;

    impl Drop for StyleBorderTopLeftRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_left_radius_value_delete)(self); } }


    /// `StyleBorderTopRightRadiusValue` struct
    pub use crate::dll::AzStyleBorderTopRightRadiusValue as StyleBorderTopRightRadiusValue;

    impl Drop for StyleBorderTopRightRadiusValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_right_radius_value_delete)(self); } }


    /// `StyleBorderTopStyleValue` struct
    pub use crate::dll::AzStyleBorderTopStyleValue as StyleBorderTopStyleValue;

    impl Drop for StyleBorderTopStyleValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_style_value_delete)(self); } }


    /// `StyleBorderTopWidthValue` struct
    pub use crate::dll::AzStyleBorderTopWidthValue as StyleBorderTopWidthValue;

    impl Drop for StyleBorderTopWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_border_top_width_value_delete)(self); } }


    /// `StyleCursorValue` struct
    pub use crate::dll::AzStyleCursorValue as StyleCursorValue;

    impl Drop for StyleCursorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_cursor_value_delete)(self); } }


    /// `StyleFontFamilyValue` struct
    pub use crate::dll::AzStyleFontFamilyValue as StyleFontFamilyValue;

    impl Drop for StyleFontFamilyValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_family_value_delete)(self); } }


    /// `StyleFontSizeValue` struct
    pub use crate::dll::AzStyleFontSizeValue as StyleFontSizeValue;

    impl Drop for StyleFontSizeValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_size_value_delete)(self); } }


    /// `StyleLetterSpacingValue` struct
    pub use crate::dll::AzStyleLetterSpacingValue as StyleLetterSpacingValue;

    impl Drop for StyleLetterSpacingValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_letter_spacing_value_delete)(self); } }


    /// `StyleLineHeightValue` struct
    pub use crate::dll::AzStyleLineHeightValue as StyleLineHeightValue;

    impl Drop for StyleLineHeightValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_line_height_value_delete)(self); } }


    /// `StyleTabWidthValue` struct
    pub use crate::dll::AzStyleTabWidthValue as StyleTabWidthValue;

    impl Drop for StyleTabWidthValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_tab_width_value_delete)(self); } }


    /// `StyleTextAlignmentHorzValue` struct
    pub use crate::dll::AzStyleTextAlignmentHorzValue as StyleTextAlignmentHorzValue;

    impl Drop for StyleTextAlignmentHorzValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_text_alignment_horz_value_delete)(self); } }


    /// `StyleTextColorValue` struct
    pub use crate::dll::AzStyleTextColorValue as StyleTextColorValue;

    impl Drop for StyleTextColorValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_text_color_value_delete)(self); } }


    /// `StyleWordSpacingValue` struct
    pub use crate::dll::AzStyleWordSpacingValue as StyleWordSpacingValue;

    impl Drop for StyleWordSpacingValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_word_spacing_value_delete)(self); } }


    /// Parsed CSS key-value pair
    pub use crate::dll::AzCssProperty as CssProperty;

    impl Drop for CssProperty { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_property_delete)(self); } }
}

/// `Dom` construction and configuration
#[allow(dead_code, unused_imports)]
pub mod dom {

    use crate::dll::*;
    use crate::str::String;
    use crate::resources::{TextId, ImageId};
    use crate::callbacks::{Callback, GlCallback, IFrameCallback, RefAny};
    use crate::vec::StringVec;
    use crate::css::CssProperty;


    /// `Dom` struct
    pub use crate::dll::AzDomPtr as Dom;

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
        pub fn gl_texture(data: RefAny, callback: GlCallback) -> Self { (crate::dll::get_azul_dll().az_dom_gl_texture)(data, callback) }
        /// Creates a new node with a callback that will return a `Dom` after being layouted. See the documentation for [IFrameCallback]() for more info about iframe callbacks.
        pub fn iframe_callback(data: RefAny, callback: IFrameCallback) -> Self { (crate::dll::get_azul_dll().az_dom_iframe_callback)(data, callback) }
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
        pub fn add_callback(&mut self, event: EventFilter, data: RefAny, callback: Callback)  { (crate::dll::get_azul_dll().az_dom_add_callback)(self, event, data, callback) }
        /// Same as [`Dom::add_callback`](#method.add_callback), but as a builder method
        pub fn with_callback(self, event: EventFilter, data: RefAny, callback: Callback)  -> crate::dom::Dom { { (crate::dll::get_azul_dll().az_dom_with_callback)(self, event, data, callback)} }
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

    impl Drop for Dom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_dom_delete)(self); } }


    /// `EventFilter` struct
    pub use crate::dll::AzEventFilter as EventFilter;

    impl Drop for EventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_event_filter_delete)(self); } }


    /// `HoverEventFilter` struct
    pub use crate::dll::AzHoverEventFilter as HoverEventFilter;

    impl Drop for HoverEventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_hover_event_filter_delete)(self); } }


    /// `FocusEventFilter` struct
    pub use crate::dll::AzFocusEventFilter as FocusEventFilter;

    impl Drop for FocusEventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_focus_event_filter_delete)(self); } }


    /// `NotEventFilter` struct
    pub use crate::dll::AzNotEventFilter as NotEventFilter;

    impl Drop for NotEventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_not_event_filter_delete)(self); } }


    /// `WindowEventFilter` struct
    pub use crate::dll::AzWindowEventFilter as WindowEventFilter;

    impl Drop for WindowEventFilter { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_event_filter_delete)(self); } }


    /// `TabIndex` struct
    pub use crate::dll::AzTabIndex as TabIndex;

    impl Drop for TabIndex { fn drop(&mut self) { (crate::dll::get_azul_dll().az_tab_index_delete)(self); } }
}

/// Struct definition for image / font / text IDs
#[allow(dead_code, unused_imports)]
pub mod resources {

    use crate::dll::*;
    use crate::vec::U8Vec;


    /// `TextId` struct
    pub use crate::dll::AzTextId as TextId;

    impl TextId {
        /// Creates a new, unique `TextId`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_text_id_new)() }
    }

    impl Drop for TextId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_text_id_delete)(self); } }


    /// `ImageId` struct
    pub use crate::dll::AzImageId as ImageId;

    impl ImageId {
        /// Creates a new, unique `ImageId`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_image_id_new)() }
    }

    impl Drop for ImageId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_image_id_delete)(self); } }


    /// `FontId` struct
    pub use crate::dll::AzFontId as FontId;

    impl FontId {
        /// Creates a new, unique `FontId`
        pub fn new() -> Self { (crate::dll::get_azul_dll().az_font_id_new)() }
    }

    impl Drop for FontId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_font_id_delete)(self); } }


    /// `ImageSource` struct
    pub use crate::dll::AzImageSource as ImageSource;

    impl Drop for ImageSource { fn drop(&mut self) { (crate::dll::get_azul_dll().az_image_source_delete)(self); } }


    /// `FontSource` struct
    pub use crate::dll::AzFontSource as FontSource;

    impl Drop for FontSource { fn drop(&mut self) { (crate::dll::get_azul_dll().az_font_source_delete)(self); } }


    /// `RawImage` struct
    pub use crate::dll::AzRawImage as RawImage;

    impl RawImage {
        /// Creates a new `RawImage` by loading the decoded bytes
        pub fn new(decoded_pixels: U8Vec, width: usize, height: usize, data_format: RawImageFormat) -> Self { (crate::dll::get_azul_dll().az_raw_image_new)(decoded_pixels, width, height, data_format) }
    }

    impl Drop for RawImage { fn drop(&mut self) { (crate::dll::get_azul_dll().az_raw_image_delete)(self); } }


    /// `RawImageFormat` struct
    pub use crate::dll::AzRawImageFormat as RawImageFormat;

    impl Drop for RawImageFormat { fn drop(&mut self) { (crate::dll::get_azul_dll().az_raw_image_format_delete)(self); } }
}

/// Window creation / startup configuration
#[allow(dead_code, unused_imports)]
pub mod window {

    use crate::dll::*;
    use crate::css::Css;


    /// `WindowCreateOptions` struct
    pub use crate::dll::AzWindowCreateOptionsPtr as WindowCreateOptions;

    impl WindowCreateOptions {
        /// Creates a new `WindowCreateOptions` instance.
        pub fn new(css: Css) -> Self { (crate::dll::get_azul_dll().az_window_create_options_new)(css) }
    }

    impl Drop for WindowCreateOptions { fn drop(&mut self) { (crate::dll::get_azul_dll().az_window_create_options_delete)(self); } }
}

