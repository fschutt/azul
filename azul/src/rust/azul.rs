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
        lib: Box<Library>,
        az_string_from_utf8_unchecked: Symbol<extern fn(_: usize) -> AzString>,
        az_string_from_utf8_lossy: Symbol<extern fn(_: usize) -> AzString>,
        az_string_into_bytes: Symbol<extern fn(_: AzString) -> AzU8Vec>,
        az_string_delete: Symbol<extern fn(_: &mut AzString)>,
        az_string_deep_copy: Symbol<extern fn(_: &AzString) -> AzString>,
        az_u8_vec_delete: Symbol<extern fn(_: &mut AzU8Vec)>,
        az_u8_vec_deep_copy: Symbol<extern fn(_: &AzU8Vec) -> AzU8Vec>,
        az_string_vec_copy_from: Symbol<extern fn(_: usize) -> AzStringVec>,
        az_string_vec_delete: Symbol<extern fn(_: &mut AzStringVec)>,
        az_string_vec_deep_copy: Symbol<extern fn(_: &AzStringVec) -> AzStringVec>,
        az_gradient_stop_pre_vec_copy_from: Symbol<extern fn(_: usize) -> AzGradientStopPreVec>,
        az_gradient_stop_pre_vec_delete: Symbol<extern fn(_: &mut AzGradientStopPreVec)>,
        az_gradient_stop_pre_vec_deep_copy: Symbol<extern fn(_: &AzGradientStopPreVec) -> AzGradientStopPreVec>,
        az_option_percentage_value_delete: Symbol<extern fn(_: &mut AzOptionPercentageValue)>,
        az_option_percentage_value_deep_copy: Symbol<extern fn(_: &AzOptionPercentageValue) -> AzOptionPercentageValue>,
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
        az_size_metric_delete: Symbol<extern fn(_: &mut AzSizeMetric)>,
        az_size_metric_deep_copy: Symbol<extern fn(_: &AzSizeMetric) -> AzSizeMetric>,
        az_float_value_delete: Symbol<extern fn(_: &mut AzFloatValue)>,
        az_float_value_deep_copy: Symbol<extern fn(_: &AzFloatValue) -> AzFloatValue>,
        az_pixel_value_delete: Symbol<extern fn(_: &mut AzPixelValue)>,
        az_pixel_value_deep_copy: Symbol<extern fn(_: &AzPixelValue) -> AzPixelValue>,
        az_pixel_value_no_percent_delete: Symbol<extern fn(_: &mut AzPixelValueNoPercent)>,
        az_pixel_value_no_percent_deep_copy: Symbol<extern fn(_: &AzPixelValueNoPercent) -> AzPixelValueNoPercent>,
        az_box_shadow_clip_mode_delete: Symbol<extern fn(_: &mut AzBoxShadowClipMode)>,
        az_box_shadow_clip_mode_deep_copy: Symbol<extern fn(_: &AzBoxShadowClipMode) -> AzBoxShadowClipMode>,
        az_box_shadow_pre_display_item_delete: Symbol<extern fn(_: &mut AzBoxShadowPreDisplayItem)>,
        az_box_shadow_pre_display_item_deep_copy: Symbol<extern fn(_: &AzBoxShadowPreDisplayItem) -> AzBoxShadowPreDisplayItem>,
        az_layout_align_content_delete: Symbol<extern fn(_: &mut AzLayoutAlignContent)>,
        az_layout_align_content_deep_copy: Symbol<extern fn(_: &AzLayoutAlignContent) -> AzLayoutAlignContent>,
        az_layout_align_items_delete: Symbol<extern fn(_: &mut AzLayoutAlignItems)>,
        az_layout_align_items_deep_copy: Symbol<extern fn(_: &AzLayoutAlignItems) -> AzLayoutAlignItems>,
        az_layout_bottom_delete: Symbol<extern fn(_: &mut AzLayoutBottom)>,
        az_layout_bottom_deep_copy: Symbol<extern fn(_: &AzLayoutBottom) -> AzLayoutBottom>,
        az_layout_box_sizing_delete: Symbol<extern fn(_: &mut AzLayoutBoxSizing)>,
        az_layout_box_sizing_deep_copy: Symbol<extern fn(_: &AzLayoutBoxSizing) -> AzLayoutBoxSizing>,
        az_layout_direction_delete: Symbol<extern fn(_: &mut AzLayoutDirection)>,
        az_layout_direction_deep_copy: Symbol<extern fn(_: &AzLayoutDirection) -> AzLayoutDirection>,
        az_layout_display_delete: Symbol<extern fn(_: &mut AzLayoutDisplay)>,
        az_layout_display_deep_copy: Symbol<extern fn(_: &AzLayoutDisplay) -> AzLayoutDisplay>,
        az_layout_flex_grow_delete: Symbol<extern fn(_: &mut AzLayoutFlexGrow)>,
        az_layout_flex_grow_deep_copy: Symbol<extern fn(_: &AzLayoutFlexGrow) -> AzLayoutFlexGrow>,
        az_layout_flex_shrink_delete: Symbol<extern fn(_: &mut AzLayoutFlexShrink)>,
        az_layout_flex_shrink_deep_copy: Symbol<extern fn(_: &AzLayoutFlexShrink) -> AzLayoutFlexShrink>,
        az_layout_float_delete: Symbol<extern fn(_: &mut AzLayoutFloat)>,
        az_layout_float_deep_copy: Symbol<extern fn(_: &AzLayoutFloat) -> AzLayoutFloat>,
        az_layout_height_delete: Symbol<extern fn(_: &mut AzLayoutHeight)>,
        az_layout_height_deep_copy: Symbol<extern fn(_: &AzLayoutHeight) -> AzLayoutHeight>,
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
        az_layout_position_delete: Symbol<extern fn(_: &mut AzLayoutPosition)>,
        az_layout_position_deep_copy: Symbol<extern fn(_: &AzLayoutPosition) -> AzLayoutPosition>,
        az_layout_right_delete: Symbol<extern fn(_: &mut AzLayoutRight)>,
        az_layout_right_deep_copy: Symbol<extern fn(_: &AzLayoutRight) -> AzLayoutRight>,
        az_layout_top_delete: Symbol<extern fn(_: &mut AzLayoutTop)>,
        az_layout_top_deep_copy: Symbol<extern fn(_: &AzLayoutTop) -> AzLayoutTop>,
        az_layout_width_delete: Symbol<extern fn(_: &mut AzLayoutWidth)>,
        az_layout_width_deep_copy: Symbol<extern fn(_: &AzLayoutWidth) -> AzLayoutWidth>,
        az_layout_wrap_delete: Symbol<extern fn(_: &mut AzLayoutWrap)>,
        az_layout_wrap_deep_copy: Symbol<extern fn(_: &AzLayoutWrap) -> AzLayoutWrap>,
        az_overflow_delete: Symbol<extern fn(_: &mut AzOverflow)>,
        az_overflow_deep_copy: Symbol<extern fn(_: &AzOverflow) -> AzOverflow>,
        az_percentage_value_delete: Symbol<extern fn(_: &mut AzPercentageValue)>,
        az_percentage_value_deep_copy: Symbol<extern fn(_: &AzPercentageValue) -> AzPercentageValue>,
        az_gradient_stop_pre_delete: Symbol<extern fn(_: &mut AzGradientStopPre)>,
        az_gradient_stop_pre_deep_copy: Symbol<extern fn(_: &AzGradientStopPre) -> AzGradientStopPre>,
        az_direction_corner_delete: Symbol<extern fn(_: &mut AzDirectionCorner)>,
        az_direction_corner_deep_copy: Symbol<extern fn(_: &AzDirectionCorner) -> AzDirectionCorner>,
        az_direction_corners_delete: Symbol<extern fn(_: &mut AzDirectionCorners)>,
        az_direction_corners_deep_copy: Symbol<extern fn(_: &AzDirectionCorners) -> AzDirectionCorners>,
        az_direction_delete: Symbol<extern fn(_: &mut AzDirection)>,
        az_direction_deep_copy: Symbol<extern fn(_: &AzDirection) -> AzDirection>,
        az_extend_mode_delete: Symbol<extern fn(_: &mut AzExtendMode)>,
        az_extend_mode_deep_copy: Symbol<extern fn(_: &AzExtendMode) -> AzExtendMode>,
        az_linear_gradient_delete: Symbol<extern fn(_: &mut AzLinearGradient)>,
        az_linear_gradient_deep_copy: Symbol<extern fn(_: &AzLinearGradient) -> AzLinearGradient>,
        az_shape_delete: Symbol<extern fn(_: &mut AzShape)>,
        az_shape_deep_copy: Symbol<extern fn(_: &AzShape) -> AzShape>,
        az_radial_gradient_delete: Symbol<extern fn(_: &mut AzRadialGradient)>,
        az_radial_gradient_deep_copy: Symbol<extern fn(_: &AzRadialGradient) -> AzRadialGradient>,
        az_css_image_id_delete: Symbol<extern fn(_: &mut AzCssImageId)>,
        az_css_image_id_deep_copy: Symbol<extern fn(_: &AzCssImageId) -> AzCssImageId>,
        az_style_background_content_delete: Symbol<extern fn(_: &mut AzStyleBackgroundContent)>,
        az_style_background_content_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundContent) -> AzStyleBackgroundContent>,
        az_background_position_horizontal_delete: Symbol<extern fn(_: &mut AzBackgroundPositionHorizontal)>,
        az_background_position_horizontal_deep_copy: Symbol<extern fn(_: &AzBackgroundPositionHorizontal) -> AzBackgroundPositionHorizontal>,
        az_background_position_vertical_delete: Symbol<extern fn(_: &mut AzBackgroundPositionVertical)>,
        az_background_position_vertical_deep_copy: Symbol<extern fn(_: &AzBackgroundPositionVertical) -> AzBackgroundPositionVertical>,
        az_style_background_position_delete: Symbol<extern fn(_: &mut AzStyleBackgroundPosition)>,
        az_style_background_position_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundPosition) -> AzStyleBackgroundPosition>,
        az_style_background_repeat_delete: Symbol<extern fn(_: &mut AzStyleBackgroundRepeat)>,
        az_style_background_repeat_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundRepeat) -> AzStyleBackgroundRepeat>,
        az_style_background_size_delete: Symbol<extern fn(_: &mut AzStyleBackgroundSize)>,
        az_style_background_size_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundSize) -> AzStyleBackgroundSize>,
        az_style_border_bottom_color_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomColor)>,
        az_style_border_bottom_color_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomColor) -> AzStyleBorderBottomColor>,
        az_style_border_bottom_left_radius_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomLeftRadius)>,
        az_style_border_bottom_left_radius_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadius>,
        az_style_border_bottom_right_radius_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomRightRadius)>,
        az_style_border_bottom_right_radius_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadius>,
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
        az_style_text_alignment_horz_delete: Symbol<extern fn(_: &mut AzStyleTextAlignmentHorz)>,
        az_style_text_alignment_horz_deep_copy: Symbol<extern fn(_: &AzStyleTextAlignmentHorz) -> AzStyleTextAlignmentHorz>,
        az_style_text_color_delete: Symbol<extern fn(_: &mut AzStyleTextColor)>,
        az_style_text_color_deep_copy: Symbol<extern fn(_: &AzStyleTextColor) -> AzStyleTextColor>,
        az_style_word_spacing_delete: Symbol<extern fn(_: &mut AzStyleWordSpacing)>,
        az_style_word_spacing_deep_copy: Symbol<extern fn(_: &AzStyleWordSpacing) -> AzStyleWordSpacing>,
        az_box_shadow_pre_display_item_value_delete: Symbol<extern fn(_: &mut AzBoxShadowPreDisplayItemValue)>,
        az_box_shadow_pre_display_item_value_deep_copy: Symbol<extern fn(_: &AzBoxShadowPreDisplayItemValue) -> AzBoxShadowPreDisplayItemValue>,
        az_layout_align_content_value_delete: Symbol<extern fn(_: &mut AzLayoutAlignContentValue)>,
        az_layout_align_content_value_deep_copy: Symbol<extern fn(_: &AzLayoutAlignContentValue) -> AzLayoutAlignContentValue>,
        az_layout_align_items_value_delete: Symbol<extern fn(_: &mut AzLayoutAlignItemsValue)>,
        az_layout_align_items_value_deep_copy: Symbol<extern fn(_: &AzLayoutAlignItemsValue) -> AzLayoutAlignItemsValue>,
        az_layout_bottom_value_delete: Symbol<extern fn(_: &mut AzLayoutBottomValue)>,
        az_layout_bottom_value_deep_copy: Symbol<extern fn(_: &AzLayoutBottomValue) -> AzLayoutBottomValue>,
        az_layout_box_sizing_value_delete: Symbol<extern fn(_: &mut AzLayoutBoxSizingValue)>,
        az_layout_box_sizing_value_deep_copy: Symbol<extern fn(_: &AzLayoutBoxSizingValue) -> AzLayoutBoxSizingValue>,
        az_layout_direction_value_delete: Symbol<extern fn(_: &mut AzLayoutDirectionValue)>,
        az_layout_direction_value_deep_copy: Symbol<extern fn(_: &AzLayoutDirectionValue) -> AzLayoutDirectionValue>,
        az_layout_display_value_delete: Symbol<extern fn(_: &mut AzLayoutDisplayValue)>,
        az_layout_display_value_deep_copy: Symbol<extern fn(_: &AzLayoutDisplayValue) -> AzLayoutDisplayValue>,
        az_layout_flex_grow_value_delete: Symbol<extern fn(_: &mut AzLayoutFlexGrowValue)>,
        az_layout_flex_grow_value_deep_copy: Symbol<extern fn(_: &AzLayoutFlexGrowValue) -> AzLayoutFlexGrowValue>,
        az_layout_flex_shrink_value_delete: Symbol<extern fn(_: &mut AzLayoutFlexShrinkValue)>,
        az_layout_flex_shrink_value_deep_copy: Symbol<extern fn(_: &AzLayoutFlexShrinkValue) -> AzLayoutFlexShrinkValue>,
        az_layout_float_value_delete: Symbol<extern fn(_: &mut AzLayoutFloatValue)>,
        az_layout_float_value_deep_copy: Symbol<extern fn(_: &AzLayoutFloatValue) -> AzLayoutFloatValue>,
        az_layout_height_value_delete: Symbol<extern fn(_: &mut AzLayoutHeightValue)>,
        az_layout_height_value_deep_copy: Symbol<extern fn(_: &AzLayoutHeightValue) -> AzLayoutHeightValue>,
        az_layout_justify_content_value_delete: Symbol<extern fn(_: &mut AzLayoutJustifyContentValue)>,
        az_layout_justify_content_value_deep_copy: Symbol<extern fn(_: &AzLayoutJustifyContentValue) -> AzLayoutJustifyContentValue>,
        az_layout_left_value_delete: Symbol<extern fn(_: &mut AzLayoutLeftValue)>,
        az_layout_left_value_deep_copy: Symbol<extern fn(_: &AzLayoutLeftValue) -> AzLayoutLeftValue>,
        az_layout_margin_bottom_value_delete: Symbol<extern fn(_: &mut AzLayoutMarginBottomValue)>,
        az_layout_margin_bottom_value_deep_copy: Symbol<extern fn(_: &AzLayoutMarginBottomValue) -> AzLayoutMarginBottomValue>,
        az_layout_margin_left_value_delete: Symbol<extern fn(_: &mut AzLayoutMarginLeftValue)>,
        az_layout_margin_left_value_deep_copy: Symbol<extern fn(_: &AzLayoutMarginLeftValue) -> AzLayoutMarginLeftValue>,
        az_layout_margin_right_value_delete: Symbol<extern fn(_: &mut AzLayoutMarginRightValue)>,
        az_layout_margin_right_value_deep_copy: Symbol<extern fn(_: &AzLayoutMarginRightValue) -> AzLayoutMarginRightValue>,
        az_layout_margin_top_value_delete: Symbol<extern fn(_: &mut AzLayoutMarginTopValue)>,
        az_layout_margin_top_value_deep_copy: Symbol<extern fn(_: &AzLayoutMarginTopValue) -> AzLayoutMarginTopValue>,
        az_layout_max_height_value_delete: Symbol<extern fn(_: &mut AzLayoutMaxHeightValue)>,
        az_layout_max_height_value_deep_copy: Symbol<extern fn(_: &AzLayoutMaxHeightValue) -> AzLayoutMaxHeightValue>,
        az_layout_max_width_value_delete: Symbol<extern fn(_: &mut AzLayoutMaxWidthValue)>,
        az_layout_max_width_value_deep_copy: Symbol<extern fn(_: &AzLayoutMaxWidthValue) -> AzLayoutMaxWidthValue>,
        az_layout_min_height_value_delete: Symbol<extern fn(_: &mut AzLayoutMinHeightValue)>,
        az_layout_min_height_value_deep_copy: Symbol<extern fn(_: &AzLayoutMinHeightValue) -> AzLayoutMinHeightValue>,
        az_layout_min_width_value_delete: Symbol<extern fn(_: &mut AzLayoutMinWidthValue)>,
        az_layout_min_width_value_deep_copy: Symbol<extern fn(_: &AzLayoutMinWidthValue) -> AzLayoutMinWidthValue>,
        az_layout_padding_bottom_value_delete: Symbol<extern fn(_: &mut AzLayoutPaddingBottomValue)>,
        az_layout_padding_bottom_value_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingBottomValue) -> AzLayoutPaddingBottomValue>,
        az_layout_padding_left_value_delete: Symbol<extern fn(_: &mut AzLayoutPaddingLeftValue)>,
        az_layout_padding_left_value_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingLeftValue) -> AzLayoutPaddingLeftValue>,
        az_layout_padding_right_value_delete: Symbol<extern fn(_: &mut AzLayoutPaddingRightValue)>,
        az_layout_padding_right_value_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingRightValue) -> AzLayoutPaddingRightValue>,
        az_layout_padding_top_value_delete: Symbol<extern fn(_: &mut AzLayoutPaddingTopValue)>,
        az_layout_padding_top_value_deep_copy: Symbol<extern fn(_: &AzLayoutPaddingTopValue) -> AzLayoutPaddingTopValue>,
        az_layout_position_value_delete: Symbol<extern fn(_: &mut AzLayoutPositionValue)>,
        az_layout_position_value_deep_copy: Symbol<extern fn(_: &AzLayoutPositionValue) -> AzLayoutPositionValue>,
        az_layout_right_value_delete: Symbol<extern fn(_: &mut AzLayoutRightValue)>,
        az_layout_right_value_deep_copy: Symbol<extern fn(_: &AzLayoutRightValue) -> AzLayoutRightValue>,
        az_layout_top_value_delete: Symbol<extern fn(_: &mut AzLayoutTopValue)>,
        az_layout_top_value_deep_copy: Symbol<extern fn(_: &AzLayoutTopValue) -> AzLayoutTopValue>,
        az_layout_width_value_delete: Symbol<extern fn(_: &mut AzLayoutWidthValue)>,
        az_layout_width_value_deep_copy: Symbol<extern fn(_: &AzLayoutWidthValue) -> AzLayoutWidthValue>,
        az_layout_wrap_value_delete: Symbol<extern fn(_: &mut AzLayoutWrapValue)>,
        az_layout_wrap_value_deep_copy: Symbol<extern fn(_: &AzLayoutWrapValue) -> AzLayoutWrapValue>,
        az_overflow_value_delete: Symbol<extern fn(_: &mut AzOverflowValue)>,
        az_overflow_value_deep_copy: Symbol<extern fn(_: &AzOverflowValue) -> AzOverflowValue>,
        az_style_background_content_value_delete: Symbol<extern fn(_: &mut AzStyleBackgroundContentValue)>,
        az_style_background_content_value_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue>,
        az_style_background_position_value_delete: Symbol<extern fn(_: &mut AzStyleBackgroundPositionValue)>,
        az_style_background_position_value_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundPositionValue) -> AzStyleBackgroundPositionValue>,
        az_style_background_repeat_value_delete: Symbol<extern fn(_: &mut AzStyleBackgroundRepeatValue)>,
        az_style_background_repeat_value_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundRepeatValue) -> AzStyleBackgroundRepeatValue>,
        az_style_background_size_value_delete: Symbol<extern fn(_: &mut AzStyleBackgroundSizeValue)>,
        az_style_background_size_value_deep_copy: Symbol<extern fn(_: &AzStyleBackgroundSizeValue) -> AzStyleBackgroundSizeValue>,
        az_style_border_bottom_color_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomColorValue)>,
        az_style_border_bottom_color_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomColorValue) -> AzStyleBorderBottomColorValue>,
        az_style_border_bottom_left_radius_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomLeftRadiusValue)>,
        az_style_border_bottom_left_radius_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomLeftRadiusValue) -> AzStyleBorderBottomLeftRadiusValue>,
        az_style_border_bottom_right_radius_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomRightRadiusValue)>,
        az_style_border_bottom_right_radius_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomRightRadiusValue) -> AzStyleBorderBottomRightRadiusValue>,
        az_style_border_bottom_style_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomStyleValue)>,
        az_style_border_bottom_style_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomStyleValue) -> AzStyleBorderBottomStyleValue>,
        az_style_border_bottom_width_value_delete: Symbol<extern fn(_: &mut AzStyleBorderBottomWidthValue)>,
        az_style_border_bottom_width_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderBottomWidthValue) -> AzStyleBorderBottomWidthValue>,
        az_style_border_left_color_value_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftColorValue)>,
        az_style_border_left_color_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderLeftColorValue) -> AzStyleBorderLeftColorValue>,
        az_style_border_left_style_value_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftStyleValue)>,
        az_style_border_left_style_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderLeftStyleValue) -> AzStyleBorderLeftStyleValue>,
        az_style_border_left_width_value_delete: Symbol<extern fn(_: &mut AzStyleBorderLeftWidthValue)>,
        az_style_border_left_width_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderLeftWidthValue) -> AzStyleBorderLeftWidthValue>,
        az_style_border_right_color_value_delete: Symbol<extern fn(_: &mut AzStyleBorderRightColorValue)>,
        az_style_border_right_color_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderRightColorValue) -> AzStyleBorderRightColorValue>,
        az_style_border_right_style_value_delete: Symbol<extern fn(_: &mut AzStyleBorderRightStyleValue)>,
        az_style_border_right_style_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderRightStyleValue) -> AzStyleBorderRightStyleValue>,
        az_style_border_right_width_value_delete: Symbol<extern fn(_: &mut AzStyleBorderRightWidthValue)>,
        az_style_border_right_width_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderRightWidthValue) -> AzStyleBorderRightWidthValue>,
        az_style_border_top_color_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopColorValue)>,
        az_style_border_top_color_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopColorValue) -> AzStyleBorderTopColorValue>,
        az_style_border_top_left_radius_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopLeftRadiusValue)>,
        az_style_border_top_left_radius_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopLeftRadiusValue) -> AzStyleBorderTopLeftRadiusValue>,
        az_style_border_top_right_radius_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopRightRadiusValue)>,
        az_style_border_top_right_radius_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopRightRadiusValue) -> AzStyleBorderTopRightRadiusValue>,
        az_style_border_top_style_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopStyleValue)>,
        az_style_border_top_style_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopStyleValue) -> AzStyleBorderTopStyleValue>,
        az_style_border_top_width_value_delete: Symbol<extern fn(_: &mut AzStyleBorderTopWidthValue)>,
        az_style_border_top_width_value_deep_copy: Symbol<extern fn(_: &AzStyleBorderTopWidthValue) -> AzStyleBorderTopWidthValue>,
        az_style_cursor_value_delete: Symbol<extern fn(_: &mut AzStyleCursorValue)>,
        az_style_cursor_value_deep_copy: Symbol<extern fn(_: &AzStyleCursorValue) -> AzStyleCursorValue>,
        az_style_font_family_value_delete: Symbol<extern fn(_: &mut AzStyleFontFamilyValue)>,
        az_style_font_family_value_deep_copy: Symbol<extern fn(_: &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue>,
        az_style_font_size_value_delete: Symbol<extern fn(_: &mut AzStyleFontSizeValue)>,
        az_style_font_size_value_deep_copy: Symbol<extern fn(_: &AzStyleFontSizeValue) -> AzStyleFontSizeValue>,
        az_style_letter_spacing_value_delete: Symbol<extern fn(_: &mut AzStyleLetterSpacingValue)>,
        az_style_letter_spacing_value_deep_copy: Symbol<extern fn(_: &AzStyleLetterSpacingValue) -> AzStyleLetterSpacingValue>,
        az_style_line_height_value_delete: Symbol<extern fn(_: &mut AzStyleLineHeightValue)>,
        az_style_line_height_value_deep_copy: Symbol<extern fn(_: &AzStyleLineHeightValue) -> AzStyleLineHeightValue>,
        az_style_tab_width_value_delete: Symbol<extern fn(_: &mut AzStyleTabWidthValue)>,
        az_style_tab_width_value_deep_copy: Symbol<extern fn(_: &AzStyleTabWidthValue) -> AzStyleTabWidthValue>,
        az_style_text_alignment_horz_value_delete: Symbol<extern fn(_: &mut AzStyleTextAlignmentHorzValue)>,
        az_style_text_alignment_horz_value_deep_copy: Symbol<extern fn(_: &AzStyleTextAlignmentHorzValue) -> AzStyleTextAlignmentHorzValue>,
        az_style_text_color_value_delete: Symbol<extern fn(_: &mut AzStyleTextColorValue)>,
        az_style_text_color_value_deep_copy: Symbol<extern fn(_: &AzStyleTextColorValue) -> AzStyleTextColorValue>,
        az_style_word_spacing_value_delete: Symbol<extern fn(_: &mut AzStyleWordSpacingValue)>,
        az_style_word_spacing_value_deep_copy: Symbol<extern fn(_: &AzStyleWordSpacingValue) -> AzStyleWordSpacingValue>,
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
        az_event_filter_delete: Symbol<extern fn(_: &mut AzEventFilter)>,
        az_event_filter_deep_copy: Symbol<extern fn(_: &AzEventFilter) -> AzEventFilter>,
        az_hover_event_filter_delete: Symbol<extern fn(_: &mut AzHoverEventFilter)>,
        az_hover_event_filter_deep_copy: Symbol<extern fn(_: &AzHoverEventFilter) -> AzHoverEventFilter>,
        az_focus_event_filter_delete: Symbol<extern fn(_: &mut AzFocusEventFilter)>,
        az_focus_event_filter_deep_copy: Symbol<extern fn(_: &AzFocusEventFilter) -> AzFocusEventFilter>,
        az_not_event_filter_delete: Symbol<extern fn(_: &mut AzNotEventFilter)>,
        az_not_event_filter_deep_copy: Symbol<extern fn(_: &AzNotEventFilter) -> AzNotEventFilter>,
        az_window_event_filter_delete: Symbol<extern fn(_: &mut AzWindowEventFilter)>,
        az_window_event_filter_deep_copy: Symbol<extern fn(_: &AzWindowEventFilter) -> AzWindowEventFilter>,
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
        az_image_source_delete: Symbol<extern fn(_: &mut AzImageSource)>,
        az_image_source_deep_copy: Symbol<extern fn(_: &AzImageSource) -> AzImageSource>,
        az_font_source_delete: Symbol<extern fn(_: &mut AzFontSource)>,
        az_font_source_deep_copy: Symbol<extern fn(_: &AzFontSource) -> AzFontSource>,
        az_raw_image_new: Symbol<extern fn(_: AzRawImageFormat) -> AzRawImage>,
        az_raw_image_delete: Symbol<extern fn(_: &mut AzRawImage)>,
        az_raw_image_deep_copy: Symbol<extern fn(_: &AzRawImage) -> AzRawImage>,
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
        let az_u8_vec_delete = unsafe { lib.get::<extern fn(_: &mut AzU8Vec)>(b"az_u8_vec_delete").ok()? };
        let az_u8_vec_deep_copy = unsafe { lib.get::<extern fn(_: &AzU8Vec) -> AzU8Vec>(b"az_u8_vec_deep_copy").ok()? };
        let az_string_vec_copy_from = unsafe { lib.get::<extern fn(_: usize) -> AzStringVec>(b"az_string_vec_copy_from").ok()? };
        let az_string_vec_delete = unsafe { lib.get::<extern fn(_: &mut AzStringVec)>(b"az_string_vec_delete").ok()? };
        let az_string_vec_deep_copy = unsafe { lib.get::<extern fn(_: &AzStringVec) -> AzStringVec>(b"az_string_vec_deep_copy").ok()? };
        let az_gradient_stop_pre_vec_copy_from = unsafe { lib.get::<extern fn(_: usize) -> AzGradientStopPreVec>(b"az_gradient_stop_pre_vec_copy_from").ok()? };
        let az_gradient_stop_pre_vec_delete = unsafe { lib.get::<extern fn(_: &mut AzGradientStopPreVec)>(b"az_gradient_stop_pre_vec_delete").ok()? };
        let az_gradient_stop_pre_vec_deep_copy = unsafe { lib.get::<extern fn(_: &AzGradientStopPreVec) -> AzGradientStopPreVec>(b"az_gradient_stop_pre_vec_deep_copy").ok()? };
        let az_option_percentage_value_delete = unsafe { lib.get::<extern fn(_: &mut AzOptionPercentageValue)>(b"az_option_percentage_value_delete").ok()? };
        let az_option_percentage_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzOptionPercentageValue) -> AzOptionPercentageValue>(b"az_option_percentage_value_deep_copy").ok()? };
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
        let az_size_metric_delete = unsafe { lib.get::<extern fn(_: &mut AzSizeMetric)>(b"az_size_metric_delete").ok()? };
        let az_size_metric_deep_copy = unsafe { lib.get::<extern fn(_: &AzSizeMetric) -> AzSizeMetric>(b"az_size_metric_deep_copy").ok()? };
        let az_float_value_delete = unsafe { lib.get::<extern fn(_: &mut AzFloatValue)>(b"az_float_value_delete").ok()? };
        let az_float_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzFloatValue) -> AzFloatValue>(b"az_float_value_deep_copy").ok()? };
        let az_pixel_value_delete = unsafe { lib.get::<extern fn(_: &mut AzPixelValue)>(b"az_pixel_value_delete").ok()? };
        let az_pixel_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzPixelValue) -> AzPixelValue>(b"az_pixel_value_deep_copy").ok()? };
        let az_pixel_value_no_percent_delete = unsafe { lib.get::<extern fn(_: &mut AzPixelValueNoPercent)>(b"az_pixel_value_no_percent_delete").ok()? };
        let az_pixel_value_no_percent_deep_copy = unsafe { lib.get::<extern fn(_: &AzPixelValueNoPercent) -> AzPixelValueNoPercent>(b"az_pixel_value_no_percent_deep_copy").ok()? };
        let az_box_shadow_clip_mode_delete = unsafe { lib.get::<extern fn(_: &mut AzBoxShadowClipMode)>(b"az_box_shadow_clip_mode_delete").ok()? };
        let az_box_shadow_clip_mode_deep_copy = unsafe { lib.get::<extern fn(_: &AzBoxShadowClipMode) -> AzBoxShadowClipMode>(b"az_box_shadow_clip_mode_deep_copy").ok()? };
        let az_box_shadow_pre_display_item_delete = unsafe { lib.get::<extern fn(_: &mut AzBoxShadowPreDisplayItem)>(b"az_box_shadow_pre_display_item_delete").ok()? };
        let az_box_shadow_pre_display_item_deep_copy = unsafe { lib.get::<extern fn(_: &AzBoxShadowPreDisplayItem) -> AzBoxShadowPreDisplayItem>(b"az_box_shadow_pre_display_item_deep_copy").ok()? };
        let az_layout_align_content_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutAlignContent)>(b"az_layout_align_content_delete").ok()? };
        let az_layout_align_content_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutAlignContent) -> AzLayoutAlignContent>(b"az_layout_align_content_deep_copy").ok()? };
        let az_layout_align_items_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutAlignItems)>(b"az_layout_align_items_delete").ok()? };
        let az_layout_align_items_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutAlignItems) -> AzLayoutAlignItems>(b"az_layout_align_items_deep_copy").ok()? };
        let az_layout_bottom_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutBottom)>(b"az_layout_bottom_delete").ok()? };
        let az_layout_bottom_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutBottom) -> AzLayoutBottom>(b"az_layout_bottom_deep_copy").ok()? };
        let az_layout_box_sizing_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutBoxSizing)>(b"az_layout_box_sizing_delete").ok()? };
        let az_layout_box_sizing_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutBoxSizing) -> AzLayoutBoxSizing>(b"az_layout_box_sizing_deep_copy").ok()? };
        let az_layout_direction_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutDirection)>(b"az_layout_direction_delete").ok()? };
        let az_layout_direction_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutDirection) -> AzLayoutDirection>(b"az_layout_direction_deep_copy").ok()? };
        let az_layout_display_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutDisplay)>(b"az_layout_display_delete").ok()? };
        let az_layout_display_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutDisplay) -> AzLayoutDisplay>(b"az_layout_display_deep_copy").ok()? };
        let az_layout_flex_grow_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFlexGrow)>(b"az_layout_flex_grow_delete").ok()? };
        let az_layout_flex_grow_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFlexGrow) -> AzLayoutFlexGrow>(b"az_layout_flex_grow_deep_copy").ok()? };
        let az_layout_flex_shrink_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFlexShrink)>(b"az_layout_flex_shrink_delete").ok()? };
        let az_layout_flex_shrink_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFlexShrink) -> AzLayoutFlexShrink>(b"az_layout_flex_shrink_deep_copy").ok()? };
        let az_layout_float_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFloat)>(b"az_layout_float_delete").ok()? };
        let az_layout_float_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFloat) -> AzLayoutFloat>(b"az_layout_float_deep_copy").ok()? };
        let az_layout_height_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutHeight)>(b"az_layout_height_delete").ok()? };
        let az_layout_height_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutHeight) -> AzLayoutHeight>(b"az_layout_height_deep_copy").ok()? };
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
        let az_layout_position_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPosition)>(b"az_layout_position_delete").ok()? };
        let az_layout_position_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPosition) -> AzLayoutPosition>(b"az_layout_position_deep_copy").ok()? };
        let az_layout_right_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutRight)>(b"az_layout_right_delete").ok()? };
        let az_layout_right_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutRight) -> AzLayoutRight>(b"az_layout_right_deep_copy").ok()? };
        let az_layout_top_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutTop)>(b"az_layout_top_delete").ok()? };
        let az_layout_top_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutTop) -> AzLayoutTop>(b"az_layout_top_deep_copy").ok()? };
        let az_layout_width_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutWidth)>(b"az_layout_width_delete").ok()? };
        let az_layout_width_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutWidth) -> AzLayoutWidth>(b"az_layout_width_deep_copy").ok()? };
        let az_layout_wrap_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutWrap)>(b"az_layout_wrap_delete").ok()? };
        let az_layout_wrap_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutWrap) -> AzLayoutWrap>(b"az_layout_wrap_deep_copy").ok()? };
        let az_overflow_delete = unsafe { lib.get::<extern fn(_: &mut AzOverflow)>(b"az_overflow_delete").ok()? };
        let az_overflow_deep_copy = unsafe { lib.get::<extern fn(_: &AzOverflow) -> AzOverflow>(b"az_overflow_deep_copy").ok()? };
        let az_percentage_value_delete = unsafe { lib.get::<extern fn(_: &mut AzPercentageValue)>(b"az_percentage_value_delete").ok()? };
        let az_percentage_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzPercentageValue) -> AzPercentageValue>(b"az_percentage_value_deep_copy").ok()? };
        let az_gradient_stop_pre_delete = unsafe { lib.get::<extern fn(_: &mut AzGradientStopPre)>(b"az_gradient_stop_pre_delete").ok()? };
        let az_gradient_stop_pre_deep_copy = unsafe { lib.get::<extern fn(_: &AzGradientStopPre) -> AzGradientStopPre>(b"az_gradient_stop_pre_deep_copy").ok()? };
        let az_direction_corner_delete = unsafe { lib.get::<extern fn(_: &mut AzDirectionCorner)>(b"az_direction_corner_delete").ok()? };
        let az_direction_corner_deep_copy = unsafe { lib.get::<extern fn(_: &AzDirectionCorner) -> AzDirectionCorner>(b"az_direction_corner_deep_copy").ok()? };
        let az_direction_corners_delete = unsafe { lib.get::<extern fn(_: &mut AzDirectionCorners)>(b"az_direction_corners_delete").ok()? };
        let az_direction_corners_deep_copy = unsafe { lib.get::<extern fn(_: &AzDirectionCorners) -> AzDirectionCorners>(b"az_direction_corners_deep_copy").ok()? };
        let az_direction_delete = unsafe { lib.get::<extern fn(_: &mut AzDirection)>(b"az_direction_delete").ok()? };
        let az_direction_deep_copy = unsafe { lib.get::<extern fn(_: &AzDirection) -> AzDirection>(b"az_direction_deep_copy").ok()? };
        let az_extend_mode_delete = unsafe { lib.get::<extern fn(_: &mut AzExtendMode)>(b"az_extend_mode_delete").ok()? };
        let az_extend_mode_deep_copy = unsafe { lib.get::<extern fn(_: &AzExtendMode) -> AzExtendMode>(b"az_extend_mode_deep_copy").ok()? };
        let az_linear_gradient_delete = unsafe { lib.get::<extern fn(_: &mut AzLinearGradient)>(b"az_linear_gradient_delete").ok()? };
        let az_linear_gradient_deep_copy = unsafe { lib.get::<extern fn(_: &AzLinearGradient) -> AzLinearGradient>(b"az_linear_gradient_deep_copy").ok()? };
        let az_shape_delete = unsafe { lib.get::<extern fn(_: &mut AzShape)>(b"az_shape_delete").ok()? };
        let az_shape_deep_copy = unsafe { lib.get::<extern fn(_: &AzShape) -> AzShape>(b"az_shape_deep_copy").ok()? };
        let az_radial_gradient_delete = unsafe { lib.get::<extern fn(_: &mut AzRadialGradient)>(b"az_radial_gradient_delete").ok()? };
        let az_radial_gradient_deep_copy = unsafe { lib.get::<extern fn(_: &AzRadialGradient) -> AzRadialGradient>(b"az_radial_gradient_deep_copy").ok()? };
        let az_css_image_id_delete = unsafe { lib.get::<extern fn(_: &mut AzCssImageId)>(b"az_css_image_id_delete").ok()? };
        let az_css_image_id_deep_copy = unsafe { lib.get::<extern fn(_: &AzCssImageId) -> AzCssImageId>(b"az_css_image_id_deep_copy").ok()? };
        let az_style_background_content_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundContent)>(b"az_style_background_content_delete").ok()? };
        let az_style_background_content_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundContent) -> AzStyleBackgroundContent>(b"az_style_background_content_deep_copy").ok()? };
        let az_background_position_horizontal_delete = unsafe { lib.get::<extern fn(_: &mut AzBackgroundPositionHorizontal)>(b"az_background_position_horizontal_delete").ok()? };
        let az_background_position_horizontal_deep_copy = unsafe { lib.get::<extern fn(_: &AzBackgroundPositionHorizontal) -> AzBackgroundPositionHorizontal>(b"az_background_position_horizontal_deep_copy").ok()? };
        let az_background_position_vertical_delete = unsafe { lib.get::<extern fn(_: &mut AzBackgroundPositionVertical)>(b"az_background_position_vertical_delete").ok()? };
        let az_background_position_vertical_deep_copy = unsafe { lib.get::<extern fn(_: &AzBackgroundPositionVertical) -> AzBackgroundPositionVertical>(b"az_background_position_vertical_deep_copy").ok()? };
        let az_style_background_position_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundPosition)>(b"az_style_background_position_delete").ok()? };
        let az_style_background_position_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundPosition) -> AzStyleBackgroundPosition>(b"az_style_background_position_deep_copy").ok()? };
        let az_style_background_repeat_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundRepeat)>(b"az_style_background_repeat_delete").ok()? };
        let az_style_background_repeat_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundRepeat) -> AzStyleBackgroundRepeat>(b"az_style_background_repeat_deep_copy").ok()? };
        let az_style_background_size_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundSize)>(b"az_style_background_size_delete").ok()? };
        let az_style_background_size_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundSize) -> AzStyleBackgroundSize>(b"az_style_background_size_deep_copy").ok()? };
        let az_style_border_bottom_color_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomColor)>(b"az_style_border_bottom_color_delete").ok()? };
        let az_style_border_bottom_color_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomColor) -> AzStyleBorderBottomColor>(b"az_style_border_bottom_color_deep_copy").ok()? };
        let az_style_border_bottom_left_radius_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomLeftRadius)>(b"az_style_border_bottom_left_radius_delete").ok()? };
        let az_style_border_bottom_left_radius_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomLeftRadius) -> AzStyleBorderBottomLeftRadius>(b"az_style_border_bottom_left_radius_deep_copy").ok()? };
        let az_style_border_bottom_right_radius_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomRightRadius)>(b"az_style_border_bottom_right_radius_delete").ok()? };
        let az_style_border_bottom_right_radius_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomRightRadius) -> AzStyleBorderBottomRightRadius>(b"az_style_border_bottom_right_radius_deep_copy").ok()? };
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
        let az_style_text_alignment_horz_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTextAlignmentHorz)>(b"az_style_text_alignment_horz_delete").ok()? };
        let az_style_text_alignment_horz_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTextAlignmentHorz) -> AzStyleTextAlignmentHorz>(b"az_style_text_alignment_horz_deep_copy").ok()? };
        let az_style_text_color_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTextColor)>(b"az_style_text_color_delete").ok()? };
        let az_style_text_color_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTextColor) -> AzStyleTextColor>(b"az_style_text_color_deep_copy").ok()? };
        let az_style_word_spacing_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleWordSpacing)>(b"az_style_word_spacing_delete").ok()? };
        let az_style_word_spacing_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleWordSpacing) -> AzStyleWordSpacing>(b"az_style_word_spacing_deep_copy").ok()? };
        let az_box_shadow_pre_display_item_value_delete = unsafe { lib.get::<extern fn(_: &mut AzBoxShadowPreDisplayItemValue)>(b"az_box_shadow_pre_display_item_value_delete").ok()? };
        let az_box_shadow_pre_display_item_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzBoxShadowPreDisplayItemValue) -> AzBoxShadowPreDisplayItemValue>(b"az_box_shadow_pre_display_item_value_deep_copy").ok()? };
        let az_layout_align_content_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutAlignContentValue)>(b"az_layout_align_content_value_delete").ok()? };
        let az_layout_align_content_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutAlignContentValue) -> AzLayoutAlignContentValue>(b"az_layout_align_content_value_deep_copy").ok()? };
        let az_layout_align_items_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutAlignItemsValue)>(b"az_layout_align_items_value_delete").ok()? };
        let az_layout_align_items_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutAlignItemsValue) -> AzLayoutAlignItemsValue>(b"az_layout_align_items_value_deep_copy").ok()? };
        let az_layout_bottom_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutBottomValue)>(b"az_layout_bottom_value_delete").ok()? };
        let az_layout_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutBottomValue) -> AzLayoutBottomValue>(b"az_layout_bottom_value_deep_copy").ok()? };
        let az_layout_box_sizing_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutBoxSizingValue)>(b"az_layout_box_sizing_value_delete").ok()? };
        let az_layout_box_sizing_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutBoxSizingValue) -> AzLayoutBoxSizingValue>(b"az_layout_box_sizing_value_deep_copy").ok()? };
        let az_layout_direction_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutDirectionValue)>(b"az_layout_direction_value_delete").ok()? };
        let az_layout_direction_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutDirectionValue) -> AzLayoutDirectionValue>(b"az_layout_direction_value_deep_copy").ok()? };
        let az_layout_display_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutDisplayValue)>(b"az_layout_display_value_delete").ok()? };
        let az_layout_display_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutDisplayValue) -> AzLayoutDisplayValue>(b"az_layout_display_value_deep_copy").ok()? };
        let az_layout_flex_grow_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFlexGrowValue)>(b"az_layout_flex_grow_value_delete").ok()? };
        let az_layout_flex_grow_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFlexGrowValue) -> AzLayoutFlexGrowValue>(b"az_layout_flex_grow_value_deep_copy").ok()? };
        let az_layout_flex_shrink_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFlexShrinkValue)>(b"az_layout_flex_shrink_value_delete").ok()? };
        let az_layout_flex_shrink_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFlexShrinkValue) -> AzLayoutFlexShrinkValue>(b"az_layout_flex_shrink_value_deep_copy").ok()? };
        let az_layout_float_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutFloatValue)>(b"az_layout_float_value_delete").ok()? };
        let az_layout_float_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutFloatValue) -> AzLayoutFloatValue>(b"az_layout_float_value_deep_copy").ok()? };
        let az_layout_height_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutHeightValue)>(b"az_layout_height_value_delete").ok()? };
        let az_layout_height_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutHeightValue) -> AzLayoutHeightValue>(b"az_layout_height_value_deep_copy").ok()? };
        let az_layout_justify_content_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutJustifyContentValue)>(b"az_layout_justify_content_value_delete").ok()? };
        let az_layout_justify_content_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutJustifyContentValue) -> AzLayoutJustifyContentValue>(b"az_layout_justify_content_value_deep_copy").ok()? };
        let az_layout_left_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutLeftValue)>(b"az_layout_left_value_delete").ok()? };
        let az_layout_left_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutLeftValue) -> AzLayoutLeftValue>(b"az_layout_left_value_deep_copy").ok()? };
        let az_layout_margin_bottom_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginBottomValue)>(b"az_layout_margin_bottom_value_delete").ok()? };
        let az_layout_margin_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginBottomValue) -> AzLayoutMarginBottomValue>(b"az_layout_margin_bottom_value_deep_copy").ok()? };
        let az_layout_margin_left_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginLeftValue)>(b"az_layout_margin_left_value_delete").ok()? };
        let az_layout_margin_left_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginLeftValue) -> AzLayoutMarginLeftValue>(b"az_layout_margin_left_value_deep_copy").ok()? };
        let az_layout_margin_right_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginRightValue)>(b"az_layout_margin_right_value_delete").ok()? };
        let az_layout_margin_right_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginRightValue) -> AzLayoutMarginRightValue>(b"az_layout_margin_right_value_deep_copy").ok()? };
        let az_layout_margin_top_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMarginTopValue)>(b"az_layout_margin_top_value_delete").ok()? };
        let az_layout_margin_top_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMarginTopValue) -> AzLayoutMarginTopValue>(b"az_layout_margin_top_value_deep_copy").ok()? };
        let az_layout_max_height_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMaxHeightValue)>(b"az_layout_max_height_value_delete").ok()? };
        let az_layout_max_height_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMaxHeightValue) -> AzLayoutMaxHeightValue>(b"az_layout_max_height_value_deep_copy").ok()? };
        let az_layout_max_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMaxWidthValue)>(b"az_layout_max_width_value_delete").ok()? };
        let az_layout_max_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMaxWidthValue) -> AzLayoutMaxWidthValue>(b"az_layout_max_width_value_deep_copy").ok()? };
        let az_layout_min_height_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMinHeightValue)>(b"az_layout_min_height_value_delete").ok()? };
        let az_layout_min_height_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMinHeightValue) -> AzLayoutMinHeightValue>(b"az_layout_min_height_value_deep_copy").ok()? };
        let az_layout_min_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutMinWidthValue)>(b"az_layout_min_width_value_delete").ok()? };
        let az_layout_min_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutMinWidthValue) -> AzLayoutMinWidthValue>(b"az_layout_min_width_value_deep_copy").ok()? };
        let az_layout_padding_bottom_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingBottomValue)>(b"az_layout_padding_bottom_value_delete").ok()? };
        let az_layout_padding_bottom_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingBottomValue) -> AzLayoutPaddingBottomValue>(b"az_layout_padding_bottom_value_deep_copy").ok()? };
        let az_layout_padding_left_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingLeftValue)>(b"az_layout_padding_left_value_delete").ok()? };
        let az_layout_padding_left_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingLeftValue) -> AzLayoutPaddingLeftValue>(b"az_layout_padding_left_value_deep_copy").ok()? };
        let az_layout_padding_right_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingRightValue)>(b"az_layout_padding_right_value_delete").ok()? };
        let az_layout_padding_right_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingRightValue) -> AzLayoutPaddingRightValue>(b"az_layout_padding_right_value_deep_copy").ok()? };
        let az_layout_padding_top_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPaddingTopValue)>(b"az_layout_padding_top_value_delete").ok()? };
        let az_layout_padding_top_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPaddingTopValue) -> AzLayoutPaddingTopValue>(b"az_layout_padding_top_value_deep_copy").ok()? };
        let az_layout_position_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutPositionValue)>(b"az_layout_position_value_delete").ok()? };
        let az_layout_position_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutPositionValue) -> AzLayoutPositionValue>(b"az_layout_position_value_deep_copy").ok()? };
        let az_layout_right_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutRightValue)>(b"az_layout_right_value_delete").ok()? };
        let az_layout_right_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutRightValue) -> AzLayoutRightValue>(b"az_layout_right_value_deep_copy").ok()? };
        let az_layout_top_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutTopValue)>(b"az_layout_top_value_delete").ok()? };
        let az_layout_top_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutTopValue) -> AzLayoutTopValue>(b"az_layout_top_value_deep_copy").ok()? };
        let az_layout_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutWidthValue)>(b"az_layout_width_value_delete").ok()? };
        let az_layout_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutWidthValue) -> AzLayoutWidthValue>(b"az_layout_width_value_deep_copy").ok()? };
        let az_layout_wrap_value_delete = unsafe { lib.get::<extern fn(_: &mut AzLayoutWrapValue)>(b"az_layout_wrap_value_delete").ok()? };
        let az_layout_wrap_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzLayoutWrapValue) -> AzLayoutWrapValue>(b"az_layout_wrap_value_deep_copy").ok()? };
        let az_overflow_value_delete = unsafe { lib.get::<extern fn(_: &mut AzOverflowValue)>(b"az_overflow_value_delete").ok()? };
        let az_overflow_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzOverflowValue) -> AzOverflowValue>(b"az_overflow_value_deep_copy").ok()? };
        let az_style_background_content_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundContentValue)>(b"az_style_background_content_value_delete").ok()? };
        let az_style_background_content_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundContentValue) -> AzStyleBackgroundContentValue>(b"az_style_background_content_value_deep_copy").ok()? };
        let az_style_background_position_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundPositionValue)>(b"az_style_background_position_value_delete").ok()? };
        let az_style_background_position_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundPositionValue) -> AzStyleBackgroundPositionValue>(b"az_style_background_position_value_deep_copy").ok()? };
        let az_style_background_repeat_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundRepeatValue)>(b"az_style_background_repeat_value_delete").ok()? };
        let az_style_background_repeat_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundRepeatValue) -> AzStyleBackgroundRepeatValue>(b"az_style_background_repeat_value_deep_copy").ok()? };
        let az_style_background_size_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBackgroundSizeValue)>(b"az_style_background_size_value_delete").ok()? };
        let az_style_background_size_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBackgroundSizeValue) -> AzStyleBackgroundSizeValue>(b"az_style_background_size_value_deep_copy").ok()? };
        let az_style_border_bottom_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomColorValue)>(b"az_style_border_bottom_color_value_delete").ok()? };
        let az_style_border_bottom_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomColorValue) -> AzStyleBorderBottomColorValue>(b"az_style_border_bottom_color_value_deep_copy").ok()? };
        let az_style_border_bottom_left_radius_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomLeftRadiusValue)>(b"az_style_border_bottom_left_radius_value_delete").ok()? };
        let az_style_border_bottom_left_radius_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomLeftRadiusValue) -> AzStyleBorderBottomLeftRadiusValue>(b"az_style_border_bottom_left_radius_value_deep_copy").ok()? };
        let az_style_border_bottom_right_radius_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomRightRadiusValue)>(b"az_style_border_bottom_right_radius_value_delete").ok()? };
        let az_style_border_bottom_right_radius_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomRightRadiusValue) -> AzStyleBorderBottomRightRadiusValue>(b"az_style_border_bottom_right_radius_value_deep_copy").ok()? };
        let az_style_border_bottom_style_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomStyleValue)>(b"az_style_border_bottom_style_value_delete").ok()? };
        let az_style_border_bottom_style_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomStyleValue) -> AzStyleBorderBottomStyleValue>(b"az_style_border_bottom_style_value_deep_copy").ok()? };
        let az_style_border_bottom_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderBottomWidthValue)>(b"az_style_border_bottom_width_value_delete").ok()? };
        let az_style_border_bottom_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderBottomWidthValue) -> AzStyleBorderBottomWidthValue>(b"az_style_border_bottom_width_value_deep_copy").ok()? };
        let az_style_border_left_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftColorValue)>(b"az_style_border_left_color_value_delete").ok()? };
        let az_style_border_left_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftColorValue) -> AzStyleBorderLeftColorValue>(b"az_style_border_left_color_value_deep_copy").ok()? };
        let az_style_border_left_style_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftStyleValue)>(b"az_style_border_left_style_value_delete").ok()? };
        let az_style_border_left_style_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftStyleValue) -> AzStyleBorderLeftStyleValue>(b"az_style_border_left_style_value_deep_copy").ok()? };
        let az_style_border_left_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderLeftWidthValue)>(b"az_style_border_left_width_value_delete").ok()? };
        let az_style_border_left_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderLeftWidthValue) -> AzStyleBorderLeftWidthValue>(b"az_style_border_left_width_value_deep_copy").ok()? };
        let az_style_border_right_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightColorValue)>(b"az_style_border_right_color_value_delete").ok()? };
        let az_style_border_right_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightColorValue) -> AzStyleBorderRightColorValue>(b"az_style_border_right_color_value_deep_copy").ok()? };
        let az_style_border_right_style_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightStyleValue)>(b"az_style_border_right_style_value_delete").ok()? };
        let az_style_border_right_style_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightStyleValue) -> AzStyleBorderRightStyleValue>(b"az_style_border_right_style_value_deep_copy").ok()? };
        let az_style_border_right_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderRightWidthValue)>(b"az_style_border_right_width_value_delete").ok()? };
        let az_style_border_right_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderRightWidthValue) -> AzStyleBorderRightWidthValue>(b"az_style_border_right_width_value_deep_copy").ok()? };
        let az_style_border_top_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopColorValue)>(b"az_style_border_top_color_value_delete").ok()? };
        let az_style_border_top_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopColorValue) -> AzStyleBorderTopColorValue>(b"az_style_border_top_color_value_deep_copy").ok()? };
        let az_style_border_top_left_radius_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopLeftRadiusValue)>(b"az_style_border_top_left_radius_value_delete").ok()? };
        let az_style_border_top_left_radius_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopLeftRadiusValue) -> AzStyleBorderTopLeftRadiusValue>(b"az_style_border_top_left_radius_value_deep_copy").ok()? };
        let az_style_border_top_right_radius_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopRightRadiusValue)>(b"az_style_border_top_right_radius_value_delete").ok()? };
        let az_style_border_top_right_radius_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopRightRadiusValue) -> AzStyleBorderTopRightRadiusValue>(b"az_style_border_top_right_radius_value_deep_copy").ok()? };
        let az_style_border_top_style_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopStyleValue)>(b"az_style_border_top_style_value_delete").ok()? };
        let az_style_border_top_style_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopStyleValue) -> AzStyleBorderTopStyleValue>(b"az_style_border_top_style_value_deep_copy").ok()? };
        let az_style_border_top_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleBorderTopWidthValue)>(b"az_style_border_top_width_value_delete").ok()? };
        let az_style_border_top_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleBorderTopWidthValue) -> AzStyleBorderTopWidthValue>(b"az_style_border_top_width_value_deep_copy").ok()? };
        let az_style_cursor_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleCursorValue)>(b"az_style_cursor_value_delete").ok()? };
        let az_style_cursor_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleCursorValue) -> AzStyleCursorValue>(b"az_style_cursor_value_deep_copy").ok()? };
        let az_style_font_family_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleFontFamilyValue)>(b"az_style_font_family_value_delete").ok()? };
        let az_style_font_family_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleFontFamilyValue) -> AzStyleFontFamilyValue>(b"az_style_font_family_value_deep_copy").ok()? };
        let az_style_font_size_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleFontSizeValue)>(b"az_style_font_size_value_delete").ok()? };
        let az_style_font_size_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleFontSizeValue) -> AzStyleFontSizeValue>(b"az_style_font_size_value_deep_copy").ok()? };
        let az_style_letter_spacing_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleLetterSpacingValue)>(b"az_style_letter_spacing_value_delete").ok()? };
        let az_style_letter_spacing_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleLetterSpacingValue) -> AzStyleLetterSpacingValue>(b"az_style_letter_spacing_value_deep_copy").ok()? };
        let az_style_line_height_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleLineHeightValue)>(b"az_style_line_height_value_delete").ok()? };
        let az_style_line_height_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleLineHeightValue) -> AzStyleLineHeightValue>(b"az_style_line_height_value_deep_copy").ok()? };
        let az_style_tab_width_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTabWidthValue)>(b"az_style_tab_width_value_delete").ok()? };
        let az_style_tab_width_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTabWidthValue) -> AzStyleTabWidthValue>(b"az_style_tab_width_value_deep_copy").ok()? };
        let az_style_text_alignment_horz_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTextAlignmentHorzValue)>(b"az_style_text_alignment_horz_value_delete").ok()? };
        let az_style_text_alignment_horz_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTextAlignmentHorzValue) -> AzStyleTextAlignmentHorzValue>(b"az_style_text_alignment_horz_value_deep_copy").ok()? };
        let az_style_text_color_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleTextColorValue)>(b"az_style_text_color_value_delete").ok()? };
        let az_style_text_color_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleTextColorValue) -> AzStyleTextColorValue>(b"az_style_text_color_value_deep_copy").ok()? };
        let az_style_word_spacing_value_delete = unsafe { lib.get::<extern fn(_: &mut AzStyleWordSpacingValue)>(b"az_style_word_spacing_value_delete").ok()? };
        let az_style_word_spacing_value_deep_copy = unsafe { lib.get::<extern fn(_: &AzStyleWordSpacingValue) -> AzStyleWordSpacingValue>(b"az_style_word_spacing_value_deep_copy").ok()? };
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
        let az_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzEventFilter)>(b"az_event_filter_delete").ok()? };
        let az_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzEventFilter) -> AzEventFilter>(b"az_event_filter_deep_copy").ok()? };
        let az_hover_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzHoverEventFilter)>(b"az_hover_event_filter_delete").ok()? };
        let az_hover_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzHoverEventFilter) -> AzHoverEventFilter>(b"az_hover_event_filter_deep_copy").ok()? };
        let az_focus_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzFocusEventFilter)>(b"az_focus_event_filter_delete").ok()? };
        let az_focus_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzFocusEventFilter) -> AzFocusEventFilter>(b"az_focus_event_filter_deep_copy").ok()? };
        let az_not_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzNotEventFilter)>(b"az_not_event_filter_delete").ok()? };
        let az_not_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzNotEventFilter) -> AzNotEventFilter>(b"az_not_event_filter_deep_copy").ok()? };
        let az_window_event_filter_delete = unsafe { lib.get::<extern fn(_: &mut AzWindowEventFilter)>(b"az_window_event_filter_delete").ok()? };
        let az_window_event_filter_deep_copy = unsafe { lib.get::<extern fn(_: &AzWindowEventFilter) -> AzWindowEventFilter>(b"az_window_event_filter_deep_copy").ok()? };
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
        let az_image_source_delete = unsafe { lib.get::<extern fn(_: &mut AzImageSource)>(b"az_image_source_delete").ok()? };
        let az_image_source_deep_copy = unsafe { lib.get::<extern fn(_: &AzImageSource) -> AzImageSource>(b"az_image_source_deep_copy").ok()? };
        let az_font_source_delete = unsafe { lib.get::<extern fn(_: &mut AzFontSource)>(b"az_font_source_delete").ok()? };
        let az_font_source_deep_copy = unsafe { lib.get::<extern fn(_: &AzFontSource) -> AzFontSource>(b"az_font_source_deep_copy").ok()? };
        let az_raw_image_new = unsafe { lib.get::<extern fn(_: AzRawImageFormat) -> AzRawImage>(b"az_raw_image_new").ok()? };
        let az_raw_image_delete = unsafe { lib.get::<extern fn(_: &mut AzRawImage)>(b"az_raw_image_delete").ok()? };
        let az_raw_image_deep_copy = unsafe { lib.get::<extern fn(_: &AzRawImage) -> AzRawImage>(b"az_raw_image_deep_copy").ok()? };
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
            az_u8_vec_delete,
            az_u8_vec_deep_copy,
            az_string_vec_copy_from,
            az_string_vec_delete,
            az_string_vec_deep_copy,
            az_gradient_stop_pre_vec_copy_from,
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

    use crate::dll::*;

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
        pub fn from_utf8_unchecked(ptr: *const u8, len: usize) -> Self { az_string_from_utf8_unchecked(ptr, len) }
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn from_utf8_lossy(ptr: *const u8, len: usize) -> Self { az_string_from_utf8_lossy(ptr, len) }
        /// Creates + allocates a Rust `String` by **copying** it from another utf8-encoded string
        pub fn into_bytes(self)  -> crate::vec::U8Vec { { az_string_into_bytes(self.leak())} }
    }

    impl Drop for String { fn drop(&mut self) { az_string_delete(&mut self); } }
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


    /// Wrapper over a Rust-allocated `U8Vec`
    pub struct U8Vec { pub(crate) object: AzU8Vec }

    impl U8Vec {
    }

    impl Drop for U8Vec { fn drop(&mut self) { az_u8_vec_delete(&mut self); } }


    /// Wrapper over a Rust-allocated `StringVec`
    pub struct StringVec { pub(crate) object: AzStringVec }

    impl StringVec {
        /// Creates + allocates a Rust `Vec<String>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzString, len: usize) -> Self { az_string_vec_copy_from(ptr, len) }
    }

    impl Drop for StringVec { fn drop(&mut self) { az_string_vec_delete(&mut self); } }


    /// Wrapper over a Rust-allocated `GradientStopPreVec`
    pub struct GradientStopPreVec { pub(crate) object: AzGradientStopPreVec }

    impl GradientStopPreVec {
        /// Creates + allocates a Rust `Vec<GradientStopPre>` by **copying** it from a bytes source
        pub fn copy_from(ptr: *const AzGradientStopPre, len: usize) -> Self { az_gradient_stop_pre_vec_copy_from(ptr, len) }
    }

    impl Drop for GradientStopPreVec { fn drop(&mut self) { az_gradient_stop_pre_vec_delete(&mut self); } }
}

/// Definition of azuls internal `Option<*>` wrappers
#[allow(dead_code, unused_imports)]
pub mod option {

    use crate::dll::*;


    /// `OptionPercentageValue` struct
    pub struct OptionPercentageValue { pub(crate) object: AzOptionPercentageValue }

    impl OptionPercentageValue {
        pub fn none() -> Self { az_option_percentage_value_none()  }
        pub fn some(variant_data: crate::css::PercentageValue) -> Self { az_option_percentage_value_some(variant_data.leak())}
    }

    impl Drop for OptionPercentageValue { fn drop(&mut self) { az_option_percentage_value_delete(&mut self); } }
}

/// `App` construction and configuration
#[allow(dead_code, unused_imports)]
pub mod app {

    use crate::dll::*;
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

    use crate::dll::*;


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


    #[no_mangle]
    #[repr(C)]
    pub struct RefAny {
        pub _internal_ptr: *const c_void,
        pub _internal_len: usize,
        pub _internal_layout_size: usize,
        pub _internal_layout_align: usize,
        pub type_id: u64,
        pub type_name: AzString,
        pub strong_count: usize,
        pub is_currently_mutable: bool,
        pub custom_destructor: fn(RefAny),
    }

    impl Clone for RefAny {
        fn clone(&self) -> Self {
            RefAny(az_ref_any_shallow_copy(&self))
        }
    }

    impl RefAny {

        /// Creates a new, type-erased pointer by casting the `T` value into a `Vec<u8>` and saving the length + type ID
        pub fn new<T: 'static>(value: T) -> Self {
            use azul_dll::*;

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

        /// Returns the inner `RefAny`
        pub fn leak(self) -> RefAny {
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

    use crate::dll::*;
    use crate::str::String;


    /// `Css` struct
    pub struct Css { pub(crate) ptr: AzCssPtr }

    impl Css {
        /// Loads the native style for the given operating system
        pub fn native() -> Self { Self { ptr: az_css_native() } }
        /// Returns an empty CSS style
        pub fn empty() -> Self { Self { ptr: az_css_empty() } }
        /// Returns a CSS style parsed from a `String`
        pub fn from_string(s: String) -> Self { Self { ptr: az_css_from_string(s) } }
        /// Appends a parsed stylesheet to `Css::native()`
        pub fn override_native(s: String) -> Self { Self { ptr: az_css_override_native(s) } }
       /// Prevents the destructor from running and returns the internal `AzCssPtr`
       pub fn leak(self) -> AzCssPtr { let p = az_css_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for Css { fn drop(&mut self) { az_css_delete(&mut self.ptr); } }


    /// `CssHotReloader` struct
    pub struct CssHotReloader { pub(crate) ptr: AzCssHotReloaderPtr }

    impl CssHotReloader {
        /// Creates a `HotReloadHandler` that hot-reloads a CSS file every X milliseconds
        pub fn new(path: String, reload_ms: u64) -> Self { Self { ptr: az_css_hot_reloader_new(path, reload_ms) } }
        /// Creates a `HotReloadHandler` that overrides the `Css::native()` stylesheet with a CSS file, reloaded every X milliseconds
        pub fn override_native(path: String, reload_ms: u64) -> Self { Self { ptr: az_css_hot_reloader_override_native(path, reload_ms) } }
       /// Prevents the destructor from running and returns the internal `AzCssHotReloaderPtr`
       pub fn leak(self) -> AzCssHotReloaderPtr { let p = az_css_hot_reloader_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for CssHotReloader { fn drop(&mut self) { az_css_hot_reloader_delete(&mut self.ptr); } }


    /// `ColorU` struct
    pub struct ColorU { pub(crate) object: AzColorU }

    impl ColorU {
    }

    impl Drop for ColorU { fn drop(&mut self) { az_color_u_delete(&mut self); } }


    /// `SizeMetric` struct
    pub struct SizeMetric { pub(crate) object: AzSizeMetric }

    impl SizeMetric {
        pub fn px() -> Self { az_size_metric_px()  }
        pub fn pt() -> Self { az_size_metric_pt()  }
        pub fn em() -> Self { az_size_metric_em()  }
        pub fn percent() -> Self { az_size_metric_percent()  }
    }

    impl Drop for SizeMetric { fn drop(&mut self) { az_size_metric_delete(&mut self); } }


    /// `FloatValue` struct
    pub struct FloatValue { pub(crate) object: AzFloatValue }

    impl FloatValue {
    }

    impl Drop for FloatValue { fn drop(&mut self) { az_float_value_delete(&mut self); } }


    /// `PixelValue` struct
    pub struct PixelValue { pub(crate) object: AzPixelValue }

    impl PixelValue {
    }

    impl Drop for PixelValue { fn drop(&mut self) { az_pixel_value_delete(&mut self); } }


    /// `PixelValueNoPercent` struct
    pub struct PixelValueNoPercent { pub(crate) object: AzPixelValueNoPercent }

    impl PixelValueNoPercent {
    }

    impl Drop for PixelValueNoPercent { fn drop(&mut self) { az_pixel_value_no_percent_delete(&mut self); } }


    /// `BoxShadowClipMode` struct
    pub struct BoxShadowClipMode { pub(crate) object: AzBoxShadowClipMode }

    impl BoxShadowClipMode {
        pub fn outset() -> Self { az_box_shadow_clip_mode_outset()  }
        pub fn inset() -> Self { az_box_shadow_clip_mode_inset()  }
    }

    impl Drop for BoxShadowClipMode { fn drop(&mut self) { az_box_shadow_clip_mode_delete(&mut self); } }


    /// `BoxShadowPreDisplayItem` struct
    pub struct BoxShadowPreDisplayItem { pub(crate) object: AzBoxShadowPreDisplayItem }

    impl BoxShadowPreDisplayItem {
    }

    impl Drop for BoxShadowPreDisplayItem { fn drop(&mut self) { az_box_shadow_pre_display_item_delete(&mut self); } }


    /// `LayoutAlignContent` struct
    pub struct LayoutAlignContent { pub(crate) object: AzLayoutAlignContent }

    impl LayoutAlignContent {
        /// Default value. Lines stretch to take up the remaining space
        pub fn stretch() -> Self { az_layout_align_content_stretch()  }
        /// Lines are packed toward the center of the flex container
        pub fn center() -> Self { az_layout_align_content_center()  }
        /// Lines are packed toward the start of the flex container
        pub fn start() -> Self { az_layout_align_content_start()  }
        /// Lines are packed toward the end of the flex container
        pub fn end() -> Self { az_layout_align_content_end()  }
        /// Lines are evenly distributed in the flex container
        pub fn space_between() -> Self { az_layout_align_content_space_between()  }
        /// Lines are evenly distributed in the flex container, with half-size spaces on either end
        pub fn space_around() -> Self { az_layout_align_content_space_around()  }
    }

    impl Drop for LayoutAlignContent { fn drop(&mut self) { az_layout_align_content_delete(&mut self); } }


    /// `LayoutAlignItems` struct
    pub struct LayoutAlignItems { pub(crate) object: AzLayoutAlignItems }

    impl LayoutAlignItems {
        /// Items are stretched to fit the container
        pub fn stretch() -> Self { az_layout_align_items_stretch()  }
        /// Items are positioned at the center of the container
        pub fn center() -> Self { az_layout_align_items_center()  }
        /// Items are positioned at the beginning of the container
        pub fn start() -> Self { az_layout_align_items_start()  }
        /// Items are positioned at the end of the container
        pub fn end() -> Self { az_layout_align_items_end()  }
    }

    impl Drop for LayoutAlignItems { fn drop(&mut self) { az_layout_align_items_delete(&mut self); } }


    /// `LayoutBottom` struct
    pub struct LayoutBottom { pub(crate) object: AzLayoutBottom }

    impl LayoutBottom {
    }

    impl Drop for LayoutBottom { fn drop(&mut self) { az_layout_bottom_delete(&mut self); } }


    /// `LayoutBoxSizing` struct
    pub struct LayoutBoxSizing { pub(crate) object: AzLayoutBoxSizing }

    impl LayoutBoxSizing {
        pub fn content_box() -> Self { az_layout_box_sizing_content_box()  }
        pub fn border_box() -> Self { az_layout_box_sizing_border_box()  }
    }

    impl Drop for LayoutBoxSizing { fn drop(&mut self) { az_layout_box_sizing_delete(&mut self); } }


    /// `LayoutDirection` struct
    pub struct LayoutDirection { pub(crate) object: AzLayoutDirection }

    impl LayoutDirection {
        pub fn row() -> Self { az_layout_direction_row()  }
        pub fn row_reverse() -> Self { az_layout_direction_row_reverse()  }
        pub fn column() -> Self { az_layout_direction_column()  }
        pub fn column_reverse() -> Self { az_layout_direction_column_reverse()  }
    }

    impl Drop for LayoutDirection { fn drop(&mut self) { az_layout_direction_delete(&mut self); } }


    /// `LayoutDisplay` struct
    pub struct LayoutDisplay { pub(crate) object: AzLayoutDisplay }

    impl LayoutDisplay {
        pub fn flex() -> Self { az_layout_display_flex()  }
        pub fn block() -> Self { az_layout_display_block()  }
        pub fn inline_block() -> Self { az_layout_display_inline_block()  }
    }

    impl Drop for LayoutDisplay { fn drop(&mut self) { az_layout_display_delete(&mut self); } }


    /// `LayoutFlexGrow` struct
    pub struct LayoutFlexGrow { pub(crate) object: AzLayoutFlexGrow }

    impl LayoutFlexGrow {
    }

    impl Drop for LayoutFlexGrow { fn drop(&mut self) { az_layout_flex_grow_delete(&mut self); } }


    /// `LayoutFlexShrink` struct
    pub struct LayoutFlexShrink { pub(crate) object: AzLayoutFlexShrink }

    impl LayoutFlexShrink {
    }

    impl Drop for LayoutFlexShrink { fn drop(&mut self) { az_layout_flex_shrink_delete(&mut self); } }


    /// `LayoutFloat` struct
    pub struct LayoutFloat { pub(crate) object: AzLayoutFloat }

    impl LayoutFloat {
        pub fn left() -> Self { az_layout_float_left()  }
        pub fn right() -> Self { az_layout_float_right()  }
    }

    impl Drop for LayoutFloat { fn drop(&mut self) { az_layout_float_delete(&mut self); } }


    /// `LayoutHeight` struct
    pub struct LayoutHeight { pub(crate) object: AzLayoutHeight }

    impl LayoutHeight {
    }

    impl Drop for LayoutHeight { fn drop(&mut self) { az_layout_height_delete(&mut self); } }


    /// `LayoutJustifyContent` struct
    pub struct LayoutJustifyContent { pub(crate) object: AzLayoutJustifyContent }

    impl LayoutJustifyContent {
        /// Default value. Items are positioned at the beginning of the container
        pub fn start() -> Self { az_layout_justify_content_start()  }
        /// Items are positioned at the end of the container
        pub fn end() -> Self { az_layout_justify_content_end()  }
        /// Items are positioned at the center of the container
        pub fn center() -> Self { az_layout_justify_content_center()  }
        /// Items are positioned with space between the lines
        pub fn space_between() -> Self { az_layout_justify_content_space_between()  }
        /// Items are positioned with space before, between, and after the lines
        pub fn space_around() -> Self { az_layout_justify_content_space_around()  }
        /// Items are distributed so that the spacing between any two adjacent alignment subjects, before the first alignment subject, and after the last alignment subject is the same
        pub fn space_evenly() -> Self { az_layout_justify_content_space_evenly()  }
    }

    impl Drop for LayoutJustifyContent { fn drop(&mut self) { az_layout_justify_content_delete(&mut self); } }


    /// `LayoutLeft` struct
    pub struct LayoutLeft { pub(crate) object: AzLayoutLeft }

    impl LayoutLeft {
    }

    impl Drop for LayoutLeft { fn drop(&mut self) { az_layout_left_delete(&mut self); } }


    /// `LayoutMarginBottom` struct
    pub struct LayoutMarginBottom { pub(crate) object: AzLayoutMarginBottom }

    impl LayoutMarginBottom {
    }

    impl Drop for LayoutMarginBottom { fn drop(&mut self) { az_layout_margin_bottom_delete(&mut self); } }


    /// `LayoutMarginLeft` struct
    pub struct LayoutMarginLeft { pub(crate) object: AzLayoutMarginLeft }

    impl LayoutMarginLeft {
    }

    impl Drop for LayoutMarginLeft { fn drop(&mut self) { az_layout_margin_left_delete(&mut self); } }


    /// `LayoutMarginRight` struct
    pub struct LayoutMarginRight { pub(crate) object: AzLayoutMarginRight }

    impl LayoutMarginRight {
    }

    impl Drop for LayoutMarginRight { fn drop(&mut self) { az_layout_margin_right_delete(&mut self); } }


    /// `LayoutMarginTop` struct
    pub struct LayoutMarginTop { pub(crate) object: AzLayoutMarginTop }

    impl LayoutMarginTop {
    }

    impl Drop for LayoutMarginTop { fn drop(&mut self) { az_layout_margin_top_delete(&mut self); } }


    /// `LayoutMaxHeight` struct
    pub struct LayoutMaxHeight { pub(crate) object: AzLayoutMaxHeight }

    impl LayoutMaxHeight {
    }

    impl Drop for LayoutMaxHeight { fn drop(&mut self) { az_layout_max_height_delete(&mut self); } }


    /// `LayoutMaxWidth` struct
    pub struct LayoutMaxWidth { pub(crate) object: AzLayoutMaxWidth }

    impl LayoutMaxWidth {
    }

    impl Drop for LayoutMaxWidth { fn drop(&mut self) { az_layout_max_width_delete(&mut self); } }


    /// `LayoutMinHeight` struct
    pub struct LayoutMinHeight { pub(crate) object: AzLayoutMinHeight }

    impl LayoutMinHeight {
    }

    impl Drop for LayoutMinHeight { fn drop(&mut self) { az_layout_min_height_delete(&mut self); } }


    /// `LayoutMinWidth` struct
    pub struct LayoutMinWidth { pub(crate) object: AzLayoutMinWidth }

    impl LayoutMinWidth {
    }

    impl Drop for LayoutMinWidth { fn drop(&mut self) { az_layout_min_width_delete(&mut self); } }


    /// `LayoutPaddingBottom` struct
    pub struct LayoutPaddingBottom { pub(crate) object: AzLayoutPaddingBottom }

    impl LayoutPaddingBottom {
    }

    impl Drop for LayoutPaddingBottom { fn drop(&mut self) { az_layout_padding_bottom_delete(&mut self); } }


    /// `LayoutPaddingLeft` struct
    pub struct LayoutPaddingLeft { pub(crate) object: AzLayoutPaddingLeft }

    impl LayoutPaddingLeft {
    }

    impl Drop for LayoutPaddingLeft { fn drop(&mut self) { az_layout_padding_left_delete(&mut self); } }


    /// `LayoutPaddingRight` struct
    pub struct LayoutPaddingRight { pub(crate) object: AzLayoutPaddingRight }

    impl LayoutPaddingRight {
    }

    impl Drop for LayoutPaddingRight { fn drop(&mut self) { az_layout_padding_right_delete(&mut self); } }


    /// `LayoutPaddingTop` struct
    pub struct LayoutPaddingTop { pub(crate) object: AzLayoutPaddingTop }

    impl LayoutPaddingTop {
    }

    impl Drop for LayoutPaddingTop { fn drop(&mut self) { az_layout_padding_top_delete(&mut self); } }


    /// `LayoutPosition` struct
    pub struct LayoutPosition { pub(crate) object: AzLayoutPosition }

    impl LayoutPosition {
        pub fn static() -> Self { az_layout_position_static()  }
        pub fn relative() -> Self { az_layout_position_relative()  }
        pub fn absolute() -> Self { az_layout_position_absolute()  }
        pub fn fixed() -> Self { az_layout_position_fixed()  }
    }

    impl Drop for LayoutPosition { fn drop(&mut self) { az_layout_position_delete(&mut self); } }


    /// `LayoutRight` struct
    pub struct LayoutRight { pub(crate) object: AzLayoutRight }

    impl LayoutRight {
    }

    impl Drop for LayoutRight { fn drop(&mut self) { az_layout_right_delete(&mut self); } }


    /// `LayoutTop` struct
    pub struct LayoutTop { pub(crate) object: AzLayoutTop }

    impl LayoutTop {
    }

    impl Drop for LayoutTop { fn drop(&mut self) { az_layout_top_delete(&mut self); } }


    /// `LayoutWidth` struct
    pub struct LayoutWidth { pub(crate) object: AzLayoutWidth }

    impl LayoutWidth {
    }

    impl Drop for LayoutWidth { fn drop(&mut self) { az_layout_width_delete(&mut self); } }


    /// `LayoutWrap` struct
    pub struct LayoutWrap { pub(crate) object: AzLayoutWrap }

    impl LayoutWrap {
        pub fn wrap() -> Self { az_layout_wrap_wrap()  }
        pub fn no_wrap() -> Self { az_layout_wrap_no_wrap()  }
    }

    impl Drop for LayoutWrap { fn drop(&mut self) { az_layout_wrap_delete(&mut self); } }


    /// `Overflow` struct
    pub struct Overflow { pub(crate) object: AzOverflow }

    impl Overflow {
        /// Always shows a scroll bar, overflows on scroll
        pub fn scroll() -> Self { az_overflow_scroll()  }
        /// Does not show a scroll bar by default, only when text is overflowing
        pub fn auto() -> Self { az_overflow_auto()  }
        /// Never shows a scroll bar, simply clips text
        pub fn hidden() -> Self { az_overflow_hidden()  }
        /// Doesn't show a scroll bar, simply overflows the text
        pub fn visible() -> Self { az_overflow_visible()  }
    }

    impl Drop for Overflow { fn drop(&mut self) { az_overflow_delete(&mut self); } }


    /// `PercentageValue` struct
    pub struct PercentageValue { pub(crate) object: AzPercentageValue }

    impl PercentageValue {
    }

    impl Drop for PercentageValue { fn drop(&mut self) { az_percentage_value_delete(&mut self); } }


    /// `GradientStopPre` struct
    pub struct GradientStopPre { pub(crate) object: AzGradientStopPre }

    impl GradientStopPre {
    }

    impl Drop for GradientStopPre { fn drop(&mut self) { az_gradient_stop_pre_delete(&mut self); } }


    /// `DirectionCorner` struct
    pub struct DirectionCorner { pub(crate) object: AzDirectionCorner }

    impl DirectionCorner {
        pub fn right() -> Self { az_direction_corner_right()  }
        pub fn left() -> Self { az_direction_corner_left()  }
        pub fn top() -> Self { az_direction_corner_top()  }
        pub fn bottom() -> Self { az_direction_corner_bottom()  }
        pub fn top_right() -> Self { az_direction_corner_top_right()  }
        pub fn top_left() -> Self { az_direction_corner_top_left()  }
        pub fn bottom_right() -> Self { az_direction_corner_bottom_right()  }
        pub fn bottom_left() -> Self { az_direction_corner_bottom_left()  }
    }

    impl Drop for DirectionCorner { fn drop(&mut self) { az_direction_corner_delete(&mut self); } }


    /// `DirectionCorners` struct
    pub struct DirectionCorners { pub(crate) object: AzDirectionCorners }

    impl DirectionCorners {
    }

    impl Drop for DirectionCorners { fn drop(&mut self) { az_direction_corners_delete(&mut self); } }


    /// `Direction` struct
    pub struct Direction { pub(crate) object: AzDirection }

    impl Direction {
        pub fn angle(variant_data: crate::css::FloatValue) -> Self { az_direction_angle(variant_data.leak())}
        pub fn from_to(variant_data: crate::css::DirectionCorners) -> Self { az_direction_from_to(variant_data.leak())}
    }

    impl Drop for Direction { fn drop(&mut self) { az_direction_delete(&mut self); } }


    /// `ExtendMode` struct
    pub struct ExtendMode { pub(crate) object: AzExtendMode }

    impl ExtendMode {
        pub fn clamp() -> Self { az_extend_mode_clamp()  }
        pub fn repeat() -> Self { az_extend_mode_repeat()  }
    }

    impl Drop for ExtendMode { fn drop(&mut self) { az_extend_mode_delete(&mut self); } }


    /// `LinearGradient` struct
    pub struct LinearGradient { pub(crate) object: AzLinearGradient }

    impl LinearGradient {
    }

    impl Drop for LinearGradient { fn drop(&mut self) { az_linear_gradient_delete(&mut self); } }


    /// `Shape` struct
    pub struct Shape { pub(crate) object: AzShape }

    impl Shape {
        pub fn ellipse() -> Self { az_shape_ellipse()  }
        pub fn circle() -> Self { az_shape_circle()  }
    }

    impl Drop for Shape { fn drop(&mut self) { az_shape_delete(&mut self); } }


    /// `RadialGradient` struct
    pub struct RadialGradient { pub(crate) object: AzRadialGradient }

    impl RadialGradient {
    }

    impl Drop for RadialGradient { fn drop(&mut self) { az_radial_gradient_delete(&mut self); } }


    /// `CssImageId` struct
    pub struct CssImageId { pub(crate) object: AzCssImageId }

    impl CssImageId {
    }

    impl Drop for CssImageId { fn drop(&mut self) { az_css_image_id_delete(&mut self); } }


    /// `StyleBackgroundContent` struct
    pub struct StyleBackgroundContent { pub(crate) object: AzStyleBackgroundContent }

    impl StyleBackgroundContent {
        pub fn linear_gradient(variant_data: crate::css::LinearGradient) -> Self { az_style_background_content_linear_gradient(variant_data.leak())}
        pub fn radial_gradient(variant_data: crate::css::RadialGradient) -> Self { az_style_background_content_radial_gradient(variant_data.leak())}
        pub fn image(variant_data: crate::css::CssImageId) -> Self { az_style_background_content_image(variant_data.leak())}
        pub fn color(variant_data: crate::css::ColorU) -> Self { az_style_background_content_color(variant_data.leak())}
    }

    impl Drop for StyleBackgroundContent { fn drop(&mut self) { az_style_background_content_delete(&mut self); } }


    /// `BackgroundPositionHorizontal` struct
    pub struct BackgroundPositionHorizontal { pub(crate) object: AzBackgroundPositionHorizontal }

    impl BackgroundPositionHorizontal {
        pub fn left() -> Self { az_background_position_horizontal_left()  }
        pub fn center() -> Self { az_background_position_horizontal_center()  }
        pub fn right() -> Self { az_background_position_horizontal_right()  }
        pub fn exact(variant_data: crate::css::PixelValue) -> Self { az_background_position_horizontal_exact(variant_data.leak())}
    }

    impl Drop for BackgroundPositionHorizontal { fn drop(&mut self) { az_background_position_horizontal_delete(&mut self); } }


    /// `BackgroundPositionVertical` struct
    pub struct BackgroundPositionVertical { pub(crate) object: AzBackgroundPositionVertical }

    impl BackgroundPositionVertical {
        pub fn top() -> Self { az_background_position_vertical_top()  }
        pub fn center() -> Self { az_background_position_vertical_center()  }
        pub fn bottom() -> Self { az_background_position_vertical_bottom()  }
        pub fn exact(variant_data: crate::css::PixelValue) -> Self { az_background_position_vertical_exact(variant_data.leak())}
    }

    impl Drop for BackgroundPositionVertical { fn drop(&mut self) { az_background_position_vertical_delete(&mut self); } }


    /// `StyleBackgroundPosition` struct
    pub struct StyleBackgroundPosition { pub(crate) object: AzStyleBackgroundPosition }

    impl StyleBackgroundPosition {
    }

    impl Drop for StyleBackgroundPosition { fn drop(&mut self) { az_style_background_position_delete(&mut self); } }


    /// `StyleBackgroundRepeat` struct
    pub struct StyleBackgroundRepeat { pub(crate) object: AzStyleBackgroundRepeat }

    impl StyleBackgroundRepeat {
        pub fn no_repeat() -> Self { az_style_background_repeat_no_repeat()  }
        pub fn repeat() -> Self { az_style_background_repeat_repeat()  }
        pub fn repeat_x() -> Self { az_style_background_repeat_repeat_x()  }
        pub fn repeat_y() -> Self { az_style_background_repeat_repeat_y()  }
    }

    impl Drop for StyleBackgroundRepeat { fn drop(&mut self) { az_style_background_repeat_delete(&mut self); } }


    /// `StyleBackgroundSize` struct
    pub struct StyleBackgroundSize { pub(crate) object: AzStyleBackgroundSize }

    impl StyleBackgroundSize {
        pub fn exact_size(variant_data: [crate::css::PixelValue;2]) -> Self { az_style_background_size_exact_size(variant_data.leak())}
        pub fn contain() -> Self { az_style_background_size_contain()  }
        pub fn cover() -> Self { az_style_background_size_cover()  }
    }

    impl Drop for StyleBackgroundSize { fn drop(&mut self) { az_style_background_size_delete(&mut self); } }


    /// `StyleBorderBottomColor` struct
    pub struct StyleBorderBottomColor { pub(crate) object: AzStyleBorderBottomColor }

    impl StyleBorderBottomColor {
    }

    impl Drop for StyleBorderBottomColor { fn drop(&mut self) { az_style_border_bottom_color_delete(&mut self); } }


    /// `StyleBorderBottomLeftRadius` struct
    pub struct StyleBorderBottomLeftRadius { pub(crate) object: AzStyleBorderBottomLeftRadius }

    impl StyleBorderBottomLeftRadius {
    }

    impl Drop for StyleBorderBottomLeftRadius { fn drop(&mut self) { az_style_border_bottom_left_radius_delete(&mut self); } }


    /// `StyleBorderBottomRightRadius` struct
    pub struct StyleBorderBottomRightRadius { pub(crate) object: AzStyleBorderBottomRightRadius }

    impl StyleBorderBottomRightRadius {
    }

    impl Drop for StyleBorderBottomRightRadius { fn drop(&mut self) { az_style_border_bottom_right_radius_delete(&mut self); } }


    /// `BorderStyle` struct
    pub struct BorderStyle { pub(crate) object: AzBorderStyle }

    impl BorderStyle {
        pub fn none() -> Self { az_border_style_none()  }
        pub fn solid() -> Self { az_border_style_solid()  }
        pub fn double() -> Self { az_border_style_double()  }
        pub fn dotted() -> Self { az_border_style_dotted()  }
        pub fn dashed() -> Self { az_border_style_dashed()  }
        pub fn hidden() -> Self { az_border_style_hidden()  }
        pub fn groove() -> Self { az_border_style_groove()  }
        pub fn ridge() -> Self { az_border_style_ridge()  }
        pub fn inset() -> Self { az_border_style_inset()  }
        pub fn outset() -> Self { az_border_style_outset()  }
    }

    impl Drop for BorderStyle { fn drop(&mut self) { az_border_style_delete(&mut self); } }


    /// `StyleBorderBottomStyle` struct
    pub struct StyleBorderBottomStyle { pub(crate) object: AzStyleBorderBottomStyle }

    impl StyleBorderBottomStyle {
    }

    impl Drop for StyleBorderBottomStyle { fn drop(&mut self) { az_style_border_bottom_style_delete(&mut self); } }


    /// `StyleBorderBottomWidth` struct
    pub struct StyleBorderBottomWidth { pub(crate) object: AzStyleBorderBottomWidth }

    impl StyleBorderBottomWidth {
    }

    impl Drop for StyleBorderBottomWidth { fn drop(&mut self) { az_style_border_bottom_width_delete(&mut self); } }


    /// `StyleBorderLeftColor` struct
    pub struct StyleBorderLeftColor { pub(crate) object: AzStyleBorderLeftColor }

    impl StyleBorderLeftColor {
    }

    impl Drop for StyleBorderLeftColor { fn drop(&mut self) { az_style_border_left_color_delete(&mut self); } }


    /// `StyleBorderLeftStyle` struct
    pub struct StyleBorderLeftStyle { pub(crate) object: AzStyleBorderLeftStyle }

    impl StyleBorderLeftStyle {
    }

    impl Drop for StyleBorderLeftStyle { fn drop(&mut self) { az_style_border_left_style_delete(&mut self); } }


    /// `StyleBorderLeftWidth` struct
    pub struct StyleBorderLeftWidth { pub(crate) object: AzStyleBorderLeftWidth }

    impl StyleBorderLeftWidth {
    }

    impl Drop for StyleBorderLeftWidth { fn drop(&mut self) { az_style_border_left_width_delete(&mut self); } }


    /// `StyleBorderRightColor` struct
    pub struct StyleBorderRightColor { pub(crate) object: AzStyleBorderRightColor }

    impl StyleBorderRightColor {
    }

    impl Drop for StyleBorderRightColor { fn drop(&mut self) { az_style_border_right_color_delete(&mut self); } }


    /// `StyleBorderRightStyle` struct
    pub struct StyleBorderRightStyle { pub(crate) object: AzStyleBorderRightStyle }

    impl StyleBorderRightStyle {
    }

    impl Drop for StyleBorderRightStyle { fn drop(&mut self) { az_style_border_right_style_delete(&mut self); } }


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
    }

    impl Drop for StyleBorderTopColor { fn drop(&mut self) { az_style_border_top_color_delete(&mut self); } }


    /// `StyleBorderTopLeftRadius` struct
    pub struct StyleBorderTopLeftRadius { pub(crate) object: AzStyleBorderTopLeftRadius }

    impl StyleBorderTopLeftRadius {
    }

    impl Drop for StyleBorderTopLeftRadius { fn drop(&mut self) { az_style_border_top_left_radius_delete(&mut self); } }


    /// `StyleBorderTopRightRadius` struct
    pub struct StyleBorderTopRightRadius { pub(crate) object: AzStyleBorderTopRightRadius }

    impl StyleBorderTopRightRadius {
    }

    impl Drop for StyleBorderTopRightRadius { fn drop(&mut self) { az_style_border_top_right_radius_delete(&mut self); } }


    /// `StyleBorderTopStyle` struct
    pub struct StyleBorderTopStyle { pub(crate) object: AzStyleBorderTopStyle }

    impl StyleBorderTopStyle {
    }

    impl Drop for StyleBorderTopStyle { fn drop(&mut self) { az_style_border_top_style_delete(&mut self); } }


    /// `StyleBorderTopWidth` struct
    pub struct StyleBorderTopWidth { pub(crate) object: AzStyleBorderTopWidth }

    impl StyleBorderTopWidth {
    }

    impl Drop for StyleBorderTopWidth { fn drop(&mut self) { az_style_border_top_width_delete(&mut self); } }


    /// `StyleCursor` struct
    pub struct StyleCursor { pub(crate) object: AzStyleCursor }

    impl StyleCursor {
        pub fn alias() -> Self { az_style_cursor_alias()  }
        pub fn all_scroll() -> Self { az_style_cursor_all_scroll()  }
        pub fn cell() -> Self { az_style_cursor_cell()  }
        pub fn col_resize() -> Self { az_style_cursor_col_resize()  }
        pub fn context_menu() -> Self { az_style_cursor_context_menu()  }
        pub fn copy() -> Self { az_style_cursor_copy()  }
        pub fn crosshair() -> Self { az_style_cursor_crosshair()  }
        pub fn default() -> Self { az_style_cursor_default()  }
        pub fn e_resize() -> Self { az_style_cursor_e_resize()  }
        pub fn ew_resize() -> Self { az_style_cursor_ew_resize()  }
        pub fn grab() -> Self { az_style_cursor_grab()  }
        pub fn grabbing() -> Self { az_style_cursor_grabbing()  }
        pub fn help() -> Self { az_style_cursor_help()  }
        pub fn move() -> Self { az_style_cursor_move()  }
        pub fn n_resize() -> Self { az_style_cursor_n_resize()  }
        pub fn ns_resize() -> Self { az_style_cursor_ns_resize()  }
        pub fn nesw_resize() -> Self { az_style_cursor_nesw_resize()  }
        pub fn nwse_resize() -> Self { az_style_cursor_nwse_resize()  }
        pub fn pointer() -> Self { az_style_cursor_pointer()  }
        pub fn progress() -> Self { az_style_cursor_progress()  }
        pub fn row_resize() -> Self { az_style_cursor_row_resize()  }
        pub fn s_resize() -> Self { az_style_cursor_s_resize()  }
        pub fn se_resize() -> Self { az_style_cursor_se_resize()  }
        pub fn text() -> Self { az_style_cursor_text()  }
        pub fn unset() -> Self { az_style_cursor_unset()  }
        pub fn vertical_text() -> Self { az_style_cursor_vertical_text()  }
        pub fn w_resize() -> Self { az_style_cursor_w_resize()  }
        pub fn wait() -> Self { az_style_cursor_wait()  }
        pub fn zoom_in() -> Self { az_style_cursor_zoom_in()  }
        pub fn zoom_out() -> Self { az_style_cursor_zoom_out()  }
    }

    impl Drop for StyleCursor { fn drop(&mut self) { az_style_cursor_delete(&mut self); } }


    /// `StyleFontFamily` struct
    pub struct StyleFontFamily { pub(crate) object: AzStyleFontFamily }

    impl StyleFontFamily {
    }

    impl Drop for StyleFontFamily { fn drop(&mut self) { az_style_font_family_delete(&mut self); } }


    /// `StyleFontSize` struct
    pub struct StyleFontSize { pub(crate) object: AzStyleFontSize }

    impl StyleFontSize {
    }

    impl Drop for StyleFontSize { fn drop(&mut self) { az_style_font_size_delete(&mut self); } }


    /// `StyleLetterSpacing` struct
    pub struct StyleLetterSpacing { pub(crate) object: AzStyleLetterSpacing }

    impl StyleLetterSpacing {
    }

    impl Drop for StyleLetterSpacing { fn drop(&mut self) { az_style_letter_spacing_delete(&mut self); } }


    /// `StyleLineHeight` struct
    pub struct StyleLineHeight { pub(crate) object: AzStyleLineHeight }

    impl StyleLineHeight {
    }

    impl Drop for StyleLineHeight { fn drop(&mut self) { az_style_line_height_delete(&mut self); } }


    /// `StyleTabWidth` struct
    pub struct StyleTabWidth { pub(crate) object: AzStyleTabWidth }

    impl StyleTabWidth {
    }

    impl Drop for StyleTabWidth { fn drop(&mut self) { az_style_tab_width_delete(&mut self); } }


    /// `StyleTextAlignmentHorz` struct
    pub struct StyleTextAlignmentHorz { pub(crate) object: AzStyleTextAlignmentHorz }

    impl StyleTextAlignmentHorz {
        pub fn left() -> Self { az_style_text_alignment_horz_left()  }
        pub fn center() -> Self { az_style_text_alignment_horz_center()  }
        pub fn right() -> Self { az_style_text_alignment_horz_right()  }
    }

    impl Drop for StyleTextAlignmentHorz { fn drop(&mut self) { az_style_text_alignment_horz_delete(&mut self); } }


    /// `StyleTextColor` struct
    pub struct StyleTextColor { pub(crate) object: AzStyleTextColor }

    impl StyleTextColor {
    }

    impl Drop for StyleTextColor { fn drop(&mut self) { az_style_text_color_delete(&mut self); } }


    /// `StyleWordSpacing` struct
    pub struct StyleWordSpacing { pub(crate) object: AzStyleWordSpacing }

    impl StyleWordSpacing {
    }

    impl Drop for StyleWordSpacing { fn drop(&mut self) { az_style_word_spacing_delete(&mut self); } }


    /// `BoxShadowPreDisplayItemValue` struct
    pub struct BoxShadowPreDisplayItemValue { pub(crate) object: AzBoxShadowPreDisplayItemValue }

    impl BoxShadowPreDisplayItemValue {
        pub fn auto() -> Self { az_box_shadow_pre_display_item_value_auto()  }
        pub fn none() -> Self { az_box_shadow_pre_display_item_value_none()  }
        pub fn inherit() -> Self { az_box_shadow_pre_display_item_value_inherit()  }
        pub fn initial() -> Self { az_box_shadow_pre_display_item_value_initial()  }
        pub fn exact(variant_data: crate::css::BoxShadowPreDisplayItem) -> Self { az_box_shadow_pre_display_item_value_exact(variant_data.leak())}
    }

    impl Drop for BoxShadowPreDisplayItemValue { fn drop(&mut self) { az_box_shadow_pre_display_item_value_delete(&mut self); } }


    /// `LayoutAlignContentValue` struct
    pub struct LayoutAlignContentValue { pub(crate) object: AzLayoutAlignContentValue }

    impl LayoutAlignContentValue {
        pub fn auto() -> Self { az_layout_align_content_value_auto()  }
        pub fn none() -> Self { az_layout_align_content_value_none()  }
        pub fn inherit() -> Self { az_layout_align_content_value_inherit()  }
        pub fn initial() -> Self { az_layout_align_content_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutAlignContent) -> Self { az_layout_align_content_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutAlignContentValue { fn drop(&mut self) { az_layout_align_content_value_delete(&mut self); } }


    /// `LayoutAlignItemsValue` struct
    pub struct LayoutAlignItemsValue { pub(crate) object: AzLayoutAlignItemsValue }

    impl LayoutAlignItemsValue {
        pub fn auto() -> Self { az_layout_align_items_value_auto()  }
        pub fn none() -> Self { az_layout_align_items_value_none()  }
        pub fn inherit() -> Self { az_layout_align_items_value_inherit()  }
        pub fn initial() -> Self { az_layout_align_items_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutAlignItems) -> Self { az_layout_align_items_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutAlignItemsValue { fn drop(&mut self) { az_layout_align_items_value_delete(&mut self); } }


    /// `LayoutBottomValue` struct
    pub struct LayoutBottomValue { pub(crate) object: AzLayoutBottomValue }

    impl LayoutBottomValue {
        pub fn auto() -> Self { az_layout_bottom_value_auto()  }
        pub fn none() -> Self { az_layout_bottom_value_none()  }
        pub fn inherit() -> Self { az_layout_bottom_value_inherit()  }
        pub fn initial() -> Self { az_layout_bottom_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutBottom) -> Self { az_layout_bottom_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutBottomValue { fn drop(&mut self) { az_layout_bottom_value_delete(&mut self); } }


    /// `LayoutBoxSizingValue` struct
    pub struct LayoutBoxSizingValue { pub(crate) object: AzLayoutBoxSizingValue }

    impl LayoutBoxSizingValue {
        pub fn auto() -> Self { az_layout_box_sizing_value_auto()  }
        pub fn none() -> Self { az_layout_box_sizing_value_none()  }
        pub fn inherit() -> Self { az_layout_box_sizing_value_inherit()  }
        pub fn initial() -> Self { az_layout_box_sizing_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutBoxSizing) -> Self { az_layout_box_sizing_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutBoxSizingValue { fn drop(&mut self) { az_layout_box_sizing_value_delete(&mut self); } }


    /// `LayoutDirectionValue` struct
    pub struct LayoutDirectionValue { pub(crate) object: AzLayoutDirectionValue }

    impl LayoutDirectionValue {
        pub fn auto() -> Self { az_layout_direction_value_auto()  }
        pub fn none() -> Self { az_layout_direction_value_none()  }
        pub fn inherit() -> Self { az_layout_direction_value_inherit()  }
        pub fn initial() -> Self { az_layout_direction_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutDirection) -> Self { az_layout_direction_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutDirectionValue { fn drop(&mut self) { az_layout_direction_value_delete(&mut self); } }


    /// `LayoutDisplayValue` struct
    pub struct LayoutDisplayValue { pub(crate) object: AzLayoutDisplayValue }

    impl LayoutDisplayValue {
        pub fn auto() -> Self { az_layout_display_value_auto()  }
        pub fn none() -> Self { az_layout_display_value_none()  }
        pub fn inherit() -> Self { az_layout_display_value_inherit()  }
        pub fn initial() -> Self { az_layout_display_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutDisplay) -> Self { az_layout_display_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutDisplayValue { fn drop(&mut self) { az_layout_display_value_delete(&mut self); } }


    /// `LayoutFlexGrowValue` struct
    pub struct LayoutFlexGrowValue { pub(crate) object: AzLayoutFlexGrowValue }

    impl LayoutFlexGrowValue {
        pub fn auto() -> Self { az_layout_flex_grow_value_auto()  }
        pub fn none() -> Self { az_layout_flex_grow_value_none()  }
        pub fn inherit() -> Self { az_layout_flex_grow_value_inherit()  }
        pub fn initial() -> Self { az_layout_flex_grow_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutFlexGrow) -> Self { az_layout_flex_grow_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutFlexGrowValue { fn drop(&mut self) { az_layout_flex_grow_value_delete(&mut self); } }


    /// `LayoutFlexShrinkValue` struct
    pub struct LayoutFlexShrinkValue { pub(crate) object: AzLayoutFlexShrinkValue }

    impl LayoutFlexShrinkValue {
        pub fn auto() -> Self { az_layout_flex_shrink_value_auto()  }
        pub fn none() -> Self { az_layout_flex_shrink_value_none()  }
        pub fn inherit() -> Self { az_layout_flex_shrink_value_inherit()  }
        pub fn initial() -> Self { az_layout_flex_shrink_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutFlexShrink) -> Self { az_layout_flex_shrink_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutFlexShrinkValue { fn drop(&mut self) { az_layout_flex_shrink_value_delete(&mut self); } }


    /// `LayoutFloatValue` struct
    pub struct LayoutFloatValue { pub(crate) object: AzLayoutFloatValue }

    impl LayoutFloatValue {
        pub fn auto() -> Self { az_layout_float_value_auto()  }
        pub fn none() -> Self { az_layout_float_value_none()  }
        pub fn inherit() -> Self { az_layout_float_value_inherit()  }
        pub fn initial() -> Self { az_layout_float_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutFloat) -> Self { az_layout_float_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutFloatValue { fn drop(&mut self) { az_layout_float_value_delete(&mut self); } }


    /// `LayoutHeightValue` struct
    pub struct LayoutHeightValue { pub(crate) object: AzLayoutHeightValue }

    impl LayoutHeightValue {
        pub fn auto() -> Self { az_layout_height_value_auto()  }
        pub fn none() -> Self { az_layout_height_value_none()  }
        pub fn inherit() -> Self { az_layout_height_value_inherit()  }
        pub fn initial() -> Self { az_layout_height_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutHeight) -> Self { az_layout_height_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutHeightValue { fn drop(&mut self) { az_layout_height_value_delete(&mut self); } }


    /// `LayoutJustifyContentValue` struct
    pub struct LayoutJustifyContentValue { pub(crate) object: AzLayoutJustifyContentValue }

    impl LayoutJustifyContentValue {
        pub fn auto() -> Self { az_layout_justify_content_value_auto()  }
        pub fn none() -> Self { az_layout_justify_content_value_none()  }
        pub fn inherit() -> Self { az_layout_justify_content_value_inherit()  }
        pub fn initial() -> Self { az_layout_justify_content_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutJustifyContent) -> Self { az_layout_justify_content_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutJustifyContentValue { fn drop(&mut self) { az_layout_justify_content_value_delete(&mut self); } }


    /// `LayoutLeftValue` struct
    pub struct LayoutLeftValue { pub(crate) object: AzLayoutLeftValue }

    impl LayoutLeftValue {
        pub fn auto() -> Self { az_layout_left_value_auto()  }
        pub fn none() -> Self { az_layout_left_value_none()  }
        pub fn inherit() -> Self { az_layout_left_value_inherit()  }
        pub fn initial() -> Self { az_layout_left_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutLeft) -> Self { az_layout_left_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutLeftValue { fn drop(&mut self) { az_layout_left_value_delete(&mut self); } }


    /// `LayoutMarginBottomValue` struct
    pub struct LayoutMarginBottomValue { pub(crate) object: AzLayoutMarginBottomValue }

    impl LayoutMarginBottomValue {
        pub fn auto() -> Self { az_layout_margin_bottom_value_auto()  }
        pub fn none() -> Self { az_layout_margin_bottom_value_none()  }
        pub fn inherit() -> Self { az_layout_margin_bottom_value_inherit()  }
        pub fn initial() -> Self { az_layout_margin_bottom_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutMarginBottom) -> Self { az_layout_margin_bottom_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutMarginBottomValue { fn drop(&mut self) { az_layout_margin_bottom_value_delete(&mut self); } }


    /// `LayoutMarginLeftValue` struct
    pub struct LayoutMarginLeftValue { pub(crate) object: AzLayoutMarginLeftValue }

    impl LayoutMarginLeftValue {
        pub fn auto() -> Self { az_layout_margin_left_value_auto()  }
        pub fn none() -> Self { az_layout_margin_left_value_none()  }
        pub fn inherit() -> Self { az_layout_margin_left_value_inherit()  }
        pub fn initial() -> Self { az_layout_margin_left_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutMarginLeft) -> Self { az_layout_margin_left_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutMarginLeftValue { fn drop(&mut self) { az_layout_margin_left_value_delete(&mut self); } }


    /// `LayoutMarginRightValue` struct
    pub struct LayoutMarginRightValue { pub(crate) object: AzLayoutMarginRightValue }

    impl LayoutMarginRightValue {
        pub fn auto() -> Self { az_layout_margin_right_value_auto()  }
        pub fn none() -> Self { az_layout_margin_right_value_none()  }
        pub fn inherit() -> Self { az_layout_margin_right_value_inherit()  }
        pub fn initial() -> Self { az_layout_margin_right_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutMarginRight) -> Self { az_layout_margin_right_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutMarginRightValue { fn drop(&mut self) { az_layout_margin_right_value_delete(&mut self); } }


    /// `LayoutMarginTopValue` struct
    pub struct LayoutMarginTopValue { pub(crate) object: AzLayoutMarginTopValue }

    impl LayoutMarginTopValue {
        pub fn auto() -> Self { az_layout_margin_top_value_auto()  }
        pub fn none() -> Self { az_layout_margin_top_value_none()  }
        pub fn inherit() -> Self { az_layout_margin_top_value_inherit()  }
        pub fn initial() -> Self { az_layout_margin_top_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutMarginTop) -> Self { az_layout_margin_top_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutMarginTopValue { fn drop(&mut self) { az_layout_margin_top_value_delete(&mut self); } }


    /// `LayoutMaxHeightValue` struct
    pub struct LayoutMaxHeightValue { pub(crate) object: AzLayoutMaxHeightValue }

    impl LayoutMaxHeightValue {
        pub fn auto() -> Self { az_layout_max_height_value_auto()  }
        pub fn none() -> Self { az_layout_max_height_value_none()  }
        pub fn inherit() -> Self { az_layout_max_height_value_inherit()  }
        pub fn initial() -> Self { az_layout_max_height_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutMaxHeight) -> Self { az_layout_max_height_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutMaxHeightValue { fn drop(&mut self) { az_layout_max_height_value_delete(&mut self); } }


    /// `LayoutMaxWidthValue` struct
    pub struct LayoutMaxWidthValue { pub(crate) object: AzLayoutMaxWidthValue }

    impl LayoutMaxWidthValue {
        pub fn auto() -> Self { az_layout_max_width_value_auto()  }
        pub fn none() -> Self { az_layout_max_width_value_none()  }
        pub fn inherit() -> Self { az_layout_max_width_value_inherit()  }
        pub fn initial() -> Self { az_layout_max_width_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutMaxWidth) -> Self { az_layout_max_width_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutMaxWidthValue { fn drop(&mut self) { az_layout_max_width_value_delete(&mut self); } }


    /// `LayoutMinHeightValue` struct
    pub struct LayoutMinHeightValue { pub(crate) object: AzLayoutMinHeightValue }

    impl LayoutMinHeightValue {
        pub fn auto() -> Self { az_layout_min_height_value_auto()  }
        pub fn none() -> Self { az_layout_min_height_value_none()  }
        pub fn inherit() -> Self { az_layout_min_height_value_inherit()  }
        pub fn initial() -> Self { az_layout_min_height_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutMinHeight) -> Self { az_layout_min_height_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutMinHeightValue { fn drop(&mut self) { az_layout_min_height_value_delete(&mut self); } }


    /// `LayoutMinWidthValue` struct
    pub struct LayoutMinWidthValue { pub(crate) object: AzLayoutMinWidthValue }

    impl LayoutMinWidthValue {
        pub fn auto() -> Self { az_layout_min_width_value_auto()  }
        pub fn none() -> Self { az_layout_min_width_value_none()  }
        pub fn inherit() -> Self { az_layout_min_width_value_inherit()  }
        pub fn initial() -> Self { az_layout_min_width_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutMinWidth) -> Self { az_layout_min_width_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutMinWidthValue { fn drop(&mut self) { az_layout_min_width_value_delete(&mut self); } }


    /// `LayoutPaddingBottomValue` struct
    pub struct LayoutPaddingBottomValue { pub(crate) object: AzLayoutPaddingBottomValue }

    impl LayoutPaddingBottomValue {
        pub fn auto() -> Self { az_layout_padding_bottom_value_auto()  }
        pub fn none() -> Self { az_layout_padding_bottom_value_none()  }
        pub fn inherit() -> Self { az_layout_padding_bottom_value_inherit()  }
        pub fn initial() -> Self { az_layout_padding_bottom_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutPaddingBottom) -> Self { az_layout_padding_bottom_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutPaddingBottomValue { fn drop(&mut self) { az_layout_padding_bottom_value_delete(&mut self); } }


    /// `LayoutPaddingLeftValue` struct
    pub struct LayoutPaddingLeftValue { pub(crate) object: AzLayoutPaddingLeftValue }

    impl LayoutPaddingLeftValue {
        pub fn auto() -> Self { az_layout_padding_left_value_auto()  }
        pub fn none() -> Self { az_layout_padding_left_value_none()  }
        pub fn inherit() -> Self { az_layout_padding_left_value_inherit()  }
        pub fn initial() -> Self { az_layout_padding_left_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutPaddingLeft) -> Self { az_layout_padding_left_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutPaddingLeftValue { fn drop(&mut self) { az_layout_padding_left_value_delete(&mut self); } }


    /// `LayoutPaddingRightValue` struct
    pub struct LayoutPaddingRightValue { pub(crate) object: AzLayoutPaddingRightValue }

    impl LayoutPaddingRightValue {
        pub fn auto() -> Self { az_layout_padding_right_value_auto()  }
        pub fn none() -> Self { az_layout_padding_right_value_none()  }
        pub fn inherit() -> Self { az_layout_padding_right_value_inherit()  }
        pub fn initial() -> Self { az_layout_padding_right_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutPaddingRight) -> Self { az_layout_padding_right_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutPaddingRightValue { fn drop(&mut self) { az_layout_padding_right_value_delete(&mut self); } }


    /// `LayoutPaddingTopValue` struct
    pub struct LayoutPaddingTopValue { pub(crate) object: AzLayoutPaddingTopValue }

    impl LayoutPaddingTopValue {
        pub fn auto() -> Self { az_layout_padding_top_value_auto()  }
        pub fn none() -> Self { az_layout_padding_top_value_none()  }
        pub fn inherit() -> Self { az_layout_padding_top_value_inherit()  }
        pub fn initial() -> Self { az_layout_padding_top_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutPaddingTop) -> Self { az_layout_padding_top_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutPaddingTopValue { fn drop(&mut self) { az_layout_padding_top_value_delete(&mut self); } }


    /// `LayoutPositionValue` struct
    pub struct LayoutPositionValue { pub(crate) object: AzLayoutPositionValue }

    impl LayoutPositionValue {
        pub fn auto() -> Self { az_layout_position_value_auto()  }
        pub fn none() -> Self { az_layout_position_value_none()  }
        pub fn inherit() -> Self { az_layout_position_value_inherit()  }
        pub fn initial() -> Self { az_layout_position_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutPosition) -> Self { az_layout_position_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutPositionValue { fn drop(&mut self) { az_layout_position_value_delete(&mut self); } }


    /// `LayoutRightValue` struct
    pub struct LayoutRightValue { pub(crate) object: AzLayoutRightValue }

    impl LayoutRightValue {
        pub fn auto() -> Self { az_layout_right_value_auto()  }
        pub fn none() -> Self { az_layout_right_value_none()  }
        pub fn inherit() -> Self { az_layout_right_value_inherit()  }
        pub fn initial() -> Self { az_layout_right_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutRight) -> Self { az_layout_right_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutRightValue { fn drop(&mut self) { az_layout_right_value_delete(&mut self); } }


    /// `LayoutTopValue` struct
    pub struct LayoutTopValue { pub(crate) object: AzLayoutTopValue }

    impl LayoutTopValue {
        pub fn auto() -> Self { az_layout_top_value_auto()  }
        pub fn none() -> Self { az_layout_top_value_none()  }
        pub fn inherit() -> Self { az_layout_top_value_inherit()  }
        pub fn initial() -> Self { az_layout_top_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutTop) -> Self { az_layout_top_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutTopValue { fn drop(&mut self) { az_layout_top_value_delete(&mut self); } }


    /// `LayoutWidthValue` struct
    pub struct LayoutWidthValue { pub(crate) object: AzLayoutWidthValue }

    impl LayoutWidthValue {
        pub fn auto() -> Self { az_layout_width_value_auto()  }
        pub fn none() -> Self { az_layout_width_value_none()  }
        pub fn inherit() -> Self { az_layout_width_value_inherit()  }
        pub fn initial() -> Self { az_layout_width_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutWidth) -> Self { az_layout_width_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutWidthValue { fn drop(&mut self) { az_layout_width_value_delete(&mut self); } }


    /// `LayoutWrapValue` struct
    pub struct LayoutWrapValue { pub(crate) object: AzLayoutWrapValue }

    impl LayoutWrapValue {
        pub fn auto() -> Self { az_layout_wrap_value_auto()  }
        pub fn none() -> Self { az_layout_wrap_value_none()  }
        pub fn inherit() -> Self { az_layout_wrap_value_inherit()  }
        pub fn initial() -> Self { az_layout_wrap_value_initial()  }
        pub fn exact(variant_data: crate::css::LayoutWrap) -> Self { az_layout_wrap_value_exact(variant_data.leak())}
    }

    impl Drop for LayoutWrapValue { fn drop(&mut self) { az_layout_wrap_value_delete(&mut self); } }


    /// `OverflowValue` struct
    pub struct OverflowValue { pub(crate) object: AzOverflowValue }

    impl OverflowValue {
        pub fn auto() -> Self { az_overflow_value_auto()  }
        pub fn none() -> Self { az_overflow_value_none()  }
        pub fn inherit() -> Self { az_overflow_value_inherit()  }
        pub fn initial() -> Self { az_overflow_value_initial()  }
        pub fn exact(variant_data: crate::css::Overflow) -> Self { az_overflow_value_exact(variant_data.leak())}
    }

    impl Drop for OverflowValue { fn drop(&mut self) { az_overflow_value_delete(&mut self); } }


    /// `StyleBackgroundContentValue` struct
    pub struct StyleBackgroundContentValue { pub(crate) object: AzStyleBackgroundContentValue }

    impl StyleBackgroundContentValue {
        pub fn auto() -> Self { az_style_background_content_value_auto()  }
        pub fn none() -> Self { az_style_background_content_value_none()  }
        pub fn inherit() -> Self { az_style_background_content_value_inherit()  }
        pub fn initial() -> Self { az_style_background_content_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBackgroundContent) -> Self { az_style_background_content_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBackgroundContentValue { fn drop(&mut self) { az_style_background_content_value_delete(&mut self); } }


    /// `StyleBackgroundPositionValue` struct
    pub struct StyleBackgroundPositionValue { pub(crate) object: AzStyleBackgroundPositionValue }

    impl StyleBackgroundPositionValue {
        pub fn auto() -> Self { az_style_background_position_value_auto()  }
        pub fn none() -> Self { az_style_background_position_value_none()  }
        pub fn inherit() -> Self { az_style_background_position_value_inherit()  }
        pub fn initial() -> Self { az_style_background_position_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBackgroundPosition) -> Self { az_style_background_position_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBackgroundPositionValue { fn drop(&mut self) { az_style_background_position_value_delete(&mut self); } }


    /// `StyleBackgroundRepeatValue` struct
    pub struct StyleBackgroundRepeatValue { pub(crate) object: AzStyleBackgroundRepeatValue }

    impl StyleBackgroundRepeatValue {
        pub fn auto() -> Self { az_style_background_repeat_value_auto()  }
        pub fn none() -> Self { az_style_background_repeat_value_none()  }
        pub fn inherit() -> Self { az_style_background_repeat_value_inherit()  }
        pub fn initial() -> Self { az_style_background_repeat_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBackgroundRepeat) -> Self { az_style_background_repeat_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBackgroundRepeatValue { fn drop(&mut self) { az_style_background_repeat_value_delete(&mut self); } }


    /// `StyleBackgroundSizeValue` struct
    pub struct StyleBackgroundSizeValue { pub(crate) object: AzStyleBackgroundSizeValue }

    impl StyleBackgroundSizeValue {
        pub fn auto() -> Self { az_style_background_size_value_auto()  }
        pub fn none() -> Self { az_style_background_size_value_none()  }
        pub fn inherit() -> Self { az_style_background_size_value_inherit()  }
        pub fn initial() -> Self { az_style_background_size_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBackgroundSize) -> Self { az_style_background_size_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBackgroundSizeValue { fn drop(&mut self) { az_style_background_size_value_delete(&mut self); } }


    /// `StyleBorderBottomColorValue` struct
    pub struct StyleBorderBottomColorValue { pub(crate) object: AzStyleBorderBottomColorValue }

    impl StyleBorderBottomColorValue {
        pub fn auto() -> Self { az_style_border_bottom_color_value_auto()  }
        pub fn none() -> Self { az_style_border_bottom_color_value_none()  }
        pub fn inherit() -> Self { az_style_border_bottom_color_value_inherit()  }
        pub fn initial() -> Self { az_style_border_bottom_color_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomColor) -> Self { az_style_border_bottom_color_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderBottomColorValue { fn drop(&mut self) { az_style_border_bottom_color_value_delete(&mut self); } }


    /// `StyleBorderBottomLeftRadiusValue` struct
    pub struct StyleBorderBottomLeftRadiusValue { pub(crate) object: AzStyleBorderBottomLeftRadiusValue }

    impl StyleBorderBottomLeftRadiusValue {
        pub fn auto() -> Self { az_style_border_bottom_left_radius_value_auto()  }
        pub fn none() -> Self { az_style_border_bottom_left_radius_value_none()  }
        pub fn inherit() -> Self { az_style_border_bottom_left_radius_value_inherit()  }
        pub fn initial() -> Self { az_style_border_bottom_left_radius_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomLeftRadius) -> Self { az_style_border_bottom_left_radius_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderBottomLeftRadiusValue { fn drop(&mut self) { az_style_border_bottom_left_radius_value_delete(&mut self); } }


    /// `StyleBorderBottomRightRadiusValue` struct
    pub struct StyleBorderBottomRightRadiusValue { pub(crate) object: AzStyleBorderBottomRightRadiusValue }

    impl StyleBorderBottomRightRadiusValue {
        pub fn auto() -> Self { az_style_border_bottom_right_radius_value_auto()  }
        pub fn none() -> Self { az_style_border_bottom_right_radius_value_none()  }
        pub fn inherit() -> Self { az_style_border_bottom_right_radius_value_inherit()  }
        pub fn initial() -> Self { az_style_border_bottom_right_radius_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomRightRadius) -> Self { az_style_border_bottom_right_radius_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderBottomRightRadiusValue { fn drop(&mut self) { az_style_border_bottom_right_radius_value_delete(&mut self); } }


    /// `StyleBorderBottomStyleValue` struct
    pub struct StyleBorderBottomStyleValue { pub(crate) object: AzStyleBorderBottomStyleValue }

    impl StyleBorderBottomStyleValue {
        pub fn auto() -> Self { az_style_border_bottom_style_value_auto()  }
        pub fn none() -> Self { az_style_border_bottom_style_value_none()  }
        pub fn inherit() -> Self { az_style_border_bottom_style_value_inherit()  }
        pub fn initial() -> Self { az_style_border_bottom_style_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomStyle) -> Self { az_style_border_bottom_style_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderBottomStyleValue { fn drop(&mut self) { az_style_border_bottom_style_value_delete(&mut self); } }


    /// `StyleBorderBottomWidthValue` struct
    pub struct StyleBorderBottomWidthValue { pub(crate) object: AzStyleBorderBottomWidthValue }

    impl StyleBorderBottomWidthValue {
        pub fn auto() -> Self { az_style_border_bottom_width_value_auto()  }
        pub fn none() -> Self { az_style_border_bottom_width_value_none()  }
        pub fn inherit() -> Self { az_style_border_bottom_width_value_inherit()  }
        pub fn initial() -> Self { az_style_border_bottom_width_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderBottomWidth) -> Self { az_style_border_bottom_width_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderBottomWidthValue { fn drop(&mut self) { az_style_border_bottom_width_value_delete(&mut self); } }


    /// `StyleBorderLeftColorValue` struct
    pub struct StyleBorderLeftColorValue { pub(crate) object: AzStyleBorderLeftColorValue }

    impl StyleBorderLeftColorValue {
        pub fn auto() -> Self { az_style_border_left_color_value_auto()  }
        pub fn none() -> Self { az_style_border_left_color_value_none()  }
        pub fn inherit() -> Self { az_style_border_left_color_value_inherit()  }
        pub fn initial() -> Self { az_style_border_left_color_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderLeftColor) -> Self { az_style_border_left_color_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderLeftColorValue { fn drop(&mut self) { az_style_border_left_color_value_delete(&mut self); } }


    /// `StyleBorderLeftStyleValue` struct
    pub struct StyleBorderLeftStyleValue { pub(crate) object: AzStyleBorderLeftStyleValue }

    impl StyleBorderLeftStyleValue {
        pub fn auto() -> Self { az_style_border_left_style_value_auto()  }
        pub fn none() -> Self { az_style_border_left_style_value_none()  }
        pub fn inherit() -> Self { az_style_border_left_style_value_inherit()  }
        pub fn initial() -> Self { az_style_border_left_style_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderLeftStyle) -> Self { az_style_border_left_style_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderLeftStyleValue { fn drop(&mut self) { az_style_border_left_style_value_delete(&mut self); } }


    /// `StyleBorderLeftWidthValue` struct
    pub struct StyleBorderLeftWidthValue { pub(crate) object: AzStyleBorderLeftWidthValue }

    impl StyleBorderLeftWidthValue {
        pub fn auto() -> Self { az_style_border_left_width_value_auto()  }
        pub fn none() -> Self { az_style_border_left_width_value_none()  }
        pub fn inherit() -> Self { az_style_border_left_width_value_inherit()  }
        pub fn initial() -> Self { az_style_border_left_width_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderLeftWidth) -> Self { az_style_border_left_width_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderLeftWidthValue { fn drop(&mut self) { az_style_border_left_width_value_delete(&mut self); } }


    /// `StyleBorderRightColorValue` struct
    pub struct StyleBorderRightColorValue { pub(crate) object: AzStyleBorderRightColorValue }

    impl StyleBorderRightColorValue {
        pub fn auto() -> Self { az_style_border_right_color_value_auto()  }
        pub fn none() -> Self { az_style_border_right_color_value_none()  }
        pub fn inherit() -> Self { az_style_border_right_color_value_inherit()  }
        pub fn initial() -> Self { az_style_border_right_color_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderRightColor) -> Self { az_style_border_right_color_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderRightColorValue { fn drop(&mut self) { az_style_border_right_color_value_delete(&mut self); } }


    /// `StyleBorderRightStyleValue` struct
    pub struct StyleBorderRightStyleValue { pub(crate) object: AzStyleBorderRightStyleValue }

    impl StyleBorderRightStyleValue {
        pub fn auto() -> Self { az_style_border_right_style_value_auto()  }
        pub fn none() -> Self { az_style_border_right_style_value_none()  }
        pub fn inherit() -> Self { az_style_border_right_style_value_inherit()  }
        pub fn initial() -> Self { az_style_border_right_style_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderRightStyle) -> Self { az_style_border_right_style_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderRightStyleValue { fn drop(&mut self) { az_style_border_right_style_value_delete(&mut self); } }


    /// `StyleBorderRightWidthValue` struct
    pub struct StyleBorderRightWidthValue { pub(crate) object: AzStyleBorderRightWidthValue }

    impl StyleBorderRightWidthValue {
        pub fn auto() -> Self { az_style_border_right_width_value_auto()  }
        pub fn none() -> Self { az_style_border_right_width_value_none()  }
        pub fn inherit() -> Self { az_style_border_right_width_value_inherit()  }
        pub fn initial() -> Self { az_style_border_right_width_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderRightWidth) -> Self { az_style_border_right_width_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderRightWidthValue { fn drop(&mut self) { az_style_border_right_width_value_delete(&mut self); } }


    /// `StyleBorderTopColorValue` struct
    pub struct StyleBorderTopColorValue { pub(crate) object: AzStyleBorderTopColorValue }

    impl StyleBorderTopColorValue {
        pub fn auto() -> Self { az_style_border_top_color_value_auto()  }
        pub fn none() -> Self { az_style_border_top_color_value_none()  }
        pub fn inherit() -> Self { az_style_border_top_color_value_inherit()  }
        pub fn initial() -> Self { az_style_border_top_color_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderTopColor) -> Self { az_style_border_top_color_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderTopColorValue { fn drop(&mut self) { az_style_border_top_color_value_delete(&mut self); } }


    /// `StyleBorderTopLeftRadiusValue` struct
    pub struct StyleBorderTopLeftRadiusValue { pub(crate) object: AzStyleBorderTopLeftRadiusValue }

    impl StyleBorderTopLeftRadiusValue {
        pub fn auto() -> Self { az_style_border_top_left_radius_value_auto()  }
        pub fn none() -> Self { az_style_border_top_left_radius_value_none()  }
        pub fn inherit() -> Self { az_style_border_top_left_radius_value_inherit()  }
        pub fn initial() -> Self { az_style_border_top_left_radius_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderTopLeftRadius) -> Self { az_style_border_top_left_radius_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderTopLeftRadiusValue { fn drop(&mut self) { az_style_border_top_left_radius_value_delete(&mut self); } }


    /// `StyleBorderTopRightRadiusValue` struct
    pub struct StyleBorderTopRightRadiusValue { pub(crate) object: AzStyleBorderTopRightRadiusValue }

    impl StyleBorderTopRightRadiusValue {
        pub fn auto() -> Self { az_style_border_top_right_radius_value_auto()  }
        pub fn none() -> Self { az_style_border_top_right_radius_value_none()  }
        pub fn inherit() -> Self { az_style_border_top_right_radius_value_inherit()  }
        pub fn initial() -> Self { az_style_border_top_right_radius_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderTopRightRadius) -> Self { az_style_border_top_right_radius_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderTopRightRadiusValue { fn drop(&mut self) { az_style_border_top_right_radius_value_delete(&mut self); } }


    /// `StyleBorderTopStyleValue` struct
    pub struct StyleBorderTopStyleValue { pub(crate) object: AzStyleBorderTopStyleValue }

    impl StyleBorderTopStyleValue {
        pub fn auto() -> Self { az_style_border_top_style_value_auto()  }
        pub fn none() -> Self { az_style_border_top_style_value_none()  }
        pub fn inherit() -> Self { az_style_border_top_style_value_inherit()  }
        pub fn initial() -> Self { az_style_border_top_style_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderTopStyle) -> Self { az_style_border_top_style_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderTopStyleValue { fn drop(&mut self) { az_style_border_top_style_value_delete(&mut self); } }


    /// `StyleBorderTopWidthValue` struct
    pub struct StyleBorderTopWidthValue { pub(crate) object: AzStyleBorderTopWidthValue }

    impl StyleBorderTopWidthValue {
        pub fn auto() -> Self { az_style_border_top_width_value_auto()  }
        pub fn none() -> Self { az_style_border_top_width_value_none()  }
        pub fn inherit() -> Self { az_style_border_top_width_value_inherit()  }
        pub fn initial() -> Self { az_style_border_top_width_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleBorderTopWidth) -> Self { az_style_border_top_width_value_exact(variant_data.leak())}
    }

    impl Drop for StyleBorderTopWidthValue { fn drop(&mut self) { az_style_border_top_width_value_delete(&mut self); } }


    /// `StyleCursorValue` struct
    pub struct StyleCursorValue { pub(crate) object: AzStyleCursorValue }

    impl StyleCursorValue {
        pub fn auto() -> Self { az_style_cursor_value_auto()  }
        pub fn none() -> Self { az_style_cursor_value_none()  }
        pub fn inherit() -> Self { az_style_cursor_value_inherit()  }
        pub fn initial() -> Self { az_style_cursor_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleCursor) -> Self { az_style_cursor_value_exact(variant_data.leak())}
    }

    impl Drop for StyleCursorValue { fn drop(&mut self) { az_style_cursor_value_delete(&mut self); } }


    /// `StyleFontFamilyValue` struct
    pub struct StyleFontFamilyValue { pub(crate) object: AzStyleFontFamilyValue }

    impl StyleFontFamilyValue {
        pub fn auto() -> Self { az_style_font_family_value_auto()  }
        pub fn none() -> Self { az_style_font_family_value_none()  }
        pub fn inherit() -> Self { az_style_font_family_value_inherit()  }
        pub fn initial() -> Self { az_style_font_family_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleFontFamily) -> Self { az_style_font_family_value_exact(variant_data.leak())}
    }

    impl Drop for StyleFontFamilyValue { fn drop(&mut self) { az_style_font_family_value_delete(&mut self); } }


    /// `StyleFontSizeValue` struct
    pub struct StyleFontSizeValue { pub(crate) object: AzStyleFontSizeValue }

    impl StyleFontSizeValue {
        pub fn auto() -> Self { az_style_font_size_value_auto()  }
        pub fn none() -> Self { az_style_font_size_value_none()  }
        pub fn inherit() -> Self { az_style_font_size_value_inherit()  }
        pub fn initial() -> Self { az_style_font_size_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleFontSize) -> Self { az_style_font_size_value_exact(variant_data.leak())}
    }

    impl Drop for StyleFontSizeValue { fn drop(&mut self) { az_style_font_size_value_delete(&mut self); } }


    /// `StyleLetterSpacingValue` struct
    pub struct StyleLetterSpacingValue { pub(crate) object: AzStyleLetterSpacingValue }

    impl StyleLetterSpacingValue {
        pub fn auto() -> Self { az_style_letter_spacing_value_auto()  }
        pub fn none() -> Self { az_style_letter_spacing_value_none()  }
        pub fn inherit() -> Self { az_style_letter_spacing_value_inherit()  }
        pub fn initial() -> Self { az_style_letter_spacing_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleLetterSpacing) -> Self { az_style_letter_spacing_value_exact(variant_data.leak())}
    }

    impl Drop for StyleLetterSpacingValue { fn drop(&mut self) { az_style_letter_spacing_value_delete(&mut self); } }


    /// `StyleLineHeightValue` struct
    pub struct StyleLineHeightValue { pub(crate) object: AzStyleLineHeightValue }

    impl StyleLineHeightValue {
        pub fn auto() -> Self { az_style_line_height_value_auto()  }
        pub fn none() -> Self { az_style_line_height_value_none()  }
        pub fn inherit() -> Self { az_style_line_height_value_inherit()  }
        pub fn initial() -> Self { az_style_line_height_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleLineHeight) -> Self { az_style_line_height_value_exact(variant_data.leak())}
    }

    impl Drop for StyleLineHeightValue { fn drop(&mut self) { az_style_line_height_value_delete(&mut self); } }


    /// `StyleTabWidthValue` struct
    pub struct StyleTabWidthValue { pub(crate) object: AzStyleTabWidthValue }

    impl StyleTabWidthValue {
        pub fn auto() -> Self { az_style_tab_width_value_auto()  }
        pub fn none() -> Self { az_style_tab_width_value_none()  }
        pub fn inherit() -> Self { az_style_tab_width_value_inherit()  }
        pub fn initial() -> Self { az_style_tab_width_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleTabWidth) -> Self { az_style_tab_width_value_exact(variant_data.leak())}
    }

    impl Drop for StyleTabWidthValue { fn drop(&mut self) { az_style_tab_width_value_delete(&mut self); } }


    /// `StyleTextAlignmentHorzValue` struct
    pub struct StyleTextAlignmentHorzValue { pub(crate) object: AzStyleTextAlignmentHorzValue }

    impl StyleTextAlignmentHorzValue {
        pub fn auto() -> Self { az_style_text_alignment_horz_value_auto()  }
        pub fn none() -> Self { az_style_text_alignment_horz_value_none()  }
        pub fn inherit() -> Self { az_style_text_alignment_horz_value_inherit()  }
        pub fn initial() -> Self { az_style_text_alignment_horz_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleTextAlignmentHorz) -> Self { az_style_text_alignment_horz_value_exact(variant_data.leak())}
    }

    impl Drop for StyleTextAlignmentHorzValue { fn drop(&mut self) { az_style_text_alignment_horz_value_delete(&mut self); } }


    /// `StyleTextColorValue` struct
    pub struct StyleTextColorValue { pub(crate) object: AzStyleTextColorValue }

    impl StyleTextColorValue {
        pub fn auto() -> Self { az_style_text_color_value_auto()  }
        pub fn none() -> Self { az_style_text_color_value_none()  }
        pub fn inherit() -> Self { az_style_text_color_value_inherit()  }
        pub fn initial() -> Self { az_style_text_color_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleTextColor) -> Self { az_style_text_color_value_exact(variant_data.leak())}
    }

    impl Drop for StyleTextColorValue { fn drop(&mut self) { az_style_text_color_value_delete(&mut self); } }


    /// `StyleWordSpacingValue` struct
    pub struct StyleWordSpacingValue { pub(crate) object: AzStyleWordSpacingValue }

    impl StyleWordSpacingValue {
        pub fn auto() -> Self { az_style_word_spacing_value_auto()  }
        pub fn none() -> Self { az_style_word_spacing_value_none()  }
        pub fn inherit() -> Self { az_style_word_spacing_value_inherit()  }
        pub fn initial() -> Self { az_style_word_spacing_value_initial()  }
        pub fn exact(variant_data: crate::css::StyleWordSpacing) -> Self { az_style_word_spacing_value_exact(variant_data.leak())}
    }

    impl Drop for StyleWordSpacingValue { fn drop(&mut self) { az_style_word_spacing_value_delete(&mut self); } }


    /// Parsed CSS key-value pair
    pub struct CssProperty { pub(crate) object: AzCssProperty }

    impl CssProperty {
        pub fn text_color(variant_data: crate::css::StyleTextColorValue) -> Self { az_css_property_text_color(variant_data.leak())}
        pub fn font_size(variant_data: crate::css::StyleFontSizeValue) -> Self { az_css_property_font_size(variant_data.leak())}
        pub fn font_family(variant_data: crate::css::StyleFontFamilyValue) -> Self { az_css_property_font_family(variant_data.leak())}
        pub fn text_align(variant_data: crate::css::StyleTextAlignmentHorzValue) -> Self { az_css_property_text_align(variant_data.leak())}
        pub fn letter_spacing(variant_data: crate::css::StyleLetterSpacingValue) -> Self { az_css_property_letter_spacing(variant_data.leak())}
        pub fn line_height(variant_data: crate::css::StyleLineHeightValue) -> Self { az_css_property_line_height(variant_data.leak())}
        pub fn word_spacing(variant_data: crate::css::StyleWordSpacingValue) -> Self { az_css_property_word_spacing(variant_data.leak())}
        pub fn tab_width(variant_data: crate::css::StyleTabWidthValue) -> Self { az_css_property_tab_width(variant_data.leak())}
        pub fn cursor(variant_data: crate::css::StyleCursorValue) -> Self { az_css_property_cursor(variant_data.leak())}
        pub fn display(variant_data: crate::css::LayoutDisplayValue) -> Self { az_css_property_display(variant_data.leak())}
        pub fn float(variant_data: crate::css::LayoutFloatValue) -> Self { az_css_property_float(variant_data.leak())}
        pub fn box_sizing(variant_data: crate::css::LayoutBoxSizingValue) -> Self { az_css_property_box_sizing(variant_data.leak())}
        pub fn width(variant_data: crate::css::LayoutWidthValue) -> Self { az_css_property_width(variant_data.leak())}
        pub fn height(variant_data: crate::css::LayoutHeightValue) -> Self { az_css_property_height(variant_data.leak())}
        pub fn min_width(variant_data: crate::css::LayoutMinWidthValue) -> Self { az_css_property_min_width(variant_data.leak())}
        pub fn min_height(variant_data: crate::css::LayoutMinHeightValue) -> Self { az_css_property_min_height(variant_data.leak())}
        pub fn max_width(variant_data: crate::css::LayoutMaxWidthValue) -> Self { az_css_property_max_width(variant_data.leak())}
        pub fn max_height(variant_data: crate::css::LayoutMaxHeightValue) -> Self { az_css_property_max_height(variant_data.leak())}
        pub fn position(variant_data: crate::css::LayoutPositionValue) -> Self { az_css_property_position(variant_data.leak())}
        pub fn top(variant_data: crate::css::LayoutTopValue) -> Self { az_css_property_top(variant_data.leak())}
        pub fn right(variant_data: crate::css::LayoutRightValue) -> Self { az_css_property_right(variant_data.leak())}
        pub fn left(variant_data: crate::css::LayoutLeftValue) -> Self { az_css_property_left(variant_data.leak())}
        pub fn bottom(variant_data: crate::css::LayoutBottomValue) -> Self { az_css_property_bottom(variant_data.leak())}
        pub fn flex_wrap(variant_data: crate::css::LayoutWrapValue) -> Self { az_css_property_flex_wrap(variant_data.leak())}
        pub fn flex_direction(variant_data: crate::css::LayoutDirectionValue) -> Self { az_css_property_flex_direction(variant_data.leak())}
        pub fn flex_grow(variant_data: crate::css::LayoutFlexGrowValue) -> Self { az_css_property_flex_grow(variant_data.leak())}
        pub fn flex_shrink(variant_data: crate::css::LayoutFlexShrinkValue) -> Self { az_css_property_flex_shrink(variant_data.leak())}
        pub fn justify_content(variant_data: crate::css::LayoutJustifyContentValue) -> Self { az_css_property_justify_content(variant_data.leak())}
        pub fn align_items(variant_data: crate::css::LayoutAlignItemsValue) -> Self { az_css_property_align_items(variant_data.leak())}
        pub fn align_content(variant_data: crate::css::LayoutAlignContentValue) -> Self { az_css_property_align_content(variant_data.leak())}
        pub fn background_content(variant_data: crate::css::StyleBackgroundContentValue) -> Self { az_css_property_background_content(variant_data.leak())}
        pub fn background_position(variant_data: crate::css::StyleBackgroundPositionValue) -> Self { az_css_property_background_position(variant_data.leak())}
        pub fn background_size(variant_data: crate::css::StyleBackgroundSizeValue) -> Self { az_css_property_background_size(variant_data.leak())}
        pub fn background_repeat(variant_data: crate::css::StyleBackgroundRepeatValue) -> Self { az_css_property_background_repeat(variant_data.leak())}
        pub fn overflow_x(variant_data: crate::css::OverflowValue) -> Self { az_css_property_overflow_x(variant_data.leak())}
        pub fn overflow_y(variant_data: crate::css::OverflowValue) -> Self { az_css_property_overflow_y(variant_data.leak())}
        pub fn padding_top(variant_data: crate::css::LayoutPaddingTopValue) -> Self { az_css_property_padding_top(variant_data.leak())}
        pub fn padding_left(variant_data: crate::css::LayoutPaddingLeftValue) -> Self { az_css_property_padding_left(variant_data.leak())}
        pub fn padding_right(variant_data: crate::css::LayoutPaddingRightValue) -> Self { az_css_property_padding_right(variant_data.leak())}
        pub fn padding_bottom(variant_data: crate::css::LayoutPaddingBottomValue) -> Self { az_css_property_padding_bottom(variant_data.leak())}
        pub fn margin_top(variant_data: crate::css::LayoutMarginTopValue) -> Self { az_css_property_margin_top(variant_data.leak())}
        pub fn margin_left(variant_data: crate::css::LayoutMarginLeftValue) -> Self { az_css_property_margin_left(variant_data.leak())}
        pub fn margin_right(variant_data: crate::css::LayoutMarginRightValue) -> Self { az_css_property_margin_right(variant_data.leak())}
        pub fn margin_bottom(variant_data: crate::css::LayoutMarginBottomValue) -> Self { az_css_property_margin_bottom(variant_data.leak())}
        pub fn border_top_left_radius(variant_data: crate::css::StyleBorderTopLeftRadiusValue) -> Self { az_css_property_border_top_left_radius(variant_data.leak())}
        pub fn border_top_right_radius(variant_data: crate::css::StyleBorderTopRightRadiusValue) -> Self { az_css_property_border_top_right_radius(variant_data.leak())}
        pub fn border_bottom_left_radius(variant_data: crate::css::StyleBorderBottomLeftRadiusValue) -> Self { az_css_property_border_bottom_left_radius(variant_data.leak())}
        pub fn border_bottom_right_radius(variant_data: crate::css::StyleBorderBottomRightRadiusValue) -> Self { az_css_property_border_bottom_right_radius(variant_data.leak())}
        pub fn border_top_color(variant_data: crate::css::StyleBorderTopColorValue) -> Self { az_css_property_border_top_color(variant_data.leak())}
        pub fn border_right_color(variant_data: crate::css::StyleBorderRightColorValue) -> Self { az_css_property_border_right_color(variant_data.leak())}
        pub fn border_left_color(variant_data: crate::css::StyleBorderLeftColorValue) -> Self { az_css_property_border_left_color(variant_data.leak())}
        pub fn border_bottom_color(variant_data: crate::css::StyleBorderBottomColorValue) -> Self { az_css_property_border_bottom_color(variant_data.leak())}
        pub fn border_top_style(variant_data: crate::css::StyleBorderTopStyleValue) -> Self { az_css_property_border_top_style(variant_data.leak())}
        pub fn border_right_style(variant_data: crate::css::StyleBorderRightStyleValue) -> Self { az_css_property_border_right_style(variant_data.leak())}
        pub fn border_left_style(variant_data: crate::css::StyleBorderLeftStyleValue) -> Self { az_css_property_border_left_style(variant_data.leak())}
        pub fn border_bottom_style(variant_data: crate::css::StyleBorderBottomStyleValue) -> Self { az_css_property_border_bottom_style(variant_data.leak())}
        pub fn border_top_width(variant_data: crate::css::StyleBorderTopWidthValue) -> Self { az_css_property_border_top_width(variant_data.leak())}
        pub fn border_right_width(variant_data: crate::css::StyleBorderRightWidthValue) -> Self { az_css_property_border_right_width(variant_data.leak())}
        pub fn border_left_width(variant_data: crate::css::StyleBorderLeftWidthValue) -> Self { az_css_property_border_left_width(variant_data.leak())}
        pub fn border_bottom_width(variant_data: crate::css::StyleBorderBottomWidthValue) -> Self { az_css_property_border_bottom_width(variant_data.leak())}
        pub fn box_shadow_left(variant_data: crate::css::BoxShadowPreDisplayItemValue) -> Self { az_css_property_box_shadow_left(variant_data.leak())}
        pub fn box_shadow_right(variant_data: crate::css::BoxShadowPreDisplayItemValue) -> Self { az_css_property_box_shadow_right(variant_data.leak())}
        pub fn box_shadow_top(variant_data: crate::css::BoxShadowPreDisplayItemValue) -> Self { az_css_property_box_shadow_top(variant_data.leak())}
        pub fn box_shadow_bottom(variant_data: crate::css::BoxShadowPreDisplayItemValue) -> Self { az_css_property_box_shadow_bottom(variant_data.leak())}
    }

    impl Drop for CssProperty { fn drop(&mut self) { az_css_property_delete(&mut self); } }
}

/// `Dom` construction and configuration
#[allow(dead_code, unused_imports)]
pub mod dom {

    use crate::dll::*;
    use crate::str::String;
    use crate::resources::{ImageId, TextId};
    use crate::callbacks::{RefAny, IFrameCallback, Callback, GlCallback};
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
        pub fn label(text: String) -> Self { Self { ptr: az_dom_label(text) } }
        /// Creates a new `p` node from a (cached) text referenced by a `TextId`
        pub fn text(text_id: TextId) -> Self { Self { ptr: az_dom_text(text_id) } }
        /// Creates a new `img` node from a (cached) text referenced by a `ImageId`
        pub fn image(image_id: ImageId) -> Self { Self { ptr: az_dom_image(image_id) } }
        /// Creates a new node which will render an OpenGL texture after the layout step is finished. See the documentation for [GlCallback]() for more info about OpenGL rendering callbacks.
        pub fn gl_texture(data: RefAny, callback: GlCallback) -> Self { Self { ptr: az_dom_gl_texture(data.leak(), callback) } }
        /// Creates a new node with a callback that will return a `Dom` after being layouted. See the documentation for [IFrameCallback]() for more info about iframe callbacks.
        pub fn iframe_callback(data: RefAny, callback: IFrameCallback) -> Self { Self { ptr: az_dom_iframe_callback(data.leak(), callback) } }
        /// Adds a CSS ID (`#something`) to the DOM node
        pub fn add_id(&mut self, id: String)  { az_dom_add_id(&mut self.ptr, id) }
        /// Same as [`Dom::add_id`](#method.add_id), but as a builder method
        pub fn with_id(self, id: String)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_id(self.leak(), id) } } }
        /// Same as calling [`Dom::add_id`](#method.add_id) for each CSS ID, but this function **replaces** all current CSS IDs
        pub fn set_ids(&mut self, ids: StringVec)  { az_dom_set_ids(&mut self.ptr, ids) }
        /// Same as [`Dom::set_ids`](#method.set_ids), but as a builder method
        pub fn with_ids(self, ids: StringVec)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_ids(self.leak(), ids) } } }
        /// Adds a CSS class (`.something`) to the DOM node
        pub fn add_class(&mut self, class: String)  { az_dom_add_class(&mut self.ptr, class) }
        /// Same as [`Dom::add_class`](#method.add_class), but as a builder method
        pub fn with_class(self, class: String)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_class(self.leak(), class) } } }
        /// Same as calling [`Dom::add_class`](#method.add_class) for each class, but this function **replaces** all current classes
        pub fn set_classes(&mut self, classes: StringVec)  { az_dom_set_classes(&mut self.ptr, classes) }
        /// Same as [`Dom::set_classes`](#method.set_classes), but as a builder method
        pub fn with_classes(self, classes: StringVec)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_classes(self.leak(), classes) } } }
        /// Adds a [`Callback`](callbacks/type.Callback) that acts on the `data` the `event` happens
        pub fn add_callback(&mut self, event: EventFilter, data: RefAny, callback: Callback)  { az_dom_add_callback(&mut self.ptr, event, data.leak(), callback) }
        /// Same as [`Dom::add_callback`](#method.add_callback), but as a builder method
        pub fn with_callback(self, event: EventFilter, data: RefAny, callback: Callback)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_callback(self.leak(), event, data.leak(), callback) } } }
        /// Overrides the CSS property of this DOM node with a value (for example `"width = 200px"`)
        pub fn add_css_override(&mut self, id: String, prop: CssProperty)  { az_dom_add_css_override(&mut self.ptr, id, prop) }
        /// Same as [`Dom::add_css_override`](#method.add_css_override), but as a builder method
        pub fn with_css_override(self, id: String, prop: CssProperty)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_css_override(self.leak(), id, prop) } } }
        /// Sets the `is_draggable` attribute of this DOM node (default: false)
        pub fn set_is_draggable(&mut self, is_draggable: bool)  { az_dom_set_is_draggable(&mut self.ptr, is_draggable) }
        /// Same as [`Dom::set_is_draggable`](#method.set_is_draggable), but as a builder method
        pub fn is_draggable(self, is_draggable: bool)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_is_draggable(self.leak(), is_draggable) } } }
        /// Sets the `tabindex` attribute of this DOM node (makes an element focusable - default: None)
        pub fn set_tab_index(&mut self, tab_index: TabIndex)  { az_dom_set_tab_index(&mut self.ptr, tab_index) }
        /// Same as [`Dom::set_tab_index`](#method.set_tab_index), but as a builder method
        pub fn with_tab_index(self, tab_index: TabIndex)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_tab_index(self.leak(), tab_index) } } }
        /// Reparents another `Dom` to be the child node of this `Dom`
        pub fn add_child(&mut self, child: Dom)  { az_dom_add_child(&mut self.ptr, child.leak()) }
        /// Same as [`Dom::add_child`](#method.add_child), but as a builder method
        pub fn with_child(self, child: Dom)  -> crate::dom::Dom { crate::dom::Dom { ptr: { az_dom_with_child(self.leak(), child.leak()) } } }
        /// Returns if the DOM node has a certain CSS ID
        pub fn has_id(&mut self, id: String)  -> bool { az_dom_has_id(&mut self.ptr, id) }
        /// Returns if the DOM node has a certain CSS class
        pub fn has_class(&mut self, class: String)  -> bool { az_dom_has_class(&mut self.ptr, class) }
        /// Returns the HTML String for this DOM
        pub fn get_html_string(&mut self)  -> crate::str::String { { az_dom_get_html_string(&mut self.ptr)} }
       /// Prevents the destructor from running and returns the internal `AzDomPtr`
       pub fn leak(self) -> AzDomPtr { let p = az_dom_shallow_copy(&self.ptr); std::mem::forget(self); p }
    }

    impl Drop for Dom { fn drop(&mut self) { az_dom_delete(&mut self.ptr); } }


    /// `EventFilter` struct
    pub struct EventFilter { pub(crate) object: AzEventFilter }

    impl EventFilter {
        pub fn hover(variant_data: crate::dom::HoverEventFilter) -> Self { az_event_filter_hover(variant_data.leak())}
        pub fn not(variant_data: crate::dom::NotEventFilter) -> Self { az_event_filter_not(variant_data.leak())}
        pub fn focus(variant_data: crate::dom::FocusEventFilter) -> Self { az_event_filter_focus(variant_data.leak())}
        pub fn window(variant_data: crate::dom::WindowEventFilter) -> Self { az_event_filter_window(variant_data.leak())}
    }

    impl Drop for EventFilter { fn drop(&mut self) { az_event_filter_delete(&mut self); } }


    /// `HoverEventFilter` struct
    pub struct HoverEventFilter { pub(crate) object: AzHoverEventFilter }

    impl HoverEventFilter {
        pub fn mouse_over() -> Self { az_hover_event_filter_mouse_over()  }
        pub fn mouse_down() -> Self { az_hover_event_filter_mouse_down()  }
        pub fn left_mouse_down() -> Self { az_hover_event_filter_left_mouse_down()  }
        pub fn right_mouse_down() -> Self { az_hover_event_filter_right_mouse_down()  }
        pub fn middle_mouse_down() -> Self { az_hover_event_filter_middle_mouse_down()  }
        pub fn mouse_up() -> Self { az_hover_event_filter_mouse_up()  }
        pub fn left_mouse_up() -> Self { az_hover_event_filter_left_mouse_up()  }
        pub fn right_mouse_up() -> Self { az_hover_event_filter_right_mouse_up()  }
        pub fn middle_mouse_up() -> Self { az_hover_event_filter_middle_mouse_up()  }
        pub fn mouse_enter() -> Self { az_hover_event_filter_mouse_enter()  }
        pub fn mouse_leave() -> Self { az_hover_event_filter_mouse_leave()  }
        pub fn scroll() -> Self { az_hover_event_filter_scroll()  }
        pub fn scroll_start() -> Self { az_hover_event_filter_scroll_start()  }
        pub fn scroll_end() -> Self { az_hover_event_filter_scroll_end()  }
        pub fn text_input() -> Self { az_hover_event_filter_text_input()  }
        pub fn virtual_key_down() -> Self { az_hover_event_filter_virtual_key_down()  }
        pub fn virtual_key_up() -> Self { az_hover_event_filter_virtual_key_up()  }
        pub fn hovered_file() -> Self { az_hover_event_filter_hovered_file()  }
        pub fn dropped_file() -> Self { az_hover_event_filter_dropped_file()  }
        pub fn hovered_file_cancelled() -> Self { az_hover_event_filter_hovered_file_cancelled()  }
    }

    impl Drop for HoverEventFilter { fn drop(&mut self) { az_hover_event_filter_delete(&mut self); } }


    /// `FocusEventFilter` struct
    pub struct FocusEventFilter { pub(crate) object: AzFocusEventFilter }

    impl FocusEventFilter {
        pub fn mouse_over() -> Self { az_focus_event_filter_mouse_over()  }
        pub fn mouse_down() -> Self { az_focus_event_filter_mouse_down()  }
        pub fn left_mouse_down() -> Self { az_focus_event_filter_left_mouse_down()  }
        pub fn right_mouse_down() -> Self { az_focus_event_filter_right_mouse_down()  }
        pub fn middle_mouse_down() -> Self { az_focus_event_filter_middle_mouse_down()  }
        pub fn mouse_up() -> Self { az_focus_event_filter_mouse_up()  }
        pub fn left_mouse_up() -> Self { az_focus_event_filter_left_mouse_up()  }
        pub fn right_mouse_up() -> Self { az_focus_event_filter_right_mouse_up()  }
        pub fn middle_mouse_up() -> Self { az_focus_event_filter_middle_mouse_up()  }
        pub fn mouse_enter() -> Self { az_focus_event_filter_mouse_enter()  }
        pub fn mouse_leave() -> Self { az_focus_event_filter_mouse_leave()  }
        pub fn scroll() -> Self { az_focus_event_filter_scroll()  }
        pub fn scroll_start() -> Self { az_focus_event_filter_scroll_start()  }
        pub fn scroll_end() -> Self { az_focus_event_filter_scroll_end()  }
        pub fn text_input() -> Self { az_focus_event_filter_text_input()  }
        pub fn virtual_key_down() -> Self { az_focus_event_filter_virtual_key_down()  }
        pub fn virtual_key_up() -> Self { az_focus_event_filter_virtual_key_up()  }
        pub fn focus_received() -> Self { az_focus_event_filter_focus_received()  }
        pub fn focus_lost() -> Self { az_focus_event_filter_focus_lost()  }
    }

    impl Drop for FocusEventFilter { fn drop(&mut self) { az_focus_event_filter_delete(&mut self); } }


    /// `NotEventFilter` struct
    pub struct NotEventFilter { pub(crate) object: AzNotEventFilter }

    impl NotEventFilter {
        pub fn hover(variant_data: crate::dom::HoverEventFilter) -> Self { az_not_event_filter_hover(variant_data.leak())}
        pub fn focus(variant_data: crate::dom::FocusEventFilter) -> Self { az_not_event_filter_focus(variant_data.leak())}
    }

    impl Drop for NotEventFilter { fn drop(&mut self) { az_not_event_filter_delete(&mut self); } }


    /// `WindowEventFilter` struct
    pub struct WindowEventFilter { pub(crate) object: AzWindowEventFilter }

    impl WindowEventFilter {
        pub fn mouse_over() -> Self { az_window_event_filter_mouse_over()  }
        pub fn mouse_down() -> Self { az_window_event_filter_mouse_down()  }
        pub fn left_mouse_down() -> Self { az_window_event_filter_left_mouse_down()  }
        pub fn right_mouse_down() -> Self { az_window_event_filter_right_mouse_down()  }
        pub fn middle_mouse_down() -> Self { az_window_event_filter_middle_mouse_down()  }
        pub fn mouse_up() -> Self { az_window_event_filter_mouse_up()  }
        pub fn left_mouse_up() -> Self { az_window_event_filter_left_mouse_up()  }
        pub fn right_mouse_up() -> Self { az_window_event_filter_right_mouse_up()  }
        pub fn middle_mouse_up() -> Self { az_window_event_filter_middle_mouse_up()  }
        pub fn mouse_enter() -> Self { az_window_event_filter_mouse_enter()  }
        pub fn mouse_leave() -> Self { az_window_event_filter_mouse_leave()  }
        pub fn scroll() -> Self { az_window_event_filter_scroll()  }
        pub fn scroll_start() -> Self { az_window_event_filter_scroll_start()  }
        pub fn scroll_end() -> Self { az_window_event_filter_scroll_end()  }
        pub fn text_input() -> Self { az_window_event_filter_text_input()  }
        pub fn virtual_key_down() -> Self { az_window_event_filter_virtual_key_down()  }
        pub fn virtual_key_up() -> Self { az_window_event_filter_virtual_key_up()  }
        pub fn hovered_file() -> Self { az_window_event_filter_hovered_file()  }
        pub fn dropped_file() -> Self { az_window_event_filter_dropped_file()  }
        pub fn hovered_file_cancelled() -> Self { az_window_event_filter_hovered_file_cancelled()  }
    }

    impl Drop for WindowEventFilter { fn drop(&mut self) { az_window_event_filter_delete(&mut self); } }


    /// `TabIndex` struct
    pub struct TabIndex { pub(crate) object: AzTabIndex }

    impl TabIndex {
        /// Automatic tab index, similar to simply setting `focusable = "true"` or `tabindex = 0`, (both have the effect of making the element focusable)
        pub fn auto() -> Self { az_tab_index_auto()  }
        ///  Set the tab index in relation to its parent element (`tabindex = n`)
        pub fn override_in_parent(variant_data: usize) -> Self { az_tab_index_override_in_parent(variant_data)}
        /// Elements can be focused in callbacks, but are not accessible via keyboard / tab navigation (`tabindex = -1` ) 
        pub fn no_keyboard_focus() -> Self { az_tab_index_no_keyboard_focus()  }
    }

    impl Drop for TabIndex { fn drop(&mut self) { az_tab_index_delete(&mut self); } }
}

/// Struct definition for image / font / text IDs
#[allow(dead_code, unused_imports)]
pub mod resources {

    use crate::dll::*;
    use crate::vec::U8Vec;


    /// `TextId` struct
    pub struct TextId { pub(crate) object: AzTextId }

    impl TextId {
        /// Creates a new, unique `TextId`
        pub fn new() -> Self { az_text_id_new() }
    }

    impl Drop for TextId { fn drop(&mut self) { az_text_id_delete(&mut self); } }


    /// `ImageId` struct
    pub struct ImageId { pub(crate) object: AzImageId }

    impl ImageId {
        /// Creates a new, unique `ImageId`
        pub fn new() -> Self { az_image_id_new() }
    }

    impl Drop for ImageId { fn drop(&mut self) { az_image_id_delete(&mut self); } }


    /// `FontId` struct
    pub struct FontId { pub(crate) object: AzFontId }

    impl FontId {
        /// Creates a new, unique `FontId`
        pub fn new() -> Self { az_font_id_new() }
    }

    impl Drop for FontId { fn drop(&mut self) { az_font_id_delete(&mut self); } }


    /// `ImageSource` struct
    pub struct ImageSource { pub(crate) object: AzImageSource }

    impl ImageSource {
        /// Bytes of the image, encoded in PNG / JPG / etc. format
        pub fn embedded(variant_data: crate::vec::U8Vec) -> Self { az_image_source_embedded(variant_data.leak())}
        /// References an (encoded!) image as a file from the file system that is loaded when necessary
        pub fn file(variant_data: crate::str::String) -> Self { az_image_source_file(variant_data.leak())}
        /// References a decoded (!) `RawImage` as the image source
        pub fn raw(variant_data: crate::resources::RawImage) -> Self { az_image_source_raw(variant_data.leak())}
    }

    impl Drop for ImageSource { fn drop(&mut self) { az_image_source_delete(&mut self); } }


    /// `FontSource` struct
    pub struct FontSource { pub(crate) object: AzFontSource }

    impl FontSource {
        /// Bytes are the bytes of the font file
        pub fn embedded(variant_data: crate::vec::U8Vec) -> Self { az_font_source_embedded(variant_data.leak())}
        /// References a font from a file path, which is loaded when necessary
        pub fn file(variant_data: crate::str::String) -> Self { az_font_source_file(variant_data.leak())}
        /// References a font from from a system font identifier, such as `"Arial"` or `"Helvetica"`
        pub fn system(variant_data: crate::str::String) -> Self { az_font_source_system(variant_data.leak())}
    }

    impl Drop for FontSource { fn drop(&mut self) { az_font_source_delete(&mut self); } }


    /// `RawImage` struct
    pub struct RawImage { pub(crate) object: AzRawImage }

    impl RawImage {
        /// Creates a new `RawImage` by loading the decoded bytes
        pub fn new(decoded_pixels: U8Vec, width: usize, height: usize, data_format: RawImageFormat) -> Self { az_raw_image_new(decoded_pixels, width, height, data_format) }
    }

    impl Drop for RawImage { fn drop(&mut self) { az_raw_image_delete(&mut self); } }


    /// `RawImageFormat` struct
    pub struct RawImageFormat { pub(crate) object: AzRawImageFormat }

    impl RawImageFormat {
        /// Bytes are in the R-unsinged-8bit format
        pub fn r8() -> Self { az_raw_image_format_r8()  }
        /// Bytes are in the R-unsinged-16bit format
        pub fn r16() -> Self { az_raw_image_format_r16()  }
        /// Bytes are in the RG-unsinged-16bit format
        pub fn rg16() -> Self { az_raw_image_format_rg16()  }
        /// Bytes are in the BRGA-unsigned-8bit format
        pub fn bgra8() -> Self { az_raw_image_format_bgra8()  }
        /// Bytes are in the RGBA-floating-point-32bit format
        pub fn rgbaf32() -> Self { az_raw_image_format_rgbaf32()  }
        /// Bytes are in the RG-unsigned-8bit format
        pub fn rg8() -> Self { az_raw_image_format_rg8()  }
        /// Bytes are in the RGBA-signed-32bit format
        pub fn rgbai32() -> Self { az_raw_image_format_rgbai32()  }
        /// Bytes are in the RGBA-unsigned-8bit format
        pub fn rgba8() -> Self { az_raw_image_format_rgba8()  }
    }

    impl Drop for RawImageFormat { fn drop(&mut self) { az_raw_image_format_delete(&mut self); } }
}

/// Window creation / startup configuration
#[allow(dead_code, unused_imports)]
pub mod window {

    use crate::dll::*;
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

