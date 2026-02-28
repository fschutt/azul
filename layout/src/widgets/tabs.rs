use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};
use azul_css::css::BoxOrStatic;

use crate::callbacks::{Callback, CallbackInfo};

const STRING_16146701490593874959: AzString = AzString::from_const_str("sans-serif");
const STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_1400070954008106244_ITEMS,
        ),
    })];

const STYLE_BACKGROUND_CONTENT_15534185073326444643_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
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
        color: ColorOrSystem::color(ColorU {
            r: 240,
            g: 240,
            b: 240,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 229,
            g: 229,
            b: 229,
            a: 255,
        }),
    },
];
const LINEAR_COLOR_STOP_16259001466875079747_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(ColorU {
            r: 236,
            g: 244,
            b: 252,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 221,
            g: 237,
            b: 252,
            a: 255,
        }),
    },
];

const CSS_MATCH_13824480602841492081_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active:hover
    CssPropertyWithConditions::on_hover(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_15534185073326444643_ITEMS,
        )),
    )),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-noleftborder
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::None,
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::None,
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::None,
    )),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
        LayoutMarginTop {
            inner: PixelValue::const_px(2),
        },
    ))),
    // .__azul-native-tabs-header p
    CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(21)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS,
        )),
    )),
    CssPropertyWithConditions::simple(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::Center,
    ))),
];
const CSS_MATCH_13824480602841492081: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_13824480602841492081_PROPERTIES);

const CSS_MATCH_14575853790110873394_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-active
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(7),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(7),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(3),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(3),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(23)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BoxSizing(LayoutBoxSizingValue::Exact(
        LayoutBoxSizing::ContentBox,
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS,
        )),
    )),
    // .__azul-native-tabs-header p
    CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(21)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS,
        )),
    )),
    CssPropertyWithConditions::simple(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::Center,
    ))),
];
const CSS_MATCH_14575853790110873394: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_14575853790110873394_PROPERTIES);

const CSS_MATCH_17290739305197504468_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header .__azul-native-tabs-before-tabs
    CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
        LayoutWidth::Px(PixelValue::const_px(2)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
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
const CSS_MATCH_17290739305197504468: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_17290739305197504468_PROPERTIES);

const CSS_MATCH_18014909903571752977_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-content
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::None)),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::None)),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(StyleBorderTopColorValue::None)),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS,
        )),
    )),
];
const CSS_MATCH_18014909903571752977: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_18014909903571752977_PROPERTIES);

const CSS_MATCH_3088386549906605418_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header .__azul-native-tabs-after-tabs
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
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
const CSS_MATCH_3088386549906605418: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_3088386549906605418_PROPERTIES);

const CSS_MATCH_4415083954137121609_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active:hover
    CssPropertyWithConditions::on_hover(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_15534185073326444643_ITEMS,
        )),
    )),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-norightborder
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::None,
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::None,
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::None,
    )),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
        LayoutMarginTop {
            inner: PixelValue::const_px(2),
        },
    ))),
    // .__azul-native-tabs-header p
    CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(21)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS,
        )),
    )),
    CssPropertyWithConditions::simple(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::Center,
    ))),
];
const CSS_MATCH_4415083954137121609: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_4415083954137121609_PROPERTIES);

const CSS_MATCH_4738503469417034630_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-container
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
];
const CSS_MATCH_4738503469417034630: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_4738503469417034630_PROPERTIES);

const CSS_MATCH_9988039989460234263_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header
    CssPropertyWithConditions::simple(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
];
const CSS_MATCH_9988039989460234263: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_9988039989460234263_PROPERTIES);

// -- NO PADDING
const CSS_MATCH_18014909903571752977_PROPERTIES_NO_PADDING: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-content
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS,
        )),
    )),
];
const CSS_MATCH_18014909903571752977_NO_PADDING: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(
        CSS_MATCH_18014909903571752977_PROPERTIES_NO_PADDING,
    );

const CSS_MATCH_4738503469417034630_PROPERTIES_NO_PADDING: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-container
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
];
const CSS_MATCH_4738503469417034630_NO_PADDING: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(
        CSS_MATCH_4738503469417034630_PROPERTIES_NO_PADDING,
    );

// -- REGULAR_INACTIVE_TAB

const CSS_MATCH_11510695043643111367_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active:hover
    CssPropertyWithConditions::on_hover(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 126,
                g: 180,
                b: 234,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_15534185073326444643_ITEMS,
        )),
    )),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
        LayoutMarginTop {
            inner: PixelValue::const_px(2),
        },
    ))),
    // .__azul-native-tabs-header p
    CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(21)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS,
        )),
    )),
    CssPropertyWithConditions::simple(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::Center,
    ))),
];
const CSS_MATCH_11510695043643111367: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_11510695043643111367_PROPERTIES);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct TabHeaderState {
    pub active_tab: usize,
}

pub type TabOnClickCallbackType = extern "C" fn(RefAny, CallbackInfo, TabHeaderState) -> Update;
impl_widget_callback!(
    TabOnClick,
    OptionTabOnClick,
    TabOnClickCallback,
    TabOnClickCallbackType
);

impl TabHeader {
    pub fn create(tabs: StringVec) -> Self {
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

    pub fn with_active_tab(mut self, active_tab: usize) -> Self {
        self.set_active_tab(active_tab);
        self
    }

    pub fn set_on_click<C: Into<TabOnClickCallback>>(&mut self, refany: RefAny, on_click: C) {
        self.on_click = Some(TabOnClick {
            refany,
            callback: on_click.into(),
        })
        .into();
    }

    pub fn with_on_click<C: Into<TabOnClickCallback>>(
        mut self,
        refany: RefAny,
        on_click: C,
    ) -> Self {
        self.set_on_click(refany, on_click);
        self
    }

    pub fn dom(self) -> Dom {
        use azul_core::callbacks::CoreCallbackDataVec;

        let on_click_is_some = self.on_click.is_some();

        Dom::create_div()
            .with_css_props(CSS_MATCH_9988039989460234263)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_6172459441955124689: &[IdOrClass] =
                    &[Class(AzString::from_const_str("__azul-native-tabs-header"))];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6172459441955124689)
            })
            .with_children({
                let mut tab_items = vec![Dom::create_div()
                    .with_css_props(CSS_MATCH_17290739305197504468)
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_8360971686689797550: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-tabs-before-tabs"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8360971686689797550)
                    })];

                let dataset = TabLocalDataset {
                    tab_idx: 0,
                    on_click: self.on_click,
                };

                for (tab_idx, tab) in self.tabs.as_ref().iter().enumerate() {
                    let next_tab_is_active = self.active_tab == tab_idx.saturating_add(1);
                    let previous_tab_was_active = if self.active_tab == 0 {
                        false
                    } else {
                        self.active_tab == tab_idx.saturating_sub(1)
                    };

                    let tab_is_active = self.active_tab == tab_idx;

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
                        Dom::create_text(tab.clone())
                            .with_callbacks(if on_click_is_some {
                                vec![CoreCallbackData {
                                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                                    callback: CoreCallback {
                                        cb: on_tab_click as usize,
                                        ctx: azul_core::refany::OptionRefAny::None,
                                    },
                                    refany: dataset.clone(),
                                }]
                                .into()
                            } else {
                                CoreCallbackDataVec::from_const_slice(&[])
                            })
                            .with_dataset(Some(dataset).into())
                            .with_css_props(css_props)
                            .with_ids_and_classes(IdOrClassVec::from_const_slice(ids_and_classes)),
                    );
                }

                tab_items.push(
                    Dom::create_div()
                        .with_css_props(CSS_MATCH_3088386549906605418)
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
            content: Dom::create_div(),
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

    pub fn with_padding(mut self, padding: bool) -> Self {
        self.set_padding(padding);
        self
    }

    pub fn set_padding(&mut self, padding: bool) {
        self.has_padding = padding;
    }

    pub fn dom(self) -> Dom {
        const IDS_AND_CLASSES_2989815829020816222: &[IdOrClass] = &[Class(
            AzString::from_const_str("__azul-native-tabs-content"),
        )];

        let tab_content_css_style = if self.has_padding {
            CSS_MATCH_18014909903571752977
        } else {
            CSS_MATCH_18014909903571752977_NO_PADDING
        };

        Dom::create_div()
            .with_css_props(tab_content_css_style)
            .with_children(DomVec::from_vec(vec![Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(
                    IDS_AND_CLASSES_2989815829020816222,
                ))
                .with_children(DomVec::from_vec(vec![self.content]))]))
    }
}

#[derive(Clone)]
struct TabLocalDataset {
    tab_idx: usize,
    on_click: OptionTabOnClick,
}

extern "C" fn on_tab_click(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    fn select_new_tab_inner(mut refany: RefAny, info: &mut CallbackInfo) -> Option<()> {
        let mut tab_local_dataset = refany.downcast_mut::<TabLocalDataset>()?;
        let tab_idx = tab_local_dataset.tab_idx;
        let tab_header_state = TabHeaderState {
            active_tab: tab_idx,
        };

        let _result = {
            // rustc doesn't understand the borrowing lifetime here
            let tab_local_dataset = &mut *tab_local_dataset;
            let onclick = &mut tab_local_dataset.on_click;

            match onclick.as_mut() {
                Some(TabOnClick { callback, refany }) => {
                    (callback.cb)(refany.clone(), info.clone(), tab_header_state)
                }
                None => Update::DoNothing,
            }
        };

        Some(())
    }

    let _ = select_new_tab_inner(refany, &mut info);

    Update::RefreshDom
}
