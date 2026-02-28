use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, FocusEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec,
        TabIndex,
    },
    menu::{Menu, MenuItem, MenuItemVec, MenuPopupPosition, StringMenuItem},
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
const STYLE_BACKGROUND_CONTENT_4857374953508308215_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_8909964754681718371_ITEMS,
        ),
    })];
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
const STYLE_BACKGROUND_CONTENT_16125239329823337131_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_8010235203234495977_ITEMS,
        ),
    })];
const STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    })];
const STYLE_TRANSFORM_9499236770162623295_ITEMS: &[StyleTransform] = &[
    StyleTransform::Rotate(AngleValue::const_deg(315)),
    StyleTransform::Translate(StyleTransformTranslate2D {
        x: PixelValue::const_px(0),
        y: PixelValue::const_px(-2),
    }),
];
const STYLE_FONT_FAMILY_18001933966972968559_ITEMS: &[StyleFontFamily] =
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
const LINEAR_COLOR_STOP_8010235203234495977_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(ColorU {
            r: 218,
            g: 236,
            b: 252,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 196,
            g: 224,
            b: 252,
            a: 255,
        }),
    },
];
const LINEAR_COLOR_STOP_8909964754681718371_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(ColorU {
            r: 235,
            g: 244,
            b: 252,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 220,
            g: 236,
            b: 252,
            a: 255,
        }),
    },
];

const CSS_MATCH_10188117026223137249_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-dropdown-wrapper:focus
    CssPropertyWithConditions::on_focus(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16125239329823337131_ITEMS,
        )),
    )),
    // .__azul-native-dropdown-wrapper:active
    CssPropertyWithConditions::on_active(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16125239329823337131_ITEMS,
        )),
    )),
    // .__azul-native-dropdown-wrapper:hover
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
            STYLE_BACKGROUND_CONTENT_4857374953508308215_ITEMS,
        )),
    )),
    // .__azul-native-dropdown-wrapper
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(2),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(2),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(2),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(2),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::MinWidth(LayoutMinWidthValue::Exact(
        LayoutMinWidth {
            inner: PixelValue::const_px(120),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_18001933966972968559_ITEMS),
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(0),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
    CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
        LayoutDisplay::Block,
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
];
const CSS_MATCH_10188117026223137249: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_10188117026223137249_PROPERTIES);

const CSS_MATCH_16432538576103237591_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-dropdown-wrapper p
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(8),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
        LayoutDisplay::Block,
    ))),
];
const CSS_MATCH_16432538576103237591: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_16432538576103237591_PROPERTIES);

const CSS_MATCH_2883986488332352590_PROPERTIES: &[CssPropertyWithConditions] = &[
    // body
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS,
        )),
    )),
];
const CSS_MATCH_2883986488332352590: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_2883986488332352590_PROPERTIES);

const CSS_MATCH_4428877324022630014_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-dropdown
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(0),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
];
const CSS_MATCH_4428877324022630014: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_4428877324022630014_PROPERTIES);

const CSS_MATCH_4687758758634879229_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-dropdown-arrow-wrapper
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
            inner: PixelValue::const_px(0),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(0),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::JustifyContent(
        LayoutJustifyContentValue::Exact(LayoutJustifyContent::End),
    )),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
];
const CSS_MATCH_4687758758634879229: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_4687758758634879229_PROPERTIES);

const CSS_MATCH_5369484915686807864_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-dropdown-arrow-content
    CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
        LayoutWidth::Px(PixelValue::const_px(6)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::Transform(StyleTransformVecValue::Exact(
        StyleTransformVec::from_const_slice(STYLE_TRANSFORM_9499236770162623295_ITEMS),
    ))),
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(6)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(2),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 96,
                g: 96,
                b: 96,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(2),
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
                r: 96,
                g: 96,
                b: 96,
                a: 255,
            },
        }),
    )),
];
const CSS_MATCH_5369484915686807864: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_5369484915686807864_PROPERTIES);

const CSS_MATCH_6763840958685503000_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-dropdown-arrow
    CssPropertyWithConditions::simple(CssProperty::MinWidth(LayoutMinWidthValue::Exact(
        LayoutMinWidth {
            inner: PixelValue::const_px(20),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::JustifyContent(
        LayoutJustifyContentValue::Exact(LayoutJustifyContent::Center),
    )),
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(0),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
];
const CSS_MATCH_6763840958685503000: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_6763840958685503000_PROPERTIES);

const CSS_MATCH_7938442083662451131_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-dropdown-focused-text:hover
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
            inner: BorderStyle::Dotted,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Dotted,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Dotted,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Dotted,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        }),
    )),
    // .__azul-native-dropdown-focused-text
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(15),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomRightRadius(
        StyleBorderBottomRightRadiusValue::Exact(StyleBorderBottomRightRadius {
            inner: PixelValue::const_px(2),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomLeftRadius(
        StyleBorderBottomLeftRadiusValue::Exact(StyleBorderBottomLeftRadius {
            inner: PixelValue::const_px(2),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopRightRadius(
        StyleBorderTopRightRadiusValue::Exact(StyleBorderTopRightRadius {
            inner: PixelValue::const_px(2),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopLeftRadius(
        StyleBorderTopLeftRadiusValue::Exact(StyleBorderTopLeftRadius {
            inner: PixelValue::const_px(2),
        }),
    )),
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
                r: 255,
                g: 255,
                b: 255,
                a: 0,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 0,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 0,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 0,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::Center,
    ))),
];
const CSS_MATCH_7938442083662451131: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_7938442083662451131_PROPERTIES);

pub type DropDownOnChoiceChangeCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    DropDownOnChoiceChange,
    OptionDropDownOnChoiceChange,
    DropDownOnChoiceChangeCallback,
    DropDownOnChoiceChangeCallbackType
);

#[repr(C)]
pub struct DropDown {
    pub choices: StringVec,
    pub selected: usize,
    pub on_choice_change: OptionDropDownOnChoiceChange,
}

impl Default for DropDown {
    fn default() -> Self {
        Self {
            choices: StringVec::from_const_slice(&[]),
            selected: 0,
            on_choice_change: None.into(),
        }
    }
}

impl DropDown {
    pub fn new(choices: StringVec) -> Self {
        Self {
            choices,
            selected: 0,
            on_choice_change: None.into(),
        }
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut m = DropDown::default();
        core::mem::swap(&mut m, self);
        m
    }

    pub fn dom(self) -> Dom {
        let refany = RefAny::new(self);

        Dom::create_div()
            .with_css_props(CSS_MATCH_4428877324022630014)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_9466018534284317754: &[IdOrClass] =
                    &[Class(AzString::from_const_str("__azul-native-dropdown"))];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_9466018534284317754)
            })
            .with_children(DomVec::from_vec(vec![Dom::create_div()
                .with_css_props(CSS_MATCH_10188117026223137249)
                .with_ids_and_classes({
                    const IDS_AND_CLASSES_6395608618544226348: &[IdOrClass] = &[Class(
                        AzString::from_const_str("__azul-native-dropdown-wrapper"),
                    )];
                    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6395608618544226348)
                })
                .with_tab_index(TabIndex::Auto)
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Focus(FocusEventFilter::FocusReceived),
                        refany: refany.clone(),
                        callback: CoreCallback {
                            cb: on_dropdown_click as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                    }]
                    .into(),
                )
                .with_children(DomVec::from_vec(vec![
                    Dom::create_div()
                        .with_css_props(CSS_MATCH_7938442083662451131)
                        .with_ids_and_classes({
                            const IDS_AND_CLASSES_11862789041977911489: &[IdOrClass] = &[Class(
                                AzString::from_const_str("__azul-native-dropdown-focused-text"),
                            )];
                            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_11862789041977911489)
                        })
                        .with_children(DomVec::from_vec(vec![Dom::create_text(
                            AzString::from_const_str("Checkbox"),
                        )
                        .with_css_props(CSS_MATCH_16432538576103237591)])),
                    Dom::create_div()
                        .with_css_props(CSS_MATCH_6763840958685503000)
                        .with_ids_and_classes({
                            const IDS_AND_CLASSES_17649077225810153180: &[IdOrClass] = &[Class(
                                AzString::from_const_str("__azul-native-dropdown-arrow"),
                            )];
                            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_17649077225810153180)
                        })
                        .with_children(DomVec::from_vec(vec![Dom::create_div()
                            .with_css_props(CSS_MATCH_4687758758634879229)
                            .with_ids_and_classes({
                                const IDS_AND_CLASSES_17777388057004109464: &[IdOrClass] =
                                    &[Class(AzString::from_const_str(
                                        "__azul-native-dropdown-arrow-wrapper",
                                    ))];
                                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_17777388057004109464)
                            })
                            .with_children(DomVec::from_vec(vec![Dom::create_div()
                                .with_css_props(CSS_MATCH_5369484915686807864)
                                .with_ids_and_classes({
                                    const IDS_AND_CLASSES_12603885741328163120: &[IdOrClass] =
                                        &[Class(AzString::from_const_str(
                                            "__azul-native-dropdown-arrow-content",
                                        ))];
                                    IdOrClassVec::from_const_slice(
                                        IDS_AND_CLASSES_12603885741328163120,
                                    )
                                })]))])),
                ]))]))
    }
}

// dataset holding the choices
struct DropDownLocalDataset {
    choices: StringVec,
    selected: usize,
    on_choice_change: OptionDropDownOnChoiceChange,
}

// dataset for individual menu item callback
struct ChoiceCallbackData {
    choice_id: usize,
    on_choice_change: OptionDropDownOnChoiceChange,
}

extern "C" fn on_dropdown_click(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let refany = match refany.downcast_ref::<DropDown>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // Build menu items from choices
    let menu_items: Vec<MenuItem> = refany
        .choices
        .iter()
        .enumerate()
        .map(|(idx, choice)| {
            MenuItem::String(StringMenuItem::create(choice.clone()).with_callback(
                RefAny::new(ChoiceCallbackData {
                    choice_id: idx,
                    on_choice_change: refany.on_choice_change.clone(),
                }),
                on_choice_selected as usize,
            ))
        })
        .collect();

    let menu = Menu {
        items: menu_items.into(),
        position: MenuPopupPosition::BottomOfHitRect,
        ..Default::default()
    };

    // Open native menu positioned below the dropdown
    info.open_menu_for_hit_node(menu);

    Update::DoNothing
}

extern "C" fn on_choice_selected(mut refany: RefAny, info: CallbackInfo) -> Update {
    let mut refany = match refany.downcast_mut::<ChoiceCallbackData>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let choice_id = refany.choice_id;

    match refany.on_choice_change.as_mut() {
        Some(DropDownOnChoiceChange { refany, callback }) => {
            (callback.cb)(refany.clone(), info.clone(), choice_id)
        }
        None => Update::DoNothing,
    }
}

impl From<DropDown> for Dom {
    fn from(b: DropDown) -> Dom {
        b.dom()
    }
}
