//! Native-styled tab widget consisting of a [`TabHeader`] (the clickable tab bar)
//! and [`TabContent`] (the panel shown for the active tab).
//!
//! Styling emulates the Windows-native tab control appearance via inline CSS
//! constants.

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
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

const STRING_16146701490593874959: AzString = AzString::from_const_str("system:ui");
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

/// Header bar for a tab widget, containing the clickable tab labels.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TabHeader {
    /// Labels for each tab.
    pub tabs: StringVec,
    /// Zero-based index of the currently active tab.
    pub active_tab: usize,
    /// Optional callback invoked when a tab is clicked.
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

/// State passed to the tab-click callback, indicating which tab was selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct TabHeaderState {
    /// Zero-based index of the newly selected tab.
    pub active_tab: usize,
}

/// Signature for the tab-click callback function.
pub type TabOnClickCallbackType = extern "C" fn(RefAny, CallbackInfo, TabHeaderState) -> Update;
impl_widget_callback!(
    TabOnClick,
    OptionTabOnClick,
    TabOnClickCallback,
    TabOnClickCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TabOnClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TAB_ON_CLICK_INVOKER,
    invoker_ty:     AzTabOnClickCallbackInvoker,
    thunk_fn:       az_tab_on_click_callback_thunk,
    setter_fn:      AzApp_setTabOnClickCallbackInvoker,
    from_handle_fn: AzTabOnClickCallback_createFromHostHandle,
    extra_args:     [ state: TabHeaderState ],
}

impl TabHeader {
    #[must_use] pub fn create(tabs: StringVec) -> Self {
        Self {
            tabs,
            active_tab: 0,
            on_click: None.into(),
        }
    }

    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut default = Self::default();
        core::mem::swap(&mut default, self);
        default
    }

    pub const fn set_active_tab(&mut self, active_tab: usize) {
        self.active_tab = active_tab;
    }

    #[must_use] pub const fn with_active_tab(mut self, active_tab: usize) -> Self {
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

    #[must_use]
    pub fn with_on_click<C: Into<TabOnClickCallback>>(
        mut self,
        refany: RefAny,
        on_click: C,
    ) -> Self {
        self.set_on_click(refany, on_click);
        self
    }

    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    #[must_use] pub fn dom(self) -> Dom {
        use azul_core::callbacks::CoreCallbackDataVec;

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
                        crate::widgets::widget_p_with_text(tab.clone())
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

/// Content panel displayed beneath the active tab in a tab widget.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TabContent {
    /// The DOM subtree shown as the tab's content area.
    pub content: Dom,
    /// Whether the content area includes default padding.
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
    #[must_use] pub const fn new(content: Dom) -> Self {
        Self {
            content,
            has_padding: true,
        }
    }

    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut default = Self::default();
        core::mem::swap(&mut default, self);
        default
    }

    #[must_use] pub const fn with_padding(mut self, padding: bool) -> Self {
        self.set_padding(padding);
        self
    }

    pub const fn set_padding(&mut self, padding: bool) {
        self.has_padding = padding;
    }

    #[must_use] pub fn dom(self) -> Dom {
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

extern "C" fn on_tab_click(mut refany: RefAny, info: CallbackInfo) -> Update {
    fn select_new_tab_inner(mut refany: RefAny, info: &CallbackInfo) -> Option<Update> {
        let mut tab_local_dataset = refany.downcast_mut::<TabLocalDataset>()?;
        let tab_idx = tab_local_dataset.tab_idx;
        let tab_header_state = TabHeaderState {
            active_tab: tab_idx,
        };

        let result = {
            // rustc doesn't understand the borrowing lifetime here
            let tab_local_dataset = &mut *tab_local_dataset;
            let onclick = &mut tab_local_dataset.on_click;

            match onclick.as_mut() {
                Some(TabOnClick { callback, refany }) => {
                    (callback.cb)(refany.clone(), *info, tab_header_state)
                }
                None => Update::DoNothing,
            }
        };

        Some(result)
    }

    select_new_tab_inner(refany, &info).unwrap_or(Update::RefreshDom)
}

#[cfg(test)]
mod autotest_generated {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        geom::OptionLogicalPosition,
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::system::SystemStyle;
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        window::LayoutWindow,
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    const CLASS_ACTIVE: &str = "__azul-native-tabs-tab-active";
    const CLASS_NOT_ACTIVE: &str = "__azul-native-tabs-tab-not-active";
    const CLASS_NO_LEFT: &str = "__azul-native-tabs-tab-noleftborder";
    const CLASS_NO_RIGHT: &str = "__azul-native-tabs-tab-norightborder";
    const CLASS_HEADER: &str = "__azul-native-tabs-header";
    const CLASS_BEFORE: &str = "__azul-native-tabs-before-tabs";
    const CLASS_AFTER: &str = "__azul-native-tabs-after-tabs";
    const CLASS_CONTENT: &str = "__azul-native-tabs-content";

    fn strings(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect())
    }

    fn numbered_labels(n: usize) -> StringVec {
        StringVec::from_vec((0..n).map(|i| AzString::from(format!("tab {i}"))).collect())
    }

    /// The text of a text node, looking through the `<p>` block wrapper the
    /// label convention mandates (`p > text`).
    fn text_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            NodeType::P => match node.children.as_ref() {
                [only] => match only.root.get_node_type() {
                    NodeType::Text(s) => Some(s.as_ref().as_str()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    fn classes(node: &Dom) -> Vec<String> {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    fn class_strs(node: &Dom) -> Vec<&'static str> {
        // The widget only ever attaches the eight `&'static str` classes above;
        // map back to them so the tests can compare against string literals.
        classes(node)
            .into_iter()
            .map(|c| {
                [
                    CLASS_ACTIVE,
                    CLASS_NOT_ACTIVE,
                    CLASS_NO_LEFT,
                    CLASS_NO_RIGHT,
                    CLASS_HEADER,
                    CLASS_BEFORE,
                    CLASS_AFTER,
                    CLASS_CONTENT,
                ]
                .into_iter()
                .find(|known| *known == c)
                .unwrap_or_else(|| panic!("tabs.rs emitted an unknown class: {c}"))
            })
            .collect()
    }

    /// A style vec as `(property, condition-count)` pairs in declaration order —
    /// the exact shape `Css::from(CssPropertyWithConditionsVec)` preserves.
    fn declared(v: &CssPropertyWithConditionsVec) -> Vec<(CssProperty, usize)> {
        v.as_ref()
            .iter()
            .map(|p| (p.property.clone(), p.apply_if.as_ref().len()))
            .collect()
    }

    /// The same view, read back off a rendered node's inline `Css`.
    fn inline_declared(node: &Dom) -> Vec<(CssProperty, usize)> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, conds)| (p.clone(), conds.as_ref().len()))
            .collect()
    }

    /// Every declaration in `v` matching `pred`, in declaration order.
    fn decls_where(
        v: &CssPropertyWithConditionsVec,
        pred: fn(&CssProperty) -> bool,
    ) -> Vec<CssProperty> {
        v.as_ref()
            .iter()
            .filter(|p| pred(&p.property))
            .map(|p| p.property.clone())
            .collect()
    }

    /// Same, but skipping the `:hover` block — these styles declare the border
    /// properties once for hover and again unconditionally, so a raw filter would
    /// mix the two sets.
    fn plain_decls_where(
        v: &CssPropertyWithConditionsVec,
        pred: fn(&CssProperty) -> bool,
    ) -> Vec<CssProperty> {
        v.as_ref()
            .iter()
            .filter(|p| p.apply_if.as_ref().is_empty() && pred(&p.property))
            .map(|p| p.property.clone())
            .collect()
    }

    /// The style vec the widget must pair with a given class combination.
    fn style_for_classes(cls: &[&str]) -> CssPropertyWithConditionsVec {
        let cls = cls.to_vec();
        if cls == [CLASS_ACTIVE] {
            CSS_MATCH_14575853790110873394
        } else if cls == [CLASS_NO_RIGHT, CLASS_NOT_ACTIVE] {
            CSS_MATCH_4415083954137121609
        } else if cls == [CLASS_NO_LEFT, CLASS_NOT_ACTIVE] {
            CSS_MATCH_13824480602841492081
        } else if cls == [CLASS_NOT_ACTIVE] {
            CSS_MATCH_11510695043643111367
        } else {
            panic!("unexpected class combination on a tab node: {cls:?}");
        }
    }

    /// The dataset `RefAny` a rendered tab node carries (cloned, so the caller
    /// can `downcast_*` it without borrowing the Dom mutably).
    fn dataset_of(node: &Dom) -> RefAny {
        node.root
            .get_dataset()
            .expect("every tab node carries a TabLocalDataset")
            .clone()
    }

    fn tab_idx_of(node: &Dom) -> usize {
        let mut ds = dataset_of(node);
        let tab_idx = ds
            .downcast_ref::<TabLocalDataset>()
            .expect("the dataset must be a TabLocalDataset")
            .tab_idx;
        tab_idx
    }

    /// A `RefAny` payload recording every state a user `on_click` observes.
    #[derive(Default)]
    struct ClickLog {
        seen: Vec<TabHeaderState>,
    }

    extern "C" fn record_click(
        mut refany: RefAny,
        _: CallbackInfo,
        state: TabHeaderState,
    ) -> Update {
        if let Some(mut log) = refany.downcast_mut::<ClickLog>() {
            log.seen.push(state);
        }
        Update::RefreshDom
    }

    extern "C" fn click_do_nothing(_: RefAny, _: CallbackInfo, _: TabHeaderState) -> Update {
        Update::DoNothing
    }

    extern "C" fn click_refresh_all(_: RefAny, _: CallbackInfo, _: TabHeaderState) -> Update {
        Update::RefreshDomAllWindows
    }

    /// Forces the `fn`-item -> `fn`-pointer coercion the `Into` bound needs.
    fn cb(f: TabOnClickCallbackType) -> TabOnClickCallback {
        f.into()
    }

    fn logged(refany: &mut RefAny) -> Vec<TabHeaderState> {
        refany
            .downcast_ref::<ClickLog>()
            .expect("payload must still be a ClickLog")
            .seen
            .clone()
    }

    fn dataset(tab_idx: usize, on_click: OptionTabOnClick) -> RefAny {
        RefAny::new(TabLocalDataset { tab_idx, on_click })
    }

    fn click_handler(refany: RefAny, callback: TabOnClickCallbackType) -> OptionTabOnClick {
        Some(TabOnClick {
            refany,
            callback: cb(callback),
        })
        .into()
    }

    /// Invokes `on_tab_click` with a minimal `CallbackInfo` (the handler never
    /// touches the layout window — it only downcasts its own dataset — so an
    /// empty `LayoutWindow` and node 0 as the hit node are enough).
    /// Returns the `Update` plus every recorded `CallbackChange`.
    fn run_click(data: RefAny) -> (Update, Vec<CallbackChange>) {
        let layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
            },
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let update = on_tab_click(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (update, recorded)
    }

    // ==================================================================
    // TabHeader::create
    // ==================================================================

    #[test]
    fn create_keeps_the_label_vec_verbatim_and_starts_inert() {
        for labels in [
            strings(&[]),
            strings(&["only"]),
            strings(&["a", "b", "c"]),
            numbered_labels(1000),
        ] {
            let expected: Vec<AzString> = labels.as_ref().to_vec();
            let header = TabHeader::create(labels);

            assert_eq!(
                header.tabs.as_ref(),
                expected.as_slice(),
                "create must not reorder, drop or rewrite labels"
            );
            assert_eq!(header.active_tab, 0, "a fresh header selects the first tab");
            assert!(
                header.on_click.is_none(),
                "create must not install a callback"
            );
        }
    }

    #[test]
    fn create_accepts_an_empty_label_vec_and_still_renders_the_spacers() {
        // Zero tabs is the degenerate case: the loop body never runs, but the
        // before/after spacers must still be emitted or the header collapses.
        let dom = TabHeader::create(strings(&[])).dom();
        let children = dom.children.as_ref();

        assert_eq!(children.len(), 2, "an empty header is just the two spacers");
        assert_eq!(class_strs(&children[0]), vec![CLASS_BEFORE]);
        assert_eq!(class_strs(&children[1]), vec![CLASS_AFTER]);
    }

    #[test]
    fn create_round_trips_pathological_labels_through_the_dom() {
        // The label is only ever cloned into a `NodeType::Text`, never parsed,
        // trimmed or NUL-terminated — so every byte must survive verbatim.
        let long = "\u{e9}".repeat(50_000);
        let pathological: Vec<String> = vec![
            String::new(),
            " ".to_string(),
            "\0embedded\0nul\0".to_string(),
            "\u{1F600}\u{1F3F4}\u{E0067}\u{E007F}".to_string(), // emoji + tag sequence
            "a\u{0301}\u{0327}\u{0328}".to_string(),            // stacked combining marks
            "\u{202E}reversed\u{202C}".to_string(),             // RTL override
            "\r\n\t".to_string(),
            "\u{FFFD}\u{FEFF}".to_string(), // replacement char + BOM
            long.clone(),
        ];

        let labels = StringVec::from_vec(
            pathological
                .iter()
                .map(|s| AzString::from(s.clone()))
                .collect(),
        );
        let dom = TabHeader::create(labels).dom();
        let children = dom.children.as_ref();

        assert_eq!(children.len(), pathological.len() + 2);
        for (i, expected) in pathological.iter().enumerate() {
            assert_eq!(
                text_of(&children[i + 1]),
                Some(expected.as_str()),
                "label {i} did not round-trip through the DOM"
            );
        }
    }

    // ==================================================================
    // TabHeader::set_active_tab / with_active_tab  (numeric)
    // ==================================================================

    #[test]
    fn set_active_tab_stores_any_usize_without_clamping_or_panicking() {
        // `active_tab` is an unsigned index with no documented upper bound and
        // no relation to `tabs.len()` — there is no signed path to test, so the
        // adversarial values are 0, the wrap-around of 0 and the two extremes.
        let extremes = [
            0usize,
            1,
            2,
            usize::MAX / 2,
            usize::MAX - 1,
            usize::MAX,
            0usize.wrapping_sub(1),
        ];

        for value in extremes {
            let mut header = TabHeader::create(strings(&["a", "b", "c"]));
            header.set_active_tab(value);
            assert_eq!(
                header.active_tab, value,
                "set_active_tab must store the index verbatim (no clamp to len)"
            );

            // Idempotent + last-write-wins.
            header.set_active_tab(value);
            assert_eq!(header.active_tab, value);
            header.set_active_tab(0);
            assert_eq!(header.active_tab, 0, "the last write must win");
        }
    }

    #[test]
    fn set_active_tab_touches_nothing_but_the_index() {
        let mut header = TabHeader::create(strings(&["a", "b"]));
        header.set_on_click(RefAny::new(ClickLog::default()), cb(record_click));

        header.set_active_tab(usize::MAX);

        assert_eq!(
            header.tabs.as_ref(),
            strings(&["a", "b"]).as_ref(),
            "the labels must be untouched"
        );
        assert!(
            header.on_click.is_some(),
            "the callback must survive a selection change"
        );
    }

    #[test]
    fn with_active_tab_agrees_with_set_active_tab_at_every_extreme() {
        for value in [0usize, 1, usize::MAX / 2, usize::MAX - 1, usize::MAX] {
            let built = TabHeader::create(strings(&["a", "b"])).with_active_tab(value);
            let mut mutated = TabHeader::create(strings(&["a", "b"]));
            mutated.set_active_tab(value);

            assert_eq!(built.active_tab, mutated.active_tab);
            assert_eq!(built.active_tab, value);
            assert_eq!(built.tabs.as_ref(), mutated.tabs.as_ref());
            assert!(built.on_click.is_none() && mutated.on_click.is_none());
        }
    }

    #[test]
    fn with_active_tab_chains_last_wins() {
        let header = TabHeader::create(strings(&["a"]))
            .with_active_tab(usize::MAX)
            .with_active_tab(7)
            .with_active_tab(0);
        assert_eq!(header.active_tab, 0);
    }

    #[test]
    fn dom_never_marks_a_tab_active_when_the_index_is_out_of_range() {
        // An out-of-range selection must degrade to "nothing selected" rather
        // than panicking or wrapping onto some other tab.
        let n = 4usize;
        for active in [n, n + 1, n + 2, usize::MAX / 2, usize::MAX - 1, usize::MAX] {
            let dom = TabHeader::create(numbered_labels(n))
                .with_active_tab(active)
                .dom();
            let children = dom.children.as_ref();
            assert_eq!(children.len(), n + 2, "active={active}: wrong child count");

            for (i, node) in children[1..=n].iter().enumerate() {
                let cls = class_strs(node);
                assert!(
                    !cls.contains(&CLASS_ACTIVE),
                    "active={active}: tab {i} must not be styled active"
                );
                assert!(
                    cls.contains(&CLASS_NOT_ACTIVE),
                    "active={active}: tab {i} must be styled inactive"
                );
            }
        }
    }

    #[test]
    fn dom_at_usize_max_gives_every_tab_the_plain_inactive_style() {
        // `tab_idx.saturating_add(1)` / `saturating_sub(1)` must not wrap into a
        // false neighbour match at the extreme.
        let n = 5usize;
        let dom = TabHeader::create(numbered_labels(n))
            .with_active_tab(usize::MAX)
            .dom();

        for (i, node) in dom.children.as_ref()[1..=n].iter().enumerate() {
            assert_eq!(
                class_strs(node),
                vec![CLASS_NOT_ACTIVE],
                "tab {i} must carry the plain inactive class only"
            );
            assert_eq!(
                inline_declared(node),
                declared(&CSS_MATCH_11510695043643111367),
                "tab {i} must carry the plain inactive style"
            );
        }
    }

    #[test]
    fn dom_one_past_the_end_still_seams_the_last_tab() {
        // PINNED QUIRK: with `active_tab == tabs.len()` no tab is active, yet the
        // *last* tab still matches `active_tab == tab_idx + 1` and loses its right
        // border — a visible seam against the after-tabs spacer. Deliberately
        // pinned: if the widget starts range-checking `active_tab`, this flips.
        let n = 3usize;
        let dom = TabHeader::create(numbered_labels(n)).with_active_tab(n).dom();
        let children = dom.children.as_ref();

        assert_eq!(
            class_strs(&children[n]),
            vec![CLASS_NO_RIGHT, CLASS_NOT_ACTIVE],
            "the last tab is styled as if the (non-existent) next tab were active"
        );
        for (i, node) in children[1..n].iter().enumerate() {
            assert_eq!(class_strs(node), vec![CLASS_NOT_ACTIVE], "tab {i}");
        }
    }

    // ==================================================================
    // TabHeader::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_moves_the_state_out_and_leaves_a_default() {
        let mut header = TabHeader::create(strings(&["a", "b", "c"])).with_active_tab(2);
        header.set_on_click(RefAny::new(ClickLog::default()), cb(record_click));

        let taken = header.swap_with_default();

        assert_eq!(taken.tabs.as_ref(), strings(&["a", "b", "c"]).as_ref());
        assert_eq!(taken.active_tab, 2);
        assert!(taken.on_click.is_some(), "the callback moves out with the state");

        assert!(header.tabs.as_ref().is_empty(), "the husk must have no tabs");
        assert_eq!(header.active_tab, 0);
        assert!(
            header.on_click.is_none(),
            "the husk must not keep a live callback"
        );
    }

    #[test]
    fn swap_with_default_twice_yields_a_default_the_second_time() {
        let mut header = TabHeader::create(strings(&["a"])).with_active_tab(usize::MAX);
        let first = header.swap_with_default();
        let second = header.swap_with_default();

        assert_eq!(first.active_tab, usize::MAX);
        assert_eq!(first.tabs.as_ref().len(), 1);
        assert_eq!(second.active_tab, 0);
        assert!(second.tabs.as_ref().is_empty());
    }

    #[test]
    fn swap_with_default_releases_the_callback_payload_when_dropped() {
        let user = RefAny::new(ClickLog::default());
        let mut header = TabHeader::create(strings(&["a"]));
        header.set_on_click(user.clone(), cb(record_click));
        assert_eq!(user.get_ref_count(), 2);

        drop(header.swap_with_default());
        assert_eq!(
            user.get_ref_count(),
            1,
            "the swapped-out header must drop its payload clone"
        );
    }

    // ==================================================================
    // TabHeader::set_on_click / with_on_click
    // ==================================================================

    #[test]
    fn with_on_click_installs_the_callback_and_touches_nothing_else() {
        let before = TabHeader::create(strings(&["a", "b"])).with_active_tab(1);
        let mut after = TabHeader::create(strings(&["a", "b"]))
            .with_active_tab(1)
            .with_on_click(RefAny::new(ClickLog::default()), cb(record_click));

        assert_eq!(after.tabs.as_ref(), before.tabs.as_ref());
        assert_eq!(after.active_tab, before.active_tab);

        let installed = after
            .on_click
            .as_mut()
            .expect("with_on_click must install Some(..)");
        assert_eq!(installed.callback.cb as usize, record_click as usize);
        assert!(
            installed.refany.downcast_ref::<ClickLog>().is_some(),
            "the payload must be stored as handed in"
        );
    }

    #[test]
    fn set_on_click_overwrites_the_previous_callback_and_payload() {
        let mut header = TabHeader::create(strings(&["a"]));
        header.set_on_click(RefAny::new(ClickLog::default()), cb(record_click));
        header.set_on_click(RefAny::new(42u32), cb(click_do_nothing));

        let installed = header.on_click.as_mut().expect("still Some after overwrite");
        assert_eq!(
            installed.callback.cb as usize,
            click_do_nothing as usize,
            "the last set_on_click must win"
        );
        assert_eq!(
            installed.refany.downcast_ref::<u32>().map(|v| *v),
            Some(42),
            "the payload must be replaced together with the fn pointer"
        );
        assert!(
            installed.refany.downcast_ref::<ClickLog>().is_none(),
            "the stale payload must be gone"
        );
    }

    #[test]
    fn set_on_click_drops_the_previous_payload() {
        let first = RefAny::new(ClickLog::default());
        let second = RefAny::new(ClickLog::default());
        let mut header = TabHeader::create(strings(&["a"]));

        header.set_on_click(first.clone(), cb(record_click));
        assert_eq!(first.get_ref_count(), 2);
        header.set_on_click(second.clone(), cb(record_click));

        assert_eq!(
            first.get_ref_count(),
            1,
            "overwriting the callback must release the old payload"
        );
        assert_eq!(second.get_ref_count(), 2);
    }

    #[test]
    fn generic_callback_conversion_round_trips_the_fn_pointer() {
        // The FFI path (`From<Callback>`) transmutes the fn pointer; a corrupted
        // value would be an unconditional jump into garbage at click time.
        // (Never invoked here.)
        let raw = record_click as usize;
        let generic = Callback {
            cb: unsafe { core::mem::transmute::<usize, crate::callbacks::CallbackType>(raw) },
            ctx: OptionRefAny::None,
        };
        let converted: TabOnClickCallback = generic.into();
        assert_eq!(converted.cb as usize, raw);
    }

    // ==================================================================
    // TabHeader::dom — structure
    // ==================================================================

    #[test]
    fn dom_is_a_header_div_wrapping_spacer_tabs_spacer() {
        for n in [0usize, 1, 2, 3, 17] {
            let dom = TabHeader::create(numbered_labels(n)).dom();

            assert_eq!(class_strs(&dom), vec![CLASS_HEADER]);
            assert_eq!(inline_declared(&dom), declared(&CSS_MATCH_9988039989460234263));
            assert!(
                matches!(dom.root.get_node_type(), NodeType::Div),
                "the header itself is a plain div"
            );

            let children = dom.children.as_ref();
            assert_eq!(children.len(), n + 2, "n={n}: spacer + {n} tabs + spacer");

            assert_eq!(class_strs(&children[0]), vec![CLASS_BEFORE]);
            assert_eq!(
                inline_declared(&children[0]),
                declared(&CSS_MATCH_17290739305197504468)
            );
            assert_eq!(class_strs(&children[n + 1]), vec![CLASS_AFTER]);
            assert_eq!(
                inline_declared(&children[n + 1]),
                declared(&CSS_MATCH_3088386549906605418)
            );

            for (i, node) in children[1..=n].iter().enumerate() {
                assert_eq!(
                    text_of(node),
                    Some(format!("tab {i}").as_str()),
                    "n={n}: tab {i} sits at sibling position {}",
                    i + 1
                );
            }
        }
    }

    #[test]
    fn dom_marks_exactly_one_tab_active_for_every_in_range_index() {
        let n = 6usize;
        for active in 0..n {
            let dom = TabHeader::create(numbered_labels(n))
                .with_active_tab(active)
                .dom();
            let children = dom.children.as_ref();

            let active_nodes: Vec<usize> = children[1..=n]
                .iter()
                .enumerate()
                .filter(|(_, node)| class_strs(node).contains(&CLASS_ACTIVE))
                .map(|(i, _)| i)
                .collect();
            assert_eq!(
                active_nodes,
                vec![active],
                "exactly the selected tab must carry the active class"
            );

            for (i, node) in children[1..=n].iter().enumerate() {
                let cls = class_strs(node);
                assert_eq!(
                    cls.contains(&CLASS_ACTIVE),
                    !cls.contains(&CLASS_NOT_ACTIVE),
                    "active={active}, tab {i}: active/not-active must be exclusive"
                );
                assert!(
                    !(cls.contains(&CLASS_NO_LEFT) && cls.contains(&CLASS_NO_RIGHT)),
                    "active={active}, tab {i}: a tab cannot drop both side borders"
                );
                assert!(
                    !(cls.contains(&CLASS_ACTIVE)
                        && (cls.contains(&CLASS_NO_LEFT) || cls.contains(&CLASS_NO_RIGHT))),
                    "active={active}, tab {i}: the active tab keeps both side borders"
                );
            }
        }
    }

    #[test]
    fn dom_gives_the_neighbours_of_the_active_tab_their_seam_classes() {
        // active = 2 of 5: tab 1 loses its right border, tab 3 its left one.
        let dom = TabHeader::create(numbered_labels(5)).with_active_tab(2).dom();
        let children = dom.children.as_ref();

        assert_eq!(class_strs(&children[1]), vec![CLASS_NOT_ACTIVE], "tab 0");
        assert_eq!(
            class_strs(&children[2]),
            vec![CLASS_NO_RIGHT, CLASS_NOT_ACTIVE],
            "tab 1 sits before the active tab"
        );
        assert_eq!(class_strs(&children[3]), vec![CLASS_ACTIVE], "tab 2");
        assert_eq!(
            class_strs(&children[4]),
            vec![CLASS_NO_LEFT, CLASS_NOT_ACTIVE],
            "tab 3 sits after the active tab"
        );
        assert_eq!(class_strs(&children[5]), vec![CLASS_NOT_ACTIVE], "tab 4");
    }

    #[test]
    fn dom_first_tab_active_leaves_the_second_tab_with_a_left_border() {
        // PINNED BUG: `previous_tab_was_active` short-circuits on
        // `self.active_tab == 0`, so for the *default* selection (tab 0) the tab
        // right after the active one never gets `-noleftborder` — it draws a left
        // border straight against the active tab's right edge. Every other
        // selection (see the test above) does emit the class. Pinned as-is so the
        // fix flips this test loudly.
        let dom = TabHeader::create(numbered_labels(4)).with_active_tab(0).dom();
        let children = dom.children.as_ref();

        assert_eq!(class_strs(&children[1]), vec![CLASS_ACTIVE], "tab 0");
        assert_eq!(
            class_strs(&children[2]),
            vec![CLASS_NOT_ACTIVE],
            "tab 1 should be [-noleftborder, -not-active] like the active=1..n case"
        );
        assert_eq!(class_strs(&children[3]), vec![CLASS_NOT_ACTIVE], "tab 2");

        // The symmetric neighbour (active = 1) *does* get the class, which is what
        // makes the case above an inconsistency rather than a design choice.
        let dom = TabHeader::create(numbered_labels(4)).with_active_tab(1).dom();
        assert_eq!(
            class_strs(&dom.children.as_ref()[3]),
            vec![CLASS_NO_LEFT, CLASS_NOT_ACTIVE],
            "tab 2 after active tab 1"
        );
    }

    #[test]
    fn dom_pairs_every_tab_style_with_the_classes_it_advertises() {
        // A swapped class/style pair (e.g. the "noleftborder" node getting the
        // style that nulls the *right* border) would be invisible in a class-only
        // check, so compare the rendered inline style against the vec the class
        // combination demands.
        let n = 6usize;
        for active in [0usize, 1, 2, n - 1, n, usize::MAX] {
            let dom = TabHeader::create(numbered_labels(n))
                .with_active_tab(active)
                .dom();
            for (i, node) in dom.children.as_ref()[1..=n].iter().enumerate() {
                let cls = class_strs(node);
                assert_eq!(
                    inline_declared(node),
                    declared(&style_for_classes(&cls)),
                    "active={active}, tab {i}: style does not match classes {cls:?}"
                );
            }
        }
    }

    #[test]
    fn neighbour_styles_null_the_border_on_the_side_their_class_names_claim() {
        let no_left = plain_decls_where(&CSS_MATCH_13824480602841492081, |p| {
            matches!(p, CssProperty::BorderLeftWidth(_))
        });
        let no_right = plain_decls_where(&CSS_MATCH_4415083954137121609, |p| {
            matches!(p, CssProperty::BorderRightWidth(_))
        });

        assert_eq!(
            no_left.first(),
            Some(&CssProperty::BorderLeftWidth(
                LayoutBorderLeftWidthValue::None
            )),
            "the -noleftborder style must null the LEFT border first"
        );
        assert_eq!(
            no_right.first(),
            Some(&CssProperty::BorderRightWidth(
                LayoutBorderRightWidthValue::None
            )),
            "the -norightborder style must null the RIGHT border first"
        );

        // ...and must not null the opposite side.
        assert!(
            !decls_where(&CSS_MATCH_13824480602841492081, |p| matches!(
                p,
                CssProperty::BorderRightWidth(_)
            ))
            .contains(&CssProperty::BorderRightWidth(
                LayoutBorderRightWidthValue::None
            )),
            "the -noleftborder style must keep the right border"
        );
        assert!(
            !decls_where(&CSS_MATCH_4415083954137121609, |p| matches!(
                p,
                CssProperty::BorderLeftWidth(_)
            ))
            .contains(&CssProperty::BorderLeftWidth(
                LayoutBorderLeftWidthValue::None
            )),
            "the -norightborder style must keep the left border"
        );

        // The plain inactive style (no active neighbour) keeps both side borders:
        // exactly one unconditional declaration per side, and neither is `None`.
        let plain_left = plain_decls_where(&CSS_MATCH_11510695043643111367, |p| {
            matches!(p, CssProperty::BorderLeftWidth(_))
        });
        let plain_right = plain_decls_where(&CSS_MATCH_11510695043643111367, |p| {
            matches!(p, CssProperty::BorderRightWidth(_))
        });
        assert_eq!(plain_left.len(), 1, "one unconditional left-border width");
        assert_eq!(plain_right.len(), 1, "one unconditional right-border width");
        assert_ne!(
            plain_left[0],
            CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::None),
            "a tab with no active neighbour keeps its left border"
        );
        assert_ne!(
            plain_right[0],
            CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::None),
            "a tab with no active neighbour keeps its right border"
        );
    }

    #[test]
    fn tab_styles_redeclare_properties_and_therefore_depend_on_declaration_order() {
        // PINNED HAZARD: each vec is emitted most-specific-block-first, so the
        // generic `.__azul-native-tabs-header p` block *repeats* properties the
        // state-specific block already set, with different values. Which one wins
        // is decided by the resolver, and the two in azul disagree:
        // `PropertyCache::get_property` takes the FIRST match (specific wins,
        // intended), `get_property_with_context` takes the LAST ("last wins",
        // which silently repaints the active tab like an inactive one).
        let heights = decls_where(&CSS_MATCH_14575853790110873394, |p| {
            matches!(p, CssProperty::Height(_))
        });
        assert_eq!(heights.len(), 2, "the active tab declares height twice");
        assert_eq!(
            heights[0],
            CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight::Px(
                PixelValue::const_px(23)
            ))),
            "the active-tab block (23px) must come first"
        );
        assert_eq!(
            heights[1],
            CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight::Px(
                PixelValue::const_px(21)
            ))),
            "the generic p block (21px) shadows it under last-wins"
        );

        let backgrounds = decls_where(&CSS_MATCH_14575853790110873394, |p| {
            matches!(p, CssProperty::BackgroundContent(_))
        });
        assert_eq!(backgrounds.len(), 2);
        let kind = |p: &CssProperty| -> &'static str {
            let CssProperty::BackgroundContent(v) = p else {
                unreachable!()
            };
            match v
                .get_property()
                .expect("an exact background")
                .as_ref()
                .first()
                .expect("one layer")
            {
                StyleBackgroundContent::Color(_) => "flat",
                StyleBackgroundContent::LinearGradient(_) => "gradient",
                _ => "other",
            }
        };
        assert_eq!(kind(&backgrounds[0]), "flat", "active = white fill");
        assert_eq!(
            kind(&backgrounds[1]),
            "gradient",
            "the generic p block re-declares the inactive gradient"
        );

        // Same shape on the seam styles: the None triple is re-declared as 1px.
        let left = plain_decls_where(&CSS_MATCH_13824480602841492081, |p| {
            matches!(p, CssProperty::BorderLeftWidth(_))
        });
        assert_eq!(left.len(), 2, "the -noleftborder style declares it twice");
        assert_eq!(left[0], CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::None));
        assert_eq!(
            left[1],
            CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(
                LayoutBorderLeftWidth {
                    inner: PixelValue::const_px(1)
                }
            ))
        );
    }

    #[test]
    fn hover_declarations_stay_conditional_and_the_rest_stay_unconditional() {
        // A hover rule leaking into the unconditional set would permanently paint
        // every inactive tab in the hover colour.
        for style in [
            CSS_MATCH_11510695043643111367,
            CSS_MATCH_13824480602841492081,
            CSS_MATCH_4415083954137121609,
        ] {
            let conditional = style
                .as_ref()
                .iter()
                .filter(|p| !p.apply_if.as_ref().is_empty())
                .count();
            assert_eq!(
                conditional, 13,
                "each inactive-tab style carries exactly the 13 :hover declarations"
            );
        }

        for style in [
            CSS_MATCH_14575853790110873394,
            CSS_MATCH_9988039989460234263,
            CSS_MATCH_17290739305197504468,
            CSS_MATCH_3088386549906605418,
            CSS_MATCH_18014909903571752977,
            CSS_MATCH_18014909903571752977_NO_PADDING,
            CSS_MATCH_4738503469417034630,
            CSS_MATCH_4738503469417034630_NO_PADDING,
        ] {
            assert!(
                style.as_ref().iter().all(|p| p.apply_if.as_ref().is_empty()),
                "this style must apply unconditionally"
            );
        }
    }

    #[test]
    fn dom_datasets_carry_the_position_of_their_tab() {
        for n in [1usize, 2, 5, 1000] {
            let dom = TabHeader::create(numbered_labels(n)).dom();
            let children = dom.children.as_ref();
            for (i, node) in children[1..=n].iter().enumerate() {
                assert_eq!(
                    tab_idx_of(node),
                    i,
                    "n={n}: tab {i} must carry its own index, not the loop seed"
                );
            }
        }
    }

    #[test]
    fn dom_attaches_a_dataset_even_without_a_click_callback() {
        let dom = TabHeader::create(numbered_labels(2)).dom();
        for node in &dom.children.as_ref()[1..=2] {
            assert!(
                node.root.get_dataset().is_some(),
                "the dataset is unconditional (only the callback is not)"
            );
            assert!(
                node.root.get_callbacks().as_ref().is_empty(),
                "no callback must be attached when on_click is None"
            );
        }
    }

    #[test]
    fn dom_attaches_exactly_one_mouseup_callback_per_tab_when_on_click_is_set() {
        let n = 4usize;
        let dom = TabHeader::create(numbered_labels(n))
            .with_on_click(RefAny::new(ClickLog::default()), cb(record_click))
            .dom();
        let children = dom.children.as_ref();

        for node in &children[1..=n] {
            let cbs = node.root.get_callbacks();
            assert_eq!(cbs.as_ref().len(), 1, "exactly one callback per tab");
            let data = &cbs.as_ref()[0];
            assert_eq!(
                data.event,
                EventFilter::Hover(HoverEventFilter::MouseUp),
                "tabs must react on mouse-up, not mouse-down"
            );
            assert_eq!(
                data.callback.cb,
                on_tab_click as usize,
                "the dispatcher must be the widget's own trampoline"
            );
        }

        // The spacers must stay inert — a click on the filler must not select.
        assert!(children[0].root.get_callbacks().as_ref().is_empty());
        assert!(children[n + 1].root.get_callbacks().as_ref().is_empty());
    }

    #[test]
    fn dom_keeps_the_estimated_child_count_consistent() {
        // `estimated_total_children` is what sizes the flat arena; a stale value
        // makes `convert_dom_into_compact_dom` under-allocate and panic.
        for n in [0usize, 1, 2, 50] {
            let dom = TabHeader::create(numbered_labels(n)).dom();
            assert_eq!(
                dom.estimated_total_children,
                dom.recompute_estimated_total_children(),
                "n={n}: cached descendant count desynced"
            );
            assert_eq!(
                dom.node_count(),
                2 * n + 3,
                "header + spacer + {n} tabs (each a <p> wrapping one text node) + spacer"
            );

            let styled = StyledDom::create_from_dom(dom);
            assert_eq!(
                styled.node_hierarchy.as_ref().len(),
                2 * n + 3,
                "n={n}: the flattened arena must match node_count()"
            );
        }
    }

    #[test]
    fn dom_releases_every_dataset_clone_when_the_dom_is_dropped() {
        // Each tab clones the user payload into its own `TabLocalDataset`; if the
        // widget leaked one, the app state would outlive the DOM forever.
        for n in [0usize, 1, 5] {
            let user = RefAny::new(ClickLog::default());
            let dom = TabHeader::create(numbered_labels(n))
                .with_on_click(user.clone(), cb(record_click))
                .dom();

            assert_eq!(
                user.get_ref_count(),
                n + 1,
                "n={n}: one payload clone per tab, plus the caller's handle"
            );

            drop(dom);
            assert_eq!(
                user.get_ref_count(),
                1,
                "n={n}: dropping the DOM must release every payload clone"
            );
        }
    }

    #[test]
    fn dom_is_deterministic() {
        let build = || {
            let dom = TabHeader::create(numbered_labels(5))
                .with_active_tab(2)
                .dom();
            let children = dom.children.as_ref();
            let shape: Vec<(Option<String>, Vec<&'static str>)> = children
                .iter()
                .map(|c| (text_of(c).map(str::to_string), class_strs(c)))
                .collect();
            shape
        };
        assert_eq!(build(), build(), "dom() must be a pure function of its state");
    }

    // ==================================================================
    // TabContent
    // ==================================================================

    fn nested(depth: usize) -> Dom {
        let mut dom = Dom::create_text_do_not_use_without_block_level_wrapper("leaf");
        for _ in 0..depth {
            dom = Dom::create_div().with_child(dom);
        }
        dom
    }

    #[test]
    fn content_new_defaults_to_padding_and_keeps_the_content_verbatim() {
        let content = nested(3);
        let tab_content = TabContent::new(content.clone());

        assert!(tab_content.has_padding, "new() defaults to padded");
        assert_eq!(
            tab_content.content, content,
            "new() must not rewrap or normalise the content"
        );
        assert!(
            TabContent::default().has_padding,
            "Default agrees with new()"
        );
    }

    #[test]
    fn with_padding_and_set_padding_agree_and_are_last_wins() {
        for flag in [true, false] {
            let built = TabContent::new(Dom::create_div()).with_padding(flag);
            let mut mutated = TabContent::new(Dom::create_div());
            mutated.set_padding(flag);
            assert_eq!(built.has_padding, flag);
            assert_eq!(built.has_padding, mutated.has_padding);
        }

        let toggled = TabContent::new(Dom::create_div())
            .with_padding(false)
            .with_padding(true)
            .with_padding(false);
        assert!(!toggled.has_padding, "the last write must win");
    }

    #[test]
    fn content_dom_nests_the_content_under_the_content_class() {
        let content = nested(2);
        let dom = TabContent::new(content.clone()).dom();

        assert!(
            class_strs(&dom).is_empty(),
            "the outer wrapper carries the style, not the class"
        );
        assert_eq!(dom.children.as_ref().len(), 1, "one wrapper child");

        let inner = &dom.children.as_ref()[0];
        assert_eq!(class_strs(inner), vec![CLASS_CONTENT]);
        assert!(
            inline_declared(inner).is_empty(),
            "the classed node carries no inline style of its own"
        );
        assert_eq!(inner.children.as_ref().len(), 1);
        assert_eq!(
            inner.children.as_ref()[0],
            content,
            "the user content must survive the two wrappers untouched"
        );
    }

    #[test]
    fn content_dom_picks_the_style_vec_the_padding_flag_asks_for() {
        let padded = TabContent::new(Dom::create_div()).with_padding(true).dom();
        assert_eq!(
            inline_declared(&padded),
            declared(&CSS_MATCH_18014909903571752977)
        );

        let bare = TabContent::new(Dom::create_div()).with_padding(false).dom();
        assert_eq!(
            inline_declared(&bare),
            declared(&CSS_MATCH_18014909903571752977_NO_PADDING)
        );

        assert_ne!(
            inline_declared(&padded),
            inline_declared(&bare),
            "the two padding modes must not collapse to the same style"
        );
    }

    #[test]
    fn the_unpadded_content_style_declares_no_padding_at_all() {
        let padding_decls = decls_where(&CSS_MATCH_18014909903571752977_NO_PADDING, |p| {
            matches!(
                p,
                CssProperty::PaddingTop(_)
                    | CssProperty::PaddingBottom(_)
                    | CssProperty::PaddingLeft(_)
                    | CssProperty::PaddingRight(_)
            )
        });
        assert!(
            padding_decls.is_empty(),
            "has_padding == false must not leave a padding declaration behind"
        );

        let padded = decls_where(&CSS_MATCH_18014909903571752977, |p| {
            matches!(
                p,
                CssProperty::PaddingTop(_)
                    | CssProperty::PaddingBottom(_)
                    | CssProperty::PaddingLeft(_)
                    | CssProperty::PaddingRight(_)
            )
        });
        assert_eq!(padded.len(), 4, "the padded variant sets all four sides");
    }

    #[test]
    fn content_dom_keeps_the_estimated_child_count_consistent() {
        for depth in [0usize, 1, 3, 64] {
            let content = nested(depth);
            let expected = content.node_count() + 2; // outer wrapper + classed wrapper
            let dom = TabContent::new(content).dom();

            assert_eq!(
                dom.estimated_total_children,
                dom.recompute_estimated_total_children(),
                "depth={depth}: cached descendant count desynced"
            );
            assert_eq!(dom.node_count(), expected, "depth={depth}");

            let styled = StyledDom::create_from_dom(dom);
            assert_eq!(
                styled.node_hierarchy.as_ref().len(),
                expected,
                "depth={depth}: the flattened arena must match node_count()"
            );
        }
    }

    #[test]
    fn content_swap_with_default_returns_the_old_state_and_leaves_an_empty_div() {
        let content = nested(2);
        let mut tab_content = TabContent::new(content.clone()).with_padding(false);

        let taken = tab_content.swap_with_default();
        assert_eq!(taken.content, content);
        assert!(!taken.has_padding, "the flag moves out with the content");

        assert_eq!(
            tab_content.content,
            Dom::create_div(),
            "the husk must be an empty div"
        );
        assert!(
            tab_content.has_padding,
            "the husk is a Default, i.e. padded again"
        );
    }

    // ==================================================================
    // on_tab_click
    // ==================================================================

    #[test]
    fn on_tab_click_reports_the_clicked_index_to_the_user_callback() {
        for idx in [0usize, 1, 7, usize::MAX / 2, usize::MAX - 1, usize::MAX] {
            let mut user = RefAny::new(ClickLog::default());
            let (update, changes) =
                run_click(dataset(idx, click_handler(user.clone(), record_click)));

            assert_eq!(update, Update::RefreshDom);
            assert!(
                changes.is_empty(),
                "the tab handler is stateless — it must not push CallbackChanges"
            );
            assert_eq!(
                logged(&mut user),
                vec![TabHeaderState { active_tab: idx }],
                "the index must be forwarded verbatim, without arithmetic"
            );
        }
    }

    #[test]
    fn on_tab_click_propagates_the_user_update_verbatim() {
        for (callback, expected) in [
            (record_click as TabOnClickCallbackType, Update::RefreshDom),
            (click_do_nothing, Update::DoNothing),
            (click_refresh_all, Update::RefreshDomAllWindows),
        ] {
            let (update, _) = run_click(dataset(
                3,
                click_handler(RefAny::new(ClickLog::default()), callback),
            ));
            assert_eq!(update, expected, "the user verdict must not be overridden");
        }
    }

    #[test]
    fn on_tab_click_without_a_user_callback_does_nothing() {
        // A dataset with no `on_click` is reachable only by hand (dom() attaches
        // the trampoline only when a callback exists) — it must stay silent
        // rather than force a relayout.
        let (update, changes) = run_click(dataset(2, None.into()));
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn on_tab_click_on_a_foreign_payload_falls_back_to_refresh() {
        // Wrong-typed payload => `downcast_mut` returns None => the documented
        // `unwrap_or(RefreshDom)` fallback. Note this is the *opposite* verdict
        // from the no-callback case above, which is the asymmetry to watch.
        for foreign in [RefAny::new(42u32), RefAny::new(String::from("nope"))] {
            let (update, changes) = run_click(foreign);
            assert_eq!(update, Update::RefreshDom);
            assert!(changes.is_empty());
        }
    }

    #[test]
    fn on_tab_click_declines_while_the_dataset_is_already_borrowed() {
        // Borrow tracking is shared across clones, so a live `Ref` from anywhere
        // must make the handler bail out safely instead of aliasing or panicking.
        let mut held = dataset(1, None.into());
        let clone = held.clone();
        let guard = held
            .downcast_ref::<TabLocalDataset>()
            .expect("the fixture is a TabLocalDataset");

        let (update, changes) = run_click(clone);
        assert_eq!(
            update,
            Update::RefreshDom,
            "a contended dataset must fall back, not panic"
        );
        assert!(changes.is_empty());

        drop(guard);

        // ...and the borrow must be released again afterwards.
        let (update, _) = run_click(held.clone());
        assert_eq!(update, Update::DoNothing);
    }

    #[test]
    fn on_tab_click_releases_its_borrow_so_repeated_clicks_keep_working() {
        let mut user = RefAny::new(ClickLog::default());
        let data = dataset(4, click_handler(user.clone(), record_click));

        let (first, _) = run_click(data.clone());
        let (second, _) = run_click(data.clone());
        let (third, _) = run_click(data.clone());

        assert_eq!(
            [first, second, third],
            [Update::RefreshDom; 3],
            "a leaked RefMut would turn later clicks into the RefreshDom fallback \
             without ever reaching the user callback"
        );
        assert_eq!(
            logged(&mut user),
            vec![TabHeaderState { active_tab: 4 }; 3],
            "every click must reach the user callback"
        );
    }

    #[test]
    fn clicking_a_rendered_tab_selects_that_tab_end_to_end() {
        let n = 5usize;
        let mut user = RefAny::new(ClickLog::default());
        let dom = TabHeader::create(numbered_labels(n))
            .with_on_click(user.clone(), cb(record_click))
            .dom();

        for i in 0..n {
            let data = dom.children.as_ref()[i + 1]
                .root
                .get_callbacks()
                .as_ref()
                .first()
                .expect("every tab carries the click callback")
                .refany
                .clone();
            let (update, changes) = run_click(data);
            assert_eq!(update, Update::RefreshDom);
            assert!(changes.is_empty());
        }

        assert_eq!(
            logged(&mut user),
            (0..n)
                .map(|active_tab| TabHeaderState { active_tab })
                .collect::<Vec<_>>(),
            "clicking tab i must report exactly i"
        );
    }

    #[test]
    fn clicking_a_tab_does_not_move_the_active_tab_by_itself() {
        // The widget is stateless: the handler reports the click and nothing
        // else. Re-rendering the *same* header must therefore keep tab 0 active.
        let user = RefAny::new(ClickLog::default());
        let header = TabHeader::create(numbered_labels(3))
            .with_on_click(user.clone(), cb(record_click));
        let dom = header.clone().dom();

        let data = dom.children.as_ref()[3]
            .root
            .get_callbacks()
            .as_ref()
            .first()
            .expect("tab 2 carries the click callback")
            .refany
            .clone();
        let (update, _) = run_click(data);
        assert_eq!(update, Update::RefreshDom);

        assert_eq!(
            header.active_tab, 0,
            "the header itself must be untouched by a click"
        );
        assert_eq!(
            class_strs(&header.dom().children.as_ref()[1]),
            vec![CLASS_ACTIVE],
            "re-rendering the unchanged header keeps tab 0 active"
        );
    }
}
