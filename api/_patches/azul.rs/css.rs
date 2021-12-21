
    #[cfg(not(feature = "link_static"))]
    use crate::vec::{
        StyleBackgroundPositionVec,
        StyleBackgroundContentVec,
        StyleBackgroundSizeVec,
        StyleBackgroundRepeatVec,
        StyleTransformVec,
        StyleFontFamilyVec,
        StyleFilterVec,
    };

    macro_rules! css_property_from_type {($prop_type:expr, $content_type:ident) => ({
        match $prop_type {
            CssPropertyType::TextColor => CssProperty::TextColor(StyleTextColorValue::$content_type),
            CssPropertyType::FontSize => CssProperty::FontSize(StyleFontSizeValue::$content_type),
            CssPropertyType::FontFamily => CssProperty::FontFamily(StyleFontFamilyVecValue::$content_type),
            CssPropertyType::TextAlign => CssProperty::TextAlign(StyleTextAlignValue::$content_type),
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
            CssPropertyType::FlexWrap => CssProperty::FlexWrap(LayoutFlexWrapValue::$content_type),
            CssPropertyType::FlexDirection => CssProperty::FlexDirection(LayoutFlexDirectionValue::$content_type),
            CssPropertyType::FlexGrow => CssProperty::FlexGrow(LayoutFlexGrowValue::$content_type),
            CssPropertyType::FlexShrink => CssProperty::FlexShrink(LayoutFlexShrinkValue::$content_type),
            CssPropertyType::JustifyContent => CssProperty::JustifyContent(LayoutJustifyContentValue::$content_type),
            CssPropertyType::AlignItems => CssProperty::AlignItems(LayoutAlignItemsValue::$content_type),
            CssPropertyType::AlignContent => CssProperty::AlignContent(LayoutAlignContentValue::$content_type),
            CssPropertyType::BackgroundContent => CssProperty::BackgroundContent(StyleBackgroundContentVecValue::$content_type),
            CssPropertyType::BackgroundPosition => CssProperty::BackgroundPosition(StyleBackgroundPositionVecValue::$content_type),
            CssPropertyType::BackgroundSize => CssProperty::BackgroundSize(StyleBackgroundSizeVecValue::$content_type),
            CssPropertyType::BackgroundRepeat => CssProperty::BackgroundRepeat(StyleBackgroundRepeatVecValue::$content_type),
            CssPropertyType::OverflowX => CssProperty::OverflowX(LayoutOverflowValue::$content_type),
            CssPropertyType::OverflowY => CssProperty::OverflowY(LayoutOverflowValue::$content_type),
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
            CssPropertyType::BorderTopWidth => CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::$content_type),
            CssPropertyType::BorderRightWidth => CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::$content_type),
            CssPropertyType::BorderLeftWidth => CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::$content_type),
            CssPropertyType::BorderBottomWidth => CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::$content_type),
            CssPropertyType::BoxShadowLeft => CssProperty::BoxShadowLeft(StyleBoxShadowValue::$content_type),
            CssPropertyType::BoxShadowRight => CssProperty::BoxShadowRight(StyleBoxShadowValue::$content_type),
            CssPropertyType::BoxShadowTop => CssProperty::BoxShadowTop(StyleBoxShadowValue::$content_type),
            CssPropertyType::BoxShadowBottom => CssProperty::BoxShadowBottom(StyleBoxShadowValue::$content_type),
            CssPropertyType::ScrollbarStyle => CssProperty::ScrollbarStyle(ScrollbarStyleValue::$content_type),
            CssPropertyType::Opacity => CssProperty::Opacity(StyleOpacityValue::$content_type),
            CssPropertyType::Transform => CssProperty::Transform(StyleTransformVecValue::$content_type),
            CssPropertyType::PerspectiveOrigin => CssProperty::PerspectiveOrigin(StylePerspectiveOriginValue::$content_type),
            CssPropertyType::TransformOrigin => CssProperty::TransformOrigin(StyleTransformOriginValue::$content_type),
            CssPropertyType::BackfaceVisibility => CssProperty::BackfaceVisibility(StyleBackfaceVisibilityValue::$content_type),
            CssPropertyType::MixBlendMode => CssProperty::MixBlendMode(StyleMixBlendModeValue::$content_type),
            CssPropertyType::Filter => CssProperty::Filter(StyleFilterVecValue::$content_type),
            CssPropertyType::BackdropFilter => CssProperty::BackdropFilter(StyleFilterVecValue::$content_type),
            CssPropertyType::TextShadow => CssProperty::TextShadow(StyleBoxShadowValue::$content_type),
        }
    })}

    #[cfg(not(feature = "link_static"))]
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
                CssProperty::BackgroundContent(_) => CssPropertyType::BackgroundContent,
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
                CssProperty::ScrollbarStyle(_) => CssPropertyType::ScrollbarStyle,
                CssProperty::Opacity(_) => CssPropertyType::Opacity,
                CssProperty::Transform(_) => CssPropertyType::Transform,
                CssProperty::PerspectiveOrigin(_) => CssPropertyType::PerspectiveOrigin,
                CssProperty::TransformOrigin(_) => CssPropertyType::TransformOrigin,
                CssProperty::BackfaceVisibility(_) => CssPropertyType::BackfaceVisibility,
                CssProperty::MixBlendMode(_) => CssPropertyType::MixBlendMode,
                CssProperty::Filter(_) => CssPropertyType::Filter,
                CssProperty::BackdropFilter(_) => CssPropertyType::BackdropFilter,
                CssProperty::TextShadow(_) => CssPropertyType::TextShadow,
            }
        }

        // const constructors for easier API access

        pub const fn none(prop_type: CssPropertyType) -> Self { css_property_from_type!(prop_type, None) }
        pub const fn auto(prop_type: CssPropertyType) -> Self { css_property_from_type!(prop_type, Auto) }
        pub const fn initial(prop_type: CssPropertyType) -> Self { css_property_from_type!(prop_type, Initial) }
        pub const fn inherit(prop_type: CssPropertyType) -> Self { css_property_from_type!(prop_type, Inherit) }

        pub const fn text_color(input: StyleTextColor) -> Self { CssProperty::TextColor(StyleTextColorValue::Exact(input)) }
        pub const fn font_size(input: StyleFontSize) -> Self { CssProperty::FontSize(StyleFontSizeValue::Exact(input)) }
        pub const fn font_family(input: StyleFontFamilyVec) -> Self { CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(input)) }
        pub const fn text_align(input: StyleTextAlign) -> Self { CssProperty::TextAlign(StyleTextAlignValue::Exact(input)) }
        pub const fn letter_spacing(input: StyleLetterSpacing) -> Self { CssProperty::LetterSpacing(StyleLetterSpacingValue::Exact(input)) }
        pub const fn line_height(input: StyleLineHeight) -> Self { CssProperty::LineHeight(StyleLineHeightValue::Exact(input)) }
        pub const fn word_spacing(input: StyleWordSpacing) -> Self { CssProperty::WordSpacing(StyleWordSpacingValue::Exact(input)) }
        pub const fn tab_width(input: StyleTabWidth) -> Self { CssProperty::TabWidth(StyleTabWidthValue::Exact(input)) }
        pub const fn cursor(input: StyleCursor) -> Self { CssProperty::Cursor(StyleCursorValue::Exact(input)) }
        pub const fn display(input: LayoutDisplay) -> Self { CssProperty::Display(LayoutDisplayValue::Exact(input)) }
        pub const fn float(input: LayoutFloat) -> Self { CssProperty::Float(LayoutFloatValue::Exact(input)) }
        pub const fn box_sizing(input: LayoutBoxSizing) -> Self { CssProperty::BoxSizing(LayoutBoxSizingValue::Exact(input)) }
        pub const fn width(input: LayoutWidth) -> Self { CssProperty::Width(LayoutWidthValue::Exact(input)) }
        pub const fn height(input: LayoutHeight) -> Self { CssProperty::Height(LayoutHeightValue::Exact(input)) }
        pub const fn min_width(input: LayoutMinWidth) -> Self { CssProperty::MinWidth(LayoutMinWidthValue::Exact(input)) }
        pub const fn min_height(input: LayoutMinHeight) -> Self { CssProperty::MinHeight(LayoutMinHeightValue::Exact(input)) }
        pub const fn max_width(input: LayoutMaxWidth) -> Self { CssProperty::MaxWidth(LayoutMaxWidthValue::Exact(input)) }
        pub const fn max_height(input: LayoutMaxHeight) -> Self { CssProperty::MaxHeight(LayoutMaxHeightValue::Exact(input)) }
        pub const fn position(input: LayoutPosition) -> Self { CssProperty::Position(LayoutPositionValue::Exact(input)) }
        pub const fn top(input: LayoutTop) -> Self { CssProperty::Top(LayoutTopValue::Exact(input)) }
        pub const fn right(input: LayoutRight) -> Self { CssProperty::Right(LayoutRightValue::Exact(input)) }
        pub const fn left(input: LayoutLeft) -> Self { CssProperty::Left(LayoutLeftValue::Exact(input)) }
        pub const fn bottom(input: LayoutBottom) -> Self { CssProperty::Bottom(LayoutBottomValue::Exact(input)) }
        pub const fn flex_wrap(input: LayoutFlexWrap) -> Self { CssProperty::FlexWrap(LayoutFlexWrapValue::Exact(input)) }
        pub const fn flex_direction(input: LayoutFlexDirection) -> Self { CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(input)) }
        pub const fn flex_grow(input: LayoutFlexGrow) -> Self { CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(input)) }
        pub const fn flex_shrink(input: LayoutFlexShrink) -> Self { CssProperty::FlexShrink(LayoutFlexShrinkValue::Exact(input)) }
        pub const fn justify_content(input: LayoutJustifyContent) -> Self { CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(input)) }
        pub const fn align_items(input: LayoutAlignItems) -> Self { CssProperty::AlignItems(LayoutAlignItemsValue::Exact(input)) }
        pub const fn align_content(input: LayoutAlignContent) -> Self { CssProperty::AlignContent(LayoutAlignContentValue::Exact(input)) }
        pub const fn background_content(input: StyleBackgroundContentVec) -> Self { CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(input)) }
        pub const fn background_position(input: StyleBackgroundPositionVec) -> Self { CssProperty::BackgroundPosition(StyleBackgroundPositionVecValue::Exact(input)) }
        pub const fn background_size(input: StyleBackgroundSizeVec) -> Self { CssProperty::BackgroundSize(StyleBackgroundSizeVecValue::Exact(input)) }
        pub const fn background_repeat(input: StyleBackgroundRepeatVec) -> Self { CssProperty::BackgroundRepeat(StyleBackgroundRepeatVecValue::Exact(input)) }
        pub const fn overflow_x(input: LayoutOverflow) -> Self { CssProperty::OverflowX(LayoutOverflowValue::Exact(input)) }
        pub const fn overflow_y(input: LayoutOverflow) -> Self { CssProperty::OverflowY(LayoutOverflowValue::Exact(input)) }
        pub const fn padding_top(input: LayoutPaddingTop) -> Self { CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(input)) }
        pub const fn padding_left(input: LayoutPaddingLeft) -> Self { CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(input)) }
        pub const fn padding_right(input: LayoutPaddingRight) -> Self { CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(input)) }
        pub const fn padding_bottom(input: LayoutPaddingBottom) -> Self { CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(input)) }
        pub const fn margin_top(input: LayoutMarginTop) -> Self { CssProperty::MarginTop(LayoutMarginTopValue::Exact(input)) }
        pub const fn margin_left(input: LayoutMarginLeft) -> Self { CssProperty::MarginLeft(LayoutMarginLeftValue::Exact(input)) }
        pub const fn margin_right(input: LayoutMarginRight) -> Self { CssProperty::MarginRight(LayoutMarginRightValue::Exact(input)) }
        pub const fn margin_bottom(input: LayoutMarginBottom) -> Self { CssProperty::MarginBottom(LayoutMarginBottomValue::Exact(input)) }
        pub const fn border_top_left_radius(input: StyleBorderTopLeftRadius) -> Self { CssProperty::BorderTopLeftRadius(StyleBorderTopLeftRadiusValue::Exact(input)) }
        pub const fn border_top_right_radius(input: StyleBorderTopRightRadius) -> Self { CssProperty::BorderTopRightRadius(StyleBorderTopRightRadiusValue::Exact(input)) }
        pub const fn border_bottom_left_radius(input: StyleBorderBottomLeftRadius) -> Self { CssProperty::BorderBottomLeftRadius(StyleBorderBottomLeftRadiusValue::Exact(input)) }
        pub const fn border_bottom_right_radius(input: StyleBorderBottomRightRadius) -> Self { CssProperty::BorderBottomRightRadius(StyleBorderBottomRightRadiusValue::Exact(input)) }
        pub const fn border_top_color(input: StyleBorderTopColor) -> Self { CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(input)) }
        pub const fn border_right_color(input: StyleBorderRightColor) -> Self { CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(input)) }
        pub const fn border_left_color(input: StyleBorderLeftColor) -> Self { CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(input)) }
        pub const fn border_bottom_color(input: StyleBorderBottomColor) -> Self { CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(input)) }
        pub const fn border_top_style(input: StyleBorderTopStyle) -> Self { CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(input)) }
        pub const fn border_right_style(input: StyleBorderRightStyle) -> Self { CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(input)) }
        pub const fn border_left_style(input: StyleBorderLeftStyle) -> Self { CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(input)) }
        pub const fn border_bottom_style(input: StyleBorderBottomStyle) -> Self { CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(input)) }
        pub const fn border_top_width(input: LayoutBorderTopWidth) -> Self { CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(input)) }
        pub const fn border_right_width(input: LayoutBorderRightWidth) -> Self { CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(input)) }
        pub const fn border_left_width(input: LayoutBorderLeftWidth) -> Self { CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(input)) }
        pub const fn border_bottom_width(input: LayoutBorderBottomWidth) -> Self { CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(input)) }
        pub const fn box_shadow_left(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(input)) }
        pub const fn box_shadow_right(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(input)) }
        pub const fn box_shadow_top(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(input)) }
        pub const fn box_shadow_bottom(input: StyleBoxShadow) -> Self { CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(input)) }
        pub const fn opacity(input: StyleOpacity) -> Self { CssProperty::Opacity(StyleOpacityValue::Exact(input)) }
        pub const fn transform(input: StyleTransformVec) -> Self { CssProperty::Transform(StyleTransformVecValue::Exact(input)) }
        pub const fn transform_origin(input: StyleTransformOrigin) -> Self { CssProperty::TransformOrigin(StyleTransformOriginValue::Exact(input)) }
        pub const fn perspective_origin(input: StylePerspectiveOrigin) -> Self { CssProperty::PerspectiveOrigin(StylePerspectiveOriginValue::Exact(input)) }
        pub const fn backface_visiblity(input: StyleBackfaceVisibility) -> Self { CssProperty::BackfaceVisibility(StyleBackfaceVisibilityValue::Exact(input)) }
        pub const fn mix_blend_mode(input: StyleMixBlendMode) -> Self { CssProperty::MixBlendMode(StyleMixBlendModeValue::Exact(input)) }
        pub const fn filter(input: StyleFilterVec) -> Self { CssProperty::Filter(StyleFilterVecValue::Exact(input)) }
        pub const fn backdrop_filter(input: StyleFilterVec) -> Self { CssProperty::BackdropFilter(StyleFilterVecValue::Exact(input)) }
        pub const fn text_shadow(input: StyleBoxShadow) -> Self { CssProperty::TextShadow(StyleBoxShadowValue::Exact(input)) }
    }

    const FP_PRECISION_MULTIPLIER: f32 = 1000.0;
    const FP_PRECISION_MULTIPLIER_CONST: isize = FP_PRECISION_MULTIPLIER as isize;

    #[cfg(not(feature = "link_static"))]
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

    #[cfg(not(feature = "link_static"))]
    impl From<f32> for FloatValue {
        fn from(val: f32) -> Self {
            Self::new(val)
        }
    }

    #[cfg(not(feature = "link_static"))]
    impl AngleValue {

        #[inline]
        pub const fn zero() -> Self {
            const ZERO_DEG: AngleValue = AngleValue::const_deg(0);
            ZERO_DEG
        }

        /// Same as `PixelValue::px()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_deg(value: isize) -> Self {
            Self::const_from_metric(AngleMetric::Degree, value)
        }

        /// Same as `PixelValue::em()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_rad(value: isize) -> Self {
            Self::const_from_metric(AngleMetric::Radians, value)
        }

        /// Same as `PixelValue::pt()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_grad(value: isize) -> Self {
            Self::const_from_metric(AngleMetric::Grad, value)
        }

        /// Same as `PixelValue::pt()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_turn(value: isize) -> Self {
            Self::const_from_metric(AngleMetric::Turn, value)
        }

        #[inline]
        pub fn const_percent(value: isize) -> Self {
            Self::const_from_metric(AngleMetric::Percent, value)
        }

        #[inline]
        pub const fn const_from_metric(metric: AngleMetric, value: isize) -> Self {
            Self {
                metric: metric,
                number: FloatValue::const_new(value),
            }
        }

        #[inline]
        pub fn deg(value: f32) -> Self {
            Self::from_metric(AngleMetric::Degree, value)
        }

        #[inline]
        pub fn rad(value: f32) -> Self {
            Self::from_metric(AngleMetric::Radians, value)
        }

        #[inline]
        pub fn grad(value: f32) -> Self {
            Self::from_metric(AngleMetric::Grad, value)
        }

        #[inline]
        pub fn turn(value: f32) -> Self {
            Self::from_metric(AngleMetric::Turn, value)
        }

        #[inline]
        pub fn percent(value: f32) -> Self {
            Self::from_metric(AngleMetric::Percent, value)
        }

        #[inline]
        pub fn from_metric(metric: AngleMetric, value: f32) -> Self {
            Self {
                metric: metric,
                number: FloatValue::new(value),
            }
        }
    }

    #[cfg(not(feature = "link_static"))]
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

    #[cfg(not(feature = "link_static"))]
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

    #[cfg(not(feature = "link_static"))]
    impl PercentageValue {

        /// Same as `PercentageValue::new()`, but only accepts whole numbers,
        /// since using `f32` in const fn is not yet stabilized.
        #[inline]
        pub const fn const_new(value: isize) -> Self {
            Self { number: FloatValue::const_new(value) }
        }

        #[inline]
        pub fn new(value: f32) -> Self {
            Self { number: value.into() }
        }

        #[inline]
        pub fn get(&self) -> f32 {
            self.number.get()
        }
    }

    /// Creates `pt`, `px` and `em` constructors for any struct that has a
    /// `PixelValue` as it's self.0 field.
    macro_rules! impl_pixel_value {($struct:ident) => (

        #[cfg(not(feature = "link_static"))]
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
    impl_pixel_value!(LayoutBorderTopWidth);
    impl_pixel_value!(LayoutBorderLeftWidth);
    impl_pixel_value!(LayoutBorderRightWidth);
    impl_pixel_value!(LayoutBorderBottomWidth);
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
        #[cfg(not(feature = "link_static"))]
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

        #[cfg(not(feature = "link_static"))]
        impl From<f32> for $struct {
            fn from(val: f32) -> Self {
                Self { inner: FloatValue::from(val) }
            }
        }
    )}

    impl_float_value!(LayoutFlexGrow);
    impl_float_value!(LayoutFlexShrink);

    macro_rules! impl_percentage_value{($struct:ident) => (
        #[cfg(not(feature = "link_static"))]
        impl $struct {
            /// Same as `PercentageValue::new()`, but only accepts whole numbers,
            /// since using `f32` in const fn is not yet stabilized.
            #[inline]
            pub const fn const_new(value: isize) -> Self {
                Self { inner: PercentageValue::const_new(value) }
            }
        }
    )}

    impl_percentage_value!(StyleLineHeight);
    impl_percentage_value!(StyleTabWidth);
    impl_percentage_value!(StyleOpacity);
