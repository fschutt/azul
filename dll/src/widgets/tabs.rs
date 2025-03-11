use alloc::vec::Vec;

use azul_core::{
    callbacks::{Callback, CallbackInfo, RefAny, Update},
    dom::{
        CallbackData, Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass,
        IdOrClass::{Class, Id},
        IdOrClassVec, NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec, TabIndex,
    },
};
use azul_css::*;

const STRING_16146701490593874959: AzString = AzString::from_const_str("sans-serif");
const STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            from: DirectionCorner::Top,
            to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_1400070954008106244_ITEMS,
        ),
    })];

const STYLE_BACKGROUND_CONTENT_15534185073326444643_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            from: DirectionCorner::Top,
            to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_16259001466875079747_ITEMS,
        ),
    })];
const STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    })];
const STYLE_FONT_FAMILY_8122988506401935406_ITEMS: &[StyleFontFamily] =
    &[StyleFontFamily::System(STRING_16146701490593874959)];
const LINEAR_COLOR_STOP_1400070954008106244_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorU {
            r: 240,
            g: 240,
            b: 240,
            a: 255,
        },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorU {
            r: 229,
            g: 229,
            b: 229,
            a: 255,
        },
    },
];
const LINEAR_COLOR_STOP_16259001466875079747_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorU {
            r: 236,
            g: 244,
            b: 252,
            a: 255,
        },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorU {
            r: 221,
            g: 237,
            b: 252,
            a: 255,
        },
    },
];

const CSS_MATCH_13824480602841492081_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active:hover
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_15534185073326444643_ITEMS,
        )),
    )),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-noleftborder
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::None,
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::None,
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::None,
    )),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
        LayoutMarginTop {
            inner: PixelValue::const_px(2),
        },
    ))),
    // .__azul-native-tabs-header p
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight {
            inner: PixelValue::const_px(21),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS,
        )),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::Center,
    ))),
];
const CSS_MATCH_13824480602841492081: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_13824480602841492081_PROPERTIES);

const CSS_MATCH_14575853790110873394_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-active
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(7),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(7),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(3),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(3),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight {
            inner: PixelValue::const_px(23),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BoxSizing(LayoutBoxSizingValue::Exact(
        LayoutBoxSizing::ContentBox,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS,
        )),
    )),
    // .__azul-native-tabs-header p
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight {
            inner: PixelValue::const_px(21),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS,
        )),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::Center,
    ))),
];
const CSS_MATCH_14575853790110873394: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_14575853790110873394_PROPERTIES);

const CSS_MATCH_17290739305197504468_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-header .__azul-native-tabs-before-tabs
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth {
        inner: PixelValue::const_px(2),
    }))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
];
const CSS_MATCH_17290739305197504468: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_17290739305197504468_PROPERTIES);

const CSS_MATCH_18014909903571752977_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-content
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::None)),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::None)),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(StyleBorderTopColorValue::None)),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS,
        )),
    )),
];
const CSS_MATCH_18014909903571752977: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_18014909903571752977_PROPERTIES);

const CSS_MATCH_3088386549906605418_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-header .__azul-native-tabs-after-tabs
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
];
const CSS_MATCH_3088386549906605418: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_3088386549906605418_PROPERTIES);

const CSS_MATCH_4415083954137121609_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active:hover
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_15534185073326444643_ITEMS,
        )),
    )),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-norightborder
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::None,
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::None,
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::None,
    )),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
        LayoutMarginTop {
            inner: PixelValue::const_px(2),
        },
    ))),
    // .__azul-native-tabs-header p
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight {
            inner: PixelValue::const_px(21),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS,
        )),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::Center,
    ))),
];
const CSS_MATCH_4415083954137121609: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_4415083954137121609_PROPERTIES);

const CSS_MATCH_4738503469417034630_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-container
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
];
const CSS_MATCH_4738503469417034630: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_4738503469417034630_PROPERTIES);

const CSS_MATCH_9988039989460234263_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-header
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
];
const CSS_MATCH_9988039989460234263: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_9988039989460234263_PROPERTIES);

// -- NO PADDING
const CSS_MATCH_18014909903571752977_PROPERTIES_NO_PADDING: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-content
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS,
        )),
    )),
];
const CSS_MATCH_18014909903571752977_NO_PADDING: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(
        CSS_MATCH_18014909903571752977_PROPERTIES_NO_PADDING,
    );

const CSS_MATCH_4738503469417034630_PROPERTIES_NO_PADDING: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-container
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
];
const CSS_MATCH_4738503469417034630_NO_PADDING: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(
        CSS_MATCH_4738503469417034630_PROPERTIES_NO_PADDING,
    );

// -- REGULAR_INACTIVE_TAB

const CSS_MATCH_11510695043643111367_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active:hover
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_15534185073326444643_ITEMS,
        )),
    )),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
        LayoutMarginTop {
            inner: PixelValue::const_px(2),
        },
    ))),
    // .__azul-native-tabs-header p
    NodeDataInlineCssProperty::Normal(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight {
            inner: PixelValue::const_px(21),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS,
        )),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::Center,
    ))),
];
const CSS_MATCH_11510695043643111367: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_11510695043643111367_PROPERTIES);

#[derive(Debug, Clone)]
#[repr(C)]
pub struct TabHeader {
    pub tabs: StringVec,
    pub active_tab: usize,
    pub on_click: OptionTabOnClick,
}

impl Default for TabHeader {
    fn default() -> Self {
        Self {
            tabs: StringVec::from_const_slice(&[]),
            active_tab: 0,
            on_click: None.into(),
        }
    }
}

#[repr(C)]
pub struct TabHeaderState {
    pub active_tab: usize,
}

pub type TabOnClickCallbackType =
    extern "C" fn(&mut RefAny, &mut CallbackInfo, &TabHeaderState) -> Update;
impl_callback!(
    TabOnClick,
    OptionTabOnClick,
    TabOnClickCallback,
    TabOnClickCallbackType
);

impl TabHeader {
    pub fn new(tabs: StringVec) -> Self {
        Self {
            tabs,
            active_tab: 0,
            on_click: None.into(),
        }
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut default = Self::default();
        core::mem::swap(&mut default, self);
        default
    }

    pub fn set_active_tab(&mut self, active_tab: usize) {
        self.active_tab = active_tab;
    }

    pub fn with_active_tab(&mut self, active_tab: usize) -> Self {
        let mut s = self.swap_with_default();
        s.set_active_tab(active_tab);
        s
    }

    pub fn set_on_click(&mut self, data: RefAny, on_click: TabOnClickCallbackType) {
        self.on_click = Some(TabOnClick {
            data,
            callback: TabOnClickCallback { cb: on_click },
        })
        .into();
    }

    pub fn with_on_click(&mut self, data: RefAny, on_click: TabOnClickCallbackType) -> Self {
        let mut s = self.swap_with_default();
        s.set_on_click(data, on_click);
        s
    }

    pub fn dom(&mut self) -> Dom {
        use azul_core::dom::CallbackDataVec;

        let on_click_is_some = self.on_click.is_some();

        Dom::div()
            .with_inline_css_props(CSS_MATCH_9988039989460234263)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_6172459441955124689: &[IdOrClass] =
                    &[Class(AzString::from_const_str("__azul-native-tabs-header"))];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6172459441955124689)
            })
            .with_children({
                let mut tab_items = vec![Dom::div()
                    .with_inline_css_props(CSS_MATCH_17290739305197504468)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_8360971686689797550: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-tabs-before-tabs"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8360971686689797550)
                    })];

                let s = self.swap_with_default();

                let dataset = TabLocalDataset {
                    tab_idx: 0,
                    on_click: s.on_click,
                };

                for (tab_idx, tab) in s.tabs.as_ref().iter().enumerate() {
                    let next_tab_is_active = s.active_tab == tab_idx.saturating_add(1);
                    let previous_tab_was_active = if s.active_tab == 0 {
                        false
                    } else {
                        s.active_tab == tab_idx.saturating_sub(1)
                    };

                    let tab_is_active = s.active_tab == tab_idx;

                    // classes for previous tab
                    const IDS_AND_CLASSES_5117007530891373979: &[IdOrClass] = &[
                        Class(AzString::from_const_str(
                            "__azul-native-tabs-tab-norightborder",
                        )),
                        Class(AzString::from_const_str(
                            "__azul-native-tabs-tab-not-active",
                        )),
                    ]; // CSS_MATCH_4415083954137121609

                    // classes for current tab
                    const IDS_AND_CLASSES_15002865554973741556: &[IdOrClass] = &[Class(
                        AzString::from_const_str("__azul-native-tabs-tab-active"),
                    )];

                    // classes for next tab
                    const IDS_AND_CLASSES_16877793354714897051: &[IdOrClass] = &[
                        Class(AzString::from_const_str(
                            "__azul-native-tabs-tab-noleftborder",
                        )),
                        Class(AzString::from_const_str(
                            "__azul-native-tabs-tab-not-active",
                        )),
                    ];

                    // classes for default inactive tab
                    const IDS_AND_CLASSES_INACTIVE: &[IdOrClass] = &[Class(
                        AzString::from_const_str("__azul-native-tabs-tab-not-active"),
                    )];

                    let (ids_and_classes, css_props) = if tab_is_active {
                        (
                            IDS_AND_CLASSES_15002865554973741556,
                            CSS_MATCH_14575853790110873394,
                        )
                    } else if next_tab_is_active {
                        // tab before the active tab
                        (
                            IDS_AND_CLASSES_5117007530891373979,
                            CSS_MATCH_4415083954137121609,
                        )
                    } else if previous_tab_was_active {
                        // tab after the active tab
                        (
                            IDS_AND_CLASSES_16877793354714897051,
                            CSS_MATCH_13824480602841492081,
                        )
                    } else {
                        // default inactive tab
                        (IDS_AND_CLASSES_INACTIVE, CSS_MATCH_11510695043643111367)
                    };

                    let mut dataset = dataset.clone();
                    dataset.tab_idx = tab_idx;
                    let dataset = RefAny::new(dataset);

                    tab_items.push(
                        Dom::text(tab.clone())
                            .with_callbacks(if on_click_is_some {
                                vec![CallbackData {
                                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                                    callback: Callback { cb: on_tab_click },
                                    data: dataset.clone(),
                                }]
                                .into()
                            } else {
                                CallbackDataVec::from_const_slice(&[])
                            })
                            .with_dataset(Some(dataset).into())
                            .with_inline_css_props(css_props)
                            .with_ids_and_classes(IdOrClassVec::from_const_slice(ids_and_classes)),
                    );
                }

                tab_items.push(
                    Dom::div()
                        .with_inline_css_props(CSS_MATCH_3088386549906605418)
                        .with_ids_and_classes({
                            const IDS_AND_CLASSES_11001585590816277275: &[IdOrClass] = &[Class(
                                AzString::from_const_str("__azul-native-tabs-after-tabs"),
                            )];
                            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_11001585590816277275)
                        }),
                );

                tab_items.into()
            })
    }
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct TabContent {
    pub content: Dom,
    pub has_padding: bool,
}

impl Default for TabContent {
    fn default() -> Self {
        Self {
            content: Dom::div(),
            has_padding: true,
        }
    }
}

impl TabContent {
    pub fn new(content: Dom) -> Self {
        Self {
            content,
            has_padding: true,
        }
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut default = Self::default();
        core::mem::swap(&mut default, self);
        default
    }

    pub fn with_padding(&mut self, padding: bool) -> Self {
        let mut s = self.swap_with_default();
        s.set_padding(padding);
        s
    }

    pub fn set_padding(&mut self, padding: bool) {
        self.has_padding = padding;
    }

    pub fn dom(&mut self) -> Dom {
        const IDS_AND_CLASSES_2989815829020816222: &[IdOrClass] = &[Class(
            AzString::from_const_str("__azul-native-tabs-content"),
        )];

        let tab_content_css_style = if self.has_padding {
            CSS_MATCH_18014909903571752977
        } else {
            CSS_MATCH_18014909903571752977_NO_PADDING
        };

        Dom::div()
            .with_inline_css_props(tab_content_css_style)
            .with_children(DomVec::from_vec(vec![Dom::div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(
                    IDS_AND_CLASSES_2989815829020816222,
                ))
                .with_children(DomVec::from_vec(vec![self
                    .content
                    .swap_with_default()]))]))
    }
}

#[derive(Clone)]
struct TabLocalDataset {
    tab_idx: usize,
    on_click: OptionTabOnClick,
}

extern "C" fn on_tab_click(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    fn select_new_tab_inner(data: &mut RefAny, info: &mut CallbackInfo) -> Option<()> {
        let mut tab_local_dataset = data.downcast_mut::<TabLocalDataset>()?;
        let tab_idx = tab_local_dataset.tab_idx;
        let tab_header_state = TabHeaderState {
            active_tab: tab_idx,
        };

        let result = {
            // rustc doesn't understand the borrowing lifetime here
            let tab_local_dataset = &mut *tab_local_dataset;
            let onclick = &mut tab_local_dataset.on_click;

            match onclick.as_mut() {
                Some(TabOnClick { callback, data }) => (callback.cb)(data, info, &tab_header_state),
                None => Update::DoNothing,
            }
        };

        Some(())
    }

    let _ = select_new_tab_inner(data, info);

    Update::RefreshDom
}
