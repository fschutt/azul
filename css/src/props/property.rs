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
    css::{BoxOrStatic, CssPropertyValue},
    props::basic::{error::InvalidValueErr, pixel::PixelValueWithAuto},
};
// Import all property types from their new locations.
// wildcard imports: this is the property aggregator module that pulls in every
// property type from its sub-modules; enumerating them all explicitly would be
// unmaintainable and defeats the purpose of the per-category modules.
#[allow(clippy::wildcard_imports)]
use crate::{
    codegen::format::FormatAsRustCode,
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
            background::*, border::*, border_radius::*, box_shadow::*, content::*, effects::*,
            exclusion::*, filter::*, lists::*, scrollbar::*, text::*, transform::*,
            SelectionBackgroundColor, SelectionColor, SelectionRadius,
        },
    },
};

const COMBINED_CSS_PROPERTIES_KEY_MAP: [(CombinedCssPropertyType, &str); 27] = [
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
    (CombinedCssPropertyType::TextBox, "text-box"),
    // +spec:writing-modes:798cca - inset-block/inset-inline shorthand properties
    (CombinedCssPropertyType::InsetBlock, "inset-block"),
    (CombinedCssPropertyType::InsetInline, "inset-inline"),
];

const CSS_PROPERTY_KEY_MAP: [(CssPropertyType, &str); 179] = [
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
    (CssPropertyType::WordBreak, "word-break"),
    (CssPropertyType::OverflowWrap, "overflow-wrap"),
    (CssPropertyType::OverflowWrap, "word-wrap"), // +spec:line-breaking:45074d - word-wrap is legacy name alias for overflow-wrap
    (CssPropertyType::LineBreak, "line-break"),
    (CssPropertyType::ObjectFit, "object-fit"),
    (CssPropertyType::ObjectPosition, "object-position"),
    (CssPropertyType::AspectRatio, "aspect-ratio"),
    (CssPropertyType::TextOrientation, "text-orientation"),
    (CssPropertyType::TextAlignLast, "text-align-last"),
    (CssPropertyType::TextTransform, "text-transform"),
    (CssPropertyType::Direction, "direction"),
    (CssPropertyType::UserSelect, "user-select"),
    (CssPropertyType::TextDecoration, "text-decoration"),
    (CssPropertyType::TextIndent, "text-indent"),
    (CssPropertyType::InitialLetter, "initial-letter"),
    (CssPropertyType::LineClamp, "line-clamp"),
    (CssPropertyType::HangingPunctuation, "hanging-punctuation"),
    (CssPropertyType::TextCombineUpright, "text-combine-upright"),
    (CssPropertyType::UnicodeBidi, "unicode-bidi"),
    (CssPropertyType::TextBoxTrim, "text-box-trim"),
    (CssPropertyType::TextBoxEdge, "text-box-edge"),
    (CssPropertyType::DominantBaseline, "dominant-baseline"),
    (CssPropertyType::AlignmentBaseline, "alignment-baseline"),
    (CssPropertyType::InitialLetterAlign, "initial-letter-align"),
    (CssPropertyType::InitialLetterWrap, "initial-letter-wrap"),
    (CssPropertyType::ScrollbarGutter, "scrollbar-gutter"),
    (CssPropertyType::OverflowClipMargin, "overflow-clip-margin"),
    // +spec:overflow:297dc3 - clip rect() auto values resolve to border box edges
    (CssPropertyType::Clip, "clip"),
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
    // +spec:overflow:17654b - overflow-block and overflow-inline logical properties
    (CssPropertyType::OverflowBlock, "overflow-block"),
    (CssPropertyType::OverflowInline, "overflow-inline"),
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
    (
        CssPropertyType::ScrollbarVisibility,
        "-azul-scrollbar-visibility",
    ),
    (
        CssPropertyType::ScrollbarFadeDelay,
        "-azul-scrollbar-fade-delay",
    ),
    (
        CssPropertyType::ScrollbarFadeDuration,
        "-azul-scrollbar-fade-duration",
    ),
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
pub type StyleUnicodeBidiValue = CssPropertyValue<StyleUnicodeBidi>;
pub type StyleTextBoxTrimValue = CssPropertyValue<StyleTextBoxTrim>;
pub type StyleTextBoxEdgeValue = CssPropertyValue<StyleTextBoxEdge>;
pub type StyleDominantBaselineValue = CssPropertyValue<StyleDominantBaseline>;
pub type StyleAlignmentBaselineValue = CssPropertyValue<StyleAlignmentBaseline>;
pub type StyleInitialLetterAlignValue = CssPropertyValue<StyleInitialLetterAlign>;
pub type StyleInitialLetterWrapValue = CssPropertyValue<StyleInitialLetterWrap>;
pub type StyleScrollbarGutterValue = CssPropertyValue<StyleScrollbarGutter>;
pub type StyleOverflowClipMarginValue = CssPropertyValue<StyleOverflowClipMargin>;
pub type StyleClipRectValue = CssPropertyValue<StyleClipRect>;
pub type StyleExclusionMarginValue = CssPropertyValue<StyleExclusionMargin>;
pub type StyleHyphenationLanguageValue = CssPropertyValue<StyleHyphenationLanguage>;
pub type StyleWordSpacingValue = CssPropertyValue<StyleWordSpacing>;
pub type StyleTabSizeValue = CssPropertyValue<StyleTabSize>;
pub type StyleCursorValue = CssPropertyValue<StyleCursor>;
pub type StyleBoxShadowValue = CssPropertyValue<crate::css::BoxOrStaticStyleBoxShadow>;
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
pub type StyleWordBreakValue = CssPropertyValue<StyleWordBreak>;
pub type StyleOverflowWrapValue = CssPropertyValue<StyleOverflowWrap>;
pub type StyleLineBreakValue = CssPropertyValue<StyleLineBreak>;
pub type StyleObjectFitValue = CssPropertyValue<StyleObjectFit>;
pub type StyleObjectPositionValue = CssPropertyValue<StyleObjectPosition>;
pub type StyleAspectRatioValue = CssPropertyValue<StyleAspectRatio>;
pub type StyleTextOrientationValue = CssPropertyValue<StyleTextOrientation>;
pub type StyleTextAlignLastValue = CssPropertyValue<StyleTextAlignLast>;
pub type StyleTextTransformValue = CssPropertyValue<StyleTextTransform>;
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
pub type LayoutGridTemplateAreasValue =
    CssPropertyValue<GridTemplateAreas>;
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
    #[must_use] pub fn get() -> Self {
        get_css_key_map()
    }
}

/// Returns a map useful for parsing the keys of CSS stylesheets
#[must_use] pub fn get_css_key_map() -> CssKeyMap {
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
    TextBox,
    /// `inset-block` shorthand: sets `inset-block-start` + `inset-block-end`
    /// (maps to `top` + `bottom` in horizontal-tb writing mode)
    InsetBlock,
    /// `inset-inline` shorthand: sets `inset-inline-start` + `inset-inline-end`
    /// (maps to `left` + `right` in horizontal-tb writing mode)
    InsetInline,
}

impl fmt::Display for CombinedCssPropertyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The map is `[(CombinedCssPropertyType, &str)]`, so the NAME is slot 1.
        // `.map(|(k, _)| k)` bound slot 0 — the enum itself — and `write!` then called
        // this very impl on it again, recursing until the stack blew.
        let key = COMBINED_CSS_PROPERTIES_KEY_MAP
            .iter()
            .find(|(v, _)| *v == *self)
            .map(|(_, k)| k)
            .unwrap();
        write!(f, "{key}")
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
    #[must_use] pub fn from_str(input: &str, map: &CssKeyMap) -> Option<Self> {
        let input = input.trim();
        map.shorthands.get(input).copied()
    }

    /// Returns the original string that was used to construct this `CssPropertyType`.
    ///
    /// # Panics
    ///
    /// Panics if `self` is not present in `map` (i.e. `map` is not the
    /// `CssKeyMap` this property type was constructed from).
    #[must_use] pub fn to_str(&self, map: &CssKeyMap) -> &'static str {
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
    UnicodeBidi(StyleUnicodeBidiValue),
    TextBoxTrim(StyleTextBoxTrimValue),
    TextBoxEdge(StyleTextBoxEdgeValue),
    DominantBaseline(StyleDominantBaselineValue),
    AlignmentBaseline(StyleAlignmentBaselineValue),
    InitialLetterAlign(StyleInitialLetterAlignValue),
    InitialLetterWrap(StyleInitialLetterWrapValue),
    ScrollbarGutter(StyleScrollbarGutterValue),
    OverflowClipMargin(StyleOverflowClipMarginValue),
    Clip(StyleClipRectValue),
    ExclusionMargin(StyleExclusionMarginValue),
    HyphenationLanguage(StyleHyphenationLanguageValue),
    LineHeight(StyleLineHeightValue),
    WordSpacing(StyleWordSpacingValue),
    TabSize(StyleTabSizeValue),
    WhiteSpace(StyleWhiteSpaceValue),
    Hyphens(StyleHyphensValue),
    WordBreak(StyleWordBreakValue),
    OverflowWrap(StyleOverflowWrapValue),
    LineBreak(StyleLineBreakValue),
    ObjectFit(StyleObjectFitValue),
    ObjectPosition(StyleObjectPositionValue),
    AspectRatio(StyleAspectRatioValue),
    TextOrientation(StyleTextOrientationValue),
    TextAlignLast(StyleTextAlignLastValue),
    TextTransform(StyleTextTransformValue),
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
    OverflowBlock(LayoutOverflowValue),
    OverflowInline(LayoutOverflowValue),
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

crate::impl_vec!(
    CssProperty,
    CssPropertyVec,
    CssPropertyVecDestructor,
    CssPropertyVecDestructorType,
    CssPropertyVecSlice,
    OptionCssProperty
);
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
/// Reference: Taffy (<https://github.com/DioxusLabs/taffy>) uses a binary dirty flag
/// (clean/dirty). Our improvement: 4-level classification enables IFC-only reflow,
/// sizing-only recomputation, and paint-only updates without full subtree relayout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum RelayoutScope {
    /// No relayout needed — repaint only (e.g., color, background, opacity, transform).
    /// The node's size and position are unchanged.
    #[default]
    None,
    /// Only the IFC (Inline Formatting Context) containing this node needs re-shaping.
    /// Block-level siblings are unaffected unless the IFC height changes,
    /// in which case this auto-upgrades to `SizingOnly`.
    IfcOnly,
    /// This node's sizing needs recomputation. Parent may need repositioning
    /// of subsequent siblings but doesn't need full recursive relayout.
    SizingOnly,
    /// Full subtree relayout required (e.g., display, position, float change).
    Full,
}

/// Represents a CSS key (for example `"border-radius"` => `BorderRadius`).
/// You can also derive this key from a `CssProperty` by calling `CssProperty::get_type()`.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    UnicodeBidi,
    TextBoxTrim,
    TextBoxEdge,
    DominantBaseline,
    AlignmentBaseline,
    InitialLetterAlign,
    InitialLetterWrap,
    ScrollbarGutter,
    OverflowClipMargin,
    Clip,
    ExclusionMargin,
    HyphenationLanguage,
    LineHeight,
    WordSpacing,
    TabSize,
    WhiteSpace,
    Hyphens,
    WordBreak,
    OverflowWrap,
    LineBreak,
    ObjectFit,
    ObjectPosition,
    AspectRatio,
    TextOrientation,
    TextAlignLast,
    TextTransform,
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
    OverflowBlock,
    OverflowInline,
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

impl CssPropertyType {
    /// All CSS property types, in declaration order.
    ///
    /// Use this instead of strum's `EnumIter` — ensures a compile error
    /// if a variant is added to the enum but not to this array.
    pub const ALL: &[Self] = &[
        Self::CaretColor,
        Self::CaretAnimationDuration,
        Self::CaretWidth,
        Self::SelectionBackgroundColor,
        Self::SelectionColor,
        Self::SelectionRadius,
        Self::TextColor,
        Self::FontSize,
        Self::FontFamily,
        Self::FontWeight,
        Self::FontStyle,
        Self::TextAlign,
        Self::TextJustify,
        Self::VerticalAlign,
        Self::LetterSpacing,
        Self::TextIndent,
        Self::InitialLetter,
        Self::LineClamp,
        Self::HangingPunctuation,
        Self::TextCombineUpright,
        Self::UnicodeBidi,
        Self::TextBoxTrim,
        Self::TextBoxEdge,
        Self::DominantBaseline,
        Self::AlignmentBaseline,
        Self::InitialLetterAlign,
        Self::InitialLetterWrap,
        Self::ScrollbarGutter,
        Self::OverflowClipMargin,
        Self::Clip,
        Self::ExclusionMargin,
        Self::HyphenationLanguage,
        Self::LineHeight,
        Self::WordSpacing,
        Self::TabSize,
        Self::WhiteSpace,
        Self::Hyphens,
        Self::WordBreak,
        Self::OverflowWrap,
        Self::LineBreak,
        Self::ObjectFit,
        Self::ObjectPosition,
        Self::AspectRatio,
        Self::TextOrientation,
        Self::TextAlignLast,
        Self::TextTransform,
        Self::Direction,
        Self::UserSelect,
        Self::TextDecoration,
        Self::Cursor,
        Self::Display,
        Self::Float,
        Self::BoxSizing,
        Self::Width,
        Self::Height,
        Self::MinWidth,
        Self::MinHeight,
        Self::MaxWidth,
        Self::MaxHeight,
        Self::Position,
        Self::Top,
        Self::Right,
        Self::Left,
        Self::Bottom,
        Self::ZIndex,
        Self::FlexWrap,
        Self::FlexDirection,
        Self::FlexGrow,
        Self::FlexShrink,
        Self::FlexBasis,
        Self::JustifyContent,
        Self::AlignItems,
        Self::AlignContent,
        Self::ColumnGap,
        Self::RowGap,
        Self::GridTemplateColumns,
        Self::GridTemplateRows,
        Self::GridAutoColumns,
        Self::GridAutoRows,
        Self::GridColumn,
        Self::GridRow,
        Self::GridTemplateAreas,
        Self::GridAutoFlow,
        Self::JustifySelf,
        Self::JustifyItems,
        Self::Gap,
        Self::GridGap,
        Self::AlignSelf,
        Self::Font,
        Self::WritingMode,
        Self::Clear,
        Self::BackgroundContent,
        Self::BackgroundPosition,
        Self::BackgroundSize,
        Self::BackgroundRepeat,
        Self::OverflowX,
        Self::OverflowY,
        Self::OverflowBlock,
        Self::OverflowInline,
        Self::PaddingTop,
        Self::PaddingLeft,
        Self::PaddingRight,
        Self::PaddingBottom,
        Self::PaddingInlineStart,
        Self::PaddingInlineEnd,
        Self::MarginTop,
        Self::MarginLeft,
        Self::MarginRight,
        Self::MarginBottom,
        Self::BorderTopLeftRadius,
        Self::BorderTopRightRadius,
        Self::BorderBottomLeftRadius,
        Self::BorderBottomRightRadius,
        Self::BorderTopColor,
        Self::BorderRightColor,
        Self::BorderLeftColor,
        Self::BorderBottomColor,
        Self::BorderTopStyle,
        Self::BorderRightStyle,
        Self::BorderLeftStyle,
        Self::BorderBottomStyle,
        Self::BorderTopWidth,
        Self::BorderRightWidth,
        Self::BorderLeftWidth,
        Self::BorderBottomWidth,
        Self::BoxShadowLeft,
        Self::BoxShadowRight,
        Self::BoxShadowTop,
        Self::BoxShadowBottom,
        Self::ScrollbarTrack,
        Self::ScrollbarThumb,
        Self::ScrollbarButton,
        Self::ScrollbarCorner,
        Self::ScrollbarResizer,
        Self::ScrollbarWidth,
        Self::ScrollbarColor,
        Self::ScrollbarVisibility,
        Self::ScrollbarFadeDelay,
        Self::ScrollbarFadeDuration,
        Self::Opacity,
        Self::Visibility,
        Self::Transform,
        Self::TransformOrigin,
        Self::PerspectiveOrigin,
        Self::BackfaceVisibility,
        Self::MixBlendMode,
        Self::Filter,
        Self::BackdropFilter,
        Self::TextShadow,
        Self::BreakBefore,
        Self::BreakAfter,
        Self::BreakInside,
        Self::Orphans,
        Self::Widows,
        Self::BoxDecorationBreak,
        Self::ColumnCount,
        Self::ColumnWidth,
        Self::ColumnSpan,
        Self::ColumnFill,
        Self::ColumnRuleWidth,
        Self::ColumnRuleStyle,
        Self::ColumnRuleColor,
        Self::FlowInto,
        Self::FlowFrom,
        Self::ShapeOutside,
        Self::ShapeInside,
        Self::ClipPath,
        Self::ShapeMargin,
        Self::ShapeImageThreshold,
        Self::TableLayout,
        Self::BorderCollapse,
        Self::BorderSpacing,
        Self::CaptionSide,
        Self::EmptyCells,
        Self::Content,
        Self::CounterReset,
        Self::CounterIncrement,
        Self::ListStyleType,
        Self::ListStylePosition,
        Self::StringSet,
    ];

    /// Returns an iterator over all CSS property types.
    pub fn iter() -> impl Iterator<Item = Self> {
        Self::ALL.iter().copied()
    }
}

impl fmt::Debug for CssPropertyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl fmt::Display for CssPropertyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    #[must_use] pub fn from_str(input: &str, map: &CssKeyMap) -> Option<Self> {
        let input = input.trim();
        map.non_shorthands.get(input).copied()
    }

    /// Returns the original string that was used to construct this `CssPropertyType`.
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
    #[must_use] pub const fn to_str(&self) -> &'static str {
        match self {
            Self::CaretColor => "caret-color",
            Self::CaretAnimationDuration => "caret-animation-duration",
            Self::CaretWidth => "-azul-caret-width",
            Self::SelectionBackgroundColor => "-azul-selection-background-color",
            Self::SelectionColor => "-azul-selection-color",
            Self::SelectionRadius => "-azul-selection-radius",
            Self::TextColor => "color",
            Self::FontSize => "font-size",
            Self::FontFamily => "font-family",
            Self::FontWeight => "font-weight",
            Self::FontStyle => "font-style",
            Self::TextAlign => "text-align",
            Self::TextJustify => "text-justify",
            Self::VerticalAlign => "vertical-align",
            Self::LetterSpacing => "letter-spacing",
            Self::TextIndent => "text-indent",
            Self::InitialLetter => "initial-letter",
            Self::LineClamp => "line-clamp",
            Self::HangingPunctuation => "hanging-punctuation",
            Self::TextCombineUpright => "text-combine-upright",
            Self::UnicodeBidi => "unicode-bidi",
            Self::TextBoxTrim => "text-box-trim",
            Self::TextBoxEdge => "text-box-edge",
            Self::DominantBaseline => "dominant-baseline",
            Self::AlignmentBaseline => "alignment-baseline",
            Self::InitialLetterAlign => "initial-letter-align",
            Self::InitialLetterWrap => "initial-letter-wrap",
            Self::ScrollbarGutter => "scrollbar-gutter",
            Self::OverflowClipMargin => "overflow-clip-margin",
            Self::Clip => "clip",
            Self::ExclusionMargin => "-azul-exclusion-margin",
            Self::HyphenationLanguage => "-azul-hyphenation-language",
            Self::LineHeight => "line-height",
            Self::WordSpacing => "word-spacing",
            Self::TabSize => "tab-size",
            Self::Cursor => "cursor",
            Self::Display => "display",
            Self::Float => "float",
            Self::BoxSizing => "box-sizing",
            Self::Width => "width",
            Self::Height => "height",
            Self::MinWidth => "min-width",
            Self::MinHeight => "min-height",
            Self::MaxWidth => "max-width",
            Self::MaxHeight => "max-height",
            Self::Position => "position",
            Self::Top => "top",
            Self::Right => "right",
            Self::Left => "left",
            Self::Bottom => "bottom",
            Self::ZIndex => "z-index",
            Self::FlexWrap => "flex-wrap",
            Self::FlexDirection => "flex-direction",
            Self::FlexGrow => "flex-grow",
            Self::FlexShrink => "flex-shrink",
            Self::FlexBasis => "flex-basis",
            Self::JustifyContent => "justify-content",
            Self::AlignItems => "align-items",
            Self::AlignContent => "align-content",
            Self::ColumnGap => "column-gap",
            Self::RowGap => "row-gap",
            Self::GridTemplateColumns => "grid-template-columns",
            Self::GridTemplateRows => "grid-template-rows",
            Self::GridAutoFlow => "grid-auto-flow",
            Self::JustifySelf => "justify-self",
            Self::JustifyItems => "justify-items",
            Self::Gap => "gap",
            Self::GridGap => "grid-gap",
            Self::AlignSelf => "align-self",
            Self::Font => "font",
            Self::GridAutoColumns => "grid-auto-columns",
            Self::GridAutoRows => "grid-auto-rows",
            Self::GridColumn => "grid-column",
            Self::GridRow => "grid-row",
            Self::GridTemplateAreas => "grid-template-areas",
            Self::WritingMode => "writing-mode",
            Self::Clear => "clear",
            Self::BackgroundContent => "background",
            Self::BackgroundPosition => "background-position",
            Self::BackgroundSize => "background-size",
            Self::BackgroundRepeat => "background-repeat",
            Self::OverflowX => "overflow-x",
            Self::OverflowY => "overflow-y",
            Self::OverflowBlock => "overflow-block",
            Self::OverflowInline => "overflow-inline",
            Self::PaddingTop => "padding-top",
            Self::PaddingLeft => "padding-left",
            Self::PaddingRight => "padding-right",
            Self::PaddingBottom => "padding-bottom",
            Self::PaddingInlineStart => "padding-inline-start",
            Self::PaddingInlineEnd => "padding-inline-end",
            Self::MarginTop => "margin-top",
            Self::MarginLeft => "margin-left",
            Self::MarginRight => "margin-right",
            Self::MarginBottom => "margin-bottom",
            Self::BorderTopLeftRadius => "border-top-left-radius",
            Self::BorderTopRightRadius => "border-top-right-radius",
            Self::BorderBottomLeftRadius => "border-bottom-left-radius",
            Self::BorderBottomRightRadius => "border-bottom-right-radius",
            Self::BorderTopColor => "border-top-color",
            Self::BorderRightColor => "border-right-color",
            Self::BorderLeftColor => "border-left-color",
            Self::BorderBottomColor => "border-bottom-color",
            Self::BorderTopStyle => "border-top-style",
            Self::BorderRightStyle => "border-right-style",
            Self::BorderLeftStyle => "border-left-style",
            Self::BorderBottomStyle => "border-bottom-style",
            Self::BorderTopWidth => "border-top-width",
            Self::BorderRightWidth => "border-right-width",
            Self::BorderLeftWidth => "border-left-width",
            Self::BorderBottomWidth => "border-bottom-width",
            Self::BoxShadowLeft => "-azul-box-shadow-left",
            Self::BoxShadowRight => "-azul-box-shadow-right",
            Self::BoxShadowTop => "-azul-box-shadow-top",
            Self::BoxShadowBottom => "-azul-box-shadow-bottom",
            Self::ScrollbarTrack => "-azul-scrollbar-track",
            Self::ScrollbarThumb => "-azul-scrollbar-thumb",
            Self::ScrollbarButton => "-azul-scrollbar-button",
            Self::ScrollbarCorner => "-azul-scrollbar-corner",
            Self::ScrollbarResizer => "-azul-scrollbar-resizer",
            Self::ScrollbarWidth => "scrollbar-width",
            Self::ScrollbarColor => "scrollbar-color",
            Self::ScrollbarVisibility => "-azul-scrollbar-visibility",
            Self::ScrollbarFadeDelay => "-azul-scrollbar-fade-delay",
            Self::ScrollbarFadeDuration => "-azul-scrollbar-fade-duration",
            Self::Opacity => "opacity",
            Self::Visibility => "visibility",
            Self::Transform => "transform",
            Self::TransformOrigin => "transform-origin",
            Self::PerspectiveOrigin => "perspective-origin",
            Self::BackfaceVisibility => "backface-visibility",
            Self::MixBlendMode => "mix-blend-mode",
            Self::Filter => "filter",
            Self::BackdropFilter => "backdrop-filter",
            Self::TextShadow => "text-shadow",
            Self::WhiteSpace => "white-space",
            Self::Hyphens => "hyphens",
            Self::WordBreak => "word-break",
            Self::OverflowWrap => "overflow-wrap",
            Self::LineBreak => "line-break",
            Self::ObjectFit => "object-fit",
            Self::ObjectPosition => "object-position",
            Self::AspectRatio => "aspect-ratio",
            Self::TextOrientation => "text-orientation",
            Self::TextAlignLast => "text-align-last",
            Self::TextTransform => "text-transform",
            Self::Direction => "direction",
            Self::UserSelect => "user-select",
            Self::TextDecoration => "text-decoration",
            Self::BreakBefore => "break-before",
            Self::BreakAfter => "break-after",
            Self::BreakInside => "break-inside",
            Self::Orphans => "orphans",
            Self::Widows => "widows",
            Self::BoxDecorationBreak => "box-decoration-break",
            Self::ColumnCount => "column-count",
            Self::ColumnWidth => "column-width",
            Self::ColumnSpan => "column-span",
            Self::ColumnFill => "column-fill",
            Self::ColumnRuleWidth => "column-rule-width",
            Self::ColumnRuleStyle => "column-rule-style",
            Self::ColumnRuleColor => "column-rule-color",
            Self::FlowInto => "flow-into",
            Self::FlowFrom => "flow-from",
            Self::ShapeOutside => "shape-outside",
            Self::ShapeInside => "shape-inside",
            Self::ClipPath => "clip-path",
            Self::ShapeMargin => "shape-margin",
            Self::ShapeImageThreshold => "shape-image-threshold",
            Self::TableLayout => "table-layout",
            Self::BorderCollapse => "border-collapse",
            Self::BorderSpacing => "border-spacing",
            Self::CaptionSide => "caption-side",
            Self::EmptyCells => "empty-cells",
            Self::Content => "content",
            Self::CounterReset => "counter-reset",
            Self::CounterIncrement => "counter-increment",
            Self::ListStyleType => "list-style-type",
            Self::ListStylePosition => "list-style-position",
            Self::StringSet => "string-set",
        }
    }

    /// Returns whether this property will be inherited during cascading
    /// Returns whether this CSS property is inherited by default according to CSS specifications.
    ///
    /// Reference: <https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Cascade/Inheritance>
    // +spec:display-property:b4cf6d - unicode-bidi does not inherit (removed from inheritable set)
    #[must_use] pub const fn is_inheritable(&self) -> bool {
        use self::CssPropertyType::{FontFamily, FontSize, FontWeight, FontStyle, LineHeight, LetterSpacing, WordSpacing, TextIndent, TextColor, TextAlign, TextJustify, TextDecoration, WhiteSpace, Direction, Hyphens, TabSize, WordBreak, OverflowWrap, LineBreak, TextAlignLast, TextTransform, TextOrientation, HangingPunctuation, TextCombineUpright, HyphenationLanguage, ListStyleType, ListStylePosition, BorderCollapse, BorderSpacing, CaptionSide, EmptyCells, Visibility, Cursor, Widows, Orphans, WritingMode, UserSelect};
        match self {
            // Font properties
            FontFamily | FontSize | FontWeight | FontStyle | LineHeight | LetterSpacing | WordSpacing | TextIndent |

            // Text properties
            TextColor | TextAlign | TextJustify | TextDecoration | WhiteSpace | Direction | Hyphens | TabSize |
            WordBreak | OverflowWrap | LineBreak | TextAlignLast | TextTransform |
            TextOrientation |
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

    #[must_use] pub const fn has_compact_encoding(&self) -> bool {
        use self::CssPropertyType::{Display, Position, Float, OverflowX, OverflowY, BoxSizing, FlexDirection, FlexWrap, JustifyContent, AlignItems, AlignContent, WritingMode, Clear, FontWeight, FontStyle, TextAlign, Visibility, WhiteSpace, Direction, VerticalAlign, BorderCollapse, Width, Height, MinWidth, MaxWidth, MinHeight, MaxHeight, FlexBasis, FontSize, PaddingTop, PaddingRight, PaddingBottom, PaddingLeft, MarginTop, MarginRight, MarginBottom, MarginLeft, BorderTopWidth, BorderRightWidth, BorderBottomWidth, BorderLeftWidth, Top, Right, Bottom, Left, FlexGrow, FlexShrink, ZIndex, BorderTopStyle, BorderRightStyle, BorderBottomStyle, BorderLeftStyle, BorderTopColor, BorderRightColor, BorderBottomColor, BorderLeftColor, BorderSpacing, TabSize, TextColor, FontFamily, LineHeight, LetterSpacing, WordSpacing, TextIndent, AlignSelf, JustifySelf, GridAutoFlow, JustifyItems, ColumnGap, RowGap, Gap, GridColumn, GridRow};
        matches!(
            self,
            // Tier 1 enums
            Display | Position | Float | OverflowX | OverflowY | BoxSizing |
            FlexDirection | FlexWrap | JustifyContent | AlignItems | AlignContent |
            WritingMode | Clear | FontWeight | FontStyle | TextAlign |
            Visibility | WhiteSpace | Direction | VerticalAlign | BorderCollapse |
            // Tier 2 dims
            Width | Height | MinWidth | MaxWidth | MinHeight | MaxHeight |
            FlexBasis | FontSize |
            PaddingTop | PaddingRight | PaddingBottom | PaddingLeft |
            MarginTop | MarginRight | MarginBottom | MarginLeft |
            BorderTopWidth | BorderRightWidth | BorderBottomWidth | BorderLeftWidth |
            Top | Right | Bottom | Left |
            FlexGrow | FlexShrink |
            // Tier 2 cold
            ZIndex |
            BorderTopStyle | BorderRightStyle | BorderBottomStyle | BorderLeftStyle |
            BorderTopColor | BorderRightColor | BorderBottomColor | BorderLeftColor |
            BorderSpacing | TabSize |
            // Tier 2b text
            TextColor | FontFamily | LineHeight | LetterSpacing | WordSpacing | TextIndent |
            // Grid/flex alignment (tier1 extension)
            AlignSelf | JustifySelf | GridAutoFlow | JustifyItems |
            // Gap (tier2 extension)
            ColumnGap | RowGap | Gap |
            // Grid placement (tier2_cold extension)
            GridColumn | GridRow
        )
    }

    #[must_use] pub const fn get_category(&self) -> CssPropertyCategory {
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
    #[must_use] pub const fn can_trigger_relayout(&self) -> bool {
        use self::CssPropertyType::{TextColor, Cursor, BackgroundContent, BackgroundPosition, BackgroundSize, BackgroundRepeat, BorderTopLeftRadius, BorderTopRightRadius, BorderBottomLeftRadius, BorderBottomRightRadius, BorderTopColor, BorderRightColor, BorderLeftColor, BorderBottomColor, BorderTopStyle, BorderRightStyle, BorderLeftStyle, BorderBottomStyle, ColumnRuleColor, ColumnRuleStyle, BoxShadowLeft, BoxShadowRight, BoxShadowTop, BoxShadowBottom, BoxDecorationBreak, ScrollbarTrack, ScrollbarThumb, ScrollbarButton, ScrollbarCorner, ScrollbarResizer, Opacity, Transform, TransformOrigin, PerspectiveOrigin, BackfaceVisibility, MixBlendMode, Filter, BackdropFilter, TextShadow, Clip};

        // Since the border can be larger than the content,
        // in which case the content needs to be re-layouted, assume true for Border

        // FontFamily, FontSize, LetterSpacing and LineHeight can affect
        // the text layout and therefore the screen layout

        !matches!(
            self,
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
            | TextShadow
            | Clip
        )
    }

    /// Returns whether the property is a GPU property (currently only opacity and transforms)
    #[must_use] pub const fn is_gpu_only_property(&self) -> bool {
        match self {
            Self::Opacity |
            Self::Transform /* | CssPropertyType::Color */ => true,
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
    #[must_use] pub const fn relayout_scope(&self, node_is_ifc_member: bool) -> RelayoutScope {
        use CssPropertyType::{TextColor, Cursor, BackgroundContent, BackgroundPosition, BackgroundSize, BackgroundRepeat, BorderTopColor, BorderRightColor, BorderLeftColor, BorderBottomColor, BorderTopStyle, BorderRightStyle, BorderLeftStyle, BorderBottomStyle, BorderTopLeftRadius, BorderTopRightRadius, BorderBottomLeftRadius, BorderBottomRightRadius, ColumnRuleColor, ColumnRuleStyle, BoxShadowLeft, BoxShadowRight, BoxShadowTop, BoxShadowBottom, BoxDecorationBreak, ScrollbarTrack, ScrollbarThumb, ScrollbarButton, ScrollbarCorner, ScrollbarResizer, Opacity, Transform, TransformOrigin, PerspectiveOrigin, BackfaceVisibility, MixBlendMode, Filter, BackdropFilter, TextShadow, SelectionBackgroundColor, SelectionColor, SelectionRadius, CaretColor, CaretAnimationDuration, CaretWidth, ObjectFit, ObjectPosition, Clip, FontFamily, FontSize, FontWeight, FontStyle, LetterSpacing, WordSpacing, LineHeight, TextAlign, TextJustify, TextIndent, WhiteSpace, TabSize, Hyphens, WordBreak, OverflowWrap, LineBreak, TextAlignLast, TextOrientation, HyphenationLanguage, TextCombineUpright, TextDecoration, HangingPunctuation, InitialLetter, LineClamp, Direction, VerticalAlign, UnicodeBidi, TextBoxTrim, TextBoxEdge, DominantBaseline, AlignmentBaseline, InitialLetterAlign, InitialLetterWrap, Width, Height, MinWidth, MinHeight, MaxWidth, MaxHeight, PaddingTop, PaddingRight, PaddingBottom, PaddingLeft, PaddingInlineStart, PaddingInlineEnd, BorderTopWidth, BorderRightWidth, BorderBottomWidth, BorderLeftWidth, BoxSizing, ScrollbarWidth, ScrollbarVisibility, ScrollbarGutter, OverflowClipMargin};
        match self {
            // Pure paint — never triggers relayout
            TextColor
            | Cursor
            | BackgroundContent
            | BackgroundPosition
            | BackgroundSize
            | BackgroundRepeat
            | BorderTopColor
            | BorderRightColor
            | BorderLeftColor
            | BorderBottomColor
            | BorderTopStyle
            | BorderRightStyle
            | BorderLeftStyle
            | BorderBottomStyle
            | BorderTopLeftRadius
            | BorderTopRightRadius
            | BorderBottomLeftRadius
            | BorderBottomRightRadius
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
            | TextShadow
            | SelectionBackgroundColor
            | SelectionColor
            | SelectionRadius
            | CaretColor
            | CaretAnimationDuration
            | CaretWidth
            | ObjectFit
            | ObjectPosition
            | Clip => RelayoutScope::None,

            // Font/text properties — IFC-only if inside inline context,
            // otherwise no layout impact (block with only block children
            // inherits but doesn't directly reflow).
            FontFamily | FontSize | FontWeight | FontStyle | LetterSpacing | WordSpacing
            | LineHeight | TextAlign | TextJustify | TextIndent | WhiteSpace | TabSize
            | Hyphens | WordBreak | OverflowWrap | LineBreak | TextAlignLast | TextOrientation
            | HyphenationLanguage | TextCombineUpright | TextDecoration | HangingPunctuation
            | InitialLetter | LineClamp | Direction | VerticalAlign | UnicodeBidi | TextBoxTrim
            | TextBoxEdge | DominantBaseline | AlignmentBaseline | InitialLetterAlign
            | InitialLetterWrap => {
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
            Width | Height | MinWidth | MinHeight | MaxWidth | MaxHeight | PaddingTop
            | PaddingRight | PaddingBottom | PaddingLeft | PaddingInlineStart
            | PaddingInlineEnd | BorderTopWidth | BorderRightWidth | BorderBottomWidth
            | BorderLeftWidth | BoxSizing | ScrollbarWidth | ScrollbarVisibility
            | ScrollbarGutter | OverflowClipMargin => RelayoutScope::SizingOnly,

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
    BorderRadius(CssBorderRadiusParseError<'a>),
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
    UnicodeBidi(StyleUnicodeBidiParseError<'a>),
    TextBoxTrim(StyleTextBoxTrimParseError<'a>),
    TextBoxEdge(StyleTextBoxEdgeParseError<'a>),
    DominantBaseline(StyleDominantBaselineParseError<'a>),
    AlignmentBaseline(StyleAlignmentBaselineParseError<'a>),
    InitialLetterAlign(StyleInitialLetterAlignParseError<'a>),
    InitialLetterWrap(StyleInitialLetterWrapParseError<'a>),
    ScrollbarGutter(StyleScrollbarGutterParseError<'a>),
    OverflowClipMargin(StyleOverflowClipMarginParseError<'a>),
    Clip(StyleClipRectParseError<'a>),
    ExclusionMargin(StyleExclusionMarginParseError),
    HyphenationLanguage(StyleHyphenationLanguageParseError),
    LineHeight(StyleLineHeightParseError),
    WordSpacing(StyleWordSpacingParseError<'a>),
    TabSize(StyleTabSizeParseError<'a>),
    WhiteSpace(StyleWhiteSpaceParseError<'a>),
    Hyphens(StyleHyphensParseError<'a>),
    WordBreak(StyleWordBreakParseError<'a>),
    OverflowWrap(StyleOverflowWrapParseError<'a>),
    LineBreak(StyleLineBreakParseError<'a>),
    ObjectFit(StyleObjectFitParseError<'a>),
    ObjectPosition(StyleObjectPositionParseError<'a>),
    AspectRatio(StyleAspectRatioParseError<'a>),
    TextOrientation(StyleTextOrientationParseError<'a>),
    TextAlignLast(StyleTextAlignLastParseError<'a>),
    TextTransform(StyleTextTransformParseError<'a>),
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
    UnicodeBidi(StyleUnicodeBidiParseErrorOwned),
    TextBoxTrim(StyleTextBoxTrimParseErrorOwned),
    TextBoxEdge(StyleTextBoxEdgeParseErrorOwned),
    DominantBaseline(StyleDominantBaselineParseErrorOwned),
    AlignmentBaseline(StyleAlignmentBaselineParseErrorOwned),
    InitialLetterAlign(StyleInitialLetterAlignParseErrorOwned),
    InitialLetterWrap(StyleInitialLetterWrapParseErrorOwned),
    ScrollbarGutter(StyleScrollbarGutterParseErrorOwned),
    OverflowClipMargin(StyleOverflowClipMarginParseErrorOwned),
    Clip(StyleClipRectParseErrorOwned),
    ExclusionMargin(StyleExclusionMarginParseErrorOwned),
    HyphenationLanguage(StyleHyphenationLanguageParseErrorOwned),
    LineHeight(StyleLineHeightParseError),
    WordSpacing(StyleWordSpacingParseErrorOwned),
    TabSize(StyleTabSizeParseErrorOwned),
    WhiteSpace(StyleWhiteSpaceParseErrorOwned),
    Hyphens(StyleHyphensParseErrorOwned),
    WordBreak(StyleWordBreakParseErrorOwned),
    OverflowWrap(StyleOverflowWrapParseErrorOwned),
    LineBreak(StyleLineBreakParseErrorOwned),
    ObjectFit(StyleObjectFitParseErrorOwned),
    ObjectPosition(StyleObjectPositionParseErrorOwned),
    AspectRatio(StyleAspectRatioParseErrorOwned),
    TextOrientation(StyleTextOrientationParseErrorOwned),
    TextAlignLast(StyleTextAlignLastParseErrorOwned),
    TextTransform(StyleTextTransformParseErrorOwned),
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
    UnicodeBidi(e) => format!("Invalid unicode-bidi: {}", e),
    TextBoxTrim(e) => format!("Invalid text-box-trim: {}", e),
    TextBoxEdge(e) => format!("Invalid text-box-edge: {}", e),
    DominantBaseline(e) => format!("Invalid dominant-baseline: {}", e),
    AlignmentBaseline(e) => format!("Invalid alignment-baseline: {}", e),
    InitialLetterAlign(e) => format!("Invalid initial-letter-align: {}", e),
    InitialLetterWrap(e) => format!("Invalid initial-letter-wrap: {}", e),
    ScrollbarGutter(e) => format!("Invalid scrollbar-gutter: {}", e),
    OverflowClipMargin(e) => format!("Invalid overflow-clip-margin: {}", e),
    Clip(e) => format!("Invalid clip: {}", e),
    ExclusionMargin(e) => format!("Invalid -azul-exclusion-margin: {}", e),
    HyphenationLanguage(e) => format!("Invalid -azul-hyphenation-language: {}", e),
    LineHeight(e) => format!("Invalid line-height: {}", e),
    WordSpacing(e) => format!("Invalid word-spacing: {}", e),
    TabSize(e) => format!("Invalid tab-size: {}", e),
    WhiteSpace(e) => format!("Invalid white-space: {}", e),
    Hyphens(e) => format!("Invalid hyphens: {}", e),
    WordBreak(e) => format!("Invalid word-break: {}", e),
    OverflowWrap(e) => format!("Invalid overflow-wrap: {}", e),
    LineBreak(e) => format!("Invalid line-break: {}", e),
    ObjectFit(e) => format!("Invalid object-fit: {}", e),
    ObjectPosition(e) => format!("Invalid object-position: {}", e),
    AspectRatio(e) => format!("Invalid aspect-ratio: {}", e),
    TextOrientation(e) => format!("Invalid text-orientation: {}", e),
    TextAlignLast(e) => format!("Invalid text-align-last: {}", e),
    TextTransform(e) => format!("Invalid text-transform: {}", e),
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
impl_from!(CssBorderRadiusParseError<'a>, CssParsingError::BorderRadius);
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
impl_from!(StyleUnicodeBidiParseError<'a>, CssParsingError::UnicodeBidi);
impl_from!(StyleTextBoxTrimParseError<'a>, CssParsingError::TextBoxTrim);
impl_from!(StyleTextBoxEdgeParseError<'a>, CssParsingError::TextBoxEdge);
impl_from!(
    StyleDominantBaselineParseError<'a>,
    CssParsingError::DominantBaseline
);
impl_from!(
    StyleAlignmentBaselineParseError<'a>,
    CssParsingError::AlignmentBaseline
);
impl_from!(
    StyleInitialLetterAlignParseError<'a>,
    CssParsingError::InitialLetterAlign
);
impl_from!(
    StyleInitialLetterWrapParseError<'a>,
    CssParsingError::InitialLetterWrap
);
impl_from!(
    StyleScrollbarGutterParseError<'a>,
    CssParsingError::ScrollbarGutter
);
impl_from!(
    StyleOverflowClipMarginParseError<'a>,
    CssParsingError::OverflowClipMargin
);
impl_from!(StyleClipRectParseError<'a>, CssParsingError::Clip);

// Manual From implementation for StyleExclusionMarginParseError (no lifetime)
#[cfg(feature = "parser")]
impl From<StyleExclusionMarginParseError> for CssParsingError<'_> {
    fn from(e: StyleExclusionMarginParseError) -> Self {
        CssParsingError::ExclusionMargin(e)
    }
}

// Manual From implementation for StyleHyphenationLanguageParseError (no lifetime)
#[cfg(feature = "parser")]
impl From<StyleHyphenationLanguageParseError> for CssParsingError<'_> {
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
impl_from!(StyleWordBreakParseError<'a>, CssParsingError::WordBreak);
impl_from!(
    StyleOverflowWrapParseError<'a>,
    CssParsingError::OverflowWrap
);
impl_from!(StyleLineBreakParseError<'a>, CssParsingError::LineBreak);
impl_from!(StyleObjectFitParseError<'a>, CssParsingError::ObjectFit);
impl_from!(
    StyleObjectPositionParseError<'a>,
    CssParsingError::ObjectPosition
);
impl_from!(StyleAspectRatioParseError<'a>, CssParsingError::AspectRatio);
impl_from!(
    StyleTextOrientationParseError<'a>,
    CssParsingError::TextOrientation
);
impl_from!(
    StyleTextAlignLastParseError<'a>,
    CssParsingError::TextAlignLast
);
impl_from!(
    StyleTextTransformParseError<'a>,
    CssParsingError::TextTransform
);
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

impl From<PercentageParseError> for CssParsingError<'_> {
    fn from(e: PercentageParseError) -> Self {
        CssParsingError::Percentage(e)
    }
}

impl From<StyleLineHeightParseError> for CssParsingError<'_> {
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

impl CssParsingError<'_> {
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
    #[must_use] pub fn to_contained(&self) -> CssParsingErrorOwned {
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
            CssParsingError::UnicodeBidi(e) => CssParsingErrorOwned::UnicodeBidi(e.to_contained()),
            CssParsingError::TextBoxTrim(e) => CssParsingErrorOwned::TextBoxTrim(e.to_contained()),
            CssParsingError::TextBoxEdge(e) => CssParsingErrorOwned::TextBoxEdge(e.to_contained()),
            CssParsingError::DominantBaseline(e) => {
                CssParsingErrorOwned::DominantBaseline(e.to_contained())
            }
            CssParsingError::AlignmentBaseline(e) => {
                CssParsingErrorOwned::AlignmentBaseline(e.to_contained())
            }
            CssParsingError::InitialLetterAlign(e) => {
                CssParsingErrorOwned::InitialLetterAlign(e.to_contained())
            }
            CssParsingError::InitialLetterWrap(e) => {
                CssParsingErrorOwned::InitialLetterWrap(e.to_contained())
            }
            CssParsingError::ScrollbarGutter(e) => {
                CssParsingErrorOwned::ScrollbarGutter(e.to_contained())
            }
            CssParsingError::OverflowClipMargin(e) => {
                CssParsingErrorOwned::OverflowClipMargin(e.to_contained())
            }
            CssParsingError::Clip(e) => CssParsingErrorOwned::Clip(e.to_contained()),
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
            CssParsingError::WordBreak(e) => CssParsingErrorOwned::WordBreak(e.to_contained()),
            CssParsingError::OverflowWrap(e) => {
                CssParsingErrorOwned::OverflowWrap(e.to_contained())
            }
            CssParsingError::LineBreak(e) => CssParsingErrorOwned::LineBreak(e.to_contained()),
            CssParsingError::ObjectFit(e) => CssParsingErrorOwned::ObjectFit(e.to_contained()),
            CssParsingError::ObjectPosition(e) => {
                CssParsingErrorOwned::ObjectPosition(e.to_contained())
            }
            CssParsingError::AspectRatio(e) => CssParsingErrorOwned::AspectRatio(e.to_contained()),
            CssParsingError::TextOrientation(e) => {
                CssParsingErrorOwned::TextOrientation(e.to_contained())
            }
            CssParsingError::TextAlignLast(e) => {
                CssParsingErrorOwned::TextAlignLast(e.to_contained())
            }
            CssParsingError::TextTransform(e) => {
                CssParsingErrorOwned::TextTransform(e.to_contained())
            }
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
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
    #[must_use] pub fn to_shared(&self) -> CssParsingError<'_> {
        match self {
            Self::CaretColor(e) => CssParsingError::CaretColor(e.to_shared()),
            Self::CaretWidth(e) => CssParsingError::CaretWidth(e.to_shared()),
            Self::CaretAnimationDuration(e) => {
                CssParsingError::CaretAnimationDuration(e.to_shared())
            }
            Self::SelectionBackgroundColor(e) => {
                CssParsingError::SelectionBackgroundColor(e.to_shared())
            }
            Self::SelectionColor(e) => {
                CssParsingError::SelectionColor(e.to_shared())
            }
            Self::SelectionRadius(e) => {
                CssParsingError::SelectionRadius(e.to_shared())
            }
            Self::Border(e) => CssParsingError::Border(e.inner.to_shared()),
            Self::BorderRadius(e) => {
                CssParsingError::BorderRadius(e.inner.to_shared())
            }
            Self::Padding(e) => CssParsingError::Padding(e.to_shared()),
            Self::Margin(e) => CssParsingError::Margin(e.to_shared()),
            Self::Overflow(e) => CssParsingError::Overflow(e.to_shared()),
            Self::BoxShadow(e) => CssParsingError::BoxShadow(e.to_shared()),
            Self::Color(e) => CssParsingError::Color(e.to_shared()),
            Self::PixelValue(e) => CssParsingError::PixelValue(e.to_shared()),
            Self::Percentage(e) => CssParsingError::Percentage(e.clone()),
            Self::FontFamily(e) => CssParsingError::FontFamily(e.to_shared()),
            Self::InvalidValue(e) => CssParsingError::InvalidValue(e.to_shared()),
            Self::FlexGrow(e) => CssParsingError::FlexGrow(e.to_shared()),
            Self::FlexShrink(e) => CssParsingError::FlexShrink(e.to_shared()),
            Self::Background(e) => CssParsingError::Background(e.to_shared()),
            Self::BackgroundPosition(e) => {
                CssParsingError::BackgroundPosition(e.to_shared())
            }
            Self::Opacity(e) => CssParsingError::Opacity(e.to_shared()),
            Self::Visibility(e) => CssParsingError::Visibility(e.to_shared()),
            Self::LayoutScrollbarWidth(e) => {
                CssParsingError::LayoutScrollbarWidth(e.to_shared())
            }
            Self::StyleScrollbarColor(e) => {
                CssParsingError::StyleScrollbarColor(e.to_shared())
            }
            Self::ScrollbarVisibilityMode(e) => {
                CssParsingError::ScrollbarVisibilityMode(e.to_shared())
            }
            Self::ScrollbarFadeDelay(e) => {
                CssParsingError::ScrollbarFadeDelay(e.to_shared())
            }
            Self::ScrollbarFadeDuration(e) => {
                CssParsingError::ScrollbarFadeDuration(e.to_shared())
            }
            Self::Transform(e) => CssParsingError::Transform(e.to_shared()),
            Self::TransformOrigin(e) => {
                CssParsingError::TransformOrigin(e.to_shared())
            }
            Self::PerspectiveOrigin(e) => {
                CssParsingError::PerspectiveOrigin(e.to_shared())
            }
            Self::Filter(e) => CssParsingError::Filter(e.to_shared()),
            Self::LayoutWidth(e) => CssParsingError::LayoutWidth(e.to_shared()),
            Self::LayoutHeight(e) => CssParsingError::LayoutHeight(e.to_shared()),
            Self::LayoutMinWidth(e) => {
                CssParsingError::LayoutMinWidth(e.to_shared())
            }
            Self::LayoutMinHeight(e) => {
                CssParsingError::LayoutMinHeight(e.to_shared())
            }
            Self::LayoutMaxWidth(e) => {
                CssParsingError::LayoutMaxWidth(e.to_shared())
            }
            Self::LayoutMaxHeight(e) => {
                CssParsingError::LayoutMaxHeight(e.to_shared())
            }
            Self::LayoutPosition(e) => {
                CssParsingError::LayoutPosition(e.to_shared())
            }
            Self::LayoutTop(e) => CssParsingError::LayoutTop(e.to_shared()),
            Self::LayoutRight(e) => CssParsingError::LayoutRight(e.to_shared()),
            Self::LayoutLeft(e) => CssParsingError::LayoutLeft(e.to_shared()),
            Self::LayoutInsetBottom(e) => {
                CssParsingError::LayoutInsetBottom(e.to_shared())
            }
            Self::LayoutZIndex(e) => CssParsingError::LayoutZIndex(e.to_shared()),
            Self::FlexWrap(e) => CssParsingError::FlexWrap(e.to_shared()),
            Self::FlexDirection(e) => CssParsingError::FlexDirection(e.to_shared()),
            Self::FlexBasis(e) => CssParsingError::FlexBasis(e.to_shared()),
            Self::JustifyContent(e) => {
                CssParsingError::JustifyContent(e.to_shared())
            }
            Self::AlignItems(e) => CssParsingError::AlignItems(e.to_shared()),
            Self::AlignContent(e) => CssParsingError::AlignContent(e.to_shared()),
            Self::Grid(e) => CssParsingError::Grid(e.to_shared()),
            Self::GridAutoFlow(e) => CssParsingError::GridAutoFlow(e.to_shared()),
            Self::JustifySelf(e) => CssParsingError::JustifySelf(e.to_shared()),
            Self::JustifyItems(e) => CssParsingError::JustifyItems(e.to_shared()),
            Self::AlignSelf(e) => CssParsingError::AlignSelf(e.to_shared()),
            Self::LayoutWritingMode(e) => {
                CssParsingError::LayoutWritingMode(e.to_shared())
            }
            Self::LayoutClear(e) => CssParsingError::LayoutClear(e.to_shared()),
            Self::LayoutOverflow(e) => {
                CssParsingError::LayoutOverflow(e.to_shared())
            }
            Self::BorderTopLeftRadius(e) => {
                CssParsingError::BorderTopLeftRadius(e.to_shared())
            }
            Self::BorderTopRightRadius(e) => {
                CssParsingError::BorderTopRightRadius(e.to_shared())
            }
            Self::BorderBottomLeftRadius(e) => {
                CssParsingError::BorderBottomLeftRadius(e.to_shared())
            }
            Self::BorderBottomRightRadius(e) => {
                CssParsingError::BorderBottomRightRadius(e.to_shared())
            }
            Self::BorderStyle(e) => CssParsingError::BorderStyle(e.to_shared()),
            Self::BackfaceVisibility(e) => {
                CssParsingError::BackfaceVisibility(e.to_shared())
            }
            Self::MixBlendMode(e) => CssParsingError::MixBlendMode(e.to_shared()),
            Self::TextColor(e) => CssParsingError::TextColor(e.to_shared()),
            Self::FontSize(e) => CssParsingError::FontSize(e.to_shared()),
            Self::TextAlign(e) => CssParsingError::TextAlign(e.to_shared()),
            Self::TextJustify(e) => CssParsingError::TextJustify(e.to_borrowed()),
            Self::LetterSpacing(e) => CssParsingError::LetterSpacing(e.to_shared()),
            Self::TextIndent(e) => CssParsingError::TextIndent(e.to_shared()),
            Self::InitialLetter(e) => CssParsingError::InitialLetter(e.to_shared()),
            Self::LineClamp(e) => CssParsingError::LineClamp(e.to_shared()),
            Self::HangingPunctuation(e) => {
                CssParsingError::HangingPunctuation(e.to_shared())
            }
            Self::TextCombineUpright(e) => {
                CssParsingError::TextCombineUpright(e.to_shared())
            }
            Self::UnicodeBidi(e) => CssParsingError::UnicodeBidi(e.to_shared()),
            Self::TextBoxTrim(e) => CssParsingError::TextBoxTrim(e.to_shared()),
            Self::TextBoxEdge(e) => CssParsingError::TextBoxEdge(e.to_shared()),
            Self::DominantBaseline(e) => {
                CssParsingError::DominantBaseline(e.to_shared())
            }
            Self::AlignmentBaseline(e) => {
                CssParsingError::AlignmentBaseline(e.to_shared())
            }
            Self::InitialLetterAlign(e) => {
                CssParsingError::InitialLetterAlign(e.to_shared())
            }
            Self::InitialLetterWrap(e) => {
                CssParsingError::InitialLetterWrap(e.to_shared())
            }
            Self::ScrollbarGutter(e) => {
                CssParsingError::ScrollbarGutter(e.to_shared())
            }
            Self::OverflowClipMargin(e) => {
                CssParsingError::OverflowClipMargin(e.to_shared())
            }
            Self::Clip(e) => CssParsingError::Clip(e.to_shared()),
            Self::ExclusionMargin(e) => {
                CssParsingError::ExclusionMargin(e.to_shared())
            }
            Self::HyphenationLanguage(e) => {
                CssParsingError::HyphenationLanguage(e.to_shared())
            }
            Self::LineHeight(e) => CssParsingError::LineHeight(e.clone()),
            Self::WordSpacing(e) => CssParsingError::WordSpacing(e.to_shared()),
            Self::TabSize(e) => CssParsingError::TabSize(e.to_shared()),
            Self::WhiteSpace(e) => CssParsingError::WhiteSpace(e.to_shared()),
            Self::Hyphens(e) => CssParsingError::Hyphens(e.to_shared()),
            Self::WordBreak(e) => CssParsingError::WordBreak(e.to_shared()),
            Self::OverflowWrap(e) => CssParsingError::OverflowWrap(e.to_shared()),
            Self::LineBreak(e) => CssParsingError::LineBreak(e.to_shared()),
            Self::ObjectFit(e) => CssParsingError::ObjectFit(e.to_shared()),
            Self::ObjectPosition(e) => {
                CssParsingError::ObjectPosition(e.to_shared())
            }
            Self::AspectRatio(e) => CssParsingError::AspectRatio(e.to_shared()),
            Self::TextOrientation(e) => {
                CssParsingError::TextOrientation(e.to_shared())
            }
            Self::TextAlignLast(e) => CssParsingError::TextAlignLast(e.to_shared()),
            Self::TextTransform(e) => CssParsingError::TextTransform(e.to_shared()),
            Self::Direction(e) => CssParsingError::Direction(e.to_shared()),
            Self::UserSelect(e) => CssParsingError::UserSelect(e.to_shared()),
            Self::TextDecoration(e) => {
                CssParsingError::TextDecoration(e.to_shared())
            }
            Self::Cursor(e) => CssParsingError::Cursor(e.to_shared()),
            Self::LayoutDisplay(e) => CssParsingError::LayoutDisplay(e.to_shared()),
            Self::LayoutFloat(e) => CssParsingError::LayoutFloat(e.to_shared()),
            Self::LayoutBoxSizing(e) => {
                CssParsingError::LayoutBoxSizing(e.to_shared())
            }
            // DTP properties...
            Self::PageBreak(e) => CssParsingError::PageBreak(e.to_shared()),
            Self::BreakInside(e) => CssParsingError::BreakInside(e.to_shared()),
            Self::Widows(e) => CssParsingError::Widows(e.to_shared()),
            Self::Orphans(e) => CssParsingError::Orphans(e.to_shared()),
            Self::BoxDecorationBreak(e) => {
                CssParsingError::BoxDecorationBreak(e.to_shared())
            }
            Self::ColumnCount(e) => CssParsingError::ColumnCount(e.to_shared()),
            Self::ColumnWidth(e) => CssParsingError::ColumnWidth(e.to_shared()),
            Self::ColumnSpan(e) => CssParsingError::ColumnSpan(e.to_shared()),
            Self::ColumnFill(e) => CssParsingError::ColumnFill(e.to_shared()),
            Self::ColumnRuleWidth(e) => {
                CssParsingError::ColumnRuleWidth(e.to_shared())
            }
            Self::ColumnRuleStyle(e) => {
                CssParsingError::ColumnRuleStyle(e.to_shared())
            }
            Self::ColumnRuleColor(e) => {
                CssParsingError::ColumnRuleColor(e.to_shared())
            }
            Self::FlowInto(e) => CssParsingError::FlowInto(e.to_shared()),
            Self::FlowFrom(e) => CssParsingError::FlowFrom(e.to_shared()),
            Self::GenericParseError => CssParsingError::GenericParseError,
            Self::Content => CssParsingError::Content,
            Self::Counter => CssParsingError::Counter,
            Self::ListStyleType(e) => CssParsingError::ListStyleType(e.to_shared()),
            Self::ListStylePosition(e) => {
                CssParsingError::ListStylePosition(e.to_shared())
            }
            Self::StringSet => CssParsingError::StringSet,
            Self::FontWeight(e) => CssParsingError::FontWeight(e.to_shared()),
            Self::FontStyle(e) => CssParsingError::FontStyle(e.to_shared()),
            Self::VerticalAlign(e) => CssParsingError::VerticalAlign(e.to_shared()),
        }
    }
}

#[cfg(feature = "parser")]
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `css-property` value.
pub fn parse_css_property(
    key: CssPropertyType,
    value: &str,
) -> Result<CssProperty, CssParsingError<'_>> {
    use crate::props::style::{
        parse_selection_background_color, parse_selection_color, parse_selection_radius,
    };

    let value = value.trim();

    // For properties where "auto" or "none" is a valid typed value (not just the generic CSS
    // keyword), we must NOT intercept them here. Let the specific parser handle them.
    let has_typed_auto = matches!(
        key,
        CssPropertyType::Hyphens |      // hyphens: auto means StyleHyphens::Auto
        CssPropertyType::LineBreak |    // line-break: auto means StyleLineBreak::Auto
        CssPropertyType::TextAlignLast | // text-align-last: auto means StyleTextAlignLast::Auto
        CssPropertyType::OverflowX |
        CssPropertyType::OverflowY |
        CssPropertyType::OverflowBlock |
        CssPropertyType::OverflowInline |
        CssPropertyType::UserSelect | // user-select: auto is a typed value
        CssPropertyType::AspectRatio // aspect-ratio: auto means StyleAspectRatio::Auto
    );

    let has_typed_none = matches!(
        key,
        CssPropertyType::Hyphens |      // hyphens: none means StyleHyphens::None
        CssPropertyType::Display |      // display: none means LayoutDisplay::None
        CssPropertyType::UserSelect |
        CssPropertyType::Float |        // float: none means LayoutFloat::None
        CssPropertyType::TextDecoration | // text-decoration: none is a typed value
        CssPropertyType::ObjectFit // object-fit: none means StyleObjectFit::None
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
            CssPropertyType::UnicodeBidi => parse_style_unicode_bidi(value)?.into(),
            CssPropertyType::TextBoxTrim => parse_style_text_box_trim(value)?.into(),
            CssPropertyType::TextBoxEdge => parse_style_text_box_edge(value)?.into(),
            CssPropertyType::DominantBaseline => parse_style_dominant_baseline(value)?.into(),
            CssPropertyType::AlignmentBaseline => parse_style_alignment_baseline(value)?.into(),
            CssPropertyType::InitialLetterAlign => parse_style_initial_letter_align(value)?.into(),
            CssPropertyType::InitialLetterWrap => parse_style_initial_letter_wrap(value)?.into(),
            CssPropertyType::ScrollbarGutter => parse_style_scrollbar_gutter(value)?.into(),
            CssPropertyType::OverflowClipMargin => parse_style_overflow_clip_margin(value)?.into(),
            CssPropertyType::Clip => parse_clip_rect(value)?.into(),
            CssPropertyType::ExclusionMargin => parse_style_exclusion_margin(value)?.into(),
            CssPropertyType::HyphenationLanguage => parse_style_hyphenation_language(value)?.into(),
            CssPropertyType::LineHeight => parse_style_line_height(value)?.into(),
            CssPropertyType::WordSpacing => parse_style_word_spacing(value)?.into(),
            CssPropertyType::TabSize => parse_style_tab_size(value)?.into(),
            CssPropertyType::WhiteSpace => parse_style_white_space(value)?.into(),
            CssPropertyType::Hyphens => parse_style_hyphens(value)?.into(),
            CssPropertyType::WordBreak => parse_style_word_break(value)?.into(),
            CssPropertyType::OverflowWrap => parse_style_overflow_wrap(value)?.into(),
            CssPropertyType::LineBreak => parse_style_line_break(value)?.into(),
            CssPropertyType::ObjectFit => parse_style_object_fit(value)?.into(),
            CssPropertyType::ObjectPosition => parse_style_object_position(value)?.into(),
            CssPropertyType::AspectRatio => parse_style_aspect_ratio(value)?.into(),
            CssPropertyType::TextOrientation => parse_style_text_orientation(value)?.into(),
            CssPropertyType::TextAlignLast => parse_style_text_align_last(value)?.into(),
            CssPropertyType::TextTransform => parse_style_text_transform(value)?.into(),
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
                CssProperty::GridColumn(CssPropertyValue::Exact(parse_grid_placement(value)?))
            }
            CssPropertyType::GridRow => {
                CssProperty::GridRow(CssPropertyValue::Exact(parse_grid_placement(value)?))
            }
            CssPropertyType::GridTemplateAreas => {
                use crate::props::layout::grid::parse_grid_template_areas;
                let areas = parse_grid_template_areas(value)
                    .map_err(|()| CssParsingError::InvalidValue(InvalidValueErr(value)))?;
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
            CssPropertyType::OverflowBlock => {
                CssProperty::OverflowBlock(parse_layout_overflow(value)?.into())
            }
            CssPropertyType::OverflowInline => {
                CssProperty::OverflowInline(parse_layout_overflow(value)?.into())
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

            CssPropertyType::BoxShadowLeft => CssProperty::BoxShadowLeft(CssPropertyValue::Exact(
                BoxOrStatic::heap(parse_style_box_shadow(value)?),
            )),
            CssPropertyType::BoxShadowRight => CssProperty::BoxShadowRight(
                CssPropertyValue::Exact(BoxOrStatic::heap(parse_style_box_shadow(value)?)),
            ),
            CssPropertyType::BoxShadowTop => CssProperty::BoxShadowTop(CssPropertyValue::Exact(
                BoxOrStatic::heap(parse_style_box_shadow(value)?),
            )),
            CssPropertyType::BoxShadowBottom => CssProperty::BoxShadowBottom(
                CssPropertyValue::Exact(BoxOrStatic::heap(parse_style_box_shadow(value)?)),
            ),

            CssPropertyType::ScrollbarTrack => CssProperty::ScrollbarTrack(
                CssPropertyValue::Exact(parse_style_background_content(value)?),
            ),
            CssPropertyType::ScrollbarThumb => CssProperty::ScrollbarThumb(
                CssPropertyValue::Exact(parse_style_background_content(value)?),
            ),
            CssPropertyType::ScrollbarButton => CssProperty::ScrollbarButton(
                CssPropertyValue::Exact(parse_style_background_content(value)?),
            ),
            CssPropertyType::ScrollbarCorner => CssProperty::ScrollbarCorner(
                CssPropertyValue::Exact(parse_style_background_content(value)?),
            ),
            CssPropertyType::ScrollbarResizer => CssProperty::ScrollbarResizer(
                CssPropertyValue::Exact(parse_style_background_content(value)?),
            ),
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
            CssPropertyType::TextShadow => CssProperty::TextShadow(CssPropertyValue::Exact(
                BoxOrStatic::heap(parse_style_box_shadow(value)?),
            )),

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
            CssPropertyType::ShapeOutside => CssProperty::ShapeOutside(CssPropertyValue::Exact(
                parse_shape_outside(value).map_err(|_| CssParsingError::GenericParseError)?,
            )),
            CssPropertyType::ShapeInside => CssProperty::ShapeInside(CssPropertyValue::Exact(
                parse_shape_inside(value).map_err(|_| CssParsingError::GenericParseError)?,
            )),
            CssPropertyType::ClipPath => CssProperty::ClipPath(CssPropertyValue::Exact(
                parse_clip_path(value).map_err(|_| CssParsingError::GenericParseError)?,
            )),
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
                    .map_err(|()| CssParsingError::Content)?
                    .into(),
            ),
            CssPropertyType::CounterReset => CssProperty::CounterReset(
                parse_counter_reset(value)
                    .map_err(|()| CssParsingError::Counter)?
                    .into(),
            ),
            CssPropertyType::CounterIncrement => CssProperty::CounterIncrement(
                parse_counter_increment(value)
                    .map_err(|()| CssParsingError::Counter)?
                    .into(),
            ),
            CssPropertyType::ListStyleType => CssProperty::ListStyleType(
                parse_style_list_style_type(value)
                    .map_err(CssParsingError::ListStyleType)?
                    .into(),
            ),
            CssPropertyType::ListStylePosition => CssProperty::ListStylePosition(
                parse_style_list_style_position(value)
                    .map_err(CssParsingError::ListStylePosition)?
                    .into(),
            ),
            CssPropertyType::StringSet => CssProperty::StringSet(
                parse_string_set(value)
                    .map_err(|()| CssParsingError::StringSet)?
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
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `combined-css-property` value.
pub fn parse_combined_css_property(
    key: CombinedCssPropertyType,
    value: &str,
) -> Result<Vec<CssProperty>, CssParsingError<'_>> {
    use self::CombinedCssPropertyType::{BorderRadius, Overflow, Padding, Margin, Border, BorderLeft, BorderRight, BorderTop, BorderBottom, BorderColor, BorderStyle, BorderWidth, BoxShadow, BackgroundColor, BackgroundImage, Background, Flex, Grid, Gap, GridGap, Font, Columns, GridArea, ColumnRule, TextBox, InsetBlock, InsetInline};

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
        BackgroundColor | BackgroundImage | Background => {
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
        Gap | GridGap => {
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
        TextBox => {
            vec![CssPropertyType::TextBoxTrim, CssPropertyType::TextBoxEdge]
        }
        // +spec:writing-modes:798cca - inset-block/inset-inline shorthand expansion
        // In horizontal-tb (default), block axis = vertical, inline axis = horizontal.
        // First value = start side, second = end side; if omitted, second defaults to first.
        InsetBlock => {
            vec![CssPropertyType::Top, CssPropertyType::Bottom]
        }
        InsetInline => {
            vec![CssPropertyType::Left, CssPropertyType::Right]
        }
    };

    // For Overflow, "auto" is a typed value (LayoutOverflow::Auto), not the generic CSS keyword,
    // so we must not intercept it here and let the specific parser handle it below.
    let has_typed_auto = matches!(key, Overflow);
    let has_typed_none = false; // Currently no combined properties have typed "none"

    match value {
        "auto" if !has_typed_auto => return Ok(keys.into_iter().map(CssProperty::auto).collect()),
        "none" if !has_typed_none => return Ok(keys.into_iter().map(CssProperty::none).collect()),
        "initial" => {
            return Ok(keys.into_iter().map(CssProperty::initial).collect());
        }
        "inherit" => {
            return Ok(keys.into_iter().map(CssProperty::inherit).collect());
        }
        _ => {}
    }

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
        // +spec:overflow:ff5ea4 - overflow shorthand sets overflow-x and overflow-y; second value copied from first if omitted
        Overflow => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            match parts.len() {
                1 => {
                    let overflow = parse_layout_overflow(value)?;
                    Ok(vec![
                        CssProperty::OverflowX(overflow.into()),
                        CssProperty::OverflowY(overflow.into()),
                    ])
                }
                2 => {
                    let overflow_x = parse_layout_overflow(parts[0])?;
                    let overflow_y = parse_layout_overflow(parts[1])?;
                    Ok(vec![
                        CssProperty::OverflowX(overflow_x.into()),
                        CssProperty::OverflowY(overflow_y.into()),
                    ])
                }
                _ => Err(CssParsingError::InvalidValue(InvalidValueErr(value))),
            }
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
                CssProperty::BorderTopColor(StyleBorderTopColor { inner: colors.top }.into()),
                CssProperty::BorderRightColor(
                    StyleBorderRightColor {
                        inner: colors.right,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomColor(
                    StyleBorderBottomColor {
                        inner: colors.bottom,
                    }
                    .into(),
                ),
                CssProperty::BorderLeftColor(StyleBorderLeftColor { inner: colors.left }.into()),
            ])
        }
        BorderStyle => {
            let styles = parse_style_border_style(value)?;
            Ok(vec![
                CssProperty::BorderTopStyle(StyleBorderTopStyle { inner: styles.top }.into()),
                CssProperty::BorderRightStyle(
                    StyleBorderRightStyle {
                        inner: styles.right,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomStyle(
                    StyleBorderBottomStyle {
                        inner: styles.bottom,
                    }
                    .into(),
                ),
                CssProperty::BorderLeftStyle(StyleBorderLeftStyle { inner: styles.left }.into()),
            ])
        }
        BorderWidth => {
            let widths = parse_style_border_width(value)?;
            Ok(vec![
                CssProperty::BorderTopWidth(LayoutBorderTopWidth { inner: widths.top }.into()),
                CssProperty::BorderRightWidth(
                    LayoutBorderRightWidth {
                        inner: widths.right,
                    }
                    .into(),
                ),
                CssProperty::BorderBottomWidth(
                    LayoutBorderBottomWidth {
                        inner: widths.bottom,
                    }
                    .into(),
                ),
                CssProperty::BorderLeftWidth(LayoutBorderLeftWidth { inner: widths.left }.into()),
            ])
        }
        BoxShadow => {
            let box_shadow = parse_style_box_shadow(value)?;
            Ok(vec![
                CssProperty::BoxShadowLeft(CssPropertyValue::Exact(BoxOrStatic::heap(box_shadow))),
                CssProperty::BoxShadowRight(CssPropertyValue::Exact(BoxOrStatic::heap(box_shadow))),
                CssProperty::BoxShadowTop(CssPropertyValue::Exact(BoxOrStatic::heap(box_shadow))),
                CssProperty::BoxShadowBottom(CssPropertyValue::Exact(BoxOrStatic::heap(
                    box_shadow,
                ))),
            ])
        }
        BackgroundColor => {
            let color = parse_css_color(value)?;
            let vec: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            Ok(vec![CssProperty::BackgroundContent(
                CssPropertyValue::Exact(vec),
            )])
        }
        BackgroundImage => {
            let background_content = parse_style_background_content(value)?;
            let vec: StyleBackgroundContentVec = vec![background_content].into();
            Ok(vec![CssProperty::BackgroundContent(
                CssPropertyValue::Exact(vec),
            )])
        }
        Background => {
            let background_content = parse_style_background_content_multiple(value)?;
            Ok(vec![CssProperty::BackgroundContent(
                CssPropertyValue::Exact(background_content),
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
                // CSS spec: flex: <number> => grow: <number>, shrink: 1, basis: 0
                if let Ok(g) = parse_layout_flex_grow(parts[0]) {
                    return Ok(vec![
                        CssProperty::FlexGrow(g.into()),
                        CssProperty::FlexShrink(
                            LayoutFlexShrink {
                                inner: crate::props::basic::length::FloatValue::const_new(1),
                            }
                            .into(),
                        ),
                        CssProperty::FlexBasis(
                            LayoutFlexBasis::Exact(PixelValue::px(0.0))
                                .into(),
                        ),
                    ]);
                }
                if let Ok(b) = parse_layout_flex_basis(parts[0]) {
                    return Ok(vec![CssProperty::FlexBasis(b.into())]);
                }
            }
            if parts.len() == 2 {
                // CSS spec: flex: <number> <number> => grow, shrink, basis: 0
                // Try grow+shrink first (two unitless numbers)
                if let (Ok(g), Ok(s)) = (
                    parse_layout_flex_grow(parts[0]),
                    parse_layout_flex_shrink(parts[1]),
                ) {
                    return Ok(vec![
                        CssProperty::FlexGrow(g.into()),
                        CssProperty::FlexShrink(s.into()),
                        CssProperty::FlexBasis(
                            LayoutFlexBasis::Exact(PixelValue::px(0.0))
                                .into(),
                        ),
                    ]);
                }
                // CSS spec: flex: <number> <width> => grow, shrink: 1, basis: <width>
                if let (Ok(g), Ok(b)) = (
                    parse_layout_flex_grow(parts[0]),
                    parse_layout_flex_basis(parts[1]),
                ) {
                    return Ok(vec![
                        CssProperty::FlexGrow(g.into()),
                        CssProperty::FlexShrink(
                            LayoutFlexShrink {
                                inner: crate::props::basic::length::FloatValue::const_new(1),
                            }
                            .into(),
                        ),
                        CssProperty::FlexBasis(b.into()),
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
            Err(CssParsingError::InvalidValue(InvalidValueErr(value)))
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
                Ok(vec![
                    CssProperty::RowGap(LayoutRowGap { inner: g.inner }.into()),
                    CssProperty::ColumnGap(LayoutColumnGap { inner: g.inner }.into()),
                ])
            } else if parts.len() == 2 {
                let row = parse_layout_gap(parts[0])?;
                let col = parse_layout_gap(parts[1])?;
                Ok(vec![
                    CssProperty::RowGap(LayoutRowGap { inner: row.inner }.into()),
                    CssProperty::ColumnGap(LayoutColumnGap { inner: col.inner }.into()),
                ])
            } else {
                Err(CssParsingError::InvalidValue(InvalidValueErr(value)))
            }
        }
        GridGap => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            if parts.len() == 1 {
                let g = parse_layout_gap(parts[0])?;
                Ok(vec![
                    CssProperty::RowGap(LayoutRowGap { inner: g.inner }.into()),
                    CssProperty::ColumnGap(LayoutColumnGap { inner: g.inner }.into()),
                ])
            } else if parts.len() == 2 {
                let row = parse_layout_gap(parts[0])?;
                let col = parse_layout_gap(parts[1])?;
                Ok(vec![
                    CssProperty::RowGap(LayoutRowGap { inner: row.inner }.into()),
                    CssProperty::ColumnGap(LayoutColumnGap { inner: col.inner }.into()),
                ])
            } else {
                Err(CssParsingError::InvalidValue(InvalidValueErr(value)))
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
            let parts: Vec<&str> = value.split('/').map(str::trim).collect();
            let (row_start, col_start, row_end, col_end) = match parts.len() {
                1 => (parts[0], parts[0], parts[0], parts[0]),
                2 => (parts[0], parts[1], parts[0], parts[1]),
                3 => (parts[0], parts[1], parts[2], parts[1]),
                4 => (parts[0], parts[1], parts[2], parts[3]),
                _ => return Err(CssParsingError::InvalidValue(InvalidValueErr(value))),
            };
            let parse_line = |s: &str| -> Result<GridLine, CssParsingError<'_>> {
                parse_grid_line_owned(s.trim())
                    .map_err(|()| CssParsingError::InvalidValue(InvalidValueErr(value)))
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
        // +spec:overflow:33aaf7 - text-box shorthand: "normal" sets trim=none/edge=auto,
        // omitting trim defaults to "both", omitting edge defaults to "auto"
        TextBox => {
            let trimmed = value.trim();
            if trimmed == "normal" {
                return Ok(vec![
                    CssProperty::TextBoxTrim(CssPropertyValue::Exact(StyleTextBoxTrim::None)),
                    CssProperty::TextBoxEdge(CssPropertyValue::Exact(StyleTextBoxEdge::Auto)),
                ]);
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            let mut trim_val = None;
            let mut edge_val = None;
            for part in &parts {
                if let Ok(t) = parse_style_text_box_trim(part) {
                    trim_val = Some(t);
                } else if let Ok(e) = parse_style_text_box_edge(part) {
                    edge_val = Some(e);
                } else {
                    return Err(CssParsingError::InvalidValue(InvalidValueErr(value)));
                }
            }
            // Per spec: omitting trim defaults to "both" (not the initial "none")
            let trim = trim_val.unwrap_or(StyleTextBoxTrim::TrimBoth);
            // Per spec: omitting edge defaults to "auto" (the initial value)
            let edge = edge_val.unwrap_or(StyleTextBoxEdge::Auto);
            Ok(vec![
                CssProperty::TextBoxTrim(CssPropertyValue::Exact(trim)),
                CssProperty::TextBoxEdge(CssPropertyValue::Exact(edge)),
            ])
        }
        // +spec:writing-modes:798cca - inset-block shorthand: first value = start, second = end;
        // if omitted, second defaults to first. Maps to top/bottom in horizontal-tb.
        InsetBlock => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            let start_val = parts
                .first()
                .ok_or(CssParsingError::InvalidValue(InvalidValueErr(value)))?;
            let end_val = parts.get(1).unwrap_or(start_val);
            let start = parse_layout_top(start_val)?;
            let end = parse_layout_bottom(end_val)?;
            Ok(vec![
                CssProperty::Top(start.into()),
                CssProperty::Bottom(end.into()),
            ])
        }
        // +spec:writing-modes:798cca - inset-inline shorthand: first value = start, second = end;
        // if omitted, second defaults to first. Maps to left/right in horizontal-tb.
        InsetInline => {
            let parts: Vec<&str> = value.split_whitespace().collect();
            let start_val = parts
                .first()
                .ok_or(CssParsingError::InvalidValue(InvalidValueErr(value)))?;
            let end_val = parts.get(1).unwrap_or(start_val);
            let start = parse_layout_left(start_val)?;
            let end = parse_layout_right(end_val)?;
            Ok(vec![
                CssProperty::Left(start.into()),
                CssProperty::Right(end.into()),
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
impl_from_css_prop!(StyleUnicodeBidi, CssProperty::UnicodeBidi);
impl_from_css_prop!(StyleTextBoxTrim, CssProperty::TextBoxTrim);
impl_from_css_prop!(StyleTextBoxEdge, CssProperty::TextBoxEdge);
impl_from_css_prop!(StyleDominantBaseline, CssProperty::DominantBaseline);
impl_from_css_prop!(StyleAlignmentBaseline, CssProperty::AlignmentBaseline);
impl_from_css_prop!(StyleInitialLetterAlign, CssProperty::InitialLetterAlign);
impl_from_css_prop!(StyleInitialLetterWrap, CssProperty::InitialLetterWrap);
impl_from_css_prop!(StyleScrollbarGutter, CssProperty::ScrollbarGutter);
impl_from_css_prop!(StyleOverflowClipMargin, CssProperty::OverflowClipMargin);
impl_from_css_prop!(StyleClipRect, CssProperty::Clip);
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

// BackgroundContent uses the standard From pattern
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
impl_from_css_prop!(StyleWordBreak, CssProperty::WordBreak);
impl_from_css_prop!(StyleOverflowWrap, CssProperty::OverflowWrap);
impl_from_css_prop!(StyleLineBreak, CssProperty::LineBreak);
impl_from_css_prop!(StyleObjectFit, CssProperty::ObjectFit);
impl_from_css_prop!(StyleObjectPosition, CssProperty::ObjectPosition);
impl_from_css_prop!(StyleAspectRatio, CssProperty::AspectRatio);
impl_from_css_prop!(StyleTextOrientation, CssProperty::TextOrientation);
impl_from_css_prop!(StyleTextAlignLast, CssProperty::TextAlignLast);
impl_from_css_prop!(StyleTextTransform, CssProperty::TextTransform);
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
    #[must_use] pub const fn key(&self) -> &'static str {
        self.get_type().to_str()
    }

    // Every arm delegates to `v.get_css_value_fmt()`, but each `v` is a different
    // `CssPropertyValue<T>` — the identical bodies cannot merge into one or-pattern
    // (mismatched binding types), so clippy::match_same_arms is a false positive here.
    #[allow(clippy::match_same_arms)]
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
    #[must_use] pub fn value(&self) -> String {
        match self {
            Self::CaretColor(v) => v.get_css_value_fmt(),
            Self::CaretWidth(v) => v.get_css_value_fmt(),
            Self::CaretAnimationDuration(v) => v.get_css_value_fmt(),
            Self::SelectionBackgroundColor(v) => v.get_css_value_fmt(),
            Self::SelectionColor(v) => v.get_css_value_fmt(),
            Self::SelectionRadius(v) => v.get_css_value_fmt(),
            Self::TextJustify(v) => v.get_css_value_fmt(),
            Self::TextColor(v) => v.get_css_value_fmt(),
            Self::FontSize(v) => v.get_css_value_fmt(),
            Self::FontFamily(v) => v.get_css_value_fmt(),
            Self::TextAlign(v) => v.get_css_value_fmt(),
            Self::LetterSpacing(v) => v.get_css_value_fmt(),
            Self::TextIndent(v) => v.get_css_value_fmt(),
            Self::InitialLetter(v) => v.get_css_value_fmt(),
            Self::LineClamp(v) => v.get_css_value_fmt(),
            Self::HangingPunctuation(v) => v.get_css_value_fmt(),
            Self::TextCombineUpright(v) => v.get_css_value_fmt(),
            Self::UnicodeBidi(v) => v.get_css_value_fmt(),
            Self::TextBoxTrim(v) => v.get_css_value_fmt(),
            Self::TextBoxEdge(v) => v.get_css_value_fmt(),
            Self::DominantBaseline(v) => v.get_css_value_fmt(),
            Self::AlignmentBaseline(v) => v.get_css_value_fmt(),
            Self::InitialLetterAlign(v) => v.get_css_value_fmt(),
            Self::InitialLetterWrap(v) => v.get_css_value_fmt(),
            Self::ScrollbarGutter(v) => v.get_css_value_fmt(),
            Self::OverflowClipMargin(v) => v.get_css_value_fmt(),
            Self::Clip(v) => v.get_css_value_fmt(),
            Self::ExclusionMargin(v) => v.get_css_value_fmt(),
            Self::HyphenationLanguage(v) => v.get_css_value_fmt(),
            Self::LineHeight(v) => v.get_css_value_fmt(),
            Self::WordSpacing(v) => v.get_css_value_fmt(),
            Self::TabSize(v) => v.get_css_value_fmt(),
            Self::Cursor(v) => v.get_css_value_fmt(),
            Self::Display(v) => v.get_css_value_fmt(),
            Self::Float(v) => v.get_css_value_fmt(),
            Self::BoxSizing(v) => v.get_css_value_fmt(),
            Self::Width(v) => v.get_css_value_fmt(),
            Self::Height(v) => v.get_css_value_fmt(),
            Self::MinWidth(v) => v.get_css_value_fmt(),
            Self::MinHeight(v) => v.get_css_value_fmt(),
            Self::MaxWidth(v) => v.get_css_value_fmt(),
            Self::MaxHeight(v) => v.get_css_value_fmt(),
            Self::Position(v) => v.get_css_value_fmt(),
            Self::Top(v) => v.get_css_value_fmt(),
            Self::Right(v) => v.get_css_value_fmt(),
            Self::Left(v) => v.get_css_value_fmt(),
            Self::Bottom(v) => v.get_css_value_fmt(),
            Self::ZIndex(v) => v.get_css_value_fmt(),
            Self::FlexWrap(v) => v.get_css_value_fmt(),
            Self::FlexDirection(v) => v.get_css_value_fmt(),
            Self::FlexGrow(v) => v.get_css_value_fmt(),
            Self::FlexShrink(v) => v.get_css_value_fmt(),
            Self::FlexBasis(v) => v.get_css_value_fmt(),
            Self::JustifyContent(v) => v.get_css_value_fmt(),
            Self::AlignItems(v) => v.get_css_value_fmt(),
            Self::AlignContent(v) => v.get_css_value_fmt(),
            Self::ColumnGap(v) => v.get_css_value_fmt(),
            Self::RowGap(v) => v.get_css_value_fmt(),
            Self::GridTemplateColumns(v) => v.get_css_value_fmt(),
            Self::GridTemplateRows(v) => v.get_css_value_fmt(),
            Self::GridAutoFlow(v) => v.get_css_value_fmt(),
            Self::JustifySelf(v) => v.get_css_value_fmt(),
            Self::JustifyItems(v) => v.get_css_value_fmt(),
            Self::Gap(v) => v.get_css_value_fmt(),
            Self::GridGap(v) => v.get_css_value_fmt(),
            Self::AlignSelf(v) => v.get_css_value_fmt(),
            Self::Font(v) => v.get_css_value_fmt(),
            Self::GridAutoColumns(v) => v.get_css_value_fmt(),
            Self::GridAutoRows(v) => v.get_css_value_fmt(),
            Self::GridColumn(v) => v.get_css_value_fmt(),
            Self::GridRow(v) => v.get_css_value_fmt(),
            Self::GridTemplateAreas(v) => v.get_css_value_fmt(),
            Self::WritingMode(v) => v.get_css_value_fmt(),
            Self::Clear(v) => v.get_css_value_fmt(),
            Self::BackgroundContent(v) => v.get_css_value_fmt(),
            Self::BackgroundPosition(v) => v.get_css_value_fmt(),
            Self::BackgroundSize(v) => v.get_css_value_fmt(),
            Self::BackgroundRepeat(v) => v.get_css_value_fmt(),
            Self::OverflowX(v) => v.get_css_value_fmt(),
            Self::OverflowY(v) => v.get_css_value_fmt(),
            Self::OverflowBlock(v) => v.get_css_value_fmt(),
            Self::OverflowInline(v) => v.get_css_value_fmt(),
            Self::PaddingTop(v) => v.get_css_value_fmt(),
            Self::PaddingLeft(v) => v.get_css_value_fmt(),
            Self::PaddingRight(v) => v.get_css_value_fmt(),
            Self::PaddingBottom(v) => v.get_css_value_fmt(),
            Self::PaddingInlineStart(v) => v.get_css_value_fmt(),
            Self::PaddingInlineEnd(v) => v.get_css_value_fmt(),
            Self::MarginTop(v) => v.get_css_value_fmt(),
            Self::MarginLeft(v) => v.get_css_value_fmt(),
            Self::MarginRight(v) => v.get_css_value_fmt(),
            Self::MarginBottom(v) => v.get_css_value_fmt(),
            Self::BorderTopLeftRadius(v) => v.get_css_value_fmt(),
            Self::BorderTopRightRadius(v) => v.get_css_value_fmt(),
            Self::BorderBottomLeftRadius(v) => v.get_css_value_fmt(),
            Self::BorderBottomRightRadius(v) => v.get_css_value_fmt(),
            Self::BorderTopColor(v) => v.get_css_value_fmt(),
            Self::BorderRightColor(v) => v.get_css_value_fmt(),
            Self::BorderLeftColor(v) => v.get_css_value_fmt(),
            Self::BorderBottomColor(v) => v.get_css_value_fmt(),
            Self::BorderTopStyle(v) => v.get_css_value_fmt(),
            Self::BorderRightStyle(v) => v.get_css_value_fmt(),
            Self::BorderLeftStyle(v) => v.get_css_value_fmt(),
            Self::BorderBottomStyle(v) => v.get_css_value_fmt(),
            Self::BorderTopWidth(v) => v.get_css_value_fmt(),
            Self::BorderRightWidth(v) => v.get_css_value_fmt(),
            Self::BorderLeftWidth(v) => v.get_css_value_fmt(),
            Self::BorderBottomWidth(v) => v.get_css_value_fmt(),
            Self::BoxShadowLeft(v) => v.get_css_value_fmt(),
            Self::BoxShadowRight(v) => v.get_css_value_fmt(),
            Self::BoxShadowTop(v) => v.get_css_value_fmt(),
            Self::BoxShadowBottom(v) => v.get_css_value_fmt(),
            Self::ScrollbarTrack(v) => v.get_css_value_fmt(),
            Self::ScrollbarThumb(v) => v.get_css_value_fmt(),
            Self::ScrollbarButton(v) => v.get_css_value_fmt(),
            Self::ScrollbarCorner(v) => v.get_css_value_fmt(),
            Self::ScrollbarResizer(v) => v.get_css_value_fmt(),
            Self::ScrollbarWidth(v) => v.get_css_value_fmt(),
            Self::ScrollbarColor(v) => v.get_css_value_fmt(),
            Self::ScrollbarVisibility(v) => v.get_css_value_fmt(),
            Self::ScrollbarFadeDelay(v) => v.get_css_value_fmt(),
            Self::ScrollbarFadeDuration(v) => v.get_css_value_fmt(),
            Self::Opacity(v) => v.get_css_value_fmt(),
            Self::Visibility(v) => v.get_css_value_fmt(),
            Self::Transform(v) => v.get_css_value_fmt(),
            Self::TransformOrigin(v) => v.get_css_value_fmt(),
            Self::PerspectiveOrigin(v) => v.get_css_value_fmt(),
            Self::BackfaceVisibility(v) => v.get_css_value_fmt(),
            Self::MixBlendMode(v) => v.get_css_value_fmt(),
            Self::Filter(v) => v.get_css_value_fmt(),
            Self::BackdropFilter(v) => v.get_css_value_fmt(),
            Self::TextShadow(v) => v.get_css_value_fmt(),
            Self::Hyphens(v) => v.get_css_value_fmt(),
            Self::WordBreak(v) => v.get_css_value_fmt(),
            Self::OverflowWrap(v) => v.get_css_value_fmt(),
            Self::LineBreak(v) => v.get_css_value_fmt(),
            Self::ObjectFit(v) => v.get_css_value_fmt(),
            Self::ObjectPosition(v) => v.get_css_value_fmt(),
            Self::AspectRatio(v) => v.get_css_value_fmt(),
            Self::TextOrientation(v) => v.get_css_value_fmt(),
            Self::TextAlignLast(v) => v.get_css_value_fmt(),
            Self::TextTransform(v) => v.get_css_value_fmt(),
            Self::Direction(v) => v.get_css_value_fmt(),
            Self::UserSelect(v) => v.get_css_value_fmt(),
            Self::TextDecoration(v) => v.get_css_value_fmt(),
            Self::WhiteSpace(v) => v.get_css_value_fmt(),
            Self::BreakBefore(v) => v.get_css_value_fmt(),
            Self::BreakAfter(v) => v.get_css_value_fmt(),
            Self::BreakInside(v) => v.get_css_value_fmt(),
            Self::Orphans(v) => v.get_css_value_fmt(),
            Self::Widows(v) => v.get_css_value_fmt(),
            Self::BoxDecorationBreak(v) => v.get_css_value_fmt(),
            Self::ColumnCount(v) => v.get_css_value_fmt(),
            Self::ColumnWidth(v) => v.get_css_value_fmt(),
            Self::ColumnSpan(v) => v.get_css_value_fmt(),
            Self::ColumnFill(v) => v.get_css_value_fmt(),
            Self::ColumnRuleWidth(v) => v.get_css_value_fmt(),
            Self::ColumnRuleStyle(v) => v.get_css_value_fmt(),
            Self::ColumnRuleColor(v) => v.get_css_value_fmt(),
            Self::FlowInto(v) => v.get_css_value_fmt(),
            Self::FlowFrom(v) => v.get_css_value_fmt(),
            Self::ShapeOutside(v) => v.get_css_value_fmt(),
            Self::ShapeInside(v) => v.get_css_value_fmt(),
            Self::ClipPath(v) => v.get_css_value_fmt(),
            Self::ShapeMargin(v) => v.get_css_value_fmt(),
            Self::ShapeImageThreshold(v) => v.get_css_value_fmt(),
            Self::Content(v) => v.get_css_value_fmt(),
            Self::CounterReset(v) => v.get_css_value_fmt(),
            Self::CounterIncrement(v) => v.get_css_value_fmt(),
            Self::ListStyleType(v) => v.get_css_value_fmt(),
            Self::ListStylePosition(v) => v.get_css_value_fmt(),
            Self::StringSet(v) => v.get_css_value_fmt(),
            Self::TableLayout(v) => v.get_css_value_fmt(),
            Self::BorderCollapse(v) => v.get_css_value_fmt(),
            Self::BorderSpacing(v) => v.get_css_value_fmt(),
            Self::CaptionSide(v) => v.get_css_value_fmt(),
            Self::EmptyCells(v) => v.get_css_value_fmt(),
            Self::FontWeight(v) => v.get_css_value_fmt(),
            Self::FontStyle(v) => v.get_css_value_fmt(),
            Self::VerticalAlign(v) => v.get_css_value_fmt(),
        }
    }

    #[must_use] pub fn format_css(&self) -> String {
        format!("{}: {};", self.key(), self.value())
    }

    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
    #[must_use] pub fn interpolate(
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
        let t: f32 = interpolate_resolver.interpolate_func.evaluate(f64::from(t));

        let t = t.clamp(0.0, 1.0);

        match (self, other) {
            (Self::TextColor(col_start), Self::TextColor(col_end)) => {
                let col_start = col_start.get_property().copied().unwrap_or_default();
                let col_end = col_end.get_property().copied().unwrap_or_default();
                Self::text_color(col_start.interpolate(&col_end, t))
            }
            (Self::FontSize(fs_start), Self::FontSize(fs_end)) => {
                let fs_start = fs_start.get_property().copied().unwrap_or_default();
                let fs_end = fs_end.get_property().copied().unwrap_or_default();
                Self::font_size(fs_start.interpolate(&fs_end, t))
            }
            (Self::LetterSpacing(ls_start), Self::LetterSpacing(ls_end)) => {
                let ls_start = ls_start.get_property().copied().unwrap_or_default();
                let ls_end = ls_end.get_property().copied().unwrap_or_default();
                Self::letter_spacing(ls_start.interpolate(&ls_end, t))
            }
            (Self::TextIndent(ti_start), Self::TextIndent(ti_end)) => {
                let ti_start = ti_start.get_property().copied().unwrap_or_default();
                let ti_end = ti_end.get_property().copied().unwrap_or_default();
                Self::text_indent(ti_start.interpolate(&ti_end, t))
            }
            (Self::LineHeight(lh_start), Self::LineHeight(lh_end)) => {
                let lh_start = lh_start.get_property().copied().unwrap_or_default();
                let lh_end = lh_end.get_property().copied().unwrap_or_default();
                Self::line_height(lh_start.interpolate(&lh_end, t))
            }
            (Self::WordSpacing(ws_start), Self::WordSpacing(ws_end)) => {
                let ws_start = ws_start.get_property().copied().unwrap_or_default();
                let ws_end = ws_end.get_property().copied().unwrap_or_default();
                Self::word_spacing(ws_start.interpolate(&ws_end, t))
            }
            (Self::TabSize(tw_start), Self::TabSize(tw_end)) => {
                let tw_start = tw_start.get_property().copied().unwrap_or_default();
                let tw_end = tw_end.get_property().copied().unwrap_or_default();
                Self::tab_size(tw_start.interpolate(&tw_end, t))
            }
            (Self::Width(start), Self::Width(end)) => {
                let start =
                    start
                        .get_property()
                        .cloned()
                        .unwrap_or(LayoutWidth::Px(PixelValue::px(
                            interpolate_resolver.current_rect_width,
                        )));
                let end = end.get_property().cloned().unwrap_or_default();
                Self::Width(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::Height(start), Self::Height(end)) => {
                let start =
                    start
                        .get_property()
                        .cloned()
                        .unwrap_or(LayoutHeight::Px(PixelValue::px(
                            interpolate_resolver.current_rect_height,
                        )));
                let end = end.get_property().cloned().unwrap_or_default();
                Self::Height(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::MinWidth(start), Self::MinWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::MinWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::MinHeight(start), Self::MinHeight(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::MinHeight(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::MaxWidth(start), Self::MaxWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::MaxWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::MaxHeight(start), Self::MaxHeight(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::MaxHeight(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::Top(start), Self::Top(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::Top(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::Right(start), Self::Right(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::Right(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::Left(start), Self::Left(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::Left(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::Bottom(start), Self::Bottom(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::Bottom(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::FlexGrow(start), Self::FlexGrow(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::FlexGrow(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::FlexShrink(start), Self::FlexShrink(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::FlexShrink(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::PaddingTop(start), Self::PaddingTop(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::PaddingTop(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::PaddingLeft(start), Self::PaddingLeft(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::PaddingLeft(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::PaddingRight(start), Self::PaddingRight(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::PaddingRight(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::PaddingBottom(start), Self::PaddingBottom(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::PaddingBottom(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::MarginTop(start), Self::MarginTop(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::MarginTop(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::MarginLeft(start), Self::MarginLeft(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::MarginLeft(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::MarginRight(start), Self::MarginRight(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::MarginRight(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::MarginBottom(start), Self::MarginBottom(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::MarginBottom(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::BorderTopLeftRadius(start), Self::BorderTopLeftRadius(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::BorderTopLeftRadius(CssPropertyValue::Exact(
                    start.interpolate(&end, t),
                ))
            }
            (Self::BorderTopRightRadius(start), Self::BorderTopRightRadius(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::BorderTopRightRadius(CssPropertyValue::Exact(
                    start.interpolate(&end, t),
                ))
            }
            (
                Self::BorderBottomLeftRadius(start),
                Self::BorderBottomLeftRadius(end),
            ) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::BorderBottomLeftRadius(CssPropertyValue::Exact(
                    start.interpolate(&end, t),
                ))
            }
            (
                Self::BorderBottomRightRadius(start),
                Self::BorderBottomRightRadius(end),
            ) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::BorderBottomRightRadius(CssPropertyValue::Exact(
                    start.interpolate(&end, t),
                ))
            }
            (Self::BorderTopColor(start), Self::BorderTopColor(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::BorderTopColor(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::BorderRightColor(start), Self::BorderRightColor(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::BorderRightColor(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::BorderLeftColor(start), Self::BorderLeftColor(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::BorderLeftColor(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::BorderBottomColor(start), Self::BorderBottomColor(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::BorderBottomColor(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::BorderTopWidth(start), Self::BorderTopWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::BorderTopWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::BorderRightWidth(start), Self::BorderRightWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::BorderRightWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::BorderLeftWidth(start), Self::BorderLeftWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::BorderLeftWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::BorderBottomWidth(start), Self::BorderBottomWidth(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::BorderBottomWidth(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::Opacity(start), Self::Opacity(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::Opacity(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::TransformOrigin(start), Self::TransformOrigin(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::TransformOrigin(CssPropertyValue::Exact(start.interpolate(&end, t)))
            }
            (Self::PerspectiveOrigin(start), Self::PerspectiveOrigin(end)) => {
                let start = start.get_property().copied().unwrap_or_default();
                let end = end.get_property().copied().unwrap_or_default();
                Self::PerspectiveOrigin(CssPropertyValue::Exact(start.interpolate(&end, t)))
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
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
    #[must_use] pub const fn get_type(&self) -> CssPropertyType {
        match &self {
            Self::CaretColor(_) => CssPropertyType::CaretColor,
            Self::CaretWidth(_) => CssPropertyType::CaretWidth,
            Self::CaretAnimationDuration(_) => CssPropertyType::CaretAnimationDuration,
            Self::SelectionBackgroundColor(_) => CssPropertyType::SelectionBackgroundColor,
            Self::SelectionColor(_) => CssPropertyType::SelectionColor,
            Self::SelectionRadius(_) => CssPropertyType::SelectionRadius,

            Self::TextJustify(_) => CssPropertyType::TextJustify,
            Self::TextColor(_) => CssPropertyType::TextColor,
            Self::FontSize(_) => CssPropertyType::FontSize,
            Self::FontFamily(_) => CssPropertyType::FontFamily,
            Self::FontWeight(_) => CssPropertyType::FontWeight,
            Self::FontStyle(_) => CssPropertyType::FontStyle,
            Self::TextAlign(_) => CssPropertyType::TextAlign,
            Self::VerticalAlign(_) => CssPropertyType::VerticalAlign,
            Self::LetterSpacing(_) => CssPropertyType::LetterSpacing,
            Self::TextIndent(_) => CssPropertyType::TextIndent,
            Self::InitialLetter(_) => CssPropertyType::InitialLetter,
            Self::LineClamp(_) => CssPropertyType::LineClamp,
            Self::HangingPunctuation(_) => CssPropertyType::HangingPunctuation,
            Self::TextCombineUpright(_) => CssPropertyType::TextCombineUpright,
            Self::UnicodeBidi(_) => CssPropertyType::UnicodeBidi,
            Self::TextBoxTrim(_) => CssPropertyType::TextBoxTrim,
            Self::TextBoxEdge(_) => CssPropertyType::TextBoxEdge,
            Self::DominantBaseline(_) => CssPropertyType::DominantBaseline,
            Self::AlignmentBaseline(_) => CssPropertyType::AlignmentBaseline,
            Self::InitialLetterAlign(_) => CssPropertyType::InitialLetterAlign,
            Self::InitialLetterWrap(_) => CssPropertyType::InitialLetterWrap,
            Self::ScrollbarGutter(_) => CssPropertyType::ScrollbarGutter,
            Self::OverflowClipMargin(_) => CssPropertyType::OverflowClipMargin,
            Self::Clip(_) => CssPropertyType::Clip,
            Self::ExclusionMargin(_) => CssPropertyType::ExclusionMargin,
            Self::HyphenationLanguage(_) => CssPropertyType::HyphenationLanguage,
            Self::LineHeight(_) => CssPropertyType::LineHeight,
            Self::WordSpacing(_) => CssPropertyType::WordSpacing,
            Self::TabSize(_) => CssPropertyType::TabSize,
            Self::Cursor(_) => CssPropertyType::Cursor,
            Self::Display(_) => CssPropertyType::Display,
            Self::Float(_) => CssPropertyType::Float,
            Self::BoxSizing(_) => CssPropertyType::BoxSizing,
            Self::Width(_) => CssPropertyType::Width,
            Self::Height(_) => CssPropertyType::Height,
            Self::MinWidth(_) => CssPropertyType::MinWidth,
            Self::MinHeight(_) => CssPropertyType::MinHeight,
            Self::MaxWidth(_) => CssPropertyType::MaxWidth,
            Self::MaxHeight(_) => CssPropertyType::MaxHeight,
            Self::Position(_) => CssPropertyType::Position,
            Self::Top(_) => CssPropertyType::Top,
            Self::Right(_) => CssPropertyType::Right,
            Self::Left(_) => CssPropertyType::Left,
            Self::Bottom(_) => CssPropertyType::Bottom,
            Self::ZIndex(_) => CssPropertyType::ZIndex,
            Self::FlexWrap(_) => CssPropertyType::FlexWrap,
            Self::FlexDirection(_) => CssPropertyType::FlexDirection,
            Self::FlexGrow(_) => CssPropertyType::FlexGrow,
            Self::FlexShrink(_) => CssPropertyType::FlexShrink,
            Self::FlexBasis(_) => CssPropertyType::FlexBasis,
            Self::JustifyContent(_) => CssPropertyType::JustifyContent,
            Self::AlignItems(_) => CssPropertyType::AlignItems,
            Self::AlignContent(_) => CssPropertyType::AlignContent,
            Self::ColumnGap(_) => CssPropertyType::ColumnGap,
            Self::RowGap(_) => CssPropertyType::RowGap,
            Self::GridTemplateColumns(_) => CssPropertyType::GridTemplateColumns,
            Self::GridTemplateRows(_) => CssPropertyType::GridTemplateRows,
            Self::GridAutoColumns(_) => CssPropertyType::GridAutoColumns,
            Self::GridAutoRows(_) => CssPropertyType::GridAutoRows,
            Self::GridColumn(_) => CssPropertyType::GridColumn,
            Self::GridAutoFlow(_) => CssPropertyType::GridAutoFlow,
            Self::JustifySelf(_) => CssPropertyType::JustifySelf,
            Self::JustifyItems(_) => CssPropertyType::JustifyItems,
            Self::Gap(_) => CssPropertyType::Gap,
            Self::GridGap(_) => CssPropertyType::GridGap,
            Self::AlignSelf(_) => CssPropertyType::AlignSelf,
            Self::Font(_) => CssPropertyType::Font,
            Self::GridRow(_) => CssPropertyType::GridRow,
            Self::GridTemplateAreas(_) => CssPropertyType::GridTemplateAreas,
            Self::WritingMode(_) => CssPropertyType::WritingMode,
            Self::Clear(_) => CssPropertyType::Clear,
            Self::BackgroundContent(_) => CssPropertyType::BackgroundContent,
            Self::BackgroundPosition(_) => CssPropertyType::BackgroundPosition,
            Self::BackgroundSize(_) => CssPropertyType::BackgroundSize,
            Self::BackgroundRepeat(_) => CssPropertyType::BackgroundRepeat,
            Self::OverflowX(_) => CssPropertyType::OverflowX,
            Self::OverflowY(_) => CssPropertyType::OverflowY,
            Self::OverflowBlock(_) => CssPropertyType::OverflowBlock,
            Self::OverflowInline(_) => CssPropertyType::OverflowInline,
            Self::PaddingTop(_) => CssPropertyType::PaddingTop,
            Self::PaddingLeft(_) => CssPropertyType::PaddingLeft,
            Self::PaddingRight(_) => CssPropertyType::PaddingRight,
            Self::PaddingBottom(_) => CssPropertyType::PaddingBottom,
            Self::PaddingInlineStart(_) => CssPropertyType::PaddingInlineStart,
            Self::PaddingInlineEnd(_) => CssPropertyType::PaddingInlineEnd,
            Self::MarginTop(_) => CssPropertyType::MarginTop,
            Self::MarginLeft(_) => CssPropertyType::MarginLeft,
            Self::MarginRight(_) => CssPropertyType::MarginRight,
            Self::MarginBottom(_) => CssPropertyType::MarginBottom,
            Self::BorderTopLeftRadius(_) => CssPropertyType::BorderTopLeftRadius,
            Self::BorderTopRightRadius(_) => CssPropertyType::BorderTopRightRadius,
            Self::BorderBottomLeftRadius(_) => CssPropertyType::BorderBottomLeftRadius,
            Self::BorderBottomRightRadius(_) => CssPropertyType::BorderBottomRightRadius,
            Self::BorderTopColor(_) => CssPropertyType::BorderTopColor,
            Self::BorderRightColor(_) => CssPropertyType::BorderRightColor,
            Self::BorderLeftColor(_) => CssPropertyType::BorderLeftColor,
            Self::BorderBottomColor(_) => CssPropertyType::BorderBottomColor,
            Self::BorderTopStyle(_) => CssPropertyType::BorderTopStyle,
            Self::BorderRightStyle(_) => CssPropertyType::BorderRightStyle,
            Self::BorderLeftStyle(_) => CssPropertyType::BorderLeftStyle,
            Self::BorderBottomStyle(_) => CssPropertyType::BorderBottomStyle,
            Self::BorderTopWidth(_) => CssPropertyType::BorderTopWidth,
            Self::BorderRightWidth(_) => CssPropertyType::BorderRightWidth,
            Self::BorderLeftWidth(_) => CssPropertyType::BorderLeftWidth,
            Self::BorderBottomWidth(_) => CssPropertyType::BorderBottomWidth,
            Self::BoxShadowLeft(_) => CssPropertyType::BoxShadowLeft,
            Self::BoxShadowRight(_) => CssPropertyType::BoxShadowRight,
            Self::BoxShadowTop(_) => CssPropertyType::BoxShadowTop,
            Self::BoxShadowBottom(_) => CssPropertyType::BoxShadowBottom,
            Self::ScrollbarTrack(_) => CssPropertyType::ScrollbarTrack,
            Self::ScrollbarThumb(_) => CssPropertyType::ScrollbarThumb,
            Self::ScrollbarButton(_) => CssPropertyType::ScrollbarButton,
            Self::ScrollbarCorner(_) => CssPropertyType::ScrollbarCorner,
            Self::ScrollbarResizer(_) => CssPropertyType::ScrollbarResizer,
            Self::ScrollbarWidth(_) => CssPropertyType::ScrollbarWidth,
            Self::ScrollbarColor(_) => CssPropertyType::ScrollbarColor,
            Self::ScrollbarVisibility(_) => CssPropertyType::ScrollbarVisibility,
            Self::ScrollbarFadeDelay(_) => CssPropertyType::ScrollbarFadeDelay,
            Self::ScrollbarFadeDuration(_) => CssPropertyType::ScrollbarFadeDuration,
            Self::Opacity(_) => CssPropertyType::Opacity,
            Self::Visibility(_) => CssPropertyType::Visibility,
            Self::Transform(_) => CssPropertyType::Transform,
            Self::PerspectiveOrigin(_) => CssPropertyType::PerspectiveOrigin,
            Self::TransformOrigin(_) => CssPropertyType::TransformOrigin,
            Self::BackfaceVisibility(_) => CssPropertyType::BackfaceVisibility,
            Self::MixBlendMode(_) => CssPropertyType::MixBlendMode,
            Self::Filter(_) => CssPropertyType::Filter,
            Self::BackdropFilter(_) => CssPropertyType::BackdropFilter,
            Self::TextShadow(_) => CssPropertyType::TextShadow,
            Self::WhiteSpace(_) => CssPropertyType::WhiteSpace,
            Self::Hyphens(_) => CssPropertyType::Hyphens,
            Self::WordBreak(_) => CssPropertyType::WordBreak,
            Self::OverflowWrap(_) => CssPropertyType::OverflowWrap,
            Self::LineBreak(_) => CssPropertyType::LineBreak,
            Self::ObjectFit(_) => CssPropertyType::ObjectFit,
            Self::ObjectPosition(_) => CssPropertyType::ObjectPosition,
            Self::AspectRatio(_) => CssPropertyType::AspectRatio,
            Self::TextOrientation(_) => CssPropertyType::TextOrientation,
            Self::TextAlignLast(_) => CssPropertyType::TextAlignLast,
            Self::TextTransform(_) => CssPropertyType::TextTransform,
            Self::Direction(_) => CssPropertyType::Direction,
            Self::UserSelect(_) => CssPropertyType::UserSelect,
            Self::TextDecoration(_) => CssPropertyType::TextDecoration,
            Self::BreakBefore(_) => CssPropertyType::BreakBefore,
            Self::BreakAfter(_) => CssPropertyType::BreakAfter,
            Self::BreakInside(_) => CssPropertyType::BreakInside,
            Self::Orphans(_) => CssPropertyType::Orphans,
            Self::Widows(_) => CssPropertyType::Widows,
            Self::BoxDecorationBreak(_) => CssPropertyType::BoxDecorationBreak,
            Self::ColumnCount(_) => CssPropertyType::ColumnCount,
            Self::ColumnWidth(_) => CssPropertyType::ColumnWidth,
            Self::ColumnSpan(_) => CssPropertyType::ColumnSpan,
            Self::ColumnFill(_) => CssPropertyType::ColumnFill,
            Self::ColumnRuleWidth(_) => CssPropertyType::ColumnRuleWidth,
            Self::ColumnRuleStyle(_) => CssPropertyType::ColumnRuleStyle,
            Self::ColumnRuleColor(_) => CssPropertyType::ColumnRuleColor,
            Self::FlowInto(_) => CssPropertyType::FlowInto,
            Self::FlowFrom(_) => CssPropertyType::FlowFrom,
            Self::ShapeOutside(_) => CssPropertyType::ShapeOutside,
            Self::ShapeInside(_) => CssPropertyType::ShapeInside,
            Self::ClipPath(_) => CssPropertyType::ClipPath,
            Self::ShapeMargin(_) => CssPropertyType::ShapeMargin,
            Self::ShapeImageThreshold(_) => CssPropertyType::ShapeImageThreshold,
            Self::Content(_) => CssPropertyType::Content,
            Self::CounterReset(_) => CssPropertyType::CounterReset,
            Self::CounterIncrement(_) => CssPropertyType::CounterIncrement,
            Self::ListStyleType(_) => CssPropertyType::ListStyleType,
            Self::ListStylePosition(_) => CssPropertyType::ListStylePosition,
            Self::StringSet(_) => CssPropertyType::StringSet,
            Self::TableLayout(_) => CssPropertyType::TableLayout,
            Self::BorderCollapse(_) => CssPropertyType::BorderCollapse,
            Self::BorderSpacing(_) => CssPropertyType::BorderSpacing,
            Self::CaptionSide(_) => CssPropertyType::CaptionSide,
            Self::EmptyCells(_) => CssPropertyType::EmptyCells,
        }
    }

    // const constructors for easier API access

    #[must_use] pub const fn none(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, None)
    }
    #[must_use] pub const fn auto(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Auto)
    }
    #[must_use] pub const fn initial(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Initial)
    }
    #[must_use] pub const fn inherit(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Inherit)
    }

    #[must_use] pub const fn text_color(input: StyleTextColor) -> Self {
        Self::TextColor(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn font_size(input: StyleFontSize) -> Self {
        Self::FontSize(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn font_family(input: StyleFontFamilyVec) -> Self {
        Self::FontFamily(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn font_weight(input: StyleFontWeight) -> Self {
        Self::FontWeight(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn font_style(input: StyleFontStyle) -> Self {
        Self::FontStyle(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn text_align(input: StyleTextAlign) -> Self {
        Self::TextAlign(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn text_justify(input: LayoutTextJustify) -> Self {
        Self::TextJustify(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn vertical_align(input: StyleVerticalAlign) -> Self {
        Self::VerticalAlign(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn letter_spacing(input: StyleLetterSpacing) -> Self {
        Self::LetterSpacing(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn text_indent(input: StyleTextIndent) -> Self {
        Self::TextIndent(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn line_height(input: StyleLineHeight) -> Self {
        Self::LineHeight(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn word_spacing(input: StyleWordSpacing) -> Self {
        Self::WordSpacing(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn tab_size(input: StyleTabSize) -> Self {
        Self::TabSize(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn cursor(input: StyleCursor) -> Self {
        Self::Cursor(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn user_select(input: StyleUserSelect) -> Self {
        Self::UserSelect(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn text_decoration(input: StyleTextDecoration) -> Self {
        Self::TextDecoration(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn display(input: LayoutDisplay) -> Self {
        Self::Display(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn box_sizing(input: LayoutBoxSizing) -> Self {
        Self::BoxSizing(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn width(input: LayoutWidth) -> Self {
        Self::Width(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn height(input: LayoutHeight) -> Self {
        Self::Height(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn min_width(input: LayoutMinWidth) -> Self {
        Self::MinWidth(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn caret_color(input: CaretColor) -> Self {
        Self::CaretColor(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn caret_width(input: CaretWidth) -> Self {
        Self::CaretWidth(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn caret_animation_duration(input: CaretAnimationDuration) -> Self {
        Self::CaretAnimationDuration(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn selection_background_color(input: SelectionBackgroundColor) -> Self {
        Self::SelectionBackgroundColor(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn selection_color(input: SelectionColor) -> Self {
        Self::SelectionColor(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn min_height(input: LayoutMinHeight) -> Self {
        Self::MinHeight(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn max_width(input: LayoutMaxWidth) -> Self {
        Self::MaxWidth(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn max_height(input: LayoutMaxHeight) -> Self {
        Self::MaxHeight(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn position(input: LayoutPosition) -> Self {
        Self::Position(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn top(input: LayoutTop) -> Self {
        Self::Top(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn right(input: LayoutRight) -> Self {
        Self::Right(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn left(input: LayoutLeft) -> Self {
        Self::Left(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn bottom(input: LayoutInsetBottom) -> Self {
        Self::Bottom(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn z_index(input: LayoutZIndex) -> Self {
        Self::ZIndex(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn flex_wrap(input: LayoutFlexWrap) -> Self {
        Self::FlexWrap(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn flex_direction(input: LayoutFlexDirection) -> Self {
        Self::FlexDirection(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn flex_grow(input: LayoutFlexGrow) -> Self {
        Self::FlexGrow(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn flex_shrink(input: LayoutFlexShrink) -> Self {
        Self::FlexShrink(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn justify_content(input: LayoutJustifyContent) -> Self {
        Self::JustifyContent(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn grid_auto_flow(input: LayoutGridAutoFlow) -> Self {
        Self::GridAutoFlow(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn justify_self(input: LayoutJustifySelf) -> Self {
        Self::JustifySelf(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn justify_items(input: LayoutJustifyItems) -> Self {
        Self::JustifyItems(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn gap(input: LayoutGap) -> Self {
        Self::Gap(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn grid_gap(input: LayoutGap) -> Self {
        Self::GridGap(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn align_self(input: LayoutAlignSelf) -> Self {
        Self::AlignSelf(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn font(input: StyleFontFamilyVec) -> Self {
        Self::Font(StyleFontValue::Exact(input))
    }
    #[must_use] pub const fn align_items(input: LayoutAlignItems) -> Self {
        Self::AlignItems(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn align_content(input: LayoutAlignContent) -> Self {
        Self::AlignContent(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn background_content(input: StyleBackgroundContentVec) -> Self {
        Self::BackgroundContent(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn background_position(input: StyleBackgroundPositionVec) -> Self {
        Self::BackgroundPosition(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn background_size(input: StyleBackgroundSizeVec) -> Self {
        Self::BackgroundSize(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn background_repeat(input: StyleBackgroundRepeatVec) -> Self {
        Self::BackgroundRepeat(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn overflow_x(input: LayoutOverflow) -> Self {
        Self::OverflowX(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn overflow_y(input: LayoutOverflow) -> Self {
        Self::OverflowY(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn overflow_block(input: LayoutOverflow) -> Self {
        Self::OverflowBlock(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn overflow_inline(input: LayoutOverflow) -> Self {
        Self::OverflowInline(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn padding_top(input: LayoutPaddingTop) -> Self {
        Self::PaddingTop(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn padding_left(input: LayoutPaddingLeft) -> Self {
        Self::PaddingLeft(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn padding_right(input: LayoutPaddingRight) -> Self {
        Self::PaddingRight(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn padding_bottom(input: LayoutPaddingBottom) -> Self {
        Self::PaddingBottom(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn margin_top(input: LayoutMarginTop) -> Self {
        Self::MarginTop(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn margin_left(input: LayoutMarginLeft) -> Self {
        Self::MarginLeft(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn margin_right(input: LayoutMarginRight) -> Self {
        Self::MarginRight(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn margin_bottom(input: LayoutMarginBottom) -> Self {
        Self::MarginBottom(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_top_left_radius(input: StyleBorderTopLeftRadius) -> Self {
        Self::BorderTopLeftRadius(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_top_right_radius(input: StyleBorderTopRightRadius) -> Self {
        Self::BorderTopRightRadius(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_bottom_left_radius(input: StyleBorderBottomLeftRadius) -> Self {
        Self::BorderBottomLeftRadius(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_bottom_right_radius(input: StyleBorderBottomRightRadius) -> Self {
        Self::BorderBottomRightRadius(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_top_color(input: StyleBorderTopColor) -> Self {
        Self::BorderTopColor(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_right_color(input: StyleBorderRightColor) -> Self {
        Self::BorderRightColor(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_left_color(input: StyleBorderLeftColor) -> Self {
        Self::BorderLeftColor(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_bottom_color(input: StyleBorderBottomColor) -> Self {
        Self::BorderBottomColor(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_top_style(input: StyleBorderTopStyle) -> Self {
        Self::BorderTopStyle(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_right_style(input: StyleBorderRightStyle) -> Self {
        Self::BorderRightStyle(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_left_style(input: StyleBorderLeftStyle) -> Self {
        Self::BorderLeftStyle(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_bottom_style(input: StyleBorderBottomStyle) -> Self {
        Self::BorderBottomStyle(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_top_width(input: LayoutBorderTopWidth) -> Self {
        Self::BorderTopWidth(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_right_width(input: LayoutBorderRightWidth) -> Self {
        Self::BorderRightWidth(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_left_width(input: LayoutBorderLeftWidth) -> Self {
        Self::BorderLeftWidth(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_bottom_width(input: LayoutBorderBottomWidth) -> Self {
        Self::BorderBottomWidth(CssPropertyValue::Exact(input))
    }
    #[must_use] pub fn box_shadow_left(input: StyleBoxShadow) -> Self {
        Self::BoxShadowLeft(CssPropertyValue::Exact(BoxOrStatic::heap(input)))
    }
    #[must_use] pub fn box_shadow_right(input: StyleBoxShadow) -> Self {
        Self::BoxShadowRight(CssPropertyValue::Exact(BoxOrStatic::heap(input)))
    }
    #[must_use] pub fn box_shadow_top(input: StyleBoxShadow) -> Self {
        Self::BoxShadowTop(CssPropertyValue::Exact(BoxOrStatic::heap(input)))
    }
    #[must_use] pub fn box_shadow_bottom(input: StyleBoxShadow) -> Self {
        Self::BoxShadowBottom(CssPropertyValue::Exact(BoxOrStatic::heap(input)))
    }
    #[must_use] pub const fn opacity(input: StyleOpacity) -> Self {
        Self::Opacity(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn visibility(input: StyleVisibility) -> Self {
        Self::Visibility(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn transform(input: StyleTransformVec) -> Self {
        Self::Transform(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn transform_origin(input: StyleTransformOrigin) -> Self {
        Self::TransformOrigin(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn perspective_origin(input: StylePerspectiveOrigin) -> Self {
        Self::PerspectiveOrigin(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn backface_visibility(input: StyleBackfaceVisibility) -> Self {
        Self::BackfaceVisibility(CssPropertyValue::Exact(input))
    }

    // New DTP const fn constructors
    #[must_use] pub const fn break_before(input: PageBreak) -> Self {
        Self::BreakBefore(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn break_after(input: PageBreak) -> Self {
        Self::BreakAfter(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn break_inside(input: BreakInside) -> Self {
        Self::BreakInside(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn orphans(input: Orphans) -> Self {
        Self::Orphans(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn widows(input: Widows) -> Self {
        Self::Widows(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn box_decoration_break(input: BoxDecorationBreak) -> Self {
        Self::BoxDecorationBreak(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn column_count(input: ColumnCount) -> Self {
        Self::ColumnCount(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn column_width(input: ColumnWidth) -> Self {
        Self::ColumnWidth(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn column_span(input: ColumnSpan) -> Self {
        Self::ColumnSpan(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn column_fill(input: ColumnFill) -> Self {
        Self::ColumnFill(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn column_rule_width(input: ColumnRuleWidth) -> Self {
        Self::ColumnRuleWidth(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn column_rule_style(input: ColumnRuleStyle) -> Self {
        Self::ColumnRuleStyle(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn column_rule_color(input: ColumnRuleColor) -> Self {
        Self::ColumnRuleColor(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn flow_into(input: FlowInto) -> Self {
        Self::FlowInto(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn flow_from(input: FlowFrom) -> Self {
        Self::FlowFrom(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn shape_outside(input: ShapeOutside) -> Self {
        Self::ShapeOutside(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn shape_inside(input: ShapeInside) -> Self {
        Self::ShapeInside(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn clip_path(input: ClipPath) -> Self {
        Self::ClipPath(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn shape_margin(input: ShapeMargin) -> Self {
        Self::ShapeMargin(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn shape_image_threshold(input: ShapeImageThreshold) -> Self {
        Self::ShapeImageThreshold(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn content(input: Content) -> Self {
        Self::Content(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn counter_reset(input: CounterReset) -> Self {
        Self::CounterReset(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn counter_increment(input: CounterIncrement) -> Self {
        Self::CounterIncrement(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn list_style_type(input: StyleListStyleType) -> Self {
        Self::ListStyleType(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn list_style_position(input: StyleListStylePosition) -> Self {
        Self::ListStylePosition(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn string_set(input: StringSet) -> Self {
        Self::StringSet(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn table_layout(input: LayoutTableLayout) -> Self {
        Self::TableLayout(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_collapse(input: StyleBorderCollapse) -> Self {
        Self::BorderCollapse(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn border_spacing(input: LayoutBorderSpacing) -> Self {
        Self::BorderSpacing(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn caption_side(input: StyleCaptionSide) -> Self {
        Self::CaptionSide(CssPropertyValue::Exact(input))
    }
    #[must_use] pub const fn empty_cells(input: StyleEmptyCells) -> Self {
        Self::EmptyCells(CssPropertyValue::Exact(input))
    }

    #[must_use] pub const fn as_z_index(&self) -> Option<&LayoutZIndexValue> {
        match self {
            Self::ZIndex(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_flex_basis(&self) -> Option<&LayoutFlexBasisValue> {
        match self {
            Self::FlexBasis(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_column_gap(&self) -> Option<&LayoutColumnGapValue> {
        match self {
            Self::ColumnGap(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_row_gap(&self) -> Option<&LayoutRowGapValue> {
        match self {
            Self::RowGap(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_grid_template_columns(&self) -> Option<&LayoutGridTemplateColumnsValue> {
        match self {
            Self::GridTemplateColumns(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_grid_template_rows(&self) -> Option<&LayoutGridTemplateRowsValue> {
        match self {
            Self::GridTemplateRows(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_grid_auto_columns(&self) -> Option<&LayoutGridAutoColumnsValue> {
        match self {
            Self::GridAutoColumns(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_grid_auto_rows(&self) -> Option<&LayoutGridAutoRowsValue> {
        match self {
            Self::GridAutoRows(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_grid_column(&self) -> Option<&LayoutGridColumnValue> {
        match self {
            Self::GridColumn(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_grid_row(&self) -> Option<&LayoutGridRowValue> {
        match self {
            Self::GridRow(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_writing_mode(&self) -> Option<&LayoutWritingModeValue> {
        match self {
            Self::WritingMode(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_clear(&self) -> Option<&LayoutClearValue> {
        match self {
            Self::Clear(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_scrollbar_track(&self) -> Option<&StyleBackgroundContentValue> {
        match self {
            Self::ScrollbarTrack(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_scrollbar_thumb(&self) -> Option<&StyleBackgroundContentValue> {
        match self {
            Self::ScrollbarThumb(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_scrollbar_button(&self) -> Option<&StyleBackgroundContentValue> {
        match self {
            Self::ScrollbarButton(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_scrollbar_corner(&self) -> Option<&StyleBackgroundContentValue> {
        match self {
            Self::ScrollbarCorner(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_scrollbar_resizer(&self) -> Option<&StyleBackgroundContentValue> {
        match self {
            Self::ScrollbarResizer(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_visibility(&self) -> Option<&StyleVisibilityValue> {
        match self {
            Self::Visibility(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_background_content(&self) -> Option<&StyleBackgroundContentVecValue> {
        match self {
            Self::BackgroundContent(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_text_justify(&self) -> Option<&LayoutTextJustifyValue> {
        match self {
            Self::TextJustify(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_caret_color(&self) -> Option<&CaretColorValue> {
        match self {
            Self::CaretColor(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_caret_width(&self) -> Option<&CaretWidthValue> {
        match self {
            Self::CaretWidth(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_caret_animation_duration(&self) -> Option<&CaretAnimationDurationValue> {
        match self {
            Self::CaretAnimationDuration(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_selection_background_color(&self) -> Option<&SelectionBackgroundColorValue> {
        match self {
            Self::SelectionBackgroundColor(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_selection_color(&self) -> Option<&SelectionColorValue> {
        match self {
            Self::SelectionColor(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_selection_radius(&self) -> Option<&SelectionRadiusValue> {
        match self {
            Self::SelectionRadius(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_background_position(&self) -> Option<&StyleBackgroundPositionVecValue> {
        match self {
            Self::BackgroundPosition(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_background_size(&self) -> Option<&StyleBackgroundSizeVecValue> {
        match self {
            Self::BackgroundSize(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_background_repeat(&self) -> Option<&StyleBackgroundRepeatVecValue> {
        match self {
            Self::BackgroundRepeat(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_grid_auto_flow(&self) -> Option<&LayoutGridAutoFlowValue> {
        match self {
            Self::GridAutoFlow(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_justify_self(&self) -> Option<&LayoutJustifySelfValue> {
        match self {
            Self::JustifySelf(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_justify_items(&self) -> Option<&LayoutJustifyItemsValue> {
        match self {
            Self::JustifyItems(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_gap(&self) -> Option<&LayoutGapValue> {
        match self {
            Self::Gap(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_grid_gap(&self) -> Option<&LayoutGapValue> {
        match self {
            Self::GridGap(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_align_self(&self) -> Option<&LayoutAlignSelfValue> {
        match self {
            Self::AlignSelf(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_font(&self) -> Option<&StyleFontValue> {
        match self {
            Self::Font(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_font_size(&self) -> Option<&StyleFontSizeValue> {
        match self {
            Self::FontSize(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_font_family(&self) -> Option<&StyleFontFamilyVecValue> {
        match self {
            Self::FontFamily(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_font_weight(&self) -> Option<&StyleFontWeightValue> {
        match self {
            Self::FontWeight(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_font_style(&self) -> Option<&StyleFontStyleValue> {
        match self {
            Self::FontStyle(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_text_color(&self) -> Option<&StyleTextColorValue> {
        match self {
            Self::TextColor(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_text_align(&self) -> Option<&StyleTextAlignValue> {
        match self {
            Self::TextAlign(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_vertical_align(&self) -> Option<&StyleVerticalAlignValue> {
        match self {
            Self::VerticalAlign(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_line_height(&self) -> Option<&StyleLineHeightValue> {
        match self {
            Self::LineHeight(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_text_indent(&self) -> Option<&StyleTextIndentValue> {
        match self {
            Self::TextIndent(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_initial_letter(&self) -> Option<&StyleInitialLetterValue> {
        match self {
            Self::InitialLetter(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_line_clamp(&self) -> Option<&StyleLineClampValue> {
        match self {
            Self::LineClamp(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_hanging_punctuation(&self) -> Option<&StyleHangingPunctuationValue> {
        match self {
            Self::HangingPunctuation(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_text_combine_upright(&self) -> Option<&StyleTextCombineUprightValue> {
        match self {
            Self::TextCombineUpright(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_unicode_bidi(&self) -> Option<&StyleUnicodeBidiValue> {
        match self {
            Self::UnicodeBidi(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_text_box_trim(&self) -> Option<&StyleTextBoxTrimValue> {
        match self {
            Self::TextBoxTrim(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_text_box_edge(&self) -> Option<&StyleTextBoxEdgeValue> {
        match self {
            Self::TextBoxEdge(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_dominant_baseline(&self) -> Option<&StyleDominantBaselineValue> {
        match self {
            Self::DominantBaseline(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_alignment_baseline(&self) -> Option<&StyleAlignmentBaselineValue> {
        match self {
            Self::AlignmentBaseline(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_initial_letter_align(&self) -> Option<&StyleInitialLetterAlignValue> {
        match self {
            Self::InitialLetterAlign(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_initial_letter_wrap(&self) -> Option<&StyleInitialLetterWrapValue> {
        match self {
            Self::InitialLetterWrap(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_scrollbar_gutter(&self) -> Option<&StyleScrollbarGutterValue> {
        match self {
            Self::ScrollbarGutter(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_overflow_clip_margin(&self) -> Option<&StyleOverflowClipMarginValue> {
        match self {
            Self::OverflowClipMargin(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_clip(&self) -> Option<&StyleClipRectValue> {
        match self {
            Self::Clip(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_exclusion_margin(&self) -> Option<&StyleExclusionMarginValue> {
        match self {
            Self::ExclusionMargin(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_hyphenation_language(&self) -> Option<&StyleHyphenationLanguageValue> {
        match self {
            Self::HyphenationLanguage(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_letter_spacing(&self) -> Option<&StyleLetterSpacingValue> {
        match self {
            Self::LetterSpacing(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_word_spacing(&self) -> Option<&StyleWordSpacingValue> {
        match self {
            Self::WordSpacing(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_tab_size(&self) -> Option<&StyleTabSizeValue> {
        match self {
            Self::TabSize(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_cursor(&self) -> Option<&StyleCursorValue> {
        match self {
            Self::Cursor(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_box_shadow_left(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            Self::BoxShadowLeft(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_box_shadow_right(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            Self::BoxShadowRight(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_box_shadow_top(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            Self::BoxShadowTop(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_box_shadow_bottom(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            Self::BoxShadowBottom(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_top_color(&self) -> Option<&StyleBorderTopColorValue> {
        match self {
            Self::BorderTopColor(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_left_color(&self) -> Option<&StyleBorderLeftColorValue> {
        match self {
            Self::BorderLeftColor(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_right_color(&self) -> Option<&StyleBorderRightColorValue> {
        match self {
            Self::BorderRightColor(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_bottom_color(&self) -> Option<&StyleBorderBottomColorValue> {
        match self {
            Self::BorderBottomColor(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_top_style(&self) -> Option<&StyleBorderTopStyleValue> {
        match self {
            Self::BorderTopStyle(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_left_style(&self) -> Option<&StyleBorderLeftStyleValue> {
        match self {
            Self::BorderLeftStyle(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_right_style(&self) -> Option<&StyleBorderRightStyleValue> {
        match self {
            Self::BorderRightStyle(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_bottom_style(&self) -> Option<&StyleBorderBottomStyleValue> {
        match self {
            Self::BorderBottomStyle(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_top_left_radius(&self) -> Option<&StyleBorderTopLeftRadiusValue> {
        match self {
            Self::BorderTopLeftRadius(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_top_right_radius(&self) -> Option<&StyleBorderTopRightRadiusValue> {
        match self {
            Self::BorderTopRightRadius(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_bottom_left_radius(&self) -> Option<&StyleBorderBottomLeftRadiusValue> {
        match self {
            Self::BorderBottomLeftRadius(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_bottom_right_radius(
        &self,
    ) -> Option<&StyleBorderBottomRightRadiusValue> {
        match self {
            Self::BorderBottomRightRadius(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_opacity(&self) -> Option<&StyleOpacityValue> {
        match self {
            Self::Opacity(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_transform(&self) -> Option<&StyleTransformVecValue> {
        match self {
            Self::Transform(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_transform_origin(&self) -> Option<&StyleTransformOriginValue> {
        match self {
            Self::TransformOrigin(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_perspective_origin(&self) -> Option<&StylePerspectiveOriginValue> {
        match self {
            Self::PerspectiveOrigin(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_backface_visibility(&self) -> Option<&StyleBackfaceVisibilityValue> {
        match self {
            Self::BackfaceVisibility(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_mix_blend_mode(&self) -> Option<&StyleMixBlendModeValue> {
        match self {
            Self::MixBlendMode(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_filter(&self) -> Option<&StyleFilterVecValue> {
        match self {
            Self::Filter(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_backdrop_filter(&self) -> Option<&StyleFilterVecValue> {
        match self {
            Self::BackdropFilter(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_text_shadow(&self) -> Option<&StyleBoxShadowValue> {
        match self {
            Self::TextShadow(f) => Some(f),
            _ => None,
        }
    }

    // functions that downcast to the concrete CSS type (layout)

    #[must_use] pub const fn as_display(&self) -> Option<&LayoutDisplayValue> {
        match self {
            Self::Display(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_float(&self) -> Option<&LayoutFloatValue> {
        match self {
            Self::Float(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_box_sizing(&self) -> Option<&LayoutBoxSizingValue> {
        match self {
            Self::BoxSizing(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_width(&self) -> Option<&LayoutWidthValue> {
        match self {
            Self::Width(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_height(&self) -> Option<&LayoutHeightValue> {
        match self {
            Self::Height(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_min_width(&self) -> Option<&LayoutMinWidthValue> {
        match self {
            Self::MinWidth(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_min_height(&self) -> Option<&LayoutMinHeightValue> {
        match self {
            Self::MinHeight(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_max_width(&self) -> Option<&LayoutMaxWidthValue> {
        match self {
            Self::MaxWidth(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_max_height(&self) -> Option<&LayoutMaxHeightValue> {
        match self {
            Self::MaxHeight(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_position(&self) -> Option<&LayoutPositionValue> {
        match self {
            Self::Position(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_top(&self) -> Option<&LayoutTopValue> {
        match self {
            Self::Top(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_bottom(&self) -> Option<&LayoutInsetBottomValue> {
        match self {
            Self::Bottom(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_right(&self) -> Option<&LayoutRightValue> {
        match self {
            Self::Right(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_left(&self) -> Option<&LayoutLeftValue> {
        match self {
            Self::Left(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_padding_top(&self) -> Option<&LayoutPaddingTopValue> {
        match self {
            Self::PaddingTop(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_padding_bottom(&self) -> Option<&LayoutPaddingBottomValue> {
        match self {
            Self::PaddingBottom(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_padding_left(&self) -> Option<&LayoutPaddingLeftValue> {
        match self {
            Self::PaddingLeft(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_padding_right(&self) -> Option<&LayoutPaddingRightValue> {
        match self {
            Self::PaddingRight(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_margin_top(&self) -> Option<&LayoutMarginTopValue> {
        match self {
            Self::MarginTop(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_margin_bottom(&self) -> Option<&LayoutMarginBottomValue> {
        match self {
            Self::MarginBottom(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_margin_left(&self) -> Option<&LayoutMarginLeftValue> {
        match self {
            Self::MarginLeft(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_margin_right(&self) -> Option<&LayoutMarginRightValue> {
        match self {
            Self::MarginRight(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_top_width(&self) -> Option<&LayoutBorderTopWidthValue> {
        match self {
            Self::BorderTopWidth(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_left_width(&self) -> Option<&LayoutBorderLeftWidthValue> {
        match self {
            Self::BorderLeftWidth(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_right_width(&self) -> Option<&LayoutBorderRightWidthValue> {
        match self {
            Self::BorderRightWidth(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_bottom_width(&self) -> Option<&LayoutBorderBottomWidthValue> {
        match self {
            Self::BorderBottomWidth(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_overflow_x(&self) -> Option<&LayoutOverflowValue> {
        match self {
            Self::OverflowX(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_overflow_y(&self) -> Option<&LayoutOverflowValue> {
        match self {
            Self::OverflowY(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_overflow_block(&self) -> Option<&LayoutOverflowValue> {
        match self {
            Self::OverflowBlock(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_overflow_inline(&self) -> Option<&LayoutOverflowValue> {
        match self {
            Self::OverflowInline(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_flex_direction(&self) -> Option<&LayoutFlexDirectionValue> {
        match self {
            Self::FlexDirection(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_direction(&self) -> Option<&StyleDirectionValue> {
        match self {
            Self::Direction(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_user_select(&self) -> Option<&StyleUserSelectValue> {
        match self {
            Self::UserSelect(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_text_decoration(&self) -> Option<&StyleTextDecorationValue> {
        match self {
            Self::TextDecoration(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_hyphens(&self) -> Option<&StyleHyphensValue> {
        match self {
            Self::Hyphens(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_word_break(&self) -> Option<&StyleWordBreakValue> {
        match self {
            Self::WordBreak(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_overflow_wrap(&self) -> Option<&StyleOverflowWrapValue> {
        match self {
            Self::OverflowWrap(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_line_break(&self) -> Option<&StyleLineBreakValue> {
        match self {
            Self::LineBreak(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_object_fit(&self) -> Option<&StyleObjectFitValue> {
        match self {
            Self::ObjectFit(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_object_position(&self) -> Option<&StyleObjectPositionValue> {
        match self {
            Self::ObjectPosition(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_aspect_ratio(&self) -> Option<&StyleAspectRatioValue> {
        match self {
            Self::AspectRatio(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_text_orientation(&self) -> Option<&StyleTextOrientationValue> {
        match self {
            Self::TextOrientation(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_text_transform(&self) -> Option<&StyleTextTransformValue> {
        match self {
            Self::TextTransform(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_text_align_last(&self) -> Option<&StyleTextAlignLastValue> {
        match self {
            Self::TextAlignLast(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_white_space(&self) -> Option<&StyleWhiteSpaceValue> {
        match self {
            Self::WhiteSpace(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_flex_wrap(&self) -> Option<&LayoutFlexWrapValue> {
        match self {
            Self::FlexWrap(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_flex_grow(&self) -> Option<&LayoutFlexGrowValue> {
        match self {
            Self::FlexGrow(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_flex_shrink(&self) -> Option<&LayoutFlexShrinkValue> {
        match self {
            Self::FlexShrink(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_justify_content(&self) -> Option<&LayoutJustifyContentValue> {
        match self {
            Self::JustifyContent(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_align_items(&self) -> Option<&LayoutAlignItemsValue> {
        match self {
            Self::AlignItems(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_align_content(&self) -> Option<&LayoutAlignContentValue> {
        match self {
            Self::AlignContent(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_break_before(&self) -> Option<&PageBreakValue> {
        match self {
            Self::BreakBefore(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_break_after(&self) -> Option<&PageBreakValue> {
        match self {
            Self::BreakAfter(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_break_inside(&self) -> Option<&BreakInsideValue> {
        match self {
            Self::BreakInside(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_orphans(&self) -> Option<&OrphansValue> {
        match self {
            Self::Orphans(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_widows(&self) -> Option<&WidowsValue> {
        match self {
            Self::Widows(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_box_decoration_break(&self) -> Option<&BoxDecorationBreakValue> {
        match self {
            Self::BoxDecorationBreak(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_column_count(&self) -> Option<&ColumnCountValue> {
        match self {
            Self::ColumnCount(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_column_width(&self) -> Option<&ColumnWidthValue> {
        match self {
            Self::ColumnWidth(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_column_span(&self) -> Option<&ColumnSpanValue> {
        match self {
            Self::ColumnSpan(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_column_fill(&self) -> Option<&ColumnFillValue> {
        match self {
            Self::ColumnFill(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_column_rule_width(&self) -> Option<&ColumnRuleWidthValue> {
        match self {
            Self::ColumnRuleWidth(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_column_rule_style(&self) -> Option<&ColumnRuleStyleValue> {
        match self {
            Self::ColumnRuleStyle(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_column_rule_color(&self) -> Option<&ColumnRuleColorValue> {
        match self {
            Self::ColumnRuleColor(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_flow_into(&self) -> Option<&FlowIntoValue> {
        match self {
            Self::FlowInto(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_flow_from(&self) -> Option<&FlowFromValue> {
        match self {
            Self::FlowFrom(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_shape_outside(&self) -> Option<&ShapeOutsideValue> {
        match self {
            Self::ShapeOutside(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_shape_inside(&self) -> Option<&ShapeInsideValue> {
        match self {
            Self::ShapeInside(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_clip_path(&self) -> Option<&ClipPathValue> {
        match self {
            Self::ClipPath(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_shape_margin(&self) -> Option<&ShapeMarginValue> {
        match self {
            Self::ShapeMargin(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_shape_image_threshold(&self) -> Option<&ShapeImageThresholdValue> {
        match self {
            Self::ShapeImageThreshold(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_content(&self) -> Option<&ContentValue> {
        match self {
            Self::Content(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_counter_reset(&self) -> Option<&CounterResetValue> {
        match self {
            Self::CounterReset(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_counter_increment(&self) -> Option<&CounterIncrementValue> {
        match self {
            Self::CounterIncrement(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_list_style_type(&self) -> Option<&StyleListStyleTypeValue> {
        match self {
            Self::ListStyleType(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_list_style_position(&self) -> Option<&StyleListStylePositionValue> {
        match self {
            Self::ListStylePosition(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_string_set(&self) -> Option<&StringSetValue> {
        match self {
            Self::StringSet(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_table_layout(&self) -> Option<&LayoutTableLayoutValue> {
        match self {
            Self::TableLayout(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_collapse(&self) -> Option<&StyleBorderCollapseValue> {
        match self {
            Self::BorderCollapse(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_border_spacing(&self) -> Option<&LayoutBorderSpacingValue> {
        match self {
            Self::BorderSpacing(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_caption_side(&self) -> Option<&StyleCaptionSideValue> {
        match self {
            Self::CaptionSide(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_empty_cells(&self) -> Option<&StyleEmptyCellsValue> {
        match self {
            Self::EmptyCells(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_scrollbar_width(&self) -> Option<&LayoutScrollbarWidthValue> {
        match self {
            Self::ScrollbarWidth(f) => Some(f),
            _ => None,
        }
    }
    #[must_use] pub const fn as_scrollbar_color(&self) -> Option<&StyleScrollbarColorValue> {
        match self {
            Self::ScrollbarColor(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_scrollbar_visibility(&self) -> Option<&ScrollbarVisibilityModeValue> {
        match self {
            Self::ScrollbarVisibility(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_scrollbar_fade_delay(&self) -> Option<&ScrollbarFadeDelayValue> {
        match self {
            Self::ScrollbarFadeDelay(f) => Some(f),
            _ => None,
        }
    }

    #[must_use] pub const fn as_scrollbar_fade_duration(&self) -> Option<&ScrollbarFadeDurationValue> {
        match self {
            Self::ScrollbarFadeDuration(f) => Some(f),
            _ => None,
        }
    }

    // Cross-type dispatch: each `c` is a different `CssPropertyValue<T>`, so the
    // identical `c.is_initial()` bodies can't merge (clippy::match_same_arms FP).
    #[allow(clippy::match_same_arms)]
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
    #[must_use] pub const fn is_initial(&self) -> bool {
        use self::CssProperty::{CaretColor, CaretWidth, CaretAnimationDuration, SelectionBackgroundColor, SelectionColor, SelectionRadius, TextJustify, TextColor, FontSize, FontFamily, TextAlign, LetterSpacing, TextIndent, InitialLetter, LineClamp, HangingPunctuation, TextCombineUpright, UnicodeBidi, TextBoxTrim, TextBoxEdge, DominantBaseline, AlignmentBaseline, InitialLetterAlign, InitialLetterWrap, ScrollbarGutter, OverflowClipMargin, Clip, ExclusionMargin, HyphenationLanguage, LineHeight, WordSpacing, TabSize, Cursor, Display, Float, BoxSizing, Width, Height, MinWidth, MinHeight, MaxWidth, MaxHeight, Position, Top, Right, Left, Bottom, ZIndex, FlexWrap, FlexDirection, FlexGrow, FlexShrink, FlexBasis, JustifyContent, AlignItems, AlignContent, ColumnGap, RowGap, GridTemplateColumns, GridTemplateRows, GridAutoFlow, JustifySelf, JustifyItems, Gap, GridGap, AlignSelf, Font, GridAutoColumns, GridAutoRows, GridColumn, GridRow, GridTemplateAreas, WritingMode, Clear, BackgroundContent, BackgroundPosition, BackgroundSize, BackgroundRepeat, OverflowX, OverflowY, OverflowBlock, OverflowInline, PaddingTop, PaddingLeft, PaddingRight, PaddingBottom, PaddingInlineStart, PaddingInlineEnd, MarginTop, MarginLeft, MarginRight, MarginBottom, BorderTopLeftRadius, BorderTopRightRadius, BorderBottomLeftRadius, BorderBottomRightRadius, BorderTopColor, BorderRightColor, BorderLeftColor, BorderBottomColor, BorderTopStyle, BorderRightStyle, BorderLeftStyle, BorderBottomStyle, BorderTopWidth, BorderRightWidth, BorderLeftWidth, BorderBottomWidth, BoxShadowLeft, BoxShadowRight, BoxShadowTop, BoxShadowBottom, ScrollbarTrack, ScrollbarThumb, ScrollbarButton, ScrollbarCorner, ScrollbarResizer, ScrollbarWidth, ScrollbarColor, ScrollbarVisibility, ScrollbarFadeDelay, ScrollbarFadeDuration, Opacity, Visibility, Transform, TransformOrigin, PerspectiveOrigin, BackfaceVisibility, MixBlendMode, Filter, BackdropFilter, TextShadow, WhiteSpace, Direction, UserSelect, TextDecoration, Hyphens, WordBreak, OverflowWrap, LineBreak, ObjectFit, ObjectPosition, AspectRatio, TextOrientation, TextAlignLast, TextTransform, BreakBefore, BreakAfter, BreakInside, Orphans, Widows, BoxDecorationBreak, ColumnCount, ColumnWidth, ColumnSpan, ColumnFill, ColumnRuleWidth, ColumnRuleStyle, ColumnRuleColor, FlowInto, FlowFrom, ShapeOutside, ShapeInside, ClipPath, ShapeMargin, ShapeImageThreshold, Content, CounterReset, CounterIncrement, ListStyleType, ListStylePosition, StringSet, TableLayout, BorderCollapse, BorderSpacing, CaptionSide, EmptyCells, FontWeight, FontStyle, VerticalAlign};
        match self {
            CaretColor(c) => c.is_initial(),
            CaretWidth(c) => c.is_initial(),
            CaretAnimationDuration(c) => c.is_initial(),
            SelectionBackgroundColor(c) => c.is_initial(),
            SelectionColor(c) => c.is_initial(),
            SelectionRadius(c) => c.is_initial(),
            TextJustify(c) => c.is_initial(),
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
            UnicodeBidi(c) => c.is_initial(),
            TextBoxTrim(c) => c.is_initial(),
            TextBoxEdge(c) => c.is_initial(),
            DominantBaseline(c) => c.is_initial(),
            AlignmentBaseline(c) => c.is_initial(),
            InitialLetterAlign(c) => c.is_initial(),
            InitialLetterWrap(c) => c.is_initial(),
            ScrollbarGutter(c) => c.is_initial(),
            OverflowClipMargin(c) => c.is_initial(),
            Clip(c) => c.is_initial(),
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
            OverflowBlock(c) => c.is_initial(),
            OverflowInline(c) => c.is_initial(),
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
            WordBreak(c) => c.is_initial(),
            OverflowWrap(c) => c.is_initial(),
            LineBreak(c) => c.is_initial(),
            ObjectFit(c) => c.is_initial(),
            ObjectPosition(c) => c.is_initial(),
            AspectRatio(c) => c.is_initial(),
            TextOrientation(c) => c.is_initial(),
            TextAlignLast(c) => c.is_initial(),
            TextTransform(c) => c.is_initial(),
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

    #[must_use] pub const fn const_none(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, None)
    }
    #[must_use] pub const fn const_auto(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Auto)
    }
    #[must_use] pub const fn const_initial(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Initial)
    }
    #[must_use] pub const fn const_inherit(prop_type: CssPropertyType) -> Self {
        css_property_from_type!(prop_type, Inherit)
    }

    #[must_use] pub const fn const_text_color(input: StyleTextColor) -> Self {
        Self::TextColor(StyleTextColorValue::Exact(input))
    }
    #[must_use] pub const fn const_font_size(input: StyleFontSize) -> Self {
        Self::FontSize(StyleFontSizeValue::Exact(input))
    }
    #[must_use] pub const fn const_font_family(input: StyleFontFamilyVec) -> Self {
        Self::FontFamily(StyleFontFamilyVecValue::Exact(input))
    }
    #[must_use] pub const fn const_text_align(input: StyleTextAlign) -> Self {
        Self::TextAlign(StyleTextAlignValue::Exact(input))
    }
    #[must_use] pub const fn const_vertical_align(input: StyleVerticalAlign) -> Self {
        Self::VerticalAlign(StyleVerticalAlignValue::Exact(input))
    }
    #[must_use] pub const fn const_letter_spacing(input: StyleLetterSpacing) -> Self {
        Self::LetterSpacing(StyleLetterSpacingValue::Exact(input))
    }
    #[must_use] pub const fn const_text_indent(input: StyleTextIndent) -> Self {
        Self::TextIndent(StyleTextIndentValue::Exact(input))
    }
    #[must_use] pub const fn const_line_height(input: StyleLineHeight) -> Self {
        Self::LineHeight(StyleLineHeightValue::Exact(input))
    }
    #[must_use] pub const fn const_word_spacing(input: StyleWordSpacing) -> Self {
        Self::WordSpacing(StyleWordSpacingValue::Exact(input))
    }
    #[must_use] pub const fn const_tab_size(input: StyleTabSize) -> Self {
        Self::TabSize(StyleTabSizeValue::Exact(input))
    }
    #[must_use] pub const fn const_cursor(input: StyleCursor) -> Self {
        Self::Cursor(StyleCursorValue::Exact(input))
    }
    #[must_use] pub const fn const_display(input: LayoutDisplay) -> Self {
        Self::Display(LayoutDisplayValue::Exact(input))
    }
    #[must_use] pub const fn const_float(input: LayoutFloat) -> Self {
        Self::Float(LayoutFloatValue::Exact(input))
    }
    #[must_use] pub const fn const_box_sizing(input: LayoutBoxSizing) -> Self {
        Self::BoxSizing(LayoutBoxSizingValue::Exact(input))
    }
    #[must_use] pub const fn const_width(input: LayoutWidth) -> Self {
        Self::Width(LayoutWidthValue::Exact(input))
    }
    #[must_use] pub const fn const_height(input: LayoutHeight) -> Self {
        Self::Height(LayoutHeightValue::Exact(input))
    }
    #[must_use] pub const fn const_min_width(input: LayoutMinWidth) -> Self {
        Self::MinWidth(LayoutMinWidthValue::Exact(input))
    }
    #[must_use] pub const fn const_min_height(input: LayoutMinHeight) -> Self {
        Self::MinHeight(LayoutMinHeightValue::Exact(input))
    }
    #[must_use] pub const fn const_max_width(input: LayoutMaxWidth) -> Self {
        Self::MaxWidth(LayoutMaxWidthValue::Exact(input))
    }
    #[must_use] pub const fn const_max_height(input: LayoutMaxHeight) -> Self {
        Self::MaxHeight(LayoutMaxHeightValue::Exact(input))
    }
    #[must_use] pub const fn const_position(input: LayoutPosition) -> Self {
        Self::Position(LayoutPositionValue::Exact(input))
    }
    #[must_use] pub const fn const_top(input: LayoutTop) -> Self {
        Self::Top(LayoutTopValue::Exact(input))
    }
    #[must_use] pub const fn const_right(input: LayoutRight) -> Self {
        Self::Right(LayoutRightValue::Exact(input))
    }
    #[must_use] pub const fn const_left(input: LayoutLeft) -> Self {
        Self::Left(LayoutLeftValue::Exact(input))
    }
    #[must_use] pub const fn const_bottom(input: LayoutInsetBottom) -> Self {
        Self::Bottom(LayoutInsetBottomValue::Exact(input))
    }
    #[must_use] pub const fn const_flex_wrap(input: LayoutFlexWrap) -> Self {
        Self::FlexWrap(LayoutFlexWrapValue::Exact(input))
    }
    #[must_use] pub const fn const_flex_direction(input: LayoutFlexDirection) -> Self {
        Self::FlexDirection(LayoutFlexDirectionValue::Exact(input))
    }
    #[must_use] pub const fn const_flex_grow(input: LayoutFlexGrow) -> Self {
        Self::FlexGrow(LayoutFlexGrowValue::Exact(input))
    }
    #[must_use] pub const fn const_flex_shrink(input: LayoutFlexShrink) -> Self {
        Self::FlexShrink(LayoutFlexShrinkValue::Exact(input))
    }
    #[must_use] pub const fn const_justify_content(input: LayoutJustifyContent) -> Self {
        Self::JustifyContent(LayoutJustifyContentValue::Exact(input))
    }
    #[must_use] pub const fn const_align_items(input: LayoutAlignItems) -> Self {
        Self::AlignItems(LayoutAlignItemsValue::Exact(input))
    }
    #[must_use] pub const fn const_align_content(input: LayoutAlignContent) -> Self {
        Self::AlignContent(LayoutAlignContentValue::Exact(input))
    }
    #[must_use] pub const fn const_background_content(input: StyleBackgroundContentVec) -> Self {
        Self::BackgroundContent(StyleBackgroundContentVecValue::Exact(input))
    }
    #[must_use] pub const fn const_background_position(input: StyleBackgroundPositionVec) -> Self {
        Self::BackgroundPosition(StyleBackgroundPositionVecValue::Exact(input))
    }
    #[must_use] pub const fn const_background_size(input: StyleBackgroundSizeVec) -> Self {
        Self::BackgroundSize(StyleBackgroundSizeVecValue::Exact(input))
    }
    #[must_use] pub const fn const_background_repeat(input: StyleBackgroundRepeatVec) -> Self {
        Self::BackgroundRepeat(StyleBackgroundRepeatVecValue::Exact(input))
    }
    #[must_use] pub const fn const_overflow_x(input: LayoutOverflow) -> Self {
        Self::OverflowX(LayoutOverflowValue::Exact(input))
    }
    #[must_use] pub const fn const_overflow_y(input: LayoutOverflow) -> Self {
        Self::OverflowY(LayoutOverflowValue::Exact(input))
    }
    #[must_use] pub const fn const_overflow_block(input: LayoutOverflow) -> Self {
        Self::OverflowBlock(LayoutOverflowValue::Exact(input))
    }
    #[must_use] pub const fn const_overflow_inline(input: LayoutOverflow) -> Self {
        Self::OverflowInline(LayoutOverflowValue::Exact(input))
    }
    #[must_use] pub const fn const_padding_top(input: LayoutPaddingTop) -> Self {
        Self::PaddingTop(LayoutPaddingTopValue::Exact(input))
    }
    #[must_use] pub const fn const_padding_left(input: LayoutPaddingLeft) -> Self {
        Self::PaddingLeft(LayoutPaddingLeftValue::Exact(input))
    }
    #[must_use] pub const fn const_padding_right(input: LayoutPaddingRight) -> Self {
        Self::PaddingRight(LayoutPaddingRightValue::Exact(input))
    }
    #[must_use] pub const fn const_padding_bottom(input: LayoutPaddingBottom) -> Self {
        Self::PaddingBottom(LayoutPaddingBottomValue::Exact(input))
    }
    #[must_use] pub const fn const_margin_top(input: LayoutMarginTop) -> Self {
        Self::MarginTop(LayoutMarginTopValue::Exact(input))
    }
    #[must_use] pub const fn const_margin_left(input: LayoutMarginLeft) -> Self {
        Self::MarginLeft(LayoutMarginLeftValue::Exact(input))
    }
    #[must_use] pub const fn const_margin_right(input: LayoutMarginRight) -> Self {
        Self::MarginRight(LayoutMarginRightValue::Exact(input))
    }
    #[must_use] pub const fn const_margin_bottom(input: LayoutMarginBottom) -> Self {
        Self::MarginBottom(LayoutMarginBottomValue::Exact(input))
    }
    #[must_use] pub const fn const_border_top_left_radius(input: StyleBorderTopLeftRadius) -> Self {
        Self::BorderTopLeftRadius(StyleBorderTopLeftRadiusValue::Exact(input))
    }
    #[must_use] pub const fn const_border_top_right_radius(input: StyleBorderTopRightRadius) -> Self {
        Self::BorderTopRightRadius(StyleBorderTopRightRadiusValue::Exact(input))
    }
    #[must_use] pub const fn const_border_bottom_left_radius(input: StyleBorderBottomLeftRadius) -> Self {
        Self::BorderBottomLeftRadius(StyleBorderBottomLeftRadiusValue::Exact(input))
    }
    #[must_use] pub const fn const_border_bottom_right_radius(input: StyleBorderBottomRightRadius) -> Self {
        Self::BorderBottomRightRadius(StyleBorderBottomRightRadiusValue::Exact(input))
    }
    #[must_use] pub const fn const_border_top_color(input: StyleBorderTopColor) -> Self {
        Self::BorderTopColor(StyleBorderTopColorValue::Exact(input))
    }
    #[must_use] pub const fn const_border_right_color(input: StyleBorderRightColor) -> Self {
        Self::BorderRightColor(StyleBorderRightColorValue::Exact(input))
    }
    #[must_use] pub const fn const_border_left_color(input: StyleBorderLeftColor) -> Self {
        Self::BorderLeftColor(StyleBorderLeftColorValue::Exact(input))
    }
    #[must_use] pub const fn const_border_bottom_color(input: StyleBorderBottomColor) -> Self {
        Self::BorderBottomColor(StyleBorderBottomColorValue::Exact(input))
    }
    #[must_use] pub const fn const_border_top_style(input: StyleBorderTopStyle) -> Self {
        Self::BorderTopStyle(StyleBorderTopStyleValue::Exact(input))
    }
    #[must_use] pub const fn const_border_right_style(input: StyleBorderRightStyle) -> Self {
        Self::BorderRightStyle(StyleBorderRightStyleValue::Exact(input))
    }
    #[must_use] pub const fn const_border_left_style(input: StyleBorderLeftStyle) -> Self {
        Self::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(input))
    }
    #[must_use] pub const fn const_border_bottom_style(input: StyleBorderBottomStyle) -> Self {
        Self::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(input))
    }
    #[must_use] pub const fn const_border_top_width(input: LayoutBorderTopWidth) -> Self {
        Self::BorderTopWidth(LayoutBorderTopWidthValue::Exact(input))
    }
    #[must_use] pub const fn const_border_right_width(input: LayoutBorderRightWidth) -> Self {
        Self::BorderRightWidth(LayoutBorderRightWidthValue::Exact(input))
    }
    #[must_use] pub const fn const_border_left_width(input: LayoutBorderLeftWidth) -> Self {
        Self::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(input))
    }
    #[must_use] pub const fn const_border_bottom_width(input: LayoutBorderBottomWidth) -> Self {
        Self::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(input))
    }
    #[must_use] pub fn const_box_shadow_left(input: StyleBoxShadow) -> Self {
        Self::BoxShadowLeft(StyleBoxShadowValue::Exact(BoxOrStatic::heap(input)))
    }
    #[must_use] pub fn const_box_shadow_right(input: StyleBoxShadow) -> Self {
        Self::BoxShadowRight(StyleBoxShadowValue::Exact(BoxOrStatic::heap(input)))
    }
    #[must_use] pub fn const_box_shadow_top(input: StyleBoxShadow) -> Self {
        Self::BoxShadowTop(StyleBoxShadowValue::Exact(BoxOrStatic::heap(input)))
    }
    #[must_use] pub fn const_box_shadow_bottom(input: StyleBoxShadow) -> Self {
        Self::BoxShadowBottom(StyleBoxShadowValue::Exact(BoxOrStatic::heap(input)))
    }
    #[must_use] pub const fn const_opacity(input: StyleOpacity) -> Self {
        Self::Opacity(StyleOpacityValue::Exact(input))
    }
    #[must_use] pub const fn const_transform(input: StyleTransformVec) -> Self {
        Self::Transform(StyleTransformVecValue::Exact(input))
    }
    #[must_use] pub const fn const_transform_origin(input: StyleTransformOrigin) -> Self {
        Self::TransformOrigin(StyleTransformOriginValue::Exact(input))
    }
    #[must_use] pub const fn const_perspective_origin(input: StylePerspectiveOrigin) -> Self {
        Self::PerspectiveOrigin(StylePerspectiveOriginValue::Exact(input))
    }
    #[must_use] pub const fn const_backface_visibility(input: StyleBackfaceVisibility) -> Self {
        Self::BackfaceVisibility(StyleBackfaceVisibilityValue::Exact(input))
    }
    #[must_use] pub const fn const_break_before(input: PageBreak) -> Self {
        Self::BreakBefore(PageBreakValue::Exact(input))
    }
    #[must_use] pub const fn const_break_after(input: PageBreak) -> Self {
        Self::BreakAfter(PageBreakValue::Exact(input))
    }
    #[must_use] pub const fn const_break_inside(input: BreakInside) -> Self {
        Self::BreakInside(BreakInsideValue::Exact(input))
    }
    #[must_use] pub const fn const_orphans(input: Orphans) -> Self {
        Self::Orphans(OrphansValue::Exact(input))
    }
    #[must_use] pub const fn const_widows(input: Widows) -> Self {
        Self::Widows(WidowsValue::Exact(input))
    }
    #[must_use] pub const fn const_box_decoration_break(input: BoxDecorationBreak) -> Self {
        Self::BoxDecorationBreak(BoxDecorationBreakValue::Exact(input))
    }
    #[must_use] pub const fn const_column_count(input: ColumnCount) -> Self {
        Self::ColumnCount(ColumnCountValue::Exact(input))
    }
    #[must_use] pub const fn const_column_width(input: ColumnWidth) -> Self {
        Self::ColumnWidth(ColumnWidthValue::Exact(input))
    }
    #[must_use] pub const fn const_column_span(input: ColumnSpan) -> Self {
        Self::ColumnSpan(ColumnSpanValue::Exact(input))
    }
    #[must_use] pub const fn const_column_fill(input: ColumnFill) -> Self {
        Self::ColumnFill(ColumnFillValue::Exact(input))
    }
    #[must_use] pub const fn const_column_rule_width(input: ColumnRuleWidth) -> Self {
        Self::ColumnRuleWidth(ColumnRuleWidthValue::Exact(input))
    }
    #[must_use] pub const fn const_column_rule_style(input: ColumnRuleStyle) -> Self {
        Self::ColumnRuleStyle(ColumnRuleStyleValue::Exact(input))
    }
    #[must_use] pub const fn const_column_rule_color(input: ColumnRuleColor) -> Self {
        Self::ColumnRuleColor(ColumnRuleColorValue::Exact(input))
    }
    #[must_use] pub const fn const_flow_into(input: FlowInto) -> Self {
        Self::FlowInto(FlowIntoValue::Exact(input))
    }
    #[must_use] pub const fn const_flow_from(input: FlowFrom) -> Self {
        Self::FlowFrom(FlowFromValue::Exact(input))
    }
    #[must_use] pub const fn const_shape_outside(input: ShapeOutside) -> Self {
        Self::ShapeOutside(ShapeOutsideValue::Exact(input))
    }
    #[must_use] pub const fn const_shape_inside(input: ShapeInside) -> Self {
        Self::ShapeInside(ShapeInsideValue::Exact(input))
    }
    #[must_use] pub const fn const_clip_path(input: ClipPath) -> Self {
        Self::ClipPath(ClipPathValue::Exact(input))
    }
    #[must_use] pub const fn const_shape_margin(input: ShapeMargin) -> Self {
        Self::ShapeMargin(ShapeMarginValue::Exact(input))
    }
    #[must_use] pub const fn const_shape_image_threshold(input: ShapeImageThreshold) -> Self {
        Self::ShapeImageThreshold(ShapeImageThresholdValue::Exact(input))
    }
    #[must_use] pub const fn const_content(input: Content) -> Self {
        Self::Content(ContentValue::Exact(input))
    }
    #[must_use] pub const fn const_counter_reset(input: CounterReset) -> Self {
        Self::CounterReset(CounterResetValue::Exact(input))
    }
    #[must_use] pub const fn const_counter_increment(input: CounterIncrement) -> Self {
        Self::CounterIncrement(CounterIncrementValue::Exact(input))
    }
    #[must_use] pub const fn const_list_style_type(input: StyleListStyleType) -> Self {
        Self::ListStyleType(StyleListStyleTypeValue::Exact(input))
    }
    #[must_use] pub const fn const_list_style_position(input: StyleListStylePosition) -> Self {
        Self::ListStylePosition(StyleListStylePositionValue::Exact(input))
    }
    #[must_use] pub const fn const_string_set(input: StringSet) -> Self {
        Self::StringSet(StringSetValue::Exact(input))
    }
    #[must_use] pub const fn const_table_layout(input: LayoutTableLayout) -> Self {
        Self::TableLayout(LayoutTableLayoutValue::Exact(input))
    }
    #[must_use] pub const fn const_border_collapse(input: StyleBorderCollapse) -> Self {
        Self::BorderCollapse(StyleBorderCollapseValue::Exact(input))
    }
    #[must_use] pub const fn const_border_spacing(input: LayoutBorderSpacing) -> Self {
        Self::BorderSpacing(LayoutBorderSpacingValue::Exact(input))
    }
    #[must_use] pub const fn const_caption_side(input: StyleCaptionSide) -> Self {
        Self::CaptionSide(StyleCaptionSideValue::Exact(input))
    }
    #[must_use] pub const fn const_empty_cells(input: StyleEmptyCells) -> Self {
        Self::EmptyCells(StyleEmptyCellsValue::Exact(input))
    }
}

// Cross-type dispatch over CssProperty variants; identical format! bodies bind
// different value types and can't merge (clippy::match_same_arms false positive).
#[allow(clippy::match_same_arms)]
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
#[must_use] pub fn format_static_css_prop(prop: &CssProperty, tabs: usize) -> String {
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
        CssProperty::UnicodeBidi(p) => format!(
            "CssProperty::UnicodeBidi({})",
            print_css_property_value(p, tabs, "StyleUnicodeBidi")
        ),
        CssProperty::TextBoxTrim(p) => format!(
            "CssProperty::TextBoxTrim({})",
            print_css_property_value(p, tabs, "StyleTextBoxTrim")
        ),
        CssProperty::TextBoxEdge(p) => format!(
            "CssProperty::TextBoxEdge({})",
            print_css_property_value(p, tabs, "StyleTextBoxEdge")
        ),
        CssProperty::DominantBaseline(p) => format!(
            "CssProperty::DominantBaseline({})",
            print_css_property_value(p, tabs, "StyleDominantBaseline")
        ),
        CssProperty::AlignmentBaseline(p) => format!(
            "CssProperty::AlignmentBaseline({})",
            print_css_property_value(p, tabs, "StyleAlignmentBaseline")
        ),
        CssProperty::InitialLetterAlign(p) => format!(
            "CssProperty::InitialLetterAlign({})",
            print_css_property_value(p, tabs, "StyleInitialLetterAlign")
        ),
        CssProperty::InitialLetterWrap(p) => format!(
            "CssProperty::InitialLetterWrap({})",
            print_css_property_value(p, tabs, "StyleInitialLetterWrap")
        ),
        CssProperty::ScrollbarGutter(p) => format!(
            "CssProperty::ScrollbarGutter({})",
            print_css_property_value(p, tabs, "StyleScrollbarGutter")
        ),
        CssProperty::OverflowClipMargin(p) => format!(
            "CssProperty::OverflowClipMargin({})",
            print_css_property_value(p, tabs, "StyleOverflowClipMargin")
        ),
        CssProperty::Clip(p) => format!(
            "CssProperty::Clip({})",
            print_css_property_value(p, tabs, "StyleClipRect")
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
        CssProperty::OverflowBlock(p) => format!(
            "CssProperty::OverflowBlock({})",
            print_css_property_value(p, tabs, "LayoutOverflow")
        ),
        CssProperty::OverflowInline(p) => format!(
            "CssProperty::OverflowInline({})",
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
        CssProperty::WordBreak(p) => format!(
            "CssProperty::WordBreak({})",
            print_css_property_value(p, tabs, "StyleWordBreak")
        ),
        CssProperty::OverflowWrap(p) => format!(
            "CssProperty::OverflowWrap({})",
            print_css_property_value(p, tabs, "StyleOverflowWrap")
        ),
        CssProperty::LineBreak(p) => format!(
            "CssProperty::LineBreak({})",
            print_css_property_value(p, tabs, "StyleLineBreak")
        ),
        CssProperty::ObjectFit(p) => format!(
            "CssProperty::ObjectFit({})",
            print_css_property_value(p, tabs, "StyleObjectFit")
        ),
        CssProperty::ObjectPosition(p) => format!(
            "CssProperty::ObjectPosition({})",
            print_css_property_value(p, tabs, "StyleObjectPosition")
        ),
        CssProperty::AspectRatio(p) => format!(
            "CssProperty::AspectRatio({})",
            print_css_property_value(p, tabs, "StyleAspectRatio")
        ),
        CssProperty::TextOrientation(p) => format!(
            "CssProperty::TextOrientation({})",
            print_css_property_value(p, tabs, "StyleTextOrientation")
        ),
        CssProperty::TextAlignLast(p) => format!(
            "CssProperty::TextAlignLast({})",
            print_css_property_value(p, tabs, "StyleTextAlignLast")
        ),
        CssProperty::TextTransform(p) => format!(
            "CssProperty::TextTransform({})",
            print_css_property_value(p, tabs, "StyleTextTransform")
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
        CssPropertyValue::Auto => format!("{property_value_type}Value::Auto"),
        CssPropertyValue::None => format!("{property_value_type}Value::None"),
        CssPropertyValue::Initial => format!("{property_value_type}Value::Initial"),
        CssPropertyValue::Inherit => format!("{property_value_type}Value::Inherit"),
        CssPropertyValue::Revert => format!("{property_value_type}Value::Revert"),
        CssPropertyValue::Unset => format!("{property_value_type}Value::Unset"),
        CssPropertyValue::Exact(t) => format!(
            "{}Value::Exact({})",
            property_value_type,
            t.format_as_rust_code(tabs)
        ),
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod autotest_generated {
    use super::*;
    use crate::props::basic::animation::AnimationInterpolationFunction;

    // ---- helpers -----------------------------------------------------------

    fn resolver() -> InterpolateResolver {
        InterpolateResolver {
            interpolate_func: AnimationInterpolationFunction::Linear,
            parent_rect_width: 800.0,
            parent_rect_height: 600.0,
            current_rect_width: 400.0,
            current_rect_height: 300.0,
        }
    }

    fn font_size(px: f32) -> CssProperty {
        CssProperty::font_size(StyleFontSize {
            inner: PixelValue::px(px),
        })
    }

    fn font_size_px_of(prop: &CssProperty) -> f32 {
        match prop {
            CssProperty::FontSize(CssPropertyValue::Exact(fs)) => fs.inner.number.get(),
            other => panic!("expected an exact FontSize, got {other:?}"),
        }
    }

    /// The five CSS table properties that `CssPropertyType::to_str()` names but
    /// `CSS_PROPERTY_KEY_MAP` never registers, so `from_str` can't find them.
    /// See `bug_table_properties_are_unreachable_from_stylesheet_text`.
    const KEYS_MISSING_FROM_KEY_MAP: &[CssPropertyType] = &[
        CssPropertyType::TableLayout,
        CssPropertyType::BorderCollapse,
        CssPropertyType::BorderSpacing,
        CssPropertyType::CaptionSide,
        CssPropertyType::EmptyCells,
    ];

    // ---- CssKeyMap / get_css_key_map ---------------------------------------

    #[test]
    fn key_map_is_populated_and_deterministic() {
        let a = get_css_key_map();
        let b = CssKeyMap::get();
        assert_eq!(a, b, "CssKeyMap::get() must equal get_css_key_map()");
        assert!(!a.non_shorthands.is_empty());
        assert!(!a.shorthands.is_empty());
        // Every registered key resolves back to the type it was registered under.
        for (k, v) in &a.non_shorthands {
            assert_eq!(CssPropertyType::from_str(k, &a), Some(*v), "key {k}");
        }
        for (k, v) in &a.shorthands {
            assert_eq!(
                CombinedCssPropertyType::from_str(k, &a),
                Some(*v),
                "shorthand {k}"
            );
        }
    }

    // ---- from_str: malformed / boundary / unicode ---------------------------

    #[test]
    fn from_str_empty_input_returns_none() {
        let map = get_css_key_map();
        assert_eq!(CssPropertyType::from_str("", &map), None);
        assert_eq!(CombinedCssPropertyType::from_str("", &map), None);
    }

    #[test]
    fn from_str_whitespace_only_returns_none() {
        let map = get_css_key_map();
        for input in ["   ", "\t", "\n", "\r\n", " \t \n \r ", "\u{a0}"] {
            assert_eq!(CssPropertyType::from_str(input, &map), None, "{input:?}");
            assert_eq!(
                CombinedCssPropertyType::from_str(input, &map),
                None,
                "{input:?}"
            );
        }
    }

    #[test]
    fn from_str_trims_surrounding_whitespace() {
        let map = get_css_key_map();
        assert_eq!(
            CssPropertyType::from_str("  \t width \n ", &map),
            Some(CssPropertyType::Width)
        );
        assert_eq!(
            CombinedCssPropertyType::from_str("\n border \t", &map),
            Some(CombinedCssPropertyType::Border)
        );
    }

    #[test]
    fn from_str_garbage_returns_none() {
        let map = get_css_key_map();
        for input in [
            "asdfasdfasdf",
            ";",
            "{}",
            "widthh",
            "wid th",
            "width:",
            "width;garbage",
            "width!important",
            "\0",
            "\u{0}width\u{0}",
            "../../etc/passwd",
            "%s%s%s%n",
            "-",
            "--",
            "--custom-property",
        ] {
            assert_eq!(CssPropertyType::from_str(input, &map), None, "{input:?}");
            assert_eq!(
                CombinedCssPropertyType::from_str(input, &map),
                None,
                "{input:?}"
            );
        }
    }

    #[test]
    fn from_str_is_case_sensitive() {
        // CSS keys are case-insensitive per spec, but these lookups are raw map
        // hits: normalisation is the caller's job (see parser2). Locked in so a
        // future change to the casing contract is a deliberate, visible one.
        let map = get_css_key_map();
        for input in ["WIDTH", "Width", "wIdTh"] {
            assert_eq!(CssPropertyType::from_str(input, &map), None, "{input:?}");
        }
        assert_eq!(
            CssPropertyType::from_str("width", &map),
            Some(CssPropertyType::Width)
        );
    }

    #[test]
    fn from_str_boundary_number_strings_return_none() {
        let map = get_css_key_map();
        for input in [
            "0",
            "-0",
            "NaN",
            "nan",
            "inf",
            "-inf",
            "Infinity",
            "9223372036854775807",  // i64::MAX
            "-9223372036854775808", // i64::MIN
            "340282350000000000000000000000000000000", // ~f32::MAX
            "1e309",                // overflows f64
            "0.00000000000000000001",
        ] {
            assert_eq!(CssPropertyType::from_str(input, &map), None, "{input:?}");
            assert_eq!(
                CombinedCssPropertyType::from_str(input, &map),
                None,
                "{input:?}"
            );
        }
    }

    #[test]
    fn from_str_unicode_does_not_panic() {
        let map = get_css_key_map();
        for input in [
            "\u{1F600}",                // emoji
            "wi\u{0301}dth",            // combining acute accent
            "\u{202E}width",            // RTL override
            "ｗｉｄｔｈ",               // fullwidth latin
            "width\u{FEFF}",            // BOM suffix
            "𝓌𝒾𝒹𝓉𝒽",                    // mathematical script
            "ширина",                   // cyrillic
            "\u{0301}\u{0301}\u{0301}", // lone combining marks
        ] {
            assert_eq!(CssPropertyType::from_str(input, &map), None, "{input:?}");
            assert_eq!(
                CombinedCssPropertyType::from_str(input, &map),
                None,
                "{input:?}"
            );
        }
    }

    #[test]
    fn from_str_extremely_long_input_does_not_panic_or_hang() {
        let map = get_css_key_map();
        let huge = "width".repeat(200_000); // 1_000_000 chars
        assert_eq!(huge.len(), 1_000_000);
        assert_eq!(CssPropertyType::from_str(&huge, &map), None);
        assert_eq!(CombinedCssPropertyType::from_str(&huge, &map), None);

        // A valid key buried in a megabyte of padding is still not a valid key.
        let padded = format!("{}width{}", "x".repeat(500_000), "x".repeat(500_000));
        assert_eq!(CssPropertyType::from_str(&padded, &map), None);
    }

    #[test]
    fn from_str_deeply_nested_brackets_do_not_stack_overflow() {
        let map = get_css_key_map();
        let nested = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert_eq!(CssPropertyType::from_str(&nested, &map), None);
        assert_eq!(CombinedCssPropertyType::from_str(&nested, &map), None);
    }

    #[test]
    fn from_str_valid_minimal_positive_control() {
        let map = get_css_key_map();
        assert_eq!(
            CssPropertyType::from_str("width", &map),
            Some(CssPropertyType::Width)
        );
        assert_eq!(
            CssPropertyType::from_str("justify-content", &map),
            Some(CssPropertyType::JustifyContent)
        );
        assert_eq!(
            CombinedCssPropertyType::from_str("border", &map),
            Some(CombinedCssPropertyType::Border)
        );
    }

    #[test]
    fn word_wrap_is_a_registered_alias_for_overflow_wrap() {
        let map = get_css_key_map();
        assert_eq!(
            CssPropertyType::from_str("word-wrap", &map),
            Some(CssPropertyType::OverflowWrap)
        );
        assert_eq!(
            CssPropertyType::from_str("overflow-wrap", &map),
            Some(CssPropertyType::OverflowWrap)
        );
        // The alias is not the canonical name.
        assert_eq!(CssPropertyType::OverflowWrap.to_str(), "overflow-wrap");
    }

    #[test]
    fn keys_present_in_both_maps_are_shorthand_shadowed() {
        // These four strings are registered in *both* key maps. parser2 consults
        // the shorthand map first, so the CombinedCssPropertyType always wins and
        // the same-named CssPropertyType is never reached from stylesheet text.
        let map = get_css_key_map();
        for k in ["background", "font", "gap", "grid-gap"] {
            assert!(
                CssPropertyType::from_str(k, &map).is_some(),
                "{k} should be in the non-shorthand map"
            );
            assert!(
                CombinedCssPropertyType::from_str(k, &map).is_some(),
                "{k} should be in the shorthand map"
            );
        }
    }

    // ---- to_str / Display / Debug round-trips -------------------------------

    #[test]
    fn css_property_type_to_str_is_non_empty_and_unique() {
        let mut seen = BTreeMap::new();
        for t in CssPropertyType::iter() {
            let s = t.to_str();
            assert!(!s.is_empty(), "{t:?} has an empty to_str()");
            assert!(!s.contains(' '), "{t:?} to_str() contains whitespace: {s:?}");
            assert_eq!(s.trim(), s, "{t:?} to_str() is not trimmed: {s:?}");
            if let Some(prev) = seen.insert(s, t) {
                panic!("to_str() collision: {prev:?} and {t:?} both return {s:?}");
            }
        }
        assert_eq!(seen.len(), CssPropertyType::ALL.len());
    }

    #[test]
    fn css_property_type_display_and_debug_agree_with_to_str() {
        for t in CssPropertyType::iter() {
            assert_eq!(format!("{t}"), t.to_str());
            assert_eq!(format!("{t:?}"), t.to_str());
        }
    }

    #[test]
    fn css_property_type_iter_matches_all() {
        let collected: Vec<CssPropertyType> = CssPropertyType::iter().collect();
        assert_eq!(collected.as_slice(), CssPropertyType::ALL);
        // Iteration is repeatable (no interior state).
        let again: Vec<CssPropertyType> = CssPropertyType::iter().collect();
        assert_eq!(collected, again);
    }

    #[test]
    fn css_property_type_to_str_round_trips_except_known_gap() {
        // parse(serialize(x)) == x for every property type that is actually
        // registered in the key map. The set that fails to round-trip is pinned
        // to the five table properties below; a sixth regression fails here.
        let map = get_css_key_map();
        let mut unreachable = Vec::new();
        for t in CssPropertyType::iter() {
            match CssPropertyType::from_str(t.to_str(), &map) {
                Some(back) => assert_eq!(back, t, "{t:?} round-tripped to the wrong type"),
                None => unreachable.push(t),
            }
        }
        assert_eq!(
            unreachable.as_slice(),
            KEYS_MISSING_FROM_KEY_MAP,
            "the set of property types missing from CSS_PROPERTY_KEY_MAP changed"
        );
    }

    #[test]
    #[ignore = "RED: 5 table properties are absent from CSS_PROPERTY_KEY_MAP, so they can never \
                be parsed from stylesheet text. Remove the #[ignore] once they are registered."]
    fn bug_table_properties_are_unreachable_from_stylesheet_text() {
        let map = get_css_key_map();
        for t in KEYS_MISSING_FROM_KEY_MAP {
            assert_eq!(
                CssPropertyType::from_str(t.to_str(), &map),
                Some(*t),
                "`{}` has a to_str() name and a working value parser, but no key-map \
                 entry, so parser2 rejects the declaration outright",
                t.to_str()
            );
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn table_property_value_parsers_work_even_though_the_keys_do_not_resolve() {
        // Proves the gap above is purely in CSS_PROPERTY_KEY_MAP: given the key
        // *type*, every one of these values parses fine. Only the string -> type
        // lookup is missing.
        for (t, value) in [
            (CssPropertyType::TableLayout, "fixed"),
            (CssPropertyType::BorderCollapse, "collapse"),
            (CssPropertyType::CaptionSide, "top"),
            (CssPropertyType::EmptyCells, "hide"),
        ] {
            let parsed = parse_css_property(t, value)
                .unwrap_or_else(|e| panic!("`{}: {value}` should parse: {e}", t.to_str()));
            assert_eq!(parsed.get_type(), t);
        }
    }

    #[test]
    fn combined_css_property_type_round_trips_and_display_agrees() {
        let map = get_css_key_map();
        for t in map.shorthands.values() {
            let s = t.to_str(&map);
            assert!(!s.is_empty());
            assert_eq!(
                CombinedCssPropertyType::from_str(s, &map),
                Some(*t),
                "{t:?} did not round-trip"
            );
            // Display is derived from the static array, to_str from the map:
            // the two sources must not drift apart.
            assert_eq!(format!("{t}"), s, "Display disagrees with to_str for {t:?}");
        }
        assert_eq!(map.shorthands.len(), COMBINED_CSS_PROPERTIES_KEY_MAP.len());
    }

    // ---- predicates: totality + known true/false -----------------------------

    #[test]
    fn is_inheritable_matches_the_css_spec_for_known_properties() {
        for t in [
            CssPropertyType::TextColor,
            CssPropertyType::FontFamily,
            CssPropertyType::FontSize,
            CssPropertyType::LineHeight,
            CssPropertyType::Visibility,
            CssPropertyType::Cursor,
            CssPropertyType::WritingMode,
        ] {
            assert!(t.is_inheritable(), "{t:?} is inherited per CSS spec");
        }
        for t in [
            CssPropertyType::Width,
            CssPropertyType::Height,
            CssPropertyType::Display,
            CssPropertyType::Position,
            CssPropertyType::Opacity,
            CssPropertyType::Transform,
            CssPropertyType::BackgroundContent,
            CssPropertyType::UnicodeBidi, // explicitly non-inherited, see +spec:display-property
        ] {
            assert!(!t.is_inheritable(), "{t:?} is NOT inherited per CSS spec");
        }
    }

    #[test]
    fn predicates_are_total_over_every_property_type() {
        // Every predicate must return a deterministic bool for all 180 variants
        // without panicking, and must be pure (same answer twice).
        for t in CssPropertyType::iter() {
            assert_eq!(t.is_inheritable(), t.is_inheritable());
            assert_eq!(t.has_compact_encoding(), t.has_compact_encoding());
            assert_eq!(t.can_trigger_relayout(), t.can_trigger_relayout());
            assert_eq!(t.is_gpu_only_property(), t.is_gpu_only_property());
            assert_eq!(t.get_category(), t.get_category());
            assert_eq!(t.relayout_scope(false), t.relayout_scope(false));
            assert_eq!(t.relayout_scope(true), t.relayout_scope(true));
        }
    }

    #[test]
    fn is_gpu_only_property_is_exactly_opacity_and_transform() {
        let gpu: Vec<CssPropertyType> = CssPropertyType::iter()
            .filter(CssPropertyType::is_gpu_only_property)
            .collect();
        assert_eq!(
            gpu,
            vec![CssPropertyType::Opacity, CssPropertyType::Transform]
        );
    }

    #[test]
    fn has_compact_encoding_known_true_false() {
        assert!(CssPropertyType::Display.has_compact_encoding());
        assert!(CssPropertyType::Width.has_compact_encoding());
        assert!(CssPropertyType::FlexGrow.has_compact_encoding());
        assert!(!CssPropertyType::Transform.has_compact_encoding());
        assert!(!CssPropertyType::Filter.has_compact_encoding());
        assert!(!CssPropertyType::Content.has_compact_encoding());
    }

    #[test]
    fn can_trigger_relayout_known_true_false() {
        for t in [
            CssPropertyType::Width,
            CssPropertyType::Display,
            CssPropertyType::FontSize,
            CssPropertyType::MarginTop,
        ] {
            assert!(t.can_trigger_relayout(), "{t:?} affects geometry");
        }
        for t in [
            CssPropertyType::TextColor,
            CssPropertyType::Opacity,
            CssPropertyType::Transform,
            CssPropertyType::BackgroundContent,
        ] {
            assert!(!t.can_trigger_relayout(), "{t:?} is paint-only");
        }
    }

    #[test]
    fn get_category_is_derived_consistently_from_the_predicates() {
        for t in CssPropertyType::iter() {
            let expected = if t.is_gpu_only_property() {
                CssPropertyCategory::GpuOnly
            } else {
                match (t.is_inheritable(), t.can_trigger_relayout()) {
                    (true, true) => CssPropertyCategory::InheritedLayout,
                    (true, false) => CssPropertyCategory::InheritedPaint,
                    (false, true) => CssPropertyCategory::Layout,
                    (false, false) => CssPropertyCategory::Paint,
                }
            };
            assert_eq!(t.get_category(), expected, "{t:?}");
        }
        assert_eq!(
            CssPropertyType::Opacity.get_category(),
            CssPropertyCategory::GpuOnly
        );
        assert_eq!(
            CssPropertyType::Width.get_category(),
            CssPropertyCategory::Layout
        );
        assert_eq!(
            CssPropertyType::FontSize.get_category(),
            CssPropertyCategory::InheritedLayout
        );
    }

    // ---- relayout_scope ------------------------------------------------------

    #[test]
    fn relayout_scope_never_contradicts_can_trigger_relayout() {
        // relayout_scope is documented as "a more granular replacement for
        // can_trigger_relayout()". The safe direction must hold: anything the
        // coarse predicate calls paint-only must also be scope None, or an
        // incremental-layout consumer would skip a relayout it actually needs.
        for t in CssPropertyType::iter() {
            if !t.can_trigger_relayout() {
                for ifc in [false, true] {
                    assert_eq!(
                        t.relayout_scope(ifc),
                        RelayoutScope::None,
                        "{t:?} is paint-only but claims a relayout scope (ifc={ifc})"
                    );
                }
            }
        }
    }

    #[test]
    fn relayout_scope_paint_only_ignores_the_ifc_flag() {
        for t in [
            CssPropertyType::TextColor,
            CssPropertyType::Opacity,
            CssPropertyType::Transform,
            CssPropertyType::BackgroundContent,
            CssPropertyType::CaretColor,
            CssPropertyType::ObjectFit,
        ] {
            assert_eq!(t.relayout_scope(false), RelayoutScope::None, "{t:?}");
            assert_eq!(t.relayout_scope(true), RelayoutScope::None, "{t:?}");
        }
    }

    #[test]
    fn relayout_scope_upgrades_text_properties_only_inside_an_ifc() {
        // Font/text changes reflow an inline formatting context but do not
        // resize a block container that has only block children.
        for t in [
            CssPropertyType::FontSize,
            CssPropertyType::FontFamily,
            CssPropertyType::LineHeight,
            CssPropertyType::LetterSpacing,
        ] {
            assert_eq!(t.relayout_scope(true), RelayoutScope::IfcOnly, "{t:?}");
            assert_ne!(t.relayout_scope(false), RelayoutScope::IfcOnly, "{t:?}");
        }
    }

    // ---- CssProperty keyword constructors: totality over all 180 variants -----

    #[test]
    fn keyword_constructors_preserve_the_property_type_for_every_variant() {
        // css_property_from_type! is a 180-arm hand-written macro: a single
        // copy-paste slip would silently build the wrong variant.
        for t in CssPropertyType::iter() {
            assert_eq!(CssProperty::none(t).get_type(), t, "none({t:?})");
            assert_eq!(CssProperty::auto(t).get_type(), t, "auto({t:?})");
            assert_eq!(CssProperty::initial(t).get_type(), t, "initial({t:?})");
            assert_eq!(CssProperty::inherit(t).get_type(), t, "inherit({t:?})");
        }
    }

    #[test]
    fn key_agrees_with_get_type_for_every_variant() {
        for t in CssPropertyType::iter() {
            assert_eq!(CssProperty::none(t).key(), t.to_str(), "{t:?}");
        }
    }

    #[test]
    fn value_and_format_css_are_well_formed_for_every_keyword_variant() {
        for t in CssPropertyType::iter() {
            for (ctor, keyword) in [
                (CssProperty::none as fn(CssPropertyType) -> CssProperty, "none"),
                (CssProperty::auto, "auto"),
                (CssProperty::initial, "initial"),
                (CssProperty::inherit, "inherit"),
            ] {
                let prop = ctor(t);
                assert_eq!(prop.value(), keyword, "{t:?} {keyword}");
                assert!(!prop.value().is_empty());
                assert_eq!(
                    prop.format_css(),
                    format!("{}: {keyword};", t.to_str()),
                    "{t:?} {keyword}"
                );
            }
        }
    }

    #[test]
    fn format_css_does_not_panic_on_extreme_and_non_finite_numbers() {
        for px in [
            0.0,
            -0.0,
            1.0,
            -1.0,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            f32::EPSILON,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ] {
            let prop = font_size(px);
            let css = prop.format_css();
            assert!(!css.is_empty(), "empty css for font-size {px}");
            assert!(css.starts_with("font-size: "), "malformed: {css:?}");
            assert!(css.ends_with(';'), "malformed: {css:?}");
            assert_eq!(prop.get_type(), CssPropertyType::FontSize);
        }
    }

    #[test]
    fn extreme_float_inputs_saturate_rather_than_wrap() {
        // FloatValue stores a fixed-point isize; `f32 as isize` saturates, so
        // huge magnitudes must clamp and NaN must land on a defined value.
        assert!(font_size_px_of(&font_size(f32::INFINITY)) > 0.0);
        assert!(font_size_px_of(&font_size(f32::NEG_INFINITY)) < 0.0);
        assert_eq!(font_size_px_of(&font_size(f32::NAN)), 0.0);
        assert_eq!(font_size_px_of(&font_size(0.0)), 0.0);
        assert_eq!(font_size_px_of(&font_size(16.0)), 16.0);
        assert_eq!(font_size_px_of(&font_size(-16.0)), -16.0);
    }

    // ---- interpolate ---------------------------------------------------------

    #[test]
    fn interpolate_at_and_beyond_the_endpoints() {
        let r = resolver();
        let a = font_size(10.0);
        let b = font_size(20.0);

        assert_eq!(a.interpolate(&b, 0.0, &r), a, "t=0 must return self");
        assert_eq!(a.interpolate(&b, 1.0, &r), b, "t=1 must return other");
        assert_eq!(a.interpolate(&b, -0.0, &r), a);
        assert_eq!(a.interpolate(&b, -5.0, &r), a, "t<0 clamps to self");
        assert_eq!(a.interpolate(&b, 5.0, &r), b, "t>1 clamps to other");
        assert_eq!(a.interpolate(&b, f32::NEG_INFINITY, &r), a);
        assert_eq!(a.interpolate(&b, f32::INFINITY, &r), b);
        assert_eq!(a.interpolate(&b, f32::MIN, &r), a);
        assert_eq!(a.interpolate(&b, f32::MAX, &r), b);
    }

    #[test]
    fn interpolate_midpoint_is_the_linear_average() {
        let r = resolver();
        let out = font_size(0.0).interpolate(&font_size(100.0), 0.5, &r);
        let px = font_size_px_of(&out);
        assert!(
            (px - 50.0).abs() < 1.0,
            "linear midpoint of 0px..100px should be ~50px, got {px}"
        );
    }

    #[test]
    fn interpolate_nan_t_does_not_panic_and_keeps_the_property_type() {
        let r = resolver();
        let a = font_size(10.0);
        let b = font_size(20.0);
        // NaN fails both the `t <= 0.0` and `t >= 1.0` guards and survives
        // f32::clamp, so it reaches the per-property interpolators.
        let out = a.interpolate(&b, f32::NAN, &r);
        assert_eq!(out.get_type(), CssPropertyType::FontSize);
        assert!(!out.format_css().is_empty());
    }

    #[test]
    fn interpolate_extreme_endpoints_do_not_panic() {
        let r = resolver();
        for (from, to) in [
            (f32::MAX, f32::MIN),
            (f32::MIN, f32::MAX),
            (f32::INFINITY, f32::NEG_INFINITY),
            (f32::NAN, 10.0),
            (10.0, f32::NAN),
            (0.0, 0.0),
        ] {
            for t in [0.25, 0.5, 0.75] {
                let out = font_size(from).interpolate(&font_size(to), t, &r);
                assert_eq!(out.get_type(), CssPropertyType::FontSize);
                assert!(!out.format_css().is_empty());
            }
        }
    }

    #[test]
    fn interpolate_between_mismatched_types_falls_back_without_panic() {
        let r = resolver();
        let width = CssProperty::width(LayoutWidth::Px(PixelValue::px(10.0)));
        let height = CssProperty::height(LayoutHeight::Px(PixelValue::px(20.0)));

        // Not animatable across types: snaps to the nearer endpoint.
        assert_eq!(width.interpolate(&height, 0.25, &r), width);
        assert_eq!(width.interpolate(&height, 0.75, &r), height);
        // NaN takes neither branch of `t > 0.5`, so it must fall back to self.
        assert_eq!(width.interpolate(&height, f32::NAN, &r), width);
    }

    #[test]
    fn interpolate_keyword_operands_fall_back_to_defaults_without_panic() {
        let r = resolver();
        // `auto`/`inherit` carry no concrete value; interpolating them must not
        // unwrap a missing property.
        let auto = CssProperty::auto(CssPropertyType::FontSize);
        let inherit = CssProperty::inherit(CssPropertyType::FontSize);
        let exact = font_size(24.0);

        for (a, b) in [
            (&auto, &exact),
            (&exact, &auto),
            (&inherit, &exact),
            (&auto, &inherit),
        ] {
            let out = a.interpolate(b, 0.5, &r);
            assert_eq!(out.get_type(), CssPropertyType::FontSize);
            assert!(!out.format_css().is_empty());
        }
    }

    // ---- parse_css_property --------------------------------------------------

    #[test]
    #[cfg(feature = "parser")]
    fn parse_css_property_keyword_shortcut_works_for_every_property_type() {
        // `initial` / `inherit` short-circuit before any key-specific parsing,
        // so they must succeed for all 180 types and keep the requested type.
        for t in CssPropertyType::iter() {
            for keyword in ["initial", "inherit"] {
                let parsed = parse_css_property(t, keyword)
                    .unwrap_or_else(|e| panic!("{}: {keyword} failed: {e}", t.to_str()));
                assert_eq!(parsed.get_type(), t, "{t:?} {keyword}");
                assert_eq!(parsed.value(), keyword);
            }
            // Surrounding whitespace is trimmed before the keyword match.
            let parsed = parse_css_property(t, "  \t initial \n ")
                .unwrap_or_else(|e| panic!("{}: padded initial failed: {e}", t.to_str()));
            assert_eq!(parsed, CssProperty::initial(t), "{t:?}");
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parse_css_property_valid_minimal_positive_control() {
        let width = parse_css_property(CssPropertyType::Width, "100px").expect("100px is valid");
        assert_eq!(width.get_type(), CssPropertyType::Width);
        assert_eq!(width.value(), "100px");
        assert_eq!(width.format_css(), "width: 100px;");

        let display =
            parse_css_property(CssPropertyType::Display, "flex").expect("flex is valid display");
        assert_eq!(display.get_type(), CssPropertyType::Display);
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parse_css_property_empty_and_whitespace_only_are_rejected() {
        for value in ["", " ", "\t", "\n", "   \t\n  "] {
            assert!(
                parse_css_property(CssPropertyType::Width, value).is_err(),
                "width: {value:?} should not parse"
            );
            assert!(
                parse_css_property(CssPropertyType::TextColor, value).is_err(),
                "color: {value:?} should not parse"
            );
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parse_css_property_garbage_is_rejected_without_panicking() {
        for value in [
            "!!!",
            "not-a-value",
            "100pxx",
            "px100",
            "100 px",
            ";",
            "}",
            "100px;",
            "100px !important",
            "\0",
            "#gg0000",
            "rgb(",
            "rgb(1,2",
        ] {
            assert!(
                parse_css_property(CssPropertyType::Width, value).is_err()
                    || parse_css_property(CssPropertyType::TextColor, value).is_err(),
                "{value:?} parsed as both a width and a color"
            );
        }
        // Spot-check the ones that must be rejected by *both* parsers.
        for value in ["!!!", "not-a-value", "\0", ";"] {
            assert!(parse_css_property(CssPropertyType::Width, value).is_err());
            assert!(parse_css_property(CssPropertyType::TextColor, value).is_err());
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parse_css_property_unicode_is_rejected_without_panicking() {
        for value in [
            "\u{1F600}",
            "100\u{0301}px",
            "\u{202E}100px",
            "１００ｐｘ",
            "100px\u{FEFF}",
            "红色",
        ] {
            // Must not panic; a multibyte slice must never be cut mid-codepoint.
            let _ = parse_css_property(CssPropertyType::Width, value).is_err();
            let _ = parse_css_property(CssPropertyType::TextColor, value).is_err();
            let _ = parse_css_property(CssPropertyType::FontFamily, value).is_ok();
        }
        assert!(parse_css_property(CssPropertyType::Width, "\u{1F600}").is_err());
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parse_css_property_boundary_numbers_do_not_panic() {
        for value in [
            "0",
            "0px",
            "-0px",
            "-1px",
            "2147483647px",
            "-2147483648px",
            "9223372036854775807px",
            "340282350000000000000000000000000000000px",
            "1e309px",
            "NaNpx",
            "infpx",
            "0.00000000000000000001px",
            "99999999999999999999999999999999999999999999px",
        ] {
            // The contract is "no panic, no overflow trap" — a saturating Ok or a
            // clean Err are both acceptable, a debug-overflow panic is not.
            let parsed = parse_css_property(CssPropertyType::Width, value);
            if let Ok(p) = parsed {
                assert_eq!(p.get_type(), CssPropertyType::Width, "{value:?}");
                assert!(!p.format_css().is_empty(), "{value:?}");
            }
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parse_css_property_extremely_long_input_does_not_hang() {
        // Long *digit* runs stay at 100k: the float parser is the expensive part
        // and the point is to prove it terminates, not to benchmark it.
        let huge = "1".repeat(100_000);
        if let Ok(p) = parse_css_property(CssPropertyType::Width, &huge) {
            assert_eq!(p.get_type(), CssPropertyType::Width);
        }
        let huge_px = format!("{}px", "9".repeat(100_000));
        let _ = parse_css_property(CssPropertyType::Width, &huge_px);

        // Pure garbage is rejected on the first byte, so a full megabyte is cheap.
        let huge_garbage = "z".repeat(1_000_000);
        assert!(parse_css_property(CssPropertyType::Width, &huge_garbage).is_err());
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parse_css_property_deeply_nested_calc_does_not_stack_overflow() {
        // parse_calc_expression is an iterative stack machine, not a recursive
        // descent parser, so deep nesting must stay on the heap.
        let depth = 10_000;
        let nested = format!("calc({}1px{})", "(".repeat(depth), ")".repeat(depth));
        let _ = parse_css_property(CssPropertyType::Width, &nested);

        let unbalanced = format!("calc({})", "(".repeat(depth));
        let _ = parse_css_property(CssPropertyType::Width, &unbalanced);
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parse_css_property_leading_trailing_junk_is_handled_deterministically() {
        // Padding is trimmed...
        let padded = parse_css_property(CssPropertyType::Width, "  \t 100px \n ")
            .expect("surrounding whitespace should be trimmed");
        assert_eq!(padded.value(), "100px");
        assert_eq!(
            padded,
            parse_css_property(CssPropertyType::Width, "100px").unwrap()
        );
        // ...but embedded junk is not silently dropped.
        assert!(parse_css_property(CssPropertyType::Width, "100px;garbage").is_err());
        assert!(parse_css_property(CssPropertyType::Width, "garbage 100px").is_err());
    }

    // ---- parse_combined_css_property ------------------------------------------

    #[test]
    #[cfg(feature = "parser")]
    fn parse_combined_css_property_valid_minimal_positive_control() {
        let props = parse_combined_css_property(CombinedCssPropertyType::Margin, "10px")
            .expect("margin: 10px is valid");
        let types: Vec<CssPropertyType> = props.iter().map(CssProperty::get_type).collect();
        assert_eq!(
            types,
            vec![
                CssPropertyType::MarginTop,
                CssPropertyType::MarginBottom,
                CssPropertyType::MarginLeft,
                CssPropertyType::MarginRight,
            ]
        );
        for p in &props {
            assert_eq!(p.value(), "10px");
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parse_combined_css_property_expands_every_shorthand_or_errors_cleanly() {
        // `initial` short-circuits ahead of every value parser, so all 27
        // shorthands must expand to a non-empty list of `initial` longhands.
        let map = get_css_key_map();
        for key in map.shorthands.values() {
            let props = parse_combined_css_property(*key, "initial")
                .unwrap_or_else(|e| panic!("{key:?}: initial failed: {e}"));
            assert!(
                !props.is_empty(),
                "{key:?} expanded to an empty property list"
            );
            for p in &props {
                assert_eq!(p.value(), "initial", "{key:?} -> {:?}", p.get_type());
                assert_eq!(*p, CssProperty::initial(p.get_type()), "{key:?}");
            }
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parse_combined_css_property_empty_and_whitespace_are_rejected() {
        for value in ["", " ", "\t\n", "    "] {
            assert!(
                parse_combined_css_property(CombinedCssPropertyType::Margin, value).is_err(),
                "margin: {value:?} should not parse"
            );
            assert!(
                parse_combined_css_property(CombinedCssPropertyType::BorderRadius, value).is_err(),
                "border-radius: {value:?} should not parse"
            );
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parse_combined_css_property_garbage_is_rejected_without_panicking() {
        for value in ["!!!", "not-a-value", "10pxx", ";", "\0", "10px 20px 30px 40px 50px"] {
            assert!(
                parse_combined_css_property(CombinedCssPropertyType::Margin, value).is_err(),
                "margin: {value:?} should not parse"
            );
        }
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parse_combined_css_property_unicode_and_long_input_do_not_panic() {
        for value in ["\u{1F600}", "1\u{0301}0px", "１０ｐｘ", "红色"] {
            let _ = parse_combined_css_property(CombinedCssPropertyType::Margin, value).is_err();
            let _ = parse_combined_css_property(CombinedCssPropertyType::Background, value).is_err();
        }
        // The padding/margin parser parses every value before it counts them, so
        // 20k values already exercises the TooManyValues path without a long run.
        let many = "10px ".repeat(20_000);
        assert!(
            parse_combined_css_property(CombinedCssPropertyType::Margin, &many).is_err(),
            "20_000 margin values should be TooManyValues, not a panic"
        );
        let huge_garbage = "z".repeat(1_000_000);
        assert!(
            parse_combined_css_property(CombinedCssPropertyType::Margin, &huge_garbage).is_err()
        );
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parse_combined_css_property_nested_parens_do_not_stack_overflow() {
        let nested = format!("{}10px{}", "(".repeat(1_000), ")".repeat(1_000));
        let _ = parse_combined_css_property(CombinedCssPropertyType::Margin, &nested);
        let _ = parse_combined_css_property(CombinedCssPropertyType::Border, &nested);
    }

    // ---- CssParsingError round-trip -------------------------------------------

    #[test]
    #[cfg(feature = "parser")]
    fn parsing_error_survives_the_owned_round_trip() {
        let err = parse_css_property(CssPropertyType::Width, "definitely-not-a-width")
            .expect_err("garbage width must fail");

        let owned = err.to_contained();
        let shared = owned.to_shared();

        // to_contained/to_shared must preserve the error, not flatten it to a
        // generic variant: the rendered message is the observable contract.
        assert_eq!(format!("{err}"), format!("{shared}"));
        assert!(!format!("{err}").is_empty());
        // ...and the round-trip is idempotent.
        let owned_again = shared.to_contained();
        assert_eq!(format!("{}", owned_again.to_shared()), format!("{err}"));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn parsing_errors_round_trip_for_a_spread_of_property_kinds() {
        for t in [
            CssPropertyType::Width,
            CssPropertyType::TextColor,
            CssPropertyType::FontSize,
            CssPropertyType::Opacity,
            CssPropertyType::Transform,
            CssPropertyType::BackgroundContent,
        ] {
            let Err(err) = parse_css_property(t, "\u{1F600}not-valid\u{1F600}") else {
                continue;
            };
            let owned = err.to_contained();
            let round_tripped = owned.to_shared();
            assert_eq!(
                format!("{err}"),
                format!("{round_tripped}"),
                "{} error lost information in to_contained()",
                t.to_str()
            );
        }
    }
}
