
use alloc::vec::Vec;
use azul_desktop::css::*;
use azul_desktop::css::AzString;
use azul_desktop::dom::{
    Dom, IdOrClass, TabIndex,
    IdOrClass::{Id, Class},
    NodeDataInlineCssProperty,
    DomVec, IdOrClassVec, NodeDataInlineCssPropertyVec,
};

const STRING_16146701490593874959: AzString = AzString::from_const_str("sans-serif");
const STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners { from: DirectionCorner::Top, to: DirectionCorner::Bottom }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(LINEAR_COLOR_STOP_1400070954008106244_ITEMS),
    })
];
const STYLE_BACKGROUND_CONTENT_11062356617965867290_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 240, g: 240, b: 240, a: 255 })
];
const STYLE_BACKGROUND_CONTENT_15534185073326444643_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners { from: DirectionCorner::Top, to: DirectionCorner::Bottom }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(LINEAR_COLOR_STOP_16259001466875079747_ITEMS),
    })
];
const STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS: &[StyleBackgroundContent] = &[
    StyleBackgroundContent::Color(ColorU { r: 255, g: 255, b: 255, a: 255 })
];
const STYLE_FONT_FAMILY_8122988506401935406_ITEMS: &[StyleFontFamily] = &[
    StyleFontFamily::System(STRING_16146701490593874959)
];
const LINEAR_COLOR_STOP_1400070954008106244_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop { offset: PercentageValue::const_new(0), color: ColorU { r: 240, g: 240, b: 240, a: 255 } },
NormalizedLinearColorStop { offset: PercentageValue::const_new(100), color: ColorU { r: 229, g: 229, b: 229, a: 255 } }
];
const LINEAR_COLOR_STOP_16259001466875079747_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop { offset: PercentageValue::const_new(0), color: ColorU { r: 236, g: 244, b: 252, a: 255 } },
NormalizedLinearColorStop { offset: PercentageValue::const_new(100), color: ColorU { r: 221, g: 237, b: 252, a: 255 } }
];

const CSS_MATCH_13824480602841492081_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active:hover
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 126, g: 180, b: 234, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 126, g: 180, b: 234, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 126, g: 180, b: 234, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 126, g: 180, b: 234, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_15534185073326444643_ITEMS)))),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-noleftborder
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::None)),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::None)),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::None)),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::MarginTop(LayoutMarginTopValue::Exact(LayoutMarginTop { inner: PixelValue::const_px(2) }))),
    // .__azul-native-tabs-header p
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(21) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center)))
];
const CSS_MATCH_13824480602841492081: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_13824480602841492081_PROPERTIES);

const CSS_MATCH_14575853790110873394_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-active
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(7) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(7) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(3) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(3) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(23) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BoxSizing(LayoutBoxSizingValue::Exact(LayoutBoxSizing::ContentBox))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 255, g: 255, b: 255, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS)))),
    // .__azul-native-tabs-header p
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(21) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center)))
];
const CSS_MATCH_14575853790110873394: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_14575853790110873394_PROPERTIES);

const CSS_MATCH_17290739305197504468_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-header .__azul-native-tabs-before-tabs
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth { inner: PixelValue::const_px(2) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } })))
];
const CSS_MATCH_17290739305197504468: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_17290739305197504468_PROPERTIES);

const CSS_MATCH_18014909903571752977_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-content
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::None)),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::None)),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::None)),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS))))
];
const CSS_MATCH_18014909903571752977: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_18014909903571752977_PROPERTIES);

const CSS_MATCH_3088386549906605418_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-header .__azul-native-tabs-after-tabs
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } })))
];
const CSS_MATCH_3088386549906605418: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_3088386549906605418_PROPERTIES);

const CSS_MATCH_4415083954137121609_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active:hover
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 126, g: 180, b: 234, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 126, g: 180, b: 234, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 126, g: 180, b: 234, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 126, g: 180, b: 234, a: 255 } }))),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_15534185073326444643_ITEMS)))),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-norightborder
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::None)),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::None)),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::None)),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(LayoutPaddingRight { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(LayoutPaddingLeft { inner: PixelValue::const_px(5) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(LayoutPaddingBottom { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(LayoutPaddingTop { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::MarginTop(LayoutMarginTopValue::Exact(LayoutMarginTop { inner: PixelValue::const_px(2) }))),
    // .__azul-native-tabs-header p
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(StyleTextAlign::Center))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight { inner: PixelValue::const_px(21) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth { inner: PixelValue::const_px(1) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(StyleBorderRightStyleValue::Exact(StyleBorderRightStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::Exact(StyleBorderTopStyle { inner: BorderStyle::Solid }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: ColorU { r: 172, g: 172, b: 172, a: 255 } }))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(LayoutAlignItems::Center)))
];
const CSS_MATCH_4415083954137121609: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_4415083954137121609_PROPERTIES);

const CSS_MATCH_4738503469417034630_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-container
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(LayoutFlexGrow { inner: FloatValue::const_new(1) })))
];
const CSS_MATCH_4738503469417034630: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_4738503469417034630_PROPERTIES);

const CSS_MATCH_9988039989460234263_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-header
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(StyleFontSize { inner: PixelValue::const_px(11) }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS)))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row)))
];
const CSS_MATCH_9988039989460234263: NodeDataInlineCssPropertyVec = NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_9988039989460234263_PROPERTIES);

#[repr(C)]
pub struct TabContainer {
    pub tabs: TabVec,
    pub active_tab: usize,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Tab {
    pub title: AzString,
    pub content: Dom,
}

impl Default for TabContainer {
    fn default() -> Self {
        Self {
            tabs: TabVec::from_const_slice(&[]),
            active_tab: 0,
        }
    }
}

impl_vec!(Tab, TabVec, TabVecDestructor);
impl_vec_clone!(Tab, TabVec, TabVecDestructor);
impl_vec_debug!(Tab, TabVec);

impl TabContainer {

    pub fn new(tabs: TabVec) -> Self {
        Self {
            active_tab: 0,
            tabs,
        }
    }

    pub fn set_active_tab(&mut self, active_tab: usize) {
        self.active_tab = active_tab;
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut default = Self::default();
        core::mem::swap(&mut default, self);
        default
    }

    pub fn dom(&mut self) -> Dom {
        Dom::div()
        .with_inline_css_props(CSS_MATCH_4738503469417034630)
        .with_ids_and_classes({
            const IDS_AND_CLASSES_12678371749214821025: &[IdOrClass] = &[
                    Class(AzString::from_const_str("__azul-native-tabs-container")),

            ];
            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_12678371749214821025)
        })
        .with_children(DomVec::from_vec(vec![
            Dom::div()
            .with_inline_css_props(CSS_MATCH_9988039989460234263)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_6172459441955124689: &[IdOrClass] = &[
                            Class(AzString::from_const_str("__azul-native-tabs-header")),

                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6172459441955124689)
            })
            .with_children(DomVec::from_vec(vec![
                Dom::div()
                .with_inline_css_props(CSS_MATCH_17290739305197504468)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_8360971686689797550: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tabs-before-tabs")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8360971686689797550)
                }),
                Dom::text(AzString::from_const_str("Prozesse"))
                .with_inline_css_props(CSS_MATCH_4415083954137121609)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_5117007530891373979: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tabs-tab-norightborder")),
                    Class(AzString::from_const_str("__azul-native-tabs-tab-not-active")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_5117007530891373979)
                }),
                Dom::text(AzString::from_const_str("Leistung"))
                .with_inline_css_props(CSS_MATCH_14575853790110873394)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_15002865554973741556: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tabs-tab-active")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_15002865554973741556)
                }),
                Dom::text(AzString::from_const_str("App-Verlauf"))
                .with_inline_css_props(CSS_MATCH_13824480602841492081)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_16877793354714897051: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tabs-tab-noleftborder")),
                    Class(AzString::from_const_str("__azul-native-tabs-tab-not-active")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_16877793354714897051)
                }),
                Dom::text(AzString::from_const_str("Autostart"))
                .with_inline_css_props(CSS_MATCH_13824480602841492081)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_16877793354714897051: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tabs-tab-noleftborder")),
                    Class(AzString::from_const_str("__azul-native-tabs-tab-not-active")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_16877793354714897051)
                }),
                Dom::text(AzString::from_const_str("Benutzer"))
                .with_inline_css_props(CSS_MATCH_13824480602841492081)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_16877793354714897051: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tabs-tab-noleftborder")),
                    Class(AzString::from_const_str("__azul-native-tabs-tab-not-active")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_16877793354714897051)
                }),
                Dom::text(AzString::from_const_str("Details"))
                .with_inline_css_props(CSS_MATCH_13824480602841492081)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_16877793354714897051: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tabs-tab-noleftborder")),
                    Class(AzString::from_const_str("__azul-native-tabs-tab-not-active")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_16877793354714897051)
                }),
                Dom::text(AzString::from_const_str("Dienste"))
                .with_inline_css_props(CSS_MATCH_13824480602841492081)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_16877793354714897051: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tabs-tab-noleftborder")),
                    Class(AzString::from_const_str("__azul-native-tabs-tab-not-active")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_16877793354714897051)
                }),
                Dom::div()
                .with_inline_css_props(CSS_MATCH_3088386549906605418)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_11001585590816277275: &[IdOrClass] = &[
                                    Class(AzString::from_const_str("__azul-native-tabs-after-tabs")),

                    ];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_11001585590816277275)
                })
            ])),
            Dom::div()
            .with_inline_css_props(CSS_MATCH_18014909903571752977)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_2989815829020816222: &[IdOrClass] = &[
                            Class(AzString::from_const_str("__azul-native-tabs-content")),

                ];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2989815829020816222)
            })
        ]))
    }
}