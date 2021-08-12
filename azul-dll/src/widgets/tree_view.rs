use azul_desktop::{
    css::*,
    css::AzString,
    dom::{
        TabIndex, Dom,
        IdOrClass, IdOrClass::Class,
        NodeDataInlineCssPropertyVec,
        IdOrClassVec, DomVec,
        NodeDataInlineCssProperty,
        NodeDataInlineCssProperty::{Normal, Active, Hover, Focus},
    },
    resources::{ImageRef, OptionImageRef},
    callbacks::{RefAny, Update, CallbackInfo},
};
use std::vec::Vec;

const STRING_16146701490593874959: AzString = AzString::from_const_str("sans-serif");
const STYLE_BACKGROUND_CONTENT_2444935983575427872_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 252, g: 252, b: 252, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_3386545019168565479_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners { from: DirectionCorner::Top, to: DirectionCorner::Bottom }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(LINEAR_COLOR_STOP_8524009933333352376_ITEMS),
    })
];
const STYLE_BACKGROUND_CONTENT_11062356617965867290_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 240, g: 240, b: 240, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_15987977139837592998_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 0, g: 0, b: 0, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_16215943235627030128_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 0, g: 206, b: 209, a: 255 })
];
const STYLE_FONT_FAMILY_18001933966972968559_ITEMS: &[StyleFontFamily] = &[
    StyleFontFamily::System(STRING_16146701490593874959)
];
const LINEAR_COLOR_STOP_8524009933333352376_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop { offset: PercentageValue::const_new(0), color: ColorU { r: 250, g: 251, b: 251, a: 255 } },
NormalizedLinearColorStop { offset: PercentageValue::const_new(100), color: ColorU { r: 227, g: 227, b: 227, a: 255 } }
];

const CSS_MATCH_10250347571702901767_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-t-content-minus
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(9) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Top(LayoutTopValue::Exact(LayoutTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Absolute))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::Left(LayoutLeftValue::Exact(LayoutLeft { inner: PixelValue::const_px(9) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(9) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } })))
];
const CSS_MATCH_10250347571702901767: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_10250347571702901767_PROPERTIES);    

const CSS_MATCH_11045010670475678001_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-minus-icon
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(4) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Top(LayoutTopValue::Exact(LayoutTop { inner: PixelValue::const_px(4) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Absolute))),
    NodeDataInlineCssProperty::Normal(CssProperty::Left(LayoutLeftValue::Exact(LayoutLeft { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_15987977139837592998_ITEMS))))
];
const CSS_MATCH_11045010670475678001: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_11045010670475678001_PROPERTIES);    

const CSS_MATCH_1250869685159433269_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-cross-content-2
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(9) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Top(LayoutTopValue::Exact(LayoutTop { inner: PixelValue::const_px(8) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Absolute))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::Left(LayoutLeftValue::Exact(LayoutLeft { inner: PixelValue::const_px(8) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(9) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } })))
];
const CSS_MATCH_1250869685159433269: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_1250869685159433269_PROPERTIES);    

const CSS_MATCH_13401060217940352039_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-view
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Relative))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_18001933966972968559_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 221, g: 221, b: 221, a: 170 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 221, g: 221, b: 221, a: 170 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 221, g: 221, b: 221, a: 170 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 221, g: 221, b: 221, a: 170 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_2444935983575427872_ITEMS))))
];
const CSS_MATCH_13401060217940352039: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_13401060217940352039_PROPERTIES);    

const CSS_MATCH_13463400830017583629_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-pipe-down
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(18) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Relative))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(18) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(LayoutDisplay::Block)))
];
const CSS_MATCH_13463400830017583629: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_13463400830017583629_PROPERTIES);    

const CSS_MATCH_14249021884908901216_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-view-row-label-focusable-wrapper:hover
    NodeDataInlineCssProperty::Hover(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_16215943235627030128_ITEMS)))),
    // .__azul-native-tree-view-row-label-focusable-wrapper
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(LayoutDisplay::Block))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } })))
];
const CSS_MATCH_14249021884908901216: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_14249021884908901216_PROPERTIES);    

const CSS_MATCH_14455923367901630186_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-space-1-filled
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(8) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Relative))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(18) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(LayoutDisplay::Block)))
];
const CSS_MATCH_14455923367901630186: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_14455923367901630186_PROPERTIES);    

const CSS_MATCH_15054086665198995512_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-view-row-label-focusable-wrapper.focused
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } }))),
    // .__azul-native-tree-view-row-label-focusable-wrapper:hover
    NodeDataInlineCssProperty::Hover(CssProperty::TextColor(StyleTextColorValue::Exact(StyleTextColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_16215943235627030128_ITEMS)))),
    // .__azul-native-tree-view-row-label-focusable-wrapper
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(LayoutDisplay::Block))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 255, g: 255, b: 255, a: 0 } })))
];
const CSS_MATCH_15054086665198995512: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_15054086665198995512_PROPERTIES);    

const CSS_MATCH_17035174955428217627_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-t-content
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(8) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Top(LayoutTopValue::Exact(LayoutTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Absolute))),
    NodeDataInlineCssProperty::Normal(CssProperty::Left(LayoutLeftValue::Exact(LayoutLeft { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(18) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } })))
];
const CSS_MATCH_17035174955428217627: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_17035174955428217627_PROPERTIES);    

const CSS_MATCH_17631951240816806439_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-space-1
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(9) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(18) })))
];
const CSS_MATCH_17631951240816806439: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_17631951240816806439_PROPERTIES);    

const CSS_MATCH_17932671798964167701_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-space-1-filled-content
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(18) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Top(LayoutTopValue::Exact(LayoutTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Absolute))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::Left(LayoutLeftValue::Exact(LayoutLeft { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(9) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } })))
];
const CSS_MATCH_17932671798964167701: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_17932671798964167701_PROPERTIES);    

const CSS_MATCH_2883986488332352590_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // body
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_11062356617965867290_ITEMS))))
];
const CSS_MATCH_2883986488332352590: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_2883986488332352590_PROPERTIES);    

const CSS_MATCH_2919526787497691572_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-view-row
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row)))
];
const CSS_MATCH_2919526787497691572: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_2919526787497691572_PROPERTIES);    

const CSS_MATCH_3920366294746786702_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-view-row-label
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(18) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(LayoutDisplay::Block)))
];
const CSS_MATCH_3920366294746786702: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_3920366294746786702_PROPERTIES);    

const CSS_MATCH_5479296065075700509_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-l-content
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Top(LayoutTopValue::Exact(LayoutTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Absolute))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::Left(LayoutLeftValue::Exact(LayoutLeft { inner: PixelValue::const_px(7) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(9) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } })))
];
const CSS_MATCH_5479296065075700509: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_5479296065075700509_PROPERTIES);    

const CSS_MATCH_5748554468056235124_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-pipe-down-content
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(8) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Top(LayoutTopValue::Exact(LayoutTop { inner: PixelValue::const_px(-1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Absolute))),
    NodeDataInlineCssProperty::Normal(CssProperty::Left(LayoutLeftValue::Exact(LayoutLeft { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(19) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } })))
];
const CSS_MATCH_5748554468056235124: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_5748554468056235124_PROPERTIES);    

const CSS_MATCH_6438488809014395635_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-minus-content
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(9) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Top(LayoutTopValue::Exact(LayoutTop { inner: PixelValue::const_px(4) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Absolute))),
    NodeDataInlineCssProperty::Normal(CssProperty::Left(LayoutLeftValue::Exact(LayoutLeft { inner: PixelValue::const_px(4) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::JustifyContent(LayoutJustifyContentValue::Exact(LayoutJustifyContent::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(9) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 145, g: 145, b: 145, a: 170 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 145, g: 145, b: 145, a: 170 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 145, g: 145, b: 145, a: 170 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 145, g: 145, b: 145, a: 170 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_3386545019168565479_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center)))
];
const CSS_MATCH_6438488809014395635: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_6438488809014395635_PROPERTIES);    

const CSS_MATCH_6621536559891676126_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-cross
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(18) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Relative))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(18) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(LayoutDisplay::Block)))
];
const CSS_MATCH_6621536559891676126: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_6621536559891676126_PROPERTIES);    

const CSS_MATCH_8394859448076413888_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-minus
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(18) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Relative))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(18) })))
];
const CSS_MATCH_8394859448076413888: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_8394859448076413888_PROPERTIES);    

const CSS_MATCH_9438342815980407130_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-cross-content-1
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(9) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Top(LayoutTopValue::Exact(LayoutTop { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Absolute))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::Left(LayoutLeftValue::Exact(LayoutLeft { inner: PixelValue::const_px(0) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(9) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Dotted }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 0, g: 0, b: 0, a: 255 } })))
];
const CSS_MATCH_9438342815980407130: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_9438342815980407130_PROPERTIES);    

const CSS_MATCH_9496626968151854549_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-l
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(18) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Relative))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(LayoutOverflow::Visible))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(18) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(LayoutDisplay::Block)))
];
const CSS_MATCH_9496626968151854549: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_9496626968151854549_PROPERTIES);    

const CSS_MATCH_9703015952013196920_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tree-t
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(18) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(LayoutPosition::Relative))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(18) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(LayoutDisplay::Block)))
];
const CSS_MATCH_9703015952013196920: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_9703015952013196920_PROPERTIES);

#[derive(Default)]
#[repr(C)]
pub struct TreeView {
    pub root: AzString,
}

impl TreeView {
    pub fn new(root: AzString) -> Self {
        Self { root }
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut m = TreeView::default();
        core::mem::swap(&mut m, self);
        m
    }

    pub fn dom(self) -> Dom {
        Dom::div()
        .with_inline_css_props(CSS_MATCH_13401060217940352039)
        .with_ids_and_classes({
            const IDS_AND_CLASSES_9837365222714915139: &[IdOrClass] = &[
                    Class(AzString::from_const_str("__azul-native-tree-view")),

            ];
            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_9837365222714915139)
        })
        .with_children(DomVec::from_vec(vec![
            Dom::div()
            .with_inline_css_props(CSS_MATCH_2919526787497691572)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_15453577716812400238: &[IdOrClass] = &[
                            Class(AzString::from_const_str("__azul-native-tree-view-row")),

                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_15453577716812400238)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::div()
                .with_inline_css_props(CSS_MATCH_8394859448076413888)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_5562274544924627603: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-minus")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_5562274544924627603)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_6438488809014395635)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_15170138310983987150: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-minus-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_15170138310983987150)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::div()
                        .with_inline_css_props(CSS_MATCH_11045010670475678001)
                        .with_ids_and_classes({
                            const IDS_AND_CLASSES_276637619792188049: &[IdOrClass] = &[
                                                    Class(AzString::from_const_str("__azul-native-tree-minus-icon")),

                            ];
                            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_276637619792188049)
                        })
                    ]))
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_3920366294746786702)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_17022478219263932868: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-view-row-label")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_17022478219263932868)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_14249021884908901216)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_12039918700145849527: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-view-row-label-focusable-wrapper")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_12039918700145849527)
                    })
                        .with_tab_index(TabIndex::Auto)
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ]))
                ]))
            ])),
            Dom::div()
            .with_inline_css_props(CSS_MATCH_2919526787497691572)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_15453577716812400238: &[IdOrClass] = &[
                            Class(AzString::from_const_str("__azul-native-tree-view-row")),

                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_15453577716812400238)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::div()
                .with_inline_css_props(CSS_MATCH_13463400830017583629)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_8562870525116426737: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-pipe-down")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8562870525116426737)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_5748554468056235124)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_12623539011723615844: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-pipe-down-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_12623539011723615844)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_17631951240816806439)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_13969147764958421470: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-space-1")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_13969147764958421470)
                }),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_9703015952013196920)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_12683940372377849649: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-t")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_12683940372377849649)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_17035174955428217627)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_6948782902341484076: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-t-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6948782902341484076)
                    }),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_10250347571702901767)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_7986348685827112423: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-t-content-minus")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_7986348685827112423)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_3920366294746786702)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_17022478219263932868: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-view-row-label")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_17022478219263932868)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_15054086665198995512)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_966274871881623987: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-view-row-label-focusable-wrapper")),
                        Class(AzString::from_const_str("focused")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_966274871881623987)
                    })
                        .with_tab_index(TabIndex::Auto)
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ]))
                ]))
            ])),
            Dom::div()
            .with_inline_css_props(CSS_MATCH_2919526787497691572)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_15453577716812400238: &[IdOrClass] = &[
                            Class(AzString::from_const_str("__azul-native-tree-view-row")),

                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_15453577716812400238)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::div()
                .with_inline_css_props(CSS_MATCH_9703015952013196920)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_12683940372377849649: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-t")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_12683940372377849649)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_17035174955428217627)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_6948782902341484076: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-t-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6948782902341484076)
                    }),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_10250347571702901767)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_7986348685827112423: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-t-content-minus")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_7986348685827112423)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_14455923367901630186)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_2250273140132504407: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-space-1-filled")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2250273140132504407)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_17932671798964167701)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_11324832106902074912: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-space-1-filled-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_11324832106902074912)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_6621536559891676126)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_3445414501074686586: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-cross")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3445414501074686586)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_9438342815980407130)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_1166576799478159097: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-cross-content-1")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_1166576799478159097)
                    }),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_1250869685159433269)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_5610655148321459708: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-cross-content-2")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_5610655148321459708)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_3920366294746786702)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_17022478219263932868: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-view-row-label")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_17022478219263932868)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_14249021884908901216)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_12039918700145849527: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-view-row-label-focusable-wrapper")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_12039918700145849527)
                    })
                        .with_tab_index(TabIndex::Auto)
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ]))
                ]))
            ])),
            Dom::div()
            .with_inline_css_props(CSS_MATCH_2919526787497691572)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_15453577716812400238: &[IdOrClass] = &[
                            Class(AzString::from_const_str("__azul-native-tree-view-row")),

                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_15453577716812400238)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::div()
                .with_inline_css_props(CSS_MATCH_9703015952013196920)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_12683940372377849649: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-t")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_12683940372377849649)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_17035174955428217627)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_6948782902341484076: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-t-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6948782902341484076)
                    }),
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_10250347571702901767)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_7986348685827112423: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-t-content-minus")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_7986348685827112423)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_14455923367901630186)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_2250273140132504407: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-space-1-filled")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2250273140132504407)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_17932671798964167701)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_11324832106902074912: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-space-1-filled-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_11324832106902074912)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_8394859448076413888)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_5562274544924627603: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-minus")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_5562274544924627603)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_6438488809014395635)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_15170138310983987150: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-minus-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_15170138310983987150)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::div()
                        .with_inline_css_props(CSS_MATCH_11045010670475678001)
                        .with_ids_and_classes({
                            const IDS_AND_CLASSES_276637619792188049: &[IdOrClass] = &[
                                                    Class(AzString::from_const_str("__azul-native-tree-minus-icon")),

                            ];
                            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_276637619792188049)
                        })
                    ]))
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_3920366294746786702)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_17022478219263932868: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-view-row-label")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_17022478219263932868)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_14249021884908901216)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_12039918700145849527: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-view-row-label-focusable-wrapper")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_12039918700145849527)
                    })
                        .with_tab_index(TabIndex::Auto)
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ]))
                ]))
            ])),
            Dom::div()
            .with_inline_css_props(CSS_MATCH_2919526787497691572)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_15453577716812400238: &[IdOrClass] = &[
                            Class(AzString::from_const_str("__azul-native-tree-view-row")),

                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_15453577716812400238)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::div()
                .with_inline_css_props(CSS_MATCH_9496626968151854549)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_11091968853782313624: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-l")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_11091968853782313624)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_5479296065075700509)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_7201172733362059285: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-l-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_7201172733362059285)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_14455923367901630186)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_2250273140132504407: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-space-1-filled")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2250273140132504407)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_17932671798964167701)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_11324832106902074912: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-space-1-filled-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_11324832106902074912)
                    })
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_8394859448076413888)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_5562274544924627603: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-minus")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_5562274544924627603)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_6438488809014395635)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_15170138310983987150: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-minus-content")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_15170138310983987150)
                    })
                    .with_children(DomVec::from_vec(vec![
                        Dom::div()
                        .with_inline_css_props(CSS_MATCH_11045010670475678001)
                        .with_ids_and_classes({
                            const IDS_AND_CLASSES_276637619792188049: &[IdOrClass] = &[
                                                    Class(AzString::from_const_str("__azul-native-tree-minus-icon")),

                            ];
                            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_276637619792188049)
                        })
                    ]))
                ])),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_3920366294746786702)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_17022478219263932868: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tree-view-row-label")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_17022478219263932868)
                })
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                    .with_inline_css_props(CSS_MATCH_14249021884908901216)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_12039918700145849527: &[IdOrClass] = &[
                                            Class(AzString::from_const_str("__azul-native-tree-view-row-label-focusable-wrapper")),

                        ];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_12039918700145849527)
                    })
                        .with_tab_index(TabIndex::Auto)
                    .with_children(DomVec::from_vec(vec![
                        Dom::text(AzString::from_const_str("Hello"))
                    ]))
                ]))
            ]))
        ]))
    }
}
