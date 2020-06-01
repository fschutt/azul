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


extern crate azul_dll;

extern crate libloading;

pub(crate) mod dll {

    use std::ffi::c_void;

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
    #[repr(C)] pub struct AzPathBufPtr {
        pub ptr: *mut c_void,
    }
    #[repr(C)] pub struct AzOptionPercentageValuePtr {
        pub ptr: *mut c_void,
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
    #[repr(C)] pub struct AzStyleBorderRightWidthPtr {
        pub ptr: *mut c_void,
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
        File(AzPathBuf),
        Raw(AzRawImage),
    }
    #[repr(C, u8)] pub enum AzFontSource {
        Embedded(AzU8Vec),
        File(AzPathBuf),
        System(AzString),
    }
    #[repr(C)] pub struct AzRawImagePtr {
        pub ptr: *mut c_void,
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
        lib: Box<Library>,
        az_string_from_utf8_unchecked: Symbol<extern fn(_: usize) -> AzString>,
        az_string_from_utf8_lossy: Symbol<extern fn(_: usize) -> AzString>,
        az_string_into_bytes: Symbol<extern fn(_: AzString) -> AzU8Vec>,
        az_string_delete: Symbol<extern fn(_: &mut AzString)>,
        az_string_deep_copy: Symbol<extern fn(_: &AzString) -> AzString>,
        az_u8_vec_copy_from: Symbol<extern fn(_: usize) -> AzU8Vec>,
        az_u8_vec_as_ptr: Symbol<extern fn(_: &AzU8Vec) -> *const u8>,
        az_u8_vec_len: Symbol<extern fn(_: &AzU8Vec) -> usize>,
        az_u8_vec_delete: Symbol<extern fn(_: &mut AzU8Vec)>,
        az_u8_vec_deep_copy: Symbol<extern fn(_: &AzU8Vec) -> AzU8Vec>,
        az_string_vec_copy_from: Symbol<extern fn(_: usize) -> AzStringVec>,
        az_string_vec_delete: Symbol<extern fn(_: &mut AzStringVec)>,
        az_string_vec_deep_copy: Symbol<extern fn(_: &AzStringVec) -> AzStringVec>,
        az_gradient_stop_pre_vec_copy_from: Symbol<extern fn(_: usize) -> AzGradientStopPreVec>,
        az_gradient_stop_pre_vec_delete: Symbol<extern fn(_: &mut AzGradientStopPreVec)>,
        az_gradient_stop_pre_vec_deep_copy: Symbol<extern fn(_: &AzGradientStopPreVec) -> AzGradientStopPreVec>,
        az_path_buf_new: Symbol<extern fn(_: AzString) -> AzPathBufPtr>,
        az_path_buf_delete: Symbol<extern fn(_: &mut AzPathBufPtr)>,
        az_path_buf_shallow_copy: Symbol<extern fn(_: &AzPathBufPtr) -> AzPathBufPtr>,
        az_option_percentage_value_delete: Symbol<extern fn(_: &mut AzOptionPercentageValuePtr)>,
        az_option_percentage_value_shallow_copy: Symbol<extern fn(_: &AzOptionPercentageValuePtr) -> AzOptionPercentageValuePtr>,
        az_app_config_default: Symbol<extern fn() -> AzAppConfigPtr>,
        az_app_config_delete: Symbol<extern fn(_: &mut AzAppConfigPtr)>,
        az_app_config_shallow_copy: Symbol<extern fn(_: &AzAppConfigPtr) -> AzAppConfigPtr>,
        az_app_new: Symbol<extern fn(_: AzLayoutCallback) -> AzAppPtr>,
        az_app_run: Symbol<extern fn(_: AzWindowCreateOptionsPtr)>,
        az_app_delete: Symbol<extern fn(_: &mut AzAppPtr)>,
        az_app_shallow_copy: Symbol<extern fn(_: &AzAppPtr) -> AzAppPtr>,
        az_callback_info_delete: Symbol<extern fn(_: &mut AzCallbackInfoPtr)>,
        az_callback_info_shallow_copy: Symbol<extern fn(_: &AzCallbackInfoPtr) -> AzCallbackInfoPtr>,
        az_i_frame_callback_info_delete: Symbol<extern fn(_: &mut AzIFrameCallbackInfoPtr)>,
        az_i_frame_callback_info_shallow_copy: Symbol<extern fn(_: &AzIFrameCallbackInfoPtr) -> AzIFrameCallbackInfoPtr>,
        az_i_frame_callback_return_delete: Symbol<extern fn(_: &mut AzIFrameCallbackReturnPtr)>,
        az_i_frame_callback_return_shallow_copy: Symbol<extern fn(_: &AzIFrameCallbackReturnPtr) -> AzIFrameCallbackReturnPtr>,
        az_gl_callback_info_delete: Symbol<extern fn(_: &mut AzGlCallbackInfoPtr)>,
        az_gl_callback_info_shallow_copy: Symbol<extern fn(_: &AzGlCallbackInfoPtr) -> AzGlCallbackInfoPtr>,
        az_gl_callback_return_delete: Symbol<extern fn(_: &mut AzGlCallbackReturnPtr)>,
        az_gl_callback_return_shallow_copy: Symbol<extern fn(_: &AzGlCallbackReturnPtr) -> AzGlCallbackReturnPtr>,
        az_layout_info_delete: Symbol<extern fn(_: &mut AzLayoutInfoPtr)>,
        az_layout_info_shallow_copy: Symbol<extern fn(_: &AzLayoutInfoPtr) -> AzLayoutInfoPtr>,
        az_css_native: Symbol<extern fn() -> AzCssPtr>,
        az_css_empty: Symbol<extern fn() -> AzCssPtr>,
        az_css_from_string: Symbol<extern fn(_: AzString) -> AzCssPtr>,
        az_css_override_native: Symbol<extern fn(_: AzString) -> AzCssPtr>,
        az_css_delete: Symbol<extern fn(_: &mut AzCssPtr)>,
        az_css_shallow_copy: Symbol<extern fn(_: &AzCssPtr) -> AzCssPtr>,
        az_css_hot_reloader_new: Symbol<extern fn(_: u64) -> AzCssHotReloaderPtr>,
        az_css_hot_reloader_override_native: Symbol<extern fn(_: u64) -> AzCssHotReloaderPtr>,
        az_css_hot_reloader_delete: Symbol<extern fn(_: &mut AzCssHotReloaderPtr)>,
        az_css_hot_reloader_shallow_copy: Symbol<extern fn(_: &AzCssHotReloaderPtr) -> AzCssHotReloaderPtr>,
        az_color_u_delete: Symbol<extern fn(_: &mut AzColorU)>,
        az_color_u_deep_copy: Symbol<extern fn(_: &AzColorU) -> AzColorU>,
        az_size_metric_px: Symbol<extern fn() -> AzSizeMetric>,
        az_size_metric_pt: Symbol<extern fn() -> AzSizeMetric>,
        az_size_metric_em: Symbol<extern fn() -> AzSizeMetric>,
        az_size_metric_percent: Symbol<extern fn() -> AzSizeMetric>,
        az_size_metric_delete: Symbol<extern fn(_: &mut AzSizeMetric)>,
        az_size_metric_deep_copy: Symbol<extern fn(_: &AzSizeMetric) -> AzSizeMetric>,
        az_float_value_delete: Symbol<extern fn(_: &mut AzFloatValue)>,
        az_float_value_deep_copy: Symbol<extern fn(_: &AzFloatValue) -> AzFloatValue>,
        az_pixel_value_delete: Symbol<extern fn(_: &mut AzPixelValue)>,
        az_pixel_value_deep_copy: Symbol<extern fn(_: &AzPixelValue) -> AzPixelValue>,
        az_pixel_value_no_percent_delete: Symbol<extern fn(_: &mut AzPixelValueNoPercent)>,
        az_pixel_value_no_percent_deep_copy: Symbol<extern fn(_: &AzPixelValueNoPercent) -> AzPixelValueNoPercent>,
        az_box_shadow_clip_mode_outset: Symbol<extern fn() -> AzBoxShadowClipMode>,
        az_box_shadow_clip_mode_inset: Symbol<extern fn() -> AzBoxShadowClipMode>,
        az_box_shadow_clip_mode_delete: Symbol<extern fn(_: &mut AzBoxShadowClipMode)>,
        az_box_shadow_clip_mode_deep_copy: Symbol<extern fn(_: &AzBoxShadowClipMode) -> AzBoxShadowClipMode>,
        az_box_shadow_pre_display_item_delete: Symbol<extern fn(_: &mut AzBoxShadowPreDisplayItem)>,
        az_box_shadow_pre_display_item_deep_copy: Symbol<extern fn(_: &AzBoxShadowPreDisplayItem) -> AzBoxShadowPreDisplayItem>,
        az_layout_align_content_stretch: Symbol<extern fn() -> AzLayoutAlignContent>,
        az_layout_align_content_center: Symbol<extern fn() -> AzLayoutAlignContent>,
        az_layout_align_content_start: Symbol<extern fn() -> AzLayoutAlignContent>,
        az_layout_align_content_end: Symbol<extern fn() -> AzLayoutAlignContent>,
        az_layout_align_content_space_between: Symbol<extern fn() -> AzLayoutAlignContent>,
        az_layout_align_content_space_around: Symbol<extern fn() -> AzLayoutAlignContent>,
        az_layout_align_content_delete: Symbol<extern fn(_: &mut AzLayoutAlignContent)>,
        az_layout_align_content_deep_copy: Symbol<extern fn(_: &AzLayoutAlignContent) -> AzLayoutAlignContent>,
        az_layout_align_items_stretch: Symbol<extern fn() -> AzLayoutAlignItems>,
        az_layout_align_items_center: Symbol<extern fn() -> AzLayoutAlignItems>,
        az_layout_align_items_start: Symbol<extern fn() -> AzLayoutAlignItems>,
        az_layout_align_items_end: Symbol<extern fn() -> AzLayoutAlignItems>,
        az_layout_align_items_delete: Symbol<extern fn(_: &mut AzLayoutAlignItems)>,
        az_layout_align_items_deep_copy: Symbol<extern fn(_: &AzLayoutAlignItems) -> AzLayoutAlignItems>,
        az_layout_bottom_delete: Symbol<extern fn(_: &mut AzLayoutBottom)>,
        az_layout_bottom_deep_copy: Symbol<extern fn(_: &AzLayoutBottom) -> AzLayoutBottom>,
        az_layout_box_sizing_content_box: Symbol<extern fn() -> AzLayoutBoxSizing>,
        az_layout_box_sizing_border_box: Symbol<extern fn() -> AzLayoutBoxSizing>,
        az_layout_box_sizing_delete: Symbol<extern fn(_: &mut AzLayoutBoxSizing)>,
        az_layout_box_sizing_deep_copy: Symbol<extern fn(_: &AzLayoutBoxSizing) -> AzLayoutBoxSizing>,
        az_layout_direction_row: Symbol<extern fn() -> AzLayoutDirection>,
        az_layout_direction_row_reverse: Symbol<extern fn() -> AzLayoutDirection>,
        az_layout_direction_column: Symbol<extern fn() -> AzLayoutDirection>,
        az_layout_direction_column_reverse: Symbol<extern fn() -> AzLayoutDirection>,
        az_layout_direction_delete: Symbol<extern fn(_: &mut AzLayoutDirection)>,
        az_layout_direction_deep_copy: Symbol<extern fn(_: &AzLayoutDirection) -> AzLayoutDirection>,
        az_layout_display_flex: Symbol<extern fn() -> AzLayoutDisplay>,
        az_layout_display_block: Symbol<extern fn() -> AzLayoutDisplay>,
        az_layout_display_inline_block: Symbol<extern fn() -> AzLayoutDisplay>,
        az_layout_display_delete: Symbol<extern fn(_: &mut AzLayoutDisplay)>,
        az_layout_display_deep_copy: Symbol<extern fn(_: &AzLayoutDisplay) -> AzLayoutDisplay>,
        az_layout_flex_grow_delete: Symbol<extern fn(_: &mut AzLayoutFlexGrow)>,
        az_layout_flex_grow_deep_copy: Symbol<extern fn(_: &AzLayoutFlexGrow) -> AzLayoutFlexGrow>,
        az_layout_flex_shrink_delete: Symbol<extern fn(_: &mut AzLayoutFlexShrink)>,
        az_layout_flex_shrink_deep_copy: Symbol<extern fn(_: &AzLayoutFlexShrink) -> AzLayoutFlexShrink>,
        az_layout_float_left: Symbol<extern fn() -> AzLayoutFloat>,
        az_layout_float_right: Symbol<extern fn() -> AzLayoutFloat>,
        az_layout_float_delete: Symbol<extern fn(_: &mut AzLayoutFloat)>,
        az_layout_float_deep_copy: Symbol<extern fn(_: &AzLayoutFloat) -> AzLayoutFloat>,
        az_layout_height_delete: Symbol<extern fn(_: &mut AzLayoutHeight)>,
        az_layout_height_deep_copy: Symbol<extern fn(_: &AzLayoutHeight) -> AzLayoutHeight>,
        az_layout_justify_content_start: Symbol<extern fn() -> AzLayoutJustifyContent>,
        az_layout_justify_content_end: Symbol<extern fn() -> AzLayoutJustifyContent>,
        az_layout_justify_content_center: Symbol<extern fn() -> AzLayoutJustifyContent>,
        az_layout_justify_content_space_between: Symbol<extern fn() -> AzLayoutJustifyContent>,
        az_layout_justify_content_space_around: Symbol<extern fn() -> AzLayoutJustifyContent>,
        az_layout_justify_content_space_evenly: Symbol<extern fn() -> AzLayoutJustifyContent>,
        az_layout_justify_content_delete: Symbol<extern fn(_: &mut AzLayoutJustifyContent)>,
        az_layout_justify_content_deep_copy: Symbol<extern fn(_: &AzLayoutJustifyContent) -> AzLayoutJustifyContent>,
        az_layout_left_delete: Symbol<extern fn(_: &mut AzLayoutLeft)>,
        az_layout_left_deep_copy: Symbol<extern fn(_: &AzLayoutLeft) -> AzLayoutLeft>,
        az_layout_margin_bottom_delete: Symbol<extern fn(_: &mut AzLayoutMarginBottom)>,
        az_layout_margin_bottom_deep_copy: Symbol<extern fn(_: &AzLayoutMarginBottom) -> AzLayoutMarginBottom>,
        az_layout_margin_left_delete: Symbol<extern fn(_: &mut AzLayoutMarginLeft)>,
        az_layout_margin_left_deep_copy: Symbol<extern fn(_: &AzLayoutMarginLeft) -> AzLayoutMarginLeft>,
        az_layout_margin_right_delete: Symbol<extern fn(_: &mut AzLayoutMarginRight)>,
        az_layout_margin_right_deep_copy: Symbol<extern fn(_: &AzLayoutMarginRight) -> AzLayoutMarginRight>,
        az_layout_margin_top_delete: Symbol<extern fn(_: &mut AzLayoutMarginTop)>,
        az_layout_margin_top_deep_copy: Symbol<extern fn(_: &AzLayoutMarginTop) -> AzLayoutMarginTop>,
        az_layout_max_height_delete: Symbol<extern fn(_: &mut AzLayoutMaxHeight)>,
        az_layout_max_height_deep_copy: Symbol<extern fn(_: &AzLayoutMaxHeight) -> AzLayoutMaxHeight>,
        az_layout_max_width_delete: Symbol<extern fn(_: &mut AzLayoutMaxWidth)>,
        az_layout_max_width_deep_copy: Symbol<extern fn(_: &AzLayoutMaxWidth) -> AzLayoutMaxWidth>,
        az_layout_min_height_delete: Symbol<extern fn(_: &mut AzLayoutMinHeight)>,
        az_layout_min_height_deep_copy: Symbol<extern fn(_: &AzLayoutMinHeight) -> AzLayoutMinHeight>,
        az_layout_min_width_delete: Symbol<extern fn(_: &mut AzLayoutMinWidth)>,
        az_layout_min_width_deep_copy: Symbol<extern fn(_: &AzLayoutMinWidth) -> AzLayoutMinWidth>,
        az_layout_padding_bottom_delete: Symbol<extern fn(_: &mut AzLayoutPaddingBottom)>,
        az_layout_padding_bottom_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingBottom) -> AzLayoutPaddingBottom>,
        az_layout_padding_left_delete: Symbol<extern fn(_: &mut AzLayoutPaddingLeft)>,
        az_layout_padding_left_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingLeft) -> AzLayoutPaddingLeft>,
        az_layout_padding_right_delete: Symbol<extern fn(_: &mut AzLayoutPaddingRight)>,
        az_layout_padding_right_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingRight) -> AzLayoutPaddingRight>,
        az_layout_padding_top_delete: Symbol<extern fn(_: &mut AzLayoutPaddingTop)>,
        az_layout_padding_top_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingTop) -> AzLayoutPaddingTop>,
        az_layout_position_static: Symbol<extern fn() -> AzLayoutPosition>,
        az_layout_position_relative: Symbol<extern fn() -> AzLayoutPosition>,
        az_layout_position_absolute: Symbol<extern fn() -> AzLayoutPosition>,
        az_layout_position_fixed: Symbol<extern fn() -> AzLayoutPosition>,
        az_layout_position_delete: Symbol<extern fn(_: &mut AzLayoutPosition)>,
        az_layout_position_deep_copy: Symbol<extern fn(_: &AzLayoutPosition) -> AzLayoutPosition>,
        az_layout_right_delete: Symbol<extern fn(_: &mut AzLayoutRight)>,
        az_layout_right_deep_copy: Symbol<extern fn(_: &AzLayoutRight) -> AzLayoutRight>,
        az_layout_top_delete: Symbol<extern fn(_: &mut AzLayoutTop)>,
        az_layout_top_deep_copy: Symbol<extern fn(_: &AzLayoutTop) -> AzLayoutTop>,
        az_layout_width_delete: Symbol<extern fn(_: &mut AzLayoutWidth)>,
        az_layout_width_deep_copy: Symbol<extern fn(_: &AzLayoutWidth) -> AzLayoutWidth>,
        az_layout_wrap_wrap: Symbol<extern fn() -> AzLayoutWrap>,
        az_layout_wrap_no_wrap: Symbol<extern fn() -> AzLayoutWrap>,
        az_layout_wrap_delete: Symbol<extern fn(_: &mut AzLayoutWrap)>,
        az_layout_wrap_deep_copy: Symbol<extern fn(_: &AzLayoutWrap) -> AzLayoutWrap>,
        az_overflow_scroll: Symbol<extern fn() -> AzOverflow>,
        az_overflow_auto: Symbol<extern fn() -> AzOverflow>,
        az_overflow_hidden: Symbol<extern fn() -> AzOverflow>,
        az_overflow_visible: Symbol<extern fn() -> AzOverflow>,
        az_overflow_delete: Symbol<extern fn(_: &mut AzOverflow)>,
        az_overflow_deep_copy: Symbol<extern fn(_: &AzOverflow) -> AzOverflow>,
        az_percentage_value_delete: Symbol<extern fn(_: &mut AzPercentageValue)>,
        az_percentage_value_deep_copy: Symbol<extern fn(_: &AzPercentageValue) -> AzPercentageValue>,
        az_gradient_stop_pre_delete: Symbol<extern fn(_: &mut AzGradientStopPre)>,
        az_gradient_stop_pre_deep_copy: Symbol<extern fn(_: &AzGradientStopPre) -> AzGradientStopPre>,
        az_direction_corner_right: Symbol<extern fn() -> AzDirectionCorner>,
        az_direction_corner_left: Symbol<extern fn() -> AzDirectionCorner>,
        az_direction_corner_top: Symbol<extern fn() -> AzDirectionCorner>,
        az_direction_corner_bottom: Symbol<extern fn() -> AzDirectionCorner>,
        az_direction_corner_top_right: Symbol<extern fn() -> AzDirectionCorner>,
        az_direction_corner_top_left: Symbol<extern fn() -> AzDirectionCorner>,
        az_direction_corner_bottom_right: Symbol<extern fn() -> AzDirectionCorner>,
        az_direction_corner_bottom_left: Symbol<extern fn() -> AzDirectionCorner>,
        az_direction_corner_delete: Symbol<extern fn(_: &mut AzDirectionCorner)>,
        az_direction_corner_deep_copy: Symbol<extern fn(_: &AzDirectionCorner) -> AzDirectionCorner>,
        az_direction_corners_delete: Symbol<extern fn(_: &mut AzDirectionCorners)>,
        az_direction_corners_deep_copy: Symbol<extern fn(_: &AzDirectionCorners) -> AzDirectionCorners>,
        az_direction_angle: Symbol<extern fn(_: AzFloatValue) -> AzDirection>,
        az_direction_from_to: Symbol<extern fn(_: AzDirectionCorners) -> AzDirection>,
        az_direction_delete: Symbol<extern fn(_: &mut AzDirection)>,
        az_direction_deep_copy: Symbol<extern fn(_: &AzDirection) -> AzDirection>,
        az_extend_mode_clamp: Symbol<extern fn() -> AzExtendMode>,
        az_extend_mode_repeat: Symbol<extern fn() -> AzExtendMode>,
        az_extend_mode_delete: Symbol<extern fn(_: &mut AzExtendMode)>,
        az_extend_mode_deep_copy: Symbol<extern fn(_: &AzExtendMode) -> AzExtendMode>,
        az_linear_gradient_delete: Symbol<extern fn(_: &mut AzLinearGradient)>,
        az_linear_gradient_deep_copy: Symbol<extern fn(_: &AzLinearGradient) -> AzLinearGradient>,
        az_shape_ellipse: Symbol<extern fn() -> AzShape>,
        az_shape_circle: Symbol<extern fn() -> AzShape>,
        az_shape_delete: Symbol<extern fn(_: &mut AzShape)>,
        az_shape_deep_copy: Symbol<extern fn(_: &AzShape) -> AzShape>,
        az_radial_gradient_delete: Symbol<extern fn(_: &mut AzRadialGradient)>,
        az_radial_gradient_deep_copy: Symbol<extern fn(_: &AzRadialGradient) -> AzRadialGradient>,
        az_css_image_id_delete: Symbol<extern fn(_: &mut AzCssImageId)>,
        az_css_image_id_deep_copy: Symbol<extern fn(_: &AzCssImageId) -> AzCssImageId>,
        az_style_background_content_linear_gradient: Symbol<extern fn(_: AzLinearGradient) -> AzStyleBackgroundContent>,
        az_style_background_content_radial_gradient: Symbol<extern fn(_: AzRadialGradient) -> AzStyleBackgroundContent>,
        az_style_background_content_image: Symbol<extern fn(_: AzCssImageId) -> AzStyleBackgroundContent>,
        az_style_background_content_color: Symbol<extern fn(_: AzColorU) -> AzStyleBackgroundContent>,
        az_style_background_content_delete: Symbol<extern fn(_: &mut AzStyleBackgroundContent)>,
        az_style_background_content_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundContent) -> AzStyleBackgroundContent>,
        az_background_position_horizontal_left: Symbol<extern fn() -> AzBackgroundPositionHorizontal>,
        az_background_position_horizontal_center: Symbol<extern fn() -> AzBackgroundPositionHorizontal>,
        az_background_position_horizontal_right: Symbol<extern fn() -> AzBackgroundPositionHorizontal>,
        az_background_position_horizontal_exact: Symbol<extern fn(_: AzPixelValue) -> AzBackgroundPositionHorizontal>,
        az_background_position_horizontal_delete: Symbol<extern fn(_: &mut AzBackgroundPositionHorizontal)>,
        az_background_position_horizontal_deep_copy: Symbol<extern fn(_: &AzBackgroundPositionHorizontal) -> AzBackgroundPositionHorizontal>,
        az_background_position_vertical_top: Symbol<extern fn() -> AzBackgroundPositionVertical>,
        az_background_position_vertical_center: Symbol<extern fn() -> AzBackgroundPositionVertical>,
        az_background_position_vertical_bottom: Symbol<extern fn() -> AzBackgroundPositionVertical>,
        az_background_position_vertical_exact: Symbol<extern fn(_: AzPixelValue) -> AzBackgroundPositionVertical>,
        az_background_position_vertical_delete: Symbol<extern fn(_: &mut AzBackgroundPositionVertical)>,
        az_background_position_vertical_deep_copy: Symbol<extern fn(_: &AzBackgroundPositionVertical) -> AzBackgroundPositionVertical>,
        az_style_background_position_delete: Symbol<extern fn(_: &mut AzStyleBackgroundPosition)>,
        az_style_background_position_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundPosition) -> AzStyleBackgroundPosition>,
        az_style_background_repeat_no_repeat: Symbol<extern fn() -> AzStyleBackgroundRepeat>,
        az_style_background_repeat_repeat: Symbol<extern fn() -> AzStyleBackgroundRepeat>,
        az_style_background_repeat_repeat_x: Symbol<extern fn() -> AzStyleBackgroundRepeat>,
        az_style_background_repeat_repeat_y: Symbol<extern fn() -> AzStyleBackgroundRepeat>,
        az_style_background_repeat_delete: Symbol<extern fn(_: &mut AzStyleBackgroundRepeat)>,
        az_style_background_repeat_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundRepeat) -> AzStyleBackgroundRepeat>,
        az_style_background_size_exact_size: Symbol<extern fn(_: AzPixelValue) -> AzStyleBackgroundSize>,
        az_style_background_size_contain: Symbol<extern fn() -> AzStyleBackgroundSize>,
        az_style_background_size_cover: Symbol<extern fn() -> AzStyleBackgroundSize>,
        az_style_background_size_delete: Symbol<extern fn(_: &mut AzStyleBackgroundSize)>,
        az_style_background_size_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundSize) -> AzStyleBackgroundSize>,
        az_style_border_bottom_color_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomColor)>,
        az_style_border_bottom_color_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomColor) -> AzStyleBorderBottomColor>,
        az_style_border_bottom_left_radius_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomLeftRadius)>,
        az_style_border_bottom_left_radius_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadius>,
        az_style_border_bottom_right_radius_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomRightRadius)>,
        az_style_border_bottom_right_radius_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadius>,
        az_border_style_none: Symbol<extern fn() -> AzBorderStyle>,
        az_border_style_solid: Symbol<extern fn() -> AzBorderStyle>,
        az_border_style_double: Symbol<extern fn() -> AzBorderStyle>,
        az_border_style_dotted: Symbol<extern fn() -> AzBorderStyle>,
        az_border_style_dashed: Symbol<extern fn() -> AzBorderStyle>,
        az_border_style_hidden: Symbol<extern fn() -> AzBorderStyle>,
        az_border_style_groove: Symbol<extern fn() -> AzBorderStyle>,
        az_border_style_ridge: Symbol<extern fn() -> AzBorderStyle>,
        az_border_style_inset: Symbol<extern fn() -> AzBorderStyle>,
        az_border_style_outset: Symbol<extern fn() -> AzBorderStyle>,
        az_border_style_delete: Symbol<extern fn(_: &mut AzBorderStyle)>,
        az_border_style_deep_copy: Symbol<extern fn(_: &AzBorderStyle) -> AzBorderStyle>,
        az_style_border_bottom_style_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomStyle)>,
        az_style_border_bottom_style_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomStyle) -> AzStyleBorderBottomStyle>,
        az_style_border_bottom_width_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomWidth)>,
        az_style_border_bottom_width_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomWidth) -> AzStyleBorderBottomWidth>,
        az_style_border_left_color_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftColor)>,
        az_style_border_left_color_deep_copy: Symbol<extern fn(_: &AzStyleBorderLeftColor) -> AzStyleBorderLeftColor>,
        az_style_border_left_style_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftStyle)>,
        az_style_border_left_style_deep_copy: Symbol<extern fn(_: &AzStyleBorderLeftStyle) -> AzStyleBorderLeftStyle>,
        az_style_border_left_width_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftWidth)>,
        az_style_border_left_width_deep_copy: Symbol<extern fn(_: &AzStyleBorderLeftWidth) -> AzStyleBorderLeftWidth>,
        az_style_border_right_color_delete: Symbol<extern fn(_: &mut AzStyleBorderRightColor)>,
        az_style_border_right_color_deep_copy: Symbol<extern fn(_: &AzStyleBorderRightColor) -> AzStyleBorderRightColor>,
        az_style_border_right_style_delete: Symbol<extern fn(_: &mut AzStyleBorderRightStyle)>,
        az_style_border_right_style_deep_copy: Symbol<extern fn(_: &AzStyleBorderRightStyle) -> AzStyleBorderRightStyle>,
        az_style_border_right_width_delete: Symbol<extern fn(_: &mut AzStyleBorderRightWidthPtr)>,
        az_style_border_right_width_shallow_copy: Symbol<extern fn(_: &AzStyleBorderRightWidthPtr) -> AzStyleBorderRightWidthPtr>,
        az_style_border_top_color_delete: Symbol<extern fn(_: &mut AzStyleBorderTopColor)>,
        az_style_border_top_color_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopColor) -> AzStyleBorderTopColor>,
        az_style_border_top_left_radius_delete: Symbol<extern fn(_: &mut AzStyleBorderTopLeftRadius)>,
        az_style_border_top_left_radius_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopLeftRadius) -> AzStyleBorderTopLeftRadius>,
        az_style_border_top_right_radius_delete: Symbol<extern fn(_: &mut AzStyleBorderTopRightRadius)>,
        az_style_border_top_right_radius_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopRightRadius) -> AzStyleBorderTopRightRadius>,
        az_style_border_top_style_delete: Symbol<extern fn(_: &mut AzStyleBorderTopStyle)>,
        az_style_border_top_style_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopStyle) -> AzStyleBorderTopStyle>,
        az_style_border_top_width_delete: Symbol<extern fn(_: &mut AzStyleBorderTopWidth)>,
        az_style_border_top_width_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopWidth) -> AzStyleBorderTopWidth>,
        az_style_cursor_alias: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_all_scroll: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_cell: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_col_resize: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_context_menu: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_copy: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_crosshair: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_default: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_e_resize: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_ew_resize: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_grab: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_grabbing: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_help: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_move: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_n_resize: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_ns_resize: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_nesw_resize: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_nwse_resize: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_pointer: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_progress: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_row_resize: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_s_resize: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_se_resize: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_text: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_unset: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_vertical_text: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_w_resize: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_wait: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_zoom_in: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_zoom_out: Symbol<extern fn() -> AzStyleCursor>,
        az_style_cursor_delete: Symbol<extern fn(_: &mut AzStyleCursor)>,
        az_style_cursor_deep_copy: Symbol<extern fn(_: &AzStyleCursor) -> AzStyleCursor>,
        az_style_font_family_delete: Symbol<extern fn(_: &mut AzStyleFontFamily)>,
        az_style_font_family_deep_copy: Symbol<extern fn(_: &AzStyleFontFamily) -> AzStyleFontFamily>,
        az_style_font_size_delete: Symbol<extern fn(_: &mut AzStyleFontSize)>,
        az_style_font_size_deep_copy: Symbol<extern fn(_: &AzStyleFontSize) -> AzStyleFontSize>,
        az_style_letter_spacing_delete: Symbol<extern fn(_: &mut AzStyleLetterSpacing)>,
        az_style_letter_spacing_deep_copy: Symbol<extern fn(_: &AzStyleLetterSpacing) -> AzStyleLetterSpacing>,
        az_style_line_height_delete: Symbol<extern fn(_: &mut AzStyleLineHeight)>,
        az_style_line_height_deep_copy: Symbol<extern fn(_: &AzStyleLineHeight) -> AzStyleLineHeight>,
        az_style_tab_width_delete: Symbol<extern fn(_: &mut AzStyleTabWidth)>,
        az_style_tab_width_deep_copy: Symbol<extern fn(_: &AzStyleTabWidth) -> AzStyleTabWidth>,
        az_style_text_alignment_horz_left: Symbol<extern fn() -> AzStyleTextAlignmentHorz>,
        az_style_text_alignment_horz_center: Symbol<extern fn() -> AzStyleTextAlignmentHorz>,
        az_style_text_alignment_horz_right: Symbol<extern fn() -> AzStyleTextAlignmentHorz>,
        az_style_text_alignment_horz_delete: Symbol<extern fn(_: &mut AzStyleTextAlignmentHorz)>,
        az_style_text_alignment_horz_deep_copy: Symbol<extern fn(_: &AzStyleTextAlignmentHorz) -> AzStyleTextAlignmentHorz>,
        az_style_text_color_delete: Symbol<extern fn(_: &mut AzStyleTextColor)>,
        az_style_text_color_deep_copy: Symbol<extern fn(_: &AzStyleTextColor) -> AzStyleTextColor>,
        az_style_word_spacing_delete: Symbol<extern fn(_: &mut AzStyleWordSpacing)>,
        az_style_word_spacing_deep_copy: Symbol<extern fn(_: &AzStyleWordSpacing) -> AzStyleWordSpacing>,
        az_box_shadow_pre_display_item_value_auto: Symbol<extern fn() -> AzBoxShadowPreDisplayItemValue>,
        az_box_shadow_pre_display_item_value_none: Symbol<extern fn() -> AzBoxShadowPreDisplayItemValue>,
        az_box_shadow_pre_display_item_value_inherit: Symbol<extern fn() -> AzBoxShadowPreDisplayItemValue>,
        az_box_shadow_pre_display_item_value_initial: Symbol<extern fn() -> AzBoxShadowPreDisplayItemValue>,
        az_box_shadow_pre_display_item_value_exact: Symbol<extern fn(_: AzBoxShadowPreDisplayItem) -> AzBoxShadowPreDisplayItemValue>,
        az_box_shadow_pre_display_item_value_delete: Symbol<extern fn(_: &mut AzBoxShadowPreDisplayItemValue)>,
        az_box_shadow_pre_display_item_value_deep_copy: Symbol<extern fn(_: &AzBoxShadowPreDisplayItemValue) -> AzBoxShadowPreDisplayItemValue>,
        az_layout_align_content_value_auto: Symbol<extern fn() -> AzLayoutAlignContentValue>,
        az_layout_align_content_value_none: Symbol<extern fn() -> AzLayoutAlignContentValue>,
        az_layout_align_content_value_inherit: Symbol<extern fn() -> AzLayoutAlignContentValue>,
        az_layout_align_content_value_initial: Symbol<extern fn() -> AzLayoutAlignContentValue>,
        az_layout_align_content_value_exact: Symbol<extern fn(_: AzLayoutAlignContent) -> AzLayoutAlignContentValue>,
        az_layout_align_content_value_delete: Symbol<extern fn(_: &mut AzLayoutAlignContentValue)>,
        az_layout_align_content_value_deep_copy: Symbol<extern fn(_: &AzLayoutAlignContentValue) -> AzLayoutAlignContentValue>,
        az_layout_align_items_value_auto: Symbol<extern fn() -> AzLayoutAlignItemsValue>,
        az_layout_align_items_value_none: Symbol<extern fn() -> AzLayoutAlignItemsValue>,
        az_layout_align_items_value_inherit: Symbol<extern fn() -> AzLayoutAlignItemsValue>,
        az_layout_align_items_value_initial: Symbol<extern fn() -> AzLayoutAlignItemsValue>,
        az_layout_align_items_value_exact: Symbol<extern fn(_: AzLayoutAlignItems) -> AzLayoutAlignItemsValue>,
        az_layout_align_items_value_delete: Symbol<extern fn(_: &mut AzLayoutAlignItemsValue)>,
        az_layout_align_items_value_deep_copy: Symbol<extern fn(_: &AzLayoutAlignItemsValue) -> AzLayoutAlignItemsValue>,
        az_layout_bottom_value_auto: Symbol<extern fn() -> AzLayoutBottomValue>,
        az_layout_bottom_value_none: Symbol<extern fn() -> AzLayoutBottomValue>,
        az_layout_bottom_value_inherit: Symbol<extern fn() -> AzLayoutBottomValue>,
        az_layout_bottom_value_initial: Symbol<extern fn() -> AzLayoutBottomValue>,
        az_layout_bottom_value_exact: Symbol<extern fn(_: AzLayoutBottom) -> AzLayoutBottomValue>,
        az_layout_bottom_value_delete: Symbol<extern fn(_: &mut AzLayoutBottomValue)>,
        az_layout_bottom_value_deep_copy: Symbol<extern fn(_: &AzLayoutBottomValue) -> AzLayoutBottomValue>,
        az_layout_box_sizing_value_auto: Symbol<extern fn() -> AzLayoutBoxSizingValue>,
        az_layout_box_sizing_value_none: Symbol<extern fn() -> AzLayoutBoxSizingValue>,
        az_layout_box_sizing_value_inherit: Symbol<extern fn() -> AzLayoutBoxSizingValue>,
        az_layout_box_sizing_value_initial: Symbol<extern fn() -> AzLayoutBoxSizingValue>,
        az_layout_box_sizing_value_exact: Symbol<extern fn(_: AzLayoutBoxSizing) -> AzLayoutBoxSizingValue>,
        az_layout_box_sizing_value_delete: Symbol<extern fn(_: &mut AzLayoutBoxSizingValue)>,
        az_layout_box_sizing_value_deep_copy: Symbol<extern fn(_: &AzLayoutBoxSizingValue) -> AzLayoutBoxSizingValue>,
        az_layout_direction_value_auto: Symbol<extern fn() -> AzLayoutDirectionValue>,
        az_layout_direction_value_none: Symbol<extern fn() -> AzLayoutDirectionValue>,
        az_layout_direction_value_inherit: Symbol<extern fn() -> AzLayoutDirectionValue>,
        az_layout_direction_value_initial: Symbol<extern fn() -> AzLayoutDirectionValue>,
        az_layout_direction_value_exact: Symbol<extern fn(_: AzLayoutDirection) -> AzLayoutDirectionValue>,
        az_layout_direction_value_delete: Symbol<extern fn(_: &mut AzLayoutDirectionValue)>,
        az_layout_direction_value_deep_copy: Symbol<extern fn(_: &AzLayoutDirectionValue) -> AzLayoutDirectionValue>,
        az_layout_display_value_auto: Symbol<extern fn() -> AzLayoutDisplayValue>,
        az_layout_display_value_none: Symbol<extern fn() -> AzLayoutDisplayValue>,
        az_layout_display_value_inherit: Symbol<extern fn() -> AzLayoutDisplayValue>,
        az_layout_display_value_initial: Symbol<extern fn() -> AzLayoutDisplayValue>,
        az_layout_display_value_exact: Symbol<extern fn(_: AzLayoutDisplay) -> AzLayoutDisplayValue>,
        az_layout_display_value_delete: Symbol<extern fn(_: &mut AzLayoutDisplayValue)>,
        az_layout_display_value_deep_copy: Symbol<extern fn(_: &AzLayoutDisplayValue) -> AzLayoutDisplayValue>,
        az_layout_flex_grow_value_auto: Symbol<extern fn() -> AzLayoutFlexGrowValue>,
        az_layout_flex_grow_value_none: Symbol<extern fn() -> AzLayoutFlexGrowValue>,
        az_layout_flex_grow_value_inherit: Symbol<extern fn() -> AzLayoutFlexGrowValue>,
        az_layout_flex_grow_value_initial: Symbol<extern fn() -> AzLayoutFlexGrowValue>,
        az_layout_flex_grow_value_exact: Symbol<extern fn(_: AzLayoutFlexGrow) -> AzLayoutFlexGrowValue>,
        az_layout_flex_grow_value_delete: Symbol<extern fn(_: &mut AzLayoutFlexGrowValue)>,
        az_layout_flex_grow_value_deep_copy: Symbol<extern fn(_: &AzLayoutFlexGrowValue) -> AzLayoutFlexGrowValue>,
        az_layout_flex_shrink_value_auto: Symbol<extern fn() -> AzLayoutFlexShrinkValue>,
        az_layout_flex_shrink_value_none: Symbol<extern fn() -> AzLayoutFlexShrinkValue>,
        az_layout_flex_shrink_value_inherit: Symbol<extern fn() -> AzLayoutFlexShrinkValue>,
        az_layout_flex_shrink_value_initial: Symbol<extern fn() -> AzLayoutFlexShrinkValue>,
        az_layout_flex_shrink_value_exact: Symbol<extern fn(_: AzLayoutFlexShrink) -> AzLayoutFlexShrinkValue>,
        az_layout_flex_shrink_value_delete: Symbol<extern fn(_: &mut AzLayoutFlexShrinkValue)>,
        az_layout_flex_shrink_value_deep_copy: Symbol<extern fn(_: &AzLayoutFlexShrinkValue) -> AzLayoutFlexShrinkValue>,
        az_layout_float_value_auto: Symbol<extern fn() -> AzLayoutFloatValue>,
        az_layout_float_value_none: Symbol<extern fn() -> AzLayoutFloatValue>,
        az_layout_float_value_inherit: Symbol<extern fn() -> AzLayoutFloatValue>,
        az_layout_float_value_initial: Symbol<extern fn() -> AzLayoutFloatValue>,
        az_layout_float_value_exact: Symbol<extern fn(_: AzLayoutFloat) -> AzLayoutFloatValue>,
        az_layout_float_value_delete: Symbol<extern fn(_: &mut AzLayoutFloatValue)>,
        az_layout_float_value_deep_copy: Symbol<extern fn(_: &AzLayoutFloatValue) -> AzLayoutFloatValue>,
        az_layout_height_value_auto: Symbol<extern fn() -> AzLayoutHeightValue>,
        az_layout_height_value_none: Symbol<extern fn() -> AzLayoutHeightValue>,
        az_layout_height_value_inherit: Symbol<extern fn() -> AzLayoutHeightValue>,
        az_layout_height_value_initial: Symbol<extern fn() -> AzLayoutHeightValue>,
        az_layout_height_value_exact: Symbol<extern fn(_: AzLayoutHeight) -> AzLayoutHeightValue>,
        az_layout_height_value_delete: Symbol<extern fn(_: &mut AzLayoutHeightValue)>,
        az_layout_height_value_deep_copy: Symbol<extern fn(_: &AzLayoutHeightValue) -> AzLayoutHeightValue>,
        az_layout_justify_content_value_auto: Symbol<extern fn() -> AzLayoutJustifyContentValue>,
        az_layout_justify_content_value_none: Symbol<extern fn() -> AzLayoutJustifyContentValue>,
        az_layout_justify_content_value_inherit: Symbol<extern fn() -> AzLayoutJustifyContentValue>,
        az_layout_justify_content_value_initial: Symbol<extern fn() -> AzLayoutJustifyContentValue>,
        az_layout_justify_content_value_exact: Symbol<extern fn(_: AzLayoutJustifyContent) -> AzLayoutJustifyContentValue>,
        az_layout_justify_content_value_delete: Symbol<extern fn(_: &mut AzLayoutJustifyContentValue)>,
        az_layout_justify_content_value_deep_copy: Symbol<extern fn(_: &AzLayoutJustifyContentValue) -> AzLayoutJustifyContentValue>,
        az_layout_left_value_auto: Symbol<extern fn() -> AzLayoutLeftValue>,
        az_layout_left_value_none: Symbol<extern fn() -> AzLayoutLeftValue>,
        az_layout_left_value_inherit: Symbol<extern fn() -> AzLayoutLeftValue>,
        az_layout_left_value_initial: Symbol<extern fn() -> AzLayoutLeftValue>,
        az_layout_left_value_exact: Symbol<extern fn(_: AzLayoutLeft) -> AzLayoutLeftValue>,
        az_layout_left_value_delete: Symbol<extern fn(_: &mut AzLayoutLeftValue)>,
        az_layout_left_value_deep_copy: Symbol<extern fn(_: &AzLayoutLeftValue) -> AzLayoutLeftValue>,
        az_layout_margin_bottom_value_auto: Symbol<extern fn() -> AzLayoutMarginBottomValue>,
        az_layout_margin_bottom_value_none: Symbol<extern fn() -> AzLayoutMarginBottomValue>,
        az_layout_margin_bottom_value_inherit: Symbol<extern fn() -> AzLayoutMarginBottomValue>,
        az_layout_margin_bottom_value_initial: Symbol<extern fn() -> AzLayoutMarginBottomValue>,
        az_layout_margin_bottom_value_exact: Symbol<extern fn(_: AzLayoutMarginBottom) -> AzLayoutMarginBottomValue>,
        az_layout_margin_bottom_value_delete: Symbol<extern fn(_: &mut AzLayoutMarginBottomValue)>,
        az_layout_margin_bottom_value_deep_copy: Symbol<extern fn(_: &AzLayoutMarginBottomValue) -> AzLayoutMarginBottomValue>,
        az_layout_margin_left_value_auto: Symbol<extern fn() -> AzLayoutMarginLeftValue>,
        az_layout_margin_left_value_none: Symbol<extern fn() -> AzLayoutMarginLeftValue>,
        az_layout_margin_left_value_inherit: Symbol<extern fn() -> AzLayoutMarginLeftValue>,
        az_layout_margin_left_value_initial: Symbol<extern fn() -> AzLayoutMarginLeftValue>,
        az_layout_margin_left_value_exact: Symbol<extern fn(_: AzLayoutMarginLeft) -> AzLayoutMarginLeftValue>,
        az_layout_margin_left_value_delete: Symbol<extern fn(_: &mut AzLayoutMarginLeftValue)>,
        az_layout_margin_left_value_deep_copy: Symbol<extern fn(_: &AzLayoutMarginLeftValue) -> AzLayoutMarginLeftValue>,
        az_layout_margin_right_value_auto: Symbol<extern fn() -> AzLayoutMarginRightValue>,
        az_layout_margin_right_value_none: Symbol<extern fn() -> AzLayoutMarginRightValue>,
        az_layout_margin_right_value_inherit: Symbol<extern fn() -> AzLayoutMarginRightValue>,
        az_layout_margin_right_value_initial: Symbol<extern fn() -> AzLayoutMarginRightValue>,
        az_layout_margin_right_value_exact: Symbol<extern fn(_: AzLayoutMarginRight) -> AzLayoutMarginRightValue>,
        az_layout_margin_right_value_delete: Symbol<extern fn(_: &mut AzLayoutMarginRightValue)>,
        az_layout_margin_right_value_deep_copy: Symbol<extern fn(_: &AzLayoutMarginRightValue) -> AzLayoutMarginRightValue>,
        az_layout_margin_top_value_auto: Symbol<extern fn() -> AzLayoutMarginTopValue>,
        az_layout_margin_top_value_none: Symbol<extern fn() -> AzLayoutMarginTopValue>,
        az_layout_margin_top_value_inherit: Symbol<extern fn() -> AzLayoutMarginTopValue>,
        az_layout_margin_top_value_initial: Symbol<extern fn() -> AzLayoutMarginTopValue>,
        az_layout_margin_top_value_exact: Symbol<extern fn(_: AzLayoutMarginTop) -> AzLayoutMarginTopValue>,
        az_layout_margin_top_value_delete: Symbol<extern fn(_: &mut AzLayoutMarginTopValue)>,
        az_layout_margin_top_value_deep_copy: Symbol<extern fn(_: &AzLayoutMarginTopValue) -> AzLayoutMarginTopValue>,
        az_layout_max_height_value_auto: Symbol<extern fn() -> AzLayoutMaxHeightValue>,
        az_layout_max_height_value_none: Symbol<extern fn() -> AzLayoutMaxHeightValue>,
        az_layout_max_height_value_inherit: Symbol<extern fn() -> AzLayoutMaxHeightValue>,
        az_layout_max_height_value_initial: Symbol<extern fn() -> AzLayoutMaxHeightValue>,
        az_layout_max_height_value_exact: Symbol<extern fn(_: AzLayoutMaxHeight) -> AzLayoutMaxHeightValue>,
        az_layout_max_height_value_delete: Symbol<extern fn(_: &mut AzLayoutMaxHeightValue)>,
        az_layout_max_height_value_deep_copy: Symbol<extern fn(_: &AzLayoutMaxHeightValue) -> AzLayoutMaxHeightValue>,
        az_layout_max_width_value_auto: Symbol<extern fn() -> AzLayoutMaxWidthValue>,
        az_layout_max_width_value_none: Symbol<extern fn() -> AzLayoutMaxWidthValue>,
        az_layout_max_width_value_inherit: Symbol<extern fn() -> AzLayoutMaxWidthValue>,
        az_layout_max_width_value_initial: Symbol<extern fn() -> AzLayoutMaxWidthValue>,
        az_layout_max_width_value_exact: Symbol<extern fn(_: AzLayoutMaxWidth) -> AzLayoutMaxWidthValue>,
        az_layout_max_width_value_delete: Symbol<extern fn(_: &mut AzLayoutMaxWidthValue)>,
        az_layout_max_width_value_deep_copy: Symbol<extern fn(_: &AzLayoutMaxWidthValue) -> AzLayoutMaxWidthValue>,
        az_layout_min_height_value_auto: Symbol<extern fn() -> AzLayoutMinHeightValue>,
        az_layout_min_height_value_none: Symbol<extern fn() -> AzLayoutMinHeightValue>,
        az_layout_min_height_value_inherit: Symbol<extern fn() -> AzLayoutMinHeightValue>,
        az_layout_min_height_value_initial: Symbol<extern fn() -> AzLayoutMinHeightValue>,
        az_layout_min_height_value_exact: Symbol<extern fn(_: AzLayoutMinHeight) -> AzLayoutMinHeightValue>,
        az_layout_min_height_value_delete: Symbol<extern fn(_: &mut AzLayoutMinHeightValue)>,
        az_layout_min_height_value_deep_copy: Symbol<extern fn(_: &AzLayoutMinHeightValue) -> AzLayoutMinHeightValue>,
        az_layout_min_width_value_auto: Symbol<extern fn() -> AzLayoutMinWidthValue>,
        az_layout_min_width_value_none: Symbol<extern fn() -> AzLayoutMinWidthValue>,
        az_layout_min_width_value_inherit: Symbol<extern fn() -> AzLayoutMinWidthValue>,
        az_layout_min_width_value_initial: Symbol<extern fn() -> AzLayoutMinWidthValue>,
        az_layout_min_width_value_exact: Symbol<extern fn(_: AzLayoutMinWidth) -> AzLayoutMinWidthValue>,
        az_layout_min_width_value_delete: Symbol<extern fn(_: &mut AzLayoutMinWidthValue)>,
        az_layout_min_width_value_deep_copy: Symbol<extern fn(_: &AzLayoutMinWidthValue) -> AzLayoutMinWidthValue>,
        az_layout_padding_bottom_value_auto: Symbol<extern fn() -> AzLayoutPaddingBottomValue>,
        az_layout_padding_bottom_value_none: Symbol<extern fn() -> AzLayoutPaddingBottomValue>,
        az_layout_padding_bottom_value_inherit: Symbol<extern fn() -> AzLayoutPaddingBottomValue>,
        az_layout_padding_bottom_value_initial: Symbol<extern fn() -> AzLayoutPaddingBottomValue>,
        az_layout_padding_bottom_value_exact: Symbol<extern fn(_: AzLayoutPaddingBottom) -> AzLayoutPaddingBottomValue>,
        az_layout_padding_bottom_value_delete: Symbol<extern fn(_: &mut AzLayoutPaddingBottomValue)>,
        az_layout_padding_bottom_value_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingBottomValue) -> AzLayoutPaddingBottomValue>,
        az_layout_padding_left_value_auto: Symbol<extern fn() -> AzLayoutPaddingLeftValue>,
        az_layout_padding_left_value_none: Symbol<extern fn() -> AzLayoutPaddingLeftValue>,
        az_layout_padding_left_value_inherit: Symbol<extern fn() -> AzLayoutPaddingLeftValue>,
        az_layout_padding_left_value_initial: Symbol<extern fn() -> AzLayoutPaddingLeftValue>,
        az_layout_padding_left_value_exact: Symbol<extern fn(_: AzLayoutPaddingLeft) -> AzLayoutPaddingLeftValue>,
        az_layout_padding_left_value_delete: Symbol<extern fn(_: &mut AzLayoutPaddingLeftValue)>,
        az_layout_padding_left_value_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingLeftValue) -> AzLayoutPaddingLeftValue>,
        az_layout_padding_right_value_auto: Symbol<extern fn() -> AzLayoutPaddingRightValue>,
        az_layout_padding_right_value_none: Symbol<extern fn() -> AzLayoutPaddingRightValue>,
        az_layout_padding_right_value_inherit: Symbol<extern fn() -> AzLayoutPaddingRightValue>,
        az_layout_padding_right_value_initial: Symbol<extern fn() -> AzLayoutPaddingRightValue>,
        az_layout_padding_right_value_exact: Symbol<extern fn(_: AzLayoutPaddingRight) -> AzLayoutPaddingRightValue>,
        az_layout_padding_right_value_delete: Symbol<extern fn(_: &mut AzLayoutPaddingRightValue)>,
        az_layout_padding_right_value_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingRightValue) -> AzLayoutPaddingRightValue>,
        az_layout_padding_top_value_auto: Symbol<extern fn() -> AzLayoutPaddingTopValue>,
        az_layout_padding_top_value_none: Symbol<extern fn() -> AzLayoutPaddingTopValue>,
        az_layout_padding_top_value_inherit: Symbol<extern fn() -> AzLayoutPaddingTopValue>,
        az_layout_padding_top_value_initial: Symbol<extern fn() -> AzLayoutPaddingTopValue>,
        az_layout_padding_top_value_exact: Symbol<extern fn(_: AzLayoutPaddingTop) -> AzLayoutPaddingTopValue>,
        az_layout_padding_top_value_delete: Symbol<extern fn(_: &mut AzLayoutPaddingTopValue)>,
        az_layout_padding_top_value_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingTopValue) -> AzLayoutPaddingTopValue>,
        az_layout_position_value_auto: Symbol<extern fn() -> AzLayoutPositionValue>,
        az_layout_position_value_none: Symbol<extern fn() -> AzLayoutPositionValue>,
        az_layout_position_value_inherit: Symbol<extern fn() -> AzLayoutPositionValue>,
        az_layout_position_value_initial: Symbol<extern fn() -> AzLayoutPositionValue>,
        az_layout_position_value_exact: Symbol<extern fn(_: AzLayoutPosition) -> AzLayoutPositionValue>,
        az_layout_position_value_delete: Symbol<extern fn(_: &mut AzLayoutPositionValue)>,
        az_layout_position_value_deep_copy: Symbol<extern fn(_: &AzLayoutPositionValue) -> AzLayoutPositionValue>,
        az_layout_right_value_auto: Symbol<extern fn() -> AzLayoutRightValue>,
        az_layout_right_value_none: Symbol<extern fn() -> AzLayoutRightValue>,
        az_layout_right_value_inherit: Symbol<extern fn() -> AzLayoutRightValue>,
        az_layout_right_value_initial: Symbol<extern fn() -> AzLayoutRightValue>,
        az_layout_right_value_exact: Symbol<extern fn(_: AzLayoutRight) -> AzLayoutRightValue>,
        az_layout_right_value_delete: Symbol<extern fn(_: &mut AzLayoutRightValue)>,
        az_layout_right_value_deep_copy: Symbol<extern fn(_: &AzLayoutRightValue) -> AzLayoutRightValue>,
        az_layout_top_value_auto: Symbol<extern fn() -> AzLayoutTopValue>,
        az_layout_top_value_none: Symbol<extern fn() -> AzLayoutTopValue>,
        az_layout_top_value_inherit: Symbol<extern fn() -> AzLayoutTopValue>,
        az_layout_top_value_initial: Symbol<extern fn() -> AzLayoutTopValue>,
        az_layout_top_value_exact: Symbol<extern fn(_: AzLayoutTop) -> AzLayoutTopValue>,
        az_layout_top_value_delete: Symbol<extern fn(_: &mut AzLayoutTopValue)>,
        az_layout_top_value_deep_copy: Symbol<extern fn(_: &AzLayoutTopValue) -> AzLayoutTopValue>,
        az_layout_width_value_auto: Symbol<extern fn() -> AzLayoutWidthValue>,
        az_layout_width_value_none: Symbol<extern fn() -> AzLayoutWidthValue>,
        az_layout_width_value_inherit: Symbol<extern fn() -> AzLayoutWidthValue>,
        az_layout_width_value_initial: Symbol<extern fn() -> AzLayoutWidthValue>,
        az_layout_width_value_exact: Symbol<extern fn(_: AzLayoutWidth) -> AzLayoutWidthValue>,
        az_layout_width_value_delete: Symbol<extern fn(_: &mut AzLayoutWidthValue)>,
        az_layout_width_value_deep_copy: Symbol<extern fn(_: &AzLayoutWidthValue) -> AzLayoutWidthValue>,
        az_layout_wrap_value_auto: Symbol<extern fn() -> AzLayoutWrapValue>,
        az_layout_wrap_value_none: Symbol<extern fn() -> AzLayoutWrapValue>,
        az_layout_wrap_value_inherit: Symbol<extern fn() -> AzLayoutWrapValue>,
        az_layout_wrap_value_initial: Symbol<extern fn() -> AzLayoutWrapValue>,
        az_layout_wrap_value_exact: Symbol<extern fn(_: AzLayoutWrap) -> AzLayoutWrapValue>,
        az_layout_wrap_value_delete: Symbol<extern fn(_: &mut AzLayoutWrapValue)>,
        az_layout_wrap_value_deep_copy: Symbol<extern fn(_: &AzLayoutWrapValue) -> AzLayoutWrapValue>,
        az_overflow_value_auto: Symbol<extern fn() -> AzOverflowValue>,
        az_overflow_value_none: Symbol<extern fn() -> AzOverflowValue>,
        az_overflow_value_inherit: Symbol<extern fn() -> AzOverflowValue>,
        az_overflow_value_initial: Symbol<extern fn() -> AzOverflowValue>,
        az_overflow_value_exact: Symbol<extern fn(_: AzOverflow) -> AzOverflowValue>,
        az_overflow_value_delete: Symbol<extern fn(_: &mut AzOverflowValue)>,
        az_overflow_value_deep_copy: Symbol<extern fn(_: &AzOverflowValue) -> AzOverflowValue>,
        az_style_background_content_value_auto: Symbol<extern fn() -> AzStyleBackgroundContentValue>,
        az_style_background_content_value_none: Symbol<extern fn() -> AzStyleBackgroundContentValue>,
        az_style_background_content_value_inherit: Symbol<extern fn() -> AzStyleBackgroundContentValue>,
        az_style_background_content_value_initial: Symbol<extern fn() -> AzStyleBackgroundContentValue>,
        az_style_background_content_value_exact: Symbol<extern fn(_: AzStyleBackgroundContent) -> AzStyleBackgroundContentValue>,
        az_style_background_content_value_delete: Symbol<extern fn(_: &mut AzStyleBackgroundContentValue)>,
        az_style_background_content_value_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue>,
        az_style_background_position_value_auto: Symbol<extern fn() -> AzStyleBackgroundPositionValue>,
        az_style_background_position_value_none: Symbol<extern fn() -> AzStyleBackgroundPositionValue>,
        az_style_background_position_value_inherit: Symbol<extern fn() -> AzStyleBackgroundPositionValue>,
        az_style_background_position_value_initial: Symbol<extern fn() -> AzStyleBackgroundPositionValue>,
        az_style_background_position_value_exact: Symbol<extern fn(_: AzStyleBackgroundPosition) -> AzStyleBackgroundPositionValue>,
        az_style_background_position_value_delete: Symbol<extern fn(_: &mut AzStyleBackgroundPositionValue)>,
        az_style_background_position_value_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundPositionValue) -> AzStyleBackgroundPositionValue>,
        az_style_background_repeat_value_auto: Symbol<extern fn() -> AzStyleBackgroundRepeatValue>,
        az_style_background_repeat_value_none: Symbol<extern fn() -> AzStyleBackgroundRepeatValue>,
        az_style_background_repeat_value_inherit: Symbol<extern fn() -> AzStyleBackgroundRepeatValue>,
        az_style_background_repeat_value_initial: Symbol<extern fn() -> AzStyleBackgroundRepeatValue>,
        az_style_background_repeat_value_exact: Symbol<extern fn(_: AzStyleBackgroundRepeat) -> AzStyleBackgroundRepeatValue>,
        az_style_background_repeat_value_delete: Symbol<extern fn(_: &mut AzStyleBackgroundRepeatValue)>,
        az_style_background_repeat_value_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundRepeatValue) -> AzStyleBackgroundRepeatValue>,
        az_style_background_size_value_auto: Symbol<extern fn() -> AzStyleBackgroundSizeValue>,
        az_style_background_size_value_none: Symbol<extern fn() -> AzStyleBackgroundSizeValue>,
        az_style_background_size_value_inherit: Symbol<extern fn() -> AzStyleBackgroundSizeValue>,
        az_style_background_size_value_initial: Symbol<extern fn() -> AzStyleBackgroundSizeValue>,
        az_style_background_size_value_exact: Symbol<extern fn(_: AzStyleBackgroundSize) -> AzStyleBackgroundSizeValue>,
        az_style_background_size_value_delete: Symbol<extern fn(_: &mut AzStyleBackgroundSizeValue)>,
        az_style_background_size_value_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundSizeValue) -> AzStyleBackgroundSizeValue>,
        az_style_border_bottom_color_value_auto: Symbol<extern fn() -> AzStyleBorderBottomColorValue>,
        az_style_border_bottom_color_value_none: Symbol<extern fn() -> AzStyleBorderBottomColorValue>,
        az_style_border_bottom_color_value_inherit: Symbol<extern fn() -> AzStyleBorderBottomColorValue>,
        az_style_border_bottom_color_value_initial: Symbol<extern fn() -> AzStyleBorderBottomColorValue>,
        az_style_border_bottom_color_value_exact: Symbol<extern fn(_: AzStyleBorderBottomColor) -> AzStyleBorderBottomColorValue>,
        az_style_border_bottom_color_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomColorValue)>,
        az_style_border_bottom_color_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomColorValue) -> AzStyleBorderBottomColorValue>,
        az_style_border_bottom_left_radius_value_auto: Symbol<extern fn() -> AzStyleBorderBottomLeftRadiusValue>,
        az_style_border_bottom_left_radius_value_none: Symbol<extern fn() -> AzStyleBorderBottomLeftRadiusValue>,
        az_style_border_bottom_left_radius_value_inherit: Symbol<extern fn() -> AzStyleBorderBottomLeftRadiusValue>,
        az_style_border_bottom_left_radius_value_initial: Symbol<extern fn() -> AzStyleBorderBottomLeftRadiusValue>,
        az_style_border_bottom_left_radius_value_exact: Symbol<extern fn(_: AzStyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadiusValue>,
        az_style_border_bottom_left_radius_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomLeftRadiusValue)>,
        az_style_border_bottom_left_radius_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomLeftRadiusValue) -> AzStyleBorderBottomLeftRadiusValue>,
        az_style_border_bottom_right_radius_value_auto: Symbol<extern fn() -> AzStyleBorderBottomRightRadiusValue>,
        az_style_border_bottom_right_radius_value_none: Symbol<extern fn() -> AzStyleBorderBottomRightRadiusValue>,
        az_style_border_bottom_right_radius_value_inherit: Symbol<extern fn() -> AzStyleBorderBottomRightRadiusValue>,
        az_style_border_bottom_right_radius_value_initial: Symbol<extern fn() -> AzStyleBorderBottomRightRadiusValue>,
        az_style_border_bottom_right_radius_value_exact: Symbol<extern fn(_: AzStyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadiusValue>,
        az_style_border_bottom_right_radius_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomRightRadiusValue)>,
        az_style_border_bottom_right_radius_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomRightRadiusValue) -> AzStyleBorderBottomRightRadiusValue>,
        az_style_border_bottom_style_value_auto: Symbol<extern fn() -> AzStyleBorderBottomStyleValue>,
        az_style_border_bottom_style_value_none: Symbol<extern fn() -> AzStyleBorderBottomStyleValue>,
        az_style_border_bottom_style_value_inherit: Symbol<extern fn() -> AzStyleBorderBottomStyleValue>,
        az_style_border_bottom_style_value_initial: Symbol<extern fn() -> AzStyleBorderBottomStyleValue>,
        az_style_border_bottom_style_value_exact: Symbol<extern fn(_: AzStyleBorderBottomStyle) -> AzStyleBorderBottomStyleValue>,
        az_style_border_bottom_style_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomStyleValue)>,
        az_style_border_bottom_style_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomStyleValue) -> AzStyleBorderBottomStyleValue>,
        az_style_border_bottom_width_value_auto: Symbol<extern fn() -> AzStyleBorderBottomWidthValue>,
        az_style_border_bottom_width_value_none: Symbol<extern fn() -> AzStyleBorderBottomWidthValue>,
        az_style_border_bottom_width_value_inherit: Symbol<extern fn() -> AzStyleBorderBottomWidthValue>,
        az_style_border_bottom_width_value_initial: Symbol<extern fn() -> AzStyleBorderBottomWidthValue>,
        az_style_border_bottom_width_value_exact: Symbol<extern fn(_: AzStyleBorderBottomWidth) -> AzStyleBorderBottomWidthValue>,
        az_style_border_bottom_width_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomWidthValue)>,
        az_style_border_bottom_width_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomWidthValue) -> AzStyleBorderBottomWidthValue>,
        az_style_border_left_color_value_auto: Symbol<extern fn() -> AzStyleBorderLeftColorValue>,
        az_style_border_left_color_value_none: Symbol<extern fn() -> AzStyleBorderLeftColorValue>,
        az_style_border_left_color_value_inherit: Symbol<extern fn() -> AzStyleBorderLeftColorValue>,
        az_style_border_left_color_value_initial: Symbol<extern fn() -> AzStyleBorderLeftColorValue>,
        az_style_border_left_color_value_exact: Symbol<extern fn(_: AzStyleBorderLeftColor) -> AzStyleBorderLeftColorValue>,
        az_style_border_left_color_value_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftColorValue)>,
        az_style_border_left_color_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderLeftColorValue) -> AzStyleBorderLeftColorValue>,
        az_style_border_left_style_value_auto: Symbol<extern fn() -> AzStyleBorderLeftStyleValue>,
        az_style_border_left_style_value_none: Symbol<extern fn() -> AzStyleBorderLeftStyleValue>,
        az_style_border_left_style_value_inherit: Symbol<extern fn() -> AzStyleBorderLeftStyleValue>,
        az_style_border_left_style_value_initial: Symbol<extern fn() -> AzStyleBorderLeftStyleValue>,
        az_style_border_left_style_value_exact: Symbol<extern fn(_: AzStyleBorderLeftStyle) -> AzStyleBorderLeftStyleValue>,
        az_style_border_left_style_value_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftStyleValue)>,
        az_style_border_left_style_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderLeftStyleValue) -> AzStyleBorderLeftStyleValue>,
        az_style_border_left_width_value_auto: Symbol<extern fn() -> AzStyleBorderLeftWidthValue>,
        az_style_border_left_width_value_none: Symbol<extern fn() -> AzStyleBorderLeftWidthValue>,
        az_style_border_left_width_value_inherit: Symbol<extern fn() -> AzStyleBorderLeftWidthValue>,
        az_style_border_left_width_value_initial: Symbol<extern fn() -> AzStyleBorderLeftWidthValue>,
        az_style_border_left_width_value_exact: Symbol<extern fn(_: AzStyleBorderLeftWidth) -> AzStyleBorderLeftWidthValue>,
        az_style_border_left_width_value_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftWidthValue)>,
        az_style_border_left_width_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderLeftWidthValue) -> AzStyleBorderLeftWidthValue>,
        az_style_border_right_color_value_auto: Symbol<extern fn() -> AzStyleBorderRightColorValue>,
        az_style_border_right_color_value_none: Symbol<extern fn() -> AzStyleBorderRightColorValue>,
        az_style_border_right_color_value_inherit: Symbol<extern fn() -> AzStyleBorderRightColorValue>,
        az_style_border_right_color_value_initial: Symbol<extern fn() -> AzStyleBorderRightColorValue>,
        az_style_border_right_color_value_exact: Symbol<extern fn(_: AzStyleBorderRightColor) -> AzStyleBorderRightColorValue>,
        az_style_border_right_color_value_delete: Symbol<extern fn(_: &mut AzStyleBorderRightColorValue)>,
        az_style_border_right_color_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderRightColorValue) -> AzStyleBorderRightColorValue>,
        az_style_border_right_style_value_auto: Symbol<extern fn() -> AzStyleBorderRightStyleValue>,
        az_style_border_right_style_value_none: Symbol<extern fn() -> AzStyleBorderRightStyleValue>,
        az_style_border_right_style_value_inherit: Symbol<extern fn() -> AzStyleBorderRightStyleValue>,
        az_style_border_right_style_value_initial: Symbol<extern fn() -> AzStyleBorderRightStyleValue>,
        az_style_border_right_style_value_exact: Symbol<extern fn(_: AzStyleBorderRightStyle) -> AzStyleBorderRightStyleValue>,
        az_style_border_right_style_value_delete: Symbol<extern fn(_: &mut AzStyleBorderRightStyleValue)>,
        az_style_border_right_style_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderRightStyleValue) -> AzStyleBorderRightStyleValue>,
        az_style_border_right_width_value_auto: Symbol<extern fn() -> AzStyleBorderRightWidthValue>,
        az_style_border_right_width_value_none: Symbol<extern fn() -> AzStyleBorderRightWidthValue>,
        az_style_border_right_width_value_inherit: Symbol<extern fn() -> AzStyleBorderRightWidthValue>,
        az_style_border_right_width_value_initial: Symbol<extern fn() -> AzStyleBorderRightWidthValue>,
        az_style_border_right_width_value_exact: Symbol<extern fn(_: AzStyleBorderRightWidthPtr) -> AzStyleBorderRightWidthValue>,
        az_style_border_right_width_value_delete: Symbol<extern fn(_: &mut AzStyleBorderRightWidthValue)>,
        az_style_border_right_width_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderRightWidthValue) -> AzStyleBorderRightWidthValue>,
        az_style_border_top_color_value_auto: Symbol<extern fn() -> AzStyleBorderTopColorValue>,
        az_style_border_top_color_value_none: Symbol<extern fn() -> AzStyleBorderTopColorValue>,
        az_style_border_top_color_value_inherit: Symbol<extern fn() -> AzStyleBorderTopColorValue>,
        az_style_border_top_color_value_initial: Symbol<extern fn() -> AzStyleBorderTopColorValue>,
        az_style_border_top_color_value_exact: Symbol<extern fn(_: AzStyleBorderTopColor) -> AzStyleBorderTopColorValue>,
        az_style_border_top_color_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopColorValue)>,
        az_style_border_top_color_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopColorValue) -> AzStyleBorderTopColorValue>,
        az_style_border_top_left_radius_value_auto: Symbol<extern fn() -> AzStyleBorderTopLeftRadiusValue>,
        az_style_border_top_left_radius_value_none: Symbol<extern fn() -> AzStyleBorderTopLeftRadiusValue>,
        az_style_border_top_left_radius_value_inherit: Symbol<extern fn() -> AzStyleBorderTopLeftRadiusValue>,
        az_style_border_top_left_radius_value_initial: Symbol<extern fn() -> AzStyleBorderTopLeftRadiusValue>,
        az_style_border_top_left_radius_value_exact: Symbol<extern fn(_: AzStyleBorderTopLeftRadius) -> AzStyleBorderTopLeftRadiusValue>,
        az_style_border_top_left_radius_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopLeftRadiusValue)>,
        az_style_border_top_left_radius_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopLeftRadiusValue) -> AzStyleBorderTopLeftRadiusValue>,
        az_style_border_top_right_radius_value_auto: Symbol<extern fn() -> AzStyleBorderTopRightRadiusValue>,
        az_style_border_top_right_radius_value_none: Symbol<extern fn() -> AzStyleBorderTopRightRadiusValue>,
        az_style_border_top_right_radius_value_inherit: Symbol<extern fn() -> AzStyleBorderTopRightRadiusValue>,
        az_style_border_top_right_radius_value_initial: Symbol<extern fn() -> AzStyleBorderTopRightRadiusValue>,
        az_style_border_top_right_radius_value_exact: Symbol<extern fn(_: AzStyleBorderTopRightRadius) -> AzStyleBorderTopRightRadiusValue>,
        az_style_border_top_right_radius_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopRightRadiusValue)>,
        az_style_border_top_right_radius_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopRightRadiusValue) -> AzStyleBorderTopRightRadiusValue>,
        az_style_border_top_style_value_auto: Symbol<extern fn() -> AzStyleBorderTopStyleValue>,
        az_style_border_top_style_value_none: Symbol<extern fn() -> AzStyleBorderTopStyleValue>,
        az_style_border_top_style_value_inherit: Symbol<extern fn() -> AzStyleBorderTopStyleValue>,
        az_style_border_top_style_value_initial: Symbol<extern fn() -> AzStyleBorderTopStyleValue>,
        az_style_border_top_style_value_exact: Symbol<extern fn(_: AzStyleBorderTopStyle) -> AzStyleBorderTopStyleValue>,
        az_style_border_top_style_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopStyleValue)>,
        az_style_border_top_style_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopStyleValue) -> AzStyleBorderTopStyleValue>,
        az_style_border_top_width_value_auto: Symbol<extern fn() -> AzStyleBorderTopWidthValue>,
        az_style_border_top_width_value_none: Symbol<extern fn() -> AzStyleBorderTopWidthValue>,
        az_style_border_top_width_value_inherit: Symbol<extern fn() -> AzStyleBorderTopWidthValue>,
        az_style_border_top_width_value_initial: Symbol<extern fn() -> AzStyleBorderTopWidthValue>,
        az_style_border_top_width_value_exact: Symbol<extern fn(_: AzStyleBorderTopWidth) -> AzStyleBorderTopWidthValue>,
        az_style_border_top_width_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopWidthValue)>,
        az_style_border_top_width_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopWidthValue) -> AzStyleBorderTopWidthValue>,
        az_style_cursor_value_auto: Symbol<extern fn() -> AzStyleCursorValue>,
        az_style_cursor_value_none: Symbol<extern fn() -> AzStyleCursorValue>,
        az_style_cursor_value_inherit: Symbol<extern fn() -> AzStyleCursorValue>,
        az_style_cursor_value_initial: Symbol<extern fn() -> AzStyleCursorValue>,
        az_style_cursor_value_exact: Symbol<extern fn(_: AzStyleCursor) -> AzStyleCursorValue>,
        az_style_cursor_value_delete: Symbol<extern fn(_: &mut AzStyleCursorValue)>,
        az_style_cursor_value_deep_copy: Symbol<extern fn(_: &AzStyleCursorValue) -> AzStyleCursorValue>,
        az_style_font_family_value_auto: Symbol<extern fn() -> AzStyleFontFamilyValue>,
        az_style_font_family_value_none: Symbol<extern fn() -> AzStyleFontFamilyValue>,
        az_style_font_family_value_inherit: Symbol<extern fn() -> AzStyleFontFamilyValue>,
        az_style_font_family_value_initial: Symbol<extern fn() -> AzStyleFontFamilyValue>,
        az_style_font_family_value_exact: Symbol<extern fn(_: AzStyleFontFamily) -> AzStyleFontFamilyValue>,
        az_style_font_family_value_delete: Symbol<extern fn(_: &mut AzStyleFontFamilyValue)>,
        az_style_font_family_value_deep_copy: Symbol<extern fn(_: &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue>,
        az_style_font_size_value_auto: Symbol<extern fn() -> AzStyleFontSizeValue>,
        az_style_font_size_value_none: Symbol<extern fn() -> AzStyleFontSizeValue>,
        az_style_font_size_value_inherit: Symbol<extern fn() -> AzStyleFontSizeValue>,
        az_style_font_size_value_initial: Symbol<extern fn() -> AzStyleFontSizeValue>,
        az_style_font_size_value_exact: Symbol<extern fn(_: AzStyleFontSize) -> AzStyleFontSizeValue>,
        az_style_font_size_value_delete: Symbol<extern fn(_: &mut AzStyleFontSizeValue)>,
        az_style_font_size_value_deep_copy: Symbol<extern fn(_: &AzStyleFontSizeValue) -> AzStyleFontSizeValue>,
        az_style_letter_spacing_value_auto: Symbol<extern fn() -> AzStyleLetterSpacingValue>,
        az_style_letter_spacing_value_none: Symbol<extern fn() -> AzStyleLetterSpacingValue>,
        az_style_letter_spacing_value_inherit: Symbol<extern fn() -> AzStyleLetterSpacingValue>,
        az_style_letter_spacing_value_initial: Symbol<extern fn() -> AzStyleLetterSpacingValue>,
        az_style_letter_spacing_value_exact: Symbol<extern fn(_: AzStyleLetterSpacing) -> AzStyleLetterSpacingValue>,
        az_style_letter_spacing_value_delete: Symbol<extern fn(_: &mut AzStyleLetterSpacingValue)>,
        az_style_letter_spacing_value_deep_copy: Symbol<extern fn(_: &AzStyleLetterSpacingValue) -> AzStyleLetterSpacingValue>,
        az_style_line_height_value_auto: Symbol<extern fn() -> AzStyleLineHeightValue>,
        az_style_line_height_value_none: Symbol<extern fn() -> AzStyleLineHeightValue>,
        az_style_line_height_value_inherit: Symbol<extern fn() -> AzStyleLineHeightValue>,
        az_style_line_height_value_initial: Symbol<extern fn() -> AzStyleLineHeightValue>,
        az_style_line_height_value_exact: Symbol<extern fn(_: AzStyleLineHeight) -> AzStyleLineHeightValue>,
        az_style_line_height_value_delete: Symbol<extern fn(_: &mut AzStyleLineHeightValue)>,
        az_style_line_height_value_deep_copy: Symbol<extern fn(_: &AzStyleLineHeightValue) -> AzStyleLineHeightValue>,
        az_style_tab_width_value_auto: Symbol<extern fn() -> AzStyleTabWidthValue>,
        az_style_tab_width_value_none: Symbol<extern fn() -> AzStyleTabWidthValue>,
        az_style_tab_width_value_inherit: Symbol<extern fn() -> AzStyleTabWidthValue>,
        az_style_tab_width_value_initial: Symbol<extern fn() -> AzStyleTabWidthValue>,
        az_style_tab_width_value_exact: Symbol<extern fn(_: AzStyleTabWidth) -> AzStyleTabWidthValue>,
        az_style_tab_width_value_delete: Symbol<extern fn(_: &mut AzStyleTabWidthValue)>,
        az_style_tab_width_value_deep_copy: Symbol<extern fn(_: &AzStyleTabWidthValue) -> AzStyleTabWidthValue>,
        az_style_text_alignment_horz_value_auto: Symbol<extern fn() -> AzStyleTextAlignmentHorzValue>,
        az_style_text_alignment_horz_value_none: Symbol<extern fn() -> AzStyleTextAlignmentHorzValue>,
        az_style_text_alignment_horz_value_inherit: Symbol<extern fn() -> AzStyleTextAlignmentHorzValue>,
        az_style_text_alignment_horz_value_initial: Symbol<extern fn() -> AzStyleTextAlignmentHorzValue>,
        az_style_text_alignment_horz_value_exact: Symbol<extern fn(_: AzStyleTextAlignmentHorz) -> AzStyleTextAlignmentHorzValue>,
        az_style_text_alignment_horz_value_delete: Symbol<extern fn(_: &mut AzStyleTextAlignmentHorzValue)>,
        az_style_text_alignment_horz_value_deep_copy: Symbol<extern fn(_: &AzStyleTextAlignmentHorzValue) -> AzStyleTextAlignmentHorzValue>,
        az_style_text_color_value_auto: Symbol<extern fn() -> AzStyleTextColorValue>,
        az_style_text_color_value_none: Symbol<extern fn() -> AzStyleTextColorValue>,
        az_style_text_color_value_inherit: Symbol<extern fn() -> AzStyleTextColorValue>,
        az_style_text_color_value_initial: Symbol<extern fn() -> AzStyleTextColorValue>,
        az_style_text_color_value_exact: Symbol<extern fn(_: AzStyleTextColor) -> AzStyleTextColorValue>,
        az_style_text_color_value_delete: Symbol<extern fn(_: &mut AzStyleTextColorValue)>,
        az_style_text_color_value_deep_copy: Symbol<extern fn(_: &AzStyleTextColorValue) -> AzStyleTextColorValue>,
        az_style_word_spacing_value_auto: Symbol<extern fn() -> AzStyleWordSpacingValue>,
        az_style_word_spacing_value_none: Symbol<extern fn() -> AzStyleWordSpacingValue>,
        az_style_word_spacing_value_inherit: Symbol<extern fn() -> AzStyleWordSpacingValue>,
        az_style_word_spacing_value_initial: Symbol<extern fn() -> AzStyleWordSpacingValue>,
        az_style_word_spacing_value_exact: Symbol<extern fn(_: AzStyleWordSpacing) -> AzStyleWordSpacingValue>,
        az_style_word_spacing_value_delete: Symbol<extern fn(_: &mut AzStyleWordSpacingValue)>,
        az_style_word_spacing_value_deep_copy: Symbol<extern fn(_: &AzStyleWordSpacingValue) -> AzStyleWordSpacingValue>,
        az_css_property_text_color: Symbol<extern fn(_: AzStyleTextColorValue) -> AzCssProperty>,
        az_css_property_font_size: Symbol<extern fn(_: AzStyleFontSizeValue) -> AzCssProperty>,
        az_css_property_font_family: Symbol<extern fn(_: AzStyleFontFamilyValue) -> AzCssProperty>,
        az_css_property_text_align: Symbol<extern fn(_: AzStyleTextAlignmentHorzValue) -> AzCssProperty>,
        az_css_property_letter_spacing: Symbol<extern fn(_: AzStyleLetterSpacingValue) -> AzCssProperty>,
        az_css_property_line_height: Symbol<extern fn(_: AzStyleLineHeightValue) -> AzCssProperty>,
        az_css_property_word_spacing: Symbol<extern fn(_: AzStyleWordSpacingValue) -> AzCssProperty>,
        az_css_property_tab_width: Symbol<extern fn(_: AzStyleTabWidthValue) -> AzCssProperty>,
        az_css_property_cursor: Symbol<extern fn(_: AzStyleCursorValue) -> AzCssProperty>,
        az_css_property_display: Symbol<extern fn(_: AzLayoutDisplayValue) -> AzCssProperty>,
        az_css_property_float: Symbol<extern fn(_: AzLayoutFloatValue) -> AzCssProperty>,
        az_css_property_box_sizing: Symbol<extern fn(_: AzLayoutBoxSizingValue) -> AzCssProperty>,
        az_css_property_width: Symbol<extern fn(_: AzLayoutWidthValue) -> AzCssProperty>,
        az_css_property_height: Symbol<extern fn(_: AzLayoutHeightValue) -> AzCssProperty>,
        az_css_property_min_width: Symbol<extern fn(_: AzLayoutMinWidthValue) -> AzCssProperty>,
        az_css_property_min_height: Symbol<extern fn(_: AzLayoutMinHeightValue) -> AzCssProperty>,
        az_css_property_max_width: Symbol<extern fn(_: AzLayoutMaxWidthValue) -> AzCssProperty>,
        az_css_property_max_height: Symbol<extern fn(_: AzLayoutMaxHeightValue) -> AzCssProperty>,
        az_css_property_position: Symbol<extern fn(_: AzLayoutPositionValue) -> AzCssProperty>,
        az_css_property_top: Symbol<extern fn(_: AzLayoutTopValue) -> AzCssProperty>,
        az_css_property_right: Symbol<extern fn(_: AzLayoutRightValue) -> AzCssProperty>,
        az_css_property_left: Symbol<extern fn(_: AzLayoutLeftValue) -> AzCssProperty>,
        az_css_property_bottom: Symbol<extern fn(_: AzLayoutBottomValue) -> AzCssProperty>,
        az_css_property_flex_wrap: Symbol<extern fn(_: AzLayoutWrapValue) -> AzCssProperty>,
        az_css_property_flex_direction: Symbol<extern fn(_: AzLayoutDirectionValue) -> AzCssProperty>,
        az_css_property_flex_grow: Symbol<extern fn(_: AzLayoutFlexGrowValue) -> AzCssProperty>,
        az_css_property_flex_shrink: Symbol<extern fn(_: AzLayoutFlexShrinkValue) -> AzCssProperty>,
        az_css_property_justify_content: Symbol<extern fn(_: AzLayoutJustifyContentValue) -> AzCssProperty>,
        az_css_property_align_items: Symbol<extern fn(_: AzLayoutAlignItemsValue) -> AzCssProperty>,
        az_css_property_align_content: Symbol<extern fn(_: AzLayoutAlignContentValue) -> AzCssProperty>,
        az_css_property_background_content: Symbol<extern fn(_: AzStyleBackgroundContentValue) -> AzCssProperty>,
        az_css_property_background_position: Symbol<extern fn(_: AzStyleBackgroundPositionValue) -> AzCssProperty>,
        az_css_property_background_size: Symbol<extern fn(_: AzStyleBackgroundSizeValue) -> AzCssProperty>,
        az_css_property_background_repeat: Symbol<extern fn(_: AzStyleBackgroundRepeatValue) -> AzCssProperty>,
        az_css_property_overflow_x: Symbol<extern fn(_: AzOverflowValue) -> AzCssProperty>,
        az_css_property_overflow_y: Symbol<extern fn(_: AzOverflowValue) -> AzCssProperty>,
        az_css_property_padding_top: Symbol<extern fn(_: AzLayoutPaddingTopValue) -> AzCssProperty>,
        az_css_property_padding_left: Symbol<extern fn(_: AzLayoutPaddingLeftValue) -> AzCssProperty>,
        az_css_property_padding_right: Symbol<extern fn(_: AzLayoutPaddingRightValue) -> AzCssProperty>,
        az_css_property_padding_bottom: Symbol<extern fn(_: AzLayoutPaddingBottomValue) -> AzCssProperty>,
        az_css_property_margin_top: Symbol<extern fn(_: AzLayoutMarginTopValue) -> AzCssProperty>,
        az_css_property_margin_left: Symbol<extern fn(_: AzLayoutMarginLeftValue) -> AzCssProperty>,
        az_css_property_margin_right: Symbol<extern fn(_: AzLayoutMarginRightValue) -> AzCssProperty>,
        az_css_property_margin_bottom: Symbol<extern fn(_: AzLayoutMarginBottomValue) -> AzCssProperty>,
        az_css_property_border_top_left_radius: Symbol<extern fn(_: AzStyleBorderTopLeftRadiusValue) -> AzCssProperty>,
        az_css_property_border_top_right_radius: Symbol<extern fn(_: AzStyleBorderTopRightRadiusValue) -> AzCssProperty>,
        az_css_property_border_bottom_left_radius: Symbol<extern fn(_: AzStyleBorderBottomLeftRadiusValue) -> AzCssProperty>,
        az_css_property_border_bottom_right_radius: Symbol<extern fn(_: AzStyleBorderBottomRightRadiusValue) -> AzCssProperty>,
        az_css_property_border_top_color: Symbol<extern fn(_: AzStyleBorderTopColorValue) -> AzCssProperty>,
        az_css_property_border_right_color: Symbol<extern fn(_: AzStyleBorderRightColorValue) -> AzCssProperty>,
        az_css_property_border_left_color: Symbol<extern fn(_: AzStyleBorderLeftColorValue) -> AzCssProperty>,
        az_css_property_border_bottom_color: Symbol<extern fn(_: AzStyleBorderBottomColorValue) -> AzCssProperty>,
        az_css_property_border_top_style: Symbol<extern fn(_: AzStyleBorderTopStyleValue) -> AzCssProperty>,
        az_css_property_border_right_style: Symbol<extern fn(_: AzStyleBorderRightStyleValue) -> AzCssProperty>,
        az_css_property_border_left_style: Symbol<extern fn(_: AzStyleBorderLeftStyleValue) -> AzCssProperty>,
        az_css_property_border_bottom_style: Symbol<extern fn(_: AzStyleBorderBottomStyleValue) -> AzCssProperty>,
        az_css_property_border_top_width: Symbol<extern fn(_: AzStyleBorderTopWidthValue) -> AzCssProperty>,
        az_css_property_border_right_width: Symbol<extern fn(_: AzStyleBorderRightWidthValue) -> AzCssProperty>,
        az_css_property_border_left_width: Symbol<extern fn(_: AzStyleBorderLeftWidthValue) -> AzCssProperty>,
        az_css_property_border_bottom_width: Symbol<extern fn(_: AzStyleBorderBottomWidthValue) -> AzCssProperty>,
        az_css_property_box_shadow_left: Symbol<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>,
        az_css_property_box_shadow_right: Symbol<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>,
        az_css_property_box_shadow_top: Symbol<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>,
        az_css_property_box_shadow_bottom: Symbol<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>,
        az_css_property_delete: Symbol<extern fn(_: &mut AzCssProperty)>,
        az_css_property_deep_copy: Symbol<extern fn(_: &AzCssProperty) -> AzCssProperty>,
        az_dom_div: Symbol<extern fn() -> AzDomPtr>,
        az_dom_body: Symbol<extern fn() -> AzDomPtr>,
        az_dom_label: Symbol<extern fn(_: AzString) -> AzDomPtr>,
        az_dom_text: Symbol<extern fn(_: AzTextId) -> AzDomPtr>,
        az_dom_image: Symbol<extern fn(_: AzImageId) -> AzDomPtr>,
        az_dom_gl_texture: Symbol<extern fn(_: AzGlCallback) -> AzDomPtr>,
        az_dom_iframe_callback: Symbol<extern fn(_: AzIFrameCallback) -> AzDomPtr>,
        az_dom_add_id: Symbol<extern fn(_: AzString)>,
        az_dom_with_id: Symbol<extern fn(_: AzString) -> AzDomPtr>,
        az_dom_set_ids: Symbol<extern fn(_: AzStringVec)>,
        az_dom_with_ids: Symbol<extern fn(_: AzStringVec) -> AzDomPtr>,
        az_dom_add_class: Symbol<extern fn(_: AzString)>,
        az_dom_with_class: Symbol<extern fn(_: AzString) -> AzDomPtr>,
        az_dom_set_classes: Symbol<extern fn(_: AzStringVec)>,
        az_dom_with_classes: Symbol<extern fn(_: AzStringVec) -> AzDomPtr>,
        az_dom_add_callback: Symbol<extern fn(_: AzCallback)>,
        az_dom_with_callback: Symbol<extern fn(_: AzCallback) -> AzDomPtr>,
        az_dom_add_css_override: Symbol<extern fn(_: AzCssProperty)>,
        az_dom_with_css_override: Symbol<extern fn(_: AzCssProperty) -> AzDomPtr>,
        az_dom_set_is_draggable: Symbol<extern fn(_: bool)>,
        az_dom_is_draggable: Symbol<extern fn(_: bool) -> AzDomPtr>,
        az_dom_set_tab_index: Symbol<extern fn(_: AzTabIndex)>,
        az_dom_with_tab_index: Symbol<extern fn(_: AzTabIndex) -> AzDomPtr>,
        az_dom_add_child: Symbol<extern fn(_: AzDomPtr)>,
        az_dom_with_child: Symbol<extern fn(_: AzDomPtr) -> AzDomPtr>,
        az_dom_has_id: Symbol<extern fn(_: AzString) -> bool>,
        az_dom_has_class: Symbol<extern fn(_: AzString) -> bool>,
        az_dom_get_html_string: Symbol<extern fn(_: &mut AzDomPtr) -> AzString>,
        az_dom_delete: Symbol<extern fn(_: &mut AzDomPtr)>,
        az_dom_shallow_copy: Symbol<extern fn(_: &AzDomPtr) -> AzDomPtr>,
        az_event_filter_hover: Symbol<extern fn(_: AzHoverEventFilter) -> AzEventFilter>,
        az_event_filter_not: Symbol<extern fn(_: AzNotEventFilter) -> AzEventFilter>,
        az_event_filter_focus: Symbol<extern fn(_: AzFocusEventFilter) -> AzEventFilter>,
        az_event_filter_window: Symbol<extern fn(_: AzWindowEventFilter) -> AzEventFilter>,
        az_event_filter_delete: Symbol<extern fn(_: &mut AzEventFilter)>,
        az_event_filter_deep_copy: Symbol<extern fn(_: &AzEventFilter) -> AzEventFilter>,
        az_hover_event_filter_mouse_over: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_mouse_down: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_left_mouse_down: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_right_mouse_down: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_middle_mouse_down: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_mouse_up: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_left_mouse_up: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_right_mouse_up: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_middle_mouse_up: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_mouse_enter: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_mouse_leave: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_scroll: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_scroll_start: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_scroll_end: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_text_input: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_virtual_key_down: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_virtual_key_up: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_hovered_file: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_dropped_file: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_hovered_file_cancelled: Symbol<extern fn() -> AzHoverEventFilter>,
        az_hover_event_filter_delete: Symbol<extern fn(_: &mut AzHoverEventFilter)>,
        az_hover_event_filter_deep_copy: Symbol<extern fn(_: &AzHoverEventFilter) -> AzHoverEventFilter>,
        az_focus_event_filter_mouse_over: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_mouse_down: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_left_mouse_down: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_right_mouse_down: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_middle_mouse_down: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_mouse_up: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_left_mouse_up: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_right_mouse_up: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_middle_mouse_up: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_mouse_enter: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_mouse_leave: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_scroll: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_scroll_start: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_scroll_end: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_text_input: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_virtual_key_down: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_virtual_key_up: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_focus_received: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_focus_lost: Symbol<extern fn() -> AzFocusEventFilter>,
        az_focus_event_filter_delete: Symbol<extern fn(_: &mut AzFocusEventFilter)>,
        az_focus_event_filter_deep_copy: Symbol<extern fn(_: &AzFocusEventFilter) -> AzFocusEventFilter>,
        az_not_event_filter_hover: Symbol<extern fn(_: AzHoverEventFilter) -> AzNotEventFilter>,
        az_not_event_filter_focus: Symbol<extern fn(_: AzFocusEventFilter) -> AzNotEventFilter>,
        az_not_event_filter_delete: Symbol<extern fn(_: &mut AzNotEventFilter)>,
        az_not_event_filter_deep_copy: Symbol<extern fn(_: &AzNotEventFilter) -> AzNotEventFilter>,
        az_window_event_filter_mouse_over: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_mouse_down: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_left_mouse_down: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_right_mouse_down: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_middle_mouse_down: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_mouse_up: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_left_mouse_up: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_right_mouse_up: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_middle_mouse_up: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_mouse_enter: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_mouse_leave: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_scroll: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_scroll_start: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_scroll_end: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_text_input: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_virtual_key_down: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_virtual_key_up: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_hovered_file: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_dropped_file: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_hovered_file_cancelled: Symbol<extern fn() -> AzWindowEventFilter>,
        az_window_event_filter_delete: Symbol<extern fn(_: &mut AzWindowEventFilter)>,
        az_window_event_filter_deep_copy: Symbol<extern fn(_: &AzWindowEventFilter) -> AzWindowEventFilter>,
        az_tab_index_auto: Symbol<extern fn() -> AzTabIndex>,
        az_tab_index_override_in_parent: Symbol<extern fn(_: usize) -> AzTabIndex>,
        az_tab_index_no_keyboard_focus: Symbol<extern fn() -> AzTabIndex>,
        az_tab_index_delete: Symbol<extern fn(_: &mut AzTabIndex)>,
        az_tab_index_deep_copy: Symbol<extern fn(_: &AzTabIndex) -> AzTabIndex>,
        az_text_id_new: Symbol<extern fn() -> AzTextId>,
        az_text_id_delete: Symbol<extern fn(_: &mut AzTextId)>,
        az_text_id_deep_copy: Symbol<extern fn(_: &AzTextId) -> AzTextId>,
        az_image_id_new: Symbol<extern fn() -> AzImageId>,
        az_image_id_delete: Symbol<extern fn(_: &mut AzImageId)>,
        az_image_id_deep_copy: Symbol<extern fn(_: &AzImageId) -> AzImageId>,
        az_font_id_new: Symbol<extern fn() -> AzFontId>,
        az_font_id_delete: Symbol<extern fn(_: &mut AzFontId)>,
        az_font_id_deep_copy: Symbol<extern fn(_: &AzFontId) -> AzFontId>,
        az_image_source_embedded: Symbol<extern fn(_: AzU8Vec) -> AzImageSource>,
        az_image_source_file: Symbol<extern fn(_: AzPathBufPtr) -> AzImageSource>,
        az_image_source_raw: Symbol<extern fn(_: AzRawImagePtr) -> AzImageSource>,
        az_image_source_delete: Symbol<extern fn(_: &mut AzImageSource)>,
        az_image_source_deep_copy: Symbol<extern fn(_: &AzImageSource) -> AzImageSource>,
        az_font_source_embedded: Symbol<extern fn(_: AzU8Vec) -> AzFontSource>,
        az_font_source_file: Symbol<extern fn(_: AzPathBufPtr) -> AzFontSource>,
        az_font_source_system: Symbol<extern fn(_: AzString) -> AzFontSource>,
        az_font_source_delete: Symbol<extern fn(_: &mut AzFontSource)>,
        az_font_source_deep_copy: Symbol<extern fn(_: &AzFontSource) -> AzFontSource>,
        az_raw_image_new: Symbol<extern fn(_: AzRawImageFormat) -> AzRawImagePtr>,
        az_raw_image_delete: Symbol<extern fn(_: &mut AzRawImagePtr)>,
        az_raw_image_shallow_copy: Symbol<extern fn(_: &AzRawImagePtr) -> AzRawImagePtr>,
        az_raw_image_format_r8: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_r16: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_rg16: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_bgra8: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_rgbaf32: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_rg8: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_rgbai32: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_rgba8: Symbol<extern fn() -> AzRawImageFormat>,
        az_raw_image_format_delete: Symbol<extern fn(_: &mut AzRawImageFormat)>,
        az_raw_image_format_deep_copy: Symbol<extern fn(_: &AzRawImageFormat) -> AzRawImageFormat>,
        az_window_create_options_new: Symbol<extern fn(_: AzCssPtr) -> AzWindowCreateOptionsPtr>,
        az_window_create_options_delete: Symbol<extern fn(_: &mut AzWindowCreateOptionsPtr)>,
        az_window_create_options_shallow_copy: Symbol<extern fn(_: &AzWindowCreateOptionsPtr) -> AzWindowCreateOptionsPtr>,
    }

    pub fn initialize_library(path: &str) -> Option<AzulDll> {
        let lib = Library::new(path).ok()?;
        let az_string_from_utf8_unchecked = unsafe { lib.get::<extern fn(_: usize) -> AzString>(b"az_string_from_utf8_unchecked").ok()? };
        let az_string_from_utf8_lossy = unsafe { lib.get::<extern fn(_: usize) -> AzString>(b"az_string_from_utf8_lossy").ok()? };
        let az_string_into_bytes = unsafe { lib.get::<extern fn(_: AzString) -> AzU8Vec>(b"az_string_into_bytes").ok()? };
        let az_string_delete = unsafe { lib.get::<extern fn(_: &mut AzString)>(b"az_string_delete").ok()? };
        let az_string_deep_copy = unsafe { lib.get::<extern fn(_: &AzString) -> AzString>(b"az_string_deep_copy").ok()? };
        let az_u8_vec_copy_from = unsafe { lib.get::<extern fn(_: usize) -> AzU8Vec>(b"az_u8_vec_copy_from").ok()? };
        let az_u8_vec_as_ptr = unsafe { lib.get::<extern fn(_: &AzU8Vec) -> *const u8>(b"az_u8_vec_as_ptr").ok()? };
        let az_u8_vec_len = unsafe { lib.get::<extern fn(_: &AzU8Vec) -> usize>(b"az_u8_vec_len").ok()? };
        let az_u8_vec_delete = unsafe { lib.get::<extern fn(_: &mut AzU8Vec)>(b"az_u8_vec_delete").ok()? };
        let az_u8_vec_deep_copy = unsafe { lib.get::<extern fn(_: &AzU8Vec) -> AzU8Vec>(b"az_u8_vec_deep_copy").ok()? };
        let az_string_vec_copy_from = unsafe { lib.get::<extern fn(_: usize) -> AzStringVec>(b"az_string_vec_copy_from").ok()? };
        let az_string_vec_delete = unsafe { lib.get::<extern fn(_: &mut AzStringVec)>(b"az_string_vec_delete").ok()? };
        let az_string_vec_deep_copy = unsafe { lib.get::<extern fn(_: &AzStringVec) -> AzStringVec>(b"az_string_vec_deep_copy").ok()? };
        let az_gradient_stop_pre_vec_copy_from = unsafe { lib.get::<extern fn(_: usize) -> AzGradientStopPreVec>(b"az_gradient_stop_pre_vec_copy_from").ok()? };
        let az_gradient_stop_pre_vec_delete = unsafe { lib.get::<extern fn(_: &mut AzGradientStopPreVec)>(b"az_gradient_stop_pre_vec_delete").ok()? };
        let az_gradient_stop_pre_vec_deep_copy = unsafe { lib.get::<extern fn(_: &AzGradientStopPreVec) -> AzGradientStopPreVec>(b"az_gradient_stop_pre_vec_deep_copy").ok()? };
        let az_path_buf_new = unsafe { lib.get::<extern fn(_: AzString) -> AzPathBufPtr>(b"az_path_buf_new").ok()? };
        let az_path_buf_delete = unsafe { lib.get::<extern fn(_: &mut AzPathBufPtr)>(b"az_path_buf_delete").ok()? };
        let az_path_buf_shallow_copy = unsafe { lib.get::<extern fn(_: &AzPathBufPtr) -> AzPathBufPtr>(b"az_path_buf_shallow_copy").ok()? };
        let az_option_percentage_value_delete = unsafe { lib.get::<extern fn(_: &mut AzOptionPercentageValuePtr)>(b"az_option_percentage_value_delete").ok()? };
        let az_option_percentage_value_shallow_copy = unsafe { lib.get::<extern fn(_: &AzOptionPercentageValuePtr) -> AzOptionPercentageValuePtr>(b"az_option_percentage_value_shallow_copy").ok()? };
        let az_app_config_default = unsafe { lib.get::<extern fn() -> AzAppConfigPtr>(b"az_app_config_default").ok()? };
        let az_app_config_delete = unsafe { lib.get::<extern fn(_: &mut AzAppConfigPtr)>(b"az_app_config_delete").ok()? };
        let az_app_config_shallow_copy = unsafe { lib.get::<extern fn(_: &AzAppConfigPtr) -> AzAppConfigPtr>(b"az_app_config_shallow_copy").ok()? };
        let az_app_new = unsafe { lib.get::<extern fn(_: AzLayoutCallback) -> AzAppPtr>(b"az_app_new").ok()? };
        let az_app_run = unsafe { lib.get::<extern fn(_: AzWindowCreateOptionsPtr)>(b"az_app_run").ok()? };
        let az_app_delete = unsafe { lib.get::<extern fn(_: &mut AzAppPtr)>(b"az_app_delete").ok()? };
        let az_app_shallow_copy = unsafe { lib.get::<extern fn(_: &AzAppPtr) -> AzAppPtr>(b"az_app_shallow_copy").ok()? };
        let az_callback_info_delete = unsafe { lib.get::<extern fn(_: &mut AzCallbackInfoPtr)>(b"az_callback_info_delete").ok()? };
        let az_callback_info_shallow_copy = unsafe { lib.get::<extern fn(_: &AzCallbackInfoPtr) -> AzCallbackInfoPtr>(b"az_callback_info_shallow_copy").ok()? };
        let az_i_frame_callback_info_delete = unsafe { lib.get::<extern fn(_: &mut AzIFrameCallbackInfoPtr)>(b"az_i_frame_callback_info_delete").ok()? };
        let az_i_frame_callback_info_shallow_copy = unsafe { lib.get::<extern fn(_: &AzIFrameCallbackInfoPtr) -> AzIFrameCallbackInfoPtr>(b"az_i_frame_callback_info_shallow_copy").ok()? };
        let az_i_frame_callback_return_delete = unsafe { lib.get::<extern fn(_: &mut AzIFrameCallbackReturnPtr)>(b"az_i_frame_callback_return_delete").ok()? };
        let az_i_frame_callback_return_shallow_copy = unsafe { lib.get::<extern fn(_: &AzIFrameCallbackReturnPtr) -> AzIFrameCallbackReturnPtr>(b"az_i_frame_callback_return_shallow_copy").ok()? };
        let az_gl_callback_info_delete = unsafe { lib.get::<extern fn(_: &mut AzGlCallbackInfoPtr)>(b"az_gl_callback_info_delete").ok()? };
        let az_gl_callback_info_shallow_copy = unsafe { lib.get::<extern fn(_: &AzGlCallbackInfoPtr) -> AzGlCallbackInfoPtr>(b"az_gl_callback_info_shallow_copy").ok()? };
        let az_gl_callback_return_delete = unsafe { lib.get::<extern fn(_: &mut AzGlCallbackReturnPtr)>(b"az_gl_callback_return_delete").ok()? };
        let az_gl_callback_return_shallow_copy = unsafe { lib.get::<extern fn(_: &AzGlCallbackReturnPtr) -> AzGlCallbackReturnPtr>(b"az_gl_callback_return_shallow_copy").ok()? };
        let az_layout_info_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutInfoPtr)>(b"az_layout_info_delete").ok()? };
        let az_layout_info_shallow_copy = unsafe { lib.get::<extern fn(_: &AzLayoutInfoPtr) -> AzLayoutInfoPtr>(b"az_layout_info_shallow_copy").ok()? };
        let az_css_native = unsafe { lib.get::<extern fn() -> AzCssPtr>(b"az_css_native").ok()? };
        let az_css_empty = unsafe { lib.get::<extern fn() -> AzCssPtr>(b"az_css_empty").ok()? };
        let az_css_from_string = unsafe { lib.get::<extern fn(_: AzString) -> AzCssPtr>(b"az_css_from_string").ok()? };
        let az_css_override_native = unsafe { lib.get::<extern fn(_: AzString) -> AzCssPtr>(b"az_css_override_native").ok()? };
        let az_css_delete = unsafe { lib.get::<extern fn(_: &mut AzCssPtr)>(b"az_css_delete").ok()? };
        let az_css_shallow_copy = unsafe { lib.get::<extern fn(_: &AzCssPtr) -> AzCssPtr>(b"az_css_shallow_copy").ok()? };
        let az_css_hot_reloader_new = unsafe { lib.get::<extern fn(_: u64) -> AzCssHotReloaderPtr>(b"az_css_hot_reloader_new").ok()? };
        let az_css_hot_reloader_override_native = unsafe { lib.get::<extern fn(_: u64) -> AzCssHotReloaderPtr>(b"az_css_hot_reloader_override_native").ok()? };
        let az_css_hot_reloader_delete = unsafe { lib.get::<extern fn(_: &mut AzCssHotReloaderPtr)>(b"az_css_hot_reloader_delete").ok()? };
        let az_css_hot_reloader_shallow_copy = unsafe { lib.get::<extern fn(_: &AzCssHotReloaderPtr) -> AzCssHotReloaderPtr>(b"az_css_hot_reloader_shallow_copy").ok()? };
        let az_color_u_delete = unsafe { lib.get::<extern fn(_: &mut AzColorU)>(b"az_color_u_delete").ok()? };
        let az_color_u_deep_copy = unsafe { lib.get::<extern fn(_: &AzColorU) -> AzColorU>(b"az_color_u_deep_copy").ok()? };
        let az_size_metric_px = unsafe { lib.get::<extern fn() -> AzSizeMetric>(b"az_size_metric_px").ok()? };
        let az_size_metric_pt = unsafe { lib.get::<extern fn() -> AzSizeMetric>(b"az_size_metric_pt").ok()? };
        let az_size_metric_em = unsafe { lib.get::<extern fn() -> AzSizeMetric>(b"az_size_metric_em").ok()? };
        let az_size_metric_percent = unsafe { lib.get::<extern fn() -> AzSizeMetric>(b"az_size_metric_percent").ok()? };
        let az_size_metric_delete = unsafe { lib.get::<extern fn(_: &mut AzSizeMetric)>(b"az_size_metric_delete").ok()? };
        let az_size_metric_deep_copy = unsafe { lib.get::<extern fn(_: &AzSizeMetric) -> AzSizeMetric>(b"az_size_metric_deep_copy").ok()? };
        let az_float_value_delete = unsafe { lib.get::<extern fn(_: &mut AzFloatValue)>(b"az_float_value_delete").ok()? };
        let az_float_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzFloatValue) -> AzFloatValue>(b"az_float_value_deep_copy").ok()? };
        let az_pixel_value_delete = unsafe { lib.get::<extern fn(_: &mut AzPixelValue)>(b"az_pixel_value_delete").ok()? };
        let az_pixel_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzPixelValue) -> AzPixelValue>(b"az_pixel_value_deep_copy").ok()? };
        let az_pixel_value_no_percent_delete = unsafe { lib.get::<extern fn(_: &mut AzPixelValueNoPercent)>(b"az_pixel_value_no_percent_delete").ok()? };
        let az_pixel_value_no_percent_deep_copy = unsafe { lib.get::<extern fn(_: &AzPixelValueNoPercent) -> AzPixelValueNoPercent>(b"az_pixel_value_no_percent_deep_copy").ok()? };
        let az_box_shadow_clip_mode_outset = unsafe { lib.get::<extern fn() -> AzBoxShadowClipMode>(b"az_box_shadow_clip_mode_outset").ok()? };
        let az_box_shadow_clip_mode_inset = unsafe { lib.get::<extern fn() -> AzBoxShadowClipMode>(b"az_box_shadow_clip_mode_inset").ok()? };
        let az_box_shadow_clip_mode_delete = unsafe { lib.get::<extern fn(_: &mut AzBoxShadowClipMode)>(b"az_box_shadow_clip_mode_delete").ok()? };
        let az_box_shadow_clip_mode_deep_copy = unsafe { lib.get::<extern fn(_: &AzBoxShadowClipMode) -> AzBoxShadowClipMode>(b"az_box_shadow_clip_mode_deep_copy").ok()? };
        let az_box_shadow_pre_display_item_delete = unsafe { lib.get::<extern fn(_: &mut AzBoxShadowPreDisplayItem)>(b"az_box_shadow_pre_display_item_delete").ok()? };
        let az_box_shadow_pre_display_item_deep_copy = unsafe { lib.get::<extern fn(_: &AzBoxShadowPreDisplayItem) -> AzBoxShadowPreDisplayItem>(b"az_box_shadow_pre_display_item_deep_copy").ok()? };
        let az_layout_align_content_stretch = unsafe { lib.get::<extern fn() -> AzLayoutAlignContent>(b"az_layout_align_content_stretch").ok()? };
        let az_layout_align_content_center = unsafe { lib.get::<extern fn() -> AzLayoutAlignContent>(b"az_layout_align_content_center").ok()? };
        let az_layout_align_content_start = unsafe { lib.get::<extern fn() -> AzLayoutAlignContent>(b"az_layout_align_content_start").ok()? };
        let az_layout_align_content_end = unsafe { lib.get::<extern fn() -> AzLayoutAlignContent>(b"az_layout_align_content_end").ok()? };
        let az_layout_align_content_space_between = unsafe { lib.get::<extern fn() -> AzLayoutAlignContent>(b"az_layout_align_content_space_between").ok()? };
        let az_layout_align_content_space_around = unsafe { lib.get::<extern fn() -> AzLayoutAlignContent>(b"az_layout_align_content_space_around").ok()? };
        let az_layout_align_content_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutAlignContent)>(b"az_layout_align_content_delete").ok()? };
        let az_layout_align_content_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutAlignContent) -> AzLayoutAlignContent>(b"az_layout_align_content_deep_copy").ok()? };
        let az_layout_align_items_stretch = unsafe { lib.get::<extern fn() -> AzLayoutAlignItems>(b"az_layout_align_items_stretch").ok()? };
        let az_layout_align_items_center = unsafe { lib.get::<extern fn() -> AzLayoutAlignItems>(b"az_layout_align_items_center").ok()? };
        let az_layout_align_items_start = unsafe { lib.get::<extern fn() -> AzLayoutAlignItems>(b"az_layout_align_items_start").ok()? };
        let az_layout_align_items_end = unsafe { lib.get::<extern fn() -> AzLayoutAlignItems>(b"az_layout_align_items_end").ok()? };
        let az_layout_align_items_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutAlignItems)>(b"az_layout_align_items_delete").ok()? };
        let az_layout_align_items_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutAlignItems) -> AzLayoutAlignItems>(b"az_layout_align_items_deep_copy").ok()? };
        let az_layout_bottom_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutBottom)>(b"az_layout_bottom_delete").ok()? };
        let az_layout_bottom_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutBottom) -> AzLayoutBottom>(b"az_layout_bottom_deep_copy").ok()? };
        let az_layout_box_sizing_content_box = unsafe { lib.get::<extern fn() -> AzLayoutBoxSizing>(b"az_layout_box_sizing_content_box").ok()? };
        let az_layout_box_sizing_border_box = unsafe { lib.get::<extern fn() -> AzLayoutBoxSizing>(b"az_layout_box_sizing_border_box").ok()? };
        let az_layout_box_sizing_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutBoxSizing)>(b"az_layout_box_sizing_delete").ok()? };
        let az_layout_box_sizing_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutBoxSizing) -> AzLayoutBoxSizing>(b"az_layout_box_sizing_deep_copy").ok()? };
        let az_layout_direction_row = unsafe { lib.get::<extern fn() -> AzLayoutDirection>(b"az_layout_direction_row").ok()? };
        let az_layout_direction_row_reverse = unsafe { lib.get::<extern fn() -> AzLayoutDirection>(b"az_layout_direction_row_reverse").ok()? };
        let az_layout_direction_column = unsafe { lib.get::<extern fn() -> AzLayoutDirection>(b"az_layout_direction_column").ok()? };
        let az_layout_direction_column_reverse = unsafe { lib.get::<extern fn() -> AzLayoutDirection>(b"az_layout_direction_column_reverse").ok()? };
        let az_layout_direction_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutDirection)>(b"az_layout_direction_delete").ok()? };
        let az_layout_direction_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutDirection) -> AzLayoutDirection>(b"az_layout_direction_deep_copy").ok()? };
        let az_layout_display_flex = unsafe { lib.get::<extern fn() -> AzLayoutDisplay>(b"az_layout_display_flex").ok()? };
        let az_layout_display_block = unsafe { lib.get::<extern fn() -> AzLayoutDisplay>(b"az_layout_display_block").ok()? };
        let az_layout_display_inline_block = unsafe { lib.get::<extern fn() -> AzLayoutDisplay>(b"az_layout_display_inline_block").ok()? };
        let az_layout_display_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutDisplay)>(b"az_layout_display_delete").ok()? };
        let az_layout_display_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutDisplay) -> AzLayoutDisplay>(b"az_layout_display_deep_copy").ok()? };
        let az_layout_flex_grow_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFlexGrow)>(b"az_layout_flex_grow_delete").ok()? };
        let az_layout_flex_grow_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFlexGrow) -> AzLayoutFlexGrow>(b"az_layout_flex_grow_deep_copy").ok()? };
        let az_layout_flex_shrink_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFlexShrink)>(b"az_layout_flex_shrink_delete").ok()? };
        let az_layout_flex_shrink_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFlexShrink) -> AzLayoutFlexShrink>(b"az_layout_flex_shrink_deep_copy").ok()? };
        let az_layout_float_left = unsafe { lib.get::<extern fn() -> AzLayoutFloat>(b"az_layout_float_left").ok()? };
        let az_layout_float_right = unsafe { lib.get::<extern fn() -> AzLayoutFloat>(b"az_layout_float_right").ok()? };
        let az_layout_float_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFloat)>(b"az_layout_float_delete").ok()? };
        let az_layout_float_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFloat) -> AzLayoutFloat>(b"az_layout_float_deep_copy").ok()? };
        let az_layout_height_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutHeight)>(b"az_layout_height_delete").ok()? };
        let az_layout_height_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutHeight) -> AzLayoutHeight>(b"az_layout_height_deep_copy").ok()? };
        let az_layout_justify_content_start = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContent>(b"az_layout_justify_content_start").ok()? };
        let az_layout_justify_content_end = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContent>(b"az_layout_justify_content_end").ok()? };
        let az_layout_justify_content_center = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContent>(b"az_layout_justify_content_center").ok()? };
        let az_layout_justify_content_space_between = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContent>(b"az_layout_justify_content_space_between").ok()? };
        let az_layout_justify_content_space_around = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContent>(b"az_layout_justify_content_space_around").ok()? };
        let az_layout_justify_content_space_evenly = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContent>(b"az_layout_justify_content_space_evenly").ok()? };
        let az_layout_justify_content_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutJustifyContent)>(b"az_layout_justify_content_delete").ok()? };
        let az_layout_justify_content_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutJustifyContent) -> AzLayoutJustifyContent>(b"az_layout_justify_content_deep_copy").ok()? };
        let az_layout_left_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutLeft)>(b"az_layout_left_delete").ok()? };
        let az_layout_left_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutLeft) -> AzLayoutLeft>(b"az_layout_left_deep_copy").ok()? };
        let az_layout_margin_bottom_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginBottom)>(b"az_layout_margin_bottom_delete").ok()? };
        let az_layout_margin_bottom_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginBottom) -> AzLayoutMarginBottom>(b"az_layout_margin_bottom_deep_copy").ok()? };
        let az_layout_margin_left_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginLeft)>(b"az_layout_margin_left_delete").ok()? };
        let az_layout_margin_left_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginLeft) -> AzLayoutMarginLeft>(b"az_layout_margin_left_deep_copy").ok()? };
        let az_layout_margin_right_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginRight)>(b"az_layout_margin_right_delete").ok()? };
        let az_layout_margin_right_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginRight) -> AzLayoutMarginRight>(b"az_layout_margin_right_deep_copy").ok()? };
        let az_layout_margin_top_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginTop)>(b"az_layout_margin_top_delete").ok()? };
        let az_layout_margin_top_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginTop) -> AzLayoutMarginTop>(b"az_layout_margin_top_deep_copy").ok()? };
        let az_layout_max_height_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMaxHeight)>(b"az_layout_max_height_delete").ok()? };
        let az_layout_max_height_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMaxHeight) -> AzLayoutMaxHeight>(b"az_layout_max_height_deep_copy").ok()? };
        let az_layout_max_width_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMaxWidth)>(b"az_layout_max_width_delete").ok()? };
        let az_layout_max_width_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMaxWidth) -> AzLayoutMaxWidth>(b"az_layout_max_width_deep_copy").ok()? };
        let az_layout_min_height_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMinHeight)>(b"az_layout_min_height_delete").ok()? };
        let az_layout_min_height_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMinHeight) -> AzLayoutMinHeight>(b"az_layout_min_height_deep_copy").ok()? };
        let az_layout_min_width_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMinWidth)>(b"az_layout_min_width_delete").ok()? };
        let az_layout_min_width_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMinWidth) -> AzLayoutMinWidth>(b"az_layout_min_width_deep_copy").ok()? };
        let az_layout_padding_bottom_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingBottom)>(b"az_layout_padding_bottom_delete").ok()? };
        let az_layout_padding_bottom_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingBottom) -> AzLayoutPaddingBottom>(b"az_layout_padding_bottom_deep_copy").ok()? };
        let az_layout_padding_left_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingLeft)>(b"az_layout_padding_left_delete").ok()? };
        let az_layout_padding_left_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingLeft) -> AzLayoutPaddingLeft>(b"az_layout_padding_left_deep_copy").ok()? };
        let az_layout_padding_right_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingRight)>(b"az_layout_padding_right_delete").ok()? };
        let az_layout_padding_right_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingRight) -> AzLayoutPaddingRight>(b"az_layout_padding_right_deep_copy").ok()? };
        let az_layout_padding_top_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingTop)>(b"az_layout_padding_top_delete").ok()? };
        let az_layout_padding_top_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingTop) -> AzLayoutPaddingTop>(b"az_layout_padding_top_deep_copy").ok()? };
        let az_layout_position_static = unsafe { lib.get::<extern fn() -> AzLayoutPosition>(b"az_layout_position_static").ok()? };
        let az_layout_position_relative = unsafe { lib.get::<extern fn() -> AzLayoutPosition>(b"az_layout_position_relative").ok()? };
        let az_layout_position_absolute = unsafe { lib.get::<extern fn() -> AzLayoutPosition>(b"az_layout_position_absolute").ok()? };
        let az_layout_position_fixed = unsafe { lib.get::<extern fn() -> AzLayoutPosition>(b"az_layout_position_fixed").ok()? };
        let az_layout_position_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPosition)>(b"az_layout_position_delete").ok()? };
        let az_layout_position_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPosition) -> AzLayoutPosition>(b"az_layout_position_deep_copy").ok()? };
        let az_layout_right_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutRight)>(b"az_layout_right_delete").ok()? };
        let az_layout_right_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutRight) -> AzLayoutRight>(b"az_layout_right_deep_copy").ok()? };
        let az_layout_top_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutTop)>(b"az_layout_top_delete").ok()? };
        let az_layout_top_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutTop) -> AzLayoutTop>(b"az_layout_top_deep_copy").ok()? };
        let az_layout_width_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutWidth)>(b"az_layout_width_delete").ok()? };
        let az_layout_width_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutWidth) -> AzLayoutWidth>(b"az_layout_width_deep_copy").ok()? };
        let az_layout_wrap_wrap = unsafe { lib.get::<extern fn() -> AzLayoutWrap>(b"az_layout_wrap_wrap").ok()? };
        let az_layout_wrap_no_wrap = unsafe { lib.get::<extern fn() -> AzLayoutWrap>(b"az_layout_wrap_no_wrap").ok()? };
        let az_layout_wrap_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutWrap)>(b"az_layout_wrap_delete").ok()? };
        let az_layout_wrap_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutWrap) -> AzLayoutWrap>(b"az_layout_wrap_deep_copy").ok()? };
        let az_overflow_scroll = unsafe { lib.get::<extern fn() -> AzOverflow>(b"az_overflow_scroll").ok()? };
        let az_overflow_auto = unsafe { lib.get::<extern fn() -> AzOverflow>(b"az_overflow_auto").ok()? };
        let az_overflow_hidden = unsafe { lib.get::<extern fn() -> AzOverflow>(b"az_overflow_hidden").ok()? };
        let az_overflow_visible = unsafe { lib.get::<extern fn() -> AzOverflow>(b"az_overflow_visible").ok()? };
        let az_overflow_delete = unsafe { lib.get::<extern fn(_: &mut AzOverflow)>(b"az_overflow_delete").ok()? };
        let az_overflow_deep_copy = unsafe { lib.get::<extern fn(_: &AzOverflow) -> AzOverflow>(b"az_overflow_deep_copy").ok()? };
        let az_percentage_value_delete = unsafe { lib.get::<extern fn(_: &mut AzPercentageValue)>(b"az_percentage_value_delete").ok()? };
        let az_percentage_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzPercentageValue) -> AzPercentageValue>(b"az_percentage_value_deep_copy").ok()? };
        let az_gradient_stop_pre_delete = unsafe { lib.get::<extern fn(_: &mut AzGradientStopPre)>(b"az_gradient_stop_pre_delete").ok()? };
        let az_gradient_stop_pre_deep_copy = unsafe { lib.get::<extern fn(_: &AzGradientStopPre) -> AzGradientStopPre>(b"az_gradient_stop_pre_deep_copy").ok()? };
        let az_direction_corner_right = unsafe { lib.get::<extern fn() -> AzDirectionCorner>(b"az_direction_corner_right").ok()? };
        let az_direction_corner_left = unsafe { lib.get::<extern fn() -> AzDirectionCorner>(b"az_direction_corner_left").ok()? };
        let az_direction_corner_top = unsafe { lib.get::<extern fn() -> AzDirectionCorner>(b"az_direction_corner_top").ok()? };
        let az_direction_corner_bottom = unsafe { lib.get::<extern fn() -> AzDirectionCorner>(b"az_direction_corner_bottom").ok()? };
        let az_direction_corner_top_right = unsafe { lib.get::<extern fn() -> AzDirectionCorner>(b"az_direction_corner_top_right").ok()? };
        let az_direction_corner_top_left = unsafe { lib.get::<extern fn() -> AzDirectionCorner>(b"az_direction_corner_top_left").ok()? };
        let az_direction_corner_bottom_right = unsafe { lib.get::<extern fn() -> AzDirectionCorner>(b"az_direction_corner_bottom_right").ok()? };
        let az_direction_corner_bottom_left = unsafe { lib.get::<extern fn() -> AzDirectionCorner>(b"az_direction_corner_bottom_left").ok()? };
        let az_direction_corner_delete = unsafe { lib.get::<extern fn(_: &mut AzDirectionCorner)>(b"az_direction_corner_delete").ok()? };
        let az_direction_corner_deep_copy = unsafe { lib.get::<extern fn(_: &AzDirectionCorner) -> AzDirectionCorner>(b"az_direction_corner_deep_copy").ok()? };
        let az_direction_corners_delete = unsafe { lib.get::<extern fn(_: &mut AzDirectionCorners)>(b"az_direction_corners_delete").ok()? };
        let az_direction_corners_deep_copy = unsafe { lib.get::<extern fn(_: &AzDirectionCorners) -> AzDirectionCorners>(b"az_direction_corners_deep_copy").ok()? };
        let az_direction_angle = unsafe { lib.get::<extern fn(_: AzFloatValue) -> AzDirection>(b"az_direction_angle").ok()? };
        let az_direction_from_to = unsafe { lib.get::<extern fn(_: AzDirectionCorners) -> AzDirection>(b"az_direction_from_to").ok()? };
        let az_direction_delete = unsafe { lib.get::<extern fn(_: &mut AzDirection)>(b"az_direction_delete").ok()? };
        let az_direction_deep_copy = unsafe { lib.get::<extern fn(_: &AzDirection) -> AzDirection>(b"az_direction_deep_copy").ok()? };
        let az_extend_mode_clamp = unsafe { lib.get::<extern fn() -> AzExtendMode>(b"az_extend_mode_clamp").ok()? };
        let az_extend_mode_repeat = unsafe { lib.get::<extern fn() -> AzExtendMode>(b"az_extend_mode_repeat").ok()? };
        let az_extend_mode_delete = unsafe { lib.get::<extern fn(_: &mut AzExtendMode)>(b"az_extend_mode_delete").ok()? };
        let az_extend_mode_deep_copy = unsafe { lib.get::<extern fn(_: &AzExtendMode) -> AzExtendMode>(b"az_extend_mode_deep_copy").ok()? };
        let az_linear_gradient_delete = unsafe { lib.get::<extern fn(_: &mut AzLinearGradient)>(b"az_linear_gradient_delete").ok()? };
        let az_linear_gradient_deep_copy = unsafe { lib.get::<extern fn(_: &AzLinearGradient) -> AzLinearGradient>(b"az_linear_gradient_deep_copy").ok()? };
        let az_shape_ellipse = unsafe { lib.get::<extern fn() -> AzShape>(b"az_shape_ellipse").ok()? };
        let az_shape_circle = unsafe { lib.get::<extern fn() -> AzShape>(b"az_shape_circle").ok()? };
        let az_shape_delete = unsafe { lib.get::<extern fn(_: &mut AzShape)>(b"az_shape_delete").ok()? };
        let az_shape_deep_copy = unsafe { lib.get::<extern fn(_: &AzShape) -> AzShape>(b"az_shape_deep_copy").ok()? };
        let az_radial_gradient_delete = unsafe { lib.get::<extern fn(_: &mut AzRadialGradient)>(b"az_radial_gradient_delete").ok()? };
        let az_radial_gradient_deep_copy = unsafe { lib.get::<extern fn(_: &AzRadialGradient) -> AzRadialGradient>(b"az_radial_gradient_deep_copy").ok()? };
        let az_css_image_id_delete = unsafe { lib.get::<extern fn(_: &mut AzCssImageId)>(b"az_css_image_id_delete").ok()? };
        let az_css_image_id_deep_copy = unsafe { lib.get::<extern fn(_: &AzCssImageId) -> AzCssImageId>(b"az_css_image_id_deep_copy").ok()? };
        let az_style_background_content_linear_gradient = unsafe { lib.get::<extern fn(_: AzLinearGradient) -> AzStyleBackgroundContent>(b"az_style_background_content_linear_gradient").ok()? };
        let az_style_background_content_radial_gradient = unsafe { lib.get::<extern fn(_: AzRadialGradient) -> AzStyleBackgroundContent>(b"az_style_background_content_radial_gradient").ok()? };
        let az_style_background_content_image = unsafe { lib.get::<extern fn(_: AzCssImageId) -> AzStyleBackgroundContent>(b"az_style_background_content_image").ok()? };
        let az_style_background_content_color = unsafe { lib.get::<extern fn(_: AzColorU) -> AzStyleBackgroundContent>(b"az_style_background_content_color").ok()? };
        let az_style_background_content_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundContent)>(b"az_style_background_content_delete").ok()? };
        let az_style_background_content_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundContent) -> AzStyleBackgroundContent>(b"az_style_background_content_deep_copy").ok()? };
        let az_background_position_horizontal_left = unsafe { lib.get::<extern fn() -> AzBackgroundPositionHorizontal>(b"az_background_position_horizontal_left").ok()? };
        let az_background_position_horizontal_center = unsafe { lib.get::<extern fn() -> AzBackgroundPositionHorizontal>(b"az_background_position_horizontal_center").ok()? };
        let az_background_position_horizontal_right = unsafe { lib.get::<extern fn() -> AzBackgroundPositionHorizontal>(b"az_background_position_horizontal_right").ok()? };
        let az_background_position_horizontal_exact = unsafe { lib.get::<extern fn(_: AzPixelValue) -> AzBackgroundPositionHorizontal>(b"az_background_position_horizontal_exact").ok()? };
        let az_background_position_horizontal_delete = unsafe { lib.get::<extern fn(_: &mut AzBackgroundPositionHorizontal)>(b"az_background_position_horizontal_delete").ok()? };
        let az_background_position_horizontal_deep_copy = unsafe { lib.get::<extern fn(_: &AzBackgroundPositionHorizontal) -> AzBackgroundPositionHorizontal>(b"az_background_position_horizontal_deep_copy").ok()? };
        let az_background_position_vertical_top = unsafe { lib.get::<extern fn() -> AzBackgroundPositionVertical>(b"az_background_position_vertical_top").ok()? };
        let az_background_position_vertical_center = unsafe { lib.get::<extern fn() -> AzBackgroundPositionVertical>(b"az_background_position_vertical_center").ok()? };
        let az_background_position_vertical_bottom = unsafe { lib.get::<extern fn() -> AzBackgroundPositionVertical>(b"az_background_position_vertical_bottom").ok()? };
        let az_background_position_vertical_exact = unsafe { lib.get::<extern fn(_: AzPixelValue) -> AzBackgroundPositionVertical>(b"az_background_position_vertical_exact").ok()? };
        let az_background_position_vertical_delete = unsafe { lib.get::<extern fn(_: &mut AzBackgroundPositionVertical)>(b"az_background_position_vertical_delete").ok()? };
        let az_background_position_vertical_deep_copy = unsafe { lib.get::<extern fn(_: &AzBackgroundPositionVertical) -> AzBackgroundPositionVertical>(b"az_background_position_vertical_deep_copy").ok()? };
        let az_style_background_position_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundPosition)>(b"az_style_background_position_delete").ok()? };
        let az_style_background_position_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundPosition) -> AzStyleBackgroundPosition>(b"az_style_background_position_deep_copy").ok()? };
        let az_style_background_repeat_no_repeat = unsafe { lib.get::<extern fn() -> AzStyleBackgroundRepeat>(b"az_style_background_repeat_no_repeat").ok()? };
        let az_style_background_repeat_repeat = unsafe { lib.get::<extern fn() -> AzStyleBackgroundRepeat>(b"az_style_background_repeat_repeat").ok()? };
        let az_style_background_repeat_repeat_x = unsafe { lib.get::<extern fn() -> AzStyleBackgroundRepeat>(b"az_style_background_repeat_repeat_x").ok()? };
        let az_style_background_repeat_repeat_y = unsafe { lib.get::<extern fn() -> AzStyleBackgroundRepeat>(b"az_style_background_repeat_repeat_y").ok()? };
        let az_style_background_repeat_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundRepeat)>(b"az_style_background_repeat_delete").ok()? };
        let az_style_background_repeat_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundRepeat) -> AzStyleBackgroundRepeat>(b"az_style_background_repeat_deep_copy").ok()? };
        let az_style_background_size_exact_size = unsafe { lib.get::<extern fn(_: AzPixelValue) -> AzStyleBackgroundSize>(b"az_style_background_size_exact_size").ok()? };
        let az_style_background_size_contain = unsafe { lib.get::<extern fn() -> AzStyleBackgroundSize>(b"az_style_background_size_contain").ok()? };
        let az_style_background_size_cover = unsafe { lib.get::<extern fn() -> AzStyleBackgroundSize>(b"az_style_background_size_cover").ok()? };
        let az_style_background_size_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundSize)>(b"az_style_background_size_delete").ok()? };
        let az_style_background_size_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundSize) -> AzStyleBackgroundSize>(b"az_style_background_size_deep_copy").ok()? };
        let az_style_border_bottom_color_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomColor)>(b"az_style_border_bottom_color_delete").ok()? };
        let az_style_border_bottom_color_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomColor) -> AzStyleBorderBottomColor>(b"az_style_border_bottom_color_deep_copy").ok()? };
        let az_style_border_bottom_left_radius_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomLeftRadius)>(b"az_style_border_bottom_left_radius_delete").ok()? };
        let az_style_border_bottom_left_radius_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadius>(b"az_style_border_bottom_left_radius_deep_copy").ok()? };
        let az_style_border_bottom_right_radius_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomRightRadius)>(b"az_style_border_bottom_right_radius_delete").ok()? };
        let az_style_border_bottom_right_radius_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadius>(b"az_style_border_bottom_right_radius_deep_copy").ok()? };
        let az_border_style_none = unsafe { lib.get::<extern fn() -> AzBorderStyle>(b"az_border_style_none").ok()? };
        let az_border_style_solid = unsafe { lib.get::<extern fn() -> AzBorderStyle>(b"az_border_style_solid").ok()? };
        let az_border_style_double = unsafe { lib.get::<extern fn() -> AzBorderStyle>(b"az_border_style_double").ok()? };
        let az_border_style_dotted = unsafe { lib.get::<extern fn() -> AzBorderStyle>(b"az_border_style_dotted").ok()? };
        let az_border_style_dashed = unsafe { lib.get::<extern fn() -> AzBorderStyle>(b"az_border_style_dashed").ok()? };
        let az_border_style_hidden = unsafe { lib.get::<extern fn() -> AzBorderStyle>(b"az_border_style_hidden").ok()? };
        let az_border_style_groove = unsafe { lib.get::<extern fn() -> AzBorderStyle>(b"az_border_style_groove").ok()? };
        let az_border_style_ridge = unsafe { lib.get::<extern fn() -> AzBorderStyle>(b"az_border_style_ridge").ok()? };
        let az_border_style_inset = unsafe { lib.get::<extern fn() -> AzBorderStyle>(b"az_border_style_inset").ok()? };
        let az_border_style_outset = unsafe { lib.get::<extern fn() -> AzBorderStyle>(b"az_border_style_outset").ok()? };
        let az_border_style_delete = unsafe { lib.get::<extern fn(_: &mut AzBorderStyle)>(b"az_border_style_delete").ok()? };
        let az_border_style_deep_copy = unsafe { lib.get::<extern fn(_: &AzBorderStyle) -> AzBorderStyle>(b"az_border_style_deep_copy").ok()? };
        let az_style_border_bottom_style_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomStyle)>(b"az_style_border_bottom_style_delete").ok()? };
        let az_style_border_bottom_style_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomStyle) -> AzStyleBorderBottomStyle>(b"az_style_border_bottom_style_deep_copy").ok()? };
        let az_style_border_bottom_width_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomWidth)>(b"az_style_border_bottom_width_delete").ok()? };
        let az_style_border_bottom_width_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomWidth) -> AzStyleBorderBottomWidth>(b"az_style_border_bottom_width_deep_copy").ok()? };
        let az_style_border_left_color_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftColor)>(b"az_style_border_left_color_delete").ok()? };
        let az_style_border_left_color_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftColor) -> AzStyleBorderLeftColor>(b"az_style_border_left_color_deep_copy").ok()? };
        let az_style_border_left_style_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftStyle)>(b"az_style_border_left_style_delete").ok()? };
        let az_style_border_left_style_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftStyle) -> AzStyleBorderLeftStyle>(b"az_style_border_left_style_deep_copy").ok()? };
        let az_style_border_left_width_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftWidth)>(b"az_style_border_left_width_delete").ok()? };
        let az_style_border_left_width_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftWidth) -> AzStyleBorderLeftWidth>(b"az_style_border_left_width_deep_copy").ok()? };
        let az_style_border_right_color_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightColor)>(b"az_style_border_right_color_delete").ok()? };
        let az_style_border_right_color_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightColor) -> AzStyleBorderRightColor>(b"az_style_border_right_color_deep_copy").ok()? };
        let az_style_border_right_style_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightStyle)>(b"az_style_border_right_style_delete").ok()? };
        let az_style_border_right_style_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightStyle) -> AzStyleBorderRightStyle>(b"az_style_border_right_style_deep_copy").ok()? };
        let az_style_border_right_width_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightWidthPtr)>(b"az_style_border_right_width_delete").ok()? };
        let az_style_border_right_width_shallow_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightWidthPtr) -> AzStyleBorderRightWidthPtr>(b"az_style_border_right_width_shallow_copy").ok()? };
        let az_style_border_top_color_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopColor)>(b"az_style_border_top_color_delete").ok()? };
        let az_style_border_top_color_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopColor) -> AzStyleBorderTopColor>(b"az_style_border_top_color_deep_copy").ok()? };
        let az_style_border_top_left_radius_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopLeftRadius)>(b"az_style_border_top_left_radius_delete").ok()? };
        let az_style_border_top_left_radius_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopLeftRadius) -> AzStyleBorderTopLeftRadius>(b"az_style_border_top_left_radius_deep_copy").ok()? };
        let az_style_border_top_right_radius_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopRightRadius)>(b"az_style_border_top_right_radius_delete").ok()? };
        let az_style_border_top_right_radius_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopRightRadius) -> AzStyleBorderTopRightRadius>(b"az_style_border_top_right_radius_deep_copy").ok()? };
        let az_style_border_top_style_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopStyle)>(b"az_style_border_top_style_delete").ok()? };
        let az_style_border_top_style_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopStyle) -> AzStyleBorderTopStyle>(b"az_style_border_top_style_deep_copy").ok()? };
        let az_style_border_top_width_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopWidth)>(b"az_style_border_top_width_delete").ok()? };
        let az_style_border_top_width_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopWidth) -> AzStyleBorderTopWidth>(b"az_style_border_top_width_deep_copy").ok()? };
        let az_style_cursor_alias = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_alias").ok()? };
        let az_style_cursor_all_scroll = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_all_scroll").ok()? };
        let az_style_cursor_cell = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_cell").ok()? };
        let az_style_cursor_col_resize = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_col_resize").ok()? };
        let az_style_cursor_context_menu = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_context_menu").ok()? };
        let az_style_cursor_copy = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_copy").ok()? };
        let az_style_cursor_crosshair = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_crosshair").ok()? };
        let az_style_cursor_default = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_default").ok()? };
        let az_style_cursor_e_resize = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_e_resize").ok()? };
        let az_style_cursor_ew_resize = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_ew_resize").ok()? };
        let az_style_cursor_grab = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_grab").ok()? };
        let az_style_cursor_grabbing = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_grabbing").ok()? };
        let az_style_cursor_help = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_help").ok()? };
        let az_style_cursor_move = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_move").ok()? };
        let az_style_cursor_n_resize = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_n_resize").ok()? };
        let az_style_cursor_ns_resize = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_ns_resize").ok()? };
        let az_style_cursor_nesw_resize = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_nesw_resize").ok()? };
        let az_style_cursor_nwse_resize = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_nwse_resize").ok()? };
        let az_style_cursor_pointer = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_pointer").ok()? };
        let az_style_cursor_progress = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_progress").ok()? };
        let az_style_cursor_row_resize = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_row_resize").ok()? };
        let az_style_cursor_s_resize = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_s_resize").ok()? };
        let az_style_cursor_se_resize = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_se_resize").ok()? };
        let az_style_cursor_text = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_text").ok()? };
        let az_style_cursor_unset = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_unset").ok()? };
        let az_style_cursor_vertical_text = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_vertical_text").ok()? };
        let az_style_cursor_w_resize = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_w_resize").ok()? };
        let az_style_cursor_wait = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_wait").ok()? };
        let az_style_cursor_zoom_in = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_zoom_in").ok()? };
        let az_style_cursor_zoom_out = unsafe { lib.get::<extern fn() -> AzStyleCursor>(b"az_style_cursor_zoom_out").ok()? };
        let az_style_cursor_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleCursor)>(b"az_style_cursor_delete").ok()? };
        let az_style_cursor_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleCursor) -> AzStyleCursor>(b"az_style_cursor_deep_copy").ok()? };
        let az_style_font_family_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleFontFamily)>(b"az_style_font_family_delete").ok()? };
        let az_style_font_family_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleFontFamily) -> AzStyleFontFamily>(b"az_style_font_family_deep_copy").ok()? };
        let az_style_font_size_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleFontSize)>(b"az_style_font_size_delete").ok()? };
        let az_style_font_size_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleFontSize) -> AzStyleFontSize>(b"az_style_font_size_deep_copy").ok()? };
        let az_style_letter_spacing_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleLetterSpacing)>(b"az_style_letter_spacing_delete").ok()? };
        let az_style_letter_spacing_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleLetterSpacing) -> AzStyleLetterSpacing>(b"az_style_letter_spacing_deep_copy").ok()? };
        let az_style_line_height_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleLineHeight)>(b"az_style_line_height_delete").ok()? };
        let az_style_line_height_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleLineHeight) -> AzStyleLineHeight>(b"az_style_line_height_deep_copy").ok()? };
        let az_style_tab_width_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTabWidth)>(b"az_style_tab_width_delete").ok()? };
        let az_style_tab_width_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTabWidth) -> AzStyleTabWidth>(b"az_style_tab_width_deep_copy").ok()? };
        let az_style_text_alignment_horz_left = unsafe { lib.get::<extern fn() -> AzStyleTextAlignmentHorz>(b"az_style_text_alignment_horz_left").ok()? };
        let az_style_text_alignment_horz_center = unsafe { lib.get::<extern fn() -> AzStyleTextAlignmentHorz>(b"az_style_text_alignment_horz_center").ok()? };
        let az_style_text_alignment_horz_right = unsafe { lib.get::<extern fn() -> AzStyleTextAlignmentHorz>(b"az_style_text_alignment_horz_right").ok()? };
        let az_style_text_alignment_horz_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTextAlignmentHorz)>(b"az_style_text_alignment_horz_delete").ok()? };
        let az_style_text_alignment_horz_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTextAlignmentHorz) -> AzStyleTextAlignmentHorz>(b"az_style_text_alignment_horz_deep_copy").ok()? };
        let az_style_text_color_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTextColor)>(b"az_style_text_color_delete").ok()? };
        let az_style_text_color_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTextColor) -> AzStyleTextColor>(b"az_style_text_color_deep_copy").ok()? };
        let az_style_word_spacing_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleWordSpacing)>(b"az_style_word_spacing_delete").ok()? };
        let az_style_word_spacing_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleWordSpacing) -> AzStyleWordSpacing>(b"az_style_word_spacing_deep_copy").ok()? };
        let az_box_shadow_pre_display_item_value_auto = unsafe { lib.get::<extern fn() -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_auto").ok()? };
        let az_box_shadow_pre_display_item_value_none = unsafe { lib.get::<extern fn() -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_none").ok()? };
        let az_box_shadow_pre_display_item_value_inherit = unsafe { lib.get::<extern fn() -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_inherit").ok()? };
        let az_box_shadow_pre_display_item_value_initial = unsafe { lib.get::<extern fn() -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_initial").ok()? };
        let az_box_shadow_pre_display_item_value_exact = unsafe { lib.get::<extern fn(_: AzBoxShadowPreDisplayItem) -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_exact").ok()? };
        let az_box_shadow_pre_display_item_value_delete = unsafe { lib.get::<extern fn(_: &mut AzBoxShadowPreDisplayItemValue)>(b"az_box_shadow_pre_display_item_value_delete").ok()? };
        let az_box_shadow_pre_display_item_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzBoxShadowPreDisplayItemValue) -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_deep_copy").ok()? };
        let az_layout_align_content_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_auto").ok()? };
        let az_layout_align_content_value_none = unsafe { lib.get::<extern fn() -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_none").ok()? };
        let az_layout_align_content_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_inherit").ok()? };
        let az_layout_align_content_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_initial").ok()? };
        let az_layout_align_content_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutAlignContent) -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_exact").ok()? };
        let az_layout_align_content_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutAlignContentValue)>(b"az_layout_align_content_value_delete").ok()? };
        let az_layout_align_content_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutAlignContentValue) -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_deep_copy").ok()? };
        let az_layout_align_items_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_auto").ok()? };
        let az_layout_align_items_value_none = unsafe { lib.get::<extern fn() -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_none").ok()? };
        let az_layout_align_items_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_inherit").ok()? };
        let az_layout_align_items_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_initial").ok()? };
        let az_layout_align_items_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutAlignItems) -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_exact").ok()? };
        let az_layout_align_items_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutAlignItemsValue)>(b"az_layout_align_items_value_delete").ok()? };
        let az_layout_align_items_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutAlignItemsValue) -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_deep_copy").ok()? };
        let az_layout_bottom_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutBottomValue>(b"az_layout_bottom_value_auto").ok()? };
        let az_layout_bottom_value_none = unsafe { lib.get::<extern fn() -> AzLayoutBottomValue>(b"az_layout_bottom_value_none").ok()? };
        let az_layout_bottom_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutBottomValue>(b"az_layout_bottom_value_inherit").ok()? };
        let az_layout_bottom_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutBottomValue>(b"az_layout_bottom_value_initial").ok()? };
        let az_layout_bottom_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutBottom) -> AzLayoutBottomValue>(b"az_layout_bottom_value_exact").ok()? };
        let az_layout_bottom_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutBottomValue)>(b"az_layout_bottom_value_delete").ok()? };
        let az_layout_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutBottomValue) -> AzLayoutBottomValue>(b"az_layout_bottom_value_deep_copy").ok()? };
        let az_layout_box_sizing_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_auto").ok()? };
        let az_layout_box_sizing_value_none = unsafe { lib.get::<extern fn() -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_none").ok()? };
        let az_layout_box_sizing_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_inherit").ok()? };
        let az_layout_box_sizing_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_initial").ok()? };
        let az_layout_box_sizing_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutBoxSizing) -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_exact").ok()? };
        let az_layout_box_sizing_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutBoxSizingValue)>(b"az_layout_box_sizing_value_delete").ok()? };
        let az_layout_box_sizing_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutBoxSizingValue) -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_deep_copy").ok()? };
        let az_layout_direction_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutDirectionValue>(b"az_layout_direction_value_auto").ok()? };
        let az_layout_direction_value_none = unsafe { lib.get::<extern fn() -> AzLayoutDirectionValue>(b"az_layout_direction_value_none").ok()? };
        let az_layout_direction_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutDirectionValue>(b"az_layout_direction_value_inherit").ok()? };
        let az_layout_direction_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutDirectionValue>(b"az_layout_direction_value_initial").ok()? };
        let az_layout_direction_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutDirection) -> AzLayoutDirectionValue>(b"az_layout_direction_value_exact").ok()? };
        let az_layout_direction_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutDirectionValue)>(b"az_layout_direction_value_delete").ok()? };
        let az_layout_direction_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutDirectionValue) -> AzLayoutDirectionValue>(b"az_layout_direction_value_deep_copy").ok()? };
        let az_layout_display_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutDisplayValue>(b"az_layout_display_value_auto").ok()? };
        let az_layout_display_value_none = unsafe { lib.get::<extern fn() -> AzLayoutDisplayValue>(b"az_layout_display_value_none").ok()? };
        let az_layout_display_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutDisplayValue>(b"az_layout_display_value_inherit").ok()? };
        let az_layout_display_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutDisplayValue>(b"az_layout_display_value_initial").ok()? };
        let az_layout_display_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutDisplay) -> AzLayoutDisplayValue>(b"az_layout_display_value_exact").ok()? };
        let az_layout_display_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutDisplayValue)>(b"az_layout_display_value_delete").ok()? };
        let az_layout_display_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutDisplayValue) -> AzLayoutDisplayValue>(b"az_layout_display_value_deep_copy").ok()? };
        let az_layout_flex_grow_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_auto").ok()? };
        let az_layout_flex_grow_value_none = unsafe { lib.get::<extern fn() -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_none").ok()? };
        let az_layout_flex_grow_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_inherit").ok()? };
        let az_layout_flex_grow_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_initial").ok()? };
        let az_layout_flex_grow_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutFlexGrow) -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_exact").ok()? };
        let az_layout_flex_grow_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFlexGrowValue)>(b"az_layout_flex_grow_value_delete").ok()? };
        let az_layout_flex_grow_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFlexGrowValue) -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_deep_copy").ok()? };
        let az_layout_flex_shrink_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_auto").ok()? };
        let az_layout_flex_shrink_value_none = unsafe { lib.get::<extern fn() -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_none").ok()? };
        let az_layout_flex_shrink_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_inherit").ok()? };
        let az_layout_flex_shrink_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_initial").ok()? };
        let az_layout_flex_shrink_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutFlexShrink) -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_exact").ok()? };
        let az_layout_flex_shrink_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFlexShrinkValue)>(b"az_layout_flex_shrink_value_delete").ok()? };
        let az_layout_flex_shrink_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFlexShrinkValue) -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_deep_copy").ok()? };
        let az_layout_float_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutFloatValue>(b"az_layout_float_value_auto").ok()? };
        let az_layout_float_value_none = unsafe { lib.get::<extern fn() -> AzLayoutFloatValue>(b"az_layout_float_value_none").ok()? };
        let az_layout_float_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutFloatValue>(b"az_layout_float_value_inherit").ok()? };
        let az_layout_float_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutFloatValue>(b"az_layout_float_value_initial").ok()? };
        let az_layout_float_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutFloat) -> AzLayoutFloatValue>(b"az_layout_float_value_exact").ok()? };
        let az_layout_float_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFloatValue)>(b"az_layout_float_value_delete").ok()? };
        let az_layout_float_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFloatValue) -> AzLayoutFloatValue>(b"az_layout_float_value_deep_copy").ok()? };
        let az_layout_height_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutHeightValue>(b"az_layout_height_value_auto").ok()? };
        let az_layout_height_value_none = unsafe { lib.get::<extern fn() -> AzLayoutHeightValue>(b"az_layout_height_value_none").ok()? };
        let az_layout_height_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutHeightValue>(b"az_layout_height_value_inherit").ok()? };
        let az_layout_height_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutHeightValue>(b"az_layout_height_value_initial").ok()? };
        let az_layout_height_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutHeight) -> AzLayoutHeightValue>(b"az_layout_height_value_exact").ok()? };
        let az_layout_height_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutHeightValue)>(b"az_layout_height_value_delete").ok()? };
        let az_layout_height_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutHeightValue) -> AzLayoutHeightValue>(b"az_layout_height_value_deep_copy").ok()? };
        let az_layout_justify_content_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_auto").ok()? };
        let az_layout_justify_content_value_none = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_none").ok()? };
        let az_layout_justify_content_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_inherit").ok()? };
        let az_layout_justify_content_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_initial").ok()? };
        let az_layout_justify_content_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutJustifyContent) -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_exact").ok()? };
        let az_layout_justify_content_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutJustifyContentValue)>(b"az_layout_justify_content_value_delete").ok()? };
        let az_layout_justify_content_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutJustifyContentValue) -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_deep_copy").ok()? };
        let az_layout_left_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutLeftValue>(b"az_layout_left_value_auto").ok()? };
        let az_layout_left_value_none = unsafe { lib.get::<extern fn() -> AzLayoutLeftValue>(b"az_layout_left_value_none").ok()? };
        let az_layout_left_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutLeftValue>(b"az_layout_left_value_inherit").ok()? };
        let az_layout_left_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutLeftValue>(b"az_layout_left_value_initial").ok()? };
        let az_layout_left_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutLeft) -> AzLayoutLeftValue>(b"az_layout_left_value_exact").ok()? };
        let az_layout_left_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutLeftValue)>(b"az_layout_left_value_delete").ok()? };
        let az_layout_left_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutLeftValue) -> AzLayoutLeftValue>(b"az_layout_left_value_deep_copy").ok()? };
        let az_layout_margin_bottom_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_auto").ok()? };
        let az_layout_margin_bottom_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_none").ok()? };
        let az_layout_margin_bottom_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_inherit").ok()? };
        let az_layout_margin_bottom_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_initial").ok()? };
        let az_layout_margin_bottom_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMarginBottom) -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_exact").ok()? };
        let az_layout_margin_bottom_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginBottomValue)>(b"az_layout_margin_bottom_value_delete").ok()? };
        let az_layout_margin_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginBottomValue) -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_deep_copy").ok()? };
        let az_layout_margin_left_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_auto").ok()? };
        let az_layout_margin_left_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_none").ok()? };
        let az_layout_margin_left_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_inherit").ok()? };
        let az_layout_margin_left_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_initial").ok()? };
        let az_layout_margin_left_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMarginLeft) -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_exact").ok()? };
        let az_layout_margin_left_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginLeftValue)>(b"az_layout_margin_left_value_delete").ok()? };
        let az_layout_margin_left_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginLeftValue) -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_deep_copy").ok()? };
        let az_layout_margin_right_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_auto").ok()? };
        let az_layout_margin_right_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_none").ok()? };
        let az_layout_margin_right_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_inherit").ok()? };
        let az_layout_margin_right_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_initial").ok()? };
        let az_layout_margin_right_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMarginRight) -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_exact").ok()? };
        let az_layout_margin_right_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginRightValue)>(b"az_layout_margin_right_value_delete").ok()? };
        let az_layout_margin_right_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginRightValue) -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_deep_copy").ok()? };
        let az_layout_margin_top_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_auto").ok()? };
        let az_layout_margin_top_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_none").ok()? };
        let az_layout_margin_top_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_inherit").ok()? };
        let az_layout_margin_top_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_initial").ok()? };
        let az_layout_margin_top_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMarginTop) -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_exact").ok()? };
        let az_layout_margin_top_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginTopValue)>(b"az_layout_margin_top_value_delete").ok()? };
        let az_layout_margin_top_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginTopValue) -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_deep_copy").ok()? };
        let az_layout_max_height_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_auto").ok()? };
        let az_layout_max_height_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_none").ok()? };
        let az_layout_max_height_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_inherit").ok()? };
        let az_layout_max_height_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_initial").ok()? };
        let az_layout_max_height_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMaxHeight) -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_exact").ok()? };
        let az_layout_max_height_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMaxHeightValue)>(b"az_layout_max_height_value_delete").ok()? };
        let az_layout_max_height_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMaxHeightValue) -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_deep_copy").ok()? };
        let az_layout_max_width_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_auto").ok()? };
        let az_layout_max_width_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_none").ok()? };
        let az_layout_max_width_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_inherit").ok()? };
        let az_layout_max_width_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_initial").ok()? };
        let az_layout_max_width_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMaxWidth) -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_exact").ok()? };
        let az_layout_max_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMaxWidthValue)>(b"az_layout_max_width_value_delete").ok()? };
        let az_layout_max_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMaxWidthValue) -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_deep_copy").ok()? };
        let az_layout_min_height_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_auto").ok()? };
        let az_layout_min_height_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_none").ok()? };
        let az_layout_min_height_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_inherit").ok()? };
        let az_layout_min_height_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_initial").ok()? };
        let az_layout_min_height_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMinHeight) -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_exact").ok()? };
        let az_layout_min_height_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMinHeightValue)>(b"az_layout_min_height_value_delete").ok()? };
        let az_layout_min_height_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMinHeightValue) -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_deep_copy").ok()? };
        let az_layout_min_width_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_auto").ok()? };
        let az_layout_min_width_value_none = unsafe { lib.get::<extern fn() -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_none").ok()? };
        let az_layout_min_width_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_inherit").ok()? };
        let az_layout_min_width_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_initial").ok()? };
        let az_layout_min_width_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutMinWidth) -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_exact").ok()? };
        let az_layout_min_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMinWidthValue)>(b"az_layout_min_width_value_delete").ok()? };
        let az_layout_min_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMinWidthValue) -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_deep_copy").ok()? };
        let az_layout_padding_bottom_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_auto").ok()? };
        let az_layout_padding_bottom_value_none = unsafe { lib.get::<extern fn() -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_none").ok()? };
        let az_layout_padding_bottom_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_inherit").ok()? };
        let az_layout_padding_bottom_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_initial").ok()? };
        let az_layout_padding_bottom_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutPaddingBottom) -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_exact").ok()? };
        let az_layout_padding_bottom_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingBottomValue)>(b"az_layout_padding_bottom_value_delete").ok()? };
        let az_layout_padding_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingBottomValue) -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_deep_copy").ok()? };
        let az_layout_padding_left_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_auto").ok()? };
        let az_layout_padding_left_value_none = unsafe { lib.get::<extern fn() -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_none").ok()? };
        let az_layout_padding_left_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_inherit").ok()? };
        let az_layout_padding_left_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_initial").ok()? };
        let az_layout_padding_left_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutPaddingLeft) -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_exact").ok()? };
        let az_layout_padding_left_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingLeftValue)>(b"az_layout_padding_left_value_delete").ok()? };
        let az_layout_padding_left_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingLeftValue) -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_deep_copy").ok()? };
        let az_layout_padding_right_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_auto").ok()? };
        let az_layout_padding_right_value_none = unsafe { lib.get::<extern fn() -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_none").ok()? };
        let az_layout_padding_right_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_inherit").ok()? };
        let az_layout_padding_right_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_initial").ok()? };
        let az_layout_padding_right_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutPaddingRight) -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_exact").ok()? };
        let az_layout_padding_right_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingRightValue)>(b"az_layout_padding_right_value_delete").ok()? };
        let az_layout_padding_right_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingRightValue) -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_deep_copy").ok()? };
        let az_layout_padding_top_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_auto").ok()? };
        let az_layout_padding_top_value_none = unsafe { lib.get::<extern fn() -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_none").ok()? };
        let az_layout_padding_top_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_inherit").ok()? };
        let az_layout_padding_top_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_initial").ok()? };
        let az_layout_padding_top_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutPaddingTop) -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_exact").ok()? };
        let az_layout_padding_top_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingTopValue)>(b"az_layout_padding_top_value_delete").ok()? };
        let az_layout_padding_top_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingTopValue) -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_deep_copy").ok()? };
        let az_layout_position_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutPositionValue>(b"az_layout_position_value_auto").ok()? };
        let az_layout_position_value_none = unsafe { lib.get::<extern fn() -> AzLayoutPositionValue>(b"az_layout_position_value_none").ok()? };
        let az_layout_position_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutPositionValue>(b"az_layout_position_value_inherit").ok()? };
        let az_layout_position_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutPositionValue>(b"az_layout_position_value_initial").ok()? };
        let az_layout_position_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutPosition) -> AzLayoutPositionValue>(b"az_layout_position_value_exact").ok()? };
        let az_layout_position_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPositionValue)>(b"az_layout_position_value_delete").ok()? };
        let az_layout_position_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPositionValue) -> AzLayoutPositionValue>(b"az_layout_position_value_deep_copy").ok()? };
        let az_layout_right_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutRightValue>(b"az_layout_right_value_auto").ok()? };
        let az_layout_right_value_none = unsafe { lib.get::<extern fn() -> AzLayoutRightValue>(b"az_layout_right_value_none").ok()? };
        let az_layout_right_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutRightValue>(b"az_layout_right_value_inherit").ok()? };
        let az_layout_right_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutRightValue>(b"az_layout_right_value_initial").ok()? };
        let az_layout_right_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutRight) -> AzLayoutRightValue>(b"az_layout_right_value_exact").ok()? };
        let az_layout_right_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutRightValue)>(b"az_layout_right_value_delete").ok()? };
        let az_layout_right_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutRightValue) -> AzLayoutRightValue>(b"az_layout_right_value_deep_copy").ok()? };
        let az_layout_top_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutTopValue>(b"az_layout_top_value_auto").ok()? };
        let az_layout_top_value_none = unsafe { lib.get::<extern fn() -> AzLayoutTopValue>(b"az_layout_top_value_none").ok()? };
        let az_layout_top_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutTopValue>(b"az_layout_top_value_inherit").ok()? };
        let az_layout_top_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutTopValue>(b"az_layout_top_value_initial").ok()? };
        let az_layout_top_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutTop) -> AzLayoutTopValue>(b"az_layout_top_value_exact").ok()? };
        let az_layout_top_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutTopValue)>(b"az_layout_top_value_delete").ok()? };
        let az_layout_top_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutTopValue) -> AzLayoutTopValue>(b"az_layout_top_value_deep_copy").ok()? };
        let az_layout_width_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutWidthValue>(b"az_layout_width_value_auto").ok()? };
        let az_layout_width_value_none = unsafe { lib.get::<extern fn() -> AzLayoutWidthValue>(b"az_layout_width_value_none").ok()? };
        let az_layout_width_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutWidthValue>(b"az_layout_width_value_inherit").ok()? };
        let az_layout_width_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutWidthValue>(b"az_layout_width_value_initial").ok()? };
        let az_layout_width_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutWidth) -> AzLayoutWidthValue>(b"az_layout_width_value_exact").ok()? };
        let az_layout_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutWidthValue)>(b"az_layout_width_value_delete").ok()? };
        let az_layout_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutWidthValue) -> AzLayoutWidthValue>(b"az_layout_width_value_deep_copy").ok()? };
        let az_layout_wrap_value_auto = unsafe { lib.get::<extern fn() -> AzLayoutWrapValue>(b"az_layout_wrap_value_auto").ok()? };
        let az_layout_wrap_value_none = unsafe { lib.get::<extern fn() -> AzLayoutWrapValue>(b"az_layout_wrap_value_none").ok()? };
        let az_layout_wrap_value_inherit = unsafe { lib.get::<extern fn() -> AzLayoutWrapValue>(b"az_layout_wrap_value_inherit").ok()? };
        let az_layout_wrap_value_initial = unsafe { lib.get::<extern fn() -> AzLayoutWrapValue>(b"az_layout_wrap_value_initial").ok()? };
        let az_layout_wrap_value_exact = unsafe { lib.get::<extern fn(_: AzLayoutWrap) -> AzLayoutWrapValue>(b"az_layout_wrap_value_exact").ok()? };
        let az_layout_wrap_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutWrapValue)>(b"az_layout_wrap_value_delete").ok()? };
        let az_layout_wrap_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutWrapValue) -> AzLayoutWrapValue>(b"az_layout_wrap_value_deep_copy").ok()? };
        let az_overflow_value_auto = unsafe { lib.get::<extern fn() -> AzOverflowValue>(b"az_overflow_value_auto").ok()? };
        let az_overflow_value_none = unsafe { lib.get::<extern fn() -> AzOverflowValue>(b"az_overflow_value_none").ok()? };
        let az_overflow_value_inherit = unsafe { lib.get::<extern fn() -> AzOverflowValue>(b"az_overflow_value_inherit").ok()? };
        let az_overflow_value_initial = unsafe { lib.get::<extern fn() -> AzOverflowValue>(b"az_overflow_value_initial").ok()? };
        let az_overflow_value_exact = unsafe { lib.get::<extern fn(_: AzOverflow) -> AzOverflowValue>(b"az_overflow_value_exact").ok()? };
        let az_overflow_value_delete = unsafe { lib.get::<extern fn(_: &mut AzOverflowValue)>(b"az_overflow_value_delete").ok()? };
        let az_overflow_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzOverflowValue) -> AzOverflowValue>(b"az_overflow_value_deep_copy").ok()? };
        let az_style_background_content_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_auto").ok()? };
        let az_style_background_content_value_none = unsafe { lib.get::<extern fn() -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_none").ok()? };
        let az_style_background_content_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_inherit").ok()? };
        let az_style_background_content_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_initial").ok()? };
        let az_style_background_content_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBackgroundContent) -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_exact").ok()? };
        let az_style_background_content_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundContentValue)>(b"az_style_background_content_value_delete").ok()? };
        let az_style_background_content_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_deep_copy").ok()? };
        let az_style_background_position_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_auto").ok()? };
        let az_style_background_position_value_none = unsafe { lib.get::<extern fn() -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_none").ok()? };
        let az_style_background_position_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_inherit").ok()? };
        let az_style_background_position_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_initial").ok()? };
        let az_style_background_position_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBackgroundPosition) -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_exact").ok()? };
        let az_style_background_position_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundPositionValue)>(b"az_style_background_position_value_delete").ok()? };
        let az_style_background_position_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundPositionValue) -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_deep_copy").ok()? };
        let az_style_background_repeat_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_auto").ok()? };
        let az_style_background_repeat_value_none = unsafe { lib.get::<extern fn() -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_none").ok()? };
        let az_style_background_repeat_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_inherit").ok()? };
        let az_style_background_repeat_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_initial").ok()? };
        let az_style_background_repeat_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBackgroundRepeat) -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_exact").ok()? };
        let az_style_background_repeat_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundRepeatValue)>(b"az_style_background_repeat_value_delete").ok()? };
        let az_style_background_repeat_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundRepeatValue) -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_deep_copy").ok()? };
        let az_style_background_size_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_auto").ok()? };
        let az_style_background_size_value_none = unsafe { lib.get::<extern fn() -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_none").ok()? };
        let az_style_background_size_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_inherit").ok()? };
        let az_style_background_size_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_initial").ok()? };
        let az_style_background_size_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBackgroundSize) -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_exact").ok()? };
        let az_style_background_size_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundSizeValue)>(b"az_style_background_size_value_delete").ok()? };
        let az_style_background_size_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundSizeValue) -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_deep_copy").ok()? };
        let az_style_border_bottom_color_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_auto").ok()? };
        let az_style_border_bottom_color_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_none").ok()? };
        let az_style_border_bottom_color_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_inherit").ok()? };
        let az_style_border_bottom_color_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_initial").ok()? };
        let az_style_border_bottom_color_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomColor) -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_exact").ok()? };
        let az_style_border_bottom_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomColorValue)>(b"az_style_border_bottom_color_value_delete").ok()? };
        let az_style_border_bottom_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomColorValue) -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_deep_copy").ok()? };
        let az_style_border_bottom_left_radius_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_auto").ok()? };
        let az_style_border_bottom_left_radius_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_none").ok()? };
        let az_style_border_bottom_left_radius_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_inherit").ok()? };
        let az_style_border_bottom_left_radius_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_initial").ok()? };
        let az_style_border_bottom_left_radius_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_exact").ok()? };
        let az_style_border_bottom_left_radius_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomLeftRadiusValue)>(b"az_style_border_bottom_left_radius_value_delete").ok()? };
        let az_style_border_bottom_left_radius_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomLeftRadiusValue) -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_deep_copy").ok()? };
        let az_style_border_bottom_right_radius_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_auto").ok()? };
        let az_style_border_bottom_right_radius_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_none").ok()? };
        let az_style_border_bottom_right_radius_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_inherit").ok()? };
        let az_style_border_bottom_right_radius_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_initial").ok()? };
        let az_style_border_bottom_right_radius_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_exact").ok()? };
        let az_style_border_bottom_right_radius_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomRightRadiusValue)>(b"az_style_border_bottom_right_radius_value_delete").ok()? };
        let az_style_border_bottom_right_radius_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomRightRadiusValue) -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_deep_copy").ok()? };
        let az_style_border_bottom_style_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_auto").ok()? };
        let az_style_border_bottom_style_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_none").ok()? };
        let az_style_border_bottom_style_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_inherit").ok()? };
        let az_style_border_bottom_style_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_initial").ok()? };
        let az_style_border_bottom_style_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomStyle) -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_exact").ok()? };
        let az_style_border_bottom_style_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomStyleValue)>(b"az_style_border_bottom_style_value_delete").ok()? };
        let az_style_border_bottom_style_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomStyleValue) -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_deep_copy").ok()? };
        let az_style_border_bottom_width_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_auto").ok()? };
        let az_style_border_bottom_width_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_none").ok()? };
        let az_style_border_bottom_width_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_inherit").ok()? };
        let az_style_border_bottom_width_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_initial").ok()? };
        let az_style_border_bottom_width_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomWidth) -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_exact").ok()? };
        let az_style_border_bottom_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomWidthValue)>(b"az_style_border_bottom_width_value_delete").ok()? };
        let az_style_border_bottom_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomWidthValue) -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_deep_copy").ok()? };
        let az_style_border_left_color_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_auto").ok()? };
        let az_style_border_left_color_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_none").ok()? };
        let az_style_border_left_color_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_inherit").ok()? };
        let az_style_border_left_color_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_initial").ok()? };
        let az_style_border_left_color_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderLeftColor) -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_exact").ok()? };
        let az_style_border_left_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftColorValue)>(b"az_style_border_left_color_value_delete").ok()? };
        let az_style_border_left_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftColorValue) -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_deep_copy").ok()? };
        let az_style_border_left_style_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_auto").ok()? };
        let az_style_border_left_style_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_none").ok()? };
        let az_style_border_left_style_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_inherit").ok()? };
        let az_style_border_left_style_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_initial").ok()? };
        let az_style_border_left_style_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderLeftStyle) -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_exact").ok()? };
        let az_style_border_left_style_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftStyleValue)>(b"az_style_border_left_style_value_delete").ok()? };
        let az_style_border_left_style_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftStyleValue) -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_deep_copy").ok()? };
        let az_style_border_left_width_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_auto").ok()? };
        let az_style_border_left_width_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_none").ok()? };
        let az_style_border_left_width_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_inherit").ok()? };
        let az_style_border_left_width_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_initial").ok()? };
        let az_style_border_left_width_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderLeftWidth) -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_exact").ok()? };
        let az_style_border_left_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftWidthValue)>(b"az_style_border_left_width_value_delete").ok()? };
        let az_style_border_left_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftWidthValue) -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_deep_copy").ok()? };
        let az_style_border_right_color_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_auto").ok()? };
        let az_style_border_right_color_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_none").ok()? };
        let az_style_border_right_color_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_inherit").ok()? };
        let az_style_border_right_color_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_initial").ok()? };
        let az_style_border_right_color_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderRightColor) -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_exact").ok()? };
        let az_style_border_right_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightColorValue)>(b"az_style_border_right_color_value_delete").ok()? };
        let az_style_border_right_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightColorValue) -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_deep_copy").ok()? };
        let az_style_border_right_style_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_auto").ok()? };
        let az_style_border_right_style_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_none").ok()? };
        let az_style_border_right_style_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_inherit").ok()? };
        let az_style_border_right_style_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_initial").ok()? };
        let az_style_border_right_style_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderRightStyle) -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_exact").ok()? };
        let az_style_border_right_style_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightStyleValue)>(b"az_style_border_right_style_value_delete").ok()? };
        let az_style_border_right_style_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightStyleValue) -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_deep_copy").ok()? };
        let az_style_border_right_width_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_auto").ok()? };
        let az_style_border_right_width_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_none").ok()? };
        let az_style_border_right_width_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_inherit").ok()? };
        let az_style_border_right_width_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_initial").ok()? };
        let az_style_border_right_width_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderRightWidthPtr) -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_exact").ok()? };
        let az_style_border_right_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightWidthValue)>(b"az_style_border_right_width_value_delete").ok()? };
        let az_style_border_right_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightWidthValue) -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_deep_copy").ok()? };
        let az_style_border_top_color_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_auto").ok()? };
        let az_style_border_top_color_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_none").ok()? };
        let az_style_border_top_color_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_inherit").ok()? };
        let az_style_border_top_color_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_initial").ok()? };
        let az_style_border_top_color_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderTopColor) -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_exact").ok()? };
        let az_style_border_top_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopColorValue)>(b"az_style_border_top_color_value_delete").ok()? };
        let az_style_border_top_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopColorValue) -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_deep_copy").ok()? };
        let az_style_border_top_left_radius_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_auto").ok()? };
        let az_style_border_top_left_radius_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_none").ok()? };
        let az_style_border_top_left_radius_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_inherit").ok()? };
        let az_style_border_top_left_radius_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_initial").ok()? };
        let az_style_border_top_left_radius_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderTopLeftRadius) -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_exact").ok()? };
        let az_style_border_top_left_radius_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopLeftRadiusValue)>(b"az_style_border_top_left_radius_value_delete").ok()? };
        let az_style_border_top_left_radius_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopLeftRadiusValue) -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_deep_copy").ok()? };
        let az_style_border_top_right_radius_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_auto").ok()? };
        let az_style_border_top_right_radius_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_none").ok()? };
        let az_style_border_top_right_radius_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_inherit").ok()? };
        let az_style_border_top_right_radius_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_initial").ok()? };
        let az_style_border_top_right_radius_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderTopRightRadius) -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_exact").ok()? };
        let az_style_border_top_right_radius_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopRightRadiusValue)>(b"az_style_border_top_right_radius_value_delete").ok()? };
        let az_style_border_top_right_radius_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopRightRadiusValue) -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_deep_copy").ok()? };
        let az_style_border_top_style_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_auto").ok()? };
        let az_style_border_top_style_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_none").ok()? };
        let az_style_border_top_style_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_inherit").ok()? };
        let az_style_border_top_style_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_initial").ok()? };
        let az_style_border_top_style_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderTopStyle) -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_exact").ok()? };
        let az_style_border_top_style_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopStyleValue)>(b"az_style_border_top_style_value_delete").ok()? };
        let az_style_border_top_style_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopStyleValue) -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_deep_copy").ok()? };
        let az_style_border_top_width_value_auto = unsafe { lib.get::<extern fn() -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_auto").ok()? };
        let az_style_border_top_width_value_none = unsafe { lib.get::<extern fn() -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_none").ok()? };
        let az_style_border_top_width_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_inherit").ok()? };
        let az_style_border_top_width_value_initial = unsafe { lib.get::<extern fn() -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_initial").ok()? };
        let az_style_border_top_width_value_exact = unsafe { lib.get::<extern fn(_: AzStyleBorderTopWidth) -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_exact").ok()? };
        let az_style_border_top_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopWidthValue)>(b"az_style_border_top_width_value_delete").ok()? };
        let az_style_border_top_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopWidthValue) -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_deep_copy").ok()? };
        let az_style_cursor_value_auto = unsafe { lib.get::<extern fn() -> AzStyleCursorValue>(b"az_style_cursor_value_auto").ok()? };
        let az_style_cursor_value_none = unsafe { lib.get::<extern fn() -> AzStyleCursorValue>(b"az_style_cursor_value_none").ok()? };
        let az_style_cursor_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleCursorValue>(b"az_style_cursor_value_inherit").ok()? };
        let az_style_cursor_value_initial = unsafe { lib.get::<extern fn() -> AzStyleCursorValue>(b"az_style_cursor_value_initial").ok()? };
        let az_style_cursor_value_exact = unsafe { lib.get::<extern fn(_: AzStyleCursor) -> AzStyleCursorValue>(b"az_style_cursor_value_exact").ok()? };
        let az_style_cursor_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleCursorValue)>(b"az_style_cursor_value_delete").ok()? };
        let az_style_cursor_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleCursorValue) -> AzStyleCursorValue>(b"az_style_cursor_value_deep_copy").ok()? };
        let az_style_font_family_value_auto = unsafe { lib.get::<extern fn() -> AzStyleFontFamilyValue>(b"az_style_font_family_value_auto").ok()? };
        let az_style_font_family_value_none = unsafe { lib.get::<extern fn() -> AzStyleFontFamilyValue>(b"az_style_font_family_value_none").ok()? };
        let az_style_font_family_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleFontFamilyValue>(b"az_style_font_family_value_inherit").ok()? };
        let az_style_font_family_value_initial = unsafe { lib.get::<extern fn() -> AzStyleFontFamilyValue>(b"az_style_font_family_value_initial").ok()? };
        let az_style_font_family_value_exact = unsafe { lib.get::<extern fn(_: AzStyleFontFamily) -> AzStyleFontFamilyValue>(b"az_style_font_family_value_exact").ok()? };
        let az_style_font_family_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleFontFamilyValue)>(b"az_style_font_family_value_delete").ok()? };
        let az_style_font_family_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue>(b"az_style_font_family_value_deep_copy").ok()? };
        let az_style_font_size_value_auto = unsafe { lib.get::<extern fn() -> AzStyleFontSizeValue>(b"az_style_font_size_value_auto").ok()? };
        let az_style_font_size_value_none = unsafe { lib.get::<extern fn() -> AzStyleFontSizeValue>(b"az_style_font_size_value_none").ok()? };
        let az_style_font_size_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleFontSizeValue>(b"az_style_font_size_value_inherit").ok()? };
        let az_style_font_size_value_initial = unsafe { lib.get::<extern fn() -> AzStyleFontSizeValue>(b"az_style_font_size_value_initial").ok()? };
        let az_style_font_size_value_exact = unsafe { lib.get::<extern fn(_: AzStyleFontSize) -> AzStyleFontSizeValue>(b"az_style_font_size_value_exact").ok()? };
        let az_style_font_size_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleFontSizeValue)>(b"az_style_font_size_value_delete").ok()? };
        let az_style_font_size_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleFontSizeValue) -> AzStyleFontSizeValue>(b"az_style_font_size_value_deep_copy").ok()? };
        let az_style_letter_spacing_value_auto = unsafe { lib.get::<extern fn() -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_auto").ok()? };
        let az_style_letter_spacing_value_none = unsafe { lib.get::<extern fn() -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_none").ok()? };
        let az_style_letter_spacing_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_inherit").ok()? };
        let az_style_letter_spacing_value_initial = unsafe { lib.get::<extern fn() -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_initial").ok()? };
        let az_style_letter_spacing_value_exact = unsafe { lib.get::<extern fn(_: AzStyleLetterSpacing) -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_exact").ok()? };
        let az_style_letter_spacing_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleLetterSpacingValue)>(b"az_style_letter_spacing_value_delete").ok()? };
        let az_style_letter_spacing_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleLetterSpacingValue) -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_deep_copy").ok()? };
        let az_style_line_height_value_auto = unsafe { lib.get::<extern fn() -> AzStyleLineHeightValue>(b"az_style_line_height_value_auto").ok()? };
        let az_style_line_height_value_none = unsafe { lib.get::<extern fn() -> AzStyleLineHeightValue>(b"az_style_line_height_value_none").ok()? };
        let az_style_line_height_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleLineHeightValue>(b"az_style_line_height_value_inherit").ok()? };
        let az_style_line_height_value_initial = unsafe { lib.get::<extern fn() -> AzStyleLineHeightValue>(b"az_style_line_height_value_initial").ok()? };
        let az_style_line_height_value_exact = unsafe { lib.get::<extern fn(_: AzStyleLineHeight) -> AzStyleLineHeightValue>(b"az_style_line_height_value_exact").ok()? };
        let az_style_line_height_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleLineHeightValue)>(b"az_style_line_height_value_delete").ok()? };
        let az_style_line_height_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleLineHeightValue) -> AzStyleLineHeightValue>(b"az_style_line_height_value_deep_copy").ok()? };
        let az_style_tab_width_value_auto = unsafe { lib.get::<extern fn() -> AzStyleTabWidthValue>(b"az_style_tab_width_value_auto").ok()? };
        let az_style_tab_width_value_none = unsafe { lib.get::<extern fn() -> AzStyleTabWidthValue>(b"az_style_tab_width_value_none").ok()? };
        let az_style_tab_width_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleTabWidthValue>(b"az_style_tab_width_value_inherit").ok()? };
        let az_style_tab_width_value_initial = unsafe { lib.get::<extern fn() -> AzStyleTabWidthValue>(b"az_style_tab_width_value_initial").ok()? };
        let az_style_tab_width_value_exact = unsafe { lib.get::<extern fn(_: AzStyleTabWidth) -> AzStyleTabWidthValue>(b"az_style_tab_width_value_exact").ok()? };
        let az_style_tab_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTabWidthValue)>(b"az_style_tab_width_value_delete").ok()? };
        let az_style_tab_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTabWidthValue) -> AzStyleTabWidthValue>(b"az_style_tab_width_value_deep_copy").ok()? };
        let az_style_text_alignment_horz_value_auto = unsafe { lib.get::<extern fn() -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_auto").ok()? };
        let az_style_text_alignment_horz_value_none = unsafe { lib.get::<extern fn() -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_none").ok()? };
        let az_style_text_alignment_horz_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_inherit").ok()? };
        let az_style_text_alignment_horz_value_initial = unsafe { lib.get::<extern fn() -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_initial").ok()? };
        let az_style_text_alignment_horz_value_exact = unsafe { lib.get::<extern fn(_: AzStyleTextAlignmentHorz) -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_exact").ok()? };
        let az_style_text_alignment_horz_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTextAlignmentHorzValue)>(b"az_style_text_alignment_horz_value_delete").ok()? };
        let az_style_text_alignment_horz_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTextAlignmentHorzValue) -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_deep_copy").ok()? };
        let az_style_text_color_value_auto = unsafe { lib.get::<extern fn() -> AzStyleTextColorValue>(b"az_style_text_color_value_auto").ok()? };
        let az_style_text_color_value_none = unsafe { lib.get::<extern fn() -> AzStyleTextColorValue>(b"az_style_text_color_value_none").ok()? };
        let az_style_text_color_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleTextColorValue>(b"az_style_text_color_value_inherit").ok()? };
        let az_style_text_color_value_initial = unsafe { lib.get::<extern fn() -> AzStyleTextColorValue>(b"az_style_text_color_value_initial").ok()? };
        let az_style_text_color_value_exact = unsafe { lib.get::<extern fn(_: AzStyleTextColor) -> AzStyleTextColorValue>(b"az_style_text_color_value_exact").ok()? };
        let az_style_text_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTextColorValue)>(b"az_style_text_color_value_delete").ok()? };
        let az_style_text_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTextColorValue) -> AzStyleTextColorValue>(b"az_style_text_color_value_deep_copy").ok()? };
        let az_style_word_spacing_value_auto = unsafe { lib.get::<extern fn() -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_auto").ok()? };
        let az_style_word_spacing_value_none = unsafe { lib.get::<extern fn() -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_none").ok()? };
        let az_style_word_spacing_value_inherit = unsafe { lib.get::<extern fn() -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_inherit").ok()? };
        let az_style_word_spacing_value_initial = unsafe { lib.get::<extern fn() -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_initial").ok()? };
        let az_style_word_spacing_value_exact = unsafe { lib.get::<extern fn(_: AzStyleWordSpacing) -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_exact").ok()? };
        let az_style_word_spacing_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleWordSpacingValue)>(b"az_style_word_spacing_value_delete").ok()? };
        let az_style_word_spacing_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleWordSpacingValue) -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_deep_copy").ok()? };
        let az_css_property_text_color = unsafe { lib.get::<extern fn(_: AzStyleTextColorValue) -> AzCssProperty>(b"az_css_property_text_color").ok()? };
        let az_css_property_font_size = unsafe { lib.get::<extern fn(_: AzStyleFontSizeValue) -> AzCssProperty>(b"az_css_property_font_size").ok()? };
        let az_css_property_font_family = unsafe { lib.get::<extern fn(_: AzStyleFontFamilyValue) -> AzCssProperty>(b"az_css_property_font_family").ok()? };
        let az_css_property_text_align = unsafe { lib.get::<extern fn(_: AzStyleTextAlignmentHorzValue) -> AzCssProperty>(b"az_css_property_text_align").ok()? };
        let az_css_property_letter_spacing = unsafe { lib.get::<extern fn(_: AzStyleLetterSpacingValue) -> AzCssProperty>(b"az_css_property_letter_spacing").ok()? };
        let az_css_property_line_height = unsafe { lib.get::<extern fn(_: AzStyleLineHeightValue) -> AzCssProperty>(b"az_css_property_line_height").ok()? };
        let az_css_property_word_spacing = unsafe { lib.get::<extern fn(_: AzStyleWordSpacingValue) -> AzCssProperty>(b"az_css_property_word_spacing").ok()? };
        let az_css_property_tab_width = unsafe { lib.get::<extern fn(_: AzStyleTabWidthValue) -> AzCssProperty>(b"az_css_property_tab_width").ok()? };
        let az_css_property_cursor = unsafe { lib.get::<extern fn(_: AzStyleCursorValue) -> AzCssProperty>(b"az_css_property_cursor").ok()? };
        let az_css_property_display = unsafe { lib.get::<extern fn(_: AzLayoutDisplayValue) -> AzCssProperty>(b"az_css_property_display").ok()? };
        let az_css_property_float = unsafe { lib.get::<extern fn(_: AzLayoutFloatValue) -> AzCssProperty>(b"az_css_property_float").ok()? };
        let az_css_property_box_sizing = unsafe { lib.get::<extern fn(_: AzLayoutBoxSizingValue) -> AzCssProperty>(b"az_css_property_box_sizing").ok()? };
        let az_css_property_width = unsafe { lib.get::<extern fn(_: AzLayoutWidthValue) -> AzCssProperty>(b"az_css_property_width").ok()? };
        let az_css_property_height = unsafe { lib.get::<extern fn(_: AzLayoutHeightValue) -> AzCssProperty>(b"az_css_property_height").ok()? };
        let az_css_property_min_width = unsafe { lib.get::<extern fn(_: AzLayoutMinWidthValue) -> AzCssProperty>(b"az_css_property_min_width").ok()? };
        let az_css_property_min_height = unsafe { lib.get::<extern fn(_: AzLayoutMinHeightValue) -> AzCssProperty>(b"az_css_property_min_height").ok()? };
        let az_css_property_max_width = unsafe { lib.get::<extern fn(_: AzLayoutMaxWidthValue) -> AzCssProperty>(b"az_css_property_max_width").ok()? };
        let az_css_property_max_height = unsafe { lib.get::<extern fn(_: AzLayoutMaxHeightValue) -> AzCssProperty>(b"az_css_property_max_height").ok()? };
        let az_css_property_position = unsafe { lib.get::<extern fn(_: AzLayoutPositionValue) -> AzCssProperty>(b"az_css_property_position").ok()? };
        let az_css_property_top = unsafe { lib.get::<extern fn(_: AzLayoutTopValue) -> AzCssProperty>(b"az_css_property_top").ok()? };
        let az_css_property_right = unsafe { lib.get::<extern fn(_: AzLayoutRightValue) -> AzCssProperty>(b"az_css_property_right").ok()? };
        let az_css_property_left = unsafe { lib.get::<extern fn(_: AzLayoutLeftValue) -> AzCssProperty>(b"az_css_property_left").ok()? };
        let az_css_property_bottom = unsafe { lib.get::<extern fn(_: AzLayoutBottomValue) -> AzCssProperty>(b"az_css_property_bottom").ok()? };
        let az_css_property_flex_wrap = unsafe { lib.get::<extern fn(_: AzLayoutWrapValue) -> AzCssProperty>(b"az_css_property_flex_wrap").ok()? };
        let az_css_property_flex_direction = unsafe { lib.get::<extern fn(_: AzLayoutDirectionValue) -> AzCssProperty>(b"az_css_property_flex_direction").ok()? };
        let az_css_property_flex_grow = unsafe { lib.get::<extern fn(_: AzLayoutFlexGrowValue) -> AzCssProperty>(b"az_css_property_flex_grow").ok()? };
        let az_css_property_flex_shrink = unsafe { lib.get::<extern fn(_: AzLayoutFlexShrinkValue) -> AzCssProperty>(b"az_css_property_flex_shrink").ok()? };
        let az_css_property_justify_content = unsafe { lib.get::<extern fn(_: AzLayoutJustifyContentValue) -> AzCssProperty>(b"az_css_property_justify_content").ok()? };
        let az_css_property_align_items = unsafe { lib.get::<extern fn(_: AzLayoutAlignItemsValue) -> AzCssProperty>(b"az_css_property_align_items").ok()? };
        let az_css_property_align_content = unsafe { lib.get::<extern fn(_: AzLayoutAlignContentValue) -> AzCssProperty>(b"az_css_property_align_content").ok()? };
        let az_css_property_background_content = unsafe { lib.get::<extern fn(_: AzStyleBackgroundContentValue) -> AzCssProperty>(b"az_css_property_background_content").ok()? };
        let az_css_property_background_position = unsafe { lib.get::<extern fn(_: AzStyleBackgroundPositionValue) -> AzCssProperty>(b"az_css_property_background_position").ok()? };
        let az_css_property_background_size = unsafe { lib.get::<extern fn(_: AzStyleBackgroundSizeValue) -> AzCssProperty>(b"az_css_property_background_size").ok()? };
        let az_css_property_background_repeat = unsafe { lib.get::<extern fn(_: AzStyleBackgroundRepeatValue) -> AzCssProperty>(b"az_css_property_background_repeat").ok()? };
        let az_css_property_overflow_x = unsafe { lib.get::<extern fn(_: AzOverflowValue) -> AzCssProperty>(b"az_css_property_overflow_x").ok()? };
        let az_css_property_overflow_y = unsafe { lib.get::<extern fn(_: AzOverflowValue) -> AzCssProperty>(b"az_css_property_overflow_y").ok()? };
        let az_css_property_padding_top = unsafe { lib.get::<extern fn(_: AzLayoutPaddingTopValue) -> AzCssProperty>(b"az_css_property_padding_top").ok()? };
        let az_css_property_padding_left = unsafe { lib.get::<extern fn(_: AzLayoutPaddingLeftValue) -> AzCssProperty>(b"az_css_property_padding_left").ok()? };
        let az_css_property_padding_right = unsafe { lib.get::<extern fn(_: AzLayoutPaddingRightValue) -> AzCssProperty>(b"az_css_property_padding_right").ok()? };
        let az_css_property_padding_bottom = unsafe { lib.get::<extern fn(_: AzLayoutPaddingBottomValue) -> AzCssProperty>(b"az_css_property_padding_bottom").ok()? };
        let az_css_property_margin_top = unsafe { lib.get::<extern fn(_: AzLayoutMarginTopValue) -> AzCssProperty>(b"az_css_property_margin_top").ok()? };
        let az_css_property_margin_left = unsafe { lib.get::<extern fn(_: AzLayoutMarginLeftValue) -> AzCssProperty>(b"az_css_property_margin_left").ok()? };
        let az_css_property_margin_right = unsafe { lib.get::<extern fn(_: AzLayoutMarginRightValue) -> AzCssProperty>(b"az_css_property_margin_right").ok()? };
        let az_css_property_margin_bottom = unsafe { lib.get::<extern fn(_: AzLayoutMarginBottomValue) -> AzCssProperty>(b"az_css_property_margin_bottom").ok()? };
        let az_css_property_border_top_left_radius = unsafe { lib.get::<extern fn(_: AzStyleBorderTopLeftRadiusValue) -> AzCssProperty>(b"az_css_property_border_top_left_radius").ok()? };
        let az_css_property_border_top_right_radius = unsafe { lib.get::<extern fn(_: AzStyleBorderTopRightRadiusValue) -> AzCssProperty>(b"az_css_property_border_top_right_radius").ok()? };
        let az_css_property_border_bottom_left_radius = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomLeftRadiusValue) -> AzCssProperty>(b"az_css_property_border_bottom_left_radius").ok()? };
        let az_css_property_border_bottom_right_radius = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomRightRadiusValue) -> AzCssProperty>(b"az_css_property_border_bottom_right_radius").ok()? };
        let az_css_property_border_top_color = unsafe { lib.get::<extern fn(_: AzStyleBorderTopColorValue) -> AzCssProperty>(b"az_css_property_border_top_color").ok()? };
        let az_css_property_border_right_color = unsafe { lib.get::<extern fn(_: AzStyleBorderRightColorValue) -> AzCssProperty>(b"az_css_property_border_right_color").ok()? };
        let az_css_property_border_left_color = unsafe { lib.get::<extern fn(_: AzStyleBorderLeftColorValue) -> AzCssProperty>(b"az_css_property_border_left_color").ok()? };
        let az_css_property_border_bottom_color = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomColorValue) -> AzCssProperty>(b"az_css_property_border_bottom_color").ok()? };
        let az_css_property_border_top_style = unsafe { lib.get::<extern fn(_: AzStyleBorderTopStyleValue) -> AzCssProperty>(b"az_css_property_border_top_style").ok()? };
        let az_css_property_border_right_style = unsafe { lib.get::<extern fn(_: AzStyleBorderRightStyleValue) -> AzCssProperty>(b"az_css_property_border_right_style").ok()? };
        let az_css_property_border_left_style = unsafe { lib.get::<extern fn(_: AzStyleBorderLeftStyleValue) -> AzCssProperty>(b"az_css_property_border_left_style").ok()? };
        let az_css_property_border_bottom_style = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomStyleValue) -> AzCssProperty>(b"az_css_property_border_bottom_style").ok()? };
        let az_css_property_border_top_width = unsafe { lib.get::<extern fn(_: AzStyleBorderTopWidthValue) -> AzCssProperty>(b"az_css_property_border_top_width").ok()? };
        let az_css_property_border_right_width = unsafe { lib.get::<extern fn(_: AzStyleBorderRightWidthValue) -> AzCssProperty>(b"az_css_property_border_right_width").ok()? };
        let az_css_property_border_left_width = unsafe { lib.get::<extern fn(_: AzStyleBorderLeftWidthValue) -> AzCssProperty>(b"az_css_property_border_left_width").ok()? };
        let az_css_property_border_bottom_width = unsafe { lib.get::<extern fn(_: AzStyleBorderBottomWidthValue) -> AzCssProperty>(b"az_css_property_border_bottom_width").ok()? };
        let az_css_property_box_shadow_left = unsafe { lib.get::<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>(b"az_css_property_box_shadow_left").ok()? };
        let az_css_property_box_shadow_right = unsafe { lib.get::<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>(b"az_css_property_box_shadow_right").ok()? };
        let az_css_property_box_shadow_top = unsafe { lib.get::<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>(b"az_css_property_box_shadow_top").ok()? };
        let az_css_property_box_shadow_bottom = unsafe { lib.get::<extern fn(_: AzBoxShadowPreDisplayItemValue) -> AzCssProperty>(b"az_css_property_box_shadow_bottom").ok()? };
        let az_css_property_delete = unsafe { lib.get::<extern fn(_: &mut AzCssProperty)>(b"az_css_property_delete").ok()? };
        let az_css_property_deep_copy = unsafe { lib.get::<extern fn(_: &AzCssProperty) -> AzCssProperty>(b"az_css_property_deep_copy").ok()? };
        let az_dom_div = unsafe { lib.get::<extern fn() -> AzDomPtr>(b"az_dom_div").ok()? };
        let az_dom_body = unsafe { lib.get::<extern fn() -> AzDomPtr>(b"az_dom_body").ok()? };
        let az_dom_label = unsafe { lib.get::<extern fn(_: AzString) -> AzDomPtr>(b"az_dom_label").ok()? };
        let az_dom_text = unsafe { lib.get::<extern fn(_: AzTextId) -> AzDomPtr>(b"az_dom_text").ok()? };
        let az_dom_image = unsafe { lib.get::<extern fn(_: AzImageId) -> AzDomPtr>(b"az_dom_image").ok()? };
        let az_dom_gl_texture = unsafe { lib.get::<extern fn(_: AzGlCallback) -> AzDomPtr>(b"az_dom_gl_texture").ok()? };
        let az_dom_iframe_callback = unsafe { lib.get::<extern fn(_: AzIFrameCallback) -> AzDomPtr>(b"az_dom_iframe_callback").ok()? };
        let az_dom_add_id = unsafe { lib.get::<extern fn(_: AzString)>(b"az_dom_add_id").ok()? };
        let az_dom_with_id = unsafe { lib.get::<extern fn(_: AzString) -> AzDomPtr>(b"az_dom_with_id").ok()? };
        let az_dom_set_ids = unsafe { lib.get::<extern fn(_: AzStringVec)>(b"az_dom_set_ids").ok()? };
        let az_dom_with_ids = unsafe { lib.get::<extern fn(_: AzStringVec) -> AzDomPtr>(b"az_dom_with_ids").ok()? };
        let az_dom_add_class = unsafe { lib.get::<extern fn(_: AzString)>(b"az_dom_add_class").ok()? };
        let az_dom_with_class = unsafe { lib.get::<extern fn(_: AzString) -> AzDomPtr>(b"az_dom_with_class").ok()? };
        let az_dom_set_classes = unsafe { lib.get::<extern fn(_: AzStringVec)>(b"az_dom_set_classes").ok()? };
        let az_dom_with_classes = unsafe { lib.get::<extern fn(_: AzStringVec) -> AzDomPtr>(b"az_dom_with_classes").ok()? };
        let az_dom_add_callback = unsafe { lib.get::<extern fn(_: AzCallback)>(b"az_dom_add_callback").ok()? };
        let az_dom_with_callback = unsafe { lib.get::<extern fn(_: AzCallback) -> AzDomPtr>(b"az_dom_with_callback").ok()? };
        let az_dom_add_css_override = unsafe { lib.get::<extern fn(_: AzCssProperty)>(b"az_dom_add_css_override").ok()? };
        let az_dom_with_css_override = unsafe { lib.get::<extern fn(_: AzCssProperty) -> AzDomPtr>(b"az_dom_with_css_override").ok()? };
        let az_dom_set_is_draggable = unsafe { lib.get::<extern fn(_: bool)>(b"az_dom_set_is_draggable").ok()? };
        let az_dom_is_draggable = unsafe { lib.get::<extern fn(_: bool) -> AzDomPtr>(b"az_dom_is_draggable").ok()? };
        let az_dom_set_tab_index = unsafe { lib.get::<extern fn(_: AzTabIndex)>(b"az_dom_set_tab_index").ok()? };
        let az_dom_with_tab_index = unsafe { lib.get::<extern fn(_: AzTabIndex) -> AzDomPtr>(b"az_dom_with_tab_index").ok()? };
        let az_dom_add_child = unsafe { lib.get::<extern fn(_: AzDomPtr)>(b"az_dom_add_child").ok()? };
        let az_dom_with_child = unsafe { lib.get::<extern fn(_: AzDomPtr) -> AzDomPtr>(b"az_dom_with_child").ok()? };
        let az_dom_has_id = unsafe { lib.get::<extern fn(_: AzString) -> bool>(b"az_dom_has_id").ok()? };
        let az_dom_has_class = unsafe { lib.get::<extern fn(_: AzString) -> bool>(b"az_dom_has_class").ok()? };
        let az_dom_get_html_string = unsafe { lib.get::<extern fn(_: &mut AzDomPtr) -> AzString>(b"az_dom_get_html_string").ok()? };
        let az_dom_delete = unsafe { lib.get::<extern fn(_: &mut AzDomPtr)>(b"az_dom_delete").ok()? };
        let az_dom_shallow_copy = unsafe { lib.get::<extern fn(_: &AzDomPtr) -> AzDomPtr>(b"az_dom_shallow_copy").ok()? };
        let az_event_filter_hover = unsafe { lib.get::<extern fn(_: AzHoverEventFilter) -> AzEventFilter>(b"az_event_filter_hover").ok()? };
        let az_event_filter_not = unsafe { lib.get::<extern fn(_: AzNotEventFilter) -> AzEventFilter>(b"az_event_filter_not").ok()? };
        let az_event_filter_focus = unsafe { lib.get::<extern fn(_: AzFocusEventFilter) -> AzEventFilter>(b"az_event_filter_focus").ok()? };
        let az_event_filter_window = unsafe { lib.get::<extern fn(_: AzWindowEventFilter) -> AzEventFilter>(b"az_event_filter_window").ok()? };
        let az_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzEventFilter)>(b"az_event_filter_delete").ok()? };
        let az_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzEventFilter) -> AzEventFilter>(b"az_event_filter_deep_copy").ok()? };
        let az_hover_event_filter_mouse_over = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_mouse_over").ok()? };
        let az_hover_event_filter_mouse_down = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_mouse_down").ok()? };
        let az_hover_event_filter_left_mouse_down = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_left_mouse_down").ok()? };
        let az_hover_event_filter_right_mouse_down = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_right_mouse_down").ok()? };
        let az_hover_event_filter_middle_mouse_down = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_middle_mouse_down").ok()? };
        let az_hover_event_filter_mouse_up = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_mouse_up").ok()? };
        let az_hover_event_filter_left_mouse_up = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_left_mouse_up").ok()? };
        let az_hover_event_filter_right_mouse_up = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_right_mouse_up").ok()? };
        let az_hover_event_filter_middle_mouse_up = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_middle_mouse_up").ok()? };
        let az_hover_event_filter_mouse_enter = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_mouse_enter").ok()? };
        let az_hover_event_filter_mouse_leave = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_mouse_leave").ok()? };
        let az_hover_event_filter_scroll = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_scroll").ok()? };
        let az_hover_event_filter_scroll_start = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_scroll_start").ok()? };
        let az_hover_event_filter_scroll_end = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_scroll_end").ok()? };
        let az_hover_event_filter_text_input = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_text_input").ok()? };
        let az_hover_event_filter_virtual_key_down = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_virtual_key_down").ok()? };
        let az_hover_event_filter_virtual_key_up = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_virtual_key_up").ok()? };
        let az_hover_event_filter_hovered_file = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_hovered_file").ok()? };
        let az_hover_event_filter_dropped_file = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_dropped_file").ok()? };
        let az_hover_event_filter_hovered_file_cancelled = unsafe { lib.get::<extern fn() -> AzHoverEventFilter>(b"az_hover_event_filter_hovered_file_cancelled").ok()? };
        let az_hover_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzHoverEventFilter)>(b"az_hover_event_filter_delete").ok()? };
        let az_hover_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzHoverEventFilter) -> AzHoverEventFilter>(b"az_hover_event_filter_deep_copy").ok()? };
        let az_focus_event_filter_mouse_over = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_mouse_over").ok()? };
        let az_focus_event_filter_mouse_down = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_mouse_down").ok()? };
        let az_focus_event_filter_left_mouse_down = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_left_mouse_down").ok()? };
        let az_focus_event_filter_right_mouse_down = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_right_mouse_down").ok()? };
        let az_focus_event_filter_middle_mouse_down = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_middle_mouse_down").ok()? };
        let az_focus_event_filter_mouse_up = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_mouse_up").ok()? };
        let az_focus_event_filter_left_mouse_up = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_left_mouse_up").ok()? };
        let az_focus_event_filter_right_mouse_up = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_right_mouse_up").ok()? };
        let az_focus_event_filter_middle_mouse_up = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_middle_mouse_up").ok()? };
        let az_focus_event_filter_mouse_enter = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_mouse_enter").ok()? };
        let az_focus_event_filter_mouse_leave = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_mouse_leave").ok()? };
        let az_focus_event_filter_scroll = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_scroll").ok()? };
        let az_focus_event_filter_scroll_start = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_scroll_start").ok()? };
        let az_focus_event_filter_scroll_end = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_scroll_end").ok()? };
        let az_focus_event_filter_text_input = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_text_input").ok()? };
        let az_focus_event_filter_virtual_key_down = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_virtual_key_down").ok()? };
        let az_focus_event_filter_virtual_key_up = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_virtual_key_up").ok()? };
        let az_focus_event_filter_focus_received = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_focus_received").ok()? };
        let az_focus_event_filter_focus_lost = unsafe { lib.get::<extern fn() -> AzFocusEventFilter>(b"az_focus_event_filter_focus_lost").ok()? };
        let az_focus_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzFocusEventFilter)>(b"az_focus_event_filter_delete").ok()? };
        let az_focus_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzFocusEventFilter) -> AzFocusEventFilter>(b"az_focus_event_filter_deep_copy").ok()? };
        let az_not_event_filter_hover = unsafe { lib.get::<extern fn(_: AzHoverEventFilter) -> AzNotEventFilter>(b"az_not_event_filter_hover").ok()? };
        let az_not_event_filter_focus = unsafe { lib.get::<extern fn(_: AzFocusEventFilter) -> AzNotEventFilter>(b"az_not_event_filter_focus").ok()? };
        let az_not_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzNotEventFilter)>(b"az_not_event_filter_delete").ok()? };
        let az_not_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzNotEventFilter) -> AzNotEventFilter>(b"az_not_event_filter_deep_copy").ok()? };
        let az_window_event_filter_mouse_over = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_mouse_over").ok()? };
        let az_window_event_filter_mouse_down = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_mouse_down").ok()? };
        let az_window_event_filter_left_mouse_down = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_left_mouse_down").ok()? };
        let az_window_event_filter_right_mouse_down = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_right_mouse_down").ok()? };
        let az_window_event_filter_middle_mouse_down = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_middle_mouse_down").ok()? };
        let az_window_event_filter_mouse_up = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_mouse_up").ok()? };
        let az_window_event_filter_left_mouse_up = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_left_mouse_up").ok()? };
        let az_window_event_filter_right_mouse_up = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_right_mouse_up").ok()? };
        let az_window_event_filter_middle_mouse_up = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_middle_mouse_up").ok()? };
        let az_window_event_filter_mouse_enter = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_mouse_enter").ok()? };
        let az_window_event_filter_mouse_leave = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_mouse_leave").ok()? };
        let az_window_event_filter_scroll = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_scroll").ok()? };
        let az_window_event_filter_scroll_start = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_scroll_start").ok()? };
        let az_window_event_filter_scroll_end = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_scroll_end").ok()? };
        let az_window_event_filter_text_input = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_text_input").ok()? };
        let az_window_event_filter_virtual_key_down = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_virtual_key_down").ok()? };
        let az_window_event_filter_virtual_key_up = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_virtual_key_up").ok()? };
        let az_window_event_filter_hovered_file = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_hovered_file").ok()? };
        let az_window_event_filter_dropped_file = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_dropped_file").ok()? };
        let az_window_event_filter_hovered_file_cancelled = unsafe { lib.get::<extern fn() -> AzWindowEventFilter>(b"az_window_event_filter_hovered_file_cancelled").ok()? };
        let az_window_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzWindowEventFilter)>(b"az_window_event_filter_delete").ok()? };
        let az_window_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzWindowEventFilter) -> AzWindowEventFilter>(b"az_window_event_filter_deep_copy").ok()? };
        let az_tab_index_auto = unsafe { lib.get::<extern fn() -> AzTabIndex>(b"az_tab_index_auto").ok()? };
        let az_tab_index_override_in_parent = unsafe { lib.get::<extern fn(_: usize) -> AzTabIndex>(b"az_tab_index_override_in_parent").ok()? };
        let az_tab_index_no_keyboard_focus = unsafe { lib.get::<extern fn() -> AzTabIndex>(b"az_tab_index_no_keyboard_focus").ok()? };
        let az_tab_index_delete = unsafe { lib.get::<extern fn(_: &mut AzTabIndex)>(b"az_tab_index_delete").ok()? };
        let az_tab_index_deep_copy = unsafe { lib.get::<extern fn(_: &AzTabIndex) -> AzTabIndex>(b"az_tab_index_deep_copy").ok()? };
        let az_text_id_new = unsafe { lib.get::<extern fn() -> AzTextId>(b"az_text_id_new").ok()? };
        let az_text_id_delete = unsafe { lib.get::<extern fn(_: &mut AzTextId)>(b"az_text_id_delete").ok()? };
        let az_text_id_deep_copy = unsafe { lib.get::<extern fn(_: &AzTextId) -> AzTextId>(b"az_text_id_deep_copy").ok()? };
        let az_image_id_new = unsafe { lib.get::<extern fn() -> AzImageId>(b"az_image_id_new").ok()? };
        let az_image_id_delete = unsafe { lib.get::<extern fn(_: &mut AzImageId)>(b"az_image_id_delete").ok()? };
        let az_image_id_deep_copy = unsafe { lib.get::<extern fn(_: &AzImageId) -> AzImageId>(b"az_image_id_deep_copy").ok()? };
        let az_font_id_new = unsafe { lib.get::<extern fn() -> AzFontId>(b"az_font_id_new").ok()? };
        let az_font_id_delete = unsafe { lib.get::<extern fn(_: &mut AzFontId)>(b"az_font_id_delete").ok()? };
        let az_font_id_deep_copy = unsafe { lib.get::<extern fn(_: &AzFontId) -> AzFontId>(b"az_font_id_deep_copy").ok()? };
        let az_image_source_embedded = unsafe { lib.get::<extern fn(_: AzU8Vec) -> AzImageSource>(b"az_image_source_embedded").ok()? };
        let az_image_source_file = unsafe { lib.get::<extern fn(_: AzPathBufPtr) -> AzImageSource>(b"az_image_source_file").ok()? };
        let az_image_source_raw = unsafe { lib.get::<extern fn(_: AzRawImagePtr) -> AzImageSource>(b"az_image_source_raw").ok()? };
        let az_image_source_delete = unsafe { lib.get::<extern fn(_: &mut AzImageSource)>(b"az_image_source_delete").ok()? };
        let az_image_source_deep_copy = unsafe { lib.get::<extern fn(_: &AzImageSource) -> AzImageSource>(b"az_image_source_deep_copy").ok()? };
        let az_font_source_embedded = unsafe { lib.get::<extern fn(_: AzU8Vec) -> AzFontSource>(b"az_font_source_embedded").ok()? };
        let az_font_source_file = unsafe { lib.get::<extern fn(_: AzPathBufPtr) -> AzFontSource>(b"az_font_source_file").ok()? };
        let az_font_source_system = unsafe { lib.get::<extern fn(_: AzString) -> AzFontSource>(b"az_font_source_system").ok()? };
        let az_font_source_delete = unsafe { lib.get::<extern fn(_: &mut AzFontSource)>(b"az_font_source_delete").ok()? };
        let az_font_source_deep_copy = unsafe { lib.get::<extern fn(_: &AzFontSource) -> AzFontSource>(b"az_font_source_deep_copy").ok()? };
        let az_raw_image_new = unsafe { lib.get::<extern fn(_: AzRawImageFormat) -> AzRawImagePtr>(b"az_raw_image_new").ok()? };
        let az_raw_image_delete = unsafe { lib.get::<extern fn(_: &mut AzRawImagePtr)>(b"az_raw_image_delete").ok()? };
        let az_raw_image_shallow_copy = unsafe { lib.get::<extern fn(_: &AzRawImagePtr) -> AzRawImagePtr>(b"az_raw_image_shallow_copy").ok()? };
        let az_raw_image_format_r8 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_r8").ok()? };
        let az_raw_image_format_r16 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_r16").ok()? };
        let az_raw_image_format_rg16 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_rg16").ok()? };
        let az_raw_image_format_bgra8 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_bgra8").ok()? };
        let az_raw_image_format_rgbaf32 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_rgbaf32").ok()? };
        let az_raw_image_format_rg8 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_rg8").ok()? };
        let az_raw_image_format_rgbai32 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_rgbai32").ok()? };
        let az_raw_image_format_rgba8 = unsafe { lib.get::<extern fn() -> AzRawImageFormat>(b"az_raw_image_format_rgba8").ok()? };
        let az_raw_image_format_delete = unsafe { lib.get::<extern fn(_: &mut AzRawImageFormat)>(b"az_raw_image_format_delete").ok()? };
        let az_raw_image_format_deep_copy = unsafe { lib.get::<extern fn(_: &AzRawImageFormat) -> AzRawImageFormat>(b"az_raw_image_format_deep_copy").ok()? };
        let az_window_create_options_new = unsafe { lib.get::<extern fn(_: AzCssPtr) -> AzWindowCreateOptionsPtr>(b"az_window_create_options_new").ok()? };
        let az_window_create_options_delete = unsafe { lib.get::<extern fn(_: &mut AzWindowCreateOptionsPtr)>(b"az_window_create_options_delete").ok()? };
        let az_window_create_options_shallow_copy = unsafe { lib.get::<extern fn(_: &AzWindowCreateOptionsPtr) -> AzWindowCreateOptionsPtr>(b"az_window_create_options_shallow_copy").ok()? };
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
            az_u8_vec_delete,
            az_u8_vec_deep_copy,
            az_string_vec_copy_from,
            az_string_vec_delete,
            az_string_vec_deep_copy,
            az_gradient_stop_pre_vec_copy_from,
            az_gradient_stop_pre_vec_delete,
            az_gradient_stop_pre_vec_deep_copy,
            az_path_buf_new,
            az_path_buf_delete,
            az_path_buf_shallow_copy,
            az_option_percentage_value_delete,
            az_option_percentage_value_shallow_copy,
            az_app_config_default,
            az_app_config_delete,
            az_app_config_shallow_copy,
            az_app_new,
            az_app_run,
            az_app_delete,
            az_app_shallow_copy,
            az_callback_info_delete,
            az_callback_info_shallow_copy,
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
            az_size_metric_px,
            az_size_metric_pt,
            az_size_metric_em,
            az_size_metric_percent,
            az_size_metric_delete,
            az_size_metric_deep_copy,
            az_float_value_delete,
            az_float_value_deep_copy,
            az_pixel_value_delete,
            az_pixel_value_deep_copy,
            az_pixel_value_no_percent_delete,
            az_pixel_value_no_percent_deep_copy,
            az_box_shadow_clip_mode_outset,
            az_box_shadow_clip_mode_inset,
            az_box_shadow_clip_mode_delete,
            az_box_shadow_clip_mode_deep_copy,
            az_box_shadow_pre_display_item_delete,
            az_box_shadow_pre_display_item_deep_copy,
            az_layout_align_content_stretch,
            az_layout_align_content_center,
            az_layout_align_content_start,
            az_layout_align_content_end,
            az_layout_align_content_space_between,
            az_layout_align_content_space_around,
            az_layout_align_content_delete,
            az_layout_align_content_deep_copy,
            az_layout_align_items_stretch,
            az_layout_align_items_center,
            az_layout_align_items_start,
            az_layout_align_items_end,
            az_layout_align_items_delete,
            az_layout_align_items_deep_copy,
            az_layout_bottom_delete,
            az_layout_bottom_deep_copy,
            az_layout_box_sizing_content_box,
            az_layout_box_sizing_border_box,
            az_layout_box_sizing_delete,
            az_layout_box_sizing_deep_copy,
            az_layout_direction_row,
            az_layout_direction_row_reverse,
            az_layout_direction_column,
            az_layout_direction_column_reverse,
            az_layout_direction_delete,
            az_layout_direction_deep_copy,
            az_layout_display_flex,
            az_layout_display_block,
            az_layout_display_inline_block,
            az_layout_display_delete,
            az_layout_display_deep_copy,
            az_layout_flex_grow_delete,
            az_layout_flex_grow_deep_copy,
            az_layout_flex_shrink_delete,
            az_layout_flex_shrink_deep_copy,
            az_layout_float_left,
            az_layout_float_right,
            az_layout_float_delete,
            az_layout_float_deep_copy,
            az_layout_height_delete,
            az_layout_height_deep_copy,
            az_layout_justify_content_start,
            az_layout_justify_content_end,
            az_layout_justify_content_center,
            az_layout_justify_content_space_between,
            az_layout_justify_content_space_around,
            az_layout_justify_content_space_evenly,
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
            az_layout_position_static,
            az_layout_position_relative,
            az_layout_position_absolute,
            az_layout_position_fixed,
            az_layout_position_delete,
            az_layout_position_deep_copy,
            az_layout_right_delete,
            az_layout_right_deep_copy,
            az_layout_top_delete,
            az_layout_top_deep_copy,
            az_layout_width_delete,
            az_layout_width_deep_copy,
            az_layout_wrap_wrap,
            az_layout_wrap_no_wrap,
            az_layout_wrap_delete,
            az_layout_wrap_deep_copy,
            az_overflow_scroll,
            az_overflow_auto,
            az_overflow_hidden,
            az_overflow_visible,
            az_overflow_delete,
            az_overflow_deep_copy,
            az_percentage_value_delete,
            az_percentage_value_deep_copy,
            az_gradient_stop_pre_delete,
            az_gradient_stop_pre_deep_copy,
            az_direction_corner_right,
            az_direction_corner_left,
            az_direction_corner_top,
            az_direction_corner_bottom,
            az_direction_corner_top_right,
            az_direction_corner_top_left,
            az_direction_corner_bottom_right,
            az_direction_corner_bottom_left,
            az_direction_corner_delete,
            az_direction_corner_deep_copy,
            az_direction_corners_delete,
            az_direction_corners_deep_copy,
            az_direction_angle,
            az_direction_from_to,
            az_direction_delete,
            az_direction_deep_copy,
            az_extend_mode_clamp,
            az_extend_mode_repeat,
            az_extend_mode_delete,
            az_extend_mode_deep_copy,
            az_linear_gradient_delete,
            az_linear_gradient_deep_copy,
            az_shape_ellipse,
            az_shape_circle,
            az_shape_delete,
            az_shape_deep_copy,
            az_radial_gradient_delete,
            az_radial_gradient_deep_copy,
            az_css_image_id_delete,
            az_css_image_id_deep_copy,
            az_style_background_content_linear_gradient,
            az_style_background_content_radial_gradient,
            az_style_background_content_image,
            az_style_background_content_color,
            az_style_background_content_delete,
            az_style_background_content_deep_copy,
            az_background_position_horizontal_left,
            az_background_position_horizontal_center,
            az_background_position_horizontal_right,
            az_background_position_horizontal_exact,
            az_background_position_horizontal_delete,
            az_background_position_horizontal_deep_copy,
            az_background_position_vertical_top,
            az_background_position_vertical_center,
            az_background_position_vertical_bottom,
            az_background_position_vertical_exact,
            az_background_position_vertical_delete,
            az_background_position_vertical_deep_copy,
            az_style_background_position_delete,
            az_style_background_position_deep_copy,
            az_style_background_repeat_no_repeat,
            az_style_background_repeat_repeat,
            az_style_background_repeat_repeat_x,
            az_style_background_repeat_repeat_y,
            az_style_background_repeat_delete,
            az_style_background_repeat_deep_copy,
            az_style_background_size_exact_size,
            az_style_background_size_contain,
            az_style_background_size_cover,
            az_style_background_size_delete,
            az_style_background_size_deep_copy,
            az_style_border_bottom_color_delete,
            az_style_border_bottom_color_deep_copy,
            az_style_border_bottom_left_radius_delete,
            az_style_border_bottom_left_radius_deep_copy,
            az_style_border_bottom_right_radius_delete,
            az_style_border_bottom_right_radius_deep_copy,
            az_border_style_none,
            az_border_style_solid,
            az_border_style_double,
            az_border_style_dotted,
            az_border_style_dashed,
            az_border_style_hidden,
            az_border_style_groove,
            az_border_style_ridge,
            az_border_style_inset,
            az_border_style_outset,
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
            az_style_border_right_width_shallow_copy,
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
            az_style_cursor_alias,
            az_style_cursor_all_scroll,
            az_style_cursor_cell,
            az_style_cursor_col_resize,
            az_style_cursor_context_menu,
            az_style_cursor_copy,
            az_style_cursor_crosshair,
            az_style_cursor_default,
            az_style_cursor_e_resize,
            az_style_cursor_ew_resize,
            az_style_cursor_grab,
            az_style_cursor_grabbing,
            az_style_cursor_help,
            az_style_cursor_move,
            az_style_cursor_n_resize,
            az_style_cursor_ns_resize,
            az_style_cursor_nesw_resize,
            az_style_cursor_nwse_resize,
            az_style_cursor_pointer,
            az_style_cursor_progress,
            az_style_cursor_row_resize,
            az_style_cursor_s_resize,
            az_style_cursor_se_resize,
            az_style_cursor_text,
            az_style_cursor_unset,
            az_style_cursor_vertical_text,
            az_style_cursor_w_resize,
            az_style_cursor_wait,
            az_style_cursor_zoom_in,
            az_style_cursor_zoom_out,
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
            az_style_text_alignment_horz_left,
            az_style_text_alignment_horz_center,
            az_style_text_alignment_horz_right,
            az_style_text_alignment_horz_delete,
            az_style_text_alignment_horz_deep_copy,
            az_style_text_color_delete,
            az_style_text_color_deep_copy,
            az_style_word_spacing_delete,
            az_style_word_spacing_deep_copy,
            az_box_shadow_pre_display_item_value_auto,
            az_box_shadow_pre_display_item_value_none,
            az_box_shadow_pre_display_item_value_inherit,
            az_box_shadow_pre_display_item_value_initial,
            az_box_shadow_pre_display_item_value_exact,
            az_box_shadow_pre_display_item_value_delete,
            az_box_shadow_pre_display_item_value_deep_copy,
            az_layout_align_content_value_auto,
            az_layout_align_content_value_none,
            az_layout_align_content_value_inherit,
            az_layout_align_content_value_initial,
            az_layout_align_content_value_exact,
            az_layout_align_content_value_delete,
            az_layout_align_content_value_deep_copy,
            az_layout_align_items_value_auto,
            az_layout_align_items_value_none,
            az_layout_align_items_value_inherit,
            az_layout_align_items_value_initial,
            az_layout_align_items_value_exact,
            az_layout_align_items_value_delete,
            az_layout_align_items_value_deep_copy,
            az_layout_bottom_value_auto,
            az_layout_bottom_value_none,
            az_layout_bottom_value_inherit,
            az_layout_bottom_value_initial,
            az_layout_bottom_value_exact,
            az_layout_bottom_value_delete,
            az_layout_bottom_value_deep_copy,
            az_layout_box_sizing_value_auto,
            az_layout_box_sizing_value_none,
            az_layout_box_sizing_value_inherit,
            az_layout_box_sizing_value_initial,
            az_layout_box_sizing_value_exact,
            az_layout_box_sizing_value_delete,
            az_layout_box_sizing_value_deep_copy,
            az_layout_direction_value_auto,
            az_layout_direction_value_none,
            az_layout_direction_value_inherit,
            az_layout_direction_value_initial,
            az_layout_direction_value_exact,
            az_layout_direction_value_delete,
            az_layout_direction_value_deep_copy,
            az_layout_display_value_auto,
            az_layout_display_value_none,
            az_layout_display_value_inherit,
            az_layout_display_value_initial,
            az_layout_display_value_exact,
            az_layout_display_value_delete,
            az_layout_display_value_deep_copy,
            az_layout_flex_grow_value_auto,
            az_layout_flex_grow_value_none,
            az_layout_flex_grow_value_inherit,
            az_layout_flex_grow_value_initial,
            az_layout_flex_grow_value_exact,
            az_layout_flex_grow_value_delete,
            az_layout_flex_grow_value_deep_copy,
            az_layout_flex_shrink_value_auto,
            az_layout_flex_shrink_value_none,
            az_layout_flex_shrink_value_inherit,
            az_layout_flex_shrink_value_initial,
            az_layout_flex_shrink_value_exact,
            az_layout_flex_shrink_value_delete,
            az_layout_flex_shrink_value_deep_copy,
            az_layout_float_value_auto,
            az_layout_float_value_none,
            az_layout_float_value_inherit,
            az_layout_float_value_initial,
            az_layout_float_value_exact,
            az_layout_float_value_delete,
            az_layout_float_value_deep_copy,
            az_layout_height_value_auto,
            az_layout_height_value_none,
            az_layout_height_value_inherit,
            az_layout_height_value_initial,
            az_layout_height_value_exact,
            az_layout_height_value_delete,
            az_layout_height_value_deep_copy,
            az_layout_justify_content_value_auto,
            az_layout_justify_content_value_none,
            az_layout_justify_content_value_inherit,
            az_layout_justify_content_value_initial,
            az_layout_justify_content_value_exact,
            az_layout_justify_content_value_delete,
            az_layout_justify_content_value_deep_copy,
            az_layout_left_value_auto,
            az_layout_left_value_none,
            az_layout_left_value_inherit,
            az_layout_left_value_initial,
            az_layout_left_value_exact,
            az_layout_left_value_delete,
            az_layout_left_value_deep_copy,
            az_layout_margin_bottom_value_auto,
            az_layout_margin_bottom_value_none,
            az_layout_margin_bottom_value_inherit,
            az_layout_margin_bottom_value_initial,
            az_layout_margin_bottom_value_exact,
            az_layout_margin_bottom_value_delete,
            az_layout_margin_bottom_value_deep_copy,
            az_layout_margin_left_value_auto,
            az_layout_margin_left_value_none,
            az_layout_margin_left_value_inherit,
            az_layout_margin_left_value_initial,
            az_layout_margin_left_value_exact,
            az_layout_margin_left_value_delete,
            az_layout_margin_left_value_deep_copy,
            az_layout_margin_right_value_auto,
            az_layout_margin_right_value_none,
            az_layout_margin_right_value_inherit,
            az_layout_margin_right_value_initial,
            az_layout_margin_right_value_exact,
            az_layout_margin_right_value_delete,
            az_layout_margin_right_value_deep_copy,
            az_layout_margin_top_value_auto,
            az_layout_margin_top_value_none,
            az_layout_margin_top_value_inherit,
            az_layout_margin_top_value_initial,
            az_layout_margin_top_value_exact,
            az_layout_margin_top_value_delete,
            az_layout_margin_top_value_deep_copy,
            az_layout_max_height_value_auto,
            az_layout_max_height_value_none,
            az_layout_max_height_value_inherit,
            az_layout_max_height_value_initial,
            az_layout_max_height_value_exact,
            az_layout_max_height_value_delete,
            az_layout_max_height_value_deep_copy,
            az_layout_max_width_value_auto,
            az_layout_max_width_value_none,
            az_layout_max_width_value_inherit,
            az_layout_max_width_value_initial,
            az_layout_max_width_value_exact,
            az_layout_max_width_value_delete,
            az_layout_max_width_value_deep_copy,
            az_layout_min_height_value_auto,
            az_layout_min_height_value_none,
            az_layout_min_height_value_inherit,
            az_layout_min_height_value_initial,
            az_layout_min_height_value_exact,
            az_layout_min_height_value_delete,
            az_layout_min_height_value_deep_copy,
            az_layout_min_width_value_auto,
            az_layout_min_width_value_none,
            az_layout_min_width_value_inherit,
            az_layout_min_width_value_initial,
            az_layout_min_width_value_exact,
            az_layout_min_width_value_delete,
            az_layout_min_width_value_deep_copy,
            az_layout_padding_bottom_value_auto,
            az_layout_padding_bottom_value_none,
            az_layout_padding_bottom_value_inherit,
            az_layout_padding_bottom_value_initial,
            az_layout_padding_bottom_value_exact,
            az_layout_padding_bottom_value_delete,
            az_layout_padding_bottom_value_deep_copy,
            az_layout_padding_left_value_auto,
            az_layout_padding_left_value_none,
            az_layout_padding_left_value_inherit,
            az_layout_padding_left_value_initial,
            az_layout_padding_left_value_exact,
            az_layout_padding_left_value_delete,
            az_layout_padding_left_value_deep_copy,
            az_layout_padding_right_value_auto,
            az_layout_padding_right_value_none,
            az_layout_padding_right_value_inherit,
            az_layout_padding_right_value_initial,
            az_layout_padding_right_value_exact,
            az_layout_padding_right_value_delete,
            az_layout_padding_right_value_deep_copy,
            az_layout_padding_top_value_auto,
            az_layout_padding_top_value_none,
            az_layout_padding_top_value_inherit,
            az_layout_padding_top_value_initial,
            az_layout_padding_top_value_exact,
            az_layout_padding_top_value_delete,
            az_layout_padding_top_value_deep_copy,
            az_layout_position_value_auto,
            az_layout_position_value_none,
            az_layout_position_value_inherit,
            az_layout_position_value_initial,
            az_layout_position_value_exact,
            az_layout_position_value_delete,
            az_layout_position_value_deep_copy,
            az_layout_right_value_auto,
            az_layout_right_value_none,
            az_layout_right_value_inherit,
            az_layout_right_value_initial,
            az_layout_right_value_exact,
            az_layout_right_value_delete,
            az_layout_right_value_deep_copy,
            az_layout_top_value_auto,
            az_layout_top_value_none,
            az_layout_top_value_inherit,
            az_layout_top_value_initial,
            az_layout_top_value_exact,
            az_layout_top_value_delete,
            az_layout_top_value_deep_copy,
            az_layout_width_value_auto,
            az_layout_width_value_none,
            az_layout_width_value_inherit,
            az_layout_width_value_initial,
            az_layout_width_value_exact,
            az_layout_width_value_delete,
            az_layout_width_value_deep_copy,
            az_layout_wrap_value_auto,
            az_layout_wrap_value_none,
            az_layout_wrap_value_inherit,
            az_layout_wrap_value_initial,
            az_layout_wrap_value_exact,
            az_layout_wrap_value_delete,
            az_layout_wrap_value_deep_copy,
            az_overflow_value_auto,
            az_overflow_value_none,
            az_overflow_value_inherit,
            az_overflow_value_initial,
            az_overflow_value_exact,
            az_overflow_value_delete,
            az_overflow_value_deep_copy,
            az_style_background_content_value_auto,
            az_style_background_content_value_none,
            az_style_background_content_value_inherit,
            az_style_background_content_value_initial,
            az_style_background_content_value_exact,
            az_style_background_content_value_delete,
            az_style_background_content_value_deep_copy,
            az_style_background_position_value_auto,
            az_style_background_position_value_none,
            az_style_background_position_value_inherit,
            az_style_background_position_value_initial,
            az_style_background_position_value_exact,
            az_style_background_position_value_delete,
            az_style_background_position_value_deep_copy,
            az_style_background_repeat_value_auto,
            az_style_background_repeat_value_none,
            az_style_background_repeat_value_inherit,
            az_style_background_repeat_value_initial,
            az_style_background_repeat_value_exact,
            az_style_background_repeat_value_delete,
            az_style_background_repeat_value_deep_copy,
            az_style_background_size_value_auto,
            az_style_background_size_value_none,
            az_style_background_size_value_inherit,
            az_style_background_size_value_initial,
            az_style_background_size_value_exact,
            az_style_background_size_value_delete,
            az_style_background_size_value_deep_copy,
            az_style_border_bottom_color_value_auto,
            az_style_border_bottom_color_value_none,
            az_style_border_bottom_color_value_inherit,
            az_style_border_bottom_color_value_initial,
            az_style_border_bottom_color_value_exact,
            az_style_border_bottom_color_value_delete,
            az_style_border_bottom_color_value_deep_copy,
            az_style_border_bottom_left_radius_value_auto,
            az_style_border_bottom_left_radius_value_none,
            az_style_border_bottom_left_radius_value_inherit,
            az_style_border_bottom_left_radius_value_initial,
            az_style_border_bottom_left_radius_value_exact,
            az_style_border_bottom_left_radius_value_delete,
            az_style_border_bottom_left_radius_value_deep_copy,
            az_style_border_bottom_right_radius_value_auto,
            az_style_border_bottom_right_radius_value_none,
            az_style_border_bottom_right_radius_value_inherit,
            az_style_border_bottom_right_radius_value_initial,
            az_style_border_bottom_right_radius_value_exact,
            az_style_border_bottom_right_radius_value_delete,
            az_style_border_bottom_right_radius_value_deep_copy,
            az_style_border_bottom_style_value_auto,
            az_style_border_bottom_style_value_none,
            az_style_border_bottom_style_value_inherit,
            az_style_border_bottom_style_value_initial,
            az_style_border_bottom_style_value_exact,
            az_style_border_bottom_style_value_delete,
            az_style_border_bottom_style_value_deep_copy,
            az_style_border_bottom_width_value_auto,
            az_style_border_bottom_width_value_none,
            az_style_border_bottom_width_value_inherit,
            az_style_border_bottom_width_value_initial,
            az_style_border_bottom_width_value_exact,
            az_style_border_bottom_width_value_delete,
            az_style_border_bottom_width_value_deep_copy,
            az_style_border_left_color_value_auto,
            az_style_border_left_color_value_none,
            az_style_border_left_color_value_inherit,
            az_style_border_left_color_value_initial,
            az_style_border_left_color_value_exact,
            az_style_border_left_color_value_delete,
            az_style_border_left_color_value_deep_copy,
            az_style_border_left_style_value_auto,
            az_style_border_left_style_value_none,
            az_style_border_left_style_value_inherit,
            az_style_border_left_style_value_initial,
            az_style_border_left_style_value_exact,
            az_style_border_left_style_value_delete,
            az_style_border_left_style_value_deep_copy,
            az_style_border_left_width_value_auto,
            az_style_border_left_width_value_none,
            az_style_border_left_width_value_inherit,
            az_style_border_left_width_value_initial,
            az_style_border_left_width_value_exact,
            az_style_border_left_width_value_delete,
            az_style_border_left_width_value_deep_copy,
            az_style_border_right_color_value_auto,
            az_style_border_right_color_value_none,
            az_style_border_right_color_value_inherit,
            az_style_border_right_color_value_initial,
            az_style_border_right_color_value_exact,
            az_style_border_right_color_value_delete,
            az_style_border_right_color_value_deep_copy,
            az_style_border_right_style_value_auto,
            az_style_border_right_style_value_none,
            az_style_border_right_style_value_inherit,
            az_style_border_right_style_value_initial,
            az_style_border_right_style_value_exact,
            az_style_border_right_style_value_delete,
            az_style_border_right_style_value_deep_copy,
            az_style_border_right_width_value_auto,
            az_style_border_right_width_value_none,
            az_style_border_right_width_value_inherit,
            az_style_border_right_width_value_initial,
            az_style_border_right_width_value_exact,
            az_style_border_right_width_value_delete,
            az_style_border_right_width_value_deep_copy,
            az_style_border_top_color_value_auto,
            az_style_border_top_color_value_none,
            az_style_border_top_color_value_inherit,
            az_style_border_top_color_value_initial,
            az_style_border_top_color_value_exact,
            az_style_border_top_color_value_delete,
            az_style_border_top_color_value_deep_copy,
            az_style_border_top_left_radius_value_auto,
            az_style_border_top_left_radius_value_none,
            az_style_border_top_left_radius_value_inherit,
            az_style_border_top_left_radius_value_initial,
            az_style_border_top_left_radius_value_exact,
            az_style_border_top_left_radius_value_delete,
            az_style_border_top_left_radius_value_deep_copy,
            az_style_border_top_right_radius_value_auto,
            az_style_border_top_right_radius_value_none,
            az_style_border_top_right_radius_value_inherit,
            az_style_border_top_right_radius_value_initial,
            az_style_border_top_right_radius_value_exact,
            az_style_border_top_right_radius_value_delete,
            az_style_border_top_right_radius_value_deep_copy,
            az_style_border_top_style_value_auto,
            az_style_border_top_style_value_none,
            az_style_border_top_style_value_inherit,
            az_style_border_top_style_value_initial,
            az_style_border_top_style_value_exact,
            az_style_border_top_style_value_delete,
            az_style_border_top_style_value_deep_copy,
            az_style_border_top_width_value_auto,
            az_style_border_top_width_value_none,
            az_style_border_top_width_value_inherit,
            az_style_border_top_width_value_initial,
            az_style_border_top_width_value_exact,
            az_style_border_top_width_value_delete,
            az_style_border_top_width_value_deep_copy,
            az_style_cursor_value_auto,
            az_style_cursor_value_none,
            az_style_cursor_value_inherit,
            az_style_cursor_value_initial,
            az_style_cursor_value_exact,
            az_style_cursor_value_delete,
            az_style_cursor_value_deep_copy,
            az_style_font_family_value_auto,
            az_style_font_family_value_none,
            az_style_font_family_value_inherit,
            az_style_font_family_value_initial,
            az_style_font_family_value_exact,
            az_style_font_family_value_delete,
            az_style_font_family_value_deep_copy,
            az_style_font_size_value_auto,
            az_style_font_size_value_none,
            az_style_font_size_value_inherit,
            az_style_font_size_value_initial,
            az_style_font_size_value_exact,
            az_style_font_size_value_delete,
            az_style_font_size_value_deep_copy,
            az_style_letter_spacing_value_auto,
            az_style_letter_spacing_value_none,
            az_style_letter_spacing_value_inherit,
            az_style_letter_spacing_value_initial,
            az_style_letter_spacing_value_exact,
            az_style_letter_spacing_value_delete,
            az_style_letter_spacing_value_deep_copy,
            az_style_line_height_value_auto,
            az_style_line_height_value_none,
            az_style_line_height_value_inherit,
            az_style_line_height_value_initial,
            az_style_line_height_value_exact,
            az_style_line_height_value_delete,
            az_style_line_height_value_deep_copy,
            az_style_tab_width_value_auto,
            az_style_tab_width_value_none,
            az_style_tab_width_value_inherit,
            az_style_tab_width_value_initial,
            az_style_tab_width_value_exact,
            az_style_tab_width_value_delete,
            az_style_tab_width_value_deep_copy,
            az_style_text_alignment_horz_value_auto,
            az_style_text_alignment_horz_value_none,
            az_style_text_alignment_horz_value_inherit,
            az_style_text_alignment_horz_value_initial,
            az_style_text_alignment_horz_value_exact,
            az_style_text_alignment_horz_value_delete,
            az_style_text_alignment_horz_value_deep_copy,
            az_style_text_color_value_auto,
            az_style_text_color_value_none,
            az_style_text_color_value_inherit,
            az_style_text_color_value_initial,
            az_style_text_color_value_exact,
            az_style_text_color_value_delete,
            az_style_text_color_value_deep_copy,
            az_style_word_spacing_value_auto,
            az_style_word_spacing_value_none,
            az_style_word_spacing_value_inherit,
            az_style_word_spacing_value_initial,
            az_style_word_spacing_value_exact,
            az_style_word_spacing_value_delete,
            az_style_word_spacing_value_deep_copy,
            az_css_property_text_color,
            az_css_property_font_size,
            az_css_property_font_family,
            az_css_property_text_align,
            az_css_property_letter_spacing,
            az_css_property_line_height,
            az_css_property_word_spacing,
            az_css_property_tab_width,
            az_css_property_cursor,
            az_css_property_display,
            az_css_property_float,
            az_css_property_box_sizing,
            az_css_property_width,
            az_css_property_height,
            az_css_property_min_width,
            az_css_property_min_height,
            az_css_property_max_width,
            az_css_property_max_height,
            az_css_property_position,
            az_css_property_top,
            az_css_property_right,
            az_css_property_left,
            az_css_property_bottom,
            az_css_property_flex_wrap,
            az_css_property_flex_direction,
            az_css_property_flex_grow,
            az_css_property_flex_shrink,
            az_css_property_justify_content,
            az_css_property_align_items,
            az_css_property_align_content,
            az_css_property_background_content,
            az_css_property_background_position,
            az_css_property_background_size,
            az_css_property_background_repeat,
            az_css_property_overflow_x,
            az_css_property_overflow_y,
            az_css_property_padding_top,
            az_css_property_padding_left,
            az_css_property_padding_right,
            az_css_property_padding_bottom,
            az_css_property_margin_top,
            az_css_property_margin_left,
            az_css_property_margin_right,
            az_css_property_margin_bottom,
            az_css_property_border_top_left_radius,
            az_css_property_border_top_right_radius,
            az_css_property_border_bottom_left_radius,
            az_css_property_border_bottom_right_radius,
            az_css_property_border_top_color,
            az_css_property_border_right_color,
            az_css_property_border_left_color,
            az_css_property_border_bottom_color,
            az_css_property_border_top_style,
            az_css_property_border_right_style,
            az_css_property_border_left_style,
            az_css_property_border_bottom_style,
            az_css_property_border_top_width,
            az_css_property_border_right_width,
            az_css_property_border_left_width,
            az_css_property_border_bottom_width,
            az_css_property_box_shadow_left,
            az_css_property_box_shadow_right,
            az_css_property_box_shadow_top,
            az_css_property_box_shadow_bottom,
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
            az_event_filter_hover,
            az_event_filter_not,
            az_event_filter_focus,
            az_event_filter_window,
            az_event_filter_delete,
            az_event_filter_deep_copy,
            az_hover_event_filter_mouse_over,
            az_hover_event_filter_mouse_down,
            az_hover_event_filter_left_mouse_down,
            az_hover_event_filter_right_mouse_down,
            az_hover_event_filter_middle_mouse_down,
            az_hover_event_filter_mouse_up,
            az_hover_event_filter_left_mouse_up,
            az_hover_event_filter_right_mouse_up,
            az_hover_event_filter_middle_mouse_up,
            az_hover_event_filter_mouse_enter,
            az_hover_event_filter_mouse_leave,
            az_hover_event_filter_scroll,
            az_hover_event_filter_scroll_start,
            az_hover_event_filter_scroll_end,
            az_hover_event_filter_text_input,
            az_hover_event_filter_virtual_key_down,
            az_hover_event_filter_virtual_key_up,
            az_hover_event_filter_hovered_file,
            az_hover_event_filter_dropped_file,
            az_hover_event_filter_hovered_file_cancelled,
            az_hover_event_filter_delete,
            az_hover_event_filter_deep_copy,
            az_focus_event_filter_mouse_over,
            az_focus_event_filter_mouse_down,
            az_focus_event_filter_left_mouse_down,
            az_focus_event_filter_right_mouse_down,
            az_focus_event_filter_middle_mouse_down,
            az_focus_event_filter_mouse_up,
            az_focus_event_filter_left_mouse_up,
            az_focus_event_filter_right_mouse_up,
            az_focus_event_filter_middle_mouse_up,
            az_focus_event_filter_mouse_enter,
            az_focus_event_filter_mouse_leave,
            az_focus_event_filter_scroll,
            az_focus_event_filter_scroll_start,
            az_focus_event_filter_scroll_end,
            az_focus_event_filter_text_input,
            az_focus_event_filter_virtual_key_down,
            az_focus_event_filter_virtual_key_up,
            az_focus_event_filter_focus_received,
            az_focus_event_filter_focus_lost,
            az_focus_event_filter_delete,
            az_focus_event_filter_deep_copy,
            az_not_event_filter_hover,
            az_not_event_filter_focus,
            az_not_event_filter_delete,
            az_not_event_filter_deep_copy,
            az_window_event_filter_mouse_over,
            az_window_event_filter_mouse_down,
            az_window_event_filter_left_mouse_down,
            az_window_event_filter_right_mouse_down,
            az_window_event_filter_middle_mouse_down,
            az_window_event_filter_mouse_up,
            az_window_event_filter_left_mouse_up,
            az_window_event_filter_right_mouse_up,
            az_window_event_filter_middle_mouse_up,
            az_window_event_filter_mouse_enter,
            az_window_event_filter_mouse_leave,
            az_window_event_filter_scroll,
            az_window_event_filter_scroll_start,
            az_window_event_filter_scroll_end,
            az_window_event_filter_text_input,
            az_window_event_filter_virtual_key_down,
            az_window_event_filter_virtual_key_up,
            az_window_event_filter_hovered_file,
            az_window_event_filter_dropped_file,
            az_window_event_filter_hovered_file_cancelled,
            az_window_event_filter_delete,
            az_window_event_filter_deep_copy,
            az_tab_index_auto,
            az_tab_index_override_in_parent,
            az_tab_index_no_keyboard_focus,
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
            az_image_source_embedded,
            az_image_source_file,
            az_image_source_raw,
            az_image_source_delete,
            az_image_source_deep_copy,
            az_font_source_embedded,
            az_font_source_file,
            az_font_source_system,
            az_font_source_delete,
            az_font_source_deep_copy,
            az_raw_image_new,
            az_raw_image_delete,
            az_raw_image_shallow_copy,
            az_raw_image_format_r8,
            az_raw_image_format_r16,
            az_raw_image_format_rg16,
            az_raw_image_format_bgra8,
            az_raw_image_format_rgbaf32,
            az_raw_image_format_rg8,
            az_raw_image_format_rgbai32,
            az_raw_image_format_rgba8,
            az_raw_image_format_delete,
            az_raw_image_format_deep_copy,
            az_window_create_options_new,
            az_window_create_options_delete,
            az_window_create_options_shallow_copy,
        })
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

    use azul_dll::*;

    impl From<std::string::String> for crate::str::String {
        fn from(s: std::string::String) -> crate::str::String {
            crate::str::String::from_utf8_unchecked(s.as_ptr(), s.len()) // - copies s into a new String
            // - s is deallocated here
        }
    }

    /// `String` struct
    pub struct String { pub(crate) object: AzString }

    impl String {
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn from_utf8_unchecked(ptr: *const u8, len: usize) -> Self { Self { object: az_string_from_utf8_unchecked(ptr, len) } }
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn from_utf8_lossy(ptr: *const u8, len: usize) -> Self { Self { object: az_string_from_utf8_lossy(ptr, len) } }
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn into_bytes(self)  -> crate::vec::U8Vec { crate::vec::U8Vec { object: { az_string_into_bytes(self.leak())} } }
       /// Prevents the destructor from running and returns the internal `AzString`
       pub fn leak(self) -> AzString { az_string_deep_copy(&self.object) }
    }

    impl Drop for String { fn drop(&mut self) { az_string_delete(&mut self.object); } }
}

/// Definition of azuls internal `Vec<*>` wrappers
#[allow(dead_code, unused_imports)]
pub mod vec {

    use azul_dll::*;

    impl From<std::vec::Vec<u8>> for crate::vec::U8Vec {
        fn from(v: std::vec::Vec<u8>) -> crate::vec::U8Vec {
            crate::vec::U8Vec::copy_from(v.as_ptr(), v.len())
        }
    }

    impl From<crate::vec::U8Vec> for std::vec::Vec<u8> {
        fn from(v: crate::vec::U8Vec) -> std::vec::Vec<u8> {
            unsafe { std::slice::from_raw_parts(v.object.object.as_ptr(), v.object.object.len()).to_vec() }
        }
    }

    impl From<std::vec::Vec<std::string::String>> for crate::vec::StringVec {
        fn from(v: std::vec::Vec<std::string::String>) -> crate::vec::StringVec {
            let vec: Vec<AzString> = v.into_iter().map(|i| {
                let i: std::vec::Vec<u8> = i.into_bytes();
                az_string_from_utf8_unchecked(i.as_ptr(), i.len())
            }).collect();

            crate::vec::StringVec { object: az_string_vec_copy_from(vec.as_ptr(), vec.len()) }
        }
    }

    impl From<crate::vec::StringVec> for std::vec::Vec<std::string::String> {
        fn from(v: crate::vec::StringVec) -> std::vec::Vec<std::string::String> {
            v.leak().object
            .into_iter()
            .map(|s| unsafe {
                let s_vec: std::vec::Vec<u8> = s.into_bytes().into();
                std::string::String::from_utf8_unchecked(s_vec)
            })
            .collect()

            // delete() not necessary because StringVec is stack-allocated
        }
    }    use crate::str::String;
    use crate::css::GradientStopPre;


    /// Wrapper over a Rust-allocated `Vec<u8>`
    pub struct U8Vec { pub(crate) object: AzU8Vec }

    impl U8Vec {
        /// Creates + allocates a Rust `Vec<u8>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const u8, len: usize) -> Self { Self { object: az_u8_vec_copy_from(ptr, len) } }
        /// Returns the internal pointer to the start of the heap-allocated `[u8]`
        pub fn as_ptr(&self)  -> *const u8 { az_u8_vec_as_ptr(&self.object) }
        /// Returns the length of bytes in the heap-allocated `[u8]`
        pub fn len(&self)  -> usize { az_u8_vec_len(&self.object) }
       /// Prevents the destructor from running and returns the internal `AzU8Vec`
       pub fn leak(self) -> AzU8Vec { az_u8_vec_deep_copy(&self.object) }
    }

    impl Drop for U8Vec { fn drop(&mut self) { az_u8_vec_delete(&mut self.object); } }


    /// Wrapper over a Rust-allocated `Vec<String>`
    pub struct StringVec { pub(crate) object: AzStringVec }

    impl StringVec {
        /// Creates + allocates a Rust `Vec<String>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzString, len: usize) -> Self { Self { object: az_string_vec_copy_from(ptr, len) } }
       /// Prevents the destructor from running and returns the internal `AzStringVec`
       pub fn leak(self) -> AzStringVec { az_string_vec_deep_copy(&self.object) }
    }

    impl Drop for StringVec { fn drop(&mut self) { az_string_vec_delete(&mut self.object); } }


    /// Wrapper over a Rust-allocated `Vec<GradientStopPre>`
    pub struct GradientStopPreVec { pub(crate) object: AzGradientStopPreVec }

    impl GradientStopPreVec {
        /// Creates + allocates a Rust `Vec<GradientStopPre>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzGradientStopPre, len: usize) -> Self { Self { object: az_gradient_stop_pre_vec_copy_from(ptr, len) } }
       /// Prevents the destructor from running and returns the internal `AzGradientStopPreVec`
       pub fn leak(self) -> AzGradientStopPreVec { az_gradient_stop_pre_vec_deep_copy(&self.object) }
    }

    impl Drop for GradientStopPreVec { fn drop(&mut self) { az_gradient_stop_pre_vec_delete(&mut self.object); } }
}

/// Definition of azuls internal `PathBuf` type + functions for conversion from `std::PathBuf`
#[allow(dead_code, unused_imports)]
pub mod path {

    use azul_dll::*;
    use crate::str::String;


    /// Wrapper over a Rust-allocated `PathBuf`
    pub struct PathBuf { pub(crate) ptr: AzPathBufPtr }

    impl PathBuf {
        /// Creates a new PathBuf from a String
        pub fn new(path: String) -> Self { Self { ptr: az_path_buf_new(path.leak()) } }
       /// Prevents the destructor from running and returns the internal `AzPathBufPtr`
       pub fn leak(self) -> AzPathBufPtr { let p = az_path_buf_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for PathBuf { fn drop(&mut self) { az_path_buf_delete(&mut self.ptr); } }
}

/// Definition of azuls internal `Option<*>` wrappers
#[allow(dead_code, unused_imports)]
pub mod option {

    use azul_dll::*;


    /// `OptionPercentageValue` struct
    pub struct OptionPercentageValue { pub(crate) ptr: AzOptionPercentageValuePtr }

    impl OptionPercentageValue {
       /// Prevents the destructor from running and returns the internal `AzOptionPercentageValuePtr`
       pub fn leak(self) -> AzOptionPercentageValuePtr { let p = az_option_percentage_value_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for OptionPercentageValue { fn drop(&mut self) { az_option_percentage_value_delete(&mut self.ptr); } }
}

/// `App` construction and configuration
#[allow(dead_code, unused_imports)]
pub mod app {

    use azul_dll::*;
    use crate::callbacks::{RefAny, LayoutCallback};
    use crate::window::WindowCreateOptions;


    /// `AppConfig` struct
    pub struct AppConfig { pub(crate) ptr: AzAppConfigPtr }

    impl AppConfig {
        /// Creates a new AppConfig with default values
        pub fn default() -> Self { Self { ptr: az_app_config_default() } }
       /// Prevents the destructor from running and returns the internal `AzAppConfigPtr`
       pub fn leak(self) -> AzAppConfigPtr { let p = az_app_config_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for AppConfig { fn drop(&mut self) { az_app_config_delete(&mut self.ptr); } }


    /// `App` struct
    pub struct App { pub(crate) ptr: AzAppPtr }

    impl App {
        /// Creates a new App instance from the given `AppConfig`
        pub fn new(data: RefAny, config: AppConfig, callback: LayoutCallback) -> Self { 
            unsafe { crate::callbacks::CALLBACK = callback };
            Self {
                ptr: az_app_new(data.leak(), config.leak(), crate::callbacks::translate_callback)
            }
 }
        /// Runs the application. Due to platform restrictions (specifically `WinMain` on Windows), this function never returns.
        pub fn run(self, window: WindowCreateOptions)  { az_app_run(self.leak(), window.leak()) }
       /// Prevents the destructor from running and returns the internal `AzAppPtr`
       pub fn leak(self) -> AzAppPtr { let p = az_app_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for App { fn drop(&mut self) { az_app_delete(&mut self.ptr); } }
}

/// Callback type definitions + struct definitions of `CallbackInfo`s
#[allow(dead_code, unused_imports)]
pub mod callbacks {

    use azul_dll::*;


    use crate::dom::Dom;

    /// Callback fn that returns the layout
    pub type LayoutCallback = fn(RefAny, LayoutInfo) -> Dom;

    fn default_callback(_: RefAny, _: LayoutInfo) -> Dom {
        Dom::div()
    }

    pub(crate) static mut CALLBACK: LayoutCallback = default_callback;

    pub(crate) fn translate_callback(data: azul_dll::AzRefAny, layout: azul_dll::AzLayoutInfoPtr) -> azul_dll::AzDomPtr {
        unsafe { CALLBACK(RefAny(data), LayoutInfo { ptr: layout }) }.leak()
    }


/// Return type of a regular callback - currently `AzUpdateScreen`
pub type CallbackReturn = AzUpdateScreen;
/// Callback for responding to window events
pub type Callback = fn(AzCallbackInfoPtr) -> AzCallbackReturn;

    /// `CallbackInfo` struct
    pub struct CallbackInfo { pub(crate) ptr: AzCallbackInfoPtr }

    impl CallbackInfo {
       /// Prevents the destructor from running and returns the internal `AzCallbackInfoPtr`
       pub fn leak(self) -> AzCallbackInfoPtr { let p = az_callback_info_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for CallbackInfo { fn drop(&mut self) { az_callback_info_delete(&mut self.ptr); } }


    /// `UpdateScreen` struct
    pub struct UpdateScreen { pub(crate) object: AzUpdateScreen }

    impl<T> From<Option<T>> for UpdateScreen { fn from(o: Option<T>) -> Self { Self { object: match o { None => AzDontRedraw, Some(_) => AzRedraw }} } }


    /// `Redraw` struct
    pub static REDRAW: AzUpdateScreen = AzRedraw;



    /// `DontRedraw` struct
    pub static DONT_REDRAW: AzUpdateScreen = AzDontRedraw;



/// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
pub type IFrameCallback = fn(AzIFrameCallbackInfoPtr) -> AzIFrameCallbackReturnPtr;

    /// `IFrameCallbackInfo` struct
    pub struct IFrameCallbackInfo { pub(crate) ptr: AzIFrameCallbackInfoPtr }

    impl IFrameCallbackInfo {
       /// Prevents the destructor from running and returns the internal `AzIFrameCallbackInfoPtr`
       pub fn leak(self) -> AzIFrameCallbackInfoPtr { let p = az_i_frame_callback_info_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for IFrameCallbackInfo { fn drop(&mut self) { az_i_frame_callback_info_delete(&mut self.ptr); } }


    /// `IFrameCallbackReturn` struct
    pub struct IFrameCallbackReturn { pub(crate) ptr: AzIFrameCallbackReturnPtr }

    impl IFrameCallbackReturn {
       /// Prevents the destructor from running and returns the internal `AzIFrameCallbackReturnPtr`
       pub fn leak(self) -> AzIFrameCallbackReturnPtr { let p = az_i_frame_callback_return_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for IFrameCallbackReturn { fn drop(&mut self) { az_i_frame_callback_return_delete(&mut self.ptr); } }


/// Callback for rendering to an OpenGL texture
pub type GlCallback = fn(AzGlCallbackInfoPtr) -> AzGlCallbackReturnPtr;

    /// `GlCallbackInfo` struct
    pub struct GlCallbackInfo { pub(crate) ptr: AzGlCallbackInfoPtr }

    impl GlCallbackInfo {
       /// Prevents the destructor from running and returns the internal `AzGlCallbackInfoPtr`
       pub fn leak(self) -> AzGlCallbackInfoPtr { let p = az_gl_callback_info_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for GlCallbackInfo { fn drop(&mut self) { az_gl_callback_info_delete(&mut self.ptr); } }


    /// `GlCallbackReturn` struct
    pub struct GlCallbackReturn { pub(crate) ptr: AzGlCallbackReturnPtr }

    impl GlCallbackReturn {
       /// Prevents the destructor from running and returns the internal `AzGlCallbackReturnPtr`
       pub fn leak(self) -> AzGlCallbackReturnPtr { let p = az_gl_callback_return_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for GlCallbackReturn { fn drop(&mut self) { az_gl_callback_return_delete(&mut self.ptr); } }


    use azul_dll::AzRefAny as AzRefAnyCore;

    /// `RefAny` struct
    #[repr(transparent)]
    pub struct RefAny(pub(crate) AzRefAnyCore);

    impl Clone for RefAny {
        fn clone(&self) -> Self {
            RefAny(az_ref_any_shallow_copy(&self.0))
        }
    }

    impl RefAny {

        /// Creates a new, type-erased pointer by casting the `T` value into a `Vec<u8>` and saving the length + type ID
        pub fn new<T: 'static>(value: T) -> Self {
            use azul_dll::*;

            fn default_custom_destructor<U: 'static>(ptr: AzRefAnyCore) {
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
            let s = az_ref_any_new(
                (&value as *const T) as *const u8,
                ::std::mem::size_of::<T>(),
                Self::get_type_id::<T>() as u64,
                crate::str::String::from_utf8_unchecked(type_name_str.as_ptr(), type_name_str.len()).leak(),
                default_custom_destructor::<T>,
            );
            ::std::mem::forget(value); // do not run the destructor of T here!
            Self(s)
        }

        /// Returns the inner `AzRefAnyCore`
        pub fn leak(self) -> AzRefAnyCore {
            use std::mem;
            let s = az_ref_any_core_copy(&self.0);
            mem::forget(self); // do not run destructor
            s
        }

        /// Downcasts the type-erased pointer to a type `&U`, returns `None` if the types don't match
        #[inline]
        pub fn downcast_ref<'a, U: 'static>(&'a self) -> Option<&'a U> {
            use std::ptr;
            let ptr = az_ref_any_get_ptr(&self.0, self.0._internal_len, Self::get_type_id::<U>());
            if ptr == ptr::null() { None } else { Some(unsafe { &*(self.0._internal_ptr as *const U) as &'a U }) }
        }

        /// Downcasts the type-erased pointer to a type `&mut U`, returns `None` if the types don't match
        #[inline]
        pub fn downcast_mut<'a, U: 'static>(&'a mut self) -> Option<&'a mut U> {
            use std::ptr;
            let ptr = az_ref_any_get_mut_ptr(&self.0, self.0._internal_len, Self::get_type_id::<U>());
            if ptr == ptr::null_mut() { None } else { Some(unsafe { &mut *(self.0._internal_ptr as *mut U) as &'a mut U }) }
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
            az_ref_any_delete(&mut self.0);
        }
    }


    /// `LayoutInfo` struct
    pub struct LayoutInfo { pub(crate) ptr: AzLayoutInfoPtr }

    impl LayoutInfo {
       /// Prevents the destructor from running and returns the internal `AzLayoutInfoPtr`
       pub fn leak(self) -> AzLayoutInfoPtr { let p = az_layout_info_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for LayoutInfo { fn drop(&mut self) { az_layout_info_delete(&mut self.ptr); } }
}

/// `Css` parsing module
#[allow(dead_code, unused_imports)]
pub mod css {

    use azul_dll::*;
    use crate::str::String;
    use crate::path::PathBuf;


    /// `Css` struct
    pub struct Css { pub(crate) ptr: AzCssPtr }

    impl Css {
        /// Loads the native style for the given operating system
        pub fn native() -> Self { Self { ptr: az_css_native() } }
        /// Returns an empty CSS style
        pub fn empty() -> Self { Self { ptr: az_css_empty() } }
        /// Returns a CSS style parsed from a `String`
        pub fn from_string(s: String) -> Self { Self { ptr: az_css_from_string(s.leak()) } }
        /// Appends a parsed stylesheet to `Css::native()`
        pub fn override_native(s: String) -> Self { Self { ptr: az_css_override_native(s.leak()) } }
       /// Prevents the destructor from running and returns the internal `AzCssPtr`
       pub fn leak(self) -> AzCssPtr { let p = az_css_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for Css { fn drop(&mut self) { az_css_delete(&mut self.ptr); } }


    /// `CssHotReloader` struct
    pub struct CssHotReloader { pub(crate) ptr: AzCssHotReloaderPtr }

    impl CssHotReloader {
        /// Creates a `HotReloadHandler` that hot-reloads a CSS file every X milliseconds
        pub fn new(path: PathBuf, reload_ms: u64) -> Self { Self { ptr: az_css_hot_reloader_new(path.leak(), reload_ms) } }
        /// Creates a `HotReloadHandler` that overrides the `Css::native()` stylesheet with a CSS file, reloaded every X milliseconds
        pub fn override_native(path: PathBuf, reload_ms: u64) -> Self { Self { ptr: az_css_hot_reloader_override_native(path.leak(), reload_ms) } }
       /// Prevents the destructor from running and returns the internal `AzCssHotReloaderPtr`
       pub fn leak(self) -> AzCssHotReloaderPtr { let p = az_css_hot_reloader_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for CssHotReloader { fn drop(&mut self) { az_css_hot_reloader_delete(&mut self.ptr); } }


    /// `ColorU` struct
    pub struct ColorU { pub(crate) object: AzColorU }

    impl ColorU {
       /// Prevents the destructor from running and returns the internal `AzColorU`
       pub fn leak(self) -> AzColorU { az_color_u_deep_copy(&self.object) }
    }

    impl Drop for ColorU { fn drop(&mut self) { az_color_u_delete(&mut self.object); } }


    /// `SizeMetric` struct
    pub struct SizeMetric { pub(crate) object: AzSizeMetric }

    impl SizeMetric {
        pub fn px() -> Self { Self { object: az_size_metric_px() }  }
        pub fn pt() -> Self { Self { object: az_size_metric_pt() }  }
        pub fn em() -> Self { Self { object: az_size_metric_em() }  }
        pub fn percent() -> Self { Self { object: az_size_metric_percent() }  }
       /// Prevents the destructor from running and returns the internal `AzSizeMetric`
       pub fn leak(self) -> AzSizeMetric { az_size_metric_deep_copy(&self.object) }
    }

    impl Drop for SizeMetric { fn drop(&mut self) { az_size_metric_delete(&mut self.object); } }


    /// `FloatValue` struct
    pub struct FloatValue { pub(crate) object: AzFloatValue }

    impl FloatValue {
       /// Prevents the destructor from running and returns the internal `AzFloatValue`
       pub fn leak(self) -> AzFloatValue { az_float_value_deep_copy(&self.object) }
    }

    impl Drop for FloatValue { fn drop(&mut self) { az_float_value_delete(&mut self.object); } }


    /// `PixelValue` struct
    pub struct PixelValue { pub(crate) object: AzPixelValue }

    impl PixelValue {
       /// Prevents the destructor from running and returns the internal `AzPixelValue`
       pub fn leak(self) -> AzPixelValue { az_pixel_value_deep_copy(&self.object) }
    }

    impl Drop for PixelValue { fn drop(&mut self) { az_pixel_value_delete(&mut self.object); } }


    /// `PixelValueNoPercent` struct
    pub struct PixelValueNoPercent { pub(crate) object: AzPixelValueNoPercent }

    impl PixelValueNoPercent {
       /// Prevents the destructor from running and returns the internal `AzPixelValueNoPercent`
       pub fn leak(self) -> AzPixelValueNoPercent { az_pixel_value_no_percent_deep_copy(&self.object) }
    }

    impl Drop for PixelValueNoPercent { fn drop(&mut self) { az_pixel_value_no_percent_delete(&mut self.object); } }


    /// `BoxShadowClipMode` struct
    pub struct BoxShadowClipMode { pub(crate) object: AzBoxShadowClipMode }

    impl BoxShadowClipMode {
        pub fn outset() -> Self { Self { object: az_box_shadow_clip_mode_outset() }  }
        pub fn inset() -> Self { Self { object: az_box_shadow_clip_mode_inset() }  }
       /// Prevents the destructor from running and returns the internal `AzBoxShadowClipMode`
       pub fn leak(self) -> AzBoxShadowClipMode { az_box_shadow_clip_mode_deep_copy(&self.object) }
    }

    impl Drop for BoxShadowClipMode { fn drop(&mut self) { az_box_shadow_clip_mode_delete(&mut self.object); } }


    /// `BoxShadowPreDisplayItem` struct
    pub struct BoxShadowPreDisplayItem { pub(crate) object: AzBoxShadowPreDisplayItem }

    impl BoxShadowPreDisplayItem {
       /// Prevents the destructor from running and returns the internal `AzBoxShadowPreDisplayItem`
       pub fn leak(self) -> AzBoxShadowPreDisplayItem { az_box_shadow_pre_display_item_deep_copy(&self.object) }
    }

    impl Drop for BoxShadowPreDisplayItem { fn drop(&mut self) { az_box_shadow_pre_display_item_delete(&mut self.object); } }


    /// `LayoutAlignContent` struct
    pub struct LayoutAlignContent { pub(crate) object: AzLayoutAlignContent }

    impl LayoutAlignContent {
        /// Default value. Lines stretch to take up the remaining space
        pub fn stretch() -> Self { Self { object: az_layout_align_content_stretch() }  }
        /// Lines are packed toward the center of the flex container
        pub fn center() -> Self { Self { object: az_layout_align_content_center() }  }
        /// Lines are packed toward the start of the flex container
        pub fn start() -> Self { Self { object: az_layout_align_content_start() }  }
        /// Lines are packed toward the end of the flex container
        pub fn end() -> Self { Self { object: az_layout_align_content_end() }  }
        /// Lines are evenly distributed in the flex container
        pub fn space_between() -> Self { Self { object: az_layout_align_content_space_between() }  }
        /// Lines are evenly distributed in the flex container, with half-size spaces on either end
        pub fn space_around() -> Self { Self { object: az_layout_align_content_space_around() }  }
       /// Prevents the destructor from running and returns the internal `AzLayoutAlignContent`
       pub fn leak(self) -> AzLayoutAlignContent { az_layout_align_content_deep_copy(&self.object) }
    }

    impl Drop for LayoutAlignContent { fn drop(&mut self) { az_layout_align_content_delete(&mut self.object); } }


    /// `LayoutAlignItems` struct
    pub struct LayoutAlignItems { pub(crate) object: AzLayoutAlignItems }

    impl LayoutAlignItems {
        /// Items are stretched to fit the container
        pub fn stretch() -> Self { Self { object: az_layout_align_items_stretch() }  }
        /// Items are positioned at the center of the container
        pub fn center() -> Self { Self { object: az_layout_align_items_center() }  }
        /// Items are positioned at the beginning of the container
        pub fn start() -> Self { Self { object: az_layout_align_items_start() }  }
        /// Items are positioned at the end of the container
        pub fn end() -> Self { Self { object: az_layout_align_items_end() }  }
       /// Prevents the destructor from running and returns the internal `AzLayoutAlignItems`
       pub fn leak(self) -> AzLayoutAlignItems { az_layout_align_items_deep_copy(&self.object) }
    }

    impl Drop for LayoutAlignItems { fn drop(&mut self) { az_layout_align_items_delete(&mut self.object); } }


    /// `LayoutBottom` struct
    pub struct LayoutBottom { pub(crate) object: AzLayoutBottom }

    impl LayoutBottom {
       /// Prevents the destructor from running and returns the internal `AzLayoutBottom`
       pub fn leak(self) -> AzLayoutBottom { az_layout_bottom_deep_copy(&self.object) }
    }

    impl Drop for LayoutBottom { fn drop(&mut self) { az_layout_bottom_delete(&mut self.object); } }


    /// `LayoutBoxSizing` struct
    pub struct LayoutBoxSizing { pub(crate) object: AzLayoutBoxSizing }

    impl LayoutBoxSizing {
        pub fn content_box() -> Self { Self { object: az_layout_box_sizing_content_box() }  }
        pub fn border_box() -> Self { Self { object: az_layout_box_sizing_border_box() }  }
       /// Prevents the destructor from running and returns the internal `AzLayoutBoxSizing`
       pub fn leak(self) -> AzLayoutBoxSizing { az_layout_box_sizing_deep_copy(&self.object) }
    }

    impl Drop for LayoutBoxSizing { fn drop(&mut self) { az_layout_box_sizing_delete(&mut self.object); } }


    /// `LayoutDirection` struct
    pub struct LayoutDirection { pub(crate) object: AzLayoutDirection }

    impl LayoutDirection {
        pub fn row() -> Self { Self { object: az_layout_direction_row() }  }
        pub fn row_reverse() -> Self { Self { object: az_layout_direction_row_reverse() }  }
        pub fn column() -> Self { Self { object: az_layout_direction_column() }  }
        pub fn column_reverse() -> Self { Self { object: az_layout_direction_column_reverse() }  }
       /// Prevents the destructor from running and returns the internal `AzLayoutDirection`
       pub fn leak(self) -> AzLayoutDirection { az_layout_direction_deep_copy(&self.object) }
    }

    impl Drop for LayoutDirection { fn drop(&mut self) { az_layout_direction_delete(&mut self.object); } }


    /// `LayoutDisplay` struct
    pub struct LayoutDisplay { pub(crate) object: AzLayoutDisplay }

    impl LayoutDisplay {
        pub fn flex() -> Self { Self { object: az_layout_display_flex() }  }
        pub fn block() -> Self { Self { object: az_layout_display_block() }  }
        pub fn inline_block() -> Self { Self { object: az_layout_display_inline_block() }  }
       /// Prevents the destructor from running and returns the internal `AzLayoutDisplay`
       pub fn leak(self) -> AzLayoutDisplay { az_layout_display_deep_copy(&self.object) }
    }

    impl Drop for LayoutDisplay { fn drop(&mut self) { az_layout_display_delete(&mut self.object); } }


    /// `LayoutFlexGrow` struct
    pub struct LayoutFlexGrow { pub(crate) object: AzLayoutFlexGrow }

    impl LayoutFlexGrow {
       /// Prevents the destructor from running and returns the internal `AzLayoutFlexGrow`
       pub fn leak(self) -> AzLayoutFlexGrow { az_layout_flex_grow_deep_copy(&self.object) }
    }

    impl Drop for LayoutFlexGrow { fn drop(&mut self) { az_layout_flex_grow_delete(&mut self.object); } }


    /// `LayoutFlexShrink` struct
    pub struct LayoutFlexShrink { pub(crate) object: AzLayoutFlexShrink }

    impl LayoutFlexShrink {
       /// Prevents the destructor from running and returns the internal `AzLayoutFlexShrink`
       pub fn leak(self) -> AzLayoutFlexShrink { az_layout_flex_shrink_deep_copy(&self.object) }
    }

    impl Drop for LayoutFlexShrink { fn drop(&mut self) { az_layout_flex_shrink_delete(&mut self.object); } }


    /// `LayoutFloat` struct
    pub struct LayoutFloat { pub(crate) object: AzLayoutFloat }

    impl LayoutFloat {
        pub fn left() -> Self { Self { object: az_layout_float_left() }  }
        pub fn right() -> Self { Self { object: az_layout_float_right() }  }
       /// Prevents the destructor from running and returns the internal `AzLayoutFloat`
       pub fn leak(self) -> AzLayoutFloat { az_layout_float_deep_copy(&self.object) }
    }

    impl Drop for LayoutFloat { fn drop(&mut self) { az_layout_float_delete(&mut self.object); } }


    /// `LayoutHeight` struct
    pub struct LayoutHeight { pub(crate) object: AzLayoutHeight }

    impl LayoutHeight {
       /// Prevents the destructor from running and returns the internal `AzLayoutHeight`
       pub fn leak(self) -> AzLayoutHeight { az_layout_height_deep_copy(&self.object) }
    }

    impl Drop for LayoutHeight { fn drop(&mut self) { az_layout_height_delete(&mut self.object); } }


    /// `LayoutJustifyContent` struct
    pub struct LayoutJustifyContent { pub(crate) object: AzLayoutJustifyContent }

    impl LayoutJustifyContent {
        /// Default value. Items are positioned at the beginning of the container
        pub fn start() -> Self { Self { object: az_layout_justify_content_start() }  }
        /// Items are positioned at the end of the container
        pub fn end() -> Self { Self { object: az_layout_justify_content_end() }  }
        /// Items are positioned at the center of the container
        pub fn center() -> Self { Self { object: az_layout_justify_content_center() }  }
        /// Items are positioned with space between the lines
        pub fn space_between() -> Self { Self { object: az_layout_justify_content_space_between() }  }
        /// Items are positioned with space before, between, and after the lines
        pub fn space_around() -> Self { Self { object: az_layout_justify_content_space_around() }  }
        /// Items are distributed so that the spacing between any two adjacent alignment subjects, before the first alignment subject, and after the last alignment subject is the same
        pub fn space_evenly() -> Self { Self { object: az_layout_justify_content_space_evenly() }  }
       /// Prevents the destructor from running and returns the internal `AzLayoutJustifyContent`
       pub fn leak(self) -> AzLayoutJustifyContent { az_layout_justify_content_deep_copy(&self.object) }
    }

    impl Drop for LayoutJustifyContent { fn drop(&mut self) { az_layout_justify_content_delete(&mut self.object); } }


    /// `LayoutLeft` struct
    pub struct LayoutLeft { pub(crate) object: AzLayoutLeft }

    impl LayoutLeft {
       /// Prevents the destructor from running and returns the internal `AzLayoutLeft`
       pub fn leak(self) -> AzLayoutLeft { az_layout_left_deep_copy(&self.object) }
    }

    impl Drop for LayoutLeft { fn drop(&mut self) { az_layout_left_delete(&mut self.object); } }


    /// `LayoutMarginBottom` struct
    pub struct LayoutMarginBottom { pub(crate) object: AzLayoutMarginBottom }

    impl LayoutMarginBottom {
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginBottom`
       pub fn leak(self) -> AzLayoutMarginBottom { az_layout_margin_bottom_deep_copy(&self.object) }
    }

    impl Drop for LayoutMarginBottom { fn drop(&mut self) { az_layout_margin_bottom_delete(&mut self.object); } }


    /// `LayoutMarginLeft` struct
    pub struct LayoutMarginLeft { pub(crate) object: AzLayoutMarginLeft }

    impl LayoutMarginLeft {
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginLeft`
       pub fn leak(self) -> AzLayoutMarginLeft { az_layout_margin_left_deep_copy(&self.object) }
    }

    impl Drop for LayoutMarginLeft { fn drop(&mut self) { az_layout_margin_left_delete(&mut self.object); } }


    /// `LayoutMarginRight` struct
    pub struct LayoutMarginRight { pub(crate) object: AzLayoutMarginRight }

    impl LayoutMarginRight {
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginRight`
       pub fn leak(self) -> AzLayoutMarginRight { az_layout_margin_right_deep_copy(&self.object) }
    }

    impl Drop for LayoutMarginRight { fn drop(&mut self) { az_layout_margin_right_delete(&mut self.object); } }


    /// `LayoutMarginTop` struct
    pub struct LayoutMarginTop { pub(crate) object: AzLayoutMarginTop }

    impl LayoutMarginTop {
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginTop`
       pub fn leak(self) -> AzLayoutMarginTop { az_layout_margin_top_deep_copy(&self.object) }
    }

    impl Drop for LayoutMarginTop { fn drop(&mut self) { az_layout_margin_top_delete(&mut self.object); } }


    /// `LayoutMaxHeight` struct
    pub struct LayoutMaxHeight { pub(crate) object: AzLayoutMaxHeight }

    impl LayoutMaxHeight {
       /// Prevents the destructor from running and returns the internal `AzLayoutMaxHeight`
       pub fn leak(self) -> AzLayoutMaxHeight { az_layout_max_height_deep_copy(&self.object) }
    }

    impl Drop for LayoutMaxHeight { fn drop(&mut self) { az_layout_max_height_delete(&mut self.object); } }


    /// `LayoutMaxWidth` struct
    pub struct LayoutMaxWidth { pub(crate) object: AzLayoutMaxWidth }

    impl LayoutMaxWidth {
       /// Prevents the destructor from running and returns the internal `AzLayoutMaxWidth`
       pub fn leak(self) -> AzLayoutMaxWidth { az_layout_max_width_deep_copy(&self.object) }
    }

    impl Drop for LayoutMaxWidth { fn drop(&mut self) { az_layout_max_width_delete(&mut self.object); } }


    /// `LayoutMinHeight` struct
    pub struct LayoutMinHeight { pub(crate) object: AzLayoutMinHeight }

    impl LayoutMinHeight {
       /// Prevents the destructor from running and returns the internal `AzLayoutMinHeight`
       pub fn leak(self) -> AzLayoutMinHeight { az_layout_min_height_deep_copy(&self.object) }
    }

    impl Drop for LayoutMinHeight { fn drop(&mut self) { az_layout_min_height_delete(&mut self.object); } }


    /// `LayoutMinWidth` struct
    pub struct LayoutMinWidth { pub(crate) object: AzLayoutMinWidth }

    impl LayoutMinWidth {
       /// Prevents the destructor from running and returns the internal `AzLayoutMinWidth`
       pub fn leak(self) -> AzLayoutMinWidth { az_layout_min_width_deep_copy(&self.object) }
    }

    impl Drop for LayoutMinWidth { fn drop(&mut self) { az_layout_min_width_delete(&mut self.object); } }


    /// `LayoutPaddingBottom` struct
    pub struct LayoutPaddingBottom { pub(crate) object: AzLayoutPaddingBottom }

    impl LayoutPaddingBottom {
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingBottom`
       pub fn leak(self) -> AzLayoutPaddingBottom { az_layout_padding_bottom_deep_copy(&self.object) }
    }

    impl Drop for LayoutPaddingBottom { fn drop(&mut self) { az_layout_padding_bottom_delete(&mut self.object); } }


    /// `LayoutPaddingLeft` struct
    pub struct LayoutPaddingLeft { pub(crate) object: AzLayoutPaddingLeft }

    impl LayoutPaddingLeft {
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingLeft`
       pub fn leak(self) -> AzLayoutPaddingLeft { az_layout_padding_left_deep_copy(&self.object) }
    }

    impl Drop for LayoutPaddingLeft { fn drop(&mut self) { az_layout_padding_left_delete(&mut self.object); } }


    /// `LayoutPaddingRight` struct
    pub struct LayoutPaddingRight { pub(crate) object: AzLayoutPaddingRight }

    impl LayoutPaddingRight {
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingRight`
       pub fn leak(self) -> AzLayoutPaddingRight { az_layout_padding_right_deep_copy(&self.object) }
    }

    impl Drop for LayoutPaddingRight { fn drop(&mut self) { az_layout_padding_right_delete(&mut self.object); } }


    /// `LayoutPaddingTop` struct
    pub struct LayoutPaddingTop { pub(crate) object: AzLayoutPaddingTop }

    impl LayoutPaddingTop {
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingTop`
       pub fn leak(self) -> AzLayoutPaddingTop { az_layout_padding_top_deep_copy(&self.object) }
    }

    impl Drop for LayoutPaddingTop { fn drop(&mut self) { az_layout_padding_top_delete(&mut self.object); } }


    /// `LayoutPosition` struct
    pub struct LayoutPosition { pub(crate) object: AzLayoutPosition }

    impl LayoutPosition {
        pub fn static() -> Self { Self { object: az_layout_position_static() }  }
        pub fn relative() -> Self { Self { object: az_layout_position_relative() }  }
        pub fn absolute() -> Self { Self { object: az_layout_position_absolute() }  }
        pub fn fixed() -> Self { Self { object: az_layout_position_fixed() }  }
       /// Prevents the destructor from running and returns the internal `AzLayoutPosition`
       pub fn leak(self) -> AzLayoutPosition { az_layout_position_deep_copy(&self.object) }
    }

    impl Drop for LayoutPosition { fn drop(&mut self) { az_layout_position_delete(&mut self.object); } }


    /// `LayoutRight` struct
    pub struct LayoutRight { pub(crate) object: AzLayoutRight }

    impl LayoutRight {
       /// Prevents the destructor from running and returns the internal `AzLayoutRight`
       pub fn leak(self) -> AzLayoutRight { az_layout_right_deep_copy(&self.object) }
    }

    impl Drop for LayoutRight { fn drop(&mut self) { az_layout_right_delete(&mut self.object); } }


    /// `LayoutTop` struct
    pub struct LayoutTop { pub(crate) object: AzLayoutTop }

    impl LayoutTop {
       /// Prevents the destructor from running and returns the internal `AzLayoutTop`
       pub fn leak(self) -> AzLayoutTop { az_layout_top_deep_copy(&self.object) }
    }

    impl Drop for LayoutTop { fn drop(&mut self) { az_layout_top_delete(&mut self.object); } }


    /// `LayoutWidth` struct
    pub struct LayoutWidth { pub(crate) object: AzLayoutWidth }

    impl LayoutWidth {
       /// Prevents the destructor from running and returns the internal `AzLayoutWidth`
       pub fn leak(self) -> AzLayoutWidth { az_layout_width_deep_copy(&self.object) }
    }

    impl Drop for LayoutWidth { fn drop(&mut self) { az_layout_width_delete(&mut self.object); } }


    /// `LayoutWrap` struct
    pub struct LayoutWrap { pub(crate) object: AzLayoutWrap }

    impl LayoutWrap {
        pub fn wrap() -> Self { Self { object: az_layout_wrap_wrap() }  }
        pub fn no_wrap() -> Self { Self { object: az_layout_wrap_no_wrap() }  }
       /// Prevents the destructor from running and returns the internal `AzLayoutWrap`
       pub fn leak(self) -> AzLayoutWrap { az_layout_wrap_deep_copy(&self.object) }
    }

    impl Drop for LayoutWrap { fn drop(&mut self) { az_layout_wrap_delete(&mut self.object); } }


    /// `Overflow` struct
    pub struct Overflow { pub(crate) object: AzOverflow }

    impl Overflow {
        /// Always shows a scroll bar, overflows on scroll
        pub fn scroll() -> Self { Self { object: az_overflow_scroll() }  }
        /// Does not show a scroll bar by default, only when text is overflowing
        pub fn auto() -> Self { Self { object: az_overflow_auto() }  }
        /// Never shows a scroll bar, simply clips text
        pub fn hidden() -> Self { Self { object: az_overflow_hidden() }  }
        /// Doesn't show a scroll bar, simply overflows the text
        pub fn visible() -> Self { Self { object: az_overflow_visible() }  }
       /// Prevents the destructor from running and returns the internal `AzOverflow`
       pub fn leak(self) -> AzOverflow { az_overflow_deep_copy(&self.object) }
    }

    impl Drop for Overflow { fn drop(&mut self) { az_overflow_delete(&mut self.object); } }


    /// `PercentageValue` struct
    pub struct PercentageValue { pub(crate) object: AzPercentageValue }

    impl PercentageValue {
       /// Prevents the destructor from running and returns the internal `AzPercentageValue`
       pub fn leak(self) -> AzPercentageValue { az_percentage_value_deep_copy(&self.object) }
    }

    impl Drop for PercentageValue { fn drop(&mut self) { az_percentage_value_delete(&mut self.object); } }


    /// `GradientStopPre` struct
    pub struct GradientStopPre { pub(crate) object: AzGradientStopPre }

    impl GradientStopPre {
       /// Prevents the destructor from running and returns the internal `AzGradientStopPre`
       pub fn leak(self) -> AzGradientStopPre { az_gradient_stop_pre_deep_copy(&self.object) }
    }

    impl Drop for GradientStopPre { fn drop(&mut self) { az_gradient_stop_pre_delete(&mut self.object); } }


    /// `DirectionCorner` struct
    pub struct DirectionCorner { pub(crate) object: AzDirectionCorner }

    impl DirectionCorner {
        pub fn right() -> Self { Self { object: az_direction_corner_right() }  }
        pub fn left() -> Self { Self { object: az_direction_corner_left() }  }
        pub fn top() -> Self { Self { object: az_direction_corner_top() }  }
        pub fn bottom() -> Self { Self { object: az_direction_corner_bottom() }  }
        pub fn top_right() -> Self { Self { object: az_direction_corner_top_right() }  }
        pub fn top_left() -> Self { Self { object: az_direction_corner_top_left() }  }
        pub fn bottom_right() -> Self { Self { object: az_direction_corner_bottom_right() }  }
        pub fn bottom_left() -> Self { Self { object: az_direction_corner_bottom_left() }  }
       /// Prevents the destructor from running and returns the internal `AzDirectionCorner`
       pub fn leak(self) -> AzDirectionCorner { az_direction_corner_deep_copy(&self.object) }
    }

    impl Drop for DirectionCorner { fn drop(&mut self) { az_direction_corner_delete(&mut self.object); } }


    /// `DirectionCorners` struct
    pub struct DirectionCorners { pub(crate) object: AzDirectionCorners }

    impl DirectionCorners {
       /// Prevents the destructor from running and returns the internal `AzDirectionCorners`
       pub fn leak(self) -> AzDirectionCorners { az_direction_corners_deep_copy(&self.object) }
    }

    impl Drop for DirectionCorners { fn drop(&mut self) { az_direction_corners_delete(&mut self.object); } }


    /// `Direction` struct
    pub struct Direction { pub(crate) object: AzDirection }

    impl Direction {
        pub fn angle(variant_data: crate::css::FloatValue) -> Self { Self { object: az_direction_angle(variant_data.leak()) }}
        pub fn from_to(variant_data: crate::css::DirectionCorners) -> Self { Self { object: az_direction_from_to(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzDirection`
       pub fn leak(self) -> AzDirection { az_direction_deep_copy(&self.object) }
    }

    impl Drop for Direction { fn drop(&mut self) { az_direction_delete(&mut self.object); } }


    /// `ExtendMode` struct
    pub struct ExtendMode { pub(crate) object: AzExtendMode }

    impl ExtendMode {
        pub fn clamp() -> Self { Self { object: az_extend_mode_clamp() }  }
        pub fn repeat() -> Self { Self { object: az_extend_mode_repeat() }  }
       /// Prevents the destructor from running and returns the internal `AzExtendMode`
       pub fn leak(self) -> AzExtendMode { az_extend_mode_deep_copy(&self.object) }
    }

    impl Drop for ExtendMode { fn drop(&mut self) { az_extend_mode_delete(&mut self.object); } }


    /// `LinearGradient` struct
    pub struct LinearGradient { pub(crate) object: AzLinearGradient }

    impl LinearGradient {
       /// Prevents the destructor from running and returns the internal `AzLinearGradient`
       pub fn leak(self) -> AzLinearGradient { az_linear_gradient_deep_copy(&self.object) }
    }

    impl Drop for LinearGradient { fn drop(&mut self) { az_linear_gradient_delete(&mut self.object); } }


    /// `Shape` struct
    pub struct Shape { pub(crate) object: AzShape }

    impl Shape {
        pub fn ellipse() -> Self { Self { object: az_shape_ellipse() }  }
        pub fn circle() -> Self { Self { object: az_shape_circle() }  }
       /// Prevents the destructor from running and returns the internal `AzShape`
       pub fn leak(self) -> AzShape { az_shape_deep_copy(&self.object) }
    }

    impl Drop for Shape { fn drop(&mut self) { az_shape_delete(&mut self.object); } }


    /// `RadialGradient` struct
    pub struct RadialGradient { pub(crate) object: AzRadialGradient }

    impl RadialGradient {
       /// Prevents the destructor from running and returns the internal `AzRadialGradient`
       pub fn leak(self) -> AzRadialGradient { az_radial_gradient_deep_copy(&self.object) }
    }

    impl Drop for RadialGradient { fn drop(&mut self) { az_radial_gradient_delete(&mut self.object); } }


    /// `CssImageId` struct
    pub struct CssImageId { pub(crate) object: AzCssImageId }

    impl CssImageId {
       /// Prevents the destructor from running and returns the internal `AzCssImageId`
       pub fn leak(self) -> AzCssImageId { az_css_image_id_deep_copy(&self.object) }
    }

    impl Drop for CssImageId { fn drop(&mut self) { az_css_image_id_delete(&mut self.object); } }


    /// `StyleBackgroundContent` struct
    pub struct StyleBackgroundContent { pub(crate) object: AzStyleBackgroundContent }

    impl StyleBackgroundContent {
        pub fn linear_gradient(variant_data: crate::css::LinearGradient) -> Self { Self { object: az_style_background_content_linear_gradient(variant_data.leak()) }}
        pub fn radial_gradient(variant_data: crate::css::RadialGradient) -> Self { Self { object: az_style_background_content_radial_gradient(variant_data.leak()) }}
        pub fn image(variant_data: crate::css::CssImageId) -> Self { Self { object: az_style_background_content_image(variant_data.leak()) }}
        pub fn color(variant_data: crate::css::ColorU) -> Self { Self { object: az_style_background_content_color(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundContent`
       pub fn leak(self) -> AzStyleBackgroundContent { az_style_background_content_deep_copy(&self.object) }
    }

    impl Drop for StyleBackgroundContent { fn drop(&mut self) { az_style_background_content_delete(&mut self.object); } }


    /// `BackgroundPositionHorizontal` struct
    pub struct BackgroundPositionHorizontal { pub(crate) object: AzBackgroundPositionHorizontal }

    impl BackgroundPositionHorizontal {
        pub fn left() -> Self { Self { object: az_background_position_horizontal_left() }  }
        pub fn center() -> Self { Self { object: az_background_position_horizontal_center() }  }
        pub fn right() -> Self { Self { object: az_background_position_horizontal_right() }  }
        pub fn exact(variant_data: crate::css::PixelValue) -> Self { Self { object: az_background_position_horizontal_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzBackgroundPositionHorizontal`
       pub fn leak(self) -> AzBackgroundPositionHorizontal { az_background_position_horizontal_deep_copy(&self.object) }
    }

    impl Drop for BackgroundPositionHorizontal { fn drop(&mut self) { az_background_position_horizontal_delete(&mut self.object); } }


    /// `BackgroundPositionVertical` struct
    pub struct BackgroundPositionVertical { pub(crate) object: AzBackgroundPositionVertical }

    impl BackgroundPositionVertical {
        pub fn top() -> Self { Self { object: az_background_position_vertical_top() }  }
        pub fn center() -> Self { Self { object: az_background_position_vertical_center() }  }
        pub fn bottom() -> Self { Self { object: az_background_position_vertical_bottom() }  }
        pub fn exact(variant_data: crate::css::PixelValue) -> Self { Self { object: az_background_position_vertical_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzBackgroundPositionVertical`
       pub fn leak(self) -> AzBackgroundPositionVertical { az_background_position_vertical_deep_copy(&self.object) }
    }

    impl Drop for BackgroundPositionVertical { fn drop(&mut self) { az_background_position_vertical_delete(&mut self.object); } }


    /// `StyleBackgroundPosition` struct
    pub struct StyleBackgroundPosition { pub(crate) object: AzStyleBackgroundPosition }

    impl StyleBackgroundPosition {
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundPosition`
       pub fn leak(self) -> AzStyleBackgroundPosition { az_style_background_position_deep_copy(&self.object) }
    }

    impl Drop for StyleBackgroundPosition { fn drop(&mut self) { az_style_background_position_delete(&mut self.object); } }


    /// `StyleBackgroundRepeat` struct
    pub struct StyleBackgroundRepeat { pub(crate) object: AzStyleBackgroundRepeat }

    impl StyleBackgroundRepeat {
        pub fn no_repeat() -> Self { Self { object: az_style_background_repeat_no_repeat() }  }
        pub fn repeat() -> Self { Self { object: az_style_background_repeat_repeat() }  }
        pub fn repeat_x() -> Self { Self { object: az_style_background_repeat_repeat_x() }  }
        pub fn repeat_y() -> Self { Self { object: az_style_background_repeat_repeat_y() }  }
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundRepeat`
       pub fn leak(self) -> AzStyleBackgroundRepeat { az_style_background_repeat_deep_copy(&self.object) }
    }

    impl Drop for StyleBackgroundRepeat { fn drop(&mut self) { az_style_background_repeat_delete(&mut self.object); } }


    /// `StyleBackgroundSize` struct
    pub struct StyleBackgroundSize { pub(crate) object: AzStyleBackgroundSize }

    impl StyleBackgroundSize {
        pub fn exact_size(variant_data: [crate::css::PixelValue;2]) -> Self { Self { object: az_style_background_size_exact_size(variant_data.leak()) }}
        pub fn contain() -> Self { Self { object: az_style_background_size_contain() }  }
        pub fn cover() -> Self { Self { object: az_style_background_size_cover() }  }
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundSize`
       pub fn leak(self) -> AzStyleBackgroundSize { az_style_background_size_deep_copy(&self.object) }
    }

    impl Drop for StyleBackgroundSize { fn drop(&mut self) { az_style_background_size_delete(&mut self.object); } }


    /// `StyleBorderBottomColor` struct
    pub struct StyleBorderBottomColor { pub(crate) object: AzStyleBorderBottomColor }

    impl StyleBorderBottomColor {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomColor`
       pub fn leak(self) -> AzStyleBorderBottomColor { az_style_border_bottom_color_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomColor { fn drop(&mut self) { az_style_border_bottom_color_delete(&mut self.object); } }


    /// `StyleBorderBottomLeftRadius` struct
    pub struct StyleBorderBottomLeftRadius { pub(crate) object: AzStyleBorderBottomLeftRadius }

    impl StyleBorderBottomLeftRadius {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomLeftRadius`
       pub fn leak(self) -> AzStyleBorderBottomLeftRadius { az_style_border_bottom_left_radius_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomLeftRadius { fn drop(&mut self) { az_style_border_bottom_left_radius_delete(&mut self.object); } }


    /// `StyleBorderBottomRightRadius` struct
    pub struct StyleBorderBottomRightRadius { pub(crate) object: AzStyleBorderBottomRightRadius }

    impl StyleBorderBottomRightRadius {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomRightRadius`
       pub fn leak(self) -> AzStyleBorderBottomRightRadius { az_style_border_bottom_right_radius_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomRightRadius { fn drop(&mut self) { az_style_border_bottom_right_radius_delete(&mut self.object); } }


    /// `BorderStyle` struct
    pub struct BorderStyle { pub(crate) object: AzBorderStyle }

    impl BorderStyle {
        pub fn none() -> Self { Self { object: az_border_style_none() }  }
        pub fn solid() -> Self { Self { object: az_border_style_solid() }  }
        pub fn double() -> Self { Self { object: az_border_style_double() }  }
        pub fn dotted() -> Self { Self { object: az_border_style_dotted() }  }
        pub fn dashed() -> Self { Self { object: az_border_style_dashed() }  }
        pub fn hidden() -> Self { Self { object: az_border_style_hidden() }  }
        pub fn groove() -> Self { Self { object: az_border_style_groove() }  }
        pub fn ridge() -> Self { Self { object: az_border_style_ridge() }  }
        pub fn inset() -> Self { Self { object: az_border_style_inset() }  }
        pub fn outset() -> Self { Self { object: az_border_style_outset() }  }
       /// Prevents the destructor from running and returns the internal `AzBorderStyle`
       pub fn leak(self) -> AzBorderStyle { az_border_style_deep_copy(&self.object) }
    }

    impl Drop for BorderStyle { fn drop(&mut self) { az_border_style_delete(&mut self.object); } }


    /// `StyleBorderBottomStyle` struct
    pub struct StyleBorderBottomStyle { pub(crate) object: AzStyleBorderBottomStyle }

    impl StyleBorderBottomStyle {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomStyle`
       pub fn leak(self) -> AzStyleBorderBottomStyle { az_style_border_bottom_style_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomStyle { fn drop(&mut self) { az_style_border_bottom_style_delete(&mut self.object); } }


    /// `StyleBorderBottomWidth` struct
    pub struct StyleBorderBottomWidth { pub(crate) object: AzStyleBorderBottomWidth }

    impl StyleBorderBottomWidth {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomWidth`
       pub fn leak(self) -> AzStyleBorderBottomWidth { az_style_border_bottom_width_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomWidth { fn drop(&mut self) { az_style_border_bottom_width_delete(&mut self.object); } }


    /// `StyleBorderLeftColor` struct
    pub struct StyleBorderLeftColor { pub(crate) object: AzStyleBorderLeftColor }

    impl StyleBorderLeftColor {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderLeftColor`
       pub fn leak(self) -> AzStyleBorderLeftColor { az_style_border_left_color_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderLeftColor { fn drop(&mut self) { az_style_border_left_color_delete(&mut self.object); } }


    /// `StyleBorderLeftStyle` struct
    pub struct StyleBorderLeftStyle { pub(crate) object: AzStyleBorderLeftStyle }

    impl StyleBorderLeftStyle {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderLeftStyle`
       pub fn leak(self) -> AzStyleBorderLeftStyle { az_style_border_left_style_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderLeftStyle { fn drop(&mut self) { az_style_border_left_style_delete(&mut self.object); } }


    /// `StyleBorderLeftWidth` struct
    pub struct StyleBorderLeftWidth { pub(crate) object: AzStyleBorderLeftWidth }

    impl StyleBorderLeftWidth {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderLeftWidth`
       pub fn leak(self) -> AzStyleBorderLeftWidth { az_style_border_left_width_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderLeftWidth { fn drop(&mut self) { az_style_border_left_width_delete(&mut self.object); } }


    /// `StyleBorderRightColor` struct
    pub struct StyleBorderRightColor { pub(crate) object: AzStyleBorderRightColor }

    impl StyleBorderRightColor {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderRightColor`
       pub fn leak(self) -> AzStyleBorderRightColor { az_style_border_right_color_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderRightColor { fn drop(&mut self) { az_style_border_right_color_delete(&mut self.object); } }


    /// `StyleBorderRightStyle` struct
    pub struct StyleBorderRightStyle { pub(crate) object: AzStyleBorderRightStyle }

    impl StyleBorderRightStyle {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderRightStyle`
       pub fn leak(self) -> AzStyleBorderRightStyle { az_style_border_right_style_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderRightStyle { fn drop(&mut self) { az_style_border_right_style_delete(&mut self.object); } }


    /// `StyleBorderRightWidth` struct
    pub struct StyleBorderRightWidth { pub(crate) ptr: AzStyleBorderRightWidthPtr }

    impl StyleBorderRightWidth {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderRightWidthPtr`
       pub fn leak(self) -> AzStyleBorderRightWidthPtr { let p = az_style_border_right_width_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for StyleBorderRightWidth { fn drop(&mut self) { az_style_border_right_width_delete(&mut self.ptr); } }


    /// `StyleBorderTopColor` struct
    pub struct StyleBorderTopColor { pub(crate) object: AzStyleBorderTopColor }

    impl StyleBorderTopColor {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopColor`
       pub fn leak(self) -> AzStyleBorderTopColor { az_style_border_top_color_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopColor { fn drop(&mut self) { az_style_border_top_color_delete(&mut self.object); } }


    /// `StyleBorderTopLeftRadius` struct
    pub struct StyleBorderTopLeftRadius { pub(crate) object: AzStyleBorderTopLeftRadius }

    impl StyleBorderTopLeftRadius {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopLeftRadius`
       pub fn leak(self) -> AzStyleBorderTopLeftRadius { az_style_border_top_left_radius_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopLeftRadius { fn drop(&mut self) { az_style_border_top_left_radius_delete(&mut self.object); } }


    /// `StyleBorderTopRightRadius` struct
    pub struct StyleBorderTopRightRadius { pub(crate) object: AzStyleBorderTopRightRadius }

    impl StyleBorderTopRightRadius {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopRightRadius`
       pub fn leak(self) -> AzStyleBorderTopRightRadius { az_style_border_top_right_radius_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopRightRadius { fn drop(&mut self) { az_style_border_top_right_radius_delete(&mut self.object); } }


    /// `StyleBorderTopStyle` struct
    pub struct StyleBorderTopStyle { pub(crate) object: AzStyleBorderTopStyle }

    impl StyleBorderTopStyle {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopStyle`
       pub fn leak(self) -> AzStyleBorderTopStyle { az_style_border_top_style_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopStyle { fn drop(&mut self) { az_style_border_top_style_delete(&mut self.object); } }


    /// `StyleBorderTopWidth` struct
    pub struct StyleBorderTopWidth { pub(crate) object: AzStyleBorderTopWidth }

    impl StyleBorderTopWidth {
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopWidth`
       pub fn leak(self) -> AzStyleBorderTopWidth { az_style_border_top_width_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopWidth { fn drop(&mut self) { az_style_border_top_width_delete(&mut self.object); } }


    /// `StyleCursor` struct
    pub struct StyleCursor { pub(crate) object: AzStyleCursor }

    impl StyleCursor {
        pub fn alias() -> Self { Self { object: az_style_cursor_alias() }  }
        pub fn all_scroll() -> Self { Self { object: az_style_cursor_all_scroll() }  }
        pub fn cell() -> Self { Self { object: az_style_cursor_cell() }  }
        pub fn col_resize() -> Self { Self { object: az_style_cursor_col_resize() }  }
        pub fn context_menu() -> Self { Self { object: az_style_cursor_context_menu() }  }
        pub fn copy() -> Self { Self { object: az_style_cursor_copy() }  }
        pub fn crosshair() -> Self { Self { object: az_style_cursor_crosshair() }  }
        pub fn default() -> Self { Self { object: az_style_cursor_default() }  }
        pub fn e_resize() -> Self { Self { object: az_style_cursor_e_resize() }  }
        pub fn ew_resize() -> Self { Self { object: az_style_cursor_ew_resize() }  }
        pub fn grab() -> Self { Self { object: az_style_cursor_grab() }  }
        pub fn grabbing() -> Self { Self { object: az_style_cursor_grabbing() }  }
        pub fn help() -> Self { Self { object: az_style_cursor_help() }  }
        pub fn move() -> Self { Self { object: az_style_cursor_move() }  }
        pub fn n_resize() -> Self { Self { object: az_style_cursor_n_resize() }  }
        pub fn ns_resize() -> Self { Self { object: az_style_cursor_ns_resize() }  }
        pub fn nesw_resize() -> Self { Self { object: az_style_cursor_nesw_resize() }  }
        pub fn nwse_resize() -> Self { Self { object: az_style_cursor_nwse_resize() }  }
        pub fn pointer() -> Self { Self { object: az_style_cursor_pointer() }  }
        pub fn progress() -> Self { Self { object: az_style_cursor_progress() }  }
        pub fn row_resize() -> Self { Self { object: az_style_cursor_row_resize() }  }
        pub fn s_resize() -> Self { Self { object: az_style_cursor_s_resize() }  }
        pub fn se_resize() -> Self { Self { object: az_style_cursor_se_resize() }  }
        pub fn text() -> Self { Self { object: az_style_cursor_text() }  }
        pub fn unset() -> Self { Self { object: az_style_cursor_unset() }  }
        pub fn vertical_text() -> Self { Self { object: az_style_cursor_vertical_text() }  }
        pub fn w_resize() -> Self { Self { object: az_style_cursor_w_resize() }  }
        pub fn wait() -> Self { Self { object: az_style_cursor_wait() }  }
        pub fn zoom_in() -> Self { Self { object: az_style_cursor_zoom_in() }  }
        pub fn zoom_out() -> Self { Self { object: az_style_cursor_zoom_out() }  }
       /// Prevents the destructor from running and returns the internal `AzStyleCursor`
       pub fn leak(self) -> AzStyleCursor { az_style_cursor_deep_copy(&self.object) }
    }

    impl Drop for StyleCursor { fn drop(&mut self) { az_style_cursor_delete(&mut self.object); } }


    /// `StyleFontFamily` struct
    pub struct StyleFontFamily { pub(crate) object: AzStyleFontFamily }

    impl StyleFontFamily {
       /// Prevents the destructor from running and returns the internal `AzStyleFontFamily`
       pub fn leak(self) -> AzStyleFontFamily { az_style_font_family_deep_copy(&self.object) }
    }

    impl Drop for StyleFontFamily { fn drop(&mut self) { az_style_font_family_delete(&mut self.object); } }


    /// `StyleFontSize` struct
    pub struct StyleFontSize { pub(crate) object: AzStyleFontSize }

    impl StyleFontSize {
       /// Prevents the destructor from running and returns the internal `AzStyleFontSize`
       pub fn leak(self) -> AzStyleFontSize { az_style_font_size_deep_copy(&self.object) }
    }

    impl Drop for StyleFontSize { fn drop(&mut self) { az_style_font_size_delete(&mut self.object); } }


    /// `StyleLetterSpacing` struct
    pub struct StyleLetterSpacing { pub(crate) object: AzStyleLetterSpacing }

    impl StyleLetterSpacing {
       /// Prevents the destructor from running and returns the internal `AzStyleLetterSpacing`
       pub fn leak(self) -> AzStyleLetterSpacing { az_style_letter_spacing_deep_copy(&self.object) }
    }

    impl Drop for StyleLetterSpacing { fn drop(&mut self) { az_style_letter_spacing_delete(&mut self.object); } }


    /// `StyleLineHeight` struct
    pub struct StyleLineHeight { pub(crate) object: AzStyleLineHeight }

    impl StyleLineHeight {
       /// Prevents the destructor from running and returns the internal `AzStyleLineHeight`
       pub fn leak(self) -> AzStyleLineHeight { az_style_line_height_deep_copy(&self.object) }
    }

    impl Drop for StyleLineHeight { fn drop(&mut self) { az_style_line_height_delete(&mut self.object); } }


    /// `StyleTabWidth` struct
    pub struct StyleTabWidth { pub(crate) object: AzStyleTabWidth }

    impl StyleTabWidth {
       /// Prevents the destructor from running and returns the internal `AzStyleTabWidth`
       pub fn leak(self) -> AzStyleTabWidth { az_style_tab_width_deep_copy(&self.object) }
    }

    impl Drop for StyleTabWidth { fn drop(&mut self) { az_style_tab_width_delete(&mut self.object); } }


    /// `StyleTextAlignmentHorz` struct
    pub struct StyleTextAlignmentHorz { pub(crate) object: AzStyleTextAlignmentHorz }

    impl StyleTextAlignmentHorz {
        pub fn left() -> Self { Self { object: az_style_text_alignment_horz_left() }  }
        pub fn center() -> Self { Self { object: az_style_text_alignment_horz_center() }  }
        pub fn right() -> Self { Self { object: az_style_text_alignment_horz_right() }  }
       /// Prevents the destructor from running and returns the internal `AzStyleTextAlignmentHorz`
       pub fn leak(self) -> AzStyleTextAlignmentHorz { az_style_text_alignment_horz_deep_copy(&self.object) }
    }

    impl Drop for StyleTextAlignmentHorz { fn drop(&mut self) { az_style_text_alignment_horz_delete(&mut self.object); } }


    /// `StyleTextColor` struct
    pub struct StyleTextColor { pub(crate) object: AzStyleTextColor }

    impl StyleTextColor {
       /// Prevents the destructor from running and returns the internal `AzStyleTextColor`
       pub fn leak(self) -> AzStyleTextColor { az_style_text_color_deep_copy(&self.object) }
    }

    impl Drop for StyleTextColor { fn drop(&mut self) { az_style_text_color_delete(&mut self.object); } }


    /// `StyleWordSpacing` struct
    pub struct StyleWordSpacing { pub(crate) object: AzStyleWordSpacing }

    impl StyleWordSpacing {
       /// Prevents the destructor from running and returns the internal `AzStyleWordSpacing`
       pub fn leak(self) -> AzStyleWordSpacing { az_style_word_spacing_deep_copy(&self.object) }
    }

    impl Drop for StyleWordSpacing { fn drop(&mut self) { az_style_word_spacing_delete(&mut self.object); } }


    /// `BoxShadowPreDisplayItemValue` struct
    pub struct BoxShadowPreDisplayItemValue { pub(crate) object: AzBoxShadowPreDisplayItemValue }

    impl BoxShadowPreDisplayItemValue {
        pub fn auto() -> Self { Self { object: az_box_shadow_pre_display_item_value_auto() }  }
        pub fn none() -> Self { Self { object: az_box_shadow_pre_display_item_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_box_shadow_pre_display_item_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_box_shadow_pre_display_item_value_initial() }  }
        pub fn exact(variant_data: crate::css::BoxShadowPreDisplayItem) -> Self { Self { object: az_box_shadow_pre_display_item_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzBoxShadowPreDisplayItemValue`
       pub fn leak(self) -> AzBoxShadowPreDisplayItemValue { az_box_shadow_pre_display_item_value_deep_copy(&self.object) }
    }

    impl Drop for BoxShadowPreDisplayItemValue { fn drop(&mut self) { az_box_shadow_pre_display_item_value_delete(&mut self.object); } }


    /// `LayoutAlignContentValue` struct
    pub struct LayoutAlignContentValue { pub(crate) object: AzLayoutAlignContentValue }

    impl LayoutAlignContentValue {
        pub fn auto() -> Self { Self { object: az_layout_align_content_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_align_content_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_align_content_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_align_content_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutAlignContent) -> Self { Self { object: az_layout_align_content_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutAlignContentValue`
       pub fn leak(self) -> AzLayoutAlignContentValue { az_layout_align_content_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutAlignContentValue { fn drop(&mut self) { az_layout_align_content_value_delete(&mut self.object); } }


    /// `LayoutAlignItemsValue` struct
    pub struct LayoutAlignItemsValue { pub(crate) object: AzLayoutAlignItemsValue }

    impl LayoutAlignItemsValue {
        pub fn auto() -> Self { Self { object: az_layout_align_items_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_align_items_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_align_items_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_align_items_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutAlignItems) -> Self { Self { object: az_layout_align_items_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutAlignItemsValue`
       pub fn leak(self) -> AzLayoutAlignItemsValue { az_layout_align_items_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutAlignItemsValue { fn drop(&mut self) { az_layout_align_items_value_delete(&mut self.object); } }


    /// `LayoutBottomValue` struct
    pub struct LayoutBottomValue { pub(crate) object: AzLayoutBottomValue }

    impl LayoutBottomValue {
        pub fn auto() -> Self { Self { object: az_layout_bottom_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_bottom_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_bottom_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_bottom_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutBottom) -> Self { Self { object: az_layout_bottom_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutBottomValue`
       pub fn leak(self) -> AzLayoutBottomValue { az_layout_bottom_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutBottomValue { fn drop(&mut self) { az_layout_bottom_value_delete(&mut self.object); } }


    /// `LayoutBoxSizingValue` struct
    pub struct LayoutBoxSizingValue { pub(crate) object: AzLayoutBoxSizingValue }

    impl LayoutBoxSizingValue {
        pub fn auto() -> Self { Self { object: az_layout_box_sizing_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_box_sizing_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_box_sizing_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_box_sizing_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutBoxSizing) -> Self { Self { object: az_layout_box_sizing_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutBoxSizingValue`
       pub fn leak(self) -> AzLayoutBoxSizingValue { az_layout_box_sizing_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutBoxSizingValue { fn drop(&mut self) { az_layout_box_sizing_value_delete(&mut self.object); } }


    /// `LayoutDirectionValue` struct
    pub struct LayoutDirectionValue { pub(crate) object: AzLayoutDirectionValue }

    impl LayoutDirectionValue {
        pub fn auto() -> Self { Self { object: az_layout_direction_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_direction_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_direction_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_direction_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutDirection) -> Self { Self { object: az_layout_direction_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutDirectionValue`
       pub fn leak(self) -> AzLayoutDirectionValue { az_layout_direction_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutDirectionValue { fn drop(&mut self) { az_layout_direction_value_delete(&mut self.object); } }


    /// `LayoutDisplayValue` struct
    pub struct LayoutDisplayValue { pub(crate) object: AzLayoutDisplayValue }

    impl LayoutDisplayValue {
        pub fn auto() -> Self { Self { object: az_layout_display_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_display_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_display_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_display_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutDisplay) -> Self { Self { object: az_layout_display_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutDisplayValue`
       pub fn leak(self) -> AzLayoutDisplayValue { az_layout_display_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutDisplayValue { fn drop(&mut self) { az_layout_display_value_delete(&mut self.object); } }


    /// `LayoutFlexGrowValue` struct
    pub struct LayoutFlexGrowValue { pub(crate) object: AzLayoutFlexGrowValue }

    impl LayoutFlexGrowValue {
        pub fn auto() -> Self { Self { object: az_layout_flex_grow_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_flex_grow_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_flex_grow_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_flex_grow_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutFlexGrow) -> Self { Self { object: az_layout_flex_grow_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutFlexGrowValue`
       pub fn leak(self) -> AzLayoutFlexGrowValue { az_layout_flex_grow_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutFlexGrowValue { fn drop(&mut self) { az_layout_flex_grow_value_delete(&mut self.object); } }


    /// `LayoutFlexShrinkValue` struct
    pub struct LayoutFlexShrinkValue { pub(crate) object: AzLayoutFlexShrinkValue }

    impl LayoutFlexShrinkValue {
        pub fn auto() -> Self { Self { object: az_layout_flex_shrink_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_flex_shrink_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_flex_shrink_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_flex_shrink_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutFlexShrink) -> Self { Self { object: az_layout_flex_shrink_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutFlexShrinkValue`
       pub fn leak(self) -> AzLayoutFlexShrinkValue { az_layout_flex_shrink_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutFlexShrinkValue { fn drop(&mut self) { az_layout_flex_shrink_value_delete(&mut self.object); } }


    /// `LayoutFloatValue` struct
    pub struct LayoutFloatValue { pub(crate) object: AzLayoutFloatValue }

    impl LayoutFloatValue {
        pub fn auto() -> Self { Self { object: az_layout_float_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_float_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_float_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_float_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutFloat) -> Self { Self { object: az_layout_float_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutFloatValue`
       pub fn leak(self) -> AzLayoutFloatValue { az_layout_float_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutFloatValue { fn drop(&mut self) { az_layout_float_value_delete(&mut self.object); } }


    /// `LayoutHeightValue` struct
    pub struct LayoutHeightValue { pub(crate) object: AzLayoutHeightValue }

    impl LayoutHeightValue {
        pub fn auto() -> Self { Self { object: az_layout_height_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_height_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_height_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_height_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutHeight) -> Self { Self { object: az_layout_height_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutHeightValue`
       pub fn leak(self) -> AzLayoutHeightValue { az_layout_height_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutHeightValue { fn drop(&mut self) { az_layout_height_value_delete(&mut self.object); } }


    /// `LayoutJustifyContentValue` struct
    pub struct LayoutJustifyContentValue { pub(crate) object: AzLayoutJustifyContentValue }

    impl LayoutJustifyContentValue {
        pub fn auto() -> Self { Self { object: az_layout_justify_content_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_justify_content_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_justify_content_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_justify_content_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutJustifyContent) -> Self { Self { object: az_layout_justify_content_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutJustifyContentValue`
       pub fn leak(self) -> AzLayoutJustifyContentValue { az_layout_justify_content_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutJustifyContentValue { fn drop(&mut self) { az_layout_justify_content_value_delete(&mut self.object); } }


    /// `LayoutLeftValue` struct
    pub struct LayoutLeftValue { pub(crate) object: AzLayoutLeftValue }

    impl LayoutLeftValue {
        pub fn auto() -> Self { Self { object: az_layout_left_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_left_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_left_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_left_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutLeft) -> Self { Self { object: az_layout_left_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutLeftValue`
       pub fn leak(self) -> AzLayoutLeftValue { az_layout_left_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutLeftValue { fn drop(&mut self) { az_layout_left_value_delete(&mut self.object); } }


    /// `LayoutMarginBottomValue` struct
    pub struct LayoutMarginBottomValue { pub(crate) object: AzLayoutMarginBottomValue }

    impl LayoutMarginBottomValue {
        pub fn auto() -> Self { Self { object: az_layout_margin_bottom_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_margin_bottom_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_margin_bottom_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_margin_bottom_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMarginBottom) -> Self { Self { object: az_layout_margin_bottom_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginBottomValue`
       pub fn leak(self) -> AzLayoutMarginBottomValue { az_layout_margin_bottom_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMarginBottomValue { fn drop(&mut self) { az_layout_margin_bottom_value_delete(&mut self.object); } }


    /// `LayoutMarginLeftValue` struct
    pub struct LayoutMarginLeftValue { pub(crate) object: AzLayoutMarginLeftValue }

    impl LayoutMarginLeftValue {
        pub fn auto() -> Self { Self { object: az_layout_margin_left_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_margin_left_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_margin_left_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_margin_left_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMarginLeft) -> Self { Self { object: az_layout_margin_left_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginLeftValue`
       pub fn leak(self) -> AzLayoutMarginLeftValue { az_layout_margin_left_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMarginLeftValue { fn drop(&mut self) { az_layout_margin_left_value_delete(&mut self.object); } }


    /// `LayoutMarginRightValue` struct
    pub struct LayoutMarginRightValue { pub(crate) object: AzLayoutMarginRightValue }

    impl LayoutMarginRightValue {
        pub fn auto() -> Self { Self { object: az_layout_margin_right_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_margin_right_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_margin_right_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_margin_right_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMarginRight) -> Self { Self { object: az_layout_margin_right_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginRightValue`
       pub fn leak(self) -> AzLayoutMarginRightValue { az_layout_margin_right_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMarginRightValue { fn drop(&mut self) { az_layout_margin_right_value_delete(&mut self.object); } }


    /// `LayoutMarginTopValue` struct
    pub struct LayoutMarginTopValue { pub(crate) object: AzLayoutMarginTopValue }

    impl LayoutMarginTopValue {
        pub fn auto() -> Self { Self { object: az_layout_margin_top_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_margin_top_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_margin_top_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_margin_top_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMarginTop) -> Self { Self { object: az_layout_margin_top_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMarginTopValue`
       pub fn leak(self) -> AzLayoutMarginTopValue { az_layout_margin_top_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMarginTopValue { fn drop(&mut self) { az_layout_margin_top_value_delete(&mut self.object); } }


    /// `LayoutMaxHeightValue` struct
    pub struct LayoutMaxHeightValue { pub(crate) object: AzLayoutMaxHeightValue }

    impl LayoutMaxHeightValue {
        pub fn auto() -> Self { Self { object: az_layout_max_height_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_max_height_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_max_height_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_max_height_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMaxHeight) -> Self { Self { object: az_layout_max_height_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMaxHeightValue`
       pub fn leak(self) -> AzLayoutMaxHeightValue { az_layout_max_height_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMaxHeightValue { fn drop(&mut self) { az_layout_max_height_value_delete(&mut self.object); } }


    /// `LayoutMaxWidthValue` struct
    pub struct LayoutMaxWidthValue { pub(crate) object: AzLayoutMaxWidthValue }

    impl LayoutMaxWidthValue {
        pub fn auto() -> Self { Self { object: az_layout_max_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_max_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_max_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_max_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMaxWidth) -> Self { Self { object: az_layout_max_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMaxWidthValue`
       pub fn leak(self) -> AzLayoutMaxWidthValue { az_layout_max_width_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMaxWidthValue { fn drop(&mut self) { az_layout_max_width_value_delete(&mut self.object); } }


    /// `LayoutMinHeightValue` struct
    pub struct LayoutMinHeightValue { pub(crate) object: AzLayoutMinHeightValue }

    impl LayoutMinHeightValue {
        pub fn auto() -> Self { Self { object: az_layout_min_height_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_min_height_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_min_height_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_min_height_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMinHeight) -> Self { Self { object: az_layout_min_height_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMinHeightValue`
       pub fn leak(self) -> AzLayoutMinHeightValue { az_layout_min_height_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMinHeightValue { fn drop(&mut self) { az_layout_min_height_value_delete(&mut self.object); } }


    /// `LayoutMinWidthValue` struct
    pub struct LayoutMinWidthValue { pub(crate) object: AzLayoutMinWidthValue }

    impl LayoutMinWidthValue {
        pub fn auto() -> Self { Self { object: az_layout_min_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_min_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_min_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_min_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutMinWidth) -> Self { Self { object: az_layout_min_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutMinWidthValue`
       pub fn leak(self) -> AzLayoutMinWidthValue { az_layout_min_width_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutMinWidthValue { fn drop(&mut self) { az_layout_min_width_value_delete(&mut self.object); } }


    /// `LayoutPaddingBottomValue` struct
    pub struct LayoutPaddingBottomValue { pub(crate) object: AzLayoutPaddingBottomValue }

    impl LayoutPaddingBottomValue {
        pub fn auto() -> Self { Self { object: az_layout_padding_bottom_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_padding_bottom_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_padding_bottom_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_padding_bottom_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutPaddingBottom) -> Self { Self { object: az_layout_padding_bottom_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingBottomValue`
       pub fn leak(self) -> AzLayoutPaddingBottomValue { az_layout_padding_bottom_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutPaddingBottomValue { fn drop(&mut self) { az_layout_padding_bottom_value_delete(&mut self.object); } }


    /// `LayoutPaddingLeftValue` struct
    pub struct LayoutPaddingLeftValue { pub(crate) object: AzLayoutPaddingLeftValue }

    impl LayoutPaddingLeftValue {
        pub fn auto() -> Self { Self { object: az_layout_padding_left_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_padding_left_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_padding_left_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_padding_left_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutPaddingLeft) -> Self { Self { object: az_layout_padding_left_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingLeftValue`
       pub fn leak(self) -> AzLayoutPaddingLeftValue { az_layout_padding_left_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutPaddingLeftValue { fn drop(&mut self) { az_layout_padding_left_value_delete(&mut self.object); } }


    /// `LayoutPaddingRightValue` struct
    pub struct LayoutPaddingRightValue { pub(crate) object: AzLayoutPaddingRightValue }

    impl LayoutPaddingRightValue {
        pub fn auto() -> Self { Self { object: az_layout_padding_right_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_padding_right_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_padding_right_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_padding_right_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutPaddingRight) -> Self { Self { object: az_layout_padding_right_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingRightValue`
       pub fn leak(self) -> AzLayoutPaddingRightValue { az_layout_padding_right_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutPaddingRightValue { fn drop(&mut self) { az_layout_padding_right_value_delete(&mut self.object); } }


    /// `LayoutPaddingTopValue` struct
    pub struct LayoutPaddingTopValue { pub(crate) object: AzLayoutPaddingTopValue }

    impl LayoutPaddingTopValue {
        pub fn auto() -> Self { Self { object: az_layout_padding_top_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_padding_top_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_padding_top_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_padding_top_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutPaddingTop) -> Self { Self { object: az_layout_padding_top_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutPaddingTopValue`
       pub fn leak(self) -> AzLayoutPaddingTopValue { az_layout_padding_top_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutPaddingTopValue { fn drop(&mut self) { az_layout_padding_top_value_delete(&mut self.object); } }


    /// `LayoutPositionValue` struct
    pub struct LayoutPositionValue { pub(crate) object: AzLayoutPositionValue }

    impl LayoutPositionValue {
        pub fn auto() -> Self { Self { object: az_layout_position_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_position_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_position_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_position_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutPosition) -> Self { Self { object: az_layout_position_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutPositionValue`
       pub fn leak(self) -> AzLayoutPositionValue { az_layout_position_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutPositionValue { fn drop(&mut self) { az_layout_position_value_delete(&mut self.object); } }


    /// `LayoutRightValue` struct
    pub struct LayoutRightValue { pub(crate) object: AzLayoutRightValue }

    impl LayoutRightValue {
        pub fn auto() -> Self { Self { object: az_layout_right_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_right_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_right_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_right_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutRight) -> Self { Self { object: az_layout_right_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutRightValue`
       pub fn leak(self) -> AzLayoutRightValue { az_layout_right_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutRightValue { fn drop(&mut self) { az_layout_right_value_delete(&mut self.object); } }


    /// `LayoutTopValue` struct
    pub struct LayoutTopValue { pub(crate) object: AzLayoutTopValue }

    impl LayoutTopValue {
        pub fn auto() -> Self { Self { object: az_layout_top_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_top_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_top_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_top_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutTop) -> Self { Self { object: az_layout_top_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutTopValue`
       pub fn leak(self) -> AzLayoutTopValue { az_layout_top_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutTopValue { fn drop(&mut self) { az_layout_top_value_delete(&mut self.object); } }


    /// `LayoutWidthValue` struct
    pub struct LayoutWidthValue { pub(crate) object: AzLayoutWidthValue }

    impl LayoutWidthValue {
        pub fn auto() -> Self { Self { object: az_layout_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutWidth) -> Self { Self { object: az_layout_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutWidthValue`
       pub fn leak(self) -> AzLayoutWidthValue { az_layout_width_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutWidthValue { fn drop(&mut self) { az_layout_width_value_delete(&mut self.object); } }


    /// `LayoutWrapValue` struct
    pub struct LayoutWrapValue { pub(crate) object: AzLayoutWrapValue }

    impl LayoutWrapValue {
        pub fn auto() -> Self { Self { object: az_layout_wrap_value_auto() }  }
        pub fn none() -> Self { Self { object: az_layout_wrap_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_layout_wrap_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_layout_wrap_value_initial() }  }
        pub fn exact(variant_data: crate::css::LayoutWrap) -> Self { Self { object: az_layout_wrap_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzLayoutWrapValue`
       pub fn leak(self) -> AzLayoutWrapValue { az_layout_wrap_value_deep_copy(&self.object) }
    }

    impl Drop for LayoutWrapValue { fn drop(&mut self) { az_layout_wrap_value_delete(&mut self.object); } }


    /// `OverflowValue` struct
    pub struct OverflowValue { pub(crate) object: AzOverflowValue }

    impl OverflowValue {
        pub fn auto() -> Self { Self { object: az_overflow_value_auto() }  }
        pub fn none() -> Self { Self { object: az_overflow_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_overflow_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_overflow_value_initial() }  }
        pub fn exact(variant_data: crate::css::Overflow) -> Self { Self { object: az_overflow_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzOverflowValue`
       pub fn leak(self) -> AzOverflowValue { az_overflow_value_deep_copy(&self.object) }
    }

    impl Drop for OverflowValue { fn drop(&mut self) { az_overflow_value_delete(&mut self.object); } }


    /// `StyleBackgroundContentValue` struct
    pub struct StyleBackgroundContentValue { pub(crate) object: AzStyleBackgroundContentValue }

    impl StyleBackgroundContentValue {
        pub fn auto() -> Self { Self { object: az_style_background_content_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_background_content_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_background_content_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_background_content_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBackgroundContent) -> Self { Self { object: az_style_background_content_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundContentValue`
       pub fn leak(self) -> AzStyleBackgroundContentValue { az_style_background_content_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBackgroundContentValue { fn drop(&mut self) { az_style_background_content_value_delete(&mut self.object); } }


    /// `StyleBackgroundPositionValue` struct
    pub struct StyleBackgroundPositionValue { pub(crate) object: AzStyleBackgroundPositionValue }

    impl StyleBackgroundPositionValue {
        pub fn auto() -> Self { Self { object: az_style_background_position_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_background_position_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_background_position_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_background_position_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBackgroundPosition) -> Self { Self { object: az_style_background_position_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundPositionValue`
       pub fn leak(self) -> AzStyleBackgroundPositionValue { az_style_background_position_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBackgroundPositionValue { fn drop(&mut self) { az_style_background_position_value_delete(&mut self.object); } }


    /// `StyleBackgroundRepeatValue` struct
    pub struct StyleBackgroundRepeatValue { pub(crate) object: AzStyleBackgroundRepeatValue }

    impl StyleBackgroundRepeatValue {
        pub fn auto() -> Self { Self { object: az_style_background_repeat_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_background_repeat_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_background_repeat_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_background_repeat_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBackgroundRepeat) -> Self { Self { object: az_style_background_repeat_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundRepeatValue`
       pub fn leak(self) -> AzStyleBackgroundRepeatValue { az_style_background_repeat_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBackgroundRepeatValue { fn drop(&mut self) { az_style_background_repeat_value_delete(&mut self.object); } }


    /// `StyleBackgroundSizeValue` struct
    pub struct StyleBackgroundSizeValue { pub(crate) object: AzStyleBackgroundSizeValue }

    impl StyleBackgroundSizeValue {
        pub fn auto() -> Self { Self { object: az_style_background_size_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_background_size_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_background_size_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_background_size_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBackgroundSize) -> Self { Self { object: az_style_background_size_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBackgroundSizeValue`
       pub fn leak(self) -> AzStyleBackgroundSizeValue { az_style_background_size_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBackgroundSizeValue { fn drop(&mut self) { az_style_background_size_value_delete(&mut self.object); } }


    /// `StyleBorderBottomColorValue` struct
    pub struct StyleBorderBottomColorValue { pub(crate) object: AzStyleBorderBottomColorValue }

    impl StyleBorderBottomColorValue {
        pub fn auto() -> Self { Self { object: az_style_border_bottom_color_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_bottom_color_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_bottom_color_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_bottom_color_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomColor) -> Self { Self { object: az_style_border_bottom_color_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomColorValue`
       pub fn leak(self) -> AzStyleBorderBottomColorValue { az_style_border_bottom_color_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomColorValue { fn drop(&mut self) { az_style_border_bottom_color_value_delete(&mut self.object); } }


    /// `StyleBorderBottomLeftRadiusValue` struct
    pub struct StyleBorderBottomLeftRadiusValue { pub(crate) object: AzStyleBorderBottomLeftRadiusValue }

    impl StyleBorderBottomLeftRadiusValue {
        pub fn auto() -> Self { Self { object: az_style_border_bottom_left_radius_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_bottom_left_radius_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_bottom_left_radius_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_bottom_left_radius_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomLeftRadius) -> Self { Self { object: az_style_border_bottom_left_radius_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomLeftRadiusValue`
       pub fn leak(self) -> AzStyleBorderBottomLeftRadiusValue { az_style_border_bottom_left_radius_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomLeftRadiusValue { fn drop(&mut self) { az_style_border_bottom_left_radius_value_delete(&mut self.object); } }


    /// `StyleBorderBottomRightRadiusValue` struct
    pub struct StyleBorderBottomRightRadiusValue { pub(crate) object: AzStyleBorderBottomRightRadiusValue }

    impl StyleBorderBottomRightRadiusValue {
        pub fn auto() -> Self { Self { object: az_style_border_bottom_right_radius_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_bottom_right_radius_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_bottom_right_radius_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_bottom_right_radius_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomRightRadius) -> Self { Self { object: az_style_border_bottom_right_radius_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomRightRadiusValue`
       pub fn leak(self) -> AzStyleBorderBottomRightRadiusValue { az_style_border_bottom_right_radius_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomRightRadiusValue { fn drop(&mut self) { az_style_border_bottom_right_radius_value_delete(&mut self.object); } }


    /// `StyleBorderBottomStyleValue` struct
    pub struct StyleBorderBottomStyleValue { pub(crate) object: AzStyleBorderBottomStyleValue }

    impl StyleBorderBottomStyleValue {
        pub fn auto() -> Self { Self { object: az_style_border_bottom_style_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_bottom_style_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_bottom_style_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_bottom_style_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomStyle) -> Self { Self { object: az_style_border_bottom_style_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomStyleValue`
       pub fn leak(self) -> AzStyleBorderBottomStyleValue { az_style_border_bottom_style_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomStyleValue { fn drop(&mut self) { az_style_border_bottom_style_value_delete(&mut self.object); } }


    /// `StyleBorderBottomWidthValue` struct
    pub struct StyleBorderBottomWidthValue { pub(crate) object: AzStyleBorderBottomWidthValue }

    impl StyleBorderBottomWidthValue {
        pub fn auto() -> Self { Self { object: az_style_border_bottom_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_bottom_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_bottom_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_bottom_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomWidth) -> Self { Self { object: az_style_border_bottom_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderBottomWidthValue`
       pub fn leak(self) -> AzStyleBorderBottomWidthValue { az_style_border_bottom_width_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderBottomWidthValue { fn drop(&mut self) { az_style_border_bottom_width_value_delete(&mut self.object); } }


    /// `StyleBorderLeftColorValue` struct
    pub struct StyleBorderLeftColorValue { pub(crate) object: AzStyleBorderLeftColorValue }

    impl StyleBorderLeftColorValue {
        pub fn auto() -> Self { Self { object: az_style_border_left_color_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_left_color_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_left_color_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_left_color_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderLeftColor) -> Self { Self { object: az_style_border_left_color_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderLeftColorValue`
       pub fn leak(self) -> AzStyleBorderLeftColorValue { az_style_border_left_color_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderLeftColorValue { fn drop(&mut self) { az_style_border_left_color_value_delete(&mut self.object); } }


    /// `StyleBorderLeftStyleValue` struct
    pub struct StyleBorderLeftStyleValue { pub(crate) object: AzStyleBorderLeftStyleValue }

    impl StyleBorderLeftStyleValue {
        pub fn auto() -> Self { Self { object: az_style_border_left_style_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_left_style_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_left_style_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_left_style_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderLeftStyle) -> Self { Self { object: az_style_border_left_style_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderLeftStyleValue`
       pub fn leak(self) -> AzStyleBorderLeftStyleValue { az_style_border_left_style_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderLeftStyleValue { fn drop(&mut self) { az_style_border_left_style_value_delete(&mut self.object); } }


    /// `StyleBorderLeftWidthValue` struct
    pub struct StyleBorderLeftWidthValue { pub(crate) object: AzStyleBorderLeftWidthValue }

    impl StyleBorderLeftWidthValue {
        pub fn auto() -> Self { Self { object: az_style_border_left_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_left_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_left_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_left_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderLeftWidth) -> Self { Self { object: az_style_border_left_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderLeftWidthValue`
       pub fn leak(self) -> AzStyleBorderLeftWidthValue { az_style_border_left_width_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderLeftWidthValue { fn drop(&mut self) { az_style_border_left_width_value_delete(&mut self.object); } }


    /// `StyleBorderRightColorValue` struct
    pub struct StyleBorderRightColorValue { pub(crate) object: AzStyleBorderRightColorValue }

    impl StyleBorderRightColorValue {
        pub fn auto() -> Self { Self { object: az_style_border_right_color_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_right_color_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_right_color_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_right_color_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderRightColor) -> Self { Self { object: az_style_border_right_color_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderRightColorValue`
       pub fn leak(self) -> AzStyleBorderRightColorValue { az_style_border_right_color_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderRightColorValue { fn drop(&mut self) { az_style_border_right_color_value_delete(&mut self.object); } }


    /// `StyleBorderRightStyleValue` struct
    pub struct StyleBorderRightStyleValue { pub(crate) object: AzStyleBorderRightStyleValue }

    impl StyleBorderRightStyleValue {
        pub fn auto() -> Self { Self { object: az_style_border_right_style_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_right_style_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_right_style_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_right_style_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderRightStyle) -> Self { Self { object: az_style_border_right_style_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderRightStyleValue`
       pub fn leak(self) -> AzStyleBorderRightStyleValue { az_style_border_right_style_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderRightStyleValue { fn drop(&mut self) { az_style_border_right_style_value_delete(&mut self.object); } }


    /// `StyleBorderRightWidthValue` struct
    pub struct StyleBorderRightWidthValue { pub(crate) object: AzStyleBorderRightWidthValue }

    impl StyleBorderRightWidthValue {
        pub fn auto() -> Self { Self { object: az_style_border_right_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_right_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_right_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_right_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderRightWidth) -> Self { Self { object: az_style_border_right_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderRightWidthValue`
       pub fn leak(self) -> AzStyleBorderRightWidthValue { az_style_border_right_width_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderRightWidthValue { fn drop(&mut self) { az_style_border_right_width_value_delete(&mut self.object); } }


    /// `StyleBorderTopColorValue` struct
    pub struct StyleBorderTopColorValue { pub(crate) object: AzStyleBorderTopColorValue }

    impl StyleBorderTopColorValue {
        pub fn auto() -> Self { Self { object: az_style_border_top_color_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_top_color_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_top_color_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_top_color_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderTopColor) -> Self { Self { object: az_style_border_top_color_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopColorValue`
       pub fn leak(self) -> AzStyleBorderTopColorValue { az_style_border_top_color_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopColorValue { fn drop(&mut self) { az_style_border_top_color_value_delete(&mut self.object); } }


    /// `StyleBorderTopLeftRadiusValue` struct
    pub struct StyleBorderTopLeftRadiusValue { pub(crate) object: AzStyleBorderTopLeftRadiusValue }

    impl StyleBorderTopLeftRadiusValue {
        pub fn auto() -> Self { Self { object: az_style_border_top_left_radius_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_top_left_radius_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_top_left_radius_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_top_left_radius_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderTopLeftRadius) -> Self { Self { object: az_style_border_top_left_radius_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopLeftRadiusValue`
       pub fn leak(self) -> AzStyleBorderTopLeftRadiusValue { az_style_border_top_left_radius_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopLeftRadiusValue { fn drop(&mut self) { az_style_border_top_left_radius_value_delete(&mut self.object); } }


    /// `StyleBorderTopRightRadiusValue` struct
    pub struct StyleBorderTopRightRadiusValue { pub(crate) object: AzStyleBorderTopRightRadiusValue }

    impl StyleBorderTopRightRadiusValue {
        pub fn auto() -> Self { Self { object: az_style_border_top_right_radius_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_top_right_radius_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_top_right_radius_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_top_right_radius_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderTopRightRadius) -> Self { Self { object: az_style_border_top_right_radius_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopRightRadiusValue`
       pub fn leak(self) -> AzStyleBorderTopRightRadiusValue { az_style_border_top_right_radius_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopRightRadiusValue { fn drop(&mut self) { az_style_border_top_right_radius_value_delete(&mut self.object); } }


    /// `StyleBorderTopStyleValue` struct
    pub struct StyleBorderTopStyleValue { pub(crate) object: AzStyleBorderTopStyleValue }

    impl StyleBorderTopStyleValue {
        pub fn auto() -> Self { Self { object: az_style_border_top_style_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_top_style_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_top_style_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_top_style_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderTopStyle) -> Self { Self { object: az_style_border_top_style_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopStyleValue`
       pub fn leak(self) -> AzStyleBorderTopStyleValue { az_style_border_top_style_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopStyleValue { fn drop(&mut self) { az_style_border_top_style_value_delete(&mut self.object); } }


    /// `StyleBorderTopWidthValue` struct
    pub struct StyleBorderTopWidthValue { pub(crate) object: AzStyleBorderTopWidthValue }

    impl StyleBorderTopWidthValue {
        pub fn auto() -> Self { Self { object: az_style_border_top_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_border_top_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_border_top_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_border_top_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleBorderTopWidth) -> Self { Self { object: az_style_border_top_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleBorderTopWidthValue`
       pub fn leak(self) -> AzStyleBorderTopWidthValue { az_style_border_top_width_value_deep_copy(&self.object) }
    }

    impl Drop for StyleBorderTopWidthValue { fn drop(&mut self) { az_style_border_top_width_value_delete(&mut self.object); } }


    /// `StyleCursorValue` struct
    pub struct StyleCursorValue { pub(crate) object: AzStyleCursorValue }

    impl StyleCursorValue {
        pub fn auto() -> Self { Self { object: az_style_cursor_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_cursor_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_cursor_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_cursor_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleCursor) -> Self { Self { object: az_style_cursor_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleCursorValue`
       pub fn leak(self) -> AzStyleCursorValue { az_style_cursor_value_deep_copy(&self.object) }
    }

    impl Drop for StyleCursorValue { fn drop(&mut self) { az_style_cursor_value_delete(&mut self.object); } }


    /// `StyleFontFamilyValue` struct
    pub struct StyleFontFamilyValue { pub(crate) object: AzStyleFontFamilyValue }

    impl StyleFontFamilyValue {
        pub fn auto() -> Self { Self { object: az_style_font_family_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_font_family_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_font_family_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_font_family_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleFontFamily) -> Self { Self { object: az_style_font_family_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleFontFamilyValue`
       pub fn leak(self) -> AzStyleFontFamilyValue { az_style_font_family_value_deep_copy(&self.object) }
    }

    impl Drop for StyleFontFamilyValue { fn drop(&mut self) { az_style_font_family_value_delete(&mut self.object); } }


    /// `StyleFontSizeValue` struct
    pub struct StyleFontSizeValue { pub(crate) object: AzStyleFontSizeValue }

    impl StyleFontSizeValue {
        pub fn auto() -> Self { Self { object: az_style_font_size_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_font_size_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_font_size_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_font_size_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleFontSize) -> Self { Self { object: az_style_font_size_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleFontSizeValue`
       pub fn leak(self) -> AzStyleFontSizeValue { az_style_font_size_value_deep_copy(&self.object) }
    }

    impl Drop for StyleFontSizeValue { fn drop(&mut self) { az_style_font_size_value_delete(&mut self.object); } }


    /// `StyleLetterSpacingValue` struct
    pub struct StyleLetterSpacingValue { pub(crate) object: AzStyleLetterSpacingValue }

    impl StyleLetterSpacingValue {
        pub fn auto() -> Self { Self { object: az_style_letter_spacing_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_letter_spacing_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_letter_spacing_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_letter_spacing_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleLetterSpacing) -> Self { Self { object: az_style_letter_spacing_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleLetterSpacingValue`
       pub fn leak(self) -> AzStyleLetterSpacingValue { az_style_letter_spacing_value_deep_copy(&self.object) }
    }

    impl Drop for StyleLetterSpacingValue { fn drop(&mut self) { az_style_letter_spacing_value_delete(&mut self.object); } }


    /// `StyleLineHeightValue` struct
    pub struct StyleLineHeightValue { pub(crate) object: AzStyleLineHeightValue }

    impl StyleLineHeightValue {
        pub fn auto() -> Self { Self { object: az_style_line_height_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_line_height_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_line_height_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_line_height_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleLineHeight) -> Self { Self { object: az_style_line_height_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleLineHeightValue`
       pub fn leak(self) -> AzStyleLineHeightValue { az_style_line_height_value_deep_copy(&self.object) }
    }

    impl Drop for StyleLineHeightValue { fn drop(&mut self) { az_style_line_height_value_delete(&mut self.object); } }


    /// `StyleTabWidthValue` struct
    pub struct StyleTabWidthValue { pub(crate) object: AzStyleTabWidthValue }

    impl StyleTabWidthValue {
        pub fn auto() -> Self { Self { object: az_style_tab_width_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_tab_width_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_tab_width_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_tab_width_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleTabWidth) -> Self { Self { object: az_style_tab_width_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleTabWidthValue`
       pub fn leak(self) -> AzStyleTabWidthValue { az_style_tab_width_value_deep_copy(&self.object) }
    }

    impl Drop for StyleTabWidthValue { fn drop(&mut self) { az_style_tab_width_value_delete(&mut self.object); } }


    /// `StyleTextAlignmentHorzValue` struct
    pub struct StyleTextAlignmentHorzValue { pub(crate) object: AzStyleTextAlignmentHorzValue }

    impl StyleTextAlignmentHorzValue {
        pub fn auto() -> Self { Self { object: az_style_text_alignment_horz_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_text_alignment_horz_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_text_alignment_horz_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_text_alignment_horz_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleTextAlignmentHorz) -> Self { Self { object: az_style_text_alignment_horz_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleTextAlignmentHorzValue`
       pub fn leak(self) -> AzStyleTextAlignmentHorzValue { az_style_text_alignment_horz_value_deep_copy(&self.object) }
    }

    impl Drop for StyleTextAlignmentHorzValue { fn drop(&mut self) { az_style_text_alignment_horz_value_delete(&mut self.object); } }


    /// `StyleTextColorValue` struct
    pub struct StyleTextColorValue { pub(crate) object: AzStyleTextColorValue }

    impl StyleTextColorValue {
        pub fn auto() -> Self { Self { object: az_style_text_color_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_text_color_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_text_color_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_text_color_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleTextColor) -> Self { Self { object: az_style_text_color_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleTextColorValue`
       pub fn leak(self) -> AzStyleTextColorValue { az_style_text_color_value_deep_copy(&self.object) }
    }

    impl Drop for StyleTextColorValue { fn drop(&mut self) { az_style_text_color_value_delete(&mut self.object); } }


    /// `StyleWordSpacingValue` struct
    pub struct StyleWordSpacingValue { pub(crate) object: AzStyleWordSpacingValue }

    impl StyleWordSpacingValue {
        pub fn auto() -> Self { Self { object: az_style_word_spacing_value_auto() }  }
        pub fn none() -> Self { Self { object: az_style_word_spacing_value_none() }  }
        pub fn inherit() -> Self { Self { object: az_style_word_spacing_value_inherit() }  }
        pub fn initial() -> Self { Self { object: az_style_word_spacing_value_initial() }  }
        pub fn exact(variant_data: crate::css::StyleWordSpacing) -> Self { Self { object: az_style_word_spacing_value_exact(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzStyleWordSpacingValue`
       pub fn leak(self) -> AzStyleWordSpacingValue { az_style_word_spacing_value_deep_copy(&self.object) }
    }

    impl Drop for StyleWordSpacingValue { fn drop(&mut self) { az_style_word_spacing_value_delete(&mut self.object); } }


    /// Parsed CSS key-value pair
    pub struct CssProperty { pub(crate) object: AzCssProperty }

    impl CssProperty {
        pub fn text_color(variant_data: crate::css::StyleTextColorValue) -> Self { Self { object: az_css_property_text_color(variant_data.leak()) }}
        pub fn font_size(variant_data: crate::css::StyleFontSizeValue) -> Self { Self { object: az_css_property_font_size(variant_data.leak()) }}
        pub fn font_family(variant_data: crate::css::StyleFontFamilyValue) -> Self { Self { object: az_css_property_font_family(variant_data.leak()) }}
        pub fn text_align(variant_data: crate::css::StyleTextAlignmentHorzValue) -> Self { Self { object: az_css_property_text_align(variant_data.leak()) }}
        pub fn letter_spacing(variant_data: crate::css::StyleLetterSpacingValue) -> Self { Self { object: az_css_property_letter_spacing(variant_data.leak()) }}
        pub fn line_height(variant_data: crate::css::StyleLineHeightValue) -> Self { Self { object: az_css_property_line_height(variant_data.leak()) }}
        pub fn word_spacing(variant_data: crate::css::StyleWordSpacingValue) -> Self { Self { object: az_css_property_word_spacing(variant_data.leak()) }}
        pub fn tab_width(variant_data: crate::css::StyleTabWidthValue) -> Self { Self { object: az_css_property_tab_width(variant_data.leak()) }}
        pub fn cursor(variant_data: crate::css::StyleCursorValue) -> Self { Self { object: az_css_property_cursor(variant_data.leak()) }}
        pub fn display(variant_data: crate::css::LayoutDisplayValue) -> Self { Self { object: az_css_property_display(variant_data.leak()) }}
        pub fn float(variant_data: crate::css::LayoutFloatValue) -> Self { Self { object: az_css_property_float(variant_data.leak()) }}
        pub fn box_sizing(variant_data: crate::css::LayoutBoxSizingValue) -> Self { Self { object: az_css_property_box_sizing(variant_data.leak()) }}
        pub fn width(variant_data: crate::css::LayoutWidthValue) -> Self { Self { object: az_css_property_width(variant_data.leak()) }}
        pub fn height(variant_data: crate::css::LayoutHeightValue) -> Self { Self { object: az_css_property_height(variant_data.leak()) }}
        pub fn min_width(variant_data: crate::css::LayoutMinWidthValue) -> Self { Self { object: az_css_property_min_width(variant_data.leak()) }}
        pub fn min_height(variant_data: crate::css::LayoutMinHeightValue) -> Self { Self { object: az_css_property_min_height(variant_data.leak()) }}
        pub fn max_width(variant_data: crate::css::LayoutMaxWidthValue) -> Self { Self { object: az_css_property_max_width(variant_data.leak()) }}
        pub fn max_height(variant_data: crate::css::LayoutMaxHeightValue) -> Self { Self { object: az_css_property_max_height(variant_data.leak()) }}
        pub fn position(variant_data: crate::css::LayoutPositionValue) -> Self { Self { object: az_css_property_position(variant_data.leak()) }}
        pub fn top(variant_data: crate::css::LayoutTopValue) -> Self { Self { object: az_css_property_top(variant_data.leak()) }}
        pub fn right(variant_data: crate::css::LayoutRightValue) -> Self { Self { object: az_css_property_right(variant_data.leak()) }}
        pub fn left(variant_data: crate::css::LayoutLeftValue) -> Self { Self { object: az_css_property_left(variant_data.leak()) }}
        pub fn bottom(variant_data: crate::css::LayoutBottomValue) -> Self { Self { object: az_css_property_bottom(variant_data.leak()) }}
        pub fn flex_wrap(variant_data: crate::css::LayoutWrapValue) -> Self { Self { object: az_css_property_flex_wrap(variant_data.leak()) }}
        pub fn flex_direction(variant_data: crate::css::LayoutDirectionValue) -> Self { Self { object: az_css_property_flex_direction(variant_data.leak()) }}
        pub fn flex_grow(variant_data: crate::css::LayoutFlexGrowValue) -> Self { Self { object: az_css_property_flex_grow(variant_data.leak()) }}
        pub fn flex_shrink(variant_data: crate::css::LayoutFlexShrinkValue) -> Self { Self { object: az_css_property_flex_shrink(variant_data.leak()) }}
        pub fn justify_content(variant_data: crate::css::LayoutJustifyContentValue) -> Self { Self { object: az_css_property_justify_content(variant_data.leak()) }}
        pub fn align_items(variant_data: crate::css::LayoutAlignItemsValue) -> Self { Self { object: az_css_property_align_items(variant_data.leak()) }}
        pub fn align_content(variant_data: crate::css::LayoutAlignContentValue) -> Self { Self { object: az_css_property_align_content(variant_data.leak()) }}
        pub fn background_content(variant_data: crate::css::StyleBackgroundContentValue) -> Self { Self { object: az_css_property_background_content(variant_data.leak()) }}
        pub fn background_position(variant_data: crate::css::StyleBackgroundPositionValue) -> Self { Self { object: az_css_property_background_position(variant_data.leak()) }}
        pub fn background_size(variant_data: crate::css::StyleBackgroundSizeValue) -> Self { Self { object: az_css_property_background_size(variant_data.leak()) }}
        pub fn background_repeat(variant_data: crate::css::StyleBackgroundRepeatValue) -> Self { Self { object: az_css_property_background_repeat(variant_data.leak()) }}
        pub fn overflow_x(variant_data: crate::css::OverflowValue) -> Self { Self { object: az_css_property_overflow_x(variant_data.leak()) }}
        pub fn overflow_y(variant_data: crate::css::OverflowValue) -> Self { Self { object: az_css_property_overflow_y(variant_data.leak()) }}
        pub fn padding_top(variant_data: crate::css::LayoutPaddingTopValue) -> Self { Self { object: az_css_property_padding_top(variant_data.leak()) }}
        pub fn padding_left(variant_data: crate::css::LayoutPaddingLeftValue) -> Self { Self { object: az_css_property_padding_left(variant_data.leak()) }}
        pub fn padding_right(variant_data: crate::css::LayoutPaddingRightValue) -> Self { Self { object: az_css_property_padding_right(variant_data.leak()) }}
        pub fn padding_bottom(variant_data: crate::css::LayoutPaddingBottomValue) -> Self { Self { object: az_css_property_padding_bottom(variant_data.leak()) }}
        pub fn margin_top(variant_data: crate::css::LayoutMarginTopValue) -> Self { Self { object: az_css_property_margin_top(variant_data.leak()) }}
        pub fn margin_left(variant_data: crate::css::LayoutMarginLeftValue) -> Self { Self { object: az_css_property_margin_left(variant_data.leak()) }}
        pub fn margin_right(variant_data: crate::css::LayoutMarginRightValue) -> Self { Self { object: az_css_property_margin_right(variant_data.leak()) }}
        pub fn margin_bottom(variant_data: crate::css::LayoutMarginBottomValue) -> Self { Self { object: az_css_property_margin_bottom(variant_data.leak()) }}
        pub fn border_top_left_radius(variant_data: crate::css::StyleBorderTopLeftRadiusValue) -> Self { Self { object: az_css_property_border_top_left_radius(variant_data.leak()) }}
        pub fn border_top_right_radius(variant_data: crate::css::StyleBorderTopRightRadiusValue) -> Self { Self { object: az_css_property_border_top_right_radius(variant_data.leak()) }}
        pub fn border_bottom_left_radius(variant_data: crate::css::StyleBorderBottomLeftRadiusValue) -> Self { Self { object: az_css_property_border_bottom_left_radius(variant_data.leak()) }}
        pub fn border_bottom_right_radius(variant_data: crate::css::StyleBorderBottomRightRadiusValue) -> Self { Self { object: az_css_property_border_bottom_right_radius(variant_data.leak()) }}
        pub fn border_top_color(variant_data: crate::css::StyleBorderTopColorValue) -> Self { Self { object: az_css_property_border_top_color(variant_data.leak()) }}
        pub fn border_right_color(variant_data: crate::css::StyleBorderRightColorValue) -> Self { Self { object: az_css_property_border_right_color(variant_data.leak()) }}
        pub fn border_left_color(variant_data: crate::css::StyleBorderLeftColorValue) -> Self { Self { object: az_css_property_border_left_color(variant_data.leak()) }}
        pub fn border_bottom_color(variant_data: crate::css::StyleBorderBottomColorValue) -> Self { Self { object: az_css_property_border_bottom_color(variant_data.leak()) }}
        pub fn border_top_style(variant_data: crate::css::StyleBorderTopStyleValue) -> Self { Self { object: az_css_property_border_top_style(variant_data.leak()) }}
        pub fn border_right_style(variant_data: crate::css::StyleBorderRightStyleValue) -> Self { Self { object: az_css_property_border_right_style(variant_data.leak()) }}
        pub fn border_left_style(variant_data: crate::css::StyleBorderLeftStyleValue) -> Self { Self { object: az_css_property_border_left_style(variant_data.leak()) }}
        pub fn border_bottom_style(variant_data: crate::css::StyleBorderBottomStyleValue) -> Self { Self { object: az_css_property_border_bottom_style(variant_data.leak()) }}
        pub fn border_top_width(variant_data: crate::css::StyleBorderTopWidthValue) -> Self { Self { object: az_css_property_border_top_width(variant_data.leak()) }}
        pub fn border_right_width(variant_data: crate::css::StyleBorderRightWidthValue) -> Self { Self { object: az_css_property_border_right_width(variant_data.leak()) }}
        pub fn border_left_width(variant_data: crate::css::StyleBorderLeftWidthValue) -> Self { Self { object: az_css_property_border_left_width(variant_data.leak()) }}
        pub fn border_bottom_width(variant_data: crate::css::StyleBorderBottomWidthValue) -> Self { Self { object: az_css_property_border_bottom_width(variant_data.leak()) }}
        pub fn box_shadow_left(variant_data: crate::css::BoxShadowPreDisplayItemValue) -> Self { Self { object: az_css_property_box_shadow_left(variant_data.leak()) }}
        pub fn box_shadow_right(variant_data: crate::css::BoxShadowPreDisplayItemValue) -> Self { Self { object: az_css_property_box_shadow_right(variant_data.leak()) }}
        pub fn box_shadow_top(variant_data: crate::css::BoxShadowPreDisplayItemValue) -> Self { Self { object: az_css_property_box_shadow_top(variant_data.leak()) }}
        pub fn box_shadow_bottom(variant_data: crate::css::BoxShadowPreDisplayItemValue) -> Self { Self { object: az_css_property_box_shadow_bottom(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzCssProperty`
       pub fn leak(self) -> AzCssProperty { az_css_property_deep_copy(&self.object) }
    }

    impl Drop for CssProperty { fn drop(&mut self) { az_css_property_delete(&mut self.object); } }
}

/// `Dom` construction and configuration
#[allow(dead_code, unused_imports)]
pub mod dom {

    use azul_dll::*;
    use crate::str::String;
    use crate::resources::{ImageId, TextId};
    use crate::callbacks::{GlCallback, Callback, IFrameCallback, RefAny};
    use crate::vec::StringVec;
    use crate::css::CssProperty;


    /// `Dom` struct
    pub struct Dom { pub(crate) ptr: AzDomPtr }

    impl Dom {
        /// Creates a new `div` node
        pub fn div() -> Self { Self { ptr: az_dom_div() } }
        /// Creates a new `body` node
        pub fn body() -> Self { Self { ptr: az_dom_body() } }
        /// Creates a new `p` node with a given `String` as the text contents
        pub fn label(text: String) -> Self { Self { ptr: az_dom_label(text.leak()) } }
        /// Creates a new `p` node from a (cached) text referenced by a `TextId`
        pub fn text(text_id: TextId) -> Self { Self { ptr: az_dom_text(text_id.leak()) } }
        /// Creates a new `img` node from a (cached) text referenced by a `ImageId`
        pub fn image(image_id: ImageId) -> Self { Self { ptr: az_dom_image(image_id.leak()) } }
        /// Creates a new node which will render an OpenGL texture after the layout step is finished. See the documentation for [GlCallback]() for more info about OpenGL rendering callbacks.
        pub fn gl_texture(data: RefAny, callback: GlCallback) -> Self { Self { ptr: az_dom_gl_texture(data.leak(), callback) } }
        /// Creates a new node with a callback that will return a `Dom` after being layouted. See the documentation for [IFrameCallback]() for more info about iframe callbacks.
        pub fn iframe_callback(data: RefAny, callback: IFrameCallback) -> Self { Self { ptr: az_dom_iframe_callback(data.leak(), callback) } }
        /// Adds a CSS ID (`#something`) to the DOM node
        pub fn add_id(&mut self, id: String)  { az_dom_add_id(&mut self.ptr, id.leak()) }
        /// Same as [`Dom::add_id`](#method.add_id), but as a builder method
        pub fn with_id(self, id: String)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_id(self.leak(), id.leak()) } } }
        /// Same as calling [`Dom::add_id`](#method.add_id) for each CSS ID, but this function **replaces** all current CSS IDs
        pub fn set_ids(&mut self, ids: StringVec)  { az_dom_set_ids(&mut self.ptr, ids.leak()) }
        /// Same as [`Dom::set_ids`](#method.set_ids), but as a builder method
        pub fn with_ids(self, ids: StringVec)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_ids(self.leak(), ids.leak()) } } }
        /// Adds a CSS class (`.something`) to the DOM node
        pub fn add_class(&mut self, class: String)  { az_dom_add_class(&mut self.ptr, class.leak()) }
        /// Same as [`Dom::add_class`](#method.add_class), but as a builder method
        pub fn with_class(self, class: String)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_class(self.leak(), class.leak()) } } }
        /// Same as calling [`Dom::add_class`](#method.add_class) for each class, but this function **replaces** all current classes
        pub fn set_classes(&mut self, classes: StringVec)  { az_dom_set_classes(&mut self.ptr, classes.leak()) }
        /// Same as [`Dom::set_classes`](#method.set_classes), but as a builder method
        pub fn with_classes(self, classes: StringVec)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_classes(self.leak(), classes.leak()) } } }
        /// Adds a [`Callback`](callbacks/type.Callback) that acts on the `data` the `event` happens
        pub fn add_callback(&mut self, event: EventFilter, data: RefAny, callback: Callback)  { az_dom_add_callback(&mut self.ptr, event.leak(), data.leak(), callback) }
        /// Same as [`Dom::add_callback`](#method.add_callback), but as a builder method
        pub fn with_callback(self, event: EventFilter, data: RefAny, callback: Callback)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_callback(self.leak(), event.leak(), data.leak(), callback) } } }
        /// Overrides the CSS property of this DOM node with a value (for example `"width = 200px"`)
        pub fn add_css_override(&mut self, id: String, prop: CssProperty)  { az_dom_add_css_override(&mut self.ptr, id.leak(), prop.leak()) }
        /// Same as [`Dom::add_css_override`](#method.add_css_override), but as a builder method
        pub fn with_css_override(self, id: String, prop: CssProperty)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_css_override(self.leak(), id.leak(), prop.leak()) } } }
        /// Sets the `is_draggable` attribute of this DOM node (default: false)
        pub fn set_is_draggable(&mut self, is_draggable: bool)  { az_dom_set_is_draggable(&mut self.ptr, is_draggable) }
        /// Same as [`Dom::set_is_draggable`](#method.set_is_draggable), but as a builder method
        pub fn is_draggable(self, is_draggable: bool)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_is_draggable(self.leak(), is_draggable) } } }
        /// Sets the `tabindex` attribute of this DOM node (makes an element focusable - default: None)
        pub fn set_tab_index(&mut self, tab_index: TabIndex)  { az_dom_set_tab_index(&mut self.ptr, tab_index.leak()) }
        /// Same as [`Dom::set_tab_index`](#method.set_tab_index), but as a builder method
        pub fn with_tab_index(self, tab_index: TabIndex)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_tab_index(self.leak(), tab_index.leak()) } } }
        /// Reparents another `Dom` to be the child node of this `Dom`
        pub fn add_child(&mut self, child: Dom)  { az_dom_add_child(&mut self.ptr, child.leak()) }
        /// Same as [`Dom::add_child`](#method.add_child), but as a builder method
        pub fn with_child(self, child: Dom)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_child(self.leak(), child.leak()) } } }
        /// Returns if the DOM node has a certain CSS ID
        pub fn has_id(&mut self, id: String)  -> bool { az_dom_has_id(&mut self.ptr, id.leak()) }
        /// Returns if the DOM node has a certain CSS class
        pub fn has_class(&mut self, class: String)  -> bool { az_dom_has_class(&mut self.ptr, class.leak()) }
        /// Returns the HTML String for this DOM
        pub fn get_html_string(&mut self)  -> crate::str::String { crate::str::String { object: { az_dom_get_html_string(&mut self.ptr)} } }
       /// Prevents the destructor from running and returns the internal `AzDomPtr`
       pub fn leak(self) -> AzDomPtr { let p = az_dom_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for Dom { fn drop(&mut self) { az_dom_delete(&mut self.ptr); } }


    /// `EventFilter` struct
    pub struct EventFilter { pub(crate) object: AzEventFilter }

    impl EventFilter {
        pub fn hover(variant_data: crate::dom::HoverEventFilter) -> Self { Self { object: az_event_filter_hover(variant_data.leak()) }}
        pub fn not(variant_data: crate::dom::NotEventFilter) -> Self { Self { object: az_event_filter_not(variant_data.leak()) }}
        pub fn focus(variant_data: crate::dom::FocusEventFilter) -> Self { Self { object: az_event_filter_focus(variant_data.leak()) }}
        pub fn window(variant_data: crate::dom::WindowEventFilter) -> Self { Self { object: az_event_filter_window(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzEventFilter`
       pub fn leak(self) -> AzEventFilter { az_event_filter_deep_copy(&self.object) }
    }

    impl Drop for EventFilter { fn drop(&mut self) { az_event_filter_delete(&mut self.object); } }


    /// `HoverEventFilter` struct
    pub struct HoverEventFilter { pub(crate) object: AzHoverEventFilter }

    impl HoverEventFilter {
        pub fn mouse_over() -> Self { Self { object: az_hover_event_filter_mouse_over() }  }
        pub fn mouse_down() -> Self { Self { object: az_hover_event_filter_mouse_down() }  }
        pub fn left_mouse_down() -> Self { Self { object: az_hover_event_filter_left_mouse_down() }  }
        pub fn right_mouse_down() -> Self { Self { object: az_hover_event_filter_right_mouse_down() }  }
        pub fn middle_mouse_down() -> Self { Self { object: az_hover_event_filter_middle_mouse_down() }  }
        pub fn mouse_up() -> Self { Self { object: az_hover_event_filter_mouse_up() }  }
        pub fn left_mouse_up() -> Self { Self { object: az_hover_event_filter_left_mouse_up() }  }
        pub fn right_mouse_up() -> Self { Self { object: az_hover_event_filter_right_mouse_up() }  }
        pub fn middle_mouse_up() -> Self { Self { object: az_hover_event_filter_middle_mouse_up() }  }
        pub fn mouse_enter() -> Self { Self { object: az_hover_event_filter_mouse_enter() }  }
        pub fn mouse_leave() -> Self { Self { object: az_hover_event_filter_mouse_leave() }  }
        pub fn scroll() -> Self { Self { object: az_hover_event_filter_scroll() }  }
        pub fn scroll_start() -> Self { Self { object: az_hover_event_filter_scroll_start() }  }
        pub fn scroll_end() -> Self { Self { object: az_hover_event_filter_scroll_end() }  }
        pub fn text_input() -> Self { Self { object: az_hover_event_filter_text_input() }  }
        pub fn virtual_key_down() -> Self { Self { object: az_hover_event_filter_virtual_key_down() }  }
        pub fn virtual_key_up() -> Self { Self { object: az_hover_event_filter_virtual_key_up() }  }
        pub fn hovered_file() -> Self { Self { object: az_hover_event_filter_hovered_file() }  }
        pub fn dropped_file() -> Self { Self { object: az_hover_event_filter_dropped_file() }  }
        pub fn hovered_file_cancelled() -> Self { Self { object: az_hover_event_filter_hovered_file_cancelled() }  }
       /// Prevents the destructor from running and returns the internal `AzHoverEventFilter`
       pub fn leak(self) -> AzHoverEventFilter { az_hover_event_filter_deep_copy(&self.object) }
    }

    impl Drop for HoverEventFilter { fn drop(&mut self) { az_hover_event_filter_delete(&mut self.object); } }


    /// `FocusEventFilter` struct
    pub struct FocusEventFilter { pub(crate) object: AzFocusEventFilter }

    impl FocusEventFilter {
        pub fn mouse_over() -> Self { Self { object: az_focus_event_filter_mouse_over() }  }
        pub fn mouse_down() -> Self { Self { object: az_focus_event_filter_mouse_down() }  }
        pub fn left_mouse_down() -> Self { Self { object: az_focus_event_filter_left_mouse_down() }  }
        pub fn right_mouse_down() -> Self { Self { object: az_focus_event_filter_right_mouse_down() }  }
        pub fn middle_mouse_down() -> Self { Self { object: az_focus_event_filter_middle_mouse_down() }  }
        pub fn mouse_up() -> Self { Self { object: az_focus_event_filter_mouse_up() }  }
        pub fn left_mouse_up() -> Self { Self { object: az_focus_event_filter_left_mouse_up() }  }
        pub fn right_mouse_up() -> Self { Self { object: az_focus_event_filter_right_mouse_up() }  }
        pub fn middle_mouse_up() -> Self { Self { object: az_focus_event_filter_middle_mouse_up() }  }
        pub fn mouse_enter() -> Self { Self { object: az_focus_event_filter_mouse_enter() }  }
        pub fn mouse_leave() -> Self { Self { object: az_focus_event_filter_mouse_leave() }  }
        pub fn scroll() -> Self { Self { object: az_focus_event_filter_scroll() }  }
        pub fn scroll_start() -> Self { Self { object: az_focus_event_filter_scroll_start() }  }
        pub fn scroll_end() -> Self { Self { object: az_focus_event_filter_scroll_end() }  }
        pub fn text_input() -> Self { Self { object: az_focus_event_filter_text_input() }  }
        pub fn virtual_key_down() -> Self { Self { object: az_focus_event_filter_virtual_key_down() }  }
        pub fn virtual_key_up() -> Self { Self { object: az_focus_event_filter_virtual_key_up() }  }
        pub fn focus_received() -> Self { Self { object: az_focus_event_filter_focus_received() }  }
        pub fn focus_lost() -> Self { Self { object: az_focus_event_filter_focus_lost() }  }
       /// Prevents the destructor from running and returns the internal `AzFocusEventFilter`
       pub fn leak(self) -> AzFocusEventFilter { az_focus_event_filter_deep_copy(&self.object) }
    }

    impl Drop for FocusEventFilter { fn drop(&mut self) { az_focus_event_filter_delete(&mut self.object); } }


    /// `NotEventFilter` struct
    pub struct NotEventFilter { pub(crate) object: AzNotEventFilter }

    impl NotEventFilter {
        pub fn hover(variant_data: crate::dom::HoverEventFilter) -> Self { Self { object: az_not_event_filter_hover(variant_data.leak()) }}
        pub fn focus(variant_data: crate::dom::FocusEventFilter) -> Self { Self { object: az_not_event_filter_focus(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzNotEventFilter`
       pub fn leak(self) -> AzNotEventFilter { az_not_event_filter_deep_copy(&self.object) }
    }

    impl Drop for NotEventFilter { fn drop(&mut self) { az_not_event_filter_delete(&mut self.object); } }


    /// `WindowEventFilter` struct
    pub struct WindowEventFilter { pub(crate) object: AzWindowEventFilter }

    impl WindowEventFilter {
        pub fn mouse_over() -> Self { Self { object: az_window_event_filter_mouse_over() }  }
        pub fn mouse_down() -> Self { Self { object: az_window_event_filter_mouse_down() }  }
        pub fn left_mouse_down() -> Self { Self { object: az_window_event_filter_left_mouse_down() }  }
        pub fn right_mouse_down() -> Self { Self { object: az_window_event_filter_right_mouse_down() }  }
        pub fn middle_mouse_down() -> Self { Self { object: az_window_event_filter_middle_mouse_down() }  }
        pub fn mouse_up() -> Self { Self { object: az_window_event_filter_mouse_up() }  }
        pub fn left_mouse_up() -> Self { Self { object: az_window_event_filter_left_mouse_up() }  }
        pub fn right_mouse_up() -> Self { Self { object: az_window_event_filter_right_mouse_up() }  }
        pub fn middle_mouse_up() -> Self { Self { object: az_window_event_filter_middle_mouse_up() }  }
        pub fn mouse_enter() -> Self { Self { object: az_window_event_filter_mouse_enter() }  }
        pub fn mouse_leave() -> Self { Self { object: az_window_event_filter_mouse_leave() }  }
        pub fn scroll() -> Self { Self { object: az_window_event_filter_scroll() }  }
        pub fn scroll_start() -> Self { Self { object: az_window_event_filter_scroll_start() }  }
        pub fn scroll_end() -> Self { Self { object: az_window_event_filter_scroll_end() }  }
        pub fn text_input() -> Self { Self { object: az_window_event_filter_text_input() }  }
        pub fn virtual_key_down() -> Self { Self { object: az_window_event_filter_virtual_key_down() }  }
        pub fn virtual_key_up() -> Self { Self { object: az_window_event_filter_virtual_key_up() }  }
        pub fn hovered_file() -> Self { Self { object: az_window_event_filter_hovered_file() }  }
        pub fn dropped_file() -> Self { Self { object: az_window_event_filter_dropped_file() }  }
        pub fn hovered_file_cancelled() -> Self { Self { object: az_window_event_filter_hovered_file_cancelled() }  }
       /// Prevents the destructor from running and returns the internal `AzWindowEventFilter`
       pub fn leak(self) -> AzWindowEventFilter { az_window_event_filter_deep_copy(&self.object) }
    }

    impl Drop for WindowEventFilter { fn drop(&mut self) { az_window_event_filter_delete(&mut self.object); } }


    /// `TabIndex` struct
    pub struct TabIndex { pub(crate) object: AzTabIndex }

    impl TabIndex {
        /// Automatic tab index, similar to simply setting `focusable = "true"` or `tabindex = 0`, (both have the effect of making the element focusable)
        pub fn auto() -> Self { Self { object: az_tab_index_auto() }  }
        ///  Set the tab index in relation to its parent element (`tabindex = n`)
        pub fn override_in_parent(variant_data: usize) -> Self { Self { object: az_tab_index_override_in_parent(variant_data) }}
        /// Elements can be focused in callbacks, but are not accessible via keyboard / tab navigation (`tabindex = -1` ) 
        pub fn no_keyboard_focus() -> Self { Self { object: az_tab_index_no_keyboard_focus() }  }
       /// Prevents the destructor from running and returns the internal `AzTabIndex`
       pub fn leak(self) -> AzTabIndex { az_tab_index_deep_copy(&self.object) }
    }

    impl Drop for TabIndex { fn drop(&mut self) { az_tab_index_delete(&mut self.object); } }
}

/// Struct definition for image / font / text IDs
#[allow(dead_code, unused_imports)]
pub mod resources {

    use azul_dll::*;
    use crate::vec::U8Vec;


    /// `TextId` struct
    pub struct TextId { pub(crate) object: AzTextId }

    impl TextId {
        /// Creates a new, unique `TextId`
        pub fn new() -> Self { Self { object: az_text_id_new() } }
       /// Prevents the destructor from running and returns the internal `AzTextId`
       pub fn leak(self) -> AzTextId { az_text_id_deep_copy(&self.object) }
    }

    impl Drop for TextId { fn drop(&mut self) { az_text_id_delete(&mut self.object); } }


    /// `ImageId` struct
    pub struct ImageId { pub(crate) object: AzImageId }

    impl ImageId {
        /// Creates a new, unique `ImageId`
        pub fn new() -> Self { Self { object: az_image_id_new() } }
       /// Prevents the destructor from running and returns the internal `AzImageId`
       pub fn leak(self) -> AzImageId { az_image_id_deep_copy(&self.object) }
    }

    impl Drop for ImageId { fn drop(&mut self) { az_image_id_delete(&mut self.object); } }


    /// `FontId` struct
    pub struct FontId { pub(crate) object: AzFontId }

    impl FontId {
        /// Creates a new, unique `FontId`
        pub fn new() -> Self { Self { object: az_font_id_new() } }
       /// Prevents the destructor from running and returns the internal `AzFontId`
       pub fn leak(self) -> AzFontId { az_font_id_deep_copy(&self.object) }
    }

    impl Drop for FontId { fn drop(&mut self) { az_font_id_delete(&mut self.object); } }


    /// `ImageSource` struct
    pub struct ImageSource { pub(crate) object: AzImageSource }

    impl ImageSource {
        /// Bytes of the image, encoded in PNG / JPG / etc. format
        pub fn embedded(variant_data: crate::vec::U8Vec) -> Self { Self { object: az_image_source_embedded(variant_data.leak()) }}
        /// References an (encoded!) image as a file from the file system that is loaded when necessary
        pub fn file(variant_data: crate::path::PathBuf) -> Self { Self { object: az_image_source_file(variant_data.leak()) }}
        /// References a decoded (!) `RawImage` as the image source
        pub fn raw(variant_data: crate::resources::RawImage) -> Self { Self { object: az_image_source_raw(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzImageSource`
       pub fn leak(self) -> AzImageSource { az_image_source_deep_copy(&self.object) }
    }

    impl Drop for ImageSource { fn drop(&mut self) { az_image_source_delete(&mut self.object); } }


    /// `FontSource` struct
    pub struct FontSource { pub(crate) object: AzFontSource }

    impl FontSource {
        /// Bytes are the bytes of the font file
        pub fn embedded(variant_data: crate::vec::U8Vec) -> Self { Self { object: az_font_source_embedded(variant_data.leak()) }}
        /// References a font from a file path, which is loaded when necessary
        pub fn file(variant_data: crate::path::PathBuf) -> Self { Self { object: az_font_source_file(variant_data.leak()) }}
        /// References a font from from a system font identifier, such as `"Arial"` or `"Helvetica"`
        pub fn system(variant_data: crate::str::String) -> Self { Self { object: az_font_source_system(variant_data.leak()) }}
       /// Prevents the destructor from running and returns the internal `AzFontSource`
       pub fn leak(self) -> AzFontSource { az_font_source_deep_copy(&self.object) }
    }

    impl Drop for FontSource { fn drop(&mut self) { az_font_source_delete(&mut self.object); } }


    /// `RawImage` struct
    pub struct RawImage { pub(crate) ptr: AzRawImagePtr }

    impl RawImage {
        /// Creates a new `RawImage` by loading the decoded bytes
        pub fn new(decoded_pixels: U8Vec, width: usize, height: usize, data_format: RawImageFormat) -> Self { Self { ptr: az_raw_image_new(decoded_pixels.leak(), width, height, data_format.leak()) } }
       /// Prevents the destructor from running and returns the internal `AzRawImagePtr`
       pub fn leak(self) -> AzRawImagePtr { let p = az_raw_image_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for RawImage { fn drop(&mut self) { az_raw_image_delete(&mut self.ptr); } }


    /// `RawImageFormat` struct
    pub struct RawImageFormat { pub(crate) object: AzRawImageFormat }

    impl RawImageFormat {
        /// Bytes are in the R-unsinged-8bit format
        pub fn r8() -> Self { Self { object: az_raw_image_format_r8() }  }
        /// Bytes are in the R-unsinged-16bit format
        pub fn r16() -> Self { Self { object: az_raw_image_format_r16() }  }
        /// Bytes are in the RG-unsinged-16bit format
        pub fn rg16() -> Self { Self { object: az_raw_image_format_rg16() }  }
        /// Bytes are in the BRGA-unsigned-8bit format
        pub fn bgra8() -> Self { Self { object: az_raw_image_format_bgra8() }  }
        /// Bytes are in the RGBA-floating-point-32bit format
        pub fn rgbaf32() -> Self { Self { object: az_raw_image_format_rgbaf32() }  }
        /// Bytes are in the RG-unsigned-8bit format
        pub fn rg8() -> Self { Self { object: az_raw_image_format_rg8() }  }
        /// Bytes are in the RGBA-signed-32bit format
        pub fn rgbai32() -> Self { Self { object: az_raw_image_format_rgbai32() }  }
        /// Bytes are in the RGBA-unsigned-8bit format
        pub fn rgba8() -> Self { Self { object: az_raw_image_format_rgba8() }  }
       /// Prevents the destructor from running and returns the internal `AzRawImageFormat`
       pub fn leak(self) -> AzRawImageFormat { az_raw_image_format_deep_copy(&self.object) }
    }

    impl Drop for RawImageFormat { fn drop(&mut self) { az_raw_image_format_delete(&mut self.object); } }
}

/// Window creation / startup configuration
#[allow(dead_code, unused_imports)]
pub mod window {

    use azul_dll::*;
    use crate::css::Css;


    /// `WindowCreateOptions` struct
    pub struct WindowCreateOptions { pub(crate) ptr: AzWindowCreateOptionsPtr }

    impl WindowCreateOptions {
        /// Creates a new `WindowCreateOptions` instance.
        pub fn new(css: Css) -> Self { Self { ptr: az_window_create_options_new(css.leak()) } }
       /// Prevents the destructor from running and returns the internal `AzWindowCreateOptionsPtr`
       pub fn leak(self) -> AzWindowCreateOptionsPtr { let p = az_window_create_options_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for WindowCreateOptions { fn drop(&mut self) { az_window_create_options_delete(&mut self.ptr); } }
}

