    #![allow(dead_code, unused_imports)]
    //! `Css` parsing module
    use crate::dll::*;
    use core::ffi::c_void;
    macro_rules! css_property_from_type {($prop_type:expr, $content_type:ident) => ({
        match $prop_type {
            CssPropertyType::TextColor => CssProperty::TextColor(StyleTextColorValue::$content_type),
            CssPropertyType::FontSize => CssProperty::FontSize(StyleFontSizeValue::$content_type),
            CssPropertyType::FontFamily => CssProperty::FontFamily(StyleFontFamilyValue::$content_type),
            CssPropertyType::TextAlign => CssProperty::TextAlign(StyleTextAlignmentHorzValue::$content_type),
            CssPropertyType::LetterSpacing => CssProperty::LetterSpacing(StyleLetterSpacingValue::$content_type),
            CssPropertyType::LineHeight => CssProperty::LineHeight(StyleLineHeightValue::$content_type),
            CssPropertyType::WordSpacing => CssProperty::WordSpacing(StyleWordSpacingValue::$content_type),
            CssPropertyType::TabWidth => CssProperty::TabWidth(StyleTabWidthValue::$content_type),
            CssPropertyType::Cursor => CssProperty::Cursor(StyleCursorValue::$content_type),
            CssPropertyType::Display => CssProperty::Display(LayoutDisplayValue::$content_type),
            CssPropertyType::Float => CssProperty::Float(LayoutFloatValue::$content_type),
            CssPropertyType::BoxSizing => CssProperty::BoxSizing(LayoutBoxSizingValue::$content_type),
            CssPropertyType::Width => CssProperty::Width(LayoutWidthValue::$content_type),
            CssPropertyType::Height => CssProperty::Height(LayoutHeightValue::$content_type),
            CssPropertyType::MinWidth => CssProperty::MinWidth(LayoutMinWidthValue::$content_type),
            CssPropertyType::MinHeight => CssProperty::MinHeight(LayoutMinHeightValue::$content_type),
            CssPropertyType::MaxWidth => CssProperty::MaxWidth(LayoutMaxWidthValue::$content_type),
            CssPropertyType::MaxHeight => CssProperty::MaxHeight(LayoutMaxHeightValue::$content_type),
            CssPropertyType::Position => CssProperty::Position(LayoutPositionValue::$content_type),
            CssPropertyType::Top => CssProperty::Top(LayoutTopValue::$content_type),
            CssPropertyType::Right => CssProperty::Right(LayoutRightValue::$content_type),
            CssPropertyType::Left => CssProperty::Left(LayoutLeftValue::$content_type),
            CssPropertyType::Bottom => CssProperty::Bottom(LayoutBottomValue::$content_type),
            CssPropertyType::FlexWrap => CssProperty::FlexWrap(LayoutWrapValue::$content_type),
            CssPropertyType::FlexDirection => CssProperty::FlexDirection(LayoutFlexDirectionValue::$content_type),
            CssPropertyType::FlexGrow => CssProperty::FlexGrow(LayoutFlexGrowValue::$content_type),
            CssPropertyType::FlexShrink => CssProperty::FlexShrink(LayoutFlexShrinkValue::$content_type),
            CssPropertyType::JustifyContent => CssProperty::JustifyContent(LayoutJustifyContentValue::$content_type),
            CssPropertyType::AlignItems => CssProperty::AlignItems(LayoutAlignItemsValue::$content_type),
            CssPropertyType::AlignContent => CssProperty::AlignContent(LayoutAlignContentValue::$content_type),
            CssPropertyType::Background => CssProperty::BackgroundContent(StyleBackgroundContentValue::$content_type),
            CssPropertyType::BackgroundImage => CssProperty::BackgroundContent(StyleBackgroundContentValue::$content_type),
            CssPropertyType::BackgroundColor => CssProperty::BackgroundContent(StyleBackgroundContentValue::$content_type),
            CssPropertyType::BackgroundPosition => CssProperty::BackgroundPosition(StyleBackgroundPositionValue::$content_type),
            CssPropertyType::BackgroundSize => CssProperty::BackgroundSize(StyleBackgroundSizeValue::$content_type),
            CssPropertyType::BackgroundRepeat => CssProperty::BackgroundRepeat(StyleBackgroundRepeatValue::$content_type),
            CssPropertyType::OverflowX => CssProperty::OverflowX(OverflowValue::$content_type),
            CssPropertyType::OverflowY => CssProperty::OverflowY(OverflowValue::$content_type),
            CssPropertyType::PaddingTop => CssProperty::PaddingTop(LayoutPaddingTopValue::$content_type),
            CssPropertyType::PaddingLeft => CssProperty::PaddingLeft(LayoutPaddingLeftValue::$content_type),
            CssPropertyType::PaddingRight => CssProperty::PaddingRight(LayoutPaddingRightValue::$content_type),
            CssPropertyType::PaddingBottom => CssProperty::PaddingBottom(LayoutPaddingBottomValue::$content_type),
            CssPropertyType::MarginTop => CssProperty::MarginTop(LayoutMarginTopValue::$content_type),
            CssPropertyType::MarginLeft => CssProperty::MarginLeft(LayoutMarginLeftValue::$content_type),
            CssPropertyType::MarginRight => CssProperty::MarginRight(LayoutMarginRightValue::$content_type),
            CssPropertyType::MarginBottom => CssProperty::MarginBottom(LayoutMarginBottomValue::$content_type),
            CssPropertyType::BorderTopLeftRadius => CssProperty::BorderTopLeftRadius(StyleBorderTopLeftRadiusValue::$content_type),
            CssPropertyType::BorderTopRightRadius => CssProperty::BorderTopRightRadius(StyleBorderTopRightRadiusValue::$content_type),
            CssPropertyType::BorderBottomLeftRadius => CssProperty::BorderBottomLeftRadius(StyleBorderBottomLeftRadiusValue::$content_type),
            CssPropertyType::BorderBottomRightRadius => CssProperty::BorderBottomRightRadius(StyleBorderBottomRightRadiusValue::$content_type),
            CssPropertyType::BorderTopColor => CssProperty::BorderTopColor(StyleBorderTopColorValue::$content_type),
            CssPropertyType::BorderRightColor => CssProperty::BorderRightColor(StyleBorderRightColorValue::$content_type),
            CssPropertyType::BorderLeftColor => CssProperty::BorderLeftColor(StyleBorderLeftColorValue::$content_type),
            CssPropertyType::BorderBottomColor => CssProperty::BorderBottomColor(StyleBorderBottomColorValue::$content_type),
            CssPropertyType::BorderTopStyle => CssProperty::BorderTopStyle(StyleBorderTopStyleValue::$content_type),
            CssPropertyType::BorderRightStyle => CssProperty::BorderRightStyle(StyleBorderRightStyleValue::$content_type),
            CssPropertyType::BorderLeftStyle => CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::$content_type),
            CssPropertyType::BorderBottomStyle => CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::$content_type),
            CssPropertyType::BorderTopWidth => CssProperty::BorderTopWidth(StyleBorderTopWidthValue::$content_type),
            CssPropertyType::BorderRightWidth => CssProperty::BorderRightWidth(StyleBorderRightWidthValue::$content_type),
            CssPropertyType::BorderLeftWidth => CssProperty::BorderLeftWidth(StyleBorderLeftWidthValue::$content_type),
            CssPropertyType::BorderBottomWidth => CssProperty::BorderBottomWidth(StyleBorderBottomWidthValue::$content_type),
            CssPropertyType::BoxShadowLeft => CssProperty::BoxShadowLeft(StyleBoxShadowValue::$content_type),
            CssPropertyType::BoxShadowRight => CssProperty::BoxShadowRight(StyleBoxShadowValue::$content_type),
            CssPropertyType::BoxShadowTop => CssProperty::BoxShadowTop(StyleBoxShadowValue::$content_type),
            CssPropertyType::BoxShadowBottom => CssProperty::BoxShadowBottom(StyleBoxShadowValue::$content_type),
            CssPropertyType::Opacity => CssProperty::Opacity(StyleOpacityValue::$content_type),
            CssPropertyType::Transform => CssProperty::Transform(StyleTransformVecValue::$content_type),
            CssPropertyType::PerspectiveOrigin => CssProperty::PerspectiveOrigin(StylePerspectiveOriginValue::$content_type),
            CssPropertyType::TransformOrigin => CssProperty::TransformOrigin(StyleTransformOriginValue::$content_type),
            CssPropertyType::BackfaceVisibility => CssProperty::BackfaceVisibility(StyleBackfaceVisibilityValue::$content_type),
        }
    })}

    impl CssProperty {

        /// Return the type (key) of this property as a statically typed enum
        pub const fn get_type(&self) -> CssPropertyType {
            match &self {
                CssProperty::TextColor(_) => CssPropertyType::TextColor,
                CssProperty::FontSize(_) => CssPropertyType::FontSize,
                CssProperty::FontFamily(_) => CssPropertyType::FontFamily,
                CssProperty::TextAlign(_) => CssPropertyType::TextAlign,
                CssProperty::LetterSpacing(_) => CssPropertyType::LetterSpacing,
                CssProperty::LineHeight(_) => CssPropertyType::LineHeight,
                CssProperty::WordSpacing(_) => CssPropertyType::WordSpacing,
                CssProperty::TabWidth(_) => CssPropertyType::TabWidth,
                CssProperty::Cursor(_) => CssPropertyType::Cursor,
                CssProperty::Display(_) => CssPropertyType::Display,
                CssProperty::Float(_) => CssPropertyType::Float,
                CssProperty::BoxSizing(_) => CssPropertyType::BoxSizing,
                CssProperty::Width(_) => CssPropertyType::Width,
                CssProperty::Height(_) => CssPropertyType::Height,
                CssProperty::MinWidth(_) => CssPropertyType::MinWidth,
                CssProperty::MinHeight(_) => CssPropertyType::MinHeight,
                CssProperty::MaxWidth(_) => CssPropertyType::MaxWidth,
                CssProperty::MaxHeight(_) => CssPropertyType::MaxHeight,
                CssProperty::Position(_) => CssPropertyType::Position,
                CssProperty::Top(_) => CssPropertyType::Top,
                CssProperty::Right(_) => CssPropertyType::Right,
                CssProperty::Left(_) => CssPropertyType::Left,
                CssProperty::Bottom(_) => CssPropertyType::Bottom,
                CssProperty::FlexWrap(_) => CssPropertyType::FlexWrap,
                CssProperty::FlexDirection(_) => CssPropertyType::FlexDirection,
                CssProperty::FlexGrow(_) => CssPropertyType::FlexGrow,
                CssProperty::FlexShrink(_) => CssPropertyType::FlexShrink,
                CssProperty::JustifyContent(_) => CssPropertyType::JustifyContent,
                CssProperty::AlignItems(_) => CssPropertyType::AlignItems,
                CssProperty::AlignContent(_) => CssPropertyType::AlignContent,
                CssProperty::BackgroundContent(_) => CssPropertyType::BackgroundImage, // TODO: wrong!
                CssProperty::BackgroundPosition(_) => CssPropertyType::BackgroundPosition,
                CssProperty::BackgroundSize(_) => CssPropertyType::BackgroundSize,
                CssProperty::BackgroundRepeat(_) => CssPropertyType::BackgroundRepeat,
                CssProperty::OverflowX(_) => CssPropertyType::OverflowX,
                CssProperty::OverflowY(_) => CssPropertyType::OverflowY,
                CssProperty::PaddingTop(_) => CssPropertyType::PaddingTop,
                CssProperty::PaddingLeft(_) => CssPropertyType::PaddingLeft,
                CssProperty::PaddingRight(_) => CssPropertyType::PaddingRight,
                CssProperty::PaddingBottom(_) => CssPropertyType::PaddingBottom,
                CssProperty::MarginTop(_) => CssPropertyType::MarginTop,
                CssProperty::MarginLeft(_) => CssPropertyType::MarginLeft,
                CssProperty::MarginRight(_) => CssPropertyType::MarginRight,
                CssProperty::MarginBottom(_) => CssPropertyType::MarginBottom,
                CssProperty::BorderTopLeftRadius(_) => CssPropertyType::BorderTopLeftRadius,
                CssProperty::BorderTopRightRadius(_) => CssPropertyType::BorderTopRightRadius,
                CssProperty::BorderBottomLeftRadius(_) => CssPropertyType::BorderBottomLeftRadius,
                CssProperty::BorderBottomRightRadius(_) => CssPropertyType::BorderBottomRightRadius,
                CssProperty::BorderTopColor(_) => CssPropertyType::BorderTopColor,
                CssProperty::BorderRightColor(_) => CssPropertyType::BorderRightColor,
                CssProperty::BorderLeftColor(_) => CssPropertyType::BorderLeftColor,
                CssProperty::BorderBottomColor(_) => CssPropertyType::BorderBottomColor,
                CssProperty::BorderTopStyle(_) => CssPropertyType::BorderTopStyle,
                CssProperty::BorderRightStyle(_) => CssPropertyType::BorderRightStyle,
                CssProperty::BorderLeftStyle(_) => CssPropertyType::BorderLeftStyle,
                CssProperty::BorderBottomStyle(_) => CssPropertyType::BorderBottomStyle,
                CssProperty::BorderTopWidth(_) => CssPropertyType::BorderTopWidth,
                CssProperty::BorderRightWidth(_) => CssPropertyType::BorderRightWidth,
                CssProperty::BorderLeftWidth(_) => CssPropertyType::BorderLeftWidth,
                CssProperty::BorderBottomWidth(_) => CssPropertyType::BorderBottomWidth,
                CssProperty::BoxShadowLeft(_) => CssPropertyType::BoxShadowLeft,
                CssProperty::BoxShadowRight(_) => CssPropertyType::BoxShadowRight,
                CssProperty::BoxShadowTop(_) => CssPropertyType::BoxShadowTop,
                CssProperty::BoxShadowBottom(_) => CssPropertyType::BoxShadowBottom,
                CssProperty::Opacity(_) => CssPropertyType::Opacity,
                CssProperty::Transform(_) => CssPropertyType::Transform,
                CssProperty::PerspectiveOrigin(_) => CssPropertyType::PerspectiveOrigin,
                CssProperty::TransformOrigin(_) => CssPropertyType::TransformOrigin,
                CssProperty::BackfaceVisibility(_) => CssPropertyType::BackfaceVisibility,
            }
        }

        pub const fn none(prop_type: CssPropertyType) -> Self {
            css_property_from_type!(prop_type, None)
        }

        pub const fn auto(prop_type: CssPropertyType) -> Self {
            css_property_from_type!(prop_type, Auto)
        }

        pub const fn initial(prop_type: CssPropertyType) -> Self {
            css_property_from_type!(prop_type, Initial)
        }

        pub const fn inherit(prop_type: CssPropertyType) -> Self {
            css_property_from_type!(prop_type, Inherit)
        }

        /// Creates a `text_color` CSS attribute
        pub const fn text_color(input: StyleTextColor) -> Self { CssProperty::TextColor(StyleTextColorValue::Exact(input)) }

        /// Creates a `font_size` CSS attribute
        pub const fn font_size(input: StyleFontSize) -> Self { CssProperty::FontSize(StyleFontSizeValue::Exact(input)) }

        /// Creates a `font_family` CSS attribute
        pub const fn font_family(input: StyleFontFamily) -> Self { CssProperty::FontFamily(StyleFontFamilyValue::Exact(input)) }

        /// Creates a `text_align` CSS attribute
        pub const fn text_align(input: StyleTextAlignmentHorz) -> Self { CssProperty::TextAlign(StyleTextAlignmentHorzValue::Exact(input)) }

        /// Creates a `letter_spacing` CSS attribute
        pub const fn letter_spacing(input: StyleLetterSpacing) -> Self { CssProperty::LetterSpacing(StyleLetterSpacingValue::Exact(input)) }

        /// Creates a `line_height` CSS attribute
        pub const fn line_height(input: StyleLineHeight) -> Self { CssProperty::LineHeight(StyleLineHeightValue::Exact(input)) }

        /// Creates a `word_spacing` CSS attribute
        pub const fn word_spacing(input: StyleWordSpacing) -> Self { CssProperty::WordSpacing(StyleWordSpacingValue::Exact(input)) }

        /// Creates a `tab_width` CSS attribute
        pub const fn tab_width(input: StyleTabWidth) -> Self { CssProperty::TabWidth(StyleTabWidthValue::Exact(input)) }

        /// Creates a `cursor` CSS attribute
        pub const fn cursor(input: StyleCursor) -> Self { CssProperty::Cursor(StyleCursorValue::Exact(input)) }

        /// Creates a `display` CSS attribute
        pub const fn display(input: LayoutDisplay) -> Self { CssProperty::Display(LayoutDisplayValue::Exact(input)) }

        /// Creates a `float` CSS attribute
        pub const fn float(input: LayoutFloat) -> Self { CssProperty::Float(LayoutFloatValue::Exact(input)) }

        /// Creates a `box_sizing` CSS attribute
        pub const fn box_sizing(input: LayoutBoxSizing) -> Self { CssProperty::BoxSizing(LayoutBoxSizingValue::Exact(input)) }

        /// Creates a `width` CSS attribute
        pub const fn width(input: LayoutWidth) -> Self { CssProperty::Width(LayoutWidthValue::Exact(input)) }

        /// Creates a `height` CSS attribute
        pub const fn height(input: LayoutHeight) -> Self { CssProperty::Height(LayoutHeightValue::Exact(input)) }

        /// Creates a `min_width` CSS attribute
        pub const fn min_width(input: LayoutMinWidth) -> Self { CssProperty::MinWidth(LayoutMinWidthValue::Exact(input)) }

        /// Creates a `min_height` CSS attribute
        pub const fn min_height(input: LayoutMinHeight) -> Self { CssProperty::MinHeight(LayoutMinHeightValue::Exact(input)) }

        /// Creates a `max_width` CSS attribute
        pub const fn max_width(input: LayoutMaxWidth) -> Self { CssProperty::MaxWidth(LayoutMaxWidthValue::Exact(input)) }

        /// Creates a `max_height` CSS attribute
        pub const fn max_height(input: LayoutMaxHeight) -> Self { CssProperty::MaxHeight(LayoutMaxHeightValue::Exact(input)) }

        /// Creates a `position` CSS attribute
        pub const fn position(input: LayoutPosition) -> Self { CssProperty::Position(LayoutPositionValue::Exact(input)) }

        /// Creates a `top` CSS attribute
        pub const fn top(input: LayoutTop) -> Self { CssProperty::Top(LayoutTopValue::Exact(input)) }

        /// Creates a `right` CSS attribute
        pub const fn right(input: LayoutRight) -> Self { CssProperty::Right(LayoutRightValue::Exact(input)) }

        /// Creates a `left` CSS attribute
        pub const fn left(input: LayoutLeft) -> Self { CssProperty::Left(LayoutLeftValue::Exact(input)) }

        /// Creates a `bottom` CSS attribute
        pub const fn bottom(input: LayoutBottom) -> Self { CssProperty::Bottom(LayoutBottomValue::Exact(input)) }

        /// Creates a `flex_wrap` CSS attribute
        pub const fn flex_wrap(input: LayoutWrap) -> Self { CssProperty::FlexWrap(LayoutWrapValue::Exact(input)) }

        /// Creates a `flex_direction` CSS attribute
        pub const fn flex_direction(input: LayoutFlexDirection) -> Self { CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(input)) }

        /// Creates a `flex_grow` CSS attribute
        pub const fn flex_grow(input: LayoutFlexGrow) -> Self { CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(input)) }

        /// Creates a `flex_shrink` CSS attribute
        pub const fn flex_shrink(input: LayoutFlexShrink) -> Self { CssProperty::FlexShrink(LayoutFlexShrinkValue::Exact(input)) }

        /// Creates a `justify_content` CSS attribute
        pub const fn justify_content(input: LayoutJustifyContent) -> Self { CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(input)) }

        /// Creates a `align_items` CSS attribute
        pub const fn align_items(input: LayoutAlignItems) -> Self { CssProperty::AlignItems(LayoutAlignItemsValue::Exact(input)) }

        /// Creates a `align_content` CSS attribute
        pub const fn align_content(input: LayoutAlignContent) -> Self { CssProperty::AlignContent(LayoutAlignContentValue::Exact(input)) }

        /// Creates a `background_content` CSS attribute
        pub const fn background_content(input: StyleBackgroundContent) -> Self { CssProperty::BackgroundContent(StyleBackgroundContentValue::Exact(input)) }

        /// Creates a `background_position` CSS attribute
        pub const fn background_position(input: StyleBackgroundPosition) -> Self { CssProperty::BackgroundPosition(StyleBackgroundPositionValue::Exact(input)) }

        /// Creates a `background_size` CSS attribute
        pub const fn background_size(input: StyleBackgroundSize) -> Self { CssProperty::BackgroundSize(StyleBackgroundSizeValue::Exact(input)) }

        /// Creates a `background_repeat` CSS attribute
        pub const fn background_repeat(input: StyleBackgroundRepeat) -> Self { CssProperty::BackgroundRepeat(StyleBackgroundRepeatValue::Exact(input)) }

        /// Creates a `overflow_x` CSS attribute
        pub const fn overflow_x(input: Overflow) -> Self { CssProperty::OverflowX(OverflowValue::Exact(input)) }

        /// Creates a `overflow_y` CSS attribute
        pub const fn overflow_y(input: Overflow) -> Self { CssProperty::OverflowY(OverflowValue::Exact(input)) }

        /// Creates a `padding_top` CSS attribute
        pub const fn padding_top(input: LayoutPaddingTop) -> Self { CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(input)) }

        /// Creates a `padding_left` CSS attribute
        pub const fn padding_left(input: LayoutPaddingLeft) -> Self { CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(input)) }

        /// Creates a `padding_right` CSS attribute
        pub const fn padding_right(input: LayoutPaddingRight) -> Self { CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(input)) }

        /// Creates a `padding_bottom` CSS attribute
        pub const fn padding_bottom(input: LayoutPaddingBottom) -> Self { CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(input)) }

        /// Creates a `margin_top` CSS attribute
        pub const fn margin_top(input: LayoutMarginTop) -> Self { CssProperty::MarginTop(LayoutMarginTopValue::Exact(input)) }

        /// Creates a `margin_left` CSS attribute
        pub const fn margin_left(input: LayoutMarginLeft) -> Self { CssProperty::MarginLeft(LayoutMarginLeftValue::Exact(input)) }

        /// Creates a `margin_right` CSS attribute
        pub const fn margin_right(input: LayoutMarginRight) -> Self { CssProperty::MarginRight(LayoutMarginRightValue::Exact(input)) }

        /// Creates a `margin_bottom` CSS attribute
        pub const fn margin_bottom(input: LayoutMarginBottom) -> Self { CssProperty::MarginBottom(LayoutMarginBottomValue::Exact(input)) }

        /// Creates a `border_top_left_radius` CSS attribute
        pub const fn border_top_left_radius(input: StyleBorderTopLeftRadius) -> Self { CssProperty::BorderTopLeftRadius(StyleBorderTopLeftRadiusValue::Exact(input)) }

        /// Creates a `border_top_right_radius` CSS attribute
        pub const fn border_top_right_radius(input: StyleBorderTopRightRadius) -> Self { CssProperty::BorderTopRightRadius(StyleBorderTopRightRadiusValue::Exact(input)) }

        /// Creates a `border_bottom_left_radius` CSS attribute
        pub const fn border_bottom_left_radius(input: StyleBorderBottomLeftRadius) -> Self { CssProperty::BorderBottomLeftRadius(StyleBorderBottomLeftRadiusValue::Exact(input)) }

        /// Creates a `border_bottom_right_radius` CSS attribute
        pub const fn border_bottom_right_radius(input: StyleBorderBottomRightRadius) -> Self { CssProperty::BorderBottomRightRadius(StyleBorderBottomRightRadiusValue::Exact(input)) }

        /// Creates a `border_top_color` CSS attribute
        pub const fn border_top_color(input: StyleBorderTopColor) -> Self { CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(input)) }

        /// Creates a `border_right_color` CSS attribute
        pub const fn border_right_color(input: StyleBorderRightColor) -> Self { CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(input)) }

        /// Creates a `border_left_color` CSS attribute
        pub const fn border_left_color(input: StyleBorderLeftColor) -> Self { CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(input)) }

        /// Creates a `border_bottom_color` CSS attribute
        pub const fn border_bottom_color(input: StyleBorderBottomColor) -> Self { CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(input)) }

        /// Creates a `border_top_style` CSS attribute
        pub const fn border_top_style(input: StyleBorderTopStyle) -> Self { CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(input)) }

        /// Creates a `border_right_style` CSS attribute
        pub const fn border_right_style(input: StyleBorderRightStyle) -> Self { CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(input)) }

        /// Creates a `border_left_style` CSS attribute
        pub const fn border_left_style(input: StyleBorderLeftStyle) -> Self { CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(input)) }

        /// Creates a `border_bottom_style` CSS attribute
        pub const fn border_bottom_style(input: StyleBorderBottomStyle) -> Self { CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(input)) }

        /// Creates a `border_top_width` CSS attribute
        pub const fn border_top_width(input: StyleBorderTopWidth) -> Self { CssProperty::BorderTopWidth(StyleBorderTopWidthValue::Exact(input)) }

        /// Creates a `border_right_width` CSS attribute
        pub const fn border_right_width(input: StyleBorderRightWidth) -> Self { CssProperty::BorderRightWidth(StyleBorderRightWidthValue::Exact(input)) }

        /// Creates a `border_left_width` CSS attribute
        pub const fn border_left_width(input: StyleBorderLeftWidth) -> Self { CssProperty::BorderLeftWidth(StyleBorderLeftWidthValue::Exact(input)) }

        /// Creates a `border_bottom_width` CSS attribute
        pub const fn border_bottom_width(input: StyleBorderBottomWidth) -> Self { CssProperty::BorderBottomWidth(StyleBorderBottomWidthValue::Exact(input)) }

        /// Creates a `box_shadow_left` CSS attribute
        pub const fn box_shadow_left(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(input)) }

        /// Creates a `box_shadow_right` CSS attribute
        pub const fn box_shadow_right(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(input)) }

        /// Creates a `box_shadow_top` CSS attribute
        pub const fn box_shadow_top(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(input)) }

        /// Creates a `box_shadow_bottom` CSS attribute
        pub const fn box_shadow_bottom(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(input)) }

        /// Creates a `opacity` CSS attribute
        pub const fn opacity(input: StyleOpacity) -> Self { CssProperty::Opacity(StyleOpacityValue::Exact(input)) }

        /// Creates a `transform` CSS attribute
        pub const fn transform(input: crate::vec::StyleTransformVec) -> Self { CssProperty::Transform(StyleTransformVecValue::Exact(input)) }

        /// Creates a `transform-origin` CSS attribute
        pub const fn transform_origin(input: StyleTransformOrigin) -> Self { CssProperty::TransformOrigin(StyleTransformOriginValue::Exact(input)) }

        /// Creates a `perspective-origin` CSS attribute
        pub const fn perspective_origin(input: StylePerspectiveOrigin) -> Self { CssProperty::PerspectiveOrigin(StylePerspectiveOriginValue::Exact(input)) }

        /// Creates a `backface-visibility` CSS attribute
        pub const fn backface_visiblity(input: StyleBackfaceVisibility) -> Self { CssProperty::BackfaceVisibility(StyleBackfaceVisibilityValue::Exact(input)) }
    }

    const FP_PRECISION_MULTIPLIER: f32 = 1000.0;
    const FP_PRECISION_MULTIPLIER_CONST: isize = FP_PRECISION_MULTIPLIER as isize;

    impl FloatValue {
        /// Same as `FloatValue::new()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        pub const fn const_new(value: isize)  -> Self {
            Self { number: value * FP_PRECISION_MULTIPLIER_CONST }
        }

        pub fn new(value: f32) -> Self {
            Self { number: (value * FP_PRECISION_MULTIPLIER) as isize }
        }

        pub fn get(&self) -> f32 {
            self.number as f32 / FP_PRECISION_MULTIPLIER
        }
    }

    impl From<f32> for FloatValue {
        fn from(val: f32) -> Self {
            Self::new(val)
        }
    }

    impl PixelValue {

        #[inline]
        pub const fn zero() -> Self {
            const ZERO_PX: PixelValue = PixelValue::const_px(0);
            ZERO_PX
        }

        /// Same as `PixelValue::px()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_px(value: isize) -> Self {
            Self::const_from_metric(SizeMetric::Px, value)
        }

        /// Same as `PixelValue::em()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_em(value: isize) -> Self {
            Self::const_from_metric(SizeMetric::Em, value)
        }

        /// Same as `PixelValue::pt()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_pt(value: isize) -> Self {
            Self::const_from_metric(SizeMetric::Pt, value)
        }

        /// Same as `PixelValue::pt()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_percent(value: isize) -> Self {
            Self::const_from_metric(SizeMetric::Percent, value)
        }

        #[inline]
        pub const fn const_from_metric(metric: SizeMetric, value: isize) -> Self {
            Self {
                metric: metric,
                number: FloatValue::const_new(value),
            }
        }

        #[inline]
        pub fn px(value: f32) -> Self {
            Self::from_metric(SizeMetric::Px, value)
        }

        #[inline]
        pub fn em(value: f32) -> Self {
            Self::from_metric(SizeMetric::Em, value)
        }

        #[inline]
        pub fn pt(value: f32) -> Self {
            Self::from_metric(SizeMetric::Pt, value)
        }

        #[inline]
        pub fn percent(value: f32) -> Self {
            Self::from_metric(SizeMetric::Percent, value)
        }

        #[inline]
        pub fn from_metric(metric: SizeMetric, value: f32) -> Self {
            Self {
                metric: metric,
                number: FloatValue::new(value),
            }
        }
    }

    impl PixelValueNoPercent {

        #[inline]
        pub const fn zero() -> Self {
            Self { inner: PixelValue::zero() }
        }

        /// Same as `PixelValueNoPercent::px()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_px(value: isize) -> Self {
            Self { inner: PixelValue::const_px(value) }
        }

        /// Same as `PixelValueNoPercent::em()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_em(value: isize) -> Self {
            Self { inner: PixelValue::const_em(value) }
        }

        /// Same as `PixelValueNoPercent::pt()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_pt(value: isize) -> Self {
            Self { inner: PixelValue::const_pt(value) }
        }

        #[inline]
        const fn const_from_metric(metric: SizeMetric, value: isize) -> Self {
            Self { inner: PixelValue::const_from_metric(metric, value) }
        }

        #[inline]
        pub fn px(value: f32) -> Self {
            Self { inner: PixelValue::px(value) }
        }

        #[inline]
        pub fn em(value: f32) -> Self {
            Self { inner: PixelValue::em(value) }
        }

        #[inline]
        pub fn pt(value: f32) -> Self {
            Self { inner: PixelValue::pt(value) }
        }

        #[inline]
        fn from_metric(metric: SizeMetric, value: f32) -> Self {
            Self { inner: PixelValue::from_metric(metric, value) }
        }
    }

    /// Creates `pt`, `px` and `em` constructors for any struct that has a
    /// `PixelValue` as it's self.0 field.
    macro_rules! impl_pixel_value {($struct:ident) => (

        impl $struct {

            #[inline]
            pub const fn zero() -> Self {
                Self { inner: PixelValue::zero() }
            }

            /// Same as `PixelValue::px()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_px(value: isize) -> Self {
                Self { inner: PixelValue::const_px(value) }
            }

            /// Same as `PixelValue::em()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_em(value: isize) -> Self {
                Self { inner: PixelValue::const_em(value) }
            }

            /// Same as `PixelValue::pt()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_pt(value: isize) -> Self {
                Self { inner: PixelValue::const_pt(value) }
            }

            /// Same as `PixelValue::pt()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_percent(value: isize) -> Self {
                Self { inner: PixelValue::const_percent(value) }
            }

            #[inline]
            pub const fn const_from_metric(metric: SizeMetric, value: isize) -> Self {
                Self { inner: PixelValue::const_from_metric(metric, value) }
            }

            #[inline]
            pub fn px(value: f32) -> Self {
                Self { inner: PixelValue::px(value) }
            }

            #[inline]
            pub fn em(value: f32) -> Self {
                Self { inner: PixelValue::em(value) }
            }

            #[inline]
            pub fn pt(value: f32) -> Self {
                Self { inner: PixelValue::pt(value) }
            }

            #[inline]
            pub fn percent(value: f32) -> Self {
                Self { inner: PixelValue::percent(value) }
            }

            #[inline]
            pub fn from_metric(metric: SizeMetric, value: f32) -> Self {
                Self { inner: PixelValue::from_metric(metric, value) }
            }
        }
    )}

    impl_pixel_value!(StyleBorderTopLeftRadius);
    impl_pixel_value!(StyleBorderBottomLeftRadius);
    impl_pixel_value!(StyleBorderTopRightRadius);
    impl_pixel_value!(StyleBorderBottomRightRadius);
    impl_pixel_value!(StyleBorderTopWidth);
    impl_pixel_value!(StyleBorderLeftWidth);
    impl_pixel_value!(StyleBorderRightWidth);
    impl_pixel_value!(StyleBorderBottomWidth);
    impl_pixel_value!(LayoutWidth);
    impl_pixel_value!(LayoutHeight);
    impl_pixel_value!(LayoutMinHeight);
    impl_pixel_value!(LayoutMinWidth);
    impl_pixel_value!(LayoutMaxWidth);
    impl_pixel_value!(LayoutMaxHeight);
    impl_pixel_value!(LayoutTop);
    impl_pixel_value!(LayoutBottom);
    impl_pixel_value!(LayoutRight);
    impl_pixel_value!(LayoutLeft);
    impl_pixel_value!(LayoutPaddingTop);
    impl_pixel_value!(LayoutPaddingBottom);
    impl_pixel_value!(LayoutPaddingRight);
    impl_pixel_value!(LayoutPaddingLeft);
    impl_pixel_value!(LayoutMarginTop);
    impl_pixel_value!(LayoutMarginBottom);
    impl_pixel_value!(LayoutMarginRight);
    impl_pixel_value!(LayoutMarginLeft);
    impl_pixel_value!(StyleLetterSpacing);
    impl_pixel_value!(StyleWordSpacing);
    impl_pixel_value!(StyleFontSize);

    macro_rules! impl_float_value {($struct:ident) => (
        impl $struct {
            /// Same as `FloatValue::new()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            pub const fn const_new(value: isize)  -> Self {
                Self { inner: FloatValue::const_new(value) }
            }

            pub fn new(value: f32) -> Self {
                Self { inner: FloatValue::new(value) }
            }

            pub fn get(&self) -> f32 {
                self.inner.get()
            }
        }

        impl From<f32> for $struct {
            fn from(val: f32) -> Self {
                Self { inner: FloatValue::from(val) }
            }
        }
    )}

    impl_float_value!(LayoutFlexGrow);
    impl_float_value!(LayoutFlexShrink);
    impl_float_value!(StyleOpacity);
    use crate::str::String;


    /// `CssRuleBlock` struct
    #[doc(inline)] pub use crate::dll::AzCssRuleBlock as CssRuleBlock;

    impl Clone for CssRuleBlock { fn clone(&self) -> Self { unsafe { crate::dll::az_css_rule_block_deep_copy(self) } } }
    impl Drop for CssRuleBlock { fn drop(&mut self) { unsafe { crate::dll::az_css_rule_block_delete(self) }; } }


    /// `CssDeclaration` struct
    #[doc(inline)] pub use crate::dll::AzCssDeclaration as CssDeclaration;

    impl Clone for CssDeclaration { fn clone(&self) -> Self { unsafe { crate::dll::az_css_declaration_deep_copy(self) } } }
    impl Drop for CssDeclaration { fn drop(&mut self) { unsafe { crate::dll::az_css_declaration_delete(self) }; } }


    /// `DynamicCssProperty` struct
    #[doc(inline)] pub use crate::dll::AzDynamicCssProperty as DynamicCssProperty;

    impl Clone for DynamicCssProperty { fn clone(&self) -> Self { unsafe { crate::dll::az_dynamic_css_property_deep_copy(self) } } }
    impl Drop for DynamicCssProperty { fn drop(&mut self) { unsafe { crate::dll::az_dynamic_css_property_delete(self) }; } }


    /// `CssPath` struct
    #[doc(inline)] pub use crate::dll::AzCssPath as CssPath;

    impl Clone for CssPath { fn clone(&self) -> Self { unsafe { crate::dll::az_css_path_deep_copy(self) } } }
    impl Drop for CssPath { fn drop(&mut self) { unsafe { crate::dll::az_css_path_delete(self) }; } }


    /// `CssPathSelector` struct
    #[doc(inline)] pub use crate::dll::AzCssPathSelector as CssPathSelector;

    impl Clone for CssPathSelector { fn clone(&self) -> Self { unsafe { crate::dll::az_css_path_selector_deep_copy(self) } } }
    impl Drop for CssPathSelector { fn drop(&mut self) { unsafe { crate::dll::az_css_path_selector_delete(self) }; } }


    /// `NodeTypePath` struct
    #[doc(inline)] pub use crate::dll::AzNodeTypePath as NodeTypePath;

    impl Clone for NodeTypePath { fn clone(&self) -> Self { *self } }
    impl Copy for NodeTypePath { }


    /// `CssPathPseudoSelector` struct
    #[doc(inline)] pub use crate::dll::AzCssPathPseudoSelector as CssPathPseudoSelector;

    impl Clone for CssPathPseudoSelector { fn clone(&self) -> Self { *self } }
    impl Copy for CssPathPseudoSelector { }


    /// `CssNthChildSelector` struct
    #[doc(inline)] pub use crate::dll::AzCssNthChildSelector as CssNthChildSelector;

    impl Clone for CssNthChildSelector { fn clone(&self) -> Self { *self } }
    impl Copy for CssNthChildSelector { }


    /// `CssNthChildPattern` struct
    #[doc(inline)] pub use crate::dll::AzCssNthChildPattern as CssNthChildPattern;

    impl Clone for CssNthChildPattern { fn clone(&self) -> Self { *self } }
    impl Copy for CssNthChildPattern { }


    /// `Stylesheet` struct
    #[doc(inline)] pub use crate::dll::AzStylesheet as Stylesheet;

    impl Clone for Stylesheet { fn clone(&self) -> Self { unsafe { crate::dll::az_stylesheet_deep_copy(self) } } }
    impl Drop for Stylesheet { fn drop(&mut self) { unsafe { crate::dll::az_stylesheet_delete(self) }; } }


    /// `Css` struct
    #[doc(inline)] pub use crate::dll::AzCss as Css;

    impl Css {
        /// Returns an empty CSS style
        pub fn empty() -> Self { unsafe { crate::dll::az_css_empty() } }
        /// Returns a CSS style parsed from a `String`
        pub fn from_string(s: String) -> Self { unsafe { crate::dll::az_css_from_string(s) } }
    }

    impl Clone for Css { fn clone(&self) -> Self { unsafe { crate::dll::az_css_deep_copy(self) } } }
    impl Drop for Css { fn drop(&mut self) { unsafe { crate::dll::az_css_delete(self) }; } }


    /// `CssPropertyType` struct
    #[doc(inline)] pub use crate::dll::AzCssPropertyType as CssPropertyType;

    impl Clone for CssPropertyType { fn clone(&self) -> Self { *self } }
    impl Copy for CssPropertyType { }


    /// `ColorU` struct
    #[doc(inline)] pub use crate::dll::AzColorU as ColorU;

    impl ColorU {
        /// Creates a new `ColorU` instance.
        pub fn from_str(string: String) -> Self { unsafe { crate::dll::az_color_u_from_str(string) } }
        /// Calls the `ColorU::to_hash` function.
        pub fn to_hash(&self)  -> crate::str::String { unsafe { crate::dll::az_color_u_to_hash(self) } }
    }

    impl Clone for ColorU { fn clone(&self) -> Self { *self } }
    impl Copy for ColorU { }


    /// `SizeMetric` struct
    #[doc(inline)] pub use crate::dll::AzSizeMetric as SizeMetric;

    impl Clone for SizeMetric { fn clone(&self) -> Self { *self } }
    impl Copy for SizeMetric { }


    /// `FloatValue` struct
    #[doc(inline)] pub use crate::dll::AzFloatValue as FloatValue;

    impl Clone for FloatValue { fn clone(&self) -> Self { *self } }
    impl Copy for FloatValue { }


    /// `PixelValue` struct
    #[doc(inline)] pub use crate::dll::AzPixelValue as PixelValue;

    impl Clone for PixelValue { fn clone(&self) -> Self { *self } }
    impl Copy for PixelValue { }


    /// `PixelValueNoPercent` struct
    #[doc(inline)] pub use crate::dll::AzPixelValueNoPercent as PixelValueNoPercent;

    impl Clone for PixelValueNoPercent { fn clone(&self) -> Self { *self } }
    impl Copy for PixelValueNoPercent { }


    /// `BoxShadowClipMode` struct
    #[doc(inline)] pub use crate::dll::AzBoxShadowClipMode as BoxShadowClipMode;

    impl Clone for BoxShadowClipMode { fn clone(&self) -> Self { *self } }
    impl Copy for BoxShadowClipMode { }


    /// `StyleBoxShadow` struct
    #[doc(inline)] pub use crate::dll::AzStyleBoxShadow as StyleBoxShadow;

    impl Clone for StyleBoxShadow { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBoxShadow { }


    /// `LayoutAlignContent` struct
    #[doc(inline)] pub use crate::dll::AzLayoutAlignContent as LayoutAlignContent;

    impl Clone for LayoutAlignContent { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutAlignContent { }


    /// `LayoutAlignItems` struct
    #[doc(inline)] pub use crate::dll::AzLayoutAlignItems as LayoutAlignItems;

    impl Clone for LayoutAlignItems { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutAlignItems { }


    /// `LayoutBottom` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBottom as LayoutBottom;

    impl Clone for LayoutBottom { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBottom { }


    /// `LayoutBoxSizing` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBoxSizing as LayoutBoxSizing;

    impl Clone for LayoutBoxSizing { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBoxSizing { }


    /// `LayoutFlexDirection` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFlexDirection as LayoutFlexDirection;

    impl Clone for LayoutFlexDirection { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFlexDirection { }


    /// `LayoutDisplay` struct
    #[doc(inline)] pub use crate::dll::AzLayoutDisplay as LayoutDisplay;

    impl Clone for LayoutDisplay { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutDisplay { }


    /// `LayoutFlexGrow` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFlexGrow as LayoutFlexGrow;

    impl Clone for LayoutFlexGrow { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFlexGrow { }


    /// `LayoutFlexShrink` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFlexShrink as LayoutFlexShrink;

    impl Clone for LayoutFlexShrink { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFlexShrink { }


    /// `LayoutFloat` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFloat as LayoutFloat;

    impl Clone for LayoutFloat { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFloat { }


    /// `LayoutHeight` struct
    #[doc(inline)] pub use crate::dll::AzLayoutHeight as LayoutHeight;

    impl Clone for LayoutHeight { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutHeight { }


    /// `LayoutJustifyContent` struct
    #[doc(inline)] pub use crate::dll::AzLayoutJustifyContent as LayoutJustifyContent;

    impl Clone for LayoutJustifyContent { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutJustifyContent { }


    /// `LayoutLeft` struct
    #[doc(inline)] pub use crate::dll::AzLayoutLeft as LayoutLeft;

    impl Clone for LayoutLeft { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutLeft { }


    /// `LayoutMarginBottom` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginBottom as LayoutMarginBottom;

    impl Clone for LayoutMarginBottom { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginBottom { }


    /// `LayoutMarginLeft` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginLeft as LayoutMarginLeft;

    impl Clone for LayoutMarginLeft { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginLeft { }


    /// `LayoutMarginRight` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginRight as LayoutMarginRight;

    impl Clone for LayoutMarginRight { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginRight { }


    /// `LayoutMarginTop` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginTop as LayoutMarginTop;

    impl Clone for LayoutMarginTop { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginTop { }


    /// `LayoutMaxHeight` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMaxHeight as LayoutMaxHeight;

    impl Clone for LayoutMaxHeight { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMaxHeight { }


    /// `LayoutMaxWidth` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMaxWidth as LayoutMaxWidth;

    impl Clone for LayoutMaxWidth { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMaxWidth { }


    /// `LayoutMinHeight` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMinHeight as LayoutMinHeight;

    impl Clone for LayoutMinHeight { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMinHeight { }


    /// `LayoutMinWidth` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMinWidth as LayoutMinWidth;

    impl Clone for LayoutMinWidth { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMinWidth { }


    /// `LayoutPaddingBottom` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingBottom as LayoutPaddingBottom;

    impl Clone for LayoutPaddingBottom { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingBottom { }


    /// `LayoutPaddingLeft` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingLeft as LayoutPaddingLeft;

    impl Clone for LayoutPaddingLeft { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingLeft { }


    /// `LayoutPaddingRight` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingRight as LayoutPaddingRight;

    impl Clone for LayoutPaddingRight { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingRight { }


    /// `LayoutPaddingTop` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingTop as LayoutPaddingTop;

    impl Clone for LayoutPaddingTop { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingTop { }


    /// `LayoutPosition` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPosition as LayoutPosition;

    impl Clone for LayoutPosition { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPosition { }


    /// `LayoutRight` struct
    #[doc(inline)] pub use crate::dll::AzLayoutRight as LayoutRight;

    impl Clone for LayoutRight { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutRight { }


    /// `LayoutTop` struct
    #[doc(inline)] pub use crate::dll::AzLayoutTop as LayoutTop;

    impl Clone for LayoutTop { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutTop { }


    /// `LayoutWidth` struct
    #[doc(inline)] pub use crate::dll::AzLayoutWidth as LayoutWidth;

    impl Clone for LayoutWidth { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutWidth { }


    /// `LayoutFlexWrap` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFlexWrap as LayoutFlexWrap;

    impl Clone for LayoutFlexWrap { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFlexWrap { }


    /// `LayoutOverflow` struct
    #[doc(inline)] pub use crate::dll::AzLayoutOverflow as LayoutOverflow;

    impl Clone for LayoutOverflow { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutOverflow { }


    /// `PercentageValue` struct
    #[doc(inline)] pub use crate::dll::AzPercentageValue as PercentageValue;

    impl Clone for PercentageValue { fn clone(&self) -> Self { *self } }
    impl Copy for PercentageValue { }


    /// `AngleMetric` struct
    #[doc(inline)] pub use crate::dll::AzAngleMetric as AngleMetric;

    impl Clone for AngleMetric { fn clone(&self) -> Self { *self } }
    impl Copy for AngleMetric { }


    /// `AngleValue` struct
    #[doc(inline)] pub use crate::dll::AzAngleValue as AngleValue;

    impl Clone for AngleValue { fn clone(&self) -> Self { *self } }
    impl Copy for AngleValue { }


    /// `LinearColorStop` struct
    #[doc(inline)] pub use crate::dll::AzLinearColorStop as LinearColorStop;

    impl Clone for LinearColorStop { fn clone(&self) -> Self { *self } }
    impl Copy for LinearColorStop { }


    /// `RadialColorStop` struct
    #[doc(inline)] pub use crate::dll::AzRadialColorStop as RadialColorStop;

    impl Clone for RadialColorStop { fn clone(&self) -> Self { *self } }
    impl Copy for RadialColorStop { }


    /// `DirectionCorner` struct
    #[doc(inline)] pub use crate::dll::AzDirectionCorner as DirectionCorner;

    impl Clone for DirectionCorner { fn clone(&self) -> Self { *self } }
    impl Copy for DirectionCorner { }


    /// `DirectionCorners` struct
    #[doc(inline)] pub use crate::dll::AzDirectionCorners as DirectionCorners;

    impl Clone for DirectionCorners { fn clone(&self) -> Self { *self } }
    impl Copy for DirectionCorners { }


    /// `Direction` struct
    #[doc(inline)] pub use crate::dll::AzDirection as Direction;

    impl Clone for Direction { fn clone(&self) -> Self { *self } }
    impl Copy for Direction { }


    /// `ExtendMode` struct
    #[doc(inline)] pub use crate::dll::AzExtendMode as ExtendMode;

    impl Clone for ExtendMode { fn clone(&self) -> Self { *self } }
    impl Copy for ExtendMode { }


    /// `LinearGradient` struct
    #[doc(inline)] pub use crate::dll::AzLinearGradient as LinearGradient;

    impl Clone for LinearGradient { fn clone(&self) -> Self { unsafe { crate::dll::az_linear_gradient_deep_copy(self) } } }
    impl Drop for LinearGradient { fn drop(&mut self) { unsafe { crate::dll::az_linear_gradient_delete(self) }; } }


    /// `Shape` struct
    #[doc(inline)] pub use crate::dll::AzShape as Shape;

    impl Clone for Shape { fn clone(&self) -> Self { *self } }
    impl Copy for Shape { }


    /// `RadialGradientSize` struct
    #[doc(inline)] pub use crate::dll::AzRadialGradientSize as RadialGradientSize;

    impl Clone for RadialGradientSize { fn clone(&self) -> Self { unsafe { crate::dll::az_radial_gradient_size_deep_copy(self) } } }
    impl Drop for RadialGradientSize { fn drop(&mut self) { unsafe { crate::dll::az_radial_gradient_size_delete(self) }; } }


    /// `RadialGradient` struct
    #[doc(inline)] pub use crate::dll::AzRadialGradient as RadialGradient;

    impl Clone for RadialGradient { fn clone(&self) -> Self { unsafe { crate::dll::az_radial_gradient_deep_copy(self) } } }
    impl Drop for RadialGradient { fn drop(&mut self) { unsafe { crate::dll::az_radial_gradient_delete(self) }; } }


    /// `ConicGradient` struct
    #[doc(inline)] pub use crate::dll::AzConicGradient as ConicGradient;

    impl Clone for ConicGradient { fn clone(&self) -> Self { unsafe { crate::dll::az_conic_gradient_deep_copy(self) } } }
    impl Drop for ConicGradient { fn drop(&mut self) { unsafe { crate::dll::az_conic_gradient_delete(self) }; } }


    /// `CssImageId` struct
    #[doc(inline)] pub use crate::dll::AzCssImageId as CssImageId;

    impl Clone for CssImageId { fn clone(&self) -> Self { unsafe { crate::dll::az_css_image_id_deep_copy(self) } } }
    impl Drop for CssImageId { fn drop(&mut self) { unsafe { crate::dll::az_css_image_id_delete(self) }; } }


    /// `StyleBackgroundContent` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundContent as StyleBackgroundContent;

    impl Clone for StyleBackgroundContent { fn clone(&self) -> Self { unsafe { crate::dll::az_style_background_content_deep_copy(self) } } }
    impl Drop for StyleBackgroundContent { fn drop(&mut self) { unsafe { crate::dll::az_style_background_content_delete(self) }; } }


    /// `BackgroundPositionHorizontal` struct
    #[doc(inline)] pub use crate::dll::AzBackgroundPositionHorizontal as BackgroundPositionHorizontal;

    impl Clone for BackgroundPositionHorizontal { fn clone(&self) -> Self { *self } }
    impl Copy for BackgroundPositionHorizontal { }


    /// `BackgroundPositionVertical` struct
    #[doc(inline)] pub use crate::dll::AzBackgroundPositionVertical as BackgroundPositionVertical;

    impl Clone for BackgroundPositionVertical { fn clone(&self) -> Self { *self } }
    impl Copy for BackgroundPositionVertical { }


    /// `StyleBackgroundPosition` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundPosition as StyleBackgroundPosition;

    impl Clone for StyleBackgroundPosition { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackgroundPosition { }


    /// `StyleBackgroundRepeat` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundRepeat as StyleBackgroundRepeat;

    impl Clone for StyleBackgroundRepeat { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackgroundRepeat { }


    /// `StyleBackgroundSize` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundSize as StyleBackgroundSize;

    impl Clone for StyleBackgroundSize { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackgroundSize { }


    /// `StyleBorderBottomColor` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomColor as StyleBorderBottomColor;

    impl Clone for StyleBorderBottomColor { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomColor { }


    /// `StyleBorderBottomLeftRadius` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomLeftRadius as StyleBorderBottomLeftRadius;

    impl Clone for StyleBorderBottomLeftRadius { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomLeftRadius { }


    /// `StyleBorderBottomRightRadius` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomRightRadius as StyleBorderBottomRightRadius;

    impl Clone for StyleBorderBottomRightRadius { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomRightRadius { }


    /// `BorderStyle` struct
    #[doc(inline)] pub use crate::dll::AzBorderStyle as BorderStyle;

    impl Clone for BorderStyle { fn clone(&self) -> Self { *self } }
    impl Copy for BorderStyle { }


    /// `StyleBorderBottomStyle` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomStyle as StyleBorderBottomStyle;

    impl Clone for StyleBorderBottomStyle { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomStyle { }


    /// `LayoutBorderBottomWidth` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBorderBottomWidth as LayoutBorderBottomWidth;

    impl Clone for LayoutBorderBottomWidth { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBorderBottomWidth { }


    /// `StyleBorderLeftColor` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderLeftColor as StyleBorderLeftColor;

    impl Clone for StyleBorderLeftColor { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderLeftColor { }


    /// `StyleBorderLeftStyle` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderLeftStyle as StyleBorderLeftStyle;

    impl Clone for StyleBorderLeftStyle { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderLeftStyle { }


    /// `LayoutBorderLeftWidth` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBorderLeftWidth as LayoutBorderLeftWidth;

    impl Clone for LayoutBorderLeftWidth { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBorderLeftWidth { }


    /// `StyleBorderRightColor` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderRightColor as StyleBorderRightColor;

    impl Clone for StyleBorderRightColor { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderRightColor { }


    /// `StyleBorderRightStyle` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderRightStyle as StyleBorderRightStyle;

    impl Clone for StyleBorderRightStyle { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderRightStyle { }


    /// `LayoutBorderRightWidth` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBorderRightWidth as LayoutBorderRightWidth;

    impl Clone for LayoutBorderRightWidth { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBorderRightWidth { }


    /// `StyleBorderTopColor` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopColor as StyleBorderTopColor;

    impl Clone for StyleBorderTopColor { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopColor { }


    /// `StyleBorderTopLeftRadius` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopLeftRadius as StyleBorderTopLeftRadius;

    impl Clone for StyleBorderTopLeftRadius { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopLeftRadius { }


    /// `StyleBorderTopRightRadius` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopRightRadius as StyleBorderTopRightRadius;

    impl Clone for StyleBorderTopRightRadius { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopRightRadius { }


    /// `StyleBorderTopStyle` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopStyle as StyleBorderTopStyle;

    impl Clone for StyleBorderTopStyle { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopStyle { }


    /// `LayoutBorderTopWidth` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBorderTopWidth as LayoutBorderTopWidth;

    impl Clone for LayoutBorderTopWidth { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBorderTopWidth { }


    /// `ScrollbarInfo` struct
    #[doc(inline)] pub use crate::dll::AzScrollbarInfo as ScrollbarInfo;

    impl Clone for ScrollbarInfo { fn clone(&self) -> Self { *self } }
    impl Copy for ScrollbarInfo { }


    /// `ScrollbarStyle` struct
    #[doc(inline)] pub use crate::dll::AzScrollbarStyle as ScrollbarStyle;

    impl Clone for ScrollbarStyle { fn clone(&self) -> Self { *self } }
    impl Copy for ScrollbarStyle { }


    /// `StyleCursor` struct
    #[doc(inline)] pub use crate::dll::AzStyleCursor as StyleCursor;

    impl Clone for StyleCursor { fn clone(&self) -> Self { *self } }
    impl Copy for StyleCursor { }


    /// `StyleFontFamily` struct
    #[doc(inline)] pub use crate::dll::AzStyleFontFamily as StyleFontFamily;

    impl Clone for StyleFontFamily { fn clone(&self) -> Self { unsafe { crate::dll::az_style_font_family_deep_copy(self) } } }
    impl Drop for StyleFontFamily { fn drop(&mut self) { unsafe { crate::dll::az_style_font_family_delete(self) }; } }


    /// `StyleFontSize` struct
    #[doc(inline)] pub use crate::dll::AzStyleFontSize as StyleFontSize;

    impl Clone for StyleFontSize { fn clone(&self) -> Self { *self } }
    impl Copy for StyleFontSize { }


    /// `StyleLetterSpacing` struct
    #[doc(inline)] pub use crate::dll::AzStyleLetterSpacing as StyleLetterSpacing;

    impl Clone for StyleLetterSpacing { fn clone(&self) -> Self { *self } }
    impl Copy for StyleLetterSpacing { }


    /// `StyleLineHeight` struct
    #[doc(inline)] pub use crate::dll::AzStyleLineHeight as StyleLineHeight;

    impl Clone for StyleLineHeight { fn clone(&self) -> Self { *self } }
    impl Copy for StyleLineHeight { }


    /// `StyleTabWidth` struct
    #[doc(inline)] pub use crate::dll::AzStyleTabWidth as StyleTabWidth;

    impl Clone for StyleTabWidth { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTabWidth { }


    /// `StyleOpacity` struct
    #[doc(inline)] pub use crate::dll::AzStyleOpacity as StyleOpacity;

    impl Clone for StyleOpacity { fn clone(&self) -> Self { *self } }
    impl Copy for StyleOpacity { }


    /// `StyleTransformOrigin` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformOrigin as StyleTransformOrigin;

    impl Clone for StyleTransformOrigin { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformOrigin { }


    /// `StylePerspectiveOrigin` struct
    #[doc(inline)] pub use crate::dll::AzStylePerspectiveOrigin as StylePerspectiveOrigin;

    impl Clone for StylePerspectiveOrigin { fn clone(&self) -> Self { *self } }
    impl Copy for StylePerspectiveOrigin { }


    /// `StyleBackfaceVisibility` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackfaceVisibility as StyleBackfaceVisibility;

    impl Clone for StyleBackfaceVisibility { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackfaceVisibility { }


    /// `StyleTransform` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransform as StyleTransform;

    impl Clone for StyleTransform { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransform { }


    /// `StyleTransformMatrix2D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformMatrix2D as StyleTransformMatrix2D;

    impl Clone for StyleTransformMatrix2D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformMatrix2D { }


    /// `StyleTransformMatrix3D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformMatrix3D as StyleTransformMatrix3D;

    impl Clone for StyleTransformMatrix3D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformMatrix3D { }


    /// `StyleTransformTranslate2D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformTranslate2D as StyleTransformTranslate2D;

    impl Clone for StyleTransformTranslate2D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformTranslate2D { }


    /// `StyleTransformTranslate3D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformTranslate3D as StyleTransformTranslate3D;

    impl Clone for StyleTransformTranslate3D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformTranslate3D { }


    /// `StyleTransformRotate3D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformRotate3D as StyleTransformRotate3D;

    impl Clone for StyleTransformRotate3D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformRotate3D { }


    /// `StyleTransformScale2D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformScale2D as StyleTransformScale2D;

    impl Clone for StyleTransformScale2D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformScale2D { }


    /// `StyleTransformScale3D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformScale3D as StyleTransformScale3D;

    impl Clone for StyleTransformScale3D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformScale3D { }


    /// `StyleTransformSkew2D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformSkew2D as StyleTransformSkew2D;

    impl Clone for StyleTransformSkew2D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformSkew2D { }


    /// `StyleTextAlignmentHorz` struct
    #[doc(inline)] pub use crate::dll::AzStyleTextAlignmentHorz as StyleTextAlignmentHorz;

    impl Clone for StyleTextAlignmentHorz { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTextAlignmentHorz { }


    /// `StyleTextColor` struct
    #[doc(inline)] pub use crate::dll::AzStyleTextColor as StyleTextColor;

    impl Clone for StyleTextColor { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTextColor { }


    /// `StyleWordSpacing` struct
    #[doc(inline)] pub use crate::dll::AzStyleWordSpacing as StyleWordSpacing;

    impl Clone for StyleWordSpacing { fn clone(&self) -> Self { *self } }
    impl Copy for StyleWordSpacing { }


    /// `StyleBoxShadowValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBoxShadowValue as StyleBoxShadowValue;

    impl Clone for StyleBoxShadowValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBoxShadowValue { }


    /// `LayoutAlignContentValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutAlignContentValue as LayoutAlignContentValue;

    impl Clone for LayoutAlignContentValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutAlignContentValue { }


    /// `LayoutAlignItemsValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutAlignItemsValue as LayoutAlignItemsValue;

    impl Clone for LayoutAlignItemsValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutAlignItemsValue { }


    /// `LayoutBottomValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBottomValue as LayoutBottomValue;

    impl Clone for LayoutBottomValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBottomValue { }


    /// `LayoutBoxSizingValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBoxSizingValue as LayoutBoxSizingValue;

    impl Clone for LayoutBoxSizingValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBoxSizingValue { }


    /// `LayoutFlexDirectionValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFlexDirectionValue as LayoutFlexDirectionValue;

    impl Clone for LayoutFlexDirectionValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFlexDirectionValue { }


    /// `LayoutDisplayValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutDisplayValue as LayoutDisplayValue;

    impl Clone for LayoutDisplayValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutDisplayValue { }


    /// `LayoutFlexGrowValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFlexGrowValue as LayoutFlexGrowValue;

    impl Clone for LayoutFlexGrowValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFlexGrowValue { }


    /// `LayoutFlexShrinkValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFlexShrinkValue as LayoutFlexShrinkValue;

    impl Clone for LayoutFlexShrinkValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFlexShrinkValue { }


    /// `LayoutFloatValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFloatValue as LayoutFloatValue;

    impl Clone for LayoutFloatValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFloatValue { }


    /// `LayoutHeightValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutHeightValue as LayoutHeightValue;

    impl Clone for LayoutHeightValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutHeightValue { }


    /// `LayoutJustifyContentValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutJustifyContentValue as LayoutJustifyContentValue;

    impl Clone for LayoutJustifyContentValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutJustifyContentValue { }


    /// `LayoutLeftValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutLeftValue as LayoutLeftValue;

    impl Clone for LayoutLeftValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutLeftValue { }


    /// `LayoutMarginBottomValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginBottomValue as LayoutMarginBottomValue;

    impl Clone for LayoutMarginBottomValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginBottomValue { }


    /// `LayoutMarginLeftValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginLeftValue as LayoutMarginLeftValue;

    impl Clone for LayoutMarginLeftValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginLeftValue { }


    /// `LayoutMarginRightValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginRightValue as LayoutMarginRightValue;

    impl Clone for LayoutMarginRightValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginRightValue { }


    /// `LayoutMarginTopValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginTopValue as LayoutMarginTopValue;

    impl Clone for LayoutMarginTopValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginTopValue { }


    /// `LayoutMaxHeightValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMaxHeightValue as LayoutMaxHeightValue;

    impl Clone for LayoutMaxHeightValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMaxHeightValue { }


    /// `LayoutMaxWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMaxWidthValue as LayoutMaxWidthValue;

    impl Clone for LayoutMaxWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMaxWidthValue { }


    /// `LayoutMinHeightValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMinHeightValue as LayoutMinHeightValue;

    impl Clone for LayoutMinHeightValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMinHeightValue { }


    /// `LayoutMinWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMinWidthValue as LayoutMinWidthValue;

    impl Clone for LayoutMinWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMinWidthValue { }


    /// `LayoutPaddingBottomValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingBottomValue as LayoutPaddingBottomValue;

    impl Clone for LayoutPaddingBottomValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingBottomValue { }


    /// `LayoutPaddingLeftValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingLeftValue as LayoutPaddingLeftValue;

    impl Clone for LayoutPaddingLeftValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingLeftValue { }


    /// `LayoutPaddingRightValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingRightValue as LayoutPaddingRightValue;

    impl Clone for LayoutPaddingRightValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingRightValue { }


    /// `LayoutPaddingTopValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingTopValue as LayoutPaddingTopValue;

    impl Clone for LayoutPaddingTopValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingTopValue { }


    /// `LayoutPositionValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPositionValue as LayoutPositionValue;

    impl Clone for LayoutPositionValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPositionValue { }


    /// `LayoutRightValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutRightValue as LayoutRightValue;

    impl Clone for LayoutRightValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutRightValue { }


    /// `LayoutTopValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutTopValue as LayoutTopValue;

    impl Clone for LayoutTopValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutTopValue { }


    /// `LayoutWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutWidthValue as LayoutWidthValue;

    impl Clone for LayoutWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutWidthValue { }


    /// `LayoutFlexWrapValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFlexWrapValue as LayoutFlexWrapValue;

    impl Clone for LayoutFlexWrapValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFlexWrapValue { }


    /// `LayoutOverflowValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutOverflowValue as LayoutOverflowValue;

    impl Clone for LayoutOverflowValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutOverflowValue { }


    /// `ScrollbarStyleValue` struct
    #[doc(inline)] pub use crate::dll::AzScrollbarStyleValue as ScrollbarStyleValue;

    impl Clone for ScrollbarStyleValue { fn clone(&self) -> Self { unsafe { crate::dll::az_scrollbar_style_value_deep_copy(self) } } }
    impl Drop for ScrollbarStyleValue { fn drop(&mut self) { unsafe { crate::dll::az_scrollbar_style_value_delete(self) }; } }


    /// `StyleBackgroundContentVecValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundContentVecValue as StyleBackgroundContentVecValue;

    impl Clone for StyleBackgroundContentVecValue { fn clone(&self) -> Self { unsafe { crate::dll::az_style_background_content_vec_value_deep_copy(self) } } }
    impl Drop for StyleBackgroundContentVecValue { fn drop(&mut self) { unsafe { crate::dll::az_style_background_content_vec_value_delete(self) }; } }


    /// `StyleBackgroundPositionVecValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundPositionVecValue as StyleBackgroundPositionVecValue;

    impl Clone for StyleBackgroundPositionVecValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackgroundPositionVecValue { }


    /// `StyleBackgroundRepeatVecValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundRepeatVecValue as StyleBackgroundRepeatVecValue;

    impl Clone for StyleBackgroundRepeatVecValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackgroundRepeatVecValue { }


    /// `StyleBackgroundSizeVecValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundSizeVecValue as StyleBackgroundSizeVecValue;

    impl Clone for StyleBackgroundSizeVecValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackgroundSizeVecValue { }


    /// `StyleBorderBottomColorValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomColorValue as StyleBorderBottomColorValue;

    impl Clone for StyleBorderBottomColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomColorValue { }


    /// `StyleBorderBottomLeftRadiusValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomLeftRadiusValue as StyleBorderBottomLeftRadiusValue;

    impl Clone for StyleBorderBottomLeftRadiusValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomLeftRadiusValue { }


    /// `StyleBorderBottomRightRadiusValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomRightRadiusValue as StyleBorderBottomRightRadiusValue;

    impl Clone for StyleBorderBottomRightRadiusValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomRightRadiusValue { }


    /// `StyleBorderBottomStyleValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomStyleValue as StyleBorderBottomStyleValue;

    impl Clone for StyleBorderBottomStyleValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomStyleValue { }


    /// `LayoutBorderBottomWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBorderBottomWidthValue as LayoutBorderBottomWidthValue;

    impl Clone for LayoutBorderBottomWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBorderBottomWidthValue { }


    /// `StyleBorderLeftColorValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderLeftColorValue as StyleBorderLeftColorValue;

    impl Clone for StyleBorderLeftColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderLeftColorValue { }


    /// `StyleBorderLeftStyleValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderLeftStyleValue as StyleBorderLeftStyleValue;

    impl Clone for StyleBorderLeftStyleValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderLeftStyleValue { }


    /// `LayoutBorderLeftWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBorderLeftWidthValue as LayoutBorderLeftWidthValue;

    impl Clone for LayoutBorderLeftWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBorderLeftWidthValue { }


    /// `StyleBorderRightColorValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderRightColorValue as StyleBorderRightColorValue;

    impl Clone for StyleBorderRightColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderRightColorValue { }


    /// `StyleBorderRightStyleValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderRightStyleValue as StyleBorderRightStyleValue;

    impl Clone for StyleBorderRightStyleValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderRightStyleValue { }


    /// `LayoutBorderRightWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBorderRightWidthValue as LayoutBorderRightWidthValue;

    impl Clone for LayoutBorderRightWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBorderRightWidthValue { }


    /// `StyleBorderTopColorValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopColorValue as StyleBorderTopColorValue;

    impl Clone for StyleBorderTopColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopColorValue { }


    /// `StyleBorderTopLeftRadiusValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopLeftRadiusValue as StyleBorderTopLeftRadiusValue;

    impl Clone for StyleBorderTopLeftRadiusValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopLeftRadiusValue { }


    /// `StyleBorderTopRightRadiusValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopRightRadiusValue as StyleBorderTopRightRadiusValue;

    impl Clone for StyleBorderTopRightRadiusValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopRightRadiusValue { }


    /// `StyleBorderTopStyleValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopStyleValue as StyleBorderTopStyleValue;

    impl Clone for StyleBorderTopStyleValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopStyleValue { }


    /// `LayoutBorderTopWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBorderTopWidthValue as LayoutBorderTopWidthValue;

    impl Clone for LayoutBorderTopWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBorderTopWidthValue { }


    /// `StyleCursorValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleCursorValue as StyleCursorValue;

    impl Clone for StyleCursorValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleCursorValue { }


    /// `StyleFontFamilyValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleFontFamilyValue as StyleFontFamilyValue;

    impl Clone for StyleFontFamilyValue { fn clone(&self) -> Self { unsafe { crate::dll::az_style_font_family_value_deep_copy(self) } } }
    impl Drop for StyleFontFamilyValue { fn drop(&mut self) { unsafe { crate::dll::az_style_font_family_value_delete(self) }; } }


    /// `StyleFontSizeValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleFontSizeValue as StyleFontSizeValue;

    impl Clone for StyleFontSizeValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleFontSizeValue { }


    /// `StyleLetterSpacingValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleLetterSpacingValue as StyleLetterSpacingValue;

    impl Clone for StyleLetterSpacingValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleLetterSpacingValue { }


    /// `StyleLineHeightValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleLineHeightValue as StyleLineHeightValue;

    impl Clone for StyleLineHeightValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleLineHeightValue { }


    /// `StyleTabWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleTabWidthValue as StyleTabWidthValue;

    impl Clone for StyleTabWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTabWidthValue { }


    /// `StyleTextAlignmentHorzValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleTextAlignmentHorzValue as StyleTextAlignmentHorzValue;

    impl Clone for StyleTextAlignmentHorzValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTextAlignmentHorzValue { }


    /// `StyleTextColorValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleTextColorValue as StyleTextColorValue;

    impl Clone for StyleTextColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTextColorValue { }


    /// `StyleWordSpacingValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleWordSpacingValue as StyleWordSpacingValue;

    impl Clone for StyleWordSpacingValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleWordSpacingValue { }


    /// `StyleOpacityValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleOpacityValue as StyleOpacityValue;

    impl Clone for StyleOpacityValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleOpacityValue { }


    /// `StyleTransformVecValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformVecValue as StyleTransformVecValue;

    impl Clone for StyleTransformVecValue { fn clone(&self) -> Self { unsafe { crate::dll::az_style_transform_vec_value_deep_copy(self) } } }
    impl Drop for StyleTransformVecValue { fn drop(&mut self) { unsafe { crate::dll::az_style_transform_vec_value_delete(self) }; } }


    /// `StyleTransformOriginValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformOriginValue as StyleTransformOriginValue;

    impl Clone for StyleTransformOriginValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformOriginValue { }


    /// `StylePerspectiveOriginValue` struct
    #[doc(inline)] pub use crate::dll::AzStylePerspectiveOriginValue as StylePerspectiveOriginValue;

    impl Clone for StylePerspectiveOriginValue { fn clone(&self) -> Self { *self } }
    impl Copy for StylePerspectiveOriginValue { }


    /// `StyleBackfaceVisibilityValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackfaceVisibilityValue as StyleBackfaceVisibilityValue;

    impl Clone for StyleBackfaceVisibilityValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackfaceVisibilityValue { }


    /// Parsed CSS key-value pair
    #[doc(inline)] pub use crate::dll::AzCssProperty as CssProperty;

    impl Clone for CssProperty { fn clone(&self) -> Self { unsafe { crate::dll::az_css_property_deep_copy(self) } } }
    impl Drop for CssProperty { fn drop(&mut self) { unsafe { crate::dll::az_css_property_delete(self) }; } }
