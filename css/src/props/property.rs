//! Defines the core `CssProperty` enum, which represents any single parsed CSS property,
//! as well as top-level functions for parsing CSS keys and values.

use alloc::{
    boxed::Box,
    collections::btree_map::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

use crate::{
    corety::AzString,
    css::CssPropertyValue,
    props::basic::{error::InvalidValueErr, pixel::PixelValueWithAuto},
};
// Import all property types from their new locations
use crate::{
    format_rust_code::FormatAsRustCode,
    props::{
        basic::{
            color::{parse_css_color, ColorU, CssColorParseError, CssColorParseErrorOwned},
            font::{
                parse_style_font_family, CssStyleFontFamilyParseError,
                CssStyleFontFamilyParseErrorOwned, StyleFontFamilyVec, *,
            },
            length::{parse_float_value, parse_percentage_value, FloatValue, PercentageValue},
            pixel::{
                parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned,
                PixelValue,
            },
            DurationParseError, DurationParseErrorOwned, InterpolateResolver, InvalidValueErrOwned,
            PercentageParseError,
        },
        formatter::PrintAsCssValue,
        layout::{
            column::*, dimensions::*, display::*, flex::*, flow::*, fragmentation::*, grid::*,
            overflow::*, position::*, shape::*, spacing::*, table::*, text::*, wrapping::*,
        },
        style::{
            azul_exclusion::*, background::*, border::*, border_radius::*, box_shadow::*,
            content::*, effects::*, filter::*, lists::*, scrollbar::*, text::*, transform::*,
            SelectionBackgroundColor, SelectionColor, SelectionRadius,
        },
    },
};

const COMBINED_CSS_PROPERTIES_KEY_MAP: [(CombinedCssPropertyType, &'static str); 24] = [
    (CombinedCssPropertyType::BorderRadius, "border-radius"),
    (CombinedCssPropertyType::Overflow, "overflow"),
    (CombinedCssPropertyType::Padding, "padding"),
    (CombinedCssPropertyType::Margin, "margin"),
    (CombinedCssPropertyType::Border, "border"),
    (CombinedCssPropertyType::BorderLeft, "border-left"),
    (CombinedCssPropertyType::BorderRight, "border-right"),
    (CombinedCssPropertyType::BorderTop, "border-top"),
    (CombinedCssPropertyType::BorderBottom, "border-bottom"),
    (CombinedCssPropertyType::BorderColor, "border-color"),
    (CombinedCssPropertyType::BorderStyle, "border-style"),
    (CombinedCssPropertyType::BorderWidth, "border-width"),
    (CombinedCssPropertyType::BoxShadow, "box-shadow"),
    (CombinedCssPropertyType::BackgroundColor, "background-color"),
    (CombinedCssPropertyType::BackgroundImage, "background-image"),
    (CombinedCssPropertyType::Background, "background"),
    (CombinedCssPropertyType::Flex, "flex"),
    (CombinedCssPropertyType::Grid, "grid"),
    (CombinedCssPropertyType::Gap, "gap"),
    (CombinedCssPropertyType::GridGap, "grid-gap"),
    (CombinedCssPropertyType::Font, "font"),
    (CombinedCssPropertyType::Columns, "columns"),
    (CombinedCssPropertyType::GridArea, "grid-area"),
    (CombinedCssPropertyType::ColumnRule, "column-rule"),
];

const CSS_PROPERTY_KEY_MAP: [(CssPropertyType, &'static str); 157] = [
    (CssPropertyType::Display, "display"),
    (CssPropertyType::Float, "float"),
    (CssPropertyType::BoxSizing, "box-sizing"),
    (CssPropertyType::TextColor, "color"),
    (CssPropertyType::FontSize, "font-size"),
    (CssPropertyType::FontFamily, "font-family"),
    (CssPropertyType::FontWeight, "font-weight"),
    (CssPropertyType::FontStyle, "font-style"),
    (CssPropertyType::TextAlign, "text-align"),
    (CssPropertyType::TextJustify, "text-justify"),
    (CssPropertyType::VerticalAlign, "vertical-align"),
    (CssPropertyType::LetterSpacing, "letter-spacing"),
    (CssPropertyType::LineHeight, "line-height"),
    (CssPropertyType::WordSpacing, "word-spacing"),
    (CssPropertyType::TabSize, "tab-size"),
    (CssPropertyType::WhiteSpace, "white-space"),
    (CssPropertyType::Hyphens, "hyphens"),
    (CssPropertyType::Direction, "direction"),
    (CssPropertyType::UserSelect, "user-select"),
    (CssPropertyType::TextDecoration, "text-decoration"),
    (CssPropertyType::TextIndent, "text-indent"),
    (CssPropertyType::InitialLetter, "initial-letter"),
    (CssPropertyType::LineClamp, "line-clamp"),
    (CssPropertyType::HangingPunctuation, "hanging-punctuation"),
    (CssPropertyType::TextCombineUpright, "text-combine-upright"),
    (CssPropertyType::ExclusionMargin, "-azul-exclusion-margin"),
    (
        CssPropertyType::HyphenationLanguage,
        "-azul-hyphenation-language",
    ),
    (CssPropertyType::Cursor, "cursor"),
    (CssPropertyType::Width, "width"),
    (CssPropertyType::Height, "height"),
    (CssPropertyType::MinWidth, "min-width"),
    (CssPropertyType::MinHeight, "min-height"),
    (CssPropertyType::MaxWidth, "max-width"),
    (CssPropertyType::MaxHeight, "max-height"),
    (CssPropertyType::Position, "position"),
    (CssPropertyType::Top, "top"),
    (CssPropertyType::Right, "right"),
    (CssPropertyType::Left, "left"),
    (CssPropertyType::Bottom, "bottom"),
    (CssPropertyType::ZIndex, "z-index"),
    (CssPropertyType::FlexWrap, "flex-wrap"),
    (CssPropertyType::FlexDirection, "flex-direction"),
    (CssPropertyType::FlexGrow, "flex-grow"),
    (CssPropertyType::FlexShrink, "flex-shrink"),
    (CssPropertyType::FlexBasis, "flex-basis"),
    (CssPropertyType::JustifyContent, "justify-content"),
    (CssPropertyType::AlignItems, "align-items"),
    (CssPropertyType::AlignContent, "align-content"),
    (CssPropertyType::ColumnGap, "column-gap"),
    (CssPropertyType::RowGap, "row-gap"),
    (
        CssPropertyType::GridTemplateColumns,
        "grid-template-columns",
    ),
    (CssPropertyType::GridTemplateRows, "grid-template-rows"),
    (CssPropertyType::GridAutoColumns, "grid-auto-columns"),
    (CssPropertyType::GridAutoRows, "grid-auto-rows"),
    (CssPropertyType::GridColumn, "grid-column"),
    (CssPropertyType::GridRow, "grid-row"),
    (CssPropertyType::GridTemplateAreas, "grid-template-areas"),
    (CssPropertyType::WritingMode, "writing-mode"),
    (CssPropertyType::Clear, "clear"),
    (CssPropertyType::OverflowX, "overflow-x"),
    (CssPropertyType::OverflowY, "overflow-y"),
    (CssPropertyType::PaddingTop, "padding-top"),
    (CssPropertyType::PaddingLeft, "padding-left"),
    (CssPropertyType::PaddingRight, "padding-right"),
    (CssPropertyType::PaddingBottom, "padding-bottom"),
    (CssPropertyType::PaddingInlineStart, "padding-inline-start"),
    (CssPropertyType::PaddingInlineEnd, "padding-inline-end"),
    (CssPropertyType::MarginTop, "margin-top"),
    (CssPropertyType::MarginLeft, "margin-left"),
    (CssPropertyType::MarginRight, "margin-right"),
    (CssPropertyType::MarginBottom, "margin-bottom"),
    (CssPropertyType::BackgroundContent, "background"),
    (CssPropertyType::BackgroundPosition, "background-position"),
    (CssPropertyType::BackgroundSize, "background-size"),
    (CssPropertyType::BackgroundRepeat, "background-repeat"),
    (
        CssPropertyType::BorderTopLeftRadius,
        "border-top-left-radius",
    ),
    (
        CssPropertyType::BorderTopRightRadius,
        "border-top-right-radius",
    ),
    (
        CssPropertyType::BorderBottomLeftRadius,
        "border-bottom-left-radius",
    ),
    (
        CssPropertyType::BorderBottomRightRadius,
        "border-bottom-right-radius",
    ),
    (CssPropertyType::BorderTopColor, "border-top-color"),
    (CssPropertyType::BorderRightColor, "border-right-color"),
    (CssPropertyType::BorderLeftColor, "border-left-color"),
    (CssPropertyType::BorderBottomColor, "border-bottom-color"),
    (CssPropertyType::BorderTopStyle, "border-top-style"),
    (CssPropertyType::BorderRightStyle, "border-right-style"),
    (CssPropertyType::BorderLeftStyle, "border-left-style"),
    (CssPropertyType::BorderBottomStyle, "border-bottom-style"),
    (CssPropertyType::BorderTopWidth, "border-top-width"),
    (CssPropertyType::BorderRightWidth, "border-right-width"),
    (CssPropertyType::BorderLeftWidth, "border-left-width"),
    (CssPropertyType::BorderBottomWidth, "border-bottom-width"),
    (CssPropertyType::BoxShadowTop, "-azul-box-shadow-top"),
    (CssPropertyType::BoxShadowRight, "-azul-box-shadow-right"),
    (CssPropertyType::BoxShadowLeft, "-azul-box-shadow-left"),
    (CssPropertyType::BoxShadowBottom, "-azul-box-shadow-bottom"),
    (CssPropertyType::ScrollbarTrack, "-azul-scrollbar-track"),
    (CssPropertyType::ScrollbarThumb, "-azul-scrollbar-thumb"),
    (CssPropertyType::ScrollbarButton, "-azul-scrollbar-button"),
    (CssPropertyType::ScrollbarCorner, "-azul-scrollbar-corner"),
    (CssPropertyType::ScrollbarResizer, "-azul-scrollbar-resizer"),
    (CssPropertyType::CaretColor, "caret-color"),
    (
        CssPropertyType::CaretAnimationDuration,
        "caret-animation-duration",
    ),
    (CssPropertyType::CaretWidth, "-azul-caret-width"),
    (
        CssPropertyType::SelectionBackgroundColor,
        "-azul-selection-background-color",
    ),
    (CssPropertyType::SelectionColor, "-azul-selection-color"),
    (CssPropertyType::SelectionRadius, "-azul-selection-radius"),
    (CssPropertyType::ScrollbarWidth, "scrollbar-width"),
    (CssPropertyType::ScrollbarColor, "scrollbar-color"),
    (CssPropertyType::ScrollbarVisibility, "-azul-scrollbar-visibility"),
    (CssPropertyType::ScrollbarFadeDelay, "-azul-scrollbar-fade-delay"),
    (CssPropertyType::ScrollbarFadeDuration, "-azul-scrollbar-fade-duration"),
    (CssPropertyType::Opacity, "opacity"),
    (CssPropertyType::Visibility, "visibility"),
    (CssPropertyType::Transform, "transform"),
    (CssPropertyType::PerspectiveOrigin, "perspective-origin"),
    (CssPropertyType::TransformOrigin, "transform-origin"),
    (CssPropertyType::BackfaceVisibility, "backface-visibility"),
    (CssPropertyType::MixBlendMode, "mix-blend-mode"),
    (CssPropertyType::Filter, "filter"),
    (CssPropertyType::BackdropFilter, "backdrop-filter"),
    (CssPropertyType::TextShadow, "text-shadow"),
    (CssPropertyType::GridAutoFlow, "grid-auto-flow"),
    (CssPropertyType::JustifySelf, "justify-self"),
    (CssPropertyType::JustifyItems, "justify-items"),
    (CssPropertyType::Gap, "gap"),
    (CssPropertyType::GridGap, "grid-gap"),
    (CssPropertyType::AlignSelf, "align-self"),
    (CssPropertyType::Font, "font"),
    (CssPropertyType::BreakBefore, "break-before"),
    (CssPropertyType::BreakAfter, "break-after"),
    (CssPropertyType::BreakInside, "break-inside"),
    // CSS 2.1 legacy aliases for page breaking
    (CssPropertyType::BreakBefore, "page-break-before"),
    (CssPropertyType::BreakAfter, "page-break-after"),
    (CssPropertyType::BreakInside, "page-break-inside"),
    (CssPropertyType::Orphans, "orphans"),
    (CssPropertyType::Widows, "widows"),
    (CssPropertyType::BoxDecorationBreak, "box-decoration-break"),
    (CssPropertyType::ColumnCount, "column-count"),
    (CssPropertyType::ColumnWidth, "column-width"),
    (CssPropertyType::ColumnSpan, "column-span"),
    (CssPropertyType::ColumnFill, "column-fill"),
    (CssPropertyType::ColumnRuleWidth, "column-rule-width"),
    (CssPropertyType::ColumnRuleStyle, "column-rule-style"),
    (CssPropertyType::ColumnRuleColor, "column-rule-color"),
    (CssPropertyType::FlowInto, "flow-into"),
    (CssPropertyType::FlowFrom, "flow-from"),
    (CssPropertyType::ShapeOutside, "shape-outside"),
    (CssPropertyType::ShapeInside, "shape-inside"),
    (CssPropertyType::ClipPath, "clip-path"),
    (CssPropertyType::ShapeMargin, "shape-margin"),
    (
        CssPropertyType::ShapeImageThreshold,
        "shape-image-threshold",
    ),
    (CssPropertyType::Content, "content"),
    (CssPropertyType::CounterReset, "counter-reset"),
    (CssPropertyType::CounterIncrement, "counter-increment"),
    (CssPropertyType::ListStyleType, "list-style-type"),
    (CssPropertyType::ListStylePosition, "list-style-position"),
    (CssPropertyType::StringSet, "string-set"),
];

// Type aliases for `CssPropertyValue<T>`
pub type CaretColorValue = CssPropertyValue<CaretColor>;
pub type CaretAnimationDurationValue = CssPropertyValue<CaretAnimationDuration>;
pub type CaretWidthValue = CssPropertyValue<CaretWidth>;
pub type SelectionBackgroundColorValue = CssPropertyValue<SelectionBackgroundColor>;
pub type SelectionColorValue = CssPropertyValue<SelectionColor>;
pub type SelectionRadiusValue = CssPropertyValue<SelectionRadius>;
pub type StyleBackgroundContentVecValue = CssPropertyValue<StyleBackgroundContentVec>;
pub type StyleBackgroundPositionVecValue = CssPropertyValue<StyleBackgroundPositionVec>;
pub type StyleBackgroundSizeVecValue = CssPropertyValue<StyleBackgroundSizeVec>;
pub type StyleBackgroundRepeatVecValue = CssPropertyValue<StyleBackgroundRepeatVec>;
pub type StyleFontSizeValue = CssPropertyValue<StyleFontSize>;
pub type StyleFontFamilyVecValue = CssPropertyValue<StyleFontFamilyVec>;
pub type StyleFontWeightValue = CssPropertyValue<StyleFontWeight>;
pub type StyleFontStyleValue = CssPropertyValue<StyleFontStyle>;
pub type StyleTextColorValue = CssPropertyValue<StyleTextColor>;
pub type StyleTextAlignValue = CssPropertyValue<StyleTextAlign>;
pub type StyleVerticalAlignValue = CssPropertyValue<StyleVerticalAlign>;
pub type StyleLineHeightValue = CssPropertyValue<StyleLineHeight>;
pub type StyleLetterSpacingValue = CssPropertyValue<StyleLetterSpacing>;
pub type StyleTextIndentValue = CssPropertyValue<StyleTextIndent>;
pub type StyleInitialLetterValue = CssPropertyValue<StyleInitialLetter>;
pub type StyleLineClampValue = CssPropertyValue<StyleLineClamp>;
pub type StyleHangingPunctuationValue = CssPropertyValue<StyleHangingPunctuation>;
pub type StyleTextCombineUprightValue = CssPropertyValue<StyleTextCombineUpright>;
pub type StyleExclusionMarginValue = CssPropertyValue<StyleExclusionMargin>;
pub type StyleHyphenationLanguageValue = CssPropertyValue<StyleHyphenationLanguage>;
pub type StyleWordSpacingValue = CssPropertyValue<StyleWordSpacing>;
pub type StyleTabSizeValue = CssPropertyValue<StyleTabSize>;
pub type StyleCursorValue = CssPropertyValue<StyleCursor>;
pub type StyleBoxShadowValue = CssPropertyValue<StyleBoxShadow>;
pub type StyleBorderTopColorValue = CssPropertyValue<StyleBorderTopColor>;
pub type StyleBorderLeftColorValue = CssPropertyValue<StyleBorderLeftColor>;
pub type StyleBorderRightColorValue = CssPropertyValue<StyleBorderRightColor>;
pub type StyleBorderBottomColorValue = CssPropertyValue<StyleBorderBottomColor>;
pub type StyleBorderTopStyleValue = CssPropertyValue<StyleBorderTopStyle>;
pub type StyleBorderLeftStyleValue = CssPropertyValue<StyleBorderLeftStyle>;
pub type StyleBorderRightStyleValue = CssPropertyValue<StyleBorderRightStyle>;
pub type StyleBorderBottomStyleValue = CssPropertyValue<StyleBorderBottomStyle>;
pub type StyleBorderTopLeftRadiusValue = CssPropertyValue<StyleBorderTopLeftRadius>;
pub type StyleBorderTopRightRadiusValue = CssPropertyValue<StyleBorderTopRightRadius>;
pub type StyleBorderBottomLeftRadiusValue = CssPropertyValue<StyleBorderBottomLeftRadius>;
pub type StyleBorderBottomRightRadiusValue = CssPropertyValue<StyleBorderBottomRightRadius>;
pub type StyleOpacityValue = CssPropertyValue<StyleOpacity>;
pub type StyleVisibilityValue = CssPropertyValue<StyleVisibility>;
pub type StyleTransformVecValue = CssPropertyValue<StyleTransformVec>;
pub type StyleTransformOriginValue = CssPropertyValue<StyleTransformOrigin>;
pub type StylePerspectiveOriginValue = CssPropertyValue<StylePerspectiveOrigin>;
pub type StyleBackfaceVisibilityValue = CssPropertyValue<StyleBackfaceVisibility>;
pub type StyleMixBlendModeValue = CssPropertyValue<StyleMixBlendMode>;
pub type StyleFilterVecValue = CssPropertyValue<StyleFilterVec>;
pub type StyleBackgroundContentValue = CssPropertyValue<StyleBackgroundContent>;
pub type LayoutScrollbarWidthValue = CssPropertyValue<LayoutScrollbarWidth>;
pub type StyleScrollbarColorValue = CssPropertyValue<StyleScrollbarColor>;
pub type ScrollbarVisibilityModeValue = CssPropertyValue<ScrollbarVisibilityMode>;
pub type ScrollbarFadeDelayValue = CssPropertyValue<ScrollbarFadeDelay>;
pub type ScrollbarFadeDurationValue = CssPropertyValue<ScrollbarFadeDuration>;
pub type LayoutDisplayValue = CssPropertyValue<LayoutDisplay>;
pub type StyleHyphensValue = CssPropertyValue<StyleHyphens>;
pub type StyleDirectionValue = CssPropertyValue<StyleDirection>;
pub type StyleUserSelectValue = CssPropertyValue<StyleUserSelect>;
pub type StyleTextDecorationValue = CssPropertyValue<StyleTextDecoration>;
pub type StyleWhiteSpaceValue = CssPropertyValue<StyleWhiteSpace>;
pub type LayoutFloatValue = CssPropertyValue<LayoutFloat>;
pub type LayoutBoxSizingValue = CssPropertyValue<LayoutBoxSizing>;
pub type LayoutWidthValue = CssPropertyValue<LayoutWidth>;
pub type LayoutHeightValue = CssPropertyValue<LayoutHeight>;
pub type LayoutMinWidthValue = CssPropertyValue<LayoutMinWidth>;
pub type LayoutMinHeightValue = CssPropertyValue<LayoutMinHeight>;
pub type LayoutMaxWidthValue = CssPropertyValue<LayoutMaxWidth>;
pub type LayoutMaxHeightValue = CssPropertyValue<LayoutMaxHeight>;
pub type LayoutPositionValue = CssPropertyValue<LayoutPosition>;
pub type LayoutTopValue = CssPropertyValue<LayoutTop>;
pub type LayoutInsetBottomValue = CssPropertyValue<LayoutInsetBottom>;
pub type LayoutRightValue = CssPropertyValue<LayoutRight>;
pub type LayoutLeftValue = CssPropertyValue<LayoutLeft>;
pub type LayoutZIndexValue = CssPropertyValue<LayoutZIndex>;
pub type LayoutPaddingTopValue = CssPropertyValue<LayoutPaddingTop>;
pub type LayoutPaddingBottomValue = CssPropertyValue<LayoutPaddingBottom>;
pub type LayoutPaddingLeftValue = CssPropertyValue<LayoutPaddingLeft>;
pub type LayoutPaddingRightValue = CssPropertyValue<LayoutPaddingRight>;
pub type LayoutPaddingInlineStartValue = CssPropertyValue<LayoutPaddingInlineStart>;
pub type LayoutPaddingInlineEndValue = CssPropertyValue<LayoutPaddingInlineEnd>;
pub type LayoutMarginTopValue = CssPropertyValue<LayoutMarginTop>;
pub type LayoutMarginBottomValue = CssPropertyValue<LayoutMarginBottom>;
pub type LayoutTextJustifyValue = CssPropertyValue<LayoutTextJustify>;
pub type LayoutMarginLeftValue = CssPropertyValue<LayoutMarginLeft>;
pub type LayoutMarginRightValue = CssPropertyValue<LayoutMarginRight>;
pub type LayoutBorderTopWidthValue = CssPropertyValue<LayoutBorderTopWidth>;
pub type LayoutBorderLeftWidthValue = CssPropertyValue<LayoutBorderLeftWidth>;
pub type LayoutBorderRightWidthValue = CssPropertyValue<LayoutBorderRightWidth>;
pub type LayoutBorderBottomWidthValue = CssPropertyValue<LayoutBorderBottomWidth>;
pub type LayoutOverflowValue = CssPropertyValue<LayoutOverflow>;
pub type LayoutFlexDirectionValue = CssPropertyValue<LayoutFlexDirection>;
pub type LayoutFlexWrapValue = CssPropertyValue<LayoutFlexWrap>;
pub type LayoutFlexGrowValue = CssPropertyValue<LayoutFlexGrow>;
pub type LayoutFlexShrinkValue = CssPropertyValue<LayoutFlexShrink>;
pub type LayoutFlexBasisValue = CssPropertyValue<LayoutFlexBasis>;
pub type LayoutJustifyContentValue = CssPropertyValue<LayoutJustifyContent>;
pub type LayoutAlignItemsValue = CssPropertyValue<LayoutAlignItems>;
pub type LayoutAlignContentValue = CssPropertyValue<LayoutAlignContent>;
pub type LayoutColumnGapValue = CssPropertyValue<LayoutColumnGap>;
pub type LayoutRowGapValue = CssPropertyValue<LayoutRowGap>;
pub type LayoutGridTemplateColumnsValue = CssPropertyValue<GridTemplate>;
pub type LayoutGridTemplateRowsValue = CssPropertyValue<GridTemplate>;
pub type LayoutGridAutoColumnsValue = CssPropertyValue<GridAutoTracks>;
pub type LayoutGridAutoRowsValue = CssPropertyValue<GridAutoTracks>;
pub type LayoutGridColumnValue = CssPropertyValue<GridPlacement>;
pub type LayoutGridRowValue = CssPropertyValue<GridPlacement>;
pub type LayoutGridTemplateAreasValue = CssPropertyValue<crate::props::layout::grid::GridTemplateAreas>;
pub type LayoutWritingModeValue = CssPropertyValue<LayoutWritingMode>;
pub type LayoutClearValue = CssPropertyValue<LayoutClear>;
pub type LayoutGridAutoFlowValue = CssPropertyValue<LayoutGridAutoFlow>;
pub type LayoutJustifySelfValue = CssPropertyValue<LayoutJustifySelf>;
pub type LayoutJustifyItemsValue = CssPropertyValue<LayoutJustifyItems>;
pub type LayoutGapValue = CssPropertyValue<LayoutGap>;
pub type LayoutAlignSelfValue = CssPropertyValue<LayoutAlignSelf>;
pub type StyleFontValue = CssPropertyValue<StyleFontFamilyVec>;
pub type PageBreakValue = CssPropertyValue<PageBreak>;
pub type BreakInsideValue = CssPropertyValue<BreakInside>;
pub type WidowsValue = CssPropertyValue<Widows>;
pub type OrphansValue = CssPropertyValue<Orphans>;
pub type BoxDecorationBreakValue = CssPropertyValue<BoxDecorationBreak>;
pub type ColumnCountValue = CssPropertyValue<ColumnCount>;
pub type ColumnWidthValue = CssPropertyValue<ColumnWidth>;
pub type ColumnSpanValue = CssPropertyValue<ColumnSpan>;
pub type ColumnFillValue = CssPropertyValue<ColumnFill>;
pub type ColumnRuleWidthValue = CssPropertyValue<ColumnRuleWidth>;
pub type ColumnRuleStyleValue = CssPropertyValue<ColumnRuleStyle>;
pub type ColumnRuleColorValue = CssPropertyValue<ColumnRuleColor>;
pub type FlowIntoValue = CssPropertyValue<FlowInto>;
pub type FlowFromValue = CssPropertyValue<FlowFrom>;
pub type ShapeOutsideValue = CssPropertyValue<ShapeOutside>;
pub type ShapeInsideValue = CssPropertyValue<ShapeInside>;
pub type ClipPathValue = CssPropertyValue<ClipPath>;
pub type ShapeMarginValue = CssPropertyValue<ShapeMargin>;
pub type ShapeImageThresholdValue = CssPropertyValue<ShapeImageThreshold>;
pub type LayoutTableLayoutValue = CssPropertyValue<LayoutTableLayout>;
pub type StyleBorderCollapseValue = CssPropertyValue<StyleBorderCollapse>;
pub type LayoutBorderSpacingValue = CssPropertyValue<LayoutBorderSpacing>;
pub type StyleCaptionSideValue = CssPropertyValue<StyleCaptionSide>;
pub type StyleEmptyCellsValue = CssPropertyValue<StyleEmptyCells>;
pub type ContentValue = CssPropertyValue<Content>;
pub type CounterResetValue = CssPropertyValue<CounterReset>;
pub type CounterIncrementValue = CssPropertyValue<CounterIncrement>;
pub type StyleListStyleTypeValue = CssPropertyValue<StyleListStyleType>;
pub type StyleListStylePositionValue = CssPropertyValue<StyleListStylePosition>;
pub type StringSetValue = CssPropertyValue<StringSet>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CssKeyMap {
    // Contains all keys that have no shorthand
    pub non_shorthands: BTreeMap<&'static str, CssPropertyType>,
    // Contains all keys that act as a shorthand for other types
    pub shorthands: BTreeMap<&'static str, CombinedCssPropertyType>,
}

impl CssKeyMap {
    pub fn get() -> Self {
        get_css_key_map()
    }
}

/// Returns a map useful for parsing the keys of CSS stylesheets
pub fn get_css_key_map() -> CssKeyMap {
    CssKeyMap {
        non_shorthands: CSS_PROPERTY_KEY_MAP.iter().map(|(v, k)| (*k, *v)).collect(),
        shorthands: COMBINED_CSS_PROPERTIES_KEY_MAP
            .iter()
            .map(|(v, k)| (*k, *v))
            .collect(),
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum CombinedCssPropertyType {
    BorderRadius,
    Overflow,
    Margin,
    Border,
    BorderLeft,
    BorderRight,
    BorderTop,
    BorderBottom,
    BorderColor,
    BorderStyle,
    BorderWidth,
    Padding,
    BoxShadow,
    BackgroundColor, // BackgroundContent::Color
    BackgroundImage, // BackgroundContent::Image
    Background,
    Flex,
    Grid,
    Gap,
    GridGap,
    Font,
    Columns,
    ColumnRule,
    GridArea,
}

impl fmt::Display for CombinedCssPropertyType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let key = COMBINED_CSS_PROPERTIES_KEY_MAP
            .iter()
            .find(|(v, _)| *v == *self)
            .and_then(|(k, _)| Some(k))
            .unwrap();
        write!(f, "{}", key)
    }
}

impl CombinedCssPropertyType {
    /// Parses a CSS key, such as `width` from a string:
    ///
    /// # Example
    ///
    /// ```rust
    /// # use azul_css::props::property::{CombinedCssPropertyType, get_css_key_map};
    /// let map = get_css_key_map();
    /// assert_eq!(
    ///     Some(CombinedCssPropertyType::Border),
    ///     CombinedCssPropertyType::from_str("border", &map)
    /// );
    /// ```
    pub fn from_str(input: &str, map: &CssKeyMap) -> Option<Self> {
        let input = input.trim();
        map.shorthands.get(input).map(|x| *x)
    }

    /// Returns the original string that was used to construct this `CssPropertyType`.
    pub fn to_str(&self, map: &CssKeyMap) -> &'static str {
        map.shorthands
            .iter()
            .find(|(_, v)| *v == self)
            .map(|(k, _)| k)
            .unwrap()
    }
}

/// Represents one parsed CSS key-value pair, such as `"width: 20px"` =>
/// `CssProperty::Width(LayoutWidth::px(20.0))`
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum CssProperty {
    CaretColor(CaretColorValue),
    CaretAnimationDuration(CaretAnimationDurationValue),
    CaretWidth(CaretWidthValue),
    SelectionBackgroundColor(SelectionBackgroundColorValue),
    SelectionColor(SelectionColorValue),
    SelectionRadius(SelectionRadiusValue),
    TextColor(StyleTextColorValue),
    FontSize(StyleFontSizeValue),
    FontFamily(StyleFontFamilyVecValue),
    FontWeight(StyleFontWeightValue),
    FontStyle(StyleFontStyleValue),
    TextAlign(StyleTextAlignValue),
    TextJustify(LayoutTextJustifyValue),
    VerticalAlign(StyleVerticalAlignValue),
    LetterSpacing(StyleLetterSpacingValue),
    TextIndent(StyleTextIndentValue),
    InitialLetter(StyleInitialLetterValue),
    LineClamp(StyleLineClampValue),
    HangingPunctuation(StyleHangingPunctuationValue),
    TextCombineUpright(StyleTextCombineUprightValue),
    ExclusionMargin(StyleExclusionMarginValue),
    HyphenationLanguage(StyleHyphenationLanguageValue),
    LineHeight(StyleLineHeightValue),
    WordSpacing(StyleWordSpacingValue),
    TabSize(StyleTabSizeValue),
    WhiteSpace(StyleWhiteSpaceValue),
    Hyphens(StyleHyphensValue),
    Direction(StyleDirectionValue),
    UserSelect(StyleUserSelectValue),
    TextDecoration(StyleTextDecorationValue),
    Cursor(StyleCursorValue),
    Display(LayoutDisplayValue),
    Float(LayoutFloatValue),
    BoxSizing(LayoutBoxSizingValue),
    Width(LayoutWidthValue),
    Height(LayoutHeightValue),
    MinWidth(LayoutMinWidthValue),
    MinHeight(LayoutMinHeightValue),
    MaxWidth(LayoutMaxWidthValue),
    MaxHeight(LayoutMaxHeightValue),
    Position(LayoutPositionValue),
    Top(LayoutTopValue),
    Right(LayoutRightValue),
    Left(LayoutLeftValue),
    Bottom(LayoutInsetBottomValue),
    ZIndex(LayoutZIndexValue),
    FlexWrap(LayoutFlexWrapValue),
    FlexDirection(LayoutFlexDirectionValue),
    FlexGrow(LayoutFlexGrowValue),
    FlexShrink(LayoutFlexShrinkValue),
    FlexBasis(LayoutFlexBasisValue),
    JustifyContent(LayoutJustifyContentValue),
    AlignItems(LayoutAlignItemsValue),
    AlignContent(LayoutAlignContentValue),
    ColumnGap(LayoutColumnGapValue),
    RowGap(LayoutRowGapValue),
    GridTemplateColumns(LayoutGridTemplateColumnsValue),
    GridTemplateRows(LayoutGridTemplateRowsValue),
    GridAutoColumns(LayoutGridAutoColumnsValue),
    GridAutoRows(LayoutGridAutoRowsValue),
    GridColumn(LayoutGridColumnValue),
    GridRow(LayoutGridRowValue),
    GridTemplateAreas(LayoutGridTemplateAreasValue),
    WritingMode(LayoutWritingModeValue),
    Clear(LayoutClearValue),
    BackgroundContent(StyleBackgroundContentVecValue),
    BackgroundPosition(StyleBackgroundPositionVecValue),
    BackgroundSize(StyleBackgroundSizeVecValue),
    BackgroundRepeat(StyleBackgroundRepeatVecValue),
    OverflowX(LayoutOverflowValue),
    OverflowY(LayoutOverflowValue),
    GridAutoFlow(LayoutGridAutoFlowValue),
    JustifySelf(LayoutJustifySelfValue),
    JustifyItems(LayoutJustifyItemsValue),
    Gap(LayoutGapValue),
    GridGap(LayoutGapValue),
    AlignSelf(LayoutAlignSelfValue),
    Font(StyleFontValue),
    PaddingTop(LayoutPaddingTopValue),
    PaddingLeft(LayoutPaddingLeftValue),
    PaddingRight(LayoutPaddingRightValue),
    PaddingBottom(LayoutPaddingBottomValue),
    PaddingInlineStart(LayoutPaddingInlineStartValue),
    PaddingInlineEnd(LayoutPaddingInlineEndValue),
    MarginTop(LayoutMarginTopValue),
    MarginLeft(LayoutMarginLeftValue),
    MarginRight(LayoutMarginRightValue),
    MarginBottom(LayoutMarginBottomValue),
    BorderTopLeftRadius(StyleBorderTopLeftRadiusValue),
    LayoutTextJustify(LayoutTextJustifyValue),
    BorderTopRightRadius(StyleBorderTopRightRadiusValue),
    BorderBottomLeftRadius(StyleBorderBottomLeftRadiusValue),
    BorderBottomRightRadius(StyleBorderBottomRightRadiusValue),
    BorderTopColor(StyleBorderTopColorValue),
    BorderRightColor(StyleBorderRightColorValue),
    BorderLeftColor(StyleBorderLeftColorValue),
    BorderBottomColor(StyleBorderBottomColorValue),
    BorderTopStyle(StyleBorderTopStyleValue),
    BorderRightStyle(StyleBorderRightStyleValue),
    BorderLeftStyle(StyleBorderLeftStyleValue),
    BorderBottomStyle(StyleBorderBottomStyleValue),
    BorderTopWidth(LayoutBorderTopWidthValue),
    BorderRightWidth(LayoutBorderRightWidthValue),
    BorderLeftWidth(LayoutBorderLeftWidthValue),
    BorderBottomWidth(LayoutBorderBottomWidthValue),
    BoxShadowLeft(StyleBoxShadowValue),
    BoxShadowRight(StyleBoxShadowValue),
    BoxShadowTop(StyleBoxShadowValue),
    BoxShadowBottom(StyleBoxShadowValue),
    ScrollbarTrack(StyleBackgroundContentValue),
    ScrollbarThumb(StyleBackgroundContentValue),
    ScrollbarButton(StyleBackgroundContentValue),
    ScrollbarCorner(StyleBackgroundContentValue),
    ScrollbarResizer(StyleBackgroundContentValue),
    ScrollbarWidth(LayoutScrollbarWidthValue),
    ScrollbarColor(StyleScrollbarColorValue),
    ScrollbarVisibility(ScrollbarVisibilityModeValue),
    ScrollbarFadeDelay(ScrollbarFadeDelayValue),
    ScrollbarFadeDuration(ScrollbarFadeDurationValue),
    Opacity(StyleOpacityValue),
    Visibility(StyleVisibilityValue),
    Transform(StyleTransformVecValue),
    TransformOrigin(StyleTransformOriginValue),
    PerspectiveOrigin(StylePerspectiveOriginValue),
    BackfaceVisibility(StyleBackfaceVisibilityValue),
    MixBlendMode(StyleMixBlendModeValue),
    Filter(StyleFilterVecValue),
    BackdropFilter(StyleFilterVecValue),
    TextShadow(StyleBoxShadowValue),
    BreakBefore(PageBreakValue),
    BreakAfter(PageBreakValue),
    BreakInside(BreakInsideValue),
    Orphans(OrphansValue),
    Widows(WidowsValue),
    BoxDecorationBreak(BoxDecorationBreakValue),
    ColumnCount(ColumnCountValue),
    ColumnWidth(ColumnWidthValue),
    ColumnSpan(ColumnSpanValue),
    ColumnFill(ColumnFillValue),
    ColumnRuleWidth(ColumnRuleWidthValue),
    ColumnRuleStyle(ColumnRuleStyleValue),
    ColumnRuleColor(ColumnRuleColorValue),
    FlowInto(FlowIntoValue),
    FlowFrom(FlowFromValue),
    ShapeOutside(ShapeOutsideValue),
    ShapeInside(ShapeInsideValue),
    ClipPath(ClipPathValue),
    ShapeMargin(ShapeMarginValue),
    ShapeImageThreshold(ShapeImageThresholdValue),
    TableLayout(LayoutTableLayoutValue),
    BorderCollapse(StyleBorderCollapseValue),
    BorderSpacing(LayoutBorderSpacingValue),
    CaptionSide(StyleCaptionSideValue),
    EmptyCells(StyleEmptyCellsValue),
    Content(ContentValue),
    CounterReset(CounterResetValue),
    CounterIncrement(CounterIncrementValue),
    ListStyleType(StyleListStyleTypeValue),
    ListStylePosition(StyleListStylePositionValue),
    StringSet(StringSetValue),
}

impl_option!(
    CssProperty,
    OptionCssProperty,
    copy = false,
    [Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord]
);

crate::impl_vec!(CssProperty, CssPropertyVec, CssPropertyVecDestructor, CssPropertyVecDestructorType, CssPropertyVecSlice, OptionCssProperty);
crate::impl_vec_clone!(CssProperty, CssPropertyVec, CssPropertyVecDestructor);
crate::impl_vec_debug!(CssProperty, CssPropertyVec);
crate::impl_vec_partialeq!(CssProperty, CssPropertyVec);
crate::impl_vec_eq!(CssProperty, CssPropertyVec);
crate::impl_vec_partialord!(CssProperty, CssPropertyVec);
crate::impl_vec_ord!(CssProperty, CssPropertyVec);
crate::impl_vec_hash!(CssProperty, CssPropertyVec);

/// Categorizes a CSS property by its effect on the layout pipeline.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssPropertyCategory {
    GpuOnly,
    /// Affects geometry (width, height, margin, padding, font-size, etc.)
    Layout,
    /// Affects only appearance (color, background-color, etc.)
    Paint,
    /// A layout-affecting property that also requires children to be re-evaluated.
    InheritedLayout,
    /// A paint-affecting property that also requires children to be re-evaluated.
    InheritedPaint,
}

/// Fine-grained dirty classification for CSS property changes.
///
/// Inspired by Taffy's binary dirty flag but extended to 4 levels for CSS-specific
/// optimizations. Instead of "clean vs dirty", we classify property changes by their
/// actual layout impact, enabling the engine to skip unnecessary work.
///
/// Reference: Taffy (https://github.com/DioxusLabs/taffy) uses a binary dirty flag
/// (clean/dirty). Our improvement: 4-level classification enables IFC-only reflow,
/// sizing-only recomputation, and paint-only updates without full subtree relayout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum RelayoutScope {
    /// No relayout needed — repaint only (e.g., color, background, opacity, transform).
    /// The node's size and position are unchanged.
    None,
    /// Only the IFC (Inline Formatting Context) containing this node needs re-shaping.
    /// Block-level siblings are unaffected unless the IFC height changes,
    /// in which case this auto-upgrades to SizingOnly.
    IfcOnly,
    /// This node's sizing needs recomputation. Parent may need repositioning
    /// of subsequent siblings but doesn't need full recursive relayout.
    SizingOnly,
    /// Full subtree relayout required (e.g., display, position, float change).
    Full,
}

impl Default for RelayoutScope {
    fn default() -> Self {
        RelayoutScope::None
    }
}

/// Represents a CSS key (for example `"border-radius"` => `BorderRadius`).
/// You can also derive this key from a `CssProperty` by calling `CssProperty::get_type()`.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, strum_macros::EnumIter)]
#[repr(C)]
pub enum CssPropertyType {
    CaretColor,
    CaretAnimationDuration,
    CaretWidth,
    SelectionBackgroundColor,
    SelectionColor,
    SelectionRadius,
    TextColor,
    FontSize,
    FontFamily,
    FontWeight,
    FontStyle,
    TextAlign,
    TextJustify,
    VerticalAlign,
    LetterSpacing,
    TextIndent,
    InitialLetter,
    LineClamp,
    HangingPunctuation,
    TextCombineUpright,
    ExclusionMargin,
    HyphenationLanguage,
    LineHeight,
    WordSpacing,
    TabSize,
    WhiteSpace,
    Hyphens,
    Direction,
    UserSelect,
    TextDecoration,
    Cursor,
    Display,
    Float,
    BoxSizing,
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    Position,
    Top,
    Right,
    Left,
    Bottom,
    ZIndex,
    FlexWrap,
    FlexDirection,
    FlexGrow,
    FlexShrink,
    FlexBasis,
    JustifyContent,
    AlignItems,
    AlignContent,
    ColumnGap,
    RowGap,
    GridTemplateColumns,
    GridTemplateRows,
    GridAutoColumns,
    GridAutoRows,
    GridColumn,
    GridRow,
    GridTemplateAreas,
    GridAutoFlow,
    JustifySelf,
    JustifyItems,
    Gap,
    GridGap,
    AlignSelf,
    Font,
    WritingMode,
    Clear,
    BackgroundContent,
    BackgroundPosition,
    BackgroundSize,
    BackgroundRepeat,
    OverflowX,
    OverflowY,
    PaddingTop,
    PaddingLeft,
    PaddingRight,
    PaddingBottom,
    PaddingInlineStart,
    PaddingInlineEnd,
    MarginTop,
    MarginLeft,
    MarginRight,
    MarginBottom,
    BorderTopLeftRadius,
    BorderTopRightRadius,
    BorderBottomLeftRadius,
    BorderBottomRightRadius,
    BorderTopColor,
    BorderRightColor,
    BorderLeftColor,
    BorderBottomColor,
    BorderTopStyle,
    BorderRightStyle,
    BorderLeftStyle,
    BorderBottomStyle,
    BorderTopWidth,
    BorderRightWidth,
    BorderLeftWidth,
    BorderBottomWidth,
    BoxShadowLeft,
    BoxShadowRight,
    BoxShadowTop,
    BoxShadowBottom,
    ScrollbarTrack,
    ScrollbarThumb,
    ScrollbarButton,
    ScrollbarCorner,
    ScrollbarResizer,
    ScrollbarWidth,
    ScrollbarColor,
    ScrollbarVisibility,
    ScrollbarFadeDelay,
    ScrollbarFadeDuration,
    Opacity,
    Visibility,
    Transform,
    TransformOrigin,
    PerspectiveOrigin,
    BackfaceVisibility,
    MixBlendMode,
    Filter,
    BackdropFilter,
    TextShadow,
    BreakBefore,
    BreakAfter,
    BreakInside,
    Orphans,
    Widows,
    BoxDecorationBreak,
    ColumnCount,
    ColumnWidth,
    ColumnSpan,
    ColumnFill,
    ColumnRuleWidth,
    ColumnRuleStyle,
    ColumnRuleColor,
    FlowInto,
    FlowFrom,
    ShapeOutside,
    ShapeInside,
    ClipPath,
    ShapeMargin,
    ShapeImageThreshold,
    TableLayout,
    BorderCollapse,
    BorderSpacing,
    CaptionSide,
    EmptyCells,
    Content,
    CounterReset,
    CounterIncrement,
    ListStyleType,
    ListStylePosition,
    StringSet,
}

impl fmt::Debug for CssPropertyType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl fmt::Display for CssPropertyType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl CssPropertyType {
    /// Parses a CSS key, such as `width` from a string:
    ///
    /// # Example
    ///
    /// ```rust
    /// # use azul_css::props::property::{CssPropertyType, get_css_key_map};
    /// let map = get_css_key_map();
    /// assert_eq!(
    ///     Some(CssPropertyType::Width),
    ///     CssPropertyType::from_str("width", &map)
    /// );
    /// assert_eq!(
    ///     Some(CssPropertyType::JustifyContent),
    ///     CssPropertyType::from_str("justify-content", &map)
    /// );
    /// assert_eq!(None, CssPropertyType::from_str("asdfasdfasdf", &map));
    /// ```
    pub fn from_str(input: &str, map: &CssKeyMap) -> Option<Self> {
        let input = input.trim();
        map.non_shorthands.get(input).and_then(|x| Some(*x))
    }

    /// Returns the original string that was used to construct this `CssPropertyType`.
    pub fn to_str(&self) -> &'static str {
        match self {
            CssPropertyType::CaretColor => "caret-color",
            CssPropertyType::CaretAnimationDuration => "caret-animation-duration",
            CssPropertyType::CaretWidth => "-azul-caret-width",
            CssPropertyType::SelectionBackgroundColor => "-azul-selection-background-color",
            CssPropertyType::SelectionColor => "-azul-selection-color",
            CssPropertyType::SelectionRadius => "-azul-selection-radius",
            CssPropertyType::TextColor => "color",
            CssPropertyType::FontSize => "font-size",
            CssPropertyType::FontFamily => "font-family",
            CssPropertyType::FontWeight => "font-weight",
            CssPropertyType::FontStyle => "font-style",
            CssPropertyType::TextAlign => "text-align",
            CssPropertyType::TextJustify => "text-justify",
            CssPropertyType::VerticalAlign => "vertical-align",
            CssPropertyType::LetterSpacing => "letter-spacing",
            CssPropertyType::TextIndent => "text-indent",
            CssPropertyType::InitialLetter => "initial-letter",
            CssPropertyType::LineClamp => "line-clamp",
            CssPropertyType::HangingPunctuation => "hanging-punctuation",
            CssPropertyType::TextCombineUpright => "text-combine-upright",
            CssPropertyType::ExclusionMargin => "-azul-exclusion-margin",
            CssPropertyType::HyphenationLanguage => "-azul-hyphenation-language",
            CssPropertyType::LineHeight => "line-height",
            CssPropertyType::WordSpacing => "word-spacing",
            CssPropertyType::TabSize => "tab-size",
            CssPropertyType::Cursor => "cursor",
            CssPropertyType::Display => "display",
            CssPropertyType::Float => "float",
            CssPropertyType::BoxSizing => "box-sizing",
            CssPropertyType::Width => "width",
            CssPropertyType::Height => "height",
            CssPropertyType::MinWidth => "min-width",
            CssPropertyType::MinHeight => "min-height",
            CssPropertyType::MaxWidth => "max-width",
            CssPropertyType::MaxHeight => "max-height",
            CssPropertyType::Position => "position",
            CssPropertyType::Top => "top",
            CssPropertyType::Right => "right",
            CssPropertyType::Left => "left",
            CssPropertyType::Bottom => "bottom",
            CssPropertyType::ZIndex => "z-index",
            CssPropertyType::FlexWrap => "flex-wrap",
            CssPropertyType::FlexDirection => "flex-direction",
            CssPropertyType::FlexGrow => "flex-grow",
            CssPropertyType::FlexShrink => "flex-shrink",
            CssPropertyType::FlexBasis => "flex-basis",
            CssPropertyType::JustifyContent => "justify-content",
            CssPropertyType::AlignItems => "align-items",
            CssPropertyType::AlignContent => "align-content",
            CssPropertyType::ColumnGap => "column-gap",
            CssPropertyType::RowGap => "row-gap",
            CssPropertyType::GridTemplateColumns => "grid-template-columns",
            CssPropertyType::GridTemplateRows => "grid-template-rows",
            CssPropertyType::GridAutoFlow => "grid-auto-flow",
            CssPropertyType::JustifySelf => "justify-self",
            CssPropertyType::JustifyItems => "justify-items",
            CssPropertyType::Gap => "gap",
            CssPropertyType::GridGap => "grid-gap",
            CssPropertyType::AlignSelf => "align-self",
            CssPropertyType::Font => "font",
            CssPropertyType::GridAutoColumns => "grid-auto-columns",
            CssPropertyType::GridAutoRows => "grid-auto-rows",
            CssPropertyType::GridColumn => "grid-column",
            CssPropertyType::GridRow => "grid-row",
            CssPropertyType::GridTemplateAreas => "grid-template-areas",
            CssPropertyType::WritingMode => "writing-mode",
            CssPropertyType::Clear => "clear",
            CssPropertyType::BackgroundContent => "background",
            CssPropertyType::BackgroundPosition => "background-position",
            CssPropertyType::BackgroundSize => "background-size",
            CssPropertyType::BackgroundRepeat => "background-repeat",
            CssPropertyType::OverflowX => "overflow-x",
            CssPropertyType::OverflowY => "overflow-y",
            CssPropertyType::PaddingTop => "padding-top",
            CssPropertyType::PaddingLeft => "padding-left",
            CssPropertyType::PaddingRight => "padding-right",
            CssPropertyType::PaddingBottom => "padding-bottom",
            CssPropertyType::PaddingInlineStart => "padding-inline-start",
            CssPropertyType::PaddingInlineEnd => "padding-inline-end",
            CssPropertyType::MarginTop => "margin-top",
            CssPropertyType::MarginLeft => "margin-left",
            CssPropertyType::MarginRight => "margin-right",
            CssPropertyType::MarginBottom => "margin-bottom",
            CssPropertyType::BorderTopLeftRadius => "border-top-left-radius",
            CssPropertyType::BorderTopRightRadius => "border-top-right-radius",
            CssPropertyType::BorderBottomLeftRadius => "border-bottom-left-radius",
            CssPropertyType::BorderBottomRightRadius => "border-bottom-right-radius",
            CssPropertyType::BorderTopColor => "border-top-color",
            CssPropertyType::BorderRightColor => "border-right-color",
            CssPropertyType::BorderLeftColor => "border-left-color",
            CssPropertyType::BorderBottomColor => "border-bottom-color",
            CssPropertyType::BorderTopStyle => "border-top-style",
            CssPropertyType::BorderRightStyle => "border-right-style",
            CssPropertyType::BorderLeftStyle => "border-left-style",
            CssPropertyType::BorderBottomStyle => "border-bottom-style",
            CssPropertyType::BorderTopWidth => "border-top-width",
            CssPropertyType::BorderRightWidth => "border-right-width",
            CssPropertyType::BorderLeftWidth => "border-left-width",
            CssPropertyType::BorderBottomWidth => "border-bottom-width",
            CssPropertyType::BoxShadowLeft => "-azul-box-shadow-left",
            CssPropertyType::BoxShadowRight => "-azul-box-shadow-right",
            CssPropertyType::BoxShadowTop => "-azul-box-shadow-top",
            CssPropertyType::BoxShadowBottom => "-azul-box-shadow-bottom",
            CssPropertyType::ScrollbarTrack => "-azul-scrollbar-track",
            CssPropertyType::ScrollbarThumb => "-azul-scrollbar-thumb",
            CssPropertyType::ScrollbarButton => "-azul-scrollbar-button",
            CssPropertyType::ScrollbarCorner => "-azul-scrollbar-corner",
            CssPropertyType::ScrollbarResizer => "-azul-scrollbar-resizer",
            CssPropertyType::ScrollbarWidth => "scrollbar-width",
            CssPropertyType::ScrollbarColor => "scrollbar-color",
            CssPropertyType::ScrollbarVisibility => "-azul-scrollbar-visibility",
            CssPropertyType::ScrollbarFadeDelay => "-azul-scrollbar-fade-delay",
            CssPropertyType::ScrollbarFadeDuration => "-azul-scrollbar-fade-duration",
            CssPropertyType::Opacity => "opacity",
            CssPropertyType::Visibility => "visibility",
            CssPropertyType::Transform => "transform",
            CssPropertyType::TransformOrigin => "transform-origin",
            CssPropertyType::PerspectiveOrigin => "perspective-origin",
            CssPropertyType::BackfaceVisibility => "backface-visibility",
            CssPropertyType::MixBlendMode => "mix-blend-mode",
            CssPropertyType::Filter => "filter",
            CssPropertyType::BackdropFilter => "backdrop-filter",
            CssPropertyType::TextShadow => "text-shadow",
            CssPropertyType::WhiteSpace => "white-space",
            CssPropertyType::Hyphens => "hyphens",
            CssPropertyType::Direction => "direction",
            CssPropertyType::UserSelect => "user-select",
            CssPropertyType::TextDecoration => "text-decoration",
            CssPropertyType::BreakBefore => "break-before",
            CssPropertyType::BreakAfter => "break-after",
            CssPropertyType::BreakInside => "break-inside",
            CssPropertyType::Orphans => "orphans",
            CssPropertyType::Widows => "widows",
            CssPropertyType::BoxDecorationBreak => "box-decoration-break",
            CssPropertyType::ColumnCount => "column-count",
            CssPropertyType::ColumnWidth => "column-width",
            CssPropertyType::ColumnSpan => "column-span",
            CssPropertyType::ColumnFill => "column-fill",
            CssPropertyType::ColumnRuleWidth => "column-rule-width",
            CssPropertyType::ColumnRuleStyle => "column-rule-style",
            CssPropertyType::ColumnRuleColor => "column-rule-color",
            CssPropertyType::FlowInto => "flow-into",
            CssPropertyType::FlowFrom => "flow-from",
            CssPropertyType::ShapeOutside => "shape-outside",
            CssPropertyType::ShapeInside => "shape-inside",
            CssPropertyType::ClipPath => "clip-path",
            CssPropertyType::ShapeMargin => "shape-margin",
            CssPropertyType::ShapeImageThreshold => "shape-image-threshold",
            CssPropertyType::TableLayout => "table-layout",
            CssPropertyType::BorderCollapse => "border-collapse",
            CssPropertyType::BorderSpacing => "border-spacing",
            CssPropertyType::CaptionSide => "caption-side",
            CssPropertyType::EmptyCells => "empty-cells",
            CssPropertyType::Content => "content",
            CssPropertyType::CounterReset => "counter-reset",
            CssPropertyType::CounterIncrement => "counter-increment",
            CssPropertyType::ListStyleType => "list-style-type",
            CssPropertyType::ListStylePosition => "list-style-position",
            CssPropertyType::StringSet => "string-set",
        }
    }

    /// Returns whether this property will be inherited during cascading
    /// Returns whether this CSS property is inherited by default according to CSS specifications.
    ///
    /// Reference: https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Cascade/Inheritance
    pub fn is_inheritable(&self) -> bool {
        use self::CssPropertyType::*;
        match self {
            // Font properties
            FontFamily | FontSize | FontWeight | FontStyle | LineHeight | LetterSpacing | WordSpacing | TextIndent |

            // Text properties
            TextColor | TextAlign | TextJustify | TextDecoration | WhiteSpace | Direction | Hyphens | TabSize |
            HangingPunctuation | TextCombineUpright | HyphenationLanguage |

            // List properties
            ListStyleType | ListStylePosition |

            // Table properties
            BorderCollapse | BorderSpacing | CaptionSide | EmptyCells |

            // Other inherited properties
            // NOTE: Cursor is inheritable per CSS spec (https://developer.mozilla.org/en-US/docs/Web/CSS/cursor)
            // This means a Button with cursor:pointer will pass that to child Text nodes.
            // This is correct behavior - if you want text inside a button to show I-beam,
            // the Text node needs an explicit cursor:text style that overrides the inherited value.
            Visibility | Cursor | Widows | Orphans |

            // Writing mode
            WritingMode |

            // User interaction
            UserSelect
            => true,

            _ => false,
        }
    }

    pub fn get_category(&self) -> CssPropertyCategory {
        if self.is_gpu_only_property() {
            CssPropertyCategory::GpuOnly
        } else {
            let is_inheritable = self.is_inheritable();
            let can_trigger_layout = self.can_trigger_relayout();
            match (is_inheritable, can_trigger_layout) {
                (true, true) => CssPropertyCategory::InheritedLayout,
                (true, false) => CssPropertyCategory::InheritedPaint,
                (false, true) => CssPropertyCategory::Layout,
                (false, false) => CssPropertyCategory::Paint,
            }
        }
    }

    /// Returns whether this property can trigger a re-layout (important for incremental layout and
    /// caching layouted DOMs).
    pub fn can_trigger_relayout(&self) -> bool {
        use self::CssPropertyType::*;

        // Since the border can be larger than the content,
        // in which case the content needs to be re-layouted, assume true for Border

        // FontFamily, FontSize, LetterSpacing and LineHeight can affect
        // the text layout and therefore the screen layout

        match self {
            TextColor
            | Cursor
            | BackgroundContent
            | BackgroundPosition
            | BackgroundSize
            | BackgroundRepeat
            | BorderTopLeftRadius
            | BorderTopRightRadius
            | BorderBottomLeftRadius
            | BorderBottomRightRadius
            | BorderTopColor
            | BorderRightColor
            | BorderLeftColor
            | BorderBottomColor
            | BorderTopStyle
            | BorderRightStyle
            | BorderLeftStyle
            | BorderBottomStyle
            | ColumnRuleColor
            | ColumnRuleStyle
            | BoxShadowLeft
            | BoxShadowRight
            | BoxShadowTop
            | BoxShadowBottom
            | BoxDecorationBreak
            | ScrollbarTrack
            | ScrollbarThumb
            | ScrollbarButton
            | ScrollbarCorner
            | ScrollbarResizer
            | Opacity
            | Transform
            | TransformOrigin
            | PerspectiveOrigin
            | BackfaceVisibility
            | MixBlendMode
            | Filter
            | BackdropFilter
            | TextShadow => false,
            _ => true,
        }
    }

    /// Returns whether the property is a GPU property (currently only opacity and transforms)
    pub fn is_gpu_only_property(&self) -> bool {
        match self {
            CssPropertyType::Opacity |
            CssPropertyType::Transform /* | CssPropertyType::Color */ => true,
            _ => false
        }
    }

    /// Context-dependent relayout scope for a CSS property change.
    ///
    /// This is a more granular replacement for `can_trigger_relayout()`.
    /// Instead of returning a flat bool, it classifies the property change
    /// into one of four impact levels (see `RelayoutScope`).
    ///
    /// Inspired by Taffy's binary dirty flag, extended with CSS-specific
    /// knowledge: font/text changes only affect IFC, sizing changes don't
    /// require full subtree relayout, and paint-only changes skip layout entirely.
    ///
    /// `node_is_ifc_member`: whether this node participates in an IFC
    /// (has inline formatting context membership). When true, font/text
    /// property changes trigger IFC-only relayout instead of being ignored.
    pub fn relayout_scope(&self, node_is_ifc_member: bool) -> RelayoutScope {
        use CssPropertyType::*;
        match self {
            // Pure paint — never triggers relayout
            TextColor | Cursor | BackgroundContent | BackgroundPosition
            | BackgroundSize | BackgroundRepeat | BorderTopColor | BorderRightColor
            | BorderLeftColor | BorderBottomColor | BorderTopStyle | BorderRightStyle
            | BorderLeftStyle | BorderBottomStyle | BorderTopLeftRadius
            | BorderTopRightRadius | BorderBottomLeftRadius | BorderBottomRightRadius
            | ColumnRuleColor | ColumnRuleStyle | BoxShadowLeft | BoxShadowRight
            | BoxShadowTop | BoxShadowBottom | BoxDecorationBreak
            | ScrollbarTrack | ScrollbarThumb | ScrollbarButton
            | ScrollbarCorner | ScrollbarResizer
            | Opacity | Transform | TransformOrigin | PerspectiveOrigin
            | BackfaceVisibility | MixBlendMode | Filter | BackdropFilter
            | TextShadow | SelectionBackgroundColor | SelectionColor
            | SelectionRadius | CaretColor | CaretAnimationDuration
            | CaretWidth => RelayoutScope::None,

            // Font/text properties — IFC-only if inside inline context,
            // otherwise no layout impact (block with only block children
            // inherits but doesn't directly reflow).
            FontFamily | FontSize | FontWeight | FontStyle
            | LetterSpacing | WordSpacing | LineHeight | TextAlign | TextJustify
            | TextIndent | WhiteSpace | TabSize | Hyphens
            | HyphenationLanguage | TextCombineUpright | TextDecoration
            | HangingPunctuation | InitialLetter | LineClamp
            | Direction | VerticalAlign => {
                if node_is_ifc_member {
                    RelayoutScope::IfcOnly
                } else {
                    // Block container with only block children: font properties
                    // are inherited but don't affect this node's own sizing.
                    // Children pick up the change via inheritance and get their
                    // own dirty flags.
                    RelayoutScope::None
                }
            }

            // Sizing properties — only this node's size changes.
            // Parent may reposition subsequent siblings but doesn't need
            // full recursive relayout of unaffected subtrees.
            Width | Height | MinWidth | MinHeight | MaxWidth | MaxHeight
            | PaddingTop | PaddingRight | PaddingBottom | PaddingLeft
            | PaddingInlineStart | PaddingInlineEnd
            | BorderTopWidth | BorderRightWidth | BorderBottomWidth
            | BorderLeftWidth | BoxSizing
            | ScrollbarWidth | ScrollbarVisibility => RelayoutScope::SizingOnly,

            // Everything else: display, position, float, margin, flex-*,
            // grid-*, overflow, writing-mode, etc. — full relayout.
            _ => RelayoutScope::Full,
        }
    }
}

// -- PARSING --

/// Master error type that aggregates all possible CSS parsing errors.
#[derive(Clone, PartialEq)]
pub enum CssParsingError<'a> {
    // Shorthand properties
    Border(CssBorderParseError<'a>),
    BorderRadius(CssStyleBorderRadiusParseError<'a>),
    Padding(LayoutPaddingParseError<'a>),
    Margin(LayoutMarginParseError<'a>),
    Overflow(InvalidValueErr<'a>),
    BoxShadow(CssShadowParseError<'a>),

    // Individual properties
    Color(CssColorParseError<'a>),
    PixelValue(CssPixelValueParseError<'a>),
    Percentage(PercentageParseError),
    FontFamily(CssStyleFontFamilyParseError<'a>),
    InvalidValue(InvalidValueErr<'a>),
    FlexGrow(FlexGrowParseError<'a>),
    FlexShrink(FlexShrinkParseError<'a>),
    Background(CssBackgroundParseError<'a>),
    BackgroundPosition(CssBackgroundPositionParseError<'a>),
    Opacity(OpacityParseError<'a>),
    Visibility(StyleVisibilityParseError<'a>),
    LayoutScrollbarWidth(LayoutScrollbarWidthParseError<'a>),
    StyleScrollbarColor(StyleScrollbarColorParseError<'a>),
    ScrollbarVisibilityMode(ScrollbarVisibilityModeParseError<'a>),
    ScrollbarFadeDelay(ScrollbarFadeDelayParseError<'a>),
    ScrollbarFadeDuration(ScrollbarFadeDurationParseError<'a>),
    Transform(CssStyleTransformParseError<'a>),
    TransformOrigin(CssStyleTransformOriginParseError<'a>),
    PerspectiveOrigin(CssStylePerspectiveOriginParseError<'a>),
    Filter(CssStyleFilterParseError<'a>),

    // Text/Style properties
    TextColor(StyleTextColorParseError<'a>),
    FontSize(CssStyleFontSizeParseError<'a>),
    FontWeight(CssFontWeightParseError<'a>),
    FontStyle(CssFontStyleParseError<'a>),
    TextAlign(StyleTextAlignParseError<'a>),
    TextJustify(TextJustifyParseError<'a>),
    VerticalAlign(StyleVerticalAlignParseError<'a>),
    LetterSpacing(StyleLetterSpacingParseError<'a>),
    TextIndent(StyleTextIndentParseError<'a>),
    InitialLetter(StyleInitialLetterParseError<'a>),
    LineClamp(StyleLineClampParseError<'a>),
    HangingPunctuation(StyleHangingPunctuationParseError<'a>),
    TextCombineUpright(StyleTextCombineUprightParseError<'a>),
    ExclusionMargin(StyleExclusionMarginParseError),
    HyphenationLanguage(StyleHyphenationLanguageParseError),
    LineHeight(StyleLineHeightParseError),
    WordSpacing(StyleWordSpacingParseError<'a>),
    TabSize(StyleTabSizeParseError<'a>),
    WhiteSpace(StyleWhiteSpaceParseError<'a>),
    Hyphens(StyleHyphensParseError<'a>),
    Direction(StyleDirectionParseError<'a>),
    UserSelect(StyleUserSelectParseError<'a>),
    TextDecoration(StyleTextDecorationParseError<'a>),
    Cursor(CursorParseError<'a>),
    CaretColor(CssColorParseError<'a>),
    CaretAnimationDuration(DurationParseError<'a>),
    CaretWidth(CssPixelValueParseError<'a>),
    SelectionBackgroundColor(CssColorParseError<'a>),
    SelectionColor(CssColorParseError<'a>),
    SelectionRadius(CssPixelValueParseError<'a>),

    // Layout basic properties
    LayoutDisplay(LayoutDisplayParseError<'a>),
    LayoutFloat(LayoutFloatParseError<'a>),
    LayoutBoxSizing(LayoutBoxSizingParseError<'a>),

    // Layout dimensions
    LayoutWidth(LayoutWidthParseError<'a>),
    LayoutHeight(LayoutHeightParseError<'a>),
    LayoutMinWidth(LayoutMinWidthParseError<'a>),
    LayoutMinHeight(LayoutMinHeightParseError<'a>),
    LayoutMaxWidth(LayoutMaxWidthParseError<'a>),
    LayoutMaxHeight(LayoutMaxHeightParseError<'a>),

    // Layout position
    LayoutPosition(LayoutPositionParseError<'a>),
    LayoutTop(LayoutTopParseError<'a>),
    LayoutRight(LayoutRightParseError<'a>),
    LayoutLeft(LayoutLeftParseError<'a>),
    LayoutInsetBottom(LayoutInsetBottomParseError<'a>),
    LayoutZIndex(LayoutZIndexParseError<'a>),

    // Layout flex
    FlexWrap(FlexWrapParseError<'a>),
    FlexDirection(FlexDirectionParseError<'a>),
    FlexBasis(FlexBasisParseError<'a>),
    JustifyContent(JustifyContentParseError<'a>),
    AlignItems(AlignItemsParseError<'a>),
    AlignContent(AlignContentParseError<'a>),

    // Layout grid
    Grid(GridParseError<'a>),
    GridAutoFlow(GridAutoFlowParseError<'a>),
    JustifySelf(JustifySelfParseError<'a>),
    JustifyItems(JustifyItemsParseError<'a>),
    AlignSelf(AlignSelfParseError<'a>),

    // Layout wrapping
    LayoutWrap(LayoutWrapParseError<'a>),
    LayoutWritingMode(LayoutWritingModeParseError<'a>),
    LayoutClear(LayoutClearParseError<'a>),

    // Layout overflow
    LayoutOverflow(LayoutOverflowParseError<'a>),

    // Border radius individual corners
    BorderTopLeftRadius(StyleBorderTopLeftRadiusParseError<'a>),
    BorderTopRightRadius(StyleBorderTopRightRadiusParseError<'a>),
    BorderBottomLeftRadius(StyleBorderBottomLeftRadiusParseError<'a>),
    BorderBottomRightRadius(StyleBorderBottomRightRadiusParseError<'a>),

    // Border style
    BorderStyle(CssBorderStyleParseError<'a>),

    // Effects
    BackfaceVisibility(CssBackfaceVisibilityParseError<'a>),
    MixBlendMode(MixBlendModeParseError<'a>),

    // Fragmentation
    PageBreak(PageBreakParseError<'a>),
    BreakInside(BreakInsideParseError<'a>),
    Widows(WidowsParseError<'a>),
    Orphans(OrphansParseError<'a>),
    BoxDecorationBreak(BoxDecorationBreakParseError<'a>),

    // Columns
    ColumnCount(ColumnCountParseError<'a>),
    ColumnWidth(ColumnWidthParseError<'a>),
    ColumnSpan(ColumnSpanParseError<'a>),
    ColumnFill(ColumnFillParseError<'a>),
    ColumnRuleWidth(ColumnRuleWidthParseError<'a>),
    ColumnRuleStyle(ColumnRuleStyleParseError<'a>),
    ColumnRuleColor(ColumnRuleColorParseError<'a>),

    // Flow & Shape
    FlowInto(FlowIntoParseError<'a>),
    FlowFrom(FlowFromParseError<'a>),
    GenericParseError,

    // Content
    Content, // Simplified errors for now
    Counter,
    ListStyleType(StyleListStyleTypeParseError<'a>),
    ListStylePosition(StyleListStylePositionParseError<'a>),
    StringSet,
}

/// Owned version of `CssParsingError`.
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssParsingErrorOwned {
    // Shorthand properties
    Border(CssBorderParseErrorOwned),
    BorderRadius(CssStyleBorderRadiusParseErrorOwned),
    Padding(LayoutPaddingParseErrorOwned),
    Margin(LayoutMarginParseErrorOwned),
    Overflow(InvalidValueErrOwned),
    BoxShadow(CssShadowParseErrorOwned),

    // Individual properties
    Color(CssColorParseErrorOwned),
    PixelValue(CssPixelValueParseErrorOwned),
    Percentage(PercentageParseError),
    FontFamily(CssStyleFontFamilyParseErrorOwned),
    InvalidValue(InvalidValueErrOwned),
    FlexGrow(FlexGrowParseErrorOwned),
    FlexShrink(FlexShrinkParseErrorOwned),
    Background(CssBackgroundParseErrorOwned),
    BackgroundPosition(CssBackgroundPositionParseErrorOwned),
    Opacity(OpacityParseErrorOwned),
    Visibility(StyleVisibilityParseErrorOwned),
    LayoutScrollbarWidth(LayoutScrollbarWidthParseErrorOwned),
    StyleScrollbarColor(StyleScrollbarColorParseErrorOwned),
    ScrollbarVisibilityMode(ScrollbarVisibilityModeParseErrorOwned),
    ScrollbarFadeDelay(ScrollbarFadeDelayParseErrorOwned),
    ScrollbarFadeDuration(ScrollbarFadeDurationParseErrorOwned),
    Transform(CssStyleTransformParseErrorOwned),
    TransformOrigin(CssStyleTransformOriginParseErrorOwned),
    PerspectiveOrigin(CssStylePerspectiveOriginParseErrorOwned),
    Filter(CssStyleFilterParseErrorOwned),

    // Text/Style properties
    TextColor(StyleTextColorParseErrorOwned),
    FontSize(CssStyleFontSizeParseErrorOwned),
    FontWeight(CssFontWeightParseErrorOwned),
    FontStyle(CssFontStyleParseErrorOwned),
    TextAlign(StyleTextAlignParseErrorOwned),
    TextJustify(TextJustifyParseErrorOwned),
    VerticalAlign(StyleVerticalAlignParseErrorOwned),
    LetterSpacing(StyleLetterSpacingParseErrorOwned),
    TextIndent(StyleTextIndentParseErrorOwned),
    InitialLetter(StyleInitialLetterParseErrorOwned),
    LineClamp(StyleLineClampParseErrorOwned),
    HangingPunctuation(StyleHangingPunctuationParseErrorOwned),
    TextCombineUpright(StyleTextCombineUprightParseErrorOwned),
    ExclusionMargin(StyleExclusionMarginParseErrorOwned),
    HyphenationLanguage(StyleHyphenationLanguageParseErrorOwned),
    LineHeight(StyleLineHeightParseError),
    WordSpacing(StyleWordSpacingParseErrorOwned),
    TabSize(StyleTabSizeParseErrorOwned),
    WhiteSpace(StyleWhiteSpaceParseErrorOwned),
    Hyphens(StyleHyphensParseErrorOwned),
    Direction(StyleDirectionParseErrorOwned),
    UserSelect(StyleUserSelectParseErrorOwned),
    TextDecoration(StyleTextDecorationParseErrorOwned),
    Cursor(CursorParseErrorOwned),
    CaretColor(CssColorParseErrorOwned),
    CaretAnimationDuration(DurationParseErrorOwned),
    CaretWidth(CssPixelValueParseErrorOwned),
    SelectionBackgroundColor(CssColorParseErrorOwned),
    SelectionColor(CssColorParseErrorOwned),
    SelectionRadius(CssPixelValueParseErrorOwned),

    // Layout basic properties
    LayoutDisplay(LayoutDisplayParseErrorOwned),
    LayoutFloat(LayoutFloatParseErrorOwned),
    LayoutBoxSizing(LayoutBoxSizingParseErrorOwned),

    // Layout dimensions
    LayoutWidth(LayoutWidthParseErrorOwned),
    LayoutHeight(LayoutHeightParseErrorOwned),
    LayoutMinWidth(LayoutMinWidthParseErrorOwned),
    LayoutMinHeight(LayoutMinHeightParseErrorOwned),
    LayoutMaxWidth(LayoutMaxWidthParseErrorOwned),
    LayoutMaxHeight(LayoutMaxHeightParseErrorOwned),

    // Layout position
    LayoutPosition(LayoutPositionParseErrorOwned),
    LayoutTop(LayoutTopParseErrorOwned),
    LayoutRight(LayoutRightParseErrorOwned),
    LayoutLeft(LayoutLeftParseErrorOwned),
    LayoutInsetBottom(LayoutInsetBottomParseErrorOwned),
    LayoutZIndex(LayoutZIndexParseErrorOwned),

    // Layout flex
    FlexWrap(FlexWrapParseErrorOwned),
    FlexDirection(FlexDirectionParseErrorOwned),
    FlexBasis(FlexBasisParseErrorOwned),
    JustifyContent(JustifyContentParseErrorOwned),
    AlignItems(AlignItemsParseErrorOwned),
    AlignContent(AlignContentParseErrorOwned),

    // Layout grid
    Grid(GridParseErrorOwned),
    GridAutoFlow(GridAutoFlowParseErrorOwned),
    JustifySelf(JustifySelfParseErrorOwned),
    JustifyItems(JustifyItemsParseErrorOwned),
    AlignSelf(AlignSelfParseErrorOwned),

    // Layout wrapping
    LayoutWrap(LayoutWrapParseErrorOwned),
    LayoutWritingMode(LayoutWritingModeParseErrorOwned),
    LayoutClear(LayoutClearParseErrorOwned),

    // Layout overflow
    LayoutOverflow(LayoutOverflowParseErrorOwned),

    // Border radius individual corners
    BorderTopLeftRadius(StyleBorderTopLeftRadiusParseErrorOwned),
    BorderTopRightRadius(StyleBorderTopRightRadiusParseErrorOwned),
    BorderBottomLeftRadius(StyleBorderBottomLeftRadiusParseErrorOwned),
    BorderBottomRightRadius(StyleBorderBottomRightRadiusParseErrorOwned),

    // Border style
    BorderStyle(CssBorderStyleParseErrorOwned),

    // Effects
    BackfaceVisibility(CssBackfaceVisibilityParseErrorOwned),
    MixBlendMode(MixBlendModeParseErrorOwned),

    // Fragmentation
    PageBreak(PageBreakParseErrorOwned),
    BreakInside(BreakInsideParseErrorOwned),
    Widows(WidowsParseErrorOwned),
    Orphans(OrphansParseErrorOwned),
    BoxDecorationBreak(BoxDecorationBreakParseErrorOwned),

    // Columns
    ColumnCount(ColumnCountParseErrorOwned),
    ColumnWidth(ColumnWidthParseErrorOwned),
    ColumnSpan(ColumnSpanParseErrorOwned),
    ColumnFill(ColumnFillParseErrorOwned),
    ColumnRuleWidth(ColumnRuleWidthParseErrorOwned),
    ColumnRuleStyle(ColumnRuleStyleParseErrorOwned),
    ColumnRuleColor(ColumnRuleColorParseErrorOwned),

    // Flow & Shape
    FlowInto(FlowIntoParseErrorOwned),
    FlowFrom(FlowFromParseErrorOwned),
    GenericParseError,

    // Content
    Content,
    Counter,
    ListStyleType(StyleListStyleTypeParseErrorOwned),
    ListStylePosition(StyleListStylePositionParseErrorOwned),
    StringSet,
}

// -- PARSING ERROR IMPLEMENTATIONS --

impl_debug_as_display!(CssParsingError<'a>);
impl_display! { CssParsingError<'a>, {
    CaretColor(e) => format!("Invalid caret-color: {}", e),
    CaretAnimationDuration(e) => format!("Invalid caret-animation-duration: {}", e),
    CaretWidth(e) => format!("Invalid -azul-caret-width: {}", e),
    SelectionBackgroundColor(e) => format!("Invalid -azul-selection-background-color: {}", e),
    SelectionColor(e) => format!("Invalid -azul-selection-color: {}", e),
    SelectionRadius(e) => format!("Invalid -azul-selection-radius: {}", e),
    Border(e) => format!("Invalid border property: {}", e),
    BorderRadius(e) => format!("Invalid border-radius: {}", e),
    Padding(e) => format!("Invalid padding property: {}", e),
    Margin(e) => format!("Invalid margin property: {}", e),
    Overflow(e) => format!("Invalid overflow property: \"{}\"", e.0),
    BoxShadow(e) => format!("Invalid shadow property: {}", e),
    Color(e) => format!("Invalid color value: {}", e),
    PixelValue(e) => format!("Invalid pixel value: {}", e),
    Percentage(e) => format!("Invalid percentage value: {}", e),
    FontFamily(e) => format!("Invalid font-family value: {}", e),
    InvalidValue(e) => format!("Invalid value: \"{}\"", e.0),
    FlexGrow(e) => format!("Invalid flex-grow value: {}", e),
    FlexShrink(e) => format!("Invalid flex-shrink value: {}", e),
    Background(e) => format!("Invalid background property: {}", e),
    BackgroundPosition(e) => format!("Invalid background-position: {}", e),
    Opacity(e) => format!("Invalid opacity value: {}", e),
    Visibility(e) => format!("Invalid visibility value: {}", e),
    LayoutScrollbarWidth(e) => format!("Invalid scrollbar-width: {}", e),
    StyleScrollbarColor(e) => format!("Invalid scrollbar-color: {}", e),
    ScrollbarVisibilityMode(e) => format!("Invalid scrollbar-visibility: {}", e),
    ScrollbarFadeDelay(e) => format!("Invalid scrollbar-fade-delay: {}", e),
    ScrollbarFadeDuration(e) => format!("Invalid scrollbar-fade-duration: {}", e),
    Transform(e) => format!("Invalid transform property: {}", e),
    TransformOrigin(e) => format!("Invalid transform-origin: {}", e),
    PerspectiveOrigin(e) => format!("Invalid perspective-origin: {}", e),
    Filter(e) => format!("Invalid filter property: {}", e),
    LayoutWidth(e) => format!("Invalid width value: {}", e),
    LayoutHeight(e) => format!("Invalid height value: {}", e),
    LayoutMinWidth(e) => format!("Invalid min-width value: {}", e),
    LayoutMinHeight(e) => format!("Invalid min-height value: {}", e),
    LayoutMaxWidth(e) => format!("Invalid max-width value: {}", e),
    LayoutMaxHeight(e) => format!("Invalid max-height value: {}", e),
    LayoutPosition(e) => format!("Invalid position value: {}", e),
    LayoutTop(e) => format!("Invalid top value: {}", e),
    LayoutRight(e) => format!("Invalid right value: {}", e),
    LayoutLeft(e) => format!("Invalid left value: {}", e),
    LayoutInsetBottom(e) => format!("Invalid bottom value: {}", e),
    LayoutZIndex(e) => format!("Invalid z-index value: {}", e),
    FlexWrap(e) => format!("Invalid flex-wrap value: {}", e),
    FlexDirection(e) => format!("Invalid flex-direction value: {}", e),
    FlexBasis(e) => format!("Invalid flex-basis value: {}", e),
    JustifyContent(e) => format!("Invalid justify-content value: {}", e),
    AlignItems(e) => format!("Invalid align-items value: {}", e),
    AlignContent(e) => format!("Invalid align-content value: {}", e),
    GridAutoFlow(e) => format!("Invalid grid-auto-flow value: {}", e),
    JustifySelf(e) => format!("Invalid justify-self value: {}", e),
    JustifyItems(e) => format!("Invalid justify-items value: {}", e),
    AlignSelf(e) => format!("Invalid align-self value: {}", e),
    Grid(e) => format!("Invalid grid value: {}", e),
    LayoutWrap(e) => format!("Invalid wrap value: {}", e),
    LayoutWritingMode(e) => format!("Invalid writing-mode value: {}", e),
    LayoutClear(e) => format!("Invalid clear value: {}", e),
    LayoutOverflow(e) => format!("Invalid overflow value: {}", e),
    BorderTopLeftRadius(e) => format!("Invalid border-top-left-radius: {}", e),
    BorderTopRightRadius(e) => format!("Invalid border-top-right-radius: {}", e),
    BorderBottomLeftRadius(e) => format!("Invalid border-bottom-left-radius: {}", e),
    BorderBottomRightRadius(e) => format!("Invalid border-bottom-right-radius: {}", e),
    BorderStyle(e) => format!("Invalid border style: {}", e),
    BackfaceVisibility(e) => format!("Invalid backface-visibility: {}", e),
    MixBlendMode(e) => format!("Invalid mix-blend-mode: {}", e),
    TextColor(e) => format!("Invalid text color: {}", e),
    FontSize(e) => format!("Invalid font-size: {}", e),
    FontWeight(e) => format!("Invalid font-weight: {}", e),
    FontStyle(e) => format!("Invalid font-style: {}", e),
    TextAlign(e) => format!("Invalid text-align: {}", e),
    TextJustify(e) => format!("Invalid text-justify: {}", e),
    VerticalAlign(e) => format!("Invalid vertical-align: {}", e),
    LetterSpacing(e) => format!("Invalid letter-spacing: {}", e),
    TextIndent(e) => format!("Invalid text-indent: {}", e),
    InitialLetter(e) => format!("Invalid initial-letter: {}", e),
    LineClamp(e) => format!("Invalid line-clamp: {}", e),
    HangingPunctuation(e) => format!("Invalid hanging-punctuation: {}", e),
    TextCombineUpright(e) => format!("Invalid text-combine-upright: {}", e),
    ExclusionMargin(e) => format!("Invalid -azul-exclusion-margin: {}", e),
    HyphenationLanguage(e) => format!("Invalid -azul-hyphenation-language: {}", e),
    LineHeight(e) => format!("Invalid line-height: {}", e),
    WordSpacing(e) => format!("Invalid word-spacing: {}", e),
    TabSize(e) => format!("Invalid tab-size: {}", e),
    WhiteSpace(e) => format!("Invalid white-space: {}", e),
    Hyphens(e) => format!("Invalid hyphens: {}", e),
    Direction(e) => format!("Invalid direction: {}", e),
    UserSelect(e) => format!("Invalid user-select: {}", e),
    TextDecoration(e) => format!("Invalid text-decoration: {}", e),
    Cursor(e) => format!("Invalid cursor: {}", e),
    LayoutDisplay(e) => format!("Invalid display: {}", e),
    LayoutFloat(e) => format!("Invalid float: {}", e),
    LayoutBoxSizing(e) => format!("Invalid box-sizing: {}", e),
    PageBreak(e) => format!("Invalid break property: {}", e),
    BreakInside(e) => format!("Invalid break-inside property: {}", e),
    Widows(e) => format!("Invalid widows property: {}", e),
    Orphans(e) => format!("Invalid orphans property: {}", e),
    BoxDecorationBreak(e) => format!("Invalid box-decoration-break property: {}", e),
    ColumnCount(e) => format!("Invalid column-count: {}", e),
    ColumnWidth(e) => format!("Invalid column-width: {}", e),
    ColumnSpan(e) => format!("Invalid column-span: {}", e),
    ColumnFill(e) => format!("Invalid column-fill: {}", e),
    ColumnRuleWidth(e) => format!("Invalid column-rule-width: {}", e),
    ColumnRuleStyle(e) => format!("Invalid column-rule-style: {}", e),
    ColumnRuleColor(e) => format!("Invalid column-rule-color: {}", e),
    FlowInto(e) => format!("Invalid flow-into: {}", e),
    FlowFrom(e) => format!("Invalid flow-from: {}", e),
    GenericParseError => "Failed to parse value",
    Content => "Failed to parse content property",
    Counter => "Failed to parse counter property",
    ListStyleType(e) => format!("Invalid list-style-type: {}", e),
    ListStylePosition(e) => format!("Invalid list-style-position: {}", e),
    StringSet => "Failed to parse string-set property",
}}

// From impls for CssParsingError
impl_from!(
    DurationParseError<'a>,
    CssParsingError::CaretAnimationDuration
);
impl_from!(CssBorderParseError<'a>, CssParsingError::Border);
impl_from!(
    CssStyleBorderRadiusParseError<'a>,
    CssParsingError::BorderRadius
);
impl_from!(LayoutPaddingParseError<'a>, CssParsingError::Padding);
impl_from!(LayoutMarginParseError<'a>, CssParsingError::Margin);
impl_from!(CssShadowParseError<'a>, CssParsingError::BoxShadow);
impl_from!(CssColorParseError<'a>, CssParsingError::Color);
impl_from!(CssPixelValueParseError<'a>, CssParsingError::PixelValue);
impl_from!(
    CssStyleFontFamilyParseError<'a>,
    CssParsingError::FontFamily
);
impl_from!(CssFontWeightParseError<'a>, CssParsingError::FontWeight);
impl_from!(CssFontStyleParseError<'a>, CssParsingError::FontStyle);
impl_from!(
    StyleInitialLetterParseError<'a>,
    CssParsingError::InitialLetter
);
impl_from!(StyleLineClampParseError<'a>, CssParsingError::LineClamp);
impl_from!(
    StyleHangingPunctuationParseError<'a>,
    CssParsingError::HangingPunctuation
);
impl_from!(
    StyleTextCombineUprightParseError<'a>,
    CssParsingError::TextCombineUpright
);

// Manual From implementation for StyleExclusionMarginParseError (no lifetime)
#[cfg(feature = "parser")]
impl<'a> From<StyleExclusionMarginParseError> for CssParsingError<'a> {
    fn from(e: StyleExclusionMarginParseError) -> Self {
        CssParsingError::ExclusionMargin(e)
    }
}

// Manual From implementation for StyleHyphenationLanguageParseError (no lifetime)
#[cfg(feature = "parser")]
impl<'a> From<StyleHyphenationLanguageParseError> for CssParsingError<'a> {
    fn from(e: StyleHyphenationLanguageParseError) -> Self {
        CssParsingError::HyphenationLanguage(e)
    }
}
impl_from!(FlexGrowParseError<'a>, CssParsingError::FlexGrow);
impl_from!(FlexShrinkParseError<'a>, CssParsingError::FlexShrink);
impl_from!(CssBackgroundParseError<'a>, CssParsingError::Background);
impl_from!(
    CssBackgroundPositionParseError<'a>,
    CssParsingError::BackgroundPosition
);
impl_from!(OpacityParseError<'a>, CssParsingError::Opacity);
impl_from!(StyleVisibilityParseError<'a>, CssParsingError::Visibility);
impl_from!(
    LayoutScrollbarWidthParseError<'a>,
    CssParsingError::LayoutScrollbarWidth
);
impl_from!(
    StyleScrollbarColorParseError<'a>,
    CssParsingError::StyleScrollbarColor
);
impl_from!(
    ScrollbarVisibilityModeParseError<'a>,
    CssParsingError::ScrollbarVisibilityMode
);
impl_from!(
    ScrollbarFadeDelayParseError<'a>,
    CssParsingError::ScrollbarFadeDelay
);
impl_from!(
    ScrollbarFadeDurationParseError<'a>,
    CssParsingError::ScrollbarFadeDuration
);
impl_from!(CssStyleTransformParseError<'a>, CssParsingError::Transform);
impl_from!(
    CssStyleTransformOriginParseError<'a>,
    CssParsingError::TransformOrigin
);
impl_from!(
    CssStylePerspectiveOriginParseError<'a>,
    CssParsingError::PerspectiveOrigin
);
impl_from!(CssStyleFilterParseError<'a>, CssParsingError::Filter);

// Layout dimensions
impl_from!(LayoutWidthParseError<'a>, CssParsingError::LayoutWidth);
impl_from!(LayoutHeightParseError<'a>, CssParsingError::LayoutHeight);
impl_from!(
    LayoutMinWidthParseError<'a>,
    CssParsingError::LayoutMinWidth
);
impl_from!(
    LayoutMinHeightParseError<'a>,
    CssParsingError::LayoutMinHeight
);
impl_from!(
    LayoutMaxWidthParseError<'a>,
    CssParsingError::LayoutMaxWidth
);
impl_from!(
    LayoutMaxHeightParseError<'a>,
    CssParsingError::LayoutMaxHeight
);

// Layout position
impl_from!(
    LayoutPositionParseError<'a>,
    CssParsingError::LayoutPosition
);
impl_from!(LayoutTopParseError<'a>, CssParsingError::LayoutTop);
impl_from!(LayoutRightParseError<'a>, CssParsingError::LayoutRight);
impl_from!(LayoutLeftParseError<'a>, CssParsingError::LayoutLeft);
impl_from!(
    LayoutInsetBottomParseError<'a>,
    CssParsingError::LayoutInsetBottom
);
impl_from!(LayoutZIndexParseError<'a>, CssParsingError::LayoutZIndex);

// Layout flex
impl_from!(FlexWrapParseError<'a>, CssParsingError::FlexWrap);
impl_from!(FlexDirectionParseError<'a>, CssParsingError::FlexDirection);
impl_from!(FlexBasisParseError<'a>, CssParsingError::FlexBasis);
impl_from!(
    JustifyContentParseError<'a>,
    CssParsingError::JustifyContent
);
impl_from!(AlignItemsParseError<'a>, CssParsingError::AlignItems);
impl_from!(AlignContentParseError<'a>, CssParsingError::AlignContent);

// Layout grid
impl_from!(GridParseError<'a>, CssParsingError::Grid);
impl_from!(GridAutoFlowParseError<'a>, CssParsingError::GridAutoFlow);
impl_from!(JustifySelfParseError<'a>, CssParsingError::JustifySelf);
impl_from!(JustifyItemsParseError<'a>, CssParsingError::JustifyItems);
// pixel value impl_from already exists earlier; avoid duplicate impl
// impl_from!(CssPixelValueParseError<'a>, CssParsingError::PixelValue);
impl_from!(AlignSelfParseError<'a>, CssParsingError::AlignSelf);

// Layout wrapping
impl_from!(LayoutWrapParseError<'a>, CssParsingError::LayoutWrap);
impl_from!(
    LayoutWritingModeParseError<'a>,
    CssParsingError::LayoutWritingMode
);
impl_from!(LayoutClearParseError<'a>, CssParsingError::LayoutClear);

// Layout overflow
impl_from!(
    LayoutOverflowParseError<'a>,
    CssParsingError::LayoutOverflow
);

// Border radius individual corners
impl_from!(
    StyleBorderTopLeftRadiusParseError<'a>,
    CssParsingError::BorderTopLeftRadius
);
impl_from!(
    StyleBorderTopRightRadiusParseError<'a>,
    CssParsingError::BorderTopRightRadius
);
impl_from!(
    StyleBorderBottomLeftRadiusParseError<'a>,
    CssParsingError::BorderBottomLeftRadius
);
impl_from!(
    StyleBorderBottomRightRadiusParseError<'a>,
    CssParsingError::BorderBottomRightRadius
);

// Border style
impl_from!(CssBorderStyleParseError<'a>, CssParsingError::BorderStyle);

// Effects
impl_from!(
    CssBackfaceVisibilityParseError<'a>,
    CssParsingError::BackfaceVisibility
);
impl_from!(MixBlendModeParseError<'a>, CssParsingError::MixBlendMode);

// Text/Style properties
impl_from!(StyleTextColorParseError<'a>, CssParsingError::TextColor);
impl_from!(CssStyleFontSizeParseError<'a>, CssParsingError::FontSize);
impl_from!(StyleTextAlignParseError<'a>, CssParsingError::TextAlign);
impl_from!(TextJustifyParseError<'a>, CssParsingError::TextJustify);
impl_from!(
    StyleLetterSpacingParseError<'a>,
    CssParsingError::LetterSpacing
);
impl_from!(StyleWordSpacingParseError<'a>, CssParsingError::WordSpacing);
impl_from!(StyleTabSizeParseError<'a>, CssParsingError::TabSize);
impl_from!(StyleWhiteSpaceParseError<'a>, CssParsingError::WhiteSpace);
impl_from!(StyleHyphensParseError<'a>, CssParsingError::Hyphens);
impl_from!(StyleDirectionParseError<'a>, CssParsingError::Direction);
impl_from!(StyleUserSelectParseError<'a>, CssParsingError::UserSelect);
impl_from!(
    StyleTextDecorationParseError<'a>,
    CssParsingError::TextDecoration
);
impl_from!(CursorParseError<'a>, CssParsingError::Cursor);

// Layout basic properties
impl_from!(LayoutDisplayParseError<'a>, CssParsingError::LayoutDisplay);
impl_from!(LayoutFloatParseError<'a>, CssParsingError::LayoutFloat);
impl_from!(
    LayoutBoxSizingParseError<'a>,
    CssParsingError::LayoutBoxSizing
);

// DTP properties
impl_from!(PageBreakParseError<'a>, CssParsingError::PageBreak);
impl_from!(BreakInsideParseError<'a>, CssParsingError::BreakInside);
impl_from!(WidowsParseError<'a>, CssParsingError::Widows);
impl_from!(OrphansParseError<'a>, CssParsingError::Orphans);
impl_from!(
    BoxDecorationBreakParseError<'a>,
    CssParsingError::BoxDecorationBreak
);
impl_from!(ColumnCountParseError<'a>, CssParsingError::ColumnCount);
impl_from!(ColumnWidthParseError<'a>, CssParsingError::ColumnWidth);
impl_from!(ColumnSpanParseError<'a>, CssParsingError::ColumnSpan);
impl_from!(ColumnFillParseError<'a>, CssParsingError::ColumnFill);
impl_from!(
    ColumnRuleWidthParseError<'a>,
    CssParsingError::ColumnRuleWidth
);
impl_from!(
    ColumnRuleStyleParseError<'a>,
    CssParsingError::ColumnRuleStyle
);
impl_from!(
    ColumnRuleColorParseError<'a>,
    CssParsingError::ColumnRuleColor
);
impl_from!(FlowIntoParseError<'a>, CssParsingError::FlowInto);
impl_from!(FlowFromParseError<'a>, CssParsingError::FlowFrom);

impl<'a> From<InvalidValueErr<'a>> for CssParsingError<'a> {
    fn from(e: InvalidValueErr<'a>) -> Self {
        CssParsingError::InvalidValue(e)
    }
}

impl<'a> From<PercentageParseError> for CssParsingError<'a> {
    fn from(e: PercentageParseError) -> Self {
        CssParsingError::Percentage(e)
    }
}

impl<'a> From<StyleLineHeightParseError> for CssParsingError<'a> {
    fn from(e: StyleLineHeightParseError) -> Self {
        CssParsingError::LineHeight(e)
    }
}

impl<'a> From<StyleTextIndentParseError<'a>> for CssParsingError<'a> {
    fn from(e: StyleTextIndentParseError<'a>) -> Self {
        CssParsingError::TextIndent(e)
    }
}

impl<'a> From<StyleVerticalAlignParseError<'a>> for CssParsingError<'a> {
    fn from(e: StyleVerticalAlignParseError<'a>) -> Self {
        CssParsingError::VerticalAlign(e)
    }
}

impl<'a> CssParsingError<'a> {
    pub fn to_contained(&self) -> CssParsingErrorOwned {
        match self {
            CssParsingError::CaretColor(e) => CssParsingErrorOwned::CaretColor(e.to_contained()),
            CssParsingError::CaretWidth(e) => CssParsingErrorOwned::CaretWidth(e.to_contained()),
            CssParsingError::CaretAnimationDuration(e) => {
                CssParsingErrorOwned::CaretAnimationDuration(e.to_contained())
            }
            CssParsingError::SelectionBackgroundColor(e) => {
                CssParsingErrorOwned::SelectionBackgroundColor(e.to_contained())
            }
            CssParsingError::SelectionColor(e) => {
                CssParsingErrorOwned::SelectionColor(e.to_contained())
            }
            CssParsingError::SelectionRadius(e) => {
                CssParsingErrorOwned::SelectionRadius(e.to_contained())
            }
            CssParsingError::Border(e) => CssParsingErrorOwned::Border(e.to_contained().into()),
            CssParsingError::BorderRadius(e) => {
                CssParsingErrorOwned::BorderRadius(e.to_contained().into())
            }
            CssParsingError::Padding(e) => CssParsingErrorOwned::Padding(e.to_contained()),
            CssParsingError::Margin(e) => CssParsingErrorOwned::Margin(e.to_contained()),
            CssParsingError::Overflow(e) => CssParsingErrorOwned::Overflow(e.to_contained()),
            CssParsingError::BoxShadow(e) => CssParsingErrorOwned::BoxShadow(e.to_contained()),
            CssParsingError::Color(e) => CssParsingErrorOwned::Color(e.to_contained()),
            CssParsingError::PixelValue(e) => CssParsingErrorOwned::PixelValue(e.to_contained()),
            CssParsingError::Percentage(e) => CssParsingErrorOwned::Percentage(e.clone()),
            CssParsingError::FontFamily(e) => CssParsingErrorOwned::FontFamily(e.to_contained()),
            CssParsingError::InvalidValue(e) => {
                CssParsingErrorOwned::InvalidValue(e.to_contained())
            }
            CssParsingError::FlexGrow(e) => CssParsingErrorOwned::FlexGrow(e.to_contained()),
            CssParsingError::FlexShrink(e) => CssParsingErrorOwned::FlexShrink(e.to_contained()),
            CssParsingError::Background(e) => CssParsingErrorOwned::Background(e.to_contained()),
            CssParsingError::BackgroundPosition(e) => {
                CssParsingErrorOwned::BackgroundPosition(e.to_contained())
            }
            CssParsingError::GridAutoFlow(e) => {
                CssParsingErrorOwned::GridAutoFlow(e.to_contained())
            }
            CssParsingError::JustifySelf(e) => CssParsingErrorOwned::JustifySelf(e.to_contained()),
            CssParsingError::JustifyItems(e) => {
                CssParsingErrorOwned::JustifyItems(e.to_contained())
            }
            CssParsingError::AlignSelf(e) => CssParsingErrorOwned::AlignSelf(e.to_contained()),
            CssParsingError::Opacity(e) => CssParsingErrorOwned::Opacity(e.to_contained()),
            CssParsingError::Visibility(e) => CssParsingErrorOwned::Visibility(e.to_contained()),
            CssParsingError::LayoutScrollbarWidth(e) => {
                CssParsingErrorOwned::LayoutScrollbarWidth(e.to_contained())
            }
            CssParsingError::StyleScrollbarColor(e) => {
                CssParsingErrorOwned::StyleScrollbarColor(e.to_contained())
            }
            CssParsingError::ScrollbarVisibilityMode(e) => {
                CssParsingErrorOwned::ScrollbarVisibilityMode(e.to_contained())
            }
            CssParsingError::ScrollbarFadeDelay(e) => {
                CssParsingErrorOwned::ScrollbarFadeDelay(e.to_contained())
            }
            CssParsingError::ScrollbarFadeDuration(e) => {
                CssParsingErrorOwned::ScrollbarFadeDuration(e.to_contained())
            }
            CssParsingError::Transform(e) => CssParsingErrorOwned::Transform(e.to_contained()),
            CssParsingError::TransformOrigin(e) => {
                CssParsingErrorOwned::TransformOrigin(e.to_contained())
            }
            CssParsingError::PerspectiveOrigin(e) => {
                CssParsingErrorOwned::PerspectiveOrigin(e.to_contained())
            }
            CssParsingError::Filter(e) => CssParsingErrorOwned::Filter(e.to_contained()),
            CssParsingError::LayoutWidth(e) => CssParsingErrorOwned::LayoutWidth(e.to_contained()),
            CssParsingError::LayoutHeight(e) => {
                CssParsingErrorOwned::LayoutHeight(e.to_contained())
            }
            CssParsingError::LayoutMinWidth(e) => {
                CssParsingErrorOwned::LayoutMinWidth(e.to_contained())
            }
            CssParsingError::LayoutMinHeight(e) => {
                CssParsingErrorOwned::LayoutMinHeight(e.to_contained())
            }
            CssParsingError::LayoutMaxWidth(e) => {
                CssParsingErrorOwned::LayoutMaxWidth(e.to_contained())
            }
            CssParsingError::LayoutMaxHeight(e) => {
                CssParsingErrorOwned::LayoutMaxHeight(e.to_contained())
            }
            CssParsingError::LayoutPosition(e) => {
                CssParsingErrorOwned::LayoutPosition(e.to_contained())
            }
            CssParsingError::LayoutTop(e) => CssParsingErrorOwned::LayoutTop(e.to_contained()),
            CssParsingError::LayoutRight(e) => CssParsingErrorOwned::LayoutRight(e.to_contained()),
            CssParsingError::LayoutLeft(e) => CssParsingErrorOwned::LayoutLeft(e.to_contained()),
            CssParsingError::LayoutInsetBottom(e) => {
                CssParsingErrorOwned::LayoutInsetBottom(e.to_contained())
            }
            CssParsingError::LayoutZIndex(e) => {
                CssParsingErrorOwned::LayoutZIndex(e.to_contained())
            }
            CssParsingError::FlexWrap(e) => CssParsingErrorOwned::FlexWrap(e.to_contained()),
            CssParsingError::FlexDirection(e) => {
                CssParsingErrorOwned::FlexDirection(e.to_contained())
            }
            CssParsingError::FlexBasis(e) => CssParsingErrorOwned::FlexBasis(e.to_contained()),
            CssParsingError::JustifyContent(e) => {
                CssParsingErrorOwned::JustifyContent(e.to_contained())
            }
            CssParsingError::AlignItems(e) => CssParsingErrorOwned::AlignItems(e.to_contained()),
            CssParsingError::AlignContent(e) => {
                CssParsingErrorOwned::AlignContent(e.to_contained())
            }
            CssParsingError::Grid(e) => CssParsingErrorOwned::Grid(e.to_contained()),
            CssParsingError::LayoutWrap(e) => CssParsingErrorOwned::LayoutWrap(e.to_contained()),
            CssParsingError::LayoutWritingMode(e) => {
                CssParsingErrorOwned::LayoutWritingMode(e.to_contained())
            }
            CssParsingError::LayoutClear(e) => CssParsingErrorOwned::LayoutClear(e.to_contained()),
            CssParsingError::LayoutOverflow(e) => {
                CssParsingErrorOwned::LayoutOverflow(e.to_contained())
            }
            CssParsingError::BorderTopLeftRadius(e) => {
                CssParsingErrorOwned::BorderTopLeftRadius(e.to_contained())
            }
            CssParsingError::BorderTopRightRadius(e) => {
                CssParsingErrorOwned::BorderTopRightRadius(e.to_contained())
            }
            CssParsingError::BorderBottomLeftRadius(e) => {
                CssParsingErrorOwned::BorderBottomLeftRadius(e.to_contained())
            }
            CssParsingError::BorderBottomRightRadius(e) => {
                CssParsingErrorOwned::BorderBottomRightRadius(e.to_contained())
            }
            CssParsingError::BorderStyle(e) => CssParsingErrorOwned::BorderStyle(e.to_contained()),
            CssParsingError::BackfaceVisibility(e) => {
                CssParsingErrorOwned::BackfaceVisibility(e.to_contained())
            }
            CssParsingError::MixBlendMode(e) => {
                CssParsingErrorOwned::MixBlendMode(e.to_contained())
            }
            CssParsingError::TextColor(e) => CssParsingErrorOwned::TextColor(e.to_contained()),
            CssParsingError::FontSize(e) => CssParsingErrorOwned::FontSize(e.to_contained()),
            CssParsingError::TextAlign(e) => CssParsingErrorOwned::TextAlign(e.to_contained()),
            CssParsingError::TextJustify(e) => CssParsingErrorOwned::TextJustify(e.to_owned()),
            CssParsingError::VerticalAlign(e) => {
                CssParsingErrorOwned::VerticalAlign(e.to_contained())
            }
            CssParsingError::LetterSpacing(e) => {
                CssParsingErrorOwned::LetterSpacing(e.to_contained())
            }
            CssParsingError::TextIndent(e) => CssParsingErrorOwned::TextIndent(e.to_contained()),
            CssParsingError::InitialLetter(e) => {
                CssParsingErrorOwned::InitialLetter(e.to_contained())
            }
            CssParsingError::LineClamp(e) => CssParsingErrorOwned::LineClamp(e.to_contained()),
            CssParsingError::HangingPunctuation(e) => {
                CssParsingErrorOwned::HangingPunctuation(e.to_contained())
            }
            CssParsingError::TextCombineUpright(e) => {
                CssParsingErrorOwned::TextCombineUpright(e.to_contained())
            }
            CssParsingError::ExclusionMargin(e) => {
                CssParsingErrorOwned::ExclusionMargin(e.to_contained())
            }
            CssParsingError::HyphenationLanguage(e) => {
                CssParsingErrorOwned::HyphenationLanguage(e.to_contained())
            }
            CssParsingError::LineHeight(e) => CssParsingErrorOwned::LineHeight(e.clone()),
            CssParsingError::WordSpacing(e) => CssParsingErrorOwned::WordSpacing(e.to_contained()),
            CssParsingError::TabSize(e) => CssParsingErrorOwned::TabSize(e.to_contained()),
            CssParsingError::WhiteSpace(e) => CssParsingErrorOwned::WhiteSpace(e.to_contained()),
            CssParsingError::Hyphens(e) => CssParsingErrorOwned::Hyphens(e.to_contained()),
            CssParsingError::Direction(e) => CssParsingErrorOwned::Direction(e.to_contained()),
            CssParsingError::UserSelect(e) => CssParsingErrorOwned::UserSelect(e.to_contained()),
            CssParsingError::TextDecoration(e) => {
                CssParsingErrorOwned::TextDecoration(e.to_contained())
            }
            CssParsingError::Cursor(e) => CssParsingErrorOwned::Cursor(e.to_contained()),
            CssParsingError::LayoutDisplay(e) => {
                CssParsingErrorOwned::LayoutDisplay(e.to_contained())
            }
            CssParsingError::LayoutFloat(e) => CssParsingErrorOwned::LayoutFloat(e.to_contained()),
            CssParsingError::LayoutBoxSizing(e) => {
                CssParsingErrorOwned::LayoutBoxSizing(e.to_contained())
            }
            // DTP properties...
            CssParsingError::PageBreak(e) => CssParsingErrorOwned::PageBreak(e.to_contained()),
            CssParsingError::BreakInside(e) => CssParsingErrorOwned::BreakInside(e.to_contained()),
            CssParsingError::Widows(e) => CssParsingErrorOwned::Widows(e.to_contained()),
            CssParsingError::Orphans(e) => CssParsingErrorOwned::Orphans(e.to_contained()),
            CssParsingError::BoxDecorationBreak(e) => {
                CssParsingErrorOwned::BoxDecorationBreak(e.to_contained())
            }
            CssParsingError::ColumnCount(e) => CssParsingErrorOwned::ColumnCount(e.to_contained()),
            CssParsingError::ColumnWidth(e) => CssParsingErrorOwned::ColumnWidth(e.to_contained()),
            CssParsingError::ColumnSpan(e) => CssParsingErrorOwned::ColumnSpan(e.to_contained()),
            CssParsingError::ColumnFill(e) => CssParsingErrorOwned::ColumnFill(e.to_contained()),
            CssParsingError::ColumnRuleWidth(e) => {
                CssParsingErrorOwned::ColumnRuleWidth(e.to_contained())
            }
            CssParsingError::ColumnRuleStyle(e) => {
                CssParsingErrorOwned::ColumnRuleStyle(e.to_contained())
            }
            CssParsingError::ColumnRuleColor(e) => {
                CssParsingErrorOwned::ColumnRuleColor(e.to_contained())
            }
            CssParsingError::FlowInto(e) => CssParsingErrorOwned::FlowInto(e.to_contained()),
            CssParsingError::FlowFrom(e) => CssParsingErrorOwned::FlowFrom(e.to_contained()),
            CssParsingError::GenericParseError => CssParsingErrorOwned::GenericParseError,
            CssParsingError::Content => CssParsingErrorOwned::Content,
            CssParsingError::Counter => CssParsingErrorOwned::Counter,
            CssParsingError::ListStyleType(e) => {
                CssParsingErrorOwned::ListStyleType(e.to_contained())
            }
            CssParsingError::ListStylePosition(e) => {
                CssParsingErrorOwned::ListStylePosition(e.to_contained())
            }
            CssParsingError::StringSet => CssParsingErrorOwned::StringSet,
            CssParsingError::FontWeight(e) => CssParsingErrorOwned::FontWeight(e.to_contained()),
            CssParsingError::FontStyle(e) => CssParsingErrorOwned::FontStyle(e.to_contained()),
        }
    }
}

impl CssParsingErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssParsingError<'a> {
        match self {
            CssParsingErrorOwned::CaretColor(e) => CssParsingError::CaretColor(e.to_shared()),
            CssParsingErrorOwned::CaretWidth(e) => CssParsingError::CaretWidth(e.to_shared()),
            CssParsingErrorOwned::CaretAnimationDuration(e) => {
                CssParsingError::CaretAnimationDuration(e.to_shared())
            }
            CssParsingErrorOwned::SelectionBackgroundColor(e) => {
                CssParsingError::SelectionBackgroundColor(e.to_shared())
            }
            CssParsingErrorOwned::SelectionColor(e) => {
                CssParsingError::SelectionColor(e.to_shared())
            }
            CssParsingErrorOwned::SelectionRadius(e) => {
                CssParsingError::SelectionRadius(e.to_shared())
            }
            CssParsingErrorOwned::Border(e) => CssParsingError::Border(e.inner.to_shared()),
            CssParsingErrorOwned::BorderRadius(e) => CssParsingError::BorderRadius(e.inner.to_shared()),
            CssParsingErrorOwned::Padding(e) => CssParsingError::Padding(e.to_shared()),
            CssParsingErrorOwned::Margin(e) => CssParsingError::Margin(e.to_shared()),
            CssParsingErrorOwned::Overflow(e) => CssParsingError::Overflow(e.to_shared()),
            CssParsingErrorOwned::BoxShadow(e) => CssParsingError::BoxShadow(e.to_shared()),
            CssParsingErrorOwned::Color(e) => CssParsingError::Color(e.to_shared()),
            CssParsingErrorOwned::PixelValue(e) => CssParsingError::PixelValue(e.to_shared()),
            CssParsingErrorOwned::Percentage(e) => CssParsingError::Percentage(e.clone()),
            CssParsingErrorOwned::FontFamily(e) => CssParsingError::FontFamily(e.to_shared()),
            CssParsingErrorOwned::InvalidValue(e) => CssParsingError::InvalidValue(e.to_shared()),
            CssParsingErrorOwned::FlexGrow(e) => CssParsingError::FlexGrow(e.to_shared()),
            CssParsingErrorOwned::FlexShrink(e) => CssParsingError::FlexShrink(e.to_shared()),
            CssParsingErrorOwned::Background(e) => CssParsingError::Background(e.to_shared()),
            CssParsingErrorOwned::BackgroundPosition(e) => {
                CssParsingError::BackgroundPosition(e.to_shared())
            }
            CssParsingErrorOwned::Opacity(e) => CssParsingError::Opacity(e.to_shared()),
            CssParsingErrorOwned::Visibility(e) => CssParsingError::Visibility(e.to_shared()),
            CssParsingErrorOwned::LayoutScrollbarWidth(e) => {
                CssParsingError::LayoutScrollbarWidth(e.to_shared())
            }
            CssParsingErrorOwned::StyleScrollbarColor(e) => {
                CssParsingError::StyleScrollbarColor(e.to_shared())
            }
            CssParsingErrorOwned::ScrollbarVisibilityMode(e) => {
                CssParsingError::ScrollbarVisibilityMode(e.to_shared())
            }
            CssParsingErrorOwned::ScrollbarFadeDelay(e) => {
                CssParsingError::ScrollbarFadeDelay(e.to_shared())
            }
            CssParsingErrorOwned::ScrollbarFadeDuration(e) => {
                CssParsingError::ScrollbarFadeDuration(e.to_shared())
            }
            CssParsingErrorOwned::Transform(e) => CssParsingError::Transform(e.to_shared()),
            CssParsingErrorOwned::TransformOrigin(e) => {
                CssParsingError::TransformOrigin(e.to_shared())
            }
            CssParsingErrorOwned::PerspectiveOrigin(e) => {
                CssParsingError::PerspectiveOrigin(e.to_shared())
            }
            CssParsingErrorOwned::Filter(e) => CssParsingError::Filter(e.to_shared()),
            CssParsingErrorOwned::LayoutWidth(e) => CssParsingError::LayoutWidth(e.to_shared()),
            CssParsingErrorOwned::LayoutHeight(e) => CssParsingError::LayoutHeight(e.to_shared()),
            CssParsingErrorOwned::LayoutMinWidth(e) => {
                CssParsingError::LayoutMinWidth(e.to_shared())
            }
            CssParsingErrorOwned::LayoutMinHeight(e) => {
                CssParsingError::LayoutMinHeight(e.to_shared())
            }
            CssParsingErrorOwned::LayoutMaxWidth(e) => {
                CssParsingError::LayoutMaxWidth(e.to_shared())
            }
            CssParsingErrorOwned::LayoutMaxHeight(e) => {
                CssParsingError::LayoutMaxHeight(e.to_shared())
            }
            CssParsingErrorOwned::LayoutPosition(e) => {
                CssParsingError::LayoutPosition(e.to_shared())
            }
            CssParsingErrorOwned::LayoutTop(e) => CssParsingError::LayoutTop(e.to_shared()),
            CssParsingErrorOwned::LayoutRight(e) => CssParsingError::LayoutRight(e.to_shared()),
            CssParsingErrorOwned::LayoutLeft(e) => CssParsingError::LayoutLeft(e.to_shared()),
            CssParsingErrorOwned::LayoutInsetBottom(e) => {
                CssParsingError::LayoutInsetBottom(e.to_shared())
            }
            CssParsingErrorOwned::LayoutZIndex(e) => CssParsingError::LayoutZIndex(e.to_shared()),
            CssParsingErrorOwned::FlexWrap(e) => CssParsingError::FlexWrap(e.to_shared()),
            CssParsingErrorOwned::FlexDirection(e) => CssParsingError::FlexDirection(e.to_shared()),
            CssParsingErrorOwned::FlexBasis(e) => CssParsingError::FlexBasis(e.to_shared()),
            CssParsingErrorOwned::JustifyContent(e) => {
                CssParsingError::JustifyContent(e.to_shared())
            }
            CssParsingErrorOwned::AlignItems(e) => CssParsingError::AlignItems(e.to_shared()),
            CssParsingErrorOwned::AlignContent(e) => CssParsingError::AlignContent(e.to_shared()),
            CssParsingErrorOwned::Grid(e) => CssParsingError::Grid(e.to_shared()),
            CssParsingErrorOwned::GridAutoFlow(e) => CssParsingError::GridAutoFlow(e.to_shared()),
            CssParsingErrorOwned::JustifySelf(e) => CssParsingError::JustifySelf(e.to_shared()),
            CssParsingErrorOwned::JustifyItems(e) => CssParsingError::JustifyItems(e.to_shared()),
            CssParsingErrorOwned::AlignSelf(e) => CssParsingError::AlignSelf(e.to_shared()),
            CssParsingErrorOwned::LayoutWrap(e) => CssParsingError::LayoutWrap(e.to_shared()),
            CssParsingErrorOwned::LayoutWritingMode(e) => {
                CssParsingError::LayoutWritingMode(e.to_shared())
            }
            CssParsingErrorOwned::LayoutClear(e) => CssParsingError::LayoutClear(e.to_shared()),
            CssParsingErrorOwned::LayoutOverflow(e) => {
                CssParsingError::LayoutOverflow(e.to_shared())
            }
            CssParsingErrorOwned::BorderTopLeftRadius(e) => {
                CssParsingError::BorderTopLeftRadius(e.to_shared())
            }
            CssParsingErrorOwned::BorderTopRightRadius(e) => {
                CssParsingError::BorderTopRightRadius(e.to_shared())
            }
            CssParsingErrorOwned::BorderBottomLeftRadius(e) => {
                CssParsingError::BorderBottomLeftRadius(e.to_shared())
            }
            CssParsingErrorOwned::BorderBottomRightRadius(e) => {
                CssParsingError::BorderBottomRightRadius(e.to_shared())
            }
            CssParsingErrorOwned::BorderStyle(e) => CssParsingError::BorderStyle(e.to_shared()),
            CssParsingErrorOwned::BackfaceVisibility(e) => {
                CssParsingError::BackfaceVisibility(e.to_shared())
            }
            CssParsingErrorOwned::MixBlendMode(e) => CssParsingError::MixBlendMode(e.to_shared()),
            CssParsingErrorOwned::TextColor(e) => CssParsingError::TextColor(e.to_shared()),
            CssParsingErrorOwned::FontSize(e) => CssParsingError::FontSize(e.to_shared()),
            CssParsingErrorOwned::TextAlign(e) => CssParsingError::TextAlign(e.to_shared()),
            CssParsingErrorOwned::TextJustify(e) => CssParsingError::TextJustify(e.to_borrowed()),
            CssParsingErrorOwned::LetterSpacing(e) => CssParsingError::LetterSpacing(e.to_shared()),
            CssParsingErrorOwned::TextIndent(e) => CssParsingError::TextIndent(e.to_shared()),
            CssParsingErrorOwned::InitialLetter(e) => CssParsingError::InitialLetter(e.to_shared()),
            CssParsingErrorOwned::LineClamp(e) => CssParsingError::LineClamp(e.to_shared()),
            CssParsingErrorOwned::HangingPunctuation(e) => {
                CssParsingError::HangingPunctuation(e.to_shared())
            }
            CssParsingErrorOwned::TextCombineUpright(e) => {
                CssParsingError::TextCombineUpright(e.to_shared())
            }
            CssParsingErrorOwned::ExclusionMargin(e) => {
                CssParsingError::ExclusionMargin(e.to_shared())
            }
            CssParsingErrorOwned::HyphenationLanguage(e) => {
                CssParsingError::HyphenationLanguage(e.to_shared())
            }
            CssParsingErrorOwned::LineHeight(e) => CssParsingError::LineHeight(e.clone()),
            CssParsingErrorOwned::WordSpacing(e) => CssParsingError::WordSpacing(e.to_shared()),
            CssParsingErrorOwned::TabSize(e) => CssParsingError::TabSize(e.to_shared()),
            CssParsingErrorOwned::WhiteSpace(e) => CssParsingError::WhiteSpace(e.to_shared()),
            CssParsingErrorOwned::Hyphens(e) => CssParsingError::Hyphens(e.to_shared()),
            CssParsingErrorOwned::Direction(e) => CssParsingError::Direction(e.to_shared()),
            CssParsingErrorOwned::UserSelect(e) => CssParsingError::UserSelect(e.to_shared()),
            CssParsingErrorOwned::TextDecoration(e) => {
                CssParsingError::TextDecoration(e.to_shared())
            }
            CssParsingErrorOwned::Cursor(e) => CssParsingError::Cursor(e.to_shared()),
            CssParsingErrorOwned::LayoutDisplay(e) => CssParsingError::LayoutDisplay(e.to_shared()),
            CssParsingErrorOwned::LayoutFloat(e) => CssParsingError::LayoutFloat(e.to_shared()),
            CssParsingErrorOwned::LayoutBoxSizing(e) => {
                CssParsingError::LayoutBoxSizing(e.to_shared())
            }
            // DTP properties...
            CssParsingErrorOwned::PageBreak(e) => CssParsingError::PageBreak(e.to_shared()),
            CssParsingErrorOwned::BreakInside(e) => CssParsingError::BreakInside(e.to_shared()),
            CssParsingErrorOwned::Widows(e) => CssParsingError::Widows(e.to_shared()),
            CssParsingErrorOwned::Orphans(e) => CssParsingError::Orphans(e.to_shared()),
            CssParsingErrorOwned::BoxDecorationBreak(e) => {
                CssParsingError::BoxDecorationBreak(e.to_shared())
            }
            CssParsingErrorOwned::ColumnCount(e) => CssParsingError::ColumnCount(e.to_shared()),
            CssParsingErrorOwned::ColumnWidth(e) => CssParsingError::ColumnWidth(e.to_shared()),
            CssParsingErrorOwned::ColumnSpan(e) => CssParsingError::ColumnSpan(e.to_shared()),
            CssParsingErrorOwned::ColumnFill(e) => CssParsingError::ColumnFill(e.to_shared()),
            CssParsingErrorOwned::ColumnRuleWidth(e) => {
                CssParsingError::ColumnRuleWidth(e.to_shared())
            }
            CssParsingErrorOwned::ColumnRuleStyle(e) => {
                CssParsingError::ColumnRuleStyle(e.to_shared())
            }
            CssParsingErrorOwned::ColumnRuleColor(e) => {
                CssParsingError::ColumnRuleColor(e.to_shared())
            }
            CssParsingErrorOwned::FlowInto(e) => CssParsingError::FlowInto(e.to_shared()),
            CssParsingErrorOwned::FlowFrom(e) => CssParsingError::FlowFrom(e.to_shared()),
            CssParsingErrorOwned::GenericParseError => CssParsingError::GenericParseError,
            CssParsingErrorOwned::Content => CssParsingError::Content,
            CssParsingErrorOwned::Counter => CssParsingError::Counter,
            CssParsingErrorOwned::ListStyleType(e) => CssParsingError::ListStyleType(e.to_shared()),
            CssParsingErrorOwned::ListStylePosition(e) => {
                CssParsingError::ListStylePosition(e.to_shared())
            }
            CssParsingErrorOwned::StringSet => CssParsingError::StringSet,
            CssParsingErrorOwned::FontWeight(e) => CssParsingError::FontWeight(e.to_shared()),
            CssParsingErrorOwned::FontStyle(e) => CssParsingError::FontStyle(e.to_shared()),
            CssParsingErrorOwned::VerticalAlign(e) => CssParsingError::VerticalAlign(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_css_property<'a>(
    key: CssPropertyType,
    value: &'a str,
) -> Result<CssProperty, CssParsingError<'a>> {
    use crate::props::style::{
        parse_selection_background_color, parse_selection_color, parse_selection_radius,
    };

    let value = value.trim();

    // For properties where "auto" or "none" is a valid typed value (not just the generic CSS
    // keyword), we must NOT intercept them here. Let the specific parser handle them.
    let has_typed_auto = matches!(
        key,
        CssPropertyType::Hyphens |      // hyphens: auto means StyleHyphens::Auto
        CssPropertyType::OverflowX |
        CssPropertyType::OverflowY |
        CssPropertyType::UserSelect // user-select: auto is a typed value
    );

    let has_typed_none = matches!(
        key,
        CssPropertyType::Hyphens |      // hyphens: none means StyleHyphens::None
        CssPropertyType::Display |      // display: none means LayoutDisplay::None
        CssPropertyType::UserSelect |
        CssPropertyType::Float |        // float: none means LayoutFloat::None
        CssPropertyType::TextDecoration // text-decoration: none is a typed value
    );

    Ok(match value {
        "auto" if !has_typed_auto => CssProperty::auto(key),
        "none" if !has_typed_none => CssProperty::none(key),
        "initial" => CssProperty::initial(key),
        "inherit" => CssProperty::inherit(key),
        value => match key {
            CssPropertyType::CaretColor => parse_caret_color(value)?.into(),
            CssPropertyType::CaretWidth => parse_caret_width(value)?.into(),
            CssPropertyType::CaretAnimationDuration => {
                parse_caret_animation_duration(value)?.into()
            }
            CssPropertyType::SelectionBackgroundColor => {
                parse_selection_background_color(value)?.into()
            }
            CssPropertyType::SelectionColor => parse_selection_color(value)?.into(),
            CssPropertyType::SelectionRadius => parse_selection_radius(value)?.into(),

            CssPropertyType::TextColor => parse_style_text_color(value)?.into(),
            CssPropertyType::FontSize => {
                CssProperty::FontSize(parse_style_font_size(value)?.into())
            }
            CssPropertyType::FontFamily => parse_style_font_family(value)?.into(),
            CssPropertyType::FontWeight => {
                CssProperty::FontWeight(parse_font_weight(value)?.into())
            }
            CssPropertyType::FontStyle => CssProperty::FontStyle(parse_font_style(value)?.into()),
            CssPropertyType::TextAlign => parse_style_text_align(value)?.into(),
            CssPropertyType::TextJustify => parse_layout_text_justify(value)?.into(),
            CssPropertyType::VerticalAlign => parse_style_vertical_align(value)?.into(),
            CssPropertyType::LetterSpacing => parse_style_letter_spacing(value)?.into(),
            CssPropertyType::TextIndent => parse_style_text_indent(value)?.into(),
            CssPropertyType::InitialLetter => parse_style_initial_letter(value)?.into(),
            CssPropertyType::LineClamp => parse_style_line_clamp(value)?.into(),
            CssPropertyType::HangingPunctuation => parse_style_hanging_punctuation(value)?.into(),
            CssPropertyType::TextCombineUpright => parse_style_text_combine_upright(value)?.into(),
            CssPropertyType::ExclusionMargin => parse_style_exclusion_margin(value)?.into(),
            CssPropertyType::HyphenationLanguage => parse_style_hyphenation_language(value)?.into(),
            CssPropertyType::LineHeight => parse_style_line_height(value)?.into(),
            CssPropertyType::WordSpacing => parse_style_word_spacing(value)?.into(),
            CssPropertyType::TabSize => parse_style_tab_size(value)?.into(),
            CssPropertyType::WhiteSpace => parse_style_white_space(value)?.into(),
            CssPropertyType::Hyphens => parse_style_hyphens(value)?.into(),
            CssPropertyType::Direction => parse_style_direction(value)?.into(),
            CssPropertyType::UserSelect => parse_style_user_select(value)?.into(),
            CssPropertyType::TextDecoration => parse_style_text_decoration(value)?.into(),
            CssPropertyType::Cursor => parse_style_cursor(value)?.into(),

            CssPropertyType::Display => parse_layout_display(value)?.into(),
            CssPropertyType::Float => parse_layout_float(value)?.into(),
            CssPropertyType::BoxSizing => parse_layout_box_sizing(value)?.into(),
            CssPropertyType::Width => parse_layout_width(value)?.into(),
            CssPropertyType::Height => parse_layout_height(value)?.into(),
            CssPropertyType::MinWidth => parse_layout_min_width(value)?.into(),
            CssPropertyType::MinHeight => parse_layout_min_height(value)?.into(),
            CssPropertyType::MaxWidth => parse_layout_max_width(value)?.into(),
            CssPropertyType::MaxHeight => parse_layout_max_height(value)?.into(),
            CssPropertyType::Position => parse_layout_position(value)?.into(),
            CssPropertyType::Top => parse_layout_top(value)?.into(),
            CssPropertyType::Right => parse_layout_right(value)?.into(),
            CssPropertyType::Left => parse_layout_left(value)?.into(),
            CssPropertyType::Bottom => parse_layout_bottom(value)?.into(),
            CssPropertyType::ZIndex => CssProperty::ZIndex(parse_layout_z_index(value)?.into()),

            CssPropertyType::FlexWrap => parse_layout_flex_wrap(value)?.into(),
            CssPropertyType::FlexDirection => parse_layout_flex_direction(value)?.into(),
            CssPropertyType::FlexGrow => parse_layout_flex_grow(value)?.into(),
            CssPropertyType::FlexShrink => parse_layout_flex_shrink(value)?.into(),
            CssPropertyType::FlexBasis => parse_layout_flex_basis(value)?.into(),
            CssPropertyType::JustifyContent => parse_layout_justify_content(value)?.into(),
            CssPropertyType::AlignItems => parse_layout_align_items(value)?.into(),
            CssPropertyType::AlignContent => parse_layout_align_content(value)?.into(),
            CssPropertyType::ColumnGap => parse_layout_column_gap(value)?.into(),
            CssPropertyType::RowGap => parse_layout_row_gap(value)?.into(),
            CssPropertyType::GridTemplateColumns => {
                CssProperty::GridTemplateColumns(parse_grid_template(value)?.into())
            }
            CssPropertyType::GridTemplateRows => {
                CssProperty::GridTemplateRows(parse_grid_template(value)?.into())
            }
            CssPropertyType::GridAutoColumns => {
                let template = parse_grid_template(value)?;
                CssProperty::GridAutoColumns(CssPropertyValue::Exact(GridAutoTracks::from(
                    template,
                )))
            }
            CssPropertyType::GridAutoFlow => {
                CssProperty::GridAutoFlow(parse_layout_grid_auto_flow(value)?.into())
            }
            CssPropertyType::JustifySelf => {
                CssProperty::JustifySelf(parse_layout_justify_self(value)?.into())
            }
            CssPropertyType::JustifyItems => {
                CssProperty::JustifyItems(parse_layout_justify_items(value)?.into())
            }
            CssPropertyType::Gap => {
                // gap shorthand: single value -> both row & column
                CssProperty::Gap(parse_layout_gap(value)?.into())
            }
            CssPropertyType::GridGap => CssProperty::GridGap(parse_layout_gap(value)?.into()),
            CssPropertyType::AlignSelf => {
                CssProperty::AlignSelf(parse_layout_align_self(value)?.into())
            }
            CssPropertyType::Font => {
                // minimal font parser: map to font-family for now
                let fam = parse_style_font_family(value)?;
                CssProperty::Font(fam.into())
            }
            CssPropertyType::GridAutoRows => {
                let template = parse_grid_template(value)?;
                CssProperty::GridAutoRows(CssPropertyValue::Exact(GridAutoTracks::from(template)))
            }
            CssPropertyType::GridColumn => {
                CssProperty::GridColumn(parse_grid_placement(value)?.into())
            }
            CssPropertyType::GridRow => CssProperty::GridRow(parse_grid_placement(value)?.into()),
            CssPropertyType::GridTemplateAreas => {
                use crate::props::layout::grid::parse_grid_template_areas;
                let areas = parse_grid_template_areas(value)
                    .map_err(|_| CssParsingError::InvalidValue(InvalidValueErr(value)))?;
                CssProperty::GridTemplateAreas(CssPropertyValue::Exact(areas))
            }
            CssPropertyType::WritingMode => parse_layout_writing_mode(value)?.into(),
            CssPropertyType::Clear => parse_layout_clear(value)?.into(),

            CssPropertyType::BackgroundContent => {
                parse_style_background_content_multiple(value)?.into()
            }
            CssPropertyType::BackgroundPosition => {
                parse_style_background_position_multiple(value)?.into()
            }
            CssPropertyType::BackgroundSize => parse_style_background_size_multiple(value)?.into(),
            CssPropertyType::BackgroundRepeat => {
                parse_style_background_repeat_multiple(value)?.into()
            }

            CssPropertyType::OverflowX => {
                CssProperty::OverflowX(parse_layout_overflow(value)?.into())
            }
            CssPropertyType::OverflowY => {
                CssProperty::OverflowY(parse_layout_overflow(value)?.into())
            }

            CssPropertyType::PaddingTop => parse_layout_padding_top(value)?.into(),
            CssPropertyType::PaddingLeft => parse_layout_padding_left(value)?.into(),
            CssPropertyType::PaddingRight => parse_layout_padding_right(value)?.into(),
            CssPropertyType::PaddingBottom => parse_layout_padding_bottom(value)?.into(),
            CssPropertyType::PaddingInlineStart => parse_layout_padding_inline_start(value)?.into(),
            CssPropertyType::PaddingInlineEnd => parse_layout_padding_inline_end(value)?.into(),

            CssPropertyType::MarginTop => parse_layout_margin_top(value)?.into(),
            CssPropertyType::MarginLeft => parse_layout_margin_left(value)?.into(),
            CssPropertyType::MarginRight => parse_layout_margin_right(value)?.into(),
            CssPropertyType::MarginBottom => parse_layout_margin_bottom(value)?.into(),

            CssPropertyType::BorderTopLeftRadius => {
                parse_style_border_top_left_radius(value)?.into()
            }
            CssPropertyType::BorderTopRightRadius => {
                parse_style_border_top_right_radius(value)?.into()
            }
            CssPropertyType::BorderBottomLeftRadius => {
                parse_style_border_bottom_left_radius(value)?.into()
            }
            CssPropertyType::BorderBottomRightRadius => {
                parse_style_border_bottom_right_radius(value)?.into()
            }

            CssPropertyType::BorderTopColor => parse_border_top_color(value)?.into(),
            CssPropertyType::BorderRightColor => parse_border_right_color(value)?.into(),
            CssPropertyType::BorderLeftColor => parse_border_left_color(value)?.into(),
            CssPropertyType::BorderBottomColor => parse_border_bottom_color(value)?.into(),

            CssPropertyType::BorderTopStyle => parse_border_top_style(value)?.into(),
            CssPropertyType::BorderRightStyle => parse_border_right_style(value)?.into(),
            CssPropertyType::BorderLeftStyle => parse_border_left_style(value)?.into(),
            CssPropertyType::BorderBottomStyle => parse_border_bottom_style(value)?.into(),

            CssPropertyType::BorderTopWidth => parse_border_top_width(value)?.into(),
            CssPropertyType::BorderRightWidth => parse_border_right_width(value)?.into(),
            CssPropertyType::BorderLeftWidth => parse_border_left_width(value)?.into(),
            CssPropertyType::BorderBottomWidth => parse_border_bottom_width(value)?.into(),

            CssPropertyType::BoxShadowLeft => {
                CssProperty::BoxShadowLeft(parse_style_box_shadow(value)?.into())
            }
            CssPropertyType::BoxShadowRight => {
                CssProperty::BoxShadowRight(parse_style_box_shadow(value)?.into())
            }
            CssPropertyType::BoxShadowTop => {
                CssProperty::BoxShadowTop(parse_style_box_shadow(value)?.into())
            }
            CssPropertyType::BoxShadowBottom => {
                CssProperty::BoxShadowBottom(parse_style_box_shadow(value)?.into())
            }

            CssPropertyType::ScrollbarTrack => CssProperty::ScrollbarTrack(parse_style_background_content(value)?.into()),
            CssPropertyType::ScrollbarThumb => CssProperty::ScrollbarThumb(parse_style_background_content(value)?.into()),
            CssPropertyType::ScrollbarButton => CssProperty::ScrollbarButton(parse_style_background_content(value)?.into()),
            CssPropertyType::ScrollbarCorner => CssProperty::ScrollbarCorner(parse_style_background_content(value)?.into()),
            CssPropertyType::ScrollbarResizer => CssProperty::ScrollbarResizer(parse_style_background_content(value)?.into()),
            CssPropertyType::ScrollbarWidth => parse_layout_scrollbar_width(value)?.into(),
            CssPropertyType::ScrollbarColor => parse_style_scrollbar_color(value)?.into(),
            CssPropertyType::ScrollbarVisibility => parse_scrollbar_visibility_mode(value)?.into(),
            CssPropertyType::ScrollbarFadeDelay => parse_scrollbar_fade_delay(value)?.into(),
            CssPropertyType::ScrollbarFadeDuration => parse_scrollbar_fade_duration(value)?.into(),
            CssPropertyType::Opacity => parse_style_opacity(value)?.into(),
            CssPropertyType::Visibility => parse_style_visibility(value)?.into(),
            CssPropertyType::Transform => parse_style_transform_vec(value)?.into(),
            CssPropertyType::TransformOrigin => parse_style_transform_origin(value)?.into(),
            CssPropertyType::PerspectiveOrigin => parse_style_perspective_origin(value)?.into(),
            CssPropertyType::BackfaceVisibility => parse_style_backface_visibility(value)?.into(),

            CssPropertyType::MixBlendMode => parse_style_mix_blend_mode(value)?.into(),
            CssPropertyType::Filter => CssProperty::Filter(parse_style_filter_vec(value)?.into()),
            CssPropertyType::BackdropFilter => {
                CssProperty::BackdropFilter(parse_style_filter_vec(value)?.into())
            }
            CssPropertyType::TextShadow => {
                CssProperty::TextShadow(parse_style_box_shadow(value)?.into())
            }

            // DTP properties
            CssPropertyType::BreakBefore => {
                CssProperty::BreakBefore(parse_page_break(value)?.into())
            }
            CssPropertyType::BreakAfter => CssProperty::BreakAfter(parse_page_break(value)?.into()),
            CssPropertyType::BreakInside => {
                CssProperty::BreakInside(parse_break_inside(value)?.into())
            }
            CssPropertyType::Orphans => CssProperty::Orphans(parse_orphans(value)?.into()),
            CssPropertyType::Widows => CssProperty::Widows(parse_widows(value)?.into()),
            CssPropertyType::BoxDecorationBreak => {
                CssProperty::BoxDecorationBreak(parse_box_decoration_break(value)?.into())
            }
            CssPropertyType::ColumnCount => {
                CssProperty::ColumnCount(parse_column_count(value)?.into())
            }
            CssPropertyType::ColumnWidth => {
                CssProperty::ColumnWidth(parse_column_width(value)?.into())
            }
            CssPropertyType::ColumnSpan => {
                CssProperty::ColumnSpan(parse_column_span(value)?.into())
            }
            CssPropertyType::ColumnFill => {
                CssProperty::ColumnFill(parse_column_fill(value)?.into())
            }
            CssPropertyType::ColumnRuleWidth => {
                CssProperty::ColumnRuleWidth(parse_column_rule_width(value)?.into())
            }
            CssPropertyType::ColumnRuleStyle => {
                CssProperty::ColumnRuleStyle(parse_column_rule_style(value)?.into())
            }
            CssPropertyType::ColumnRuleColor => {
                CssProperty::ColumnRuleColor(parse_column_rule_color(value)?.into())
            }
            CssPropertyType::FlowInto => CssProperty::FlowInto(parse_flow_into(value)?.into()),
            CssPropertyType::FlowFrom => CssProperty::FlowFrom(parse_flow_from(value)?.into()),
            CssPropertyType::ShapeOutside => CssProperty::ShapeOutside(
                parse_shape_outside(value)
                    .map_err(|_| CssParsingError::GenericParseError)?
                    .into(),
            ),
            CssPropertyType::ShapeInside => CssProperty::ShapeInside(
                parse_shape_inside(value)
                    .map_err(|_| CssParsingError::GenericParseError)?
                    .into(),
            ),
            CssPropertyType::ClipPath => CssProperty::ClipPath(
                parse_clip_path(value)
                    .map_err(|_| CssParsingError::GenericParseError)?
                    .into(),
            ),
            CssPropertyType::ShapeMargin => {
                CssProperty::ShapeMargin(parse_shape_margin(value)?.into())
            }
            CssPropertyType::ShapeImageThreshold => CssProperty::ShapeImageThreshold(
                parse_shape_image_threshold(value)
                    .map_err(|_| CssParsingError::GenericParseError)?
                    .into(),
            ),
            CssPropertyType::Content => CssProperty::Content(
                parse_content(value)
                    .map_err(|_| CssParsingError::Content)?
                    .into(),
            ),
            CssPropertyType::CounterReset => CssProperty::CounterReset(
                parse_counter_reset(value)
                    .map_err(|_| CssParsingError::Counter)?
                    .into(),
            ),
            CssPropertyType::CounterIncrement => CssProperty::CounterIncrement(
                parse_counter_increment(value)
                    .map_err(|_| CssParsingError::Counter)?
                    .into(),
            ),
            CssPropertyType::ListStyleType => CssProperty::ListStyleType(
                parse_style_list_style_type(value)
                    .map_err(|e| CssParsingError::ListStyleType(e))?
                    .into(),
            ),
            CssPropertyType::ListStylePosition => CssProperty::ListStylePosition(
                parse_style_list_style_position(value)
                    .map_err(|e| CssParsingError::ListStylePosition(e))?
                    .into(),
            ),
            CssPropertyType::StringSet => CssProperty::StringSet(
                parse_string_set(value)
                    .map_err(|_| CssParsingError::StringSet)?
                    .into(),
            ),
            CssPropertyType::TableLayout => CssProperty::TableLayout(
                parse_table_layout(value)
                    .map_err(|_| CssParsingError::GenericParseError)?
                    .into(),
            ),
            CssPropertyType::BorderCollapse => CssProperty::BorderCollapse(
                parse_border_collapse(value)
                    .map_err(|_| CssParsingError::GenericParseError)?
                    .into(),
            ),
            CssPropertyType::BorderSpacing => CssProperty::BorderSpacing(
                parse_border_spacing(value)
                    .map_err(|_| CssParsingError::GenericParseError)?
                    .into(),
            ),
            CssPropertyType::CaptionSide => CssProperty::CaptionSide(
                parse_caption_side(value)
                    .map_err(|_| CssParsingError::GenericParseError)?
                    .into(),
            ),
            CssPropertyType::EmptyCells => CssProperty::EmptyCells(
                parse_empty_cells(value)
                    .map_err(|_| CssParsingError::GenericParseError)?
                    .into(),
            ),
        },
    })
}

#[cfg(feature = "parser")]

/// Parses a combined CSS property or a CSS property shorthand, for example "margin"
/// (as a shorthand for setting all four properties of "margin-top", "margin-bottom",
/// "margin-left" and "margin-right")
///
/// ```rust
/// # extern crate azul_css;
/// # use azul_css::*;
/// # use azul_css::props::style::*;
/// # use azul_css::css::CssPropertyValue;
/// # use azul_css::props::property::*;
/// assert_eq!(
///     parse_combined_css_property(CombinedCssPropertyType::BorderRadius, "10px"),
///     Ok(vec![
///         CssProperty::BorderTopLeftRadius(CssPropertyValue::Exact(
///             StyleBorderTopLeftRadius::px(10.0)
///         )),
///         CssProperty::BorderTopRightRadius(CssPropertyValue::Exact(
///             StyleBorderTopRightRadius::px(10.0)
///         )),
///         CssProperty::BorderBottomLeftRadius(CssPropertyValue::Exact(
///             StyleBorderBottomLeftRadius::px(10.0)
///         )),
///         CssProperty::BorderBottomRightRadius(CssPropertyValue::Exact(
///             StyleBorderBottomRightRadius::px(10.0)
///         )),
///     ])
/// )
/// ```
#[cfg(feature = "parser")]
pub fn parse_combined_css_property<'a>(
    key: CombinedCssPropertyType,
    value: &'a str,
) -> Result<Vec<CssProperty>, CssParsingError<'a>> {
    use self::CombinedCssPropertyType::*;

    macro_rules! convert_value {
        ($thing:expr, $prop_type:ident, $wrapper:ident) => {
            match $thing {
                PixelValueWithAuto::None => CssProperty::none(CssPropertyType::$prop_type),
                PixelValueWithAuto::Initial => CssProperty::initial(CssPropertyType::$prop_type),
                PixelValueWithAuto::Inherit => CssProperty::inherit(CssPropertyType::$prop_type),
                PixelValueWithAuto::Auto => CssProperty::auto(CssPropertyType::$prop_type),
                PixelValueWithAuto::Exact(x) => {
                    CssProperty::$prop_type($wrapper { inner: x }.into())
                }
            }
        };
    }

    let keys = match key {
        BorderRadius => {
            vec![
                CssPropertyType::BorderTopLeftRadius,
                CssPropertyType::BorderTopRightRadius,
                CssPropertyType::BorderBottomLeftRadius,
                CssPropertyType::BorderBottomRightRadius,
            ]
        }
        Overflow => {
            vec![CssPropertyType::OverflowX, CssPropertyType::OverflowY]
        }
        Padding => {
            vec![
                CssPropertyType::PaddingTop,
                CssPropertyType::PaddingBottom,
                CssPropertyType::PaddingLeft,
                CssPropertyType::PaddingRight,
            ]
        }
        Margin => {
            vec![
                CssPropertyType::MarginTop,
                CssPropertyType::MarginBottom,
                CssPropertyType::MarginLeft,
                CssPropertyType::MarginRight,
            ]
        }
        Border => {
            vec![
                CssPropertyType::BorderTopColor,
                CssPropertyType::BorderRightColor,
                CssPropertyType::BorderLeftColor,
                CssPropertyType::BorderBottomColor,
                CssPropertyType::BorderTopStyle,
                CssPropertyType::BorderRightStyle,
                CssPropertyType::BorderLeftStyle,
                CssPropertyType::BorderBottomStyle,
                CssPropertyType::BorderTopWidth,
                CssPropertyType::BorderRightWidth,
                CssPropertyType::BorderLeftWidth,
                CssPropertyType::BorderBottomWidth,
            ]
        }
        BorderLeft => {
            vec![
                CssPropertyType::BorderLeftColor,
                CssPropertyType::BorderLeftStyle,
                CssPropertyType::BorderLeftWidth,
            ]
        }
        BorderRight => {
            vec![
                CssPropertyType::BorderRightColor,
                CssPropertyType::BorderRightStyle,
                CssPropertyType::BorderRightWidth,
            ]
        }
        BorderTop => {
            vec![
                CssPropertyType::BorderTopColor,
                CssPropertyType::BorderTopStyle,
                CssPropertyType::BorderTopWidth,
            ]
        }
        BorderBottom => {
            vec![
                CssPropertyType::BorderBottomColor,
                CssPropertyType::BorderBottomStyle,
                CssPropertyType::BorderBottomWidth,
            ]
        }
        BorderColor => {
            vec![
                CssPropertyType::BorderTopColor,
                CssPropertyType::BorderRightColor,
                CssPropertyType::BorderBottomColor,
                CssPropertyType::BorderLeftColor,
            ]
        }
        BorderStyle => {
            vec![
                CssPropertyType::BorderTopStyle,
                CssPropertyType::BorderRightStyle,
                CssPropertyType::BorderBottomStyle,
                CssPropertyType::BorderLeftStyle,
            ]
        }
        BorderWidth => {
            vec![
                CssPropertyType::BorderTopWidth,
                CssPropertyType::BorderRightWidth,
                CssPropertyType::BorderBottomWidth,
                CssPropertyType::BorderLeftWidth,
            ]
        }
        BoxShadow => {
            vec![
                CssPropertyType::BoxShadowLeft,
                CssPropertyType::BoxShadowRight,
                CssPropertyType::BoxShadowTop,
                CssPropertyType::BoxShadowBottom,
            ]
        }
        BackgroundColor => {
            vec![CssPropertyType::BackgroundContent]
        }
        BackgroundImage => {
            vec![CssPropertyType::BackgroundContent]
        }
        Background => {
            vec![CssPropertyType::BackgroundContent]
        }
        Flex => {
            vec![
                CssPropertyType::FlexGrow,
                CssPropertyType::FlexShrink,
                CssPropertyType::FlexBasis,
            ]
        }
        Grid => {
            vec![
                CssPropertyType::GridTemplateColumns,
                CssPropertyType::GridTemplateRows,
            ]
        }
        Gap => {
            vec![CssPropertyType::RowGap, CssPropertyType::ColumnGap]
        }
        GridGap => {
            vec![CssPropertyType::RowGap, CssPropertyType::ColumnGap]
        }
        Font => {
            vec![CssPropertyType::Font]
        }
        Columns => {
            vec![CssPropertyType::ColumnWidth, CssPropertyType::ColumnCount]
        }
        GridArea => {
            vec![CssPropertyType::GridRow, CssPropertyType::GridColumn]
        }
        ColumnRule => {
            vec![
                CssPropertyType::ColumnRuleWidth,
                CssPropertyType::ColumnRuleStyle,
                CssPropertyType::ColumnRuleColor,
            ]
        }
    };

    // For Overflow, "auto" is a typed value (LayoutOverflow::Auto), not the generic CSS keyword,
    // so we must not intercept it here and let the specific parser handle it below.
    let has_typed_auto = matches!(key, Overflow);
    let has_typed_none = false; // Currently no combined properties have typed "none"

    match value {
        "auto" if !has_typed_auto => {
            return Ok(keys.into_iter().map(|ty| CssProperty::auto(ty)).collect())
        }
        "none" if !has_typed_none => {
            return Ok(keys.into_iter().map(|ty| CssProperty::none(ty)).collect())
        }
        "initial" => {
            return Ok(keys
                .into_iter()
                .map(|ty| CssProperty::initial(ty))
                .collect());
        }
        "inherit" => {
            return Ok(keys
                .into_iter()
                .map(|ty| CssProperty::inherit(ty))
                .collect());
        }
        _ => {}
    };

    match key {
        BorderRadius => {
            let border_radius = parse_style_border_radius(value)?;
            Ok(vec![
                CssProperty::BorderTopLeftRadius(
                    StyleBorderTopLeftRadius {
                        inner: border_radius.top_left,
                    }
                    .into(),
                ),
                CssProperty::BorderTopRightRadius(
                    StyleBorderTopRightRadius {
                        inner: border_radius.top_right,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomLeftRadius(
                    StyleBorderBottomLeftRadius {
                        inner: border_radius.bottom_left,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomRightRadius(
                    StyleBorderBottomRightRadius {
                        inner: border_radius.bottom_right,
                    }
                    .into(),
                ),
            ])
        }
        Overflow => {
            let overflow = parse_layout_overflow(value)?;
            Ok(vec![
                CssProperty::OverflowX(overflow.into()),
                CssProperty::OverflowY(overflow.into()),
            ])
        }
        Padding => {
            let padding = parse_layout_padding(value)?;
            Ok(vec![
                convert_value!(padding.top, PaddingTop, LayoutPaddingTop),
                convert_value!(padding.bottom, PaddingBottom, LayoutPaddingBottom),
                convert_value!(padding.left, PaddingLeft, LayoutPaddingLeft),
                convert_value!(padding.right, PaddingRight, LayoutPaddingRight),
            ])
        }
        Margin => {
            let margin = parse_layout_margin(value)?;
            Ok(vec![
                convert_value!(margin.top, MarginTop, LayoutMarginTop),
                convert_value!(margin.bottom, MarginBottom, LayoutMarginBottom),
                convert_value!(margin.left, MarginLeft, LayoutMarginLeft),
                convert_value!(margin.right, MarginRight, LayoutMarginRight),
            ])
        }
        Border => {
            let border = parse_style_border(value)?;
            Ok(vec![
                CssProperty::BorderTopColor(
                    StyleBorderTopColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderRightColor(
                    StyleBorderRightColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderLeftColor(
                    StyleBorderLeftColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomColor(
                    StyleBorderBottomColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderTopStyle(
                    StyleBorderTopStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderRightStyle(
                    StyleBorderRightStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderLeftStyle(
                    StyleBorderLeftStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomStyle(
                    StyleBorderBottomStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderTopWidth(
                    LayoutBorderTopWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
                CssProperty::BorderRightWidth(
                    LayoutBorderRightWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
                CssProperty::BorderLeftWidth(
                    LayoutBorderLeftWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomWidth(
                    LayoutBorderBottomWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
            ])
        }
        BorderLeft => {
            let border = parse_style_border(value)?;
            Ok(vec![
                CssProperty::BorderLeftColor(
                    StyleBorderLeftColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderLeftStyle(
                    StyleBorderLeftStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderLeftWidth(
                    LayoutBorderLeftWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
            ])
        }
        BorderRight => {
            let border = parse_style_border(value)?;
            Ok(vec![
                CssProperty::BorderRightColor(
                    StyleBorderRightColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderRightStyle(
                    StyleBorderRightStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderRightWidth(
                    LayoutBorderRightWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
            ])
        }
        BorderTop => {
            let border = parse_style_border(value)?;
            Ok(vec![
                CssProperty::BorderTopColor(
                    StyleBorderTopColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderTopStyle(
                    StyleBorderTopStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderTopWidth(
                    LayoutBorderTopWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
            ])
        }
        BorderBottom => {
            let border = parse_style_border(value)?;
            Ok(vec![
                CssProperty::BorderBottomColor(
                    StyleBorderBottomColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomStyle(
                    StyleBorderBottomStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomWidth(
                    LayoutBorderBottomWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
            ])
        }
        BorderColor => {
            let colors = parse_style_border_color(value)?;
            Ok(vec![
                CssProperty::BorderTopColor(
                    StyleBorderTopColor { inner: colors.top }.into(),
                ),
                CssProperty::BorderRightColor(
                    StyleBorderRightColor { inner: colors.right }.into(),
                ),
                CssProperty::BorderBottomColor(
                    StyleBorderBottomColor { inner: colors.bottom }.into(),
                ),
                CssProperty::BorderLeftColor(
                    StyleBorderLeftColor { inner: colors.left }.into(),
                ),
            ])
        }
        BorderStyle => {
            let styles = parse_style_border_style(value)?;
            Ok(vec![
                CssProperty::BorderTopStyle(
                    StyleBorderTopStyle { inner: styles.top }.into(),
                ),
                CssProperty::BorderRightStyle(
                    StyleBorderRightStyle { inner: styles.right }.into(),
                ),
                CssProperty::BorderBottomStyle(
                    StyleBorderBottomStyle { inner: styles.bottom }.into(),
                ),
                CssProperty::BorderLeftStyle(
                    StyleBorderLeftStyle { inner: styles.left }.into(),
                ),
            ])
        }
        BorderWidth => {
            let widths = parse_style_border_width(value)?;
            Ok(vec![
                CssProperty::BorderTopWidth(
                    LayoutBorderTopWidth { inner: widths.top }.into(),
                ),
                CssProperty::BorderRightWidth(
                    LayoutBorderRightWidth { inner: widths.right }.into(),
                ),
                CssProperty::BorderBottomWidth(
                    LayoutBorderBottomWidth { inner: widths.bottom }.into(),
                ),
                CssProperty::BorderLeftWidth(
                    LayoutBorderLeftWidth { inner: widths.left }.into(),
                ),
            ])
        }
        BoxShadow => {
            let box_shadow = parse_style_box_shadow(value)?;
            Ok(vec![
                CssProperty::BoxShadowLeft(CssPropertyValue::Exact(box_shadow)),
                CssProperty::BoxShadowRight(CssPropertyValue::Exact(box_shadow)),
                CssProperty::BoxShadowTop(CssPropertyValue::Exact(box_shadow)),
                CssProperty::BoxShadowBottom(CssPropertyValue::Exact(box_shadow)),
            ])
        }
        BackgroundColor => {
            let color = parse_css_color(value)?;
            let vec: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            Ok(vec![CssProperty::BackgroundContent(vec.into())])
        }
        BackgroundImage => {
            let background_content = parse_style_background_content(value)?;
            let vec: StyleBackgroundContentVec = vec![background_content].into();
            Ok(vec![CssProperty::BackgroundContent(vec.into())])
        }
        Background => {
            let background_content = parse_style_background_content_multiple(value)?;
            Ok(vec![CssProperty::BackgroundContent(
                background_content.into(),
            )])
        }
        Flex => {
            // parse shorthand into grow/shrink/basis
            let parts: Vec<&str> = value.split_whitespace().collect();
            if parts.len() == 1 && parts[0] == "none" {
                return Ok(vec![
                    CssProperty::FlexGrow(
                        LayoutFlexGrow {
                            inner: crate::props::basic::length::FloatValue::const_new(0),
                        }
                        .into(),
                    ),
                    CssProperty::FlexShrink(
                        LayoutFlexShrink {
                            inner: crate::props::basic::length::FloatValue::const_new(0),
                        }
                        .into(),
                    ),
                    CssProperty::FlexBasis(LayoutFlexBasis::Auto.into()),
                ]);
            }
            if parts.len() == 1 {
                // try grow or basis
                if let Ok(g) = parse_layout_flex_grow(parts[0]) {
                    return Ok(vec![CssProperty::FlexGrow(g.into())]);
                }
                if let Ok(b) = parse_layout_flex_basis(parts[0]) {
                    return Ok(vec![CssProperty::FlexBasis(b.into())]);
                }
            }
            if parts.len() == 2 {
                if let (Ok(g), Ok(b)) = (
                    parse_layout_flex_grow(parts[0]),
                    parse_layout_flex_basis(parts[1]),
                ) {
                    return Ok(vec![
                        CssProperty::FlexGrow(g.into()),
                        CssProperty::FlexBasis(b.into()),
                    ]);
                }
                if let (Ok(g), Ok(s)) = (
                    parse_layout_flex_grow(parts[0]),
                    parse_layout_flex_shrink(parts[1]),
                ) {
                    return Ok(vec![
                        CssProperty::FlexGrow(g.into()),
                        CssProperty::FlexShrink(s.into()),
                    ]);
                }
            }
            if parts.len() == 3 {
                let g = parse_layout_flex_grow(parts[0])?;
                let s = parse_layout_flex_shrink(parts[1])?;
                let b = parse_layout_flex_basis(parts[2])?;
                return Ok(vec![
                    CssProperty::FlexGrow(g.into()),
                    CssProperty::FlexShrink(s.into()),
                    CssProperty::FlexBasis(b.into()),
                ]);
            }
            return Err(CssParsingError::InvalidValue(InvalidValueErr(value)));
        }
        Grid => {
            // minimal: try to parse as grid-template and set both columns and rows
            let tpl = parse_grid_template(value)?;
            Ok(vec![
                CssProperty::GridTemplateColumns(tpl.clone().into()),
                CssProperty::GridTemplateRows(tpl.into()),
            ])
        }
        Gap => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            if parts.len() == 1 {
                let g = parse_layout_gap(parts[0])?;
                return Ok(vec![
                    CssProperty::RowGap(LayoutRowGap { inner: g.inner }.into()),
                    CssProperty::ColumnGap(LayoutColumnGap { inner: g.inner }.into()),
                ]);
            } else if parts.len() == 2 {
                let row = parse_layout_gap(parts[0])?;
                let col = parse_layout_gap(parts[1])?;
                return Ok(vec![
                    CssProperty::RowGap(LayoutRowGap { inner: row.inner }.into()),
                    CssProperty::ColumnGap(LayoutColumnGap { inner: col.inner }.into()),
                ]);
            } else {
                return Err(CssParsingError::InvalidValue(InvalidValueErr(value)));
            }
        }
        GridGap => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            if parts.len() == 1 {
                let g = parse_layout_gap(parts[0])?;
                return Ok(vec![
                    CssProperty::RowGap(LayoutRowGap { inner: g.inner }.into()),
                    CssProperty::ColumnGap(LayoutColumnGap { inner: g.inner }.into()),
                ]);
            } else if parts.len() == 2 {
                let row = parse_layout_gap(parts[0])?;
                let col = parse_layout_gap(parts[1])?;
                return Ok(vec![
                    CssProperty::RowGap(LayoutRowGap { inner: row.inner }.into()),
                    CssProperty::ColumnGap(LayoutColumnGap { inner: col.inner }.into()),
                ]);
            } else {
                return Err(CssParsingError::InvalidValue(InvalidValueErr(value)));
            }
        }
        Font => {
            let fam = parse_style_font_family(value)?;
            Ok(vec![CssProperty::Font(fam.into())])
        }
        Columns => {
            let mut props = Vec::new();
            for part in value.split_whitespace() {
                if let Ok(width) = parse_column_width(part) {
                    props.push(CssProperty::ColumnWidth(width.into()));
                } else if let Ok(count) = parse_column_count(part) {
                    props.push(CssProperty::ColumnCount(count.into()));
                } else {
                    return Err(CssParsingError::InvalidValue(InvalidValueErr(value)));
                }
            }
            Ok(props)
        }
        GridArea => {
            // CSS grid-area shorthand: grid-area: <name>
            // Expands to grid-row: <name> / <name> and grid-column: <name> / <name>
            // This tells taffy to resolve the named area via NamedLineResolver.
            //
            // Full syntax: grid-area: row-start / column-start / row-end / column-end
            // But for named areas, typically just: grid-area: <name>
            let parts: Vec<&str> = value.split('/').map(|s| s.trim()).collect();
            let (row_start, col_start, row_end, col_end) = match parts.len() {
                1 => (parts[0], parts[0], parts[0], parts[0]),
                2 => (parts[0], parts[1], parts[0], parts[1]),
                3 => (parts[0], parts[1], parts[2], parts[1]),
                4 => (parts[0], parts[1], parts[2], parts[3]),
                _ => return Err(CssParsingError::InvalidValue(InvalidValueErr(value))),
            };
            let parse_line = |s: &str| -> Result<GridLine, CssParsingError<'_>> {
                parse_grid_line_owned(s.trim()).map_err(|_| CssParsingError::InvalidValue(InvalidValueErr(value)))
            };
            Ok(vec![
                CssProperty::GridRow(CssPropertyValue::Exact(GridPlacement {
                    grid_start: parse_line(row_start)?,
                    grid_end: parse_line(row_end)?,
                })),
                CssProperty::GridColumn(CssPropertyValue::Exact(GridPlacement {
                    grid_start: parse_line(col_start)?,
                    grid_end: parse_line(col_end)?,
                })),
            ])
        }
        ColumnRule => {
            let border = parse_style_border(value)?;
            Ok(vec![
                CssProperty::ColumnRuleWidth(
                    ColumnRuleWidth {
                        inner: border.border_width,
                    }
                    .into(),
                ),
                CssProperty::ColumnRuleStyle(
                    ColumnRuleStyle {
                        inner: border.border_style,
                    }
                    .into(),
                ),
                CssProperty::ColumnRuleColor(
                    ColumnRuleColor {
                        inner: border.border_color,
                    }
                    .into(),
                ),
            ])
        }
    }
}

// Re-add the From implementations for convenience
macro_rules! impl_from_css_prop {
    ($a:ident, $b:ident:: $enum_type:ident) => {
        impl From<$a> for $b {
            fn from(e: $a) -> Self {
                $b::$enum_type(CssPropertyValue::from(e))
            }
        }
    };
}

impl_from_css_prop!(CaretColor, CssProperty::CaretColor);
impl_from_css_prop!(CaretWidth, CssProperty::CaretWidth);
impl_from_css_prop!(CaretAnimationDuration, CssProperty::CaretAnimationDuration);
impl_from_css_prop!(
    SelectionBackgroundColor,
    CssProperty::SelectionBackgroundColor
);
impl_from_css_prop!(SelectionColor, CssProperty::SelectionColor);
impl_from_css_prop!(SelectionRadius, CssProperty::SelectionRadius);
impl_from_css_prop!(StyleTextColor, CssProperty::TextColor);
impl_from_css_prop!(StyleFontSize, CssProperty::FontSize);
impl_from_css_prop!(StyleFontFamilyVec, CssProperty::FontFamily);
impl_from_css_prop!(StyleTextAlign, CssProperty::TextAlign);
impl_from_css_prop!(LayoutTextJustify, CssProperty::TextJustify);
impl_from_css_prop!(StyleVerticalAlign, CssProperty::VerticalAlign);
impl_from_css_prop!(StyleLetterSpacing, CssProperty::LetterSpacing);
impl_from_css_prop!(StyleTextIndent, CssProperty::TextIndent);
impl_from_css_prop!(StyleInitialLetter, CssProperty::InitialLetter);
impl_from_css_prop!(StyleLineClamp, CssProperty::LineClamp);
impl_from_css_prop!(StyleHangingPunctuation, CssProperty::HangingPunctuation);
impl_from_css_prop!(StyleTextCombineUpright, CssProperty::TextCombineUpright);
impl_from_css_prop!(StyleExclusionMargin, CssProperty::ExclusionMargin);
impl_from_css_prop!(StyleHyphenationLanguage, CssProperty::HyphenationLanguage);
impl_from_css_prop!(StyleLineHeight, CssProperty::LineHeight);
impl_from_css_prop!(StyleWordSpacing, CssProperty::WordSpacing);
impl_from_css_prop!(StyleTabSize, CssProperty::TabSize);
impl_from_css_prop!(StyleCursor, CssProperty::Cursor);
impl_from_css_prop!(LayoutDisplay, CssProperty::Display);
impl_from_css_prop!(LayoutFloat, CssProperty::Float);
impl_from_css_prop!(LayoutBoxSizing, CssProperty::BoxSizing);
impl_from_css_prop!(LayoutWidth, CssProperty::Width);
impl_from_css_prop!(LayoutHeight, CssProperty::Height);
impl_from_css_prop!(LayoutMinWidth, CssProperty::MinWidth);
impl_from_css_prop!(LayoutMinHeight, CssProperty::MinHeight);
impl_from_css_prop!(LayoutMaxWidth, CssProperty::MaxWidth);
impl_from_css_prop!(LayoutMaxHeight, CssProperty::MaxHeight);
impl_from_css_prop!(LayoutPosition, CssProperty::Position);
impl_from_css_prop!(LayoutTop, CssProperty::Top);
impl_from_css_prop!(LayoutRight, CssProperty::Right);
impl_from_css_prop!(LayoutLeft, CssProperty::Left);
impl_from_css_prop!(LayoutInsetBottom, CssProperty::Bottom);
impl_from_css_prop!(LayoutFlexWrap, CssProperty::FlexWrap);
impl_from_css_prop!(LayoutFlexDirection, CssProperty::FlexDirection);
impl_from_css_prop!(LayoutFlexGrow, CssProperty::FlexGrow);
impl_from_css_prop!(LayoutFlexShrink, CssProperty::FlexShrink);
impl_from_css_prop!(LayoutFlexBasis, CssProperty::FlexBasis);
impl_from_css_prop!(LayoutJustifyContent, CssProperty::JustifyContent);
impl_from_css_prop!(LayoutAlignItems, CssProperty::AlignItems);
impl_from_css_prop!(LayoutAlignContent, CssProperty::AlignContent);
impl_from_css_prop!(LayoutColumnGap, CssProperty::ColumnGap);
impl_from_css_prop!(LayoutRowGap, CssProperty::RowGap);
impl_from_css_prop!(LayoutGridAutoFlow, CssProperty::GridAutoFlow);
impl_from_css_prop!(LayoutJustifySelf, CssProperty::JustifySelf);
impl_from_css_prop!(LayoutJustifyItems, CssProperty::JustifyItems);
impl_from_css_prop!(LayoutGap, CssProperty::Gap);
impl_from_css_prop!(LayoutAlignSelf, CssProperty::AlignSelf);
impl_from_css_prop!(LayoutWritingMode, CssProperty::WritingMode);
impl_from_css_prop!(LayoutClear, CssProperty::Clear);
impl_from_css_prop!(StyleBackgroundContentVec, CssProperty::BackgroundContent);

impl_from_css_prop!(StyleBackgroundPositionVec, CssProperty::BackgroundPosition);
impl_from_css_prop!(StyleBackgroundSizeVec, CssProperty::BackgroundSize);
impl_from_css_prop!(StyleBackgroundRepeatVec, CssProperty::BackgroundRepeat);
impl_from_css_prop!(LayoutPaddingTop, CssProperty::PaddingTop);
impl_from_css_prop!(LayoutPaddingLeft, CssProperty::PaddingLeft);
impl_from_css_prop!(LayoutPaddingRight, CssProperty::PaddingRight);
impl_from_css_prop!(LayoutPaddingBottom, CssProperty::PaddingBottom);
impl_from_css_prop!(LayoutPaddingInlineStart, CssProperty::PaddingInlineStart);
impl_from_css_prop!(LayoutPaddingInlineEnd, CssProperty::PaddingInlineEnd);
impl_from_css_prop!(LayoutMarginTop, CssProperty::MarginTop);
impl_from_css_prop!(LayoutMarginLeft, CssProperty::MarginLeft);
impl_from_css_prop!(LayoutMarginRight, CssProperty::MarginRight);
impl_from_css_prop!(LayoutMarginBottom, CssProperty::MarginBottom);
impl_from_css_prop!(StyleBorderTopLeftRadius, CssProperty::BorderTopLeftRadius);
impl_from_css_prop!(StyleBorderTopRightRadius, CssProperty::BorderTopRightRadius);
impl_from_css_prop!(
    StyleBorderBottomLeftRadius,
    CssProperty::BorderBottomLeftRadius
);
impl_from_css_prop!(
    StyleBorderBottomRightRadius,
    CssProperty::BorderBottomRightRadius
);
impl_from_css_prop!(StyleBorderTopColor, CssProperty::BorderTopColor);
impl_from_css_prop!(StyleBorderRightColor, CssProperty::BorderRightColor);
impl_from_css_prop!(StyleBorderLeftColor, CssProperty::BorderLeftColor);
impl_from_css_prop!(StyleBorderBottomColor, CssProperty::BorderBottomColor);
impl_from_css_prop!(StyleBorderTopStyle, CssProperty::BorderTopStyle);
impl_from_css_prop!(StyleBorderRightStyle, CssProperty::BorderRightStyle);
impl_from_css_prop!(StyleBorderLeftStyle, CssProperty::BorderLeftStyle);
impl_from_css_prop!(StyleBorderBottomStyle, CssProperty::BorderBottomStyle);
impl_from_css_prop!(LayoutBorderTopWidth, CssProperty::BorderTopWidth);
impl_from_css_prop!(LayoutBorderRightWidth, CssProperty::BorderRightWidth);
impl_from_css_prop!(LayoutBorderLeftWidth, CssProperty::BorderLeftWidth);
impl_from_css_prop!(LayoutBorderBottomWidth, CssProperty::BorderBottomWidth);
impl_from_css_prop!(LayoutScrollbarWidth, CssProperty::ScrollbarWidth);
impl_from_css_prop!(StyleScrollbarColor, CssProperty::ScrollbarColor);
impl_from_css_prop!(ScrollbarVisibilityMode, CssProperty::ScrollbarVisibility);
impl_from_css_prop!(ScrollbarFadeDelay, CssProperty::ScrollbarFadeDelay);
impl_from_css_prop!(ScrollbarFadeDuration, CssProperty::ScrollbarFadeDuration);
impl_from_css_prop!(StyleOpacity, CssProperty::Opacity);
impl_from_css_prop!(StyleVisibility, CssProperty::Visibility);
impl_from_css_prop!(StyleTransformVec, CssProperty::Transform);
impl_from_css_prop!(StyleTransformOrigin, CssProperty::TransformOrigin);
impl_from_css_prop!(StylePerspectiveOrigin, CssProperty::PerspectiveOrigin);
impl_from_css_prop!(StyleBackfaceVisibility, CssProperty::BackfaceVisibility);
impl_from_css_prop!(StyleMixBlendMode, CssProperty::MixBlendMode);
impl_from_css_prop!(StyleHyphens, CssProperty::Hyphens);
impl_from_css_prop!(StyleDirection, CssProperty::Direction);
impl_from_css_prop!(StyleWhiteSpace, CssProperty::WhiteSpace);
impl_from_css_prop!(PageBreak, CssProperty::BreakBefore);
impl_from_css_prop!(BreakInside, CssProperty::BreakInside);
impl_from_css_prop!(Widows, CssProperty::Widows);
impl_from_css_prop!(Orphans, CssProperty::Orphans);
impl_from_css_prop!(BoxDecorationBreak, CssProperty::BoxDecorationBreak);
impl_from_css_prop!(ColumnCount, CssProperty::ColumnCount);
impl_from_css_prop!(ColumnWidth, CssProperty::ColumnWidth);
impl_from_css_prop!(ColumnSpan, CssProperty::ColumnSpan);
impl_from_css_prop!(ColumnFill, CssProperty::ColumnFill);
impl_from_css_prop!(ColumnRuleWidth, CssProperty::ColumnRuleWidth);
impl_from_css_prop!(ColumnRuleStyle, CssProperty::ColumnRuleStyle);
impl_from_css_prop!(ColumnRuleColor, CssProperty::ColumnRuleColor);
impl_from_css_prop!(FlowInto, CssProperty::FlowInto);
impl_from_css_prop!(FlowFrom, CssProperty::FlowFrom);
impl_from_css_prop!(ShapeOutside, CssProperty::ShapeOutside);
impl_from_css_prop!(ShapeInside, CssProperty::ShapeInside);
impl_from_css_prop!(ClipPath, CssProperty::ClipPath);
impl_from_css_prop!(ShapeMargin, CssProperty::ShapeMargin);
impl_from_css_prop!(ShapeImageThreshold, CssProperty::ShapeImageThreshold);
impl_from_css_prop!(Content, CssProperty::Content);
impl_from_css_prop!(CounterReset, CssProperty::CounterReset);
impl_from_css_prop!(CounterIncrement, CssProperty::CounterIncrement);
impl_from_css_prop!(StyleListStyleType, CssProperty::ListStyleType);
impl_from_css_prop!(StyleListStylePosition, CssProperty::ListStylePosition);
impl_from_css_prop!(StringSet, CssProperty::StringSet);
impl_from_css_prop!(LayoutTableLayout, CssProperty::TableLayout);
impl_from_css_prop!(StyleBorderCollapse, CssProperty::BorderCollapse);
impl_from_css_prop!(LayoutBorderSpacing, CssProperty::BorderSpacing);
impl_from_css_prop!(StyleCaptionSide, CssProperty::CaptionSide);
impl_from_css_prop!(StyleEmptyCells, CssProperty::EmptyCells);

impl CssProperty {
    pub fn key(&self) -> &'static str {
        self.get_type().to_str()
    }

    pub fn value(&self) -> String {
        match self {
            CssProperty::CaretColor(v) => v.get_css_value_fmt(),
            CssProperty::CaretWidth(v) => v.get_css_value_fmt(),
            CssProperty::CaretAnimationDuration(v) => v.get_css_value_fmt(),
            CssProperty::SelectionBackgroundColor(v) => v.get_css_value_fmt(),
            CssProperty::SelectionColor(v) => v.get_css_value_fmt(),
            CssProperty::SelectionRadius(v) => v.get_css_value_fmt(),
            CssProperty::TextJustify(v) => v.get_css_value_fmt(),
            CssProperty::LayoutTextJustify(v) => format!("{:?}", v),
            CssProperty::TextColor(v) => v.get_css_value_fmt(),
            CssProperty::FontSize(v) => v.get_css_value_fmt(),
            CssProperty::FontFamily(v) => v.get_css_value_fmt(),
            CssProperty::TextAlign(v) => v.get_css_value_fmt(),
            CssProperty::LetterSpacing(v) => v.get_css_value_fmt(),
            CssProperty::TextIndent(v) => v.get_css_value_fmt(),
            CssProperty::InitialLetter(v) => v.get_css_value_fmt(),
            CssProperty::LineClamp(v) => v.get_css_value_fmt(),
            CssProperty::HangingPunctuation(v) => v.get_css_value_fmt(),
            CssProperty::TextCombineUpright(v) => v.get_css_value_fmt(),
            CssProperty::ExclusionMargin(v) => v.get_css_value_fmt(),
            CssProperty::HyphenationLanguage(v) => v.get_css_value_fmt(),
            CssProperty::LineHeight(v) => v.get_css_value_fmt(),
            CssProperty::WordSpacing(v) => v.get_css_value_fmt(),
            CssProperty::TabSize(v) => v.get_css_value_fmt(),
            CssProperty::Cursor(v) => v.get_css_value_fmt(),
            CssProperty::Display(v) => v.get_css_value_fmt(),
            CssProperty::Float(v) => v.get_css_value_fmt(),
            CssProperty::BoxSizing(v) => v.get_css_value_fmt(),
            CssProperty::Width(v) => v.get_css_value_fmt(),
            CssProperty::Height(v) => v.get_css_value_fmt(),
            CssProperty::MinWidth(v) => v.get_css_value_fmt(),
            CssProperty::MinHeight(v) => v.get_css_value_fmt(),
            CssProperty::MaxWidth(v) => v.get_css_value_fmt(),
            CssProperty::MaxHeight(v) => v.get_css_value_fmt(),
            CssProperty::Position(v) => v.get_css_value_fmt(),
            CssProperty::Top(v) => v.get_css_value_fmt(),
            CssProperty::Right(v) => v.get_css_value_fmt(),
            CssProperty::Left(v) => v.get_css_value_fmt(),
            CssProperty::Bottom(v) => v.get_css_value_fmt(),
            CssProperty::ZIndex(v) => v.get_css_value_fmt(),
            CssProperty::FlexWrap(v) => v.get_css_value_fmt(),
            CssProperty::FlexDirection(v) => v.get_css_value_fmt(),
            CssProperty::FlexGrow(v) => v.get_css_value_fmt(),
            CssProperty::FlexShrink(v) => v.get_css_value_fmt(),
            CssProperty::FlexBasis(v) => v.get_css_value_fmt(),
            CssProperty::JustifyContent(v) => v.get_css_value_fmt(),
            CssProperty::AlignItems(v) => v.get_css_value_fmt(),
            CssProperty::AlignContent(v) => v.get_css_value_fmt(),
            CssProperty::ColumnGap(v) => v.get_css_value_fmt(),
            CssProperty::RowGap(v) => v.get_css_value_fmt(),
            CssProperty::GridTemplateColumns(v) => v.get_css_value_fmt(),
            CssProperty::GridTemplateRows(v) => v.get_css_value_fmt(),
            CssProperty::GridAutoFlow(v) => v.get_css_value_fmt(),
            CssProperty::JustifySelf(v) => v.get_css_value_fmt(),
            CssProperty::JustifyItems(v) => v.get_css_value_fmt(),
            CssProperty::Gap(v) => v.get_css_value_fmt(),
            CssProperty::GridGap(v) => v.get_css_value_fmt(),
            CssProperty::AlignSelf(v) => v.get_css_value_fmt(),
            CssProperty::Font(v) => v.get_css_value_fmt(),
            CssProperty::GridAutoColumns(v) => v.get_css_value_fmt(),
            CssProperty::GridAutoRows(v) => v.get_css_value_fmt(),
            CssProperty::GridColumn(v) => v.get_css_value_fmt(),
            CssProperty::GridRow(v) => v.get_css_value_fmt(),
            CssProperty::GridTemplateAreas(v) => v.get_css_value_fmt(),
            CssProperty::WritingMode(v) => v.get_css_value_fmt(),
            CssProperty::Clear(v) => v.get_css_value_fmt(),
            CssProperty::BackgroundContent(v) => v.get_css_value_fmt(),
            CssProperty::BackgroundPosition(v) => v.get_css_value_fmt(),
            CssProperty::BackgroundSize(v) => v.get_css_value_fmt(),
            CssProperty::BackgroundRepeat(v) => v.get_css_value_fmt(),
            CssProperty::OverflowX(v) => v.get_css_value_fmt(),
            CssProperty::OverflowY(v) => v.get_css_value_fmt(),
            CssProperty::PaddingTop(v) => v.get_css_value_fmt(),
            CssProperty::PaddingLeft(v) => v.get_css_value_fmt(),
            CssProperty::PaddingRight(v) => v.get_css_value_fmt(),
            CssProperty::PaddingBottom(v) => v.get_css_value_fmt(),
            CssProperty::PaddingInlineStart(v) => v.get_css_value_fmt(),
            CssProperty::PaddingInlineEnd(v) => v.get_css_value_fmt(),
            CssProperty::MarginTop(v) => v.get_css_value_fmt(),
            CssProperty::MarginLeft(v) => v.get_css_value_fmt(),
            CssProperty::MarginRight(v) => v.get_css_value_fmt(),
            CssProperty::MarginBottom(v) => v.get_css_value_fmt(),
            CssProperty::BorderTopLeftRadius(v) => v.get_css_value_fmt(),
            CssProperty::BorderTopRightRadius(v) => v.get_css_value_fmt(),
            CssProperty::BorderBottomLeftRadius(v) => v.get_css_value_fmt(),
            CssProperty::BorderBottomRightRadius(v) => v.get_css_value_fmt(),
            CssProperty::BorderTopColor(v) => v.get_css_value_fmt(),
            CssProperty::BorderRightColor(v) => v.get_css_value_fmt(),
            CssProperty::BorderLeftColor(v) => v.get_css_value_fmt(),
            CssProperty::BorderBottomColor(v) => v.get_css_value_fmt(),
            CssProperty::BorderTopStyle(v) => v.get_css_value_fmt(),
            CssProperty::BorderRightStyle(v) => v.get_css_value_fmt(),
            CssProperty::BorderLeftStyle(v) => v.get_css_value_fmt(),
            CssProperty::BorderBottomStyle(v) => v.get_css_value_fmt(),
            CssProperty::BorderTopWidth(v) => v.get_css_value_fmt(),
            CssProperty::BorderRightWidth(v) => v.get_css_value_fmt(),
            CssProperty::BorderLeftWidth(v) => v.get_css_value_fmt(),
            CssProperty::BorderBottomWidth(v) => v.get_css_value_fmt(),
            CssProperty::BoxShadowLeft(v) => v.get_css_value_fmt(),
            CssProperty::BoxShadowRight(v) => v.get_css_value_fmt(),
            CssProperty::BoxShadowTop(v) => v.get_css_value_fmt(),
            CssProperty::BoxShadowBottom(v) => v.get_css_value_fmt(),
            CssProperty::ScrollbarTrack(v) => v.get_css_value_fmt(),
            CssProperty::ScrollbarThumb(v) => v.get_css_value_fmt(),
            CssProperty::ScrollbarButton(v) => v.get_css_value_fmt(),
            CssProperty::ScrollbarCorner(v) => v.get_css_value_fmt(),
            CssProperty::ScrollbarResizer(v) => v.get_css_value_fmt(),
            CssProperty::ScrollbarWidth(v) => v.get_css_value_fmt(),
            CssProperty::ScrollbarColor(v) => v.get_css_value_fmt(),
            CssProperty::ScrollbarVisibility(v) => v.get_css_value_fmt(),
            CssProperty::ScrollbarFadeDelay(v) => v.get_css_value_fmt(),
            CssProperty::ScrollbarFadeDuration(v) => v.get_css_value_fmt(),
            CssProperty::Opacity(v) => v.get_css_value_fmt(),
            CssProperty::Visibility(v) => v.get_css_value_fmt(),
            CssProperty::Transform(v) => v.get_css_value_fmt(),
            CssProperty::TransformOrigin(v) => v.get_css_value_fmt(),
            CssProperty::PerspectiveOrigin(v) => v.get_css_value_fmt(),
            CssProperty::BackfaceVisibility(v) => v.get_css_value_fmt(),
            CssProperty::MixBlendMode(v) => v.get_css_value_fmt(),
            CssProperty::Filter(v) => v.get_css_value_fmt(),
            CssProperty::BackdropFilter(v) => v.get_css_value_fmt(),
            CssProperty::TextShadow(v) => v.get_css_value_fmt(),
            CssProperty::Hyphens(v) => v.get_css_value_fmt(),
            CssProperty::Direction(v) => v.get_css_value_fmt(),
            CssProperty::UserSelect(v) => v.get_css_value_fmt(),
            CssProperty::TextDecoration(v) => v.get_css_value_fmt(),
            CssProperty::WhiteSpace(v) => v.get_css_value_fmt(),
            CssProperty::BreakBefore(v) => v.get_css_value_fmt(),
            CssProperty::BreakAfter(v) => v.get_css_value_fmt(),
            CssProperty::BreakInside(v) => v.get_css_value_fmt(),
            CssProperty::Orphans(v) => v.get_css_value_fmt(),
            CssProperty::Widows(v) => v.get_css_value_fmt(),
            CssProperty::BoxDecorationBreak(v) => v.get_css_value_fmt(),
            CssProperty::ColumnCount(v) => v.get_css_value_fmt(),
            CssProperty::ColumnWidth(v) => v.get_css_value_fmt(),
            CssProperty::ColumnSpan(v) => v.get_css_value_fmt(),
            CssProperty::ColumnFill(v) => v.get_css_value_fmt(),
            CssProperty::ColumnRuleWidth(v) => v.get_css_value_fmt(),
            CssProperty::ColumnRuleStyle(v) => v.get_css_value_fmt(),
            CssProperty::ColumnRuleColor(v) => v.get_css_value_fmt(),
            CssProperty::FlowInto(v) => v.get_css_value_fmt(),
            CssProperty::FlowFrom(v) => v.get_css_value_fmt(),
            CssProperty::ShapeOutside(v) => v.get_css_value_fmt(),
            CssProperty::ShapeInside(v) => v.get_css_value_fmt(),
            CssProperty::ClipPath(v) => v.get_css_value_fmt(),
            CssProperty::ShapeMargin(v) => v.get_css_value_fmt(),
            CssProperty::ShapeImageThreshold(v) => v.get_css_value_fmt(),
            CssProperty::Content(v) => v.get_css_value_fmt(),
            CssProperty::CounterReset(v) => v.get_css_value_fmt(),
            CssProperty::CounterIncrement(v) => v.get_css_value_fmt(),
            CssProperty::ListStyleType(v) => v.get_css_value_fmt(),
            CssProperty::ListStylePosition(v) => v.get_css_value_fmt(),
            CssProperty::StringSet(v) => v.get_css_value_fmt(),
            CssProperty::TableLayout(v) => v.get_css_value_fmt(),
            CssProperty::BorderCollapse(v) => v.get_css_value_fmt(),
            CssProperty::BorderSpacing(v) => v.get_css_value_fmt(),
            CssProperty::CaptionSide(v) => v.get_css_value_fmt(),
            CssProperty::EmptyCells(v) => v.get_css_value_fmt(),
            CssProperty::FontWeight(v) => v.get_css_value_fmt(),
            CssProperty::FontStyle(v) => v.get_css_value_fmt(),
            CssProperty::VerticalAlign(v) => v.get_css_value_fmt(),
        }
    }

    pub fn format_css(&self) -> String {
        format!("{}: {};", self.key(), self.value())
    }

    pub fn interpolate(
        &self,
        other: &Self,
        t: f32,
        interpolate_resolver: &InterpolateResolver,
    ) -> Self {
        if t <= 0.0 {
            return self.clone();
        } else if t >= 1.0 {
            return other.clone();
        }

        // Map from linear interpolation function to Easing curve
        let t: f32 = interpolate_resolver.interpolate_func.evaluate(t as f64);

        let t = t.max(0.0).min(1.0);

        match (self, other) {
            (CssProperty::TextColor(col_start), CssProperty::TextColor(col_end)) => {
                let col_start = col_start.get_property().copied().unwrap_or_default();
                let col_end = col_end.get_property().copied().unwrap_or_default();
                CssProperty::text_color(col_start.interpolate(&col_end, t))
            }
            (CssProperty::FontSize(fs_start), CssProperty::FontSize(fs_end)) => {
                let fs_start = fs_start.get_property().copied().unwrap_or_default();
                let fs_end = fs_end.get_property().copied().unwrap_or_default();
                CssProperty::font_size(fs_start.interpolate(&fs_end, t))
            }
            (CssProperty::LetterSpacing(ls_start), CssProperty::LetterSpacing(ls_end)) => {
                let ls_start = ls_start.get_property().copied().unwrap_or_default();
                let ls_end = ls_end.get_property().copied().unwrap_or_default();
                CssProperty::letter_spacing(ls_start.interpolate(&ls_end, t))
            }
            (CssProperty::TextIndent(ti_start), CssProperty::TextIndent(ti_end)) => {
                let ti_start = ti_start.get_property().copied().unwrap_or_default();
                let ti_end = ti_end.get_property().copied().unwrap_or_default();
                CssProperty::text_indent(ti_start.interpolate(&ti_end, t))
            }
            (CssProperty::LineHeight(lh_start), CssProperty::LineHeight(lh_end)) => {
                let lh_start = lh_start.get_property().copied().unwrap_or_default();
                let lh_end = lh_end.get_property().copied().unwrap_or_default();
                CssProperty::line_height(lh_start.interpolate(&lh_end, t))
            }
            (CssProperty::WordSpacing(ws_start), CssProperty::WordSpacing(ws_end)) => {
                let ws_start = ws_start.get_property().copied().unwrap_or_default();
                let ws_end = ws_end.get_property().copied().unwrap_or_default();
                CssProperty::word_spacing(ws_start.interpolate(&ws_end, t))
            }
            (CssProperty::TabSize(tw_start), CssProperty::TabSize(tw_end)) => {
                let tw_start = tw_start.get_property().copied().unwrap_or_default();
                let tw_end = tw_end.get_property().copied().unwrap_or_default();
                CssProperty::tab_size(tw_start.interpolate(&tw_end, t))
            }
            (CssProperty::Width(start), CssProperty::Width(end)) => {
                let start =
                    start
                        .get_property()
                        .cloned()
                        .unwrap_or(LayoutWidth::Px(PixelValue::px(
                            interpolate_resolver.current_rect_width,
                        )));
                let end = end.get_property().cloned().unwrap_or_default();
                CssProperty::Width(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::Height(start), CssProperty::Height(end)) => {
                let start =
                    start
                        .get_property()
                        .cloned()
                        .unwrap_or(LayoutHeight::Px(PixelValue::px(
                            interpolate_resolver.current_rect_height,
                        )));
                let end = end.get_property().cloned().unwrap_or_default();
                CssProperty::Height(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MinWidth(start), CssProperty::MinWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MinWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MinHeight(start), CssProperty::MinHeight(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MinHeight(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MaxWidth(start), CssProperty::MaxWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MaxWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MaxHeight(start), CssProperty::MaxHeight(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MaxHeight(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::Top(start), CssProperty::Top(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::Top(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::Right(start), CssProperty::Right(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::Right(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::Left(start), CssProperty::Left(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::Left(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::Bottom(start), CssProperty::Bottom(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::Bottom(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::FlexGrow(start), CssProperty::FlexGrow(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::FlexGrow(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::FlexShrink(start), CssProperty::FlexShrink(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::FlexShrink(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::PaddingTop(start), CssProperty::PaddingTop(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::PaddingTop(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::PaddingLeft(start), CssProperty::PaddingLeft(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::PaddingLeft(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::PaddingRight(start), CssProperty::PaddingRight(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::PaddingRight(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::PaddingBottom(start), CssProperty::PaddingBottom(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::PaddingBottom(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MarginTop(start), CssProperty::MarginTop(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MarginTop(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MarginLeft(start), CssProperty::MarginLeft(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MarginLeft(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MarginRight(start), CssProperty::MarginRight(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MarginRight(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::MarginBottom(start), CssProperty::MarginBottom(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::MarginBottom(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderTopLeftRadius(start), CssProperty::BorderTopLeftRadius(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderTopLeftRadius(CssPropertyValue::Exact(
                    start.interpolate(&end, t),
                ))
            }
            (CssProperty::BorderTopRightRadius(start), CssProperty::BorderTopRightRadius(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderTopRightRadius(CssPropertyValue::Exact(
                    start.interpolate(&end, t),
                ))
            }
            (
                CssProperty::BorderBottomLeftRadius(start),
                CssProperty::BorderBottomLeftRadius(end),
            ) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderBottomLeftRadius(CssPropertyValue::Exact(
                    start.interpolate(&end, t),
                ))
            }
            (
                CssProperty::BorderBottomRightRadius(start),
                CssProperty::BorderBottomRightRadius(end),
            ) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderBottomRightRadius(CssPropertyValue::Exact(
                    start.interpolate(&end, t),
                ))
            }
            (CssProperty::BorderTopColor(start), CssProperty::BorderTopColor(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderTopColor(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderRightColor(start), CssProperty::BorderRightColor(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderRightColor(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderLeftColor(start), CssProperty::BorderLeftColor(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderLeftColor(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderBottomColor(start), CssProperty::BorderBottomColor(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderBottomColor(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderTopWidth(start), CssProperty::BorderTopWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderTopWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderRightWidth(start), CssProperty::BorderRightWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderRightWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderLeftWidth(start), CssProperty::BorderLeftWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderLeftWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::BorderBottomWidth(start), CssProperty::BorderBottomWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::BorderBottomWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::Opacity(start), CssProperty::Opacity(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::Opacity(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::TransformOrigin(start), CssProperty::TransformOrigin(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::TransformOrigin(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (CssProperty::PerspectiveOrigin(start), CssProperty::PerspectiveOrigin(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                CssProperty::PerspectiveOrigin(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            /*
            animate transform:
            CssProperty::Transform(CssPropertyValue<StyleTransformVec>),

            animate box shadow:
            CssProperty::BoxShadowLeft(CssPropertyValue<StyleBoxShadow>),
            CssProperty::BoxShadowRight(CssPropertyValue<StyleBoxShadow>),
            CssProperty::BoxShadowTop(CssPropertyValue<StyleBoxShadow>),
            CssProperty::BoxShadowBottom(CssPropertyValue<StyleBoxShadow>),

            animate background:
            CssProperty::BackgroundContent(CssPropertyValue<StyleBackgroundContentVec>),
            CssProperty::BackgroundPosition(CssPropertyValue<StyleBackgroundPositionVec>),
            CssProperty::BackgroundSize(CssPropertyValue<StyleBackgroundSizeVec>),
            */
            (_, _) => {
                // not animatable, fallback
                if t > 0.5 {
                    other.clone()
                } else {
                    self.clone()
                }
            }
        }
    }

    /// Return the type (key) of this property as a statically typed enum
    pub const fn get_type(&self) -> CssPropertyType {
        match &self {
            CssProperty::CaretColor(_) => CssPropertyType::CaretColor,
            CssProperty::CaretWidth(_) => CssPropertyType::CaretWidth,
            CssProperty::CaretAnimationDuration(_) => CssPropertyType::CaretAnimationDuration,
            CssProperty::SelectionBackgroundColor(_) => CssPropertyType::SelectionBackgroundColor,
            CssProperty::SelectionColor(_) => CssPropertyType::SelectionColor,
            CssProperty::SelectionRadius(_) => CssPropertyType::SelectionRadius,

            CssProperty::TextJustify(_) => CssPropertyType::TextJustify,
            CssProperty::LayoutTextJustify(_) => CssPropertyType::TextAlign, /* oder ggf. ein */
            // eigener Typ
            CssProperty::TextColor(_) => CssPropertyType::TextColor,
            CssProperty::FontSize(_) => CssPropertyType::FontSize,
            CssProperty::FontFamily(_) => CssPropertyType::FontFamily,
            CssProperty::FontWeight(_) => CssPropertyType::FontWeight,
            CssProperty::FontStyle(_) => CssPropertyType::FontStyle,
            CssProperty::TextAlign(_) => CssPropertyType::TextAlign,
            CssProperty::VerticalAlign(_) => CssPropertyType::VerticalAlign,
            CssProperty::LetterSpacing(_) => CssPropertyType::LetterSpacing,
            CssProperty::TextIndent(_) => CssPropertyType::TextIndent,
            CssProperty::InitialLetter(_) => CssPropertyType::InitialLetter,
            CssProperty::LineClamp(_) => CssPropertyType::LineClamp,
            CssProperty::HangingPunctuation(_) => CssPropertyType::HangingPunctuation,
            CssProperty::TextCombineUpright(_) => CssPropertyType::TextCombineUpright,
            CssProperty::ExclusionMargin(_) => CssPropertyType::ExclusionMargin,
            CssProperty::HyphenationLanguage(_) => CssPropertyType::HyphenationLanguage,
            CssProperty::LineHeight(_) => CssPropertyType::LineHeight,
            CssProperty::WordSpacing(_) => CssPropertyType::WordSpacing,
            CssProperty::TabSize(_) => CssPropertyType::TabSize,
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
            CssProperty::ZIndex(_) => CssPropertyType::ZIndex,
            CssProperty::FlexWrap(_) => CssPropertyType::FlexWrap,
            CssProperty::FlexDirection(_) => CssPropertyType::FlexDirection,
            CssProperty::FlexGrow(_) => CssPropertyType::FlexGrow,
            CssProperty::FlexShrink(_) => CssPropertyType::FlexShrink,
            CssProperty::FlexBasis(_) => CssPropertyType::FlexBasis,
            CssProperty::JustifyContent(_) => CssPropertyType::JustifyContent,
            CssProperty::AlignItems(_) => CssPropertyType::AlignItems,
            CssProperty::AlignContent(_) => CssPropertyType::AlignContent,
            CssProperty::ColumnGap(_) => CssPropertyType::ColumnGap,
            CssProperty::RowGap(_) => CssPropertyType::RowGap,
            CssProperty::GridTemplateColumns(_) => CssPropertyType::GridTemplateColumns,
            CssProperty::GridTemplateRows(_) => CssPropertyType::GridTemplateRows,
            CssProperty::GridAutoColumns(_) => CssPropertyType::GridAutoColumns,
            CssProperty::GridAutoRows(_) => CssPropertyType::GridAutoRows,
            CssProperty::GridColumn(_) => CssPropertyType::GridColumn,
            CssProperty::GridAutoFlow(_) => CssPropertyType::GridAutoFlow,
            CssProperty::JustifySelf(_) => CssPropertyType::JustifySelf,
            CssProperty::JustifyItems(_) => CssPropertyType::JustifyItems,
            CssProperty::Gap(_) => CssPropertyType::Gap,
            CssProperty::GridGap(_) => CssPropertyType::GridGap,
            CssProperty::AlignSelf(_) => CssPropertyType::AlignSelf,
            CssProperty::Font(_) => CssPropertyType::Font,
            CssProperty::GridRow(_) => CssPropertyType::GridRow,
            CssProperty::GridTemplateAreas(_) => CssPropertyType::GridTemplateAreas,
            CssProperty::WritingMode(_) => CssPropertyType::WritingMode,
            CssProperty::Clear(_) => CssPropertyType::Clear,
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
            CssProperty::PaddingInlineStart(_) => CssPropertyType::PaddingInlineStart,
            CssProperty::PaddingInlineEnd(_) => CssPropertyType::PaddingInlineEnd,
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
            CssProperty::ScrollbarTrack(_) => CssPropertyType::ScrollbarTrack,
            CssProperty::ScrollbarThumb(_) => CssPropertyType::ScrollbarThumb,
            CssProperty::ScrollbarButton(_) => CssPropertyType::ScrollbarButton,
            CssProperty::ScrollbarCorner(_) => CssPropertyType::ScrollbarCorner,
            CssProperty::ScrollbarResizer(_) => CssPropertyType::ScrollbarResizer,
            CssProperty::ScrollbarWidth(_) => CssPropertyType::ScrollbarWidth,
            CssProperty::ScrollbarColor(_) => CssPropertyType::ScrollbarColor,
            CssProperty::ScrollbarVisibility(_) => CssPropertyType::ScrollbarVisibility,
            CssProperty::ScrollbarFadeDelay(_) => CssPropertyType::ScrollbarFadeDelay,
            CssProperty::ScrollbarFadeDuration(_) => CssPropertyType::ScrollbarFadeDuration,
            CssProperty::Opacity(_) => CssPropertyType::Opacity,
            CssProperty::Visibility(_) => CssPropertyType::Visibility,
            CssProperty::Transform(_) => CssPropertyType::Transform,
            CssProperty::PerspectiveOrigin(_) => CssPropertyType::PerspectiveOrigin,
            CssProperty::TransformOrigin(_) => CssPropertyType::TransformOrigin,
            CssProperty::BackfaceVisibility(_) => CssPropertyType::BackfaceVisibility,
            CssProperty::MixBlendMode(_) => CssPropertyType::MixBlendMode,
            CssProperty::Filter(_) => CssPropertyType::Filter,
            CssProperty::BackdropFilter(_) => CssPropertyType::BackdropFilter,
            CssProperty::TextShadow(_) => CssPropertyType::TextShadow,
            CssProperty::WhiteSpace(_) => CssPropertyType::WhiteSpace,
            CssProperty::Hyphens(_) => CssPropertyType::Hyphens,
            CssProperty::Direction(_) => CssPropertyType::Direction,
            CssProperty::UserSelect(_) => CssPropertyType::UserSelect,
            CssProperty::TextDecoration(_) => CssPropertyType::TextDecoration,
            CssProperty::BreakBefore(_) => CssPropertyType::BreakBefore,
            CssProperty::BreakAfter(_) => CssPropertyType::BreakAfter,
            CssProperty::BreakInside(_) => CssPropertyType::BreakInside,
            CssProperty::Orphans(_) => CssPropertyType::Orphans,
            CssProperty::Widows(_) => CssPropertyType::Widows,
            CssProperty::BoxDecorationBreak(_) => CssPropertyType::BoxDecorationBreak,
            CssProperty::ColumnCount(_) => CssPropertyType::ColumnCount,
            CssProperty::ColumnWidth(_) => CssPropertyType::ColumnWidth,
            CssProperty::ColumnSpan(_) => CssPropertyType::ColumnSpan,
            CssProperty::ColumnFill(_) => CssPropertyType::ColumnFill,
            CssProperty::ColumnRuleWidth(_) => CssPropertyType::ColumnRuleWidth,
            CssProperty::ColumnRuleStyle(_) => CssPropertyType::ColumnRuleStyle,
            CssProperty::ColumnRuleColor(_) => CssPropertyType::ColumnRuleColor,
            CssProperty::FlowInto(_) => CssPropertyType::FlowInto,
            CssProperty::FlowFrom(_) => CssPropertyType::FlowFrom,
            CssProperty::ShapeOutside(_) => CssPropertyType::ShapeOutside,
            CssProperty::ShapeInside(_) => CssPropertyType::ShapeInside,
            CssProperty::ClipPath(_) => CssPropertyType::ClipPath,
            CssProperty::ShapeMargin(_) => CssPropertyType::ShapeMargin,
            CssProperty::ShapeImageThreshold(_) => CssPropertyType::ShapeImageThreshold,
            CssProperty::Content(_) => CssPropertyType::Content,
            CssProperty::CounterReset(_) => CssPropertyType::CounterReset,
            CssProperty::CounterIncrement(_) => CssPropertyType::CounterIncrement,
            CssProperty::ListStyleType(_) => CssPropertyType::ListStyleType,
            CssProperty::ListStylePosition(_) => CssPropertyType::ListStylePosition,
            CssProperty::StringSet(_) => CssPropertyType::StringSet,
            CssProperty::TableLayout(_) => CssPropertyType::TableLayout,
            CssProperty::BorderCollapse(_) => CssPropertyType::BorderCollapse,
            CssProperty::BorderSpacing(_) => CssPropertyType::BorderSpacing,
            CssProperty::CaptionSide(_) => CssPropertyType::CaptionSide,
            CssProperty::EmptyCells(_) => CssPropertyType::EmptyCells,
            CssProperty::FontWeight(_) => CssPropertyType::FontWeight,
            CssProperty::FontStyle(_) => CssPropertyType::FontStyle,
        }
    }

    // const constructors for easier API access

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

    pub const fn text_color(input: StyleTextColor) -> Self {
        CssProperty::TextColor(CssPropertyValue::Exact(input))
    }
    pub const fn font_size(input: StyleFontSize) -> Self {
        CssProperty::FontSize(CssPropertyValue::Exact(input))
    }
    pub const fn font_family(input: StyleFontFamilyVec) -> Self {
        CssProperty::FontFamily(CssPropertyValue::Exact(input))
    }
    pub const fn font_weight(input: StyleFontWeight) -> Self {
        CssProperty::FontWeight(CssPropertyValue::Exact(input))
    }
    pub const fn font_style(input: StyleFontStyle) -> Self {
        CssProperty::FontStyle(CssPropertyValue::Exact(input))
    }
    pub const fn text_align(input: StyleTextAlign) -> Self {
        CssProperty::TextAlign(CssPropertyValue::Exact(input))
    }
    pub const fn text_justify(input: LayoutTextJustify) -> Self {
        CssProperty::TextJustify(CssPropertyValue::Exact(input))
    }
    pub const fn vertical_align(input: StyleVerticalAlign) -> Self {
        CssProperty::VerticalAlign(CssPropertyValue::Exact(input))
    }
    pub const fn letter_spacing(input: StyleLetterSpacing) -> Self {
        CssProperty::LetterSpacing(CssPropertyValue::Exact(input))
    }
    pub const fn text_indent(input: StyleTextIndent) -> Self {
        CssProperty::TextIndent(CssPropertyValue::Exact(input))
    }
    pub const fn line_height(input: StyleLineHeight) -> Self {
        CssProperty::LineHeight(CssPropertyValue::Exact(input))
    }
    pub const fn word_spacing(input: StyleWordSpacing) -> Self {
        CssProperty::WordSpacing(CssPropertyValue::Exact(input))
    }
    pub const fn tab_size(input: StyleTabSize) -> Self {
        CssProperty::TabSize(CssPropertyValue::Exact(input))
    }
    pub const fn cursor(input: StyleCursor) -> Self {
        CssProperty::Cursor(CssPropertyValue::Exact(input))
    }
    pub const fn user_select(input: StyleUserSelect) -> Self {
        CssProperty::UserSelect(CssPropertyValue::Exact(input))
    }
    pub const fn text_decoration(input: StyleTextDecoration) -> Self {
        CssProperty::TextDecoration(CssPropertyValue::Exact(input))
    }
    pub const fn display(input: LayoutDisplay) -> Self {
        CssProperty::Display(CssPropertyValue::Exact(input))
    }
    pub const fn box_sizing(input: LayoutBoxSizing) -> Self {
        CssProperty::BoxSizing(CssPropertyValue::Exact(input))
    }
    pub const fn width(input: LayoutWidth) -> Self {
        CssProperty::Width(CssPropertyValue::Exact(input))
    }
    pub const fn height(input: LayoutHeight) -> Self {
        CssProperty::Height(CssPropertyValue::Exact(input))
    }
    pub const fn min_width(input: LayoutMinWidth) -> Self {
        CssProperty::MinWidth(CssPropertyValue::Exact(input))
    }
    pub const fn caret_color(input: CaretColor) -> Self {
        CssProperty::CaretColor(CssPropertyValue::Exact(input))
    }
    pub const fn caret_width(input: CaretWidth) -> Self {
        CssProperty::CaretWidth(CssPropertyValue::Exact(input))
    }
    pub const fn caret_animation_duration(input: CaretAnimationDuration) -> Self {
        CssProperty::CaretAnimationDuration(CssPropertyValue::Exact(input))
    }
    pub const fn selection_background_color(input: SelectionBackgroundColor) -> Self {
        CssProperty::SelectionBackgroundColor(CssPropertyValue::Exact(input))
    }
    pub const fn selection_color(input: SelectionColor) -> Self {
        CssProperty::SelectionColor(CssPropertyValue::Exact(input))
    }
    pub const fn min_height(input: LayoutMinHeight) -> Self {
        CssProperty::MinHeight(CssPropertyValue::Exact(input))
    }
    pub const fn max_width(input: LayoutMaxWidth) -> Self {
        CssProperty::MaxWidth(CssPropertyValue::Exact(input))
    }
    pub const fn max_height(input: LayoutMaxHeight) -> Self {
        CssProperty::MaxHeight(CssPropertyValue::Exact(input))
    }
    pub const fn position(input: LayoutPosition) -> Self {
        CssProperty::Position(CssPropertyValue::Exact(input))
    }
    pub const fn top(input: LayoutTop) -> Self {
        CssProperty::Top(CssPropertyValue::Exact(input))
    }
    pub const fn right(input: LayoutRight) -> Self {
        CssProperty::Right(CssPropertyValue::Exact(input))
    }
    pub const fn left(input: LayoutLeft) -> Self {
        CssProperty::Left(CssPropertyValue::Exact(input))
    }
    pub const fn bottom(input: LayoutInsetBottom) -> Self {
        CssProperty::Bottom(CssPropertyValue::Exact(input))
    }
    pub const fn z_index(input: LayoutZIndex) -> Self {
        CssProperty::ZIndex(CssPropertyValue::Exact(input))
    }
    pub const fn flex_wrap(input: LayoutFlexWrap) -> Self {
        CssProperty::FlexWrap(CssPropertyValue::Exact(input))
    }
    pub const fn flex_direction(input: LayoutFlexDirection) -> Self {
        CssProperty::FlexDirection(CssPropertyValue::Exact(input))
    }
    pub const fn flex_grow(input: LayoutFlexGrow) -> Self {
        CssProperty::FlexGrow(CssPropertyValue::Exact(input))
    }
    pub const fn flex_shrink(input: LayoutFlexShrink) -> Self {
        CssProperty::FlexShrink(CssPropertyValue::Exact(input))
    }
    pub const fn justify_content(input: LayoutJustifyContent) -> Self {
        CssProperty::JustifyContent(CssPropertyValue::Exact(input))
    }
    pub const fn grid_auto_flow(input: LayoutGridAutoFlow) -> Self {
        CssProperty::GridAutoFlow(CssPropertyValue::Exact(input))
    }
    pub const fn justify_self(input: LayoutJustifySelf) -> Self {
        CssProperty::JustifySelf(CssPropertyValue::Exact(input))
    }
    pub const fn justify_items(input: LayoutJustifyItems) -> Self {
        CssProperty::JustifyItems(CssPropertyValue::Exact(input))
    }
    pub const fn gap(input: LayoutGap) -> Self {
        CssProperty::Gap(CssPropertyValue::Exact(input))
    }
    pub const fn grid_gap(input: LayoutGap) -> Self {
        CssProperty::GridGap(CssPropertyValue::Exact(input))
    }
    pub const fn align_self(input: LayoutAlignSelf) -> Self {
        CssProperty::AlignSelf(CssPropertyValue::Exact(input))
    }
    pub const fn font(input: StyleFontFamilyVec) -> Self {
        CssProperty::Font(StyleFontValue::Exact(input))
    }
    pub const fn align_items(input: LayoutAlignItems) -> Self {
        CssProperty::AlignItems(CssPropertyValue::Exact(input))
    }
    pub const fn align_content(input: LayoutAlignContent) -> Self {
        CssProperty::AlignContent(CssPropertyValue::Exact(input))
    }
    pub const fn background_content(input: StyleBackgroundContentVec) -> Self {
        CssProperty::BackgroundContent(CssPropertyValue::Exact(input))
    }
    pub const fn background_position(input: StyleBackgroundPositionVec) -> Self {
        CssProperty::BackgroundPosition(CssPropertyValue::Exact(input))
    }
    pub const fn background_size(input: StyleBackgroundSizeVec) -> Self {
        CssProperty::BackgroundSize(CssPropertyValue::Exact(input))
    }
    pub const fn background_repeat(input: StyleBackgroundRepeatVec) -> Self {
        CssProperty::BackgroundRepeat(CssPropertyValue::Exact(input))
    }
    pub const fn overflow_x(input: LayoutOverflow) -> Self {
        CssProperty::OverflowX(CssPropertyValue::Exact(input))
    }
    pub const fn overflow_y(input: LayoutOverflow) -> Self {
        CssProperty::OverflowY(CssPropertyValue::Exact(input))
    }
    pub const fn padding_top(input: LayoutPaddingTop) -> Self {
        CssProperty::PaddingTop(CssPropertyValue::Exact(input))
    }
    pub const fn padding_left(input: LayoutPaddingLeft) -> Self {
        CssProperty::PaddingLeft(CssPropertyValue::Exact(input))
    }
    pub const fn padding_right(input: LayoutPaddingRight) -> Self {
        CssProperty::PaddingRight(CssPropertyValue::Exact(input))
    }
    pub const fn padding_bottom(input: LayoutPaddingBottom) -> Self {
        CssProperty::PaddingBottom(CssPropertyValue::Exact(input))
    }
    pub const fn margin_top(input: LayoutMarginTop) -> Self {
        CssProperty::MarginTop(CssPropertyValue::Exact(input))
    }
    pub const fn margin_left(input: LayoutMarginLeft) -> Self {
        CssProperty::MarginLeft(CssPropertyValue::Exact(input))
    }
    pub const fn margin_right(input: LayoutMarginRight) -> Self {
        CssProperty::MarginRight(CssPropertyValue::Exact(input))
    }
    pub const fn margin_bottom(input: LayoutMarginBottom) -> Self {
        CssProperty::MarginBottom(CssPropertyValue::Exact(input))
    }
    pub const fn border_top_left_radius(input: StyleBorderTopLeftRadius) -> Self {
        CssProperty::BorderTopLeftRadius(CssPropertyValue::Exact(input))
    }
    pub const fn border_top_right_radius(input: StyleBorderTopRightRadius) -> Self {
        CssProperty::BorderTopRightRadius(CssPropertyValue::Exact(input))
    }
    pub const fn border_bottom_left_radius(input: StyleBorderBottomLeftRadius) -> Self {
        CssProperty::BorderBottomLeftRadius(CssPropertyValue::Exact(input))
    }
    pub const fn border_bottom_right_radius(input: StyleBorderBottomRightRadius) -> Self {
        CssProperty::BorderBottomRightRadius(CssPropertyValue::Exact(input))
    }
    pub const fn border_top_color(input: StyleBorderTopColor) -> Self {
        CssProperty::BorderTopColor(CssPropertyValue::Exact(input))
    }
    pub const fn border_right_color(input: StyleBorderRightColor) -> Self {
        CssProperty::BorderRightColor(CssPropertyValue::Exact(input))
    }
    pub const fn border_left_color(input: StyleBorderLeftColor) -> Self {
        CssProperty::BorderLeftColor(CssPropertyValue::Exact(input))
    }
    pub const fn border_bottom_color(input: StyleBorderBottomColor) -> Self {
        CssProperty::BorderBottomColor(CssPropertyValue::Exact(input))
    }
    pub const fn border_top_style(input: StyleBorderTopStyle) -> Self {
        CssProperty::BorderTopStyle(CssPropertyValue::Exact(input))
    }
    pub const fn border_right_style(input: StyleBorderRightStyle) -> Self {
        CssProperty::BorderRightStyle(CssPropertyValue::Exact(input))
    }
    pub const fn border_left_style(input: StyleBorderLeftStyle) -> Self {
        CssProperty::BorderLeftStyle(CssPropertyValue::Exact(input))
    }
    pub const fn border_bottom_style(input: StyleBorderBottomStyle) -> Self {
        CssProperty::BorderBottomStyle(CssPropertyValue::Exact(input))
    }
    pub const fn border_top_width(input: LayoutBorderTopWidth) -> Self {
        CssProperty::BorderTopWidth(CssPropertyValue::Exact(input))
    }
    pub const fn border_right_width(input: LayoutBorderRightWidth) -> Self {
        CssProperty::BorderRightWidth(CssPropertyValue::Exact(input))
    }
    pub const fn border_left_width(input: LayoutBorderLeftWidth) -> Self {
        CssProperty::BorderLeftWidth(CssPropertyValue::Exact(input))
    }
    pub const fn border_bottom_width(input: LayoutBorderBottomWidth) -> Self {
        CssProperty::BorderBottomWidth(CssPropertyValue::Exact(input))
    }
    pub const fn box_shadow_left(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowLeft(CssPropertyValue::Exact(input))
    }
    pub const fn box_shadow_right(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowRight(CssPropertyValue::Exact(input))
    }
    pub const fn box_shadow_top(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowTop(CssPropertyValue::Exact(input))
    }
    pub const fn box_shadow_bottom(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowBottom(CssPropertyValue::Exact(input))
    }
    pub const fn opacity(input: StyleOpacity) -> Self {
        CssProperty::Opacity(CssPropertyValue::Exact(input))
    }
    pub const fn visibility(input: StyleVisibility) -> Self {
        CssProperty::Visibility(CssPropertyValue::Exact(input))
    }
    pub const fn transform(input: StyleTransformVec) -> Self {
        CssProperty::Transform(CssPropertyValue::Exact(input))
    }
    pub const fn transform_origin(input: StyleTransformOrigin) -> Self {
        CssProperty::TransformOrigin(CssPropertyValue::Exact(input))
    }
    pub const fn perspective_origin(input: StylePerspectiveOrigin) -> Self {
        CssProperty::PerspectiveOrigin(CssPropertyValue::Exact(input))
    }
    pub const fn backface_visiblity(input: StyleBackfaceVisibility) -> Self {
        CssProperty::BackfaceVisibility(CssPropertyValue::Exact(input))
    }

    // New DTP const fn constructors
    pub const fn break_before(input: PageBreak) -> Self {
        CssProperty::BreakBefore(CssPropertyValue::Exact(input))
    }
    pub const fn break_after(input: PageBreak) -> Self {
        CssProperty::BreakAfter(CssPropertyValue::Exact(input))
    }
    pub const fn break_inside(input: BreakInside) -> Self {
        CssProperty::BreakInside(CssPropertyValue::Exact(input))
    }
    pub const fn orphans(input: Orphans) -> Self {
        CssProperty::Orphans(CssPropertyValue::Exact(input))
    }
    pub const fn widows(input: Widows) -> Self {
        CssProperty::Widows(CssPropertyValue::Exact(input))
    }
    pub const fn box_decoration_break(input: BoxDecorationBreak) -> Self {
        CssProperty::BoxDecorationBreak(CssPropertyValue::Exact(input))
    }
    pub const fn column_count(input: ColumnCount) -> Self {
        CssProperty::ColumnCount(CssPropertyValue::Exact(input))
    }
    pub const fn column_width(input: ColumnWidth) -> Self {
        CssProperty::ColumnWidth(CssPropertyValue::Exact(input))
    }
    pub const fn column_span(input: ColumnSpan) -> Self {
        CssProperty::ColumnSpan(CssPropertyValue::Exact(input))
    }
    pub const fn column_fill(input: ColumnFill) -> Self {
        CssProperty::ColumnFill(CssPropertyValue::Exact(input))
    }
    pub const fn column_rule_width(input: ColumnRuleWidth) -> Self {
        CssProperty::ColumnRuleWidth(CssPropertyValue::Exact(input))
    }
    pub const fn column_rule_style(input: ColumnRuleStyle) -> Self {
        CssProperty::ColumnRuleStyle(CssPropertyValue::Exact(input))
    }
    pub const fn column_rule_color(input: ColumnRuleColor) -> Self {
        CssProperty::ColumnRuleColor(CssPropertyValue::Exact(input))
    }
    pub const fn flow_into(input: FlowInto) -> Self {
        CssProperty::FlowInto(CssPropertyValue::Exact(input))
    }
    pub const fn flow_from(input: FlowFrom) -> Self {
        CssProperty::FlowFrom(CssPropertyValue::Exact(input))
    }
    pub const fn shape_outside(input: ShapeOutside) -> Self {
        CssProperty::ShapeOutside(CssPropertyValue::Exact(input))
    }
    pub const fn shape_inside(input: ShapeInside) -> Self {
        CssProperty::ShapeInside(CssPropertyValue::Exact(input))
    }
    pub const fn clip_path(input: ClipPath) -> Self {
        CssProperty::ClipPath(CssPropertyValue::Exact(input))
    }
    pub const fn shape_margin(input: ShapeMargin) -> Self {
        CssProperty::ShapeMargin(CssPropertyValue::Exact(input))
    }
    pub const fn shape_image_threshold(input: ShapeImageThreshold) -> Self {
        CssProperty::ShapeImageThreshold(CssPropertyValue::Exact(input))
    }
    pub const fn content(input: Content) -> Self {
        CssProperty::Content(CssPropertyValue::Exact(input))
    }
    pub const fn counter_reset(input: CounterReset) -> Self {
        CssProperty::CounterReset(CssPropertyValue::Exact(input))
    }
    pub const fn counter_increment(input: CounterIncrement) -> Self {
        CssProperty::CounterIncrement(CssPropertyValue::Exact(input))
    }
    pub const fn list_style_type(input: StyleListStyleType) -> Self {
        CssProperty::ListStyleType(CssPropertyValue::Exact(input))
    }
    pub const fn list_style_position(input: StyleListStylePosition) -> Self {
        CssProperty::ListStylePosition(CssPropertyValue::Exact(input))
    }
    pub const fn string_set(input: StringSet) -> Self {
        CssProperty::StringSet(CssPropertyValue::Exact(input))
    }
    pub const fn table_layout(input: LayoutTableLayout) -> Self {
        CssProperty::TableLayout(CssPropertyValue::Exact(input))
    }
    pub const fn border_collapse(input: StyleBorderCollapse) -> Self {
        CssProperty::BorderCollapse(CssPropertyValue::Exact(input))
    }
    pub const fn border_spacing(input: LayoutBorderSpacing) -> Self {
        CssProperty::BorderSpacing(CssPropertyValue::Exact(input))
    }
    pub const fn caption_side(input: StyleCaptionSide) -> Self {
        CssProperty::CaptionSide(CssPropertyValue::Exact(input))
    }
    pub const fn empty_cells(input: StyleEmptyCells) -> Self {
        CssProperty::EmptyCells(CssPropertyValue::Exact(input))
    }

    pub const fn as_z_index(&self) -> Option<&LayoutZIndexValue> {
        match self {
            CssProperty::ZIndex(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_flex_basis(&self) -> Option<&LayoutFlexBasisValue> {
        match self {
            CssProperty::FlexBasis(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_column_gap(&self) -> Option<&LayoutColumnGapValue> {
        match self {
            CssProperty::ColumnGap(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_row_gap(&self) -> Option<&LayoutRowGapValue> {
        match self {
            CssProperty::RowGap(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_grid_template_columns(&self) -> Option<&LayoutGridTemplateColumnsValue> {
        match self {
            CssProperty::GridTemplateColumns(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_grid_template_rows(&self) -> Option<&LayoutGridTemplateRowsValue> {
        match self {
            CssProperty::GridTemplateRows(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_grid_auto_columns(&self) -> Option<&LayoutGridAutoColumnsValue> {
        match self {
            CssProperty::GridAutoColumns(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_grid_auto_rows(&self) -> Option<&LayoutGridAutoRowsValue> {
        match self {
            CssProperty::GridAutoRows(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_grid_column(&self) -> Option<&LayoutGridColumnValue> {
        match self {
            CssProperty::GridColumn(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_grid_row(&self) -> Option<&LayoutGridRowValue> {
        match self {
            CssProperty::GridRow(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_writing_mode(&self) -> Option<&LayoutWritingModeValue> {
        match self {
            CssProperty::WritingMode(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_clear(&self) -> Option<&LayoutClearValue> {
        match self {
            CssProperty::Clear(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_layout_text_justify(&self) -> Option<&LayoutTextJustifyValue> {
        match self {
            CssProperty::LayoutTextJustify(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_scrollbar_track(&self) -> Option<&StyleBackgroundContentValue> {
        match self {
            CssProperty::ScrollbarTrack(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_scrollbar_thumb(&self) -> Option<&StyleBackgroundContentValue> {
        match self {
            CssProperty::ScrollbarThumb(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_scrollbar_button(&self) -> Option<&StyleBackgroundContentValue> {
        match self {
            CssProperty::ScrollbarButton(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_scrollbar_corner(&self) -> Option<&StyleBackgroundContentValue> {
        match self {
            CssProperty::ScrollbarCorner(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_scrollbar_resizer(&self) -> Option<&StyleBackgroundContentValue> {
        match self {
            CssProperty::ScrollbarResizer(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_visibility(&self) -> Option<&StyleVisibilityValue> {
        match self {
            CssProperty::Visibility(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_background_content(&self) -> Option<&StyleBackgroundContentVecValue> {
        match self {
            CssProperty::BackgroundContent(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_text_justify(&self) -> Option<&LayoutTextJustifyValue> {
        match self {
            CssProperty::TextJustify(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_caret_color(&self) -> Option<&CaretColorValue> {
        match self {
            CssProperty::CaretColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_caret_width(&self) -> Option<&CaretWidthValue> {
        match self {
            CssProperty::CaretWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_caret_animation_duration(&self) -> Option<&CaretAnimationDurationValue> {
        match self {
            CssProperty::CaretAnimationDuration(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_selection_background_color(&self) -> Option<&SelectionBackgroundColorValue> {
        match self {
            CssProperty::SelectionBackgroundColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_selection_color(&self) -> Option<&SelectionColorValue> {
        match self {
            CssProperty::SelectionColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_selection_radius(&self) -> Option<&SelectionRadiusValue> {
        match self {
            CssProperty::SelectionRadius(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_background_position(&self) -> Option<&StyleBackgroundPositionVecValue> {
        match self {
            CssProperty::BackgroundPosition(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_background_size(&self) -> Option<&StyleBackgroundSizeVecValue> {
        match self {
            CssProperty::BackgroundSize(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_background_repeat(&self) -> Option<&StyleBackgroundRepeatVecValue> {
        match self {
            CssProperty::BackgroundRepeat(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_grid_auto_flow(&self) -> Option<&LayoutGridAutoFlowValue> {
        match self {
            CssProperty::GridAutoFlow(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_justify_self(&self) -> Option<&LayoutJustifySelfValue> {
        match self {
            CssProperty::JustifySelf(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_justify_items(&self) -> Option<&LayoutJustifyItemsValue> {
        match self {
            CssProperty::JustifyItems(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_gap(&self) -> Option<&LayoutGapValue> {
        match self {
            CssProperty::Gap(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_grid_gap(&self) -> Option<&LayoutGapValue> {
        match self {
            CssProperty::GridGap(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_align_self(&self) -> Option<&LayoutAlignSelfValue> {
        match self {
            CssProperty::AlignSelf(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_font(&self) -> Option<&StyleFontValue> {
        match self {
            CssProperty::Font(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_font_size(&self) -> Option<&StyleFontSizeValue> {
        match self {
            CssProperty::FontSize(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_font_family(&self) -> Option<&StyleFontFamilyVecValue> {
        match self {
            CssProperty::FontFamily(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_font_weight(&self) -> Option<&StyleFontWeightValue> {
        match self {
            CssProperty::FontWeight(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_font_style(&self) -> Option<&StyleFontStyleValue> {
        match self {
            CssProperty::FontStyle(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_text_color(&self) -> Option<&StyleTextColorValue> {
        match self {
            CssProperty::TextColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_text_align(&self) -> Option<&StyleTextAlignValue> {
        match self {
            CssProperty::TextAlign(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_vertical_align(&self) -> Option<&StyleVerticalAlignValue> {
        match self {
            CssProperty::VerticalAlign(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_line_height(&self) -> Option<&StyleLineHeightValue> {
        match self {
            CssProperty::LineHeight(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_text_indent(&self) -> Option<&StyleTextIndentValue> {
        match self {
            CssProperty::TextIndent(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_initial_letter(&self) -> Option<&StyleInitialLetterValue> {
        match self {
            CssProperty::InitialLetter(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_line_clamp(&self) -> Option<&StyleLineClampValue> {
        match self {
            CssProperty::LineClamp(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_hanging_punctuation(&self) -> Option<&StyleHangingPunctuationValue> {
        match self {
            CssProperty::HangingPunctuation(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_text_combine_upright(&self) -> Option<&StyleTextCombineUprightValue> {
        match self {
            CssProperty::TextCombineUpright(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_exclusion_margin(&self) -> Option<&StyleExclusionMarginValue> {
        match self {
            CssProperty::ExclusionMargin(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_hyphenation_language(&self) -> Option<&StyleHyphenationLanguageValue> {
        match self {
            CssProperty::HyphenationLanguage(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_letter_spacing(&self) -> Option<&StyleLetterSpacingValue> {
        match self {
            CssProperty::LetterSpacing(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_word_spacing(&self) -> Option<&StyleWordSpacingValue> {
        match self {
            CssProperty::WordSpacing(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_tab_size(&self) -> Option<&StyleTabSizeValue> {
        match self {
            CssProperty::TabSize(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_cursor(&self) -> Option<&StyleCursorValue> {
        match self {
            CssProperty::Cursor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_box_shadow_left(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            CssProperty::BoxShadowLeft(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_box_shadow_right(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            CssProperty::BoxShadowRight(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_box_shadow_top(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            CssProperty::BoxShadowTop(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_box_shadow_bottom(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            CssProperty::BoxShadowBottom(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_top_color(&self) -> Option<&StyleBorderTopColorValue> {
        match self {
            CssProperty::BorderTopColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_left_color(&self) -> Option<&StyleBorderLeftColorValue> {
        match self {
            CssProperty::BorderLeftColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_right_color(&self) -> Option<&StyleBorderRightColorValue> {
        match self {
            CssProperty::BorderRightColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_bottom_color(&self) -> Option<&StyleBorderBottomColorValue> {
        match self {
            CssProperty::BorderBottomColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_top_style(&self) -> Option<&StyleBorderTopStyleValue> {
        match self {
            CssProperty::BorderTopStyle(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_left_style(&self) -> Option<&StyleBorderLeftStyleValue> {
        match self {
            CssProperty::BorderLeftStyle(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_right_style(&self) -> Option<&StyleBorderRightStyleValue> {
        match self {
            CssProperty::BorderRightStyle(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_bottom_style(&self) -> Option<&StyleBorderBottomStyleValue> {
        match self {
            CssProperty::BorderBottomStyle(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_top_left_radius(&self) -> Option<&StyleBorderTopLeftRadiusValue> {
        match self {
            CssProperty::BorderTopLeftRadius(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_top_right_radius(&self) -> Option<&StyleBorderTopRightRadiusValue> {
        match self {
            CssProperty::BorderTopRightRadius(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_bottom_left_radius(&self) -> Option<&StyleBorderBottomLeftRadiusValue> {
        match self {
            CssProperty::BorderBottomLeftRadius(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_bottom_right_radius(
        &self,
    ) -> Option<&StyleBorderBottomRightRadiusValue> {
        match self {
            CssProperty::BorderBottomRightRadius(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_opacity(&self) -> Option<&StyleOpacityValue> {
        match self {
            CssProperty::Opacity(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_transform(&self) -> Option<&StyleTransformVecValue> {
        match self {
            CssProperty::Transform(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_transform_origin(&self) -> Option<&StyleTransformOriginValue> {
        match self {
            CssProperty::TransformOrigin(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_perspective_origin(&self) -> Option<&StylePerspectiveOriginValue> {
        match self {
            CssProperty::PerspectiveOrigin(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_backface_visibility(&self) -> Option<&StyleBackfaceVisibilityValue> {
        match self {
            CssProperty::BackfaceVisibility(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_mix_blend_mode(&self) -> Option<&StyleMixBlendModeValue> {
        match self {
            CssProperty::MixBlendMode(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_filter(&self) -> Option<&StyleFilterVecValue> {
        match self {
            CssProperty::Filter(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_backdrop_filter(&self) -> Option<&StyleFilterVecValue> {
        match self {
            CssProperty::BackdropFilter(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_text_shadow(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            CssProperty::TextShadow(f) => Some(f),
            _ => None,
        }
    }

    // functions that downcast to the concrete CSS type (layout)

    pub const fn as_display(&self) -> Option<&LayoutDisplayValue> {
        match self {
            CssProperty::Display(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_float(&self) -> Option<&LayoutFloatValue> {
        match self {
            CssProperty::Float(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_box_sizing(&self) -> Option<&LayoutBoxSizingValue> {
        match self {
            CssProperty::BoxSizing(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_width(&self) -> Option<&LayoutWidthValue> {
        match self {
            CssProperty::Width(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_height(&self) -> Option<&LayoutHeightValue> {
        match self {
            CssProperty::Height(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_min_width(&self) -> Option<&LayoutMinWidthValue> {
        match self {
            CssProperty::MinWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_min_height(&self) -> Option<&LayoutMinHeightValue> {
        match self {
            CssProperty::MinHeight(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_max_width(&self) -> Option<&LayoutMaxWidthValue> {
        match self {
            CssProperty::MaxWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_max_height(&self) -> Option<&LayoutMaxHeightValue> {
        match self {
            CssProperty::MaxHeight(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_position(&self) -> Option<&LayoutPositionValue> {
        match self {
            CssProperty::Position(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_top(&self) -> Option<&LayoutTopValue> {
        match self {
            CssProperty::Top(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_bottom(&self) -> Option<&LayoutInsetBottomValue> {
        match self {
            CssProperty::Bottom(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_right(&self) -> Option<&LayoutRightValue> {
        match self {
            CssProperty::Right(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_left(&self) -> Option<&LayoutLeftValue> {
        match self {
            CssProperty::Left(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_padding_top(&self) -> Option<&LayoutPaddingTopValue> {
        match self {
            CssProperty::PaddingTop(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_padding_bottom(&self) -> Option<&LayoutPaddingBottomValue> {
        match self {
            CssProperty::PaddingBottom(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_padding_left(&self) -> Option<&LayoutPaddingLeftValue> {
        match self {
            CssProperty::PaddingLeft(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_padding_right(&self) -> Option<&LayoutPaddingRightValue> {
        match self {
            CssProperty::PaddingRight(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_margin_top(&self) -> Option<&LayoutMarginTopValue> {
        match self {
            CssProperty::MarginTop(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_margin_bottom(&self) -> Option<&LayoutMarginBottomValue> {
        match self {
            CssProperty::MarginBottom(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_margin_left(&self) -> Option<&LayoutMarginLeftValue> {
        match self {
            CssProperty::MarginLeft(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_margin_right(&self) -> Option<&LayoutMarginRightValue> {
        match self {
            CssProperty::MarginRight(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_top_width(&self) -> Option<&LayoutBorderTopWidthValue> {
        match self {
            CssProperty::BorderTopWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_left_width(&self) -> Option<&LayoutBorderLeftWidthValue> {
        match self {
            CssProperty::BorderLeftWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_right_width(&self) -> Option<&LayoutBorderRightWidthValue> {
        match self {
            CssProperty::BorderRightWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_bottom_width(&self) -> Option<&LayoutBorderBottomWidthValue> {
        match self {
            CssProperty::BorderBottomWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_overflow_x(&self) -> Option<&LayoutOverflowValue> {
        match self {
            CssProperty::OverflowX(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_overflow_y(&self) -> Option<&LayoutOverflowValue> {
        match self {
            CssProperty::OverflowY(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_flex_direction(&self) -> Option<&LayoutFlexDirectionValue> {
        match self {
            CssProperty::FlexDirection(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_direction(&self) -> Option<&StyleDirectionValue> {
        match self {
            CssProperty::Direction(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_user_select(&self) -> Option<&StyleUserSelectValue> {
        match self {
            CssProperty::UserSelect(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_text_decoration(&self) -> Option<&StyleTextDecorationValue> {
        match self {
            CssProperty::TextDecoration(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_hyphens(&self) -> Option<&StyleHyphensValue> {
        match self {
            CssProperty::Hyphens(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_white_space(&self) -> Option<&StyleWhiteSpaceValue> {
        match self {
            CssProperty::WhiteSpace(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_flex_wrap(&self) -> Option<&LayoutFlexWrapValue> {
        match self {
            CssProperty::FlexWrap(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_flex_grow(&self) -> Option<&LayoutFlexGrowValue> {
        match self {
            CssProperty::FlexGrow(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_flex_shrink(&self) -> Option<&LayoutFlexShrinkValue> {
        match self {
            CssProperty::FlexShrink(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_justify_content(&self) -> Option<&LayoutJustifyContentValue> {
        match self {
            CssProperty::JustifyContent(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_align_items(&self) -> Option<&LayoutAlignItemsValue> {
        match self {
            CssProperty::AlignItems(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_align_content(&self) -> Option<&LayoutAlignContentValue> {
        match self {
            CssProperty::AlignContent(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_break_before(&self) -> Option<&PageBreakValue> {
        match self {
            CssProperty::BreakBefore(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_break_after(&self) -> Option<&PageBreakValue> {
        match self {
            CssProperty::BreakAfter(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_break_inside(&self) -> Option<&BreakInsideValue> {
        match self {
            CssProperty::BreakInside(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_orphans(&self) -> Option<&OrphansValue> {
        match self {
            CssProperty::Orphans(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_widows(&self) -> Option<&WidowsValue> {
        match self {
            CssProperty::Widows(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_box_decoration_break(&self) -> Option<&BoxDecorationBreakValue> {
        match self {
            CssProperty::BoxDecorationBreak(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_column_count(&self) -> Option<&ColumnCountValue> {
        match self {
            CssProperty::ColumnCount(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_column_width(&self) -> Option<&ColumnWidthValue> {
        match self {
            CssProperty::ColumnWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_column_span(&self) -> Option<&ColumnSpanValue> {
        match self {
            CssProperty::ColumnSpan(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_column_fill(&self) -> Option<&ColumnFillValue> {
        match self {
            CssProperty::ColumnFill(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_column_rule_width(&self) -> Option<&ColumnRuleWidthValue> {
        match self {
            CssProperty::ColumnRuleWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_column_rule_style(&self) -> Option<&ColumnRuleStyleValue> {
        match self {
            CssProperty::ColumnRuleStyle(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_column_rule_color(&self) -> Option<&ColumnRuleColorValue> {
        match self {
            CssProperty::ColumnRuleColor(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_flow_into(&self) -> Option<&FlowIntoValue> {
        match self {
            CssProperty::FlowInto(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_flow_from(&self) -> Option<&FlowFromValue> {
        match self {
            CssProperty::FlowFrom(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_shape_outside(&self) -> Option<&ShapeOutsideValue> {
        match self {
            CssProperty::ShapeOutside(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_shape_inside(&self) -> Option<&ShapeInsideValue> {
        match self {
            CssProperty::ShapeInside(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_clip_path(&self) -> Option<&ClipPathValue> {
        match self {
            CssProperty::ClipPath(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_shape_margin(&self) -> Option<&ShapeMarginValue> {
        match self {
            CssProperty::ShapeMargin(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_shape_image_threshold(&self) -> Option<&ShapeImageThresholdValue> {
        match self {
            CssProperty::ShapeImageThreshold(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_content(&self) -> Option<&ContentValue> {
        match self {
            CssProperty::Content(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_counter_reset(&self) -> Option<&CounterResetValue> {
        match self {
            CssProperty::CounterReset(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_counter_increment(&self) -> Option<&CounterIncrementValue> {
        match self {
            CssProperty::CounterIncrement(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_list_style_type(&self) -> Option<&StyleListStyleTypeValue> {
        match self {
            CssProperty::ListStyleType(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_list_style_position(&self) -> Option<&StyleListStylePositionValue> {
        match self {
            CssProperty::ListStylePosition(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_string_set(&self) -> Option<&StringSetValue> {
        match self {
            CssProperty::StringSet(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_table_layout(&self) -> Option<&LayoutTableLayoutValue> {
        match self {
            CssProperty::TableLayout(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_collapse(&self) -> Option<&StyleBorderCollapseValue> {
        match self {
            CssProperty::BorderCollapse(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_border_spacing(&self) -> Option<&LayoutBorderSpacingValue> {
        match self {
            CssProperty::BorderSpacing(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_caption_side(&self) -> Option<&StyleCaptionSideValue> {
        match self {
            CssProperty::CaptionSide(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_empty_cells(&self) -> Option<&StyleEmptyCellsValue> {
        match self {
            CssProperty::EmptyCells(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_scrollbar_width(&self) -> Option<&LayoutScrollbarWidthValue> {
        match self {
            CssProperty::ScrollbarWidth(f) => Some(f),
            _ => None,
        }
    }
    pub const fn as_scrollbar_color(&self) -> Option<&StyleScrollbarColorValue> {
        match self {
            CssProperty::ScrollbarColor(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_scrollbar_visibility(&self) -> Option<&ScrollbarVisibilityModeValue> {
        match self {
            CssProperty::ScrollbarVisibility(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_scrollbar_fade_delay(&self) -> Option<&ScrollbarFadeDelayValue> {
        match self {
            CssProperty::ScrollbarFadeDelay(f) => Some(f),
            _ => None,
        }
    }

    pub const fn as_scrollbar_fade_duration(&self) -> Option<&ScrollbarFadeDurationValue> {
        match self {
            CssProperty::ScrollbarFadeDuration(f) => Some(f),
            _ => None,
        }
    }

    pub fn is_initial(&self) -> bool {
        use self::CssProperty::*;
        match self {
            CaretColor(c) => c.is_initial(),
            CaretWidth(c) => c.is_initial(),
            CaretAnimationDuration(c) => c.is_initial(),
            SelectionBackgroundColor(c) => c.is_initial(),
            SelectionColor(c) => c.is_initial(),
            SelectionRadius(c) => c.is_initial(),
            TextJustify(c) => c.is_initial(),
            LayoutTextJustify(_) => false,
            TextColor(c) => c.is_initial(),
            FontSize(c) => c.is_initial(),
            FontFamily(c) => c.is_initial(),
            TextAlign(c) => c.is_initial(),
            LetterSpacing(c) => c.is_initial(),
            TextIndent(c) => c.is_initial(),
            InitialLetter(c) => c.is_initial(),
            LineClamp(c) => c.is_initial(),
            HangingPunctuation(c) => c.is_initial(),
            TextCombineUpright(c) => c.is_initial(),
            ExclusionMargin(c) => c.is_initial(),
            HyphenationLanguage(c) => c.is_initial(),
            LineHeight(c) => c.is_initial(),
            WordSpacing(c) => c.is_initial(),
            TabSize(c) => c.is_initial(),
            Cursor(c) => c.is_initial(),
            Display(c) => c.is_initial(),
            Float(c) => c.is_initial(),
            BoxSizing(c) => c.is_initial(),
            Width(c) => c.is_initial(),
            Height(c) => c.is_initial(),
            MinWidth(c) => c.is_initial(),
            MinHeight(c) => c.is_initial(),
            MaxWidth(c) => c.is_initial(),
            MaxHeight(c) => c.is_initial(),
            Position(c) => c.is_initial(),
            Top(c) => c.is_initial(),
            Right(c) => c.is_initial(),
            Left(c) => c.is_initial(),
            Bottom(c) => c.is_initial(),
            ZIndex(c) => c.is_initial(),
            FlexWrap(c) => c.is_initial(),
            FlexDirection(c) => c.is_initial(),
            FlexGrow(c) => c.is_initial(),
            FlexShrink(c) => c.is_initial(),
            FlexBasis(c) => c.is_initial(),
            JustifyContent(c) => c.is_initial(),
            AlignItems(c) => c.is_initial(),
            AlignContent(c) => c.is_initial(),
            ColumnGap(c) => c.is_initial(),
            RowGap(c) => c.is_initial(),
            GridTemplateColumns(c) => c.is_initial(),
            GridTemplateRows(c) => c.is_initial(),
            GridAutoFlow(c) => c.is_initial(),
            JustifySelf(c) => c.is_initial(),
            JustifyItems(c) => c.is_initial(),
            Gap(c) => c.is_initial(),
            GridGap(c) => c.is_initial(),
            AlignSelf(c) => c.is_initial(),
            Font(c) => c.is_initial(),
            GridAutoColumns(c) => c.is_initial(),
            GridAutoRows(c) => c.is_initial(),
            GridColumn(c) => c.is_initial(),
            GridRow(c) => c.is_initial(),
            GridTemplateAreas(c) => c.is_initial(),
            WritingMode(c) => c.is_initial(),
            Clear(c) => c.is_initial(),
            BackgroundContent(c) => c.is_initial(),
            BackgroundPosition(c) => c.is_initial(),
            BackgroundSize(c) => c.is_initial(),
            BackgroundRepeat(c) => c.is_initial(),
            OverflowX(c) => c.is_initial(),
            OverflowY(c) => c.is_initial(),
            PaddingTop(c) => c.is_initial(),
            PaddingLeft(c) => c.is_initial(),
            PaddingRight(c) => c.is_initial(),
            PaddingBottom(c) => c.is_initial(),
            PaddingInlineStart(c) => c.is_initial(),
            PaddingInlineEnd(c) => c.is_initial(),
            MarginTop(c) => c.is_initial(),
            MarginLeft(c) => c.is_initial(),
            MarginRight(c) => c.is_initial(),
            MarginBottom(c) => c.is_initial(),
            BorderTopLeftRadius(c) => c.is_initial(),
            BorderTopRightRadius(c) => c.is_initial(),
            BorderBottomLeftRadius(c) => c.is_initial(),
            BorderBottomRightRadius(c) => c.is_initial(),
            BorderTopColor(c) => c.is_initial(),
            BorderRightColor(c) => c.is_initial(),
            BorderLeftColor(c) => c.is_initial(),
            BorderBottomColor(c) => c.is_initial(),
            BorderTopStyle(c) => c.is_initial(),
            BorderRightStyle(c) => c.is_initial(),
            BorderLeftStyle(c) => c.is_initial(),
            BorderBottomStyle(c) => c.is_initial(),
            BorderTopWidth(c) => c.is_initial(),
            BorderRightWidth(c) => c.is_initial(),
            BorderLeftWidth(c) => c.is_initial(),
            BorderBottomWidth(c) => c.is_initial(),
            BoxShadowLeft(c) => c.is_initial(),
            BoxShadowRight(c) => c.is_initial(),
            BoxShadowTop(c) => c.is_initial(),
            BoxShadowBottom(c) => c.is_initial(),
            ScrollbarTrack(c) => c.is_initial(),
            ScrollbarThumb(c) => c.is_initial(),
            ScrollbarButton(c) => c.is_initial(),
            ScrollbarCorner(c) => c.is_initial(),
            ScrollbarResizer(c) => c.is_initial(),
            ScrollbarWidth(c) => c.is_initial(),
            ScrollbarColor(c) => c.is_initial(),
            ScrollbarVisibility(c) => c.is_initial(),
            ScrollbarFadeDelay(c) => c.is_initial(),
            ScrollbarFadeDuration(c) => c.is_initial(),
            Opacity(c) => c.is_initial(),
            Visibility(c) => c.is_initial(),
            Transform(c) => c.is_initial(),
            TransformOrigin(c) => c.is_initial(),
            PerspectiveOrigin(c) => c.is_initial(),
            BackfaceVisibility(c) => c.is_initial(),
            MixBlendMode(c) => c.is_initial(),
            Filter(c) => c.is_initial(),
            BackdropFilter(c) => c.is_initial(),
            TextShadow(c) => c.is_initial(),
            WhiteSpace(c) => c.is_initial(),
            Direction(c) => c.is_initial(),
            UserSelect(c) => c.is_initial(),
            TextDecoration(c) => c.is_initial(),
            Hyphens(c) => c.is_initial(),
            BreakBefore(c) => c.is_initial(),
            BreakAfter(c) => c.is_initial(),
            BreakInside(c) => c.is_initial(),
            Orphans(c) => c.is_initial(),
            Widows(c) => c.is_initial(),
            BoxDecorationBreak(c) => c.is_initial(),
            ColumnCount(c) => c.is_initial(),
            ColumnWidth(c) => c.is_initial(),
            ColumnSpan(c) => c.is_initial(),
            ColumnFill(c) => c.is_initial(),
            ColumnRuleWidth(c) => c.is_initial(),
            ColumnRuleStyle(c) => c.is_initial(),
            ColumnRuleColor(c) => c.is_initial(),
            FlowInto(c) => c.is_initial(),
            FlowFrom(c) => c.is_initial(),
            ShapeOutside(c) => c.is_initial(),
            ShapeInside(c) => c.is_initial(),
            ClipPath(c) => c.is_initial(),
            ShapeMargin(c) => c.is_initial(),
            ShapeImageThreshold(c) => c.is_initial(),
            Content(c) => c.is_initial(),
            CounterReset(c) => c.is_initial(),
            CounterIncrement(c) => c.is_initial(),
            ListStyleType(c) => c.is_initial(),
            ListStylePosition(c) => c.is_initial(),
            StringSet(c) => c.is_initial(),
            TableLayout(c) => c.is_initial(),
            BorderCollapse(c) => c.is_initial(),
            BorderSpacing(c) => c.is_initial(),
            CaptionSide(c) => c.is_initial(),
            EmptyCells(c) => c.is_initial(),
            FontWeight(c) => c.is_initial(),
            FontStyle(c) => c.is_initial(),
            VerticalAlign(c) => c.is_initial(),
        }
    }

    pub const fn const_none(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, None)
    }
    pub const fn const_auto(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Auto)
    }
    pub const fn const_initial(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Initial)
    }
    pub const fn const_inherit(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Inherit)
    }

    pub const fn const_text_color(input: StyleTextColor) -> Self {
        CssProperty::TextColor(StyleTextColorValue::Exact(input))
    }
    pub const fn const_font_size(input: StyleFontSize) -> Self {
        CssProperty::FontSize(StyleFontSizeValue::Exact(input))
    }
    pub const fn const_font_family(input: StyleFontFamilyVec) -> Self {
        CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(input))
    }
    pub const fn const_text_align(input: StyleTextAlign) -> Self {
        CssProperty::TextAlign(StyleTextAlignValue::Exact(input))
    }
    pub const fn const_vertical_align(input: StyleVerticalAlign) -> Self {
        CssProperty::VerticalAlign(StyleVerticalAlignValue::Exact(input))
    }
    pub const fn const_letter_spacing(input: StyleLetterSpacing) -> Self {
        CssProperty::LetterSpacing(StyleLetterSpacingValue::Exact(input))
    }
    pub const fn const_text_indent(input: StyleTextIndent) -> Self {
        CssProperty::TextIndent(StyleTextIndentValue::Exact(input))
    }
    pub const fn const_line_height(input: StyleLineHeight) -> Self {
        CssProperty::LineHeight(StyleLineHeightValue::Exact(input))
    }
    pub const fn const_word_spacing(input: StyleWordSpacing) -> Self {
        CssProperty::WordSpacing(StyleWordSpacingValue::Exact(input))
    }
    pub const fn const_tab_size(input: StyleTabSize) -> Self {
        CssProperty::TabSize(StyleTabSizeValue::Exact(input))
    }
    pub const fn const_cursor(input: StyleCursor) -> Self {
        CssProperty::Cursor(StyleCursorValue::Exact(input))
    }
    pub const fn const_display(input: LayoutDisplay) -> Self {
        CssProperty::Display(LayoutDisplayValue::Exact(input))
    }
    pub const fn const_float(input: LayoutFloat) -> Self {
        CssProperty::Float(LayoutFloatValue::Exact(input))
    }
    pub const fn const_box_sizing(input: LayoutBoxSizing) -> Self {
        CssProperty::BoxSizing(LayoutBoxSizingValue::Exact(input))
    }
    pub const fn const_width(input: LayoutWidth) -> Self {
        CssProperty::Width(LayoutWidthValue::Exact(input))
    }
    pub const fn const_height(input: LayoutHeight) -> Self {
        CssProperty::Height(LayoutHeightValue::Exact(input))
    }
    pub const fn const_min_width(input: LayoutMinWidth) -> Self {
        CssProperty::MinWidth(LayoutMinWidthValue::Exact(input))
    }
    pub const fn const_min_height(input: LayoutMinHeight) -> Self {
        CssProperty::MinHeight(LayoutMinHeightValue::Exact(input))
    }
    pub const fn const_max_width(input: LayoutMaxWidth) -> Self {
        CssProperty::MaxWidth(LayoutMaxWidthValue::Exact(input))
    }
    pub const fn const_max_height(input: LayoutMaxHeight) -> Self {
        CssProperty::MaxHeight(LayoutMaxHeightValue::Exact(input))
    }
    pub const fn const_position(input: LayoutPosition) -> Self {
        CssProperty::Position(LayoutPositionValue::Exact(input))
    }
    pub const fn const_top(input: LayoutTop) -> Self {
        CssProperty::Top(LayoutTopValue::Exact(input))
    }
    pub const fn const_right(input: LayoutRight) -> Self {
        CssProperty::Right(LayoutRightValue::Exact(input))
    }
    pub const fn const_left(input: LayoutLeft) -> Self {
        CssProperty::Left(LayoutLeftValue::Exact(input))
    }
    pub const fn const_bottom(input: LayoutInsetBottom) -> Self {
        CssProperty::Bottom(LayoutInsetBottomValue::Exact(input))
    }
    pub const fn const_flex_wrap(input: LayoutFlexWrap) -> Self {
        CssProperty::FlexWrap(LayoutFlexWrapValue::Exact(input))
    }
    pub const fn const_flex_direction(input: LayoutFlexDirection) -> Self {
        CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(input))
    }
    pub const fn const_flex_grow(input: LayoutFlexGrow) -> Self {
        CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(input))
    }
    pub const fn const_flex_shrink(input: LayoutFlexShrink) -> Self {
        CssProperty::FlexShrink(LayoutFlexShrinkValue::Exact(input))
    }
    pub const fn const_justify_content(input: LayoutJustifyContent) -> Self {
        CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(input))
    }
    pub const fn const_align_items(input: LayoutAlignItems) -> Self {
        CssProperty::AlignItems(LayoutAlignItemsValue::Exact(input))
    }
    pub const fn const_align_content(input: LayoutAlignContent) -> Self {
        CssProperty::AlignContent(LayoutAlignContentValue::Exact(input))
    }
    pub const fn const_background_content(input: StyleBackgroundContentVec) -> Self {
        CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(input))
    }
    pub const fn const_background_position(input: StyleBackgroundPositionVec) -> Self {
        CssProperty::BackgroundPosition(StyleBackgroundPositionVecValue::Exact(input))
    }
    pub const fn const_background_size(input: StyleBackgroundSizeVec) -> Self {
        CssProperty::BackgroundSize(StyleBackgroundSizeVecValue::Exact(input))
    }
    pub const fn const_background_repeat(input: StyleBackgroundRepeatVec) -> Self {
        CssProperty::BackgroundRepeat(StyleBackgroundRepeatVecValue::Exact(input))
    }
    pub const fn const_overflow_x(input: LayoutOverflow) -> Self {
        CssProperty::OverflowX(LayoutOverflowValue::Exact(input))
    }
    pub const fn const_overflow_y(input: LayoutOverflow) -> Self {
        CssProperty::OverflowY(LayoutOverflowValue::Exact(input))
    }
    pub const fn const_padding_top(input: LayoutPaddingTop) -> Self {
        CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(input))
    }
    pub const fn const_padding_left(input: LayoutPaddingLeft) -> Self {
        CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(input))
    }
    pub const fn const_padding_right(input: LayoutPaddingRight) -> Self {
        CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(input))
    }
    pub const fn const_padding_bottom(input: LayoutPaddingBottom) -> Self {
        CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(input))
    }
    pub const fn const_margin_top(input: LayoutMarginTop) -> Self {
        CssProperty::MarginTop(LayoutMarginTopValue::Exact(input))
    }
    pub const fn const_margin_left(input: LayoutMarginLeft) -> Self {
        CssProperty::MarginLeft(LayoutMarginLeftValue::Exact(input))
    }
    pub const fn const_margin_right(input: LayoutMarginRight) -> Self {
        CssProperty::MarginRight(LayoutMarginRightValue::Exact(input))
    }
    pub const fn const_margin_bottom(input: LayoutMarginBottom) -> Self {
        CssProperty::MarginBottom(LayoutMarginBottomValue::Exact(input))
    }
    pub const fn const_border_top_left_radius(input: StyleBorderTopLeftRadius) -> Self {
        CssProperty::BorderTopLeftRadius(StyleBorderTopLeftRadiusValue::Exact(input))
    }
    pub const fn const_border_top_right_radius(input: StyleBorderTopRightRadius) -> Self {
        CssProperty::BorderTopRightRadius(StyleBorderTopRightRadiusValue::Exact(input))
    }
    pub const fn const_border_bottom_left_radius(input: StyleBorderBottomLeftRadius) -> Self {
        CssProperty::BorderBottomLeftRadius(StyleBorderBottomLeftRadiusValue::Exact(input))
    }
    pub const fn const_border_bottom_right_radius(input: StyleBorderBottomRightRadius) -> Self {
        CssProperty::BorderBottomRightRadius(StyleBorderBottomRightRadiusValue::Exact(input))
    }
    pub const fn const_border_top_color(input: StyleBorderTopColor) -> Self {
        CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(input))
    }
    pub const fn const_border_right_color(input: StyleBorderRightColor) -> Self {
        CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(input))
    }
    pub const fn const_border_left_color(input: StyleBorderLeftColor) -> Self {
        CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(input))
    }
    pub const fn const_border_bottom_color(input: StyleBorderBottomColor) -> Self {
        CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(input))
    }
    pub const fn const_border_top_style(input: StyleBorderTopStyle) -> Self {
        CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(input))
    }
    pub const fn const_border_right_style(input: StyleBorderRightStyle) -> Self {
        CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(input))
    }
    pub const fn const_border_left_style(input: StyleBorderLeftStyle) -> Self {
        CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(input))
    }
    pub const fn const_border_bottom_style(input: StyleBorderBottomStyle) -> Self {
        CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(input))
    }
    pub const fn const_border_top_width(input: LayoutBorderTopWidth) -> Self {
        CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(input))
    }
    pub const fn const_border_right_width(input: LayoutBorderRightWidth) -> Self {
        CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(input))
    }
    pub const fn const_border_left_width(input: LayoutBorderLeftWidth) -> Self {
        CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(input))
    }
    pub const fn const_border_bottom_width(input: LayoutBorderBottomWidth) -> Self {
        CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(input))
    }
    pub const fn const_box_shadow_left(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(input))
    }
    pub const fn const_box_shadow_right(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(input))
    }
    pub const fn const_box_shadow_top(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(input))
    }
    pub const fn const_box_shadow_bottom(input: StyleBoxShadow) -> Self {
        CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(input))
    }
    pub const fn const_opacity(input: StyleOpacity) -> Self {
        CssProperty::Opacity(StyleOpacityValue::Exact(input))
    }
    pub const fn const_transform(input: StyleTransformVec) -> Self {
        CssProperty::Transform(StyleTransformVecValue::Exact(input))
    }
    pub const fn const_transform_origin(input: StyleTransformOrigin) -> Self {
        CssProperty::TransformOrigin(StyleTransformOriginValue::Exact(input))
    }
    pub const fn const_perspective_origin(input: StylePerspectiveOrigin) -> Self {
        CssProperty::PerspectiveOrigin(StylePerspectiveOriginValue::Exact(input))
    }
    pub const fn const_backface_visiblity(input: StyleBackfaceVisibility) -> Self {
        CssProperty::BackfaceVisibility(StyleBackfaceVisibilityValue::Exact(input))
    }
    pub const fn const_break_before(input: PageBreak) -> Self {
        CssProperty::BreakBefore(PageBreakValue::Exact(input))
    }
    pub const fn const_break_after(input: PageBreak) -> Self {
        CssProperty::BreakAfter(PageBreakValue::Exact(input))
    }
    pub const fn const_break_inside(input: BreakInside) -> Self {
        CssProperty::BreakInside(BreakInsideValue::Exact(input))
    }
    pub const fn const_orphans(input: Orphans) -> Self {
        CssProperty::Orphans(OrphansValue::Exact(input))
    }
    pub const fn const_widows(input: Widows) -> Self {
        CssProperty::Widows(WidowsValue::Exact(input))
    }
    pub const fn const_box_decoration_break(input: BoxDecorationBreak) -> Self {
        CssProperty::BoxDecorationBreak(BoxDecorationBreakValue::Exact(input))
    }
    pub const fn const_column_count(input: ColumnCount) -> Self {
        CssProperty::ColumnCount(ColumnCountValue::Exact(input))
    }
    pub const fn const_column_width(input: ColumnWidth) -> Self {
        CssProperty::ColumnWidth(ColumnWidthValue::Exact(input))
    }
    pub const fn const_column_span(input: ColumnSpan) -> Self {
        CssProperty::ColumnSpan(ColumnSpanValue::Exact(input))
    }
    pub const fn const_column_fill(input: ColumnFill) -> Self {
        CssProperty::ColumnFill(ColumnFillValue::Exact(input))
    }
    pub const fn const_column_rule_width(input: ColumnRuleWidth) -> Self {
        CssProperty::ColumnRuleWidth(ColumnRuleWidthValue::Exact(input))
    }
    pub const fn const_column_rule_style(input: ColumnRuleStyle) -> Self {
        CssProperty::ColumnRuleStyle(ColumnRuleStyleValue::Exact(input))
    }
    pub const fn const_column_rule_color(input: ColumnRuleColor) -> Self {
        CssProperty::ColumnRuleColor(ColumnRuleColorValue::Exact(input))
    }
    pub const fn const_flow_into(input: FlowInto) -> Self {
        CssProperty::FlowInto(FlowIntoValue::Exact(input))
    }
    pub const fn const_flow_from(input: FlowFrom) -> Self {
        CssProperty::FlowFrom(FlowFromValue::Exact(input))
    }
    pub const fn const_shape_outside(input: ShapeOutside) -> Self {
        CssProperty::ShapeOutside(ShapeOutsideValue::Exact(input))
    }
    pub const fn const_shape_inside(input: ShapeInside) -> Self {
        CssProperty::ShapeInside(ShapeInsideValue::Exact(input))
    }
    pub const fn const_clip_path(input: ClipPath) -> Self {
        CssProperty::ClipPath(ClipPathValue::Exact(input))
    }
    pub const fn const_shape_margin(input: ShapeMargin) -> Self {
        CssProperty::ShapeMargin(ShapeMarginValue::Exact(input))
    }
    pub const fn const_shape_image_threshold(input: ShapeImageThreshold) -> Self {
        CssProperty::ShapeImageThreshold(ShapeImageThresholdValue::Exact(input))
    }
    pub const fn const_content(input: Content) -> Self {
        CssProperty::Content(ContentValue::Exact(input))
    }
    pub const fn const_counter_reset(input: CounterReset) -> Self {
        CssProperty::CounterReset(CounterResetValue::Exact(input))
    }
    pub const fn const_counter_increment(input: CounterIncrement) -> Self {
        CssProperty::CounterIncrement(CounterIncrementValue::Exact(input))
    }
    pub const fn const_list_style_type(input: StyleListStyleType) -> Self {
        CssProperty::ListStyleType(StyleListStyleTypeValue::Exact(input))
    }
    pub const fn const_list_style_position(input: StyleListStylePosition) -> Self {
        CssProperty::ListStylePosition(StyleListStylePositionValue::Exact(input))
    }
    pub const fn const_string_set(input: StringSet) -> Self {
        CssProperty::StringSet(StringSetValue::Exact(input))
    }
    pub const fn const_table_layout(input: LayoutTableLayout) -> Self {
        CssProperty::TableLayout(LayoutTableLayoutValue::Exact(input))
    }
    pub const fn const_border_collapse(input: StyleBorderCollapse) -> Self {
        CssProperty::BorderCollapse(StyleBorderCollapseValue::Exact(input))
    }
    pub const fn const_border_spacing(input: LayoutBorderSpacing) -> Self {
        CssProperty::BorderSpacing(LayoutBorderSpacingValue::Exact(input))
    }
    pub const fn const_caption_side(input: StyleCaptionSide) -> Self {
        CssProperty::CaptionSide(StyleCaptionSideValue::Exact(input))
    }
    pub const fn const_empty_cells(input: StyleEmptyCells) -> Self {
        CssProperty::EmptyCells(StyleEmptyCellsValue::Exact(input))
    }
}

pub fn format_static_css_prop(prop: &CssProperty, tabs: usize) -> String {
    match prop {
        CssProperty::CaretColor(p) => format!(
            "CssProperty::CaretColor({})",
            print_css_property_value(p, tabs, "CaretColor")
        ),
        CssProperty::CaretWidth(p) => format!(
            "CssProperty::CaretWidth({})",
            print_css_property_value(p, tabs, "CaretWidth")
        ),
        CssProperty::CaretAnimationDuration(p) => format!(
            "CssProperty::CaretAnimationDuration({})",
            print_css_property_value(p, tabs, "CaretAnimationDuration")
        ),
        CssProperty::SelectionBackgroundColor(p) => format!(
            "CssProperty::SelectionBackgroundColor({})",
            print_css_property_value(p, tabs, "SelectionBackgroundColor")
        ),
        CssProperty::SelectionColor(p) => format!(
            "CssProperty::SelectionColor({})",
            print_css_property_value(p, tabs, "SelectionColor")
        ),
        CssProperty::SelectionRadius(p) => format!(
            "CssProperty::SelectionRadius({})",
            print_css_property_value(p, tabs, "SelectionRadius")
        ),
        CssProperty::TextJustify(p) => format!(
            "CssProperty::TextJustify({})",
            print_css_property_value(p, tabs, "LayoutTextJustify")
        ),
        CssProperty::LayoutTextJustify(j) => format!(
            "CssProperty::LayoutTextJustify({})",
            print_css_property_value(j, tabs, "LayoutText")
        ),
        CssProperty::TextColor(p) => format!(
            "CssProperty::TextColor({})",
            print_css_property_value(p, tabs, "StyleTextColor")
        ),
        CssProperty::FontSize(p) => format!(
            "CssProperty::FontSize({})",
            print_css_property_value(p, tabs, "StyleFontSize")
        ),
        CssProperty::FontFamily(p) => format!(
            "CssProperty::FontFamily({})",
            print_css_property_value(p, tabs, "StyleFontFamilyVec")
        ),
        CssProperty::TextAlign(p) => format!(
            "CssProperty::TextAlign({})",
            print_css_property_value(p, tabs, "StyleTextAlign")
        ),
        CssProperty::VerticalAlign(p) => format!(
            "CssProperty::VerticalAlign({})",
            print_css_property_value(p, tabs, "StyleVerticalAlign")
        ),
        CssProperty::LetterSpacing(p) => format!(
            "CssProperty::LetterSpacing({})",
            print_css_property_value(p, tabs, "StyleLetterSpacing")
        ),
        CssProperty::TextIndent(p) => format!(
            "CssProperty::TextIndent({})",
            print_css_property_value(p, tabs, "StyleTextIndent")
        ),
        CssProperty::InitialLetter(p) => format!(
            "CssProperty::InitialLetter({})",
            print_css_property_value(p, tabs, "StyleInitialLetter")
        ),
        CssProperty::LineClamp(p) => format!(
            "CssProperty::LineClamp({})",
            print_css_property_value(p, tabs, "StyleLineClamp")
        ),
        CssProperty::HangingPunctuation(p) => format!(
            "CssProperty::HangingPunctuation({})",
            print_css_property_value(p, tabs, "StyleHangingPunctuation")
        ),
        CssProperty::TextCombineUpright(p) => format!(
            "CssProperty::TextCombineUpright({})",
            print_css_property_value(p, tabs, "StyleTextCombineUpright")
        ),
        CssProperty::ExclusionMargin(p) => format!(
            "CssProperty::ExclusionMargin({})",
            print_css_property_value(p, tabs, "StyleExclusionMargin")
        ),
        CssProperty::HyphenationLanguage(p) => format!(
            "CssProperty::HyphenationLanguage({})",
            print_css_property_value(p, tabs, "StyleHyphenationLanguage")
        ),
        CssProperty::LineHeight(p) => format!(
            "CssProperty::LineHeight({})",
            print_css_property_value(p, tabs, "StyleLineHeight")
        ),
        CssProperty::WordSpacing(p) => format!(
            "CssProperty::WordSpacing({})",
            print_css_property_value(p, tabs, "StyleWordSpacing")
        ),
        CssProperty::TabSize(p) => format!(
            "CssProperty::TabSize({})",
            print_css_property_value(p, tabs, "StyleTabSize")
        ),
        CssProperty::Cursor(p) => format!(
            "CssProperty::Cursor({})",
            print_css_property_value(p, tabs, "StyleCursor")
        ),
        CssProperty::Display(p) => format!(
            "CssProperty::Display({})",
            print_css_property_value(p, tabs, "LayoutDisplay")
        ),
        CssProperty::Float(p) => format!(
            "CssProperty::Float({})",
            print_css_property_value(p, tabs, "LayoutFloat")
        ),
        CssProperty::BoxSizing(p) => format!(
            "CssProperty::BoxSizing({})",
            print_css_property_value(p, tabs, "LayoutBoxSizing")
        ),
        CssProperty::Width(p) => format!(
            "CssProperty::Width({})",
            print_css_property_value(p, tabs, "LayoutWidth")
        ),
        CssProperty::Height(p) => format!(
            "CssProperty::Height({})",
            print_css_property_value(p, tabs, "LayoutHeight")
        ),
        CssProperty::MinWidth(p) => format!(
            "CssProperty::MinWidth({})",
            print_css_property_value(p, tabs, "LayoutMinWidth")
        ),
        CssProperty::MinHeight(p) => format!(
            "CssProperty::MinHeight({})",
            print_css_property_value(p, tabs, "LayoutMinHeight")
        ),
        CssProperty::MaxWidth(p) => format!(
            "CssProperty::MaxWidth({})",
            print_css_property_value(p, tabs, "LayoutMaxWidth")
        ),
        CssProperty::MaxHeight(p) => format!(
            "CssProperty::MaxHeight({})",
            print_css_property_value(p, tabs, "LayoutMaxHeight")
        ),
        CssProperty::Position(p) => format!(
            "CssProperty::Position({})",
            print_css_property_value(p, tabs, "LayoutPosition")
        ),
        CssProperty::Top(p) => format!(
            "CssProperty::Top({})",
            print_css_property_value(p, tabs, "LayoutTop")
        ),
        CssProperty::Right(p) => format!(
            "CssProperty::Right({})",
            print_css_property_value(p, tabs, "LayoutRight")
        ),
        CssProperty::Left(p) => format!(
            "CssProperty::Left({})",
            print_css_property_value(p, tabs, "LayoutLeft")
        ),
        CssProperty::Bottom(p) => format!(
            "CssProperty::Bottom({})",
            print_css_property_value(p, tabs, "LayoutInsetBottom")
        ),
        CssProperty::ZIndex(p) => format!(
            "CssProperty::ZIndex({})",
            print_css_property_value(p, tabs, "LayoutZIndex")
        ),
        CssProperty::FlexWrap(p) => format!(
            "CssProperty::FlexWrap({})",
            print_css_property_value(p, tabs, "LayoutFlexWrap")
        ),
        CssProperty::FlexDirection(p) => format!(
            "CssProperty::FlexDirection({})",
            print_css_property_value(p, tabs, "LayoutFlexDirection")
        ),
        CssProperty::FlexGrow(p) => format!(
            "CssProperty::FlexGrow({})",
            print_css_property_value(p, tabs, "LayoutFlexGrow")
        ),
        CssProperty::FlexShrink(p) => format!(
            "CssProperty::FlexShrink({})",
            print_css_property_value(p, tabs, "LayoutFlexShrink")
        ),
        CssProperty::JustifyContent(p) => format!(
            "CssProperty::JustifyContent({})",
            print_css_property_value(p, tabs, "LayoutJustifyContent")
        ),
        CssProperty::AlignItems(p) => format!(
            "CssProperty::AlignItems({})",
            print_css_property_value(p, tabs, "LayoutAlignItems")
        ),
        CssProperty::AlignContent(p) => format!(
            "CssProperty::AlignContent({})",
            print_css_property_value(p, tabs, "LayoutAlignContent")
        ),
        CssProperty::BackgroundContent(p) => format!(
            "CssProperty::BackgroundContent({})",
            print_css_property_value(p, tabs, "StyleBackgroundContentVec")
        ),
        CssProperty::BackgroundPosition(p) => format!(
            "CssProperty::BackgroundPosition({})",
            print_css_property_value(p, tabs, "StyleBackgroundPositionVec")
        ),
        CssProperty::BackgroundSize(p) => format!(
            "CssProperty::BackgroundSize({})",
            print_css_property_value(p, tabs, "StyleBackgroundSizeVec")
        ),
        CssProperty::BackgroundRepeat(p) => format!(
            "CssProperty::BackgroundRepeat({})",
            print_css_property_value(p, tabs, "StyleBackgroundRepeatVec")
        ),
        CssProperty::OverflowX(p) => format!(
            "CssProperty::OverflowX({})",
            print_css_property_value(p, tabs, "LayoutOverflow")
        ),
        CssProperty::OverflowY(p) => format!(
            "CssProperty::OverflowY({})",
            print_css_property_value(p, tabs, "LayoutOverflow")
        ),
        CssProperty::PaddingTop(p) => format!(
            "CssProperty::PaddingTop({})",
            print_css_property_value(p, tabs, "LayoutPaddingTop")
        ),
        CssProperty::PaddingLeft(p) => format!(
            "CssProperty::PaddingLeft({})",
            print_css_property_value(p, tabs, "LayoutPaddingLeft")
        ),
        CssProperty::PaddingRight(p) => format!(
            "CssProperty::PaddingRight({})",
            print_css_property_value(p, tabs, "LayoutPaddingRight")
        ),
        CssProperty::PaddingBottom(p) => format!(
            "CssProperty::PaddingBottom({})",
            print_css_property_value(p, tabs, "LayoutPaddingBottom")
        ),
        CssProperty::PaddingInlineStart(p) => format!(
            "CssProperty::PaddingInlineStart({})",
            print_css_property_value(p, tabs, "LayoutPaddingInlineStart")
        ),
        CssProperty::PaddingInlineEnd(p) => format!(
            "CssProperty::PaddingInlineEnd({})",
            print_css_property_value(p, tabs, "LayoutPaddingInlineEnd")
        ),
        CssProperty::MarginTop(p) => format!(
            "CssProperty::MarginTop({})",
            print_css_property_value(p, tabs, "LayoutMarginTop")
        ),
        CssProperty::MarginLeft(p) => format!(
            "CssProperty::MarginLeft({})",
            print_css_property_value(p, tabs, "LayoutMarginLeft")
        ),
        CssProperty::MarginRight(p) => format!(
            "CssProperty::MarginRight({})",
            print_css_property_value(p, tabs, "LayoutMarginRight")
        ),
        CssProperty::MarginBottom(p) => format!(
            "CssProperty::MarginBottom({})",
            print_css_property_value(p, tabs, "LayoutMarginBottom")
        ),
        CssProperty::BorderTopLeftRadius(p) => format!(
            "CssProperty::BorderTopLeftRadius({})",
            print_css_property_value(p, tabs, "StyleBorderTopLeftRadius")
        ),
        CssProperty::BorderTopRightRadius(p) => format!(
            "CssProperty::BorderTopRightRadius({})",
            print_css_property_value(p, tabs, "StyleBorderTopRightRadius")
        ),
        CssProperty::BorderBottomLeftRadius(p) => format!(
            "CssProperty::BorderBottomLeftRadius({})",
            print_css_property_value(p, tabs, "StyleBorderBottomLeftRadius")
        ),
        CssProperty::BorderBottomRightRadius(p) => format!(
            "CssProperty::BorderBottomRightRadius({})",
            print_css_property_value(p, tabs, "StyleBorderBottomRightRadius")
        ),
        CssProperty::BorderTopColor(p) => format!(
            "CssProperty::BorderTopColor({})",
            print_css_property_value(p, tabs, "StyleBorderTopColor")
        ),
        CssProperty::BorderRightColor(p) => format!(
            "CssProperty::BorderRightColor({})",
            print_css_property_value(p, tabs, "StyleBorderRightColor")
        ),
        CssProperty::BorderLeftColor(p) => format!(
            "CssProperty::BorderLeftColor({})",
            print_css_property_value(p, tabs, "StyleBorderLeftColor")
        ),
        CssProperty::BorderBottomColor(p) => format!(
            "CssProperty::BorderBottomColor({})",
            print_css_property_value(p, tabs, "StyleBorderBottomColor")
        ),
        CssProperty::BorderTopStyle(p) => format!(
            "CssProperty::BorderTopStyle({})",
            print_css_property_value(p, tabs, "StyleBorderTopStyle")
        ),
        CssProperty::BorderRightStyle(p) => format!(
            "CssProperty::BorderRightStyle({})",
            print_css_property_value(p, tabs, "StyleBorderRightStyle")
        ),
        CssProperty::BorderLeftStyle(p) => format!(
            "CssProperty::BorderLeftStyle({})",
            print_css_property_value(p, tabs, "StyleBorderLeftStyle")
        ),
        CssProperty::BorderBottomStyle(p) => format!(
            "CssProperty::BorderBottomStyle({})",
            print_css_property_value(p, tabs, "StyleBorderBottomStyle")
        ),
        CssProperty::BorderTopWidth(p) => format!(
            "CssProperty::BorderTopWidth({})",
            print_css_property_value(p, tabs, "LayoutBorderTopWidth")
        ),
        CssProperty::BorderRightWidth(p) => format!(
            "CssProperty::BorderRightWidth({})",
            print_css_property_value(p, tabs, "LayoutBorderRightWidth")
        ),
        CssProperty::BorderLeftWidth(p) => format!(
            "CssProperty::BorderLeftWidth({})",
            print_css_property_value(p, tabs, "LayoutBorderLeftWidth")
        ),
        CssProperty::BorderBottomWidth(p) => format!(
            "CssProperty::BorderBottomWidth({})",
            print_css_property_value(p, tabs, "LayoutBorderBottomWidth")
        ),
        CssProperty::BoxShadowLeft(p) => format!(
            "CssProperty::BoxShadowLeft({})",
            print_css_property_value(p, tabs, "StyleBoxShadow")
        ),
        CssProperty::BoxShadowRight(p) => format!(
            "CssProperty::BoxShadowRight({})",
            print_css_property_value(p, tabs, "StyleBoxShadow")
        ),
        CssProperty::BoxShadowTop(p) => format!(
            "CssProperty::BoxShadowTop({})",
            print_css_property_value(p, tabs, "StyleBoxShadow")
        ),
        CssProperty::BoxShadowBottom(p) => format!(
            "CssProperty::BoxShadowBottom({})",
            print_css_property_value(p, tabs, "StyleBoxShadow")
        ),
        CssProperty::ScrollbarWidth(p) => format!(
            "CssProperty::ScrollbarWidth({})",
            print_css_property_value(p, tabs, "LayoutScrollbarWidth")
        ),
        CssProperty::ScrollbarColor(p) => format!(
            "CssProperty::ScrollbarColor({})",
            print_css_property_value(p, tabs, "StyleScrollbarColor")
        ),
        CssProperty::ScrollbarVisibility(p) => format!(
            "CssProperty::ScrollbarVisibility({})",
            print_css_property_value(p, tabs, "ScrollbarVisibilityMode")
        ),
        CssProperty::ScrollbarFadeDelay(p) => format!(
            "CssProperty::ScrollbarFadeDelay({})",
            print_css_property_value(p, tabs, "ScrollbarFadeDelay")
        ),
        CssProperty::ScrollbarFadeDuration(p) => format!(
            "CssProperty::ScrollbarFadeDuration({})",
            print_css_property_value(p, tabs, "ScrollbarFadeDuration")
        ),
        CssProperty::ScrollbarTrack(p) => format!(
            "CssProperty::ScrollbarTrack({})",
            print_css_property_value(p, tabs, "StyleBackgroundContent")
        ),
        CssProperty::ScrollbarThumb(p) => format!(
            "CssProperty::ScrollbarThumb({})",
            print_css_property_value(p, tabs, "StyleBackgroundContent")
        ),
        CssProperty::ScrollbarButton(p) => format!(
            "CssProperty::ScrollbarButton({})",
            print_css_property_value(p, tabs, "StyleBackgroundContent")
        ),
        CssProperty::ScrollbarCorner(p) => format!(
            "CssProperty::ScrollbarCorner({})",
            print_css_property_value(p, tabs, "StyleBackgroundContent")
        ),
        CssProperty::ScrollbarResizer(p) => format!(
            "CssProperty::ScrollbarResizer({})",
            print_css_property_value(p, tabs, "StyleBackgroundContent")
        ),
        CssProperty::Opacity(p) => format!(
            "CssProperty::Opacity({})",
            print_css_property_value(p, tabs, "StyleOpacity")
        ),
        CssProperty::Visibility(p) => format!(
            "CssProperty::Visibility({})",
            print_css_property_value(p, tabs, "StyleVisibility")
        ),
        CssProperty::Transform(p) => format!(
            "CssProperty::Transform({})",
            print_css_property_value(p, tabs, "StyleTransformVec")
        ),
        CssProperty::TransformOrigin(p) => format!(
            "CssProperty::TransformOrigin({})",
            print_css_property_value(p, tabs, "StyleTransformOrigin")
        ),
        CssProperty::PerspectiveOrigin(p) => format!(
            "CssProperty::PerspectiveOrigin({})",
            print_css_property_value(p, tabs, "StylePerspectiveOrigin")
        ),
        CssProperty::BackfaceVisibility(p) => format!(
            "CssProperty::BackfaceVisibility({})",
            print_css_property_value(p, tabs, "StyleBackfaceVisibility")
        ),
        CssProperty::MixBlendMode(p) => format!(
            "CssProperty::MixBlendMode({})",
            print_css_property_value(p, tabs, "StyleMixBlendMode")
        ),
        CssProperty::Filter(p) => format!(
            "CssProperty::Filter({})",
            print_css_property_value(p, tabs, "StyleFilterVec")
        ),
        CssProperty::BackdropFilter(p) => format!(
            "CssProperty::Filter({})",
            print_css_property_value(p, tabs, "StyleFilterVec")
        ),
        CssProperty::TextShadow(p) => format!(
            "CssProperty::TextShadow({})",
            print_css_property_value(p, tabs, "StyleBoxShadow")
        ),
        CssProperty::Hyphens(p) => format!(
            "CssProperty::Hyphens({})",
            print_css_property_value(p, tabs, "StyleHyphens")
        ),
        CssProperty::Direction(p) => format!(
            "CssProperty::Direction({})",
            print_css_property_value(p, tabs, "Direction")
        ),
        CssProperty::UserSelect(p) => format!(
            "CssProperty::UserSelect({})",
            print_css_property_value(p, tabs, "StyleUserSelect")
        ),
        CssProperty::TextDecoration(p) => format!(
            "CssProperty::TextDecoration({})",
            print_css_property_value(p, tabs, "StyleTextDecoration")
        ),
        CssProperty::WhiteSpace(p) => format!(
            "CssProperty::WhiteSpace({})",
            print_css_property_value(p, tabs, "WhiteSpace")
        ),
        CssProperty::FlexBasis(p) => format!(
            "CssProperty::FlexBasis({})",
            print_css_property_value(p, tabs, "LayoutFlexBasis")
        ),
        CssProperty::ColumnGap(p) => format!(
            "CssProperty::ColumnGap({})",
            print_css_property_value(p, tabs, "LayoutColumnGap")
        ),
        CssProperty::RowGap(p) => format!(
            "CssProperty::RowGap({})",
            print_css_property_value(p, tabs, "LayoutRowGap")
        ),
        CssProperty::GridTemplateColumns(p) => format!(
            "CssProperty::GridTemplateColumns({})",
            print_css_property_value(p, tabs, "LayoutGridTemplateColumns")
        ),
        CssProperty::GridTemplateRows(p) => format!(
            "CssProperty::GridTemplateRows({})",
            print_css_property_value(p, tabs, "LayoutGridTemplateRows")
        ),
        CssProperty::GridAutoFlow(p) => format!(
            "CssProperty::GridAutoFlow({})",
            print_css_property_value(p, tabs, "LayoutGridAutoFlow")
        ),
        CssProperty::JustifySelf(p) => format!(
            "CssProperty::JustifySelf({})",
            print_css_property_value(p, tabs, "LayoutJustifySelf")
        ),
        CssProperty::JustifyItems(p) => format!(
            "CssProperty::JustifyItems({})",
            print_css_property_value(p, tabs, "LayoutJustifyItems")
        ),
        CssProperty::Gap(p) => format!(
            "CssProperty::Gap({})",
            print_css_property_value(p, tabs, "LayoutGap")
        ),
        CssProperty::GridGap(p) => format!(
            "CssProperty::GridGap({})",
            print_css_property_value(p, tabs, "LayoutGap")
        ),
        CssProperty::AlignSelf(p) => format!(
            "CssProperty::AlignSelf({})",
            print_css_property_value(p, tabs, "LayoutAlignSelf")
        ),
        CssProperty::Font(p) => format!(
            "CssProperty::Font({})",
            print_css_property_value(p, tabs, "StyleFontFamilyVec")
        ),
        CssProperty::GridAutoRows(p) => format!(
            "CssProperty::GridAutoRows({})",
            print_css_property_value(p, tabs, "LayoutGridAutoRows")
        ),
        CssProperty::GridAutoColumns(p) => format!(
            "CssProperty::GridAutoColumns({})",
            print_css_property_value(p, tabs, "LayoutGridAutoColumns")
        ),
        CssProperty::GridRow(p) => format!(
            "CssProperty::GridRow({})",
            print_css_property_value(p, tabs, "LayoutGridRow")
        ),
        CssProperty::GridColumn(p) => format!(
            "CssProperty::GridColumn({})",
            print_css_property_value(p, tabs, "LayoutGridColumn")
        ),
        CssProperty::GridTemplateAreas(p) => format!(
            "CssProperty::GridTemplateAreas({})",
            print_css_property_value(p, tabs, "GridTemplateAreas")
        ),
        CssProperty::WritingMode(p) => format!(
            "CssProperty::WritingMode({})",
            print_css_property_value(p, tabs, "LayoutWritingMode")
        ),
        CssProperty::Clear(p) => format!(
            "CssProperty::Clear({})",
            print_css_property_value(p, tabs, "LayoutClear")
        ),
        CssProperty::BreakBefore(p) => format!(
            "CssProperty::BreakBefore({})",
            print_css_property_value(p, tabs, "PageBreak")
        ),
        CssProperty::BreakAfter(p) => format!(
            "CssProperty::BreakAfter({})",
            print_css_property_value(p, tabs, "PageBreak")
        ),
        CssProperty::BreakInside(p) => format!(
            "CssProperty::BreakInside({})",
            print_css_property_value(p, tabs, "BreakInside")
        ),
        CssProperty::Orphans(p) => format!(
            "CssProperty::Orphans({})",
            print_css_property_value(p, tabs, "Orphans")
        ),
        CssProperty::Widows(p) => format!(
            "CssProperty::Widows({})",
            print_css_property_value(p, tabs, "Widows")
        ),
        CssProperty::BoxDecorationBreak(p) => format!(
            "CssProperty::BoxDecorationBreak({})",
            print_css_property_value(p, tabs, "BoxDecorationBreak")
        ),
        CssProperty::ColumnCount(p) => format!(
            "CssProperty::ColumnCount({})",
            print_css_property_value(p, tabs, "ColumnCount")
        ),
        CssProperty::ColumnWidth(p) => format!(
            "CssProperty::ColumnWidth({})",
            print_css_property_value(p, tabs, "ColumnWidth")
        ),
        CssProperty::ColumnSpan(p) => format!(
            "CssProperty::ColumnSpan({})",
            print_css_property_value(p, tabs, "ColumnSpan")
        ),
        CssProperty::ColumnFill(p) => format!(
            "CssProperty::ColumnFill({})",
            print_css_property_value(p, tabs, "ColumnFill")
        ),
        CssProperty::ColumnRuleWidth(p) => format!(
            "CssProperty::ColumnRuleWidth({})",
            print_css_property_value(p, tabs, "ColumnRuleWidth")
        ),
        CssProperty::ColumnRuleStyle(p) => format!(
            "CssProperty::ColumnRuleStyle({})",
            print_css_property_value(p, tabs, "ColumnRuleStyle")
        ),
        CssProperty::ColumnRuleColor(p) => format!(
            "CssProperty::ColumnRuleColor({})",
            print_css_property_value(p, tabs, "ColumnRuleColor")
        ),
        CssProperty::FlowInto(p) => format!(
            "CssProperty::FlowInto({})",
            print_css_property_value(p, tabs, "FlowInto")
        ),
        CssProperty::FlowFrom(p) => format!(
            "CssProperty::FlowFrom({})",
            print_css_property_value(p, tabs, "FlowFrom")
        ),
        CssProperty::ShapeOutside(p) => format!(
            "CssProperty::ShapeOutside({})",
            print_css_property_value(p, tabs, "ShapeOutside")
        ),
        CssProperty::ShapeInside(p) => format!(
            "CssProperty::ShapeInside({})",
            print_css_property_value(p, tabs, "ShapeInside")
        ),
        CssProperty::ClipPath(p) => format!(
            "CssProperty::ClipPath({})",
            print_css_property_value(p, tabs, "ClipPath")
        ),
        CssProperty::ShapeMargin(p) => format!(
            "CssProperty::ShapeMargin({})",
            print_css_property_value(p, tabs, "ShapeMargin")
        ),
        CssProperty::ShapeImageThreshold(p) => format!(
            "CssProperty::ShapeImageThreshold({})",
            print_css_property_value(p, tabs, "ShapeImageThreshold")
        ),
        CssProperty::Content(p) => format!(
            "CssProperty::Content({})",
            print_css_property_value(p, tabs, "Content")
        ),
        CssProperty::CounterReset(p) => format!(
            "CssProperty::CounterReset({})",
            print_css_property_value(p, tabs, "CounterReset")
        ),
        CssProperty::CounterIncrement(p) => format!(
            "CssProperty::CounterIncrement({})",
            print_css_property_value(p, tabs, "CounterIncrement")
        ),
        CssProperty::ListStyleType(p) => format!(
            "CssProperty::ListStyleType({})",
            print_css_property_value(p, tabs, "StyleListStyleType")
        ),
        CssProperty::ListStylePosition(p) => format!(
            "CssProperty::ListStylePosition({})",
            print_css_property_value(p, tabs, "StyleListStylePosition")
        ),
        CssProperty::StringSet(p) => format!(
            "CssProperty::StringSet({})",
            print_css_property_value(p, tabs, "StringSet")
        ),
        CssProperty::TableLayout(p) => format!(
            "CssProperty::TableLayout({})",
            print_css_property_value(p, tabs, "LayoutTableLayout")
        ),
        CssProperty::BorderCollapse(p) => format!(
            "CssProperty::BorderCollapse({})",
            print_css_property_value(p, tabs, "StyleBorderCollapse")
        ),
        CssProperty::BorderSpacing(p) => format!(
            "CssProperty::BorderSpacing({})",
            print_css_property_value(p, tabs, "LayoutBorderSpacing")
        ),
        CssProperty::CaptionSide(p) => format!(
            "CssProperty::CaptionSide({})",
            print_css_property_value(p, tabs, "StyleCaptionSide")
        ),
        CssProperty::EmptyCells(p) => format!(
            "CssProperty::EmptyCells({})",
            print_css_property_value(p, tabs, "StyleEmptyCells")
        ),
        CssProperty::FontWeight(p) => format!(
            "CssProperty::FontWeight({})",
            print_css_property_value(p, tabs, "StyleFontWeight")
        ),
        CssProperty::FontStyle(p) => format!(
            "CssProperty::FontStyle({})",
            print_css_property_value(p, tabs, "StyleFontStyle")
        ),
    }
}

fn print_css_property_value<T: FormatAsRustCode>(
    prop_val: &CssPropertyValue<T>,
    tabs: usize,
    property_value_type: &'static str,
) -> String {
    match prop_val {
        CssPropertyValue::Auto => format!("{}Value::Auto", property_value_type),
        CssPropertyValue::None => format!("{}Value::None", property_value_type),
        CssPropertyValue::Initial => format!("{}Value::Initial", property_value_type),
        CssPropertyValue::Inherit => format!("{}Value::Inherit", property_value_type),
        CssPropertyValue::Revert => format!("{}Value::Revert", property_value_type),
        CssPropertyValue::Unset => format!("{}Value::Unset", property_value_type),
        CssPropertyValue::Exact(t) => format!(
            "{}Value::Exact({})",
            property_value_type,
            t.format_as_rust_code(tabs)
        ),
    }
}
