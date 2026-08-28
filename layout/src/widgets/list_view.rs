//! Native list view widget with column headers, row selection, and sorting indicators.

use alloc::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec,
        TabIndex,
    },
    geom::{LogicalPosition, LogicalSize},
    menu::{Menu, OptionMenu},
    refany::{OptionRefAny, RefAny},
};
use azul_css::css::BoxOrStatic;
#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use azul_css::{
    corety::OptionUsize,
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::callbacks::{Callback, CallbackInfo};

const STRING_16146701490593874959: AzString = AzString::from_const_str("system:ui");
const STYLE_BACKGROUND_CONTENT_661302523448178568_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ColorU {
        r: 209,
        g: 232,
        b: 255,
        a: 255,
    })];
const STYLE_BACKGROUND_CONTENT_2444935983575427872_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ColorU {
        r: 252,
        g: 252,
        b: 252,
        a: 255,
    })];
const STYLE_BACKGROUND_CONTENT_3010057533077499049_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ColorU {
        r: 229,
        g: 243,
        b: 251,
        a: 255,
    })];
const STYLE_BACKGROUND_CONTENT_3839348353894170136_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ColorU {
        r: 249,
        g: 250,
        b: 251,
        a: 255,
    })];
const STYLE_BACKGROUND_CONTENT_6112684430356720596_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_10827796861537038040_ITEMS,
        ),
    })];
const STYLE_BACKGROUND_CONTENT_7422581697888665934_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_513857305091467054_ITEMS,
        ),
    })];
const STYLE_BACKGROUND_CONTENT_11062356617965867290_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ColorU {
        r: 240,
        g: 240,
        b: 240,
        a: 255,
    })];
const STYLE_BACKGROUND_CONTENT_11098930083828139815_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ColorU {
        r: 184,
        g: 224,
        b: 243,
        a: 255,
    })];
const STYLE_TRANSFORM_6162542744002865382_ITEMS: &[StyleTransform] =
    &[StyleTransform::Translate(StyleTransformTranslate2D {
        x: PixelValue::const_px(7),
        y: PixelValue::const_px(0),
    })];
const STYLE_TRANSFORM_16978981723642914576_ITEMS: &[StyleTransform] =
    &[StyleTransform::Rotate(AngleValue::const_deg(45))];
const STYLE_TRANSFORM_17732691695785266054_ITEMS: &[StyleTransform] = &[
    StyleTransform::Rotate(AngleValue::const_deg(315)),
    StyleTransform::Translate(StyleTransformTranslate2D {
        x: PixelValue::const_px(0),
        y: PixelValue::const_px(2),
    }),
];
const STYLE_FONT_FAMILY_8122988506401935406_ITEMS: &[StyleFontFamily] =
    &[StyleFontFamily::System(STRING_16146701490593874959)];
const LINEAR_COLOR_STOP_513857305091467054_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(50),
        color: ColorOrSystem::color(ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(51),
        color: ColorOrSystem::color(ColorU {
            r: 247,
            g: 248,
            b: 250,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 243,
            g: 244,
            b: 246,
            a: 255,
        }),
    },
];
const LINEAR_COLOR_STOP_10827796861537038040_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(ColorU {
            r: 247,
            g: 252,
            b: 254,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(50),
        color: ColorOrSystem::color(ColorU {
            r: 247,
            g: 252,
            b: 254,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(51),
        color: ColorOrSystem::color(ColorU {
            r: 232,
            g: 246,
            b: 254,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 206,
            g: 231,
            b: 244,
            a: 255,
        }),
    },
];

const CSS_MATCH_1085706216385961159_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul_native-list-header-arrow-down
    CssPropertyWithConditions::simple(CssProperty::Transform(StyleTransformVecValue::Exact(
        StyleTransformVec::from_const_slice(STYLE_TRANSFORM_6162542744002865382_ITEMS),
    ))),
    CssPropertyWithConditions::simple(CssProperty::Position(LayoutPositionValue::Exact(
        LayoutPosition::Absolute,
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(3),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(3),
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
    CssPropertyWithConditions::simple(CssProperty::JustifyContent(
        LayoutJustifyContentValue::Exact(LayoutJustifyContent::Center),
    )),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
];
const CSS_MATCH_1085706216385961159: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_1085706216385961159_PROPERTIES);

const CSS_MATCH_12498280255863106397_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul_native-list-header-item:hover
    CssPropertyWithConditions::on_hover(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 154,
                g: 223,
                b: 254,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_6112684430356720596_ITEMS,
        )),
    )),
    // .__azul_native-list-header-item:active
    CssPropertyWithConditions::on_active(CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            color: ColorU {
                r: 206,
                g: 231,
                b: 244,
                a: 255,
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(5),
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            clip_mode: BoxShadowClipMode::Inset,
        }),
    ))),
    CssPropertyWithConditions::on_active(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            color: ColorU {
                r: 206,
                g: 231,
                b: 244,
                a: 255,
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(5),
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            clip_mode: BoxShadowClipMode::Inset,
        }),
    ))),
    CssPropertyWithConditions::on_active(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            color: ColorU {
                r: 206,
                g: 231,
                b: 244,
                a: 255,
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(5),
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            clip_mode: BoxShadowClipMode::Inset,
        }),
    ))),
    CssPropertyWithConditions::on_active(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            color: ColorU {
                r: 206,
                g: 231,
                b: 244,
                a: 255,
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(5),
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            clip_mode: BoxShadowClipMode::Inset,
        }),
    ))),
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
                r: 194,
                g: 205,
                b: 219,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 194,
                g: 205,
                b: 219,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 194,
                g: 205,
                b: 219,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 194,
                g: 205,
                b: 219,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_active(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_3839348353894170136_ITEMS,
        )),
    )),
    // .__azul_native-list-header-item
    CssPropertyWithConditions::simple(CssProperty::Position(LayoutPositionValue::Exact(
        LayoutPosition::Relative,
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(7),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::MinWidth(LayoutMinWidthValue::Exact(
        LayoutMinWidth {
            inner: PixelValue::const_px(100),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 243,
                g: 244,
                b: 246,
                a: 255,
            },
        }),
    )),
];
const CSS_MATCH_12498280255863106397: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_12498280255863106397_PROPERTIES);

const CSS_MATCH_12980082330151137475_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul_native-list-rows-row-cell
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(7),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::MinWidth(LayoutMinWidthValue::Exact(
        LayoutMinWidth {
            inner: PixelValue::const_px(100),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
    ))),
];
const CSS_MATCH_12980082330151137475: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_12980082330151137475_PROPERTIES);

const CSS_MATCH_13758717721055992976_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul_native-list-header-arrow-down-inner
    CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
        LayoutWidth::Px(PixelValue::const_px(6)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::Transform(StyleTransformVecValue::Exact(
        StyleTransformVec::from_const_slice(STYLE_TRANSFORM_16978981723642914576_ITEMS),
    ))),
    CssPropertyWithConditions::simple(CssProperty::OverflowY(LayoutOverflowValue::Exact(
        LayoutOverflow::Hidden,
    ))),
    CssPropertyWithConditions::simple(CssProperty::OverflowX(LayoutOverflowValue::Exact(
        LayoutOverflow::Hidden,
    ))),
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(6)),
    ))),
];
const CSS_MATCH_13758717721055992976: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_13758717721055992976_PROPERTIES);

const CSS_MATCH_15295293133676720691_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul_native-list-header-dragwidth-drag
    CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
        LayoutWidth::Px(PixelValue::const_px(2)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::Position(LayoutPositionValue::Exact(
        LayoutPosition::Absolute,
    ))),
];
const CSS_MATCH_15295293133676720691: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_15295293133676720691_PROPERTIES);

const CSS_MATCH_15315949193378715186_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul_native-list-header
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(25)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_7422581697888665934_ITEMS,
        )),
    )),
];
const CSS_MATCH_15315949193378715186: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_15315949193378715186_PROPERTIES);

const CSS_MATCH_15673486787900743642_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul_native-list-header .__azul_native-list-header-item p
    CssPropertyWithConditions::simple(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
    CssPropertyWithConditions::simple(CssProperty::TextColor(StyleTextColorValue::Exact(
        StyleTextColor {
            inner: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::Center,
    ))),
];
const CSS_MATCH_15673486787900743642: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_15673486787900743642_PROPERTIES);

const CSS_MATCH_1574792189506859253_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul_native-list-header-arrow-down-inner-deco
    CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
        LayoutWidth::Px(PixelValue::const_px(12)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::Transform(StyleTransformVecValue::Exact(
        StyleTransformVec::from_const_slice(STYLE_TRANSFORM_17732691695785266054_ITEMS),
    ))),
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(12)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::const_px(3),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::const_px(3),
            },
            color: ColorU {
                r: 60,
                g: 94,
                b: 114,
                a: 255,
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(10),
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            clip_mode: BoxShadowClipMode::Inset,
        }),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::const_px(3),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::const_px(3),
            },
            color: ColorU {
                r: 60,
                g: 94,
                b: 114,
                a: 255,
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(10),
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            clip_mode: BoxShadowClipMode::Inset,
        }),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::const_px(3),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::const_px(3),
            },
            color: ColorU {
                r: 60,
                g: 94,
                b: 114,
                a: 255,
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(10),
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            clip_mode: BoxShadowClipMode::Inset,
        }),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::const_px(3),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::const_px(3),
            },
            color: ColorU {
                r: 60,
                g: 94,
                b: 114,
                a: 255,
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(10),
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            clip_mode: BoxShadowClipMode::Inset,
        }),
    ))),
];
const CSS_MATCH_1574792189506859253: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_1574792189506859253_PROPERTIES);

const CSS_MATCH_17553577885456905601_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul_native_list-container
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_2444935983575427872_ITEMS,
        )),
    )),
];
const CSS_MATCH_17553577885456905601: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_17553577885456905601_PROPERTIES);

const CSS_MATCH_2883986488332352590_PROPERTIES: &[CssPropertyWithConditions] = &[
    // body
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
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_11062356617965867290_ITEMS,
        )),
    )),
];
const CSS_MATCH_2883986488332352590: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_2883986488332352590_PROPERTIES);

const CSS_MATCH_4852927511892172364_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul_native-list-rows
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
];
const CSS_MATCH_4852927511892172364: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_4852927511892172364_PROPERTIES);

const CSS_MATCH_6002662151290653203_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul_native-list-header-dragwidth
    CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
        LayoutWidth::Px(PixelValue::const_px(0)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::Position(LayoutPositionValue::Exact(
        LayoutPosition::Relative,
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
];
const CSS_MATCH_6002662151290653203: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_6002662151290653203_PROPERTIES);

const CSS_MATCH_6827198030119836081_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul_native-list-rows-row.selected
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
                r: 102,
                g: 167,
                b: 232,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 102,
                g: 167,
                b: 232,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 102,
                g: 167,
                b: 232,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 102,
                g: 167,
                b: 232,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_661302523448178568_ITEMS,
        )),
    )),
    // .__azul_native-list-rows-row:hover
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
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_3010057533077499049_ITEMS,
        )),
    )),
    // .__azul_native-list-rows-row
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(0),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(0),
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
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
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
];
const CSS_MATCH_6827198030119836081: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_6827198030119836081_PROPERTIES);

const CSS_MATCH_7894335449545988724_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul_native-list-rows-row.focused
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
                r: 38,
                g: 160,
                b: 218,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 38,
                g: 160,
                b: 218,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 38,
                g: 160,
                b: 218,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 38,
                g: 160,
                b: 218,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_focus(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_11098930083828139815_ITEMS,
        )),
    )),
    // .__azul_native-list-rows-row:hover
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
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::on_hover(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_3010057533077499049_ITEMS,
        )),
    )),
    // .__azul_native-list-rows-row
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(0),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(0),
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
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
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
];
const CSS_MATCH_7894335449545988724: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_7894335449545988724_PROPERTIES);

const IDS_AND_CLASSES_790316832563530605: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul_native-list-rows-row",
))];
const ROW_CLASS: IdOrClassVec = IdOrClassVec::from_const_slice(IDS_AND_CLASSES_790316832563530605);

const IDS_AND_CLASSES_3034181810805097699: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul_native-list-rows-row-cell",
))];
const CELL_CLASS: IdOrClassVec =
    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_3034181810805097699);

const IDS_AND_CLASSES_6012478019077291002: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul_native-list-rows"))];
const ROW_CONTAINER_CLASS: IdOrClassVec =
    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6012478019077291002);

const IDS_AND_CLASSES_10742579426112804392: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul_native-list-header"))];
const HEADER_CONTAINER_CLASS: IdOrClassVec =
    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_10742579426112804392);

const IDS_AND_CLASSES_9205819539370539587: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul_native_list-container",
))];
const LIST_VIEW_CONTAINER_CLASS: IdOrClassVec =
    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_9205819539370539587);

const IDS_AND_CLASSES_18330792117162403422: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul_native-list-header-item",
))];
const COLUMN_NAME_CLASS: IdOrClassVec =
    IdOrClassVec::from_const_slice(IDS_AND_CLASSES_18330792117162403422);

pub type ListViewOnLazyLoadScrollCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ListViewState) -> Update;
impl_widget_callback!(
    ListViewOnLazyLoadScroll,
    OptionListViewOnLazyLoadScroll,
    ListViewOnLazyLoadScrollCallback,
    ListViewOnLazyLoadScrollCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ListViewOnLazyLoadScrollCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: LIST_VIEW_ON_LAZY_LOAD_SCROLL_INVOKER,
    invoker_ty:     AzListViewOnLazyLoadScrollCallbackInvoker,
    thunk_fn:       az_list_view_on_lazy_load_scroll_callback_thunk,
    setter_fn:      AzApp_setListViewOnLazyLoadScrollCallbackInvoker,
    from_handle_fn: AzListViewOnLazyLoadScrollCallback_createFromHostHandle,
    extra_args:     [ state: ListViewState ],
}

pub type ListViewOnColumnClickCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ListViewState, column_clicked: usize) -> Update;
impl_widget_callback!(
    ListViewOnColumnClick,
    OptionListViewOnColumnClick,
    ListViewOnColumnClickCallback,
    ListViewOnColumnClickCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ListViewOnColumnClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: LIST_VIEW_ON_COLUMN_CLICK_INVOKER,
    invoker_ty:     AzListViewOnColumnClickCallbackInvoker,
    thunk_fn:       az_list_view_on_column_click_callback_thunk,
    setter_fn:      AzApp_setListViewOnColumnClickCallbackInvoker,
    from_handle_fn: AzListViewOnColumnClickCallback_createFromHostHandle,
    extra_args:     [ state: ListViewState, column_clicked: usize ],
}

pub type ListViewOnRowClickCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ListViewState, row_clicked: usize) -> Update;
impl_widget_callback!(
    ListViewOnRowClick,
    OptionListViewOnRowClick,
    ListViewOnRowClickCallback,
    ListViewOnRowClickCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ListViewOnRowClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: LIST_VIEW_ON_ROW_CLICK_INVOKER,
    invoker_ty:     AzListViewOnRowClickCallbackInvoker,
    thunk_fn:       az_list_view_on_row_click_callback_thunk,
    setter_fn:      AzApp_setListViewOnRowClickCallbackInvoker,
    from_handle_fn: AzListViewOnRowClickCallback_createFromHostHandle,
    extra_args:     [ state: ListViewState, row_clicked: usize ],
}

/// State of the `ListView`, but without row data
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ListViewState {
    /// Copy of the current column names
    pub columns: StringVec,
    /// Which column the rows are currently sorted by
    pub sorted_by: OptionUsize,
    /// Row count of rows currently loaded in the DOM
    pub current_row_count: usize,
    /// Y-offset currently applied to the rows
    pub scroll_offset: PixelValueNoPercent,
    /// Current position where the user has scrolled the `ListView` to
    pub current_scroll_position: LogicalPosition,
    /// Current height of the row container
    pub current_content_height: LogicalSize,
}

/// List view, optionally able to lazy-load data
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ListView {
    /// Column names
    pub columns: StringVec,
    /// Currently rendered rows. Note that the `ListView` does not
    /// have to render all rows at once, usually you'd only render
    /// the top 100 rows
    pub rows: ListViewRowVec,
    /// Which column is the list view sorted by (default = None)?
    pub sorted_by: OptionUsize,
    /// Offset to add to the rows used when layouting row positions
    /// during lazy-loaded scrolling. Also affects the scroll position
    pub scroll_offset: PixelValueNoPercent,
    /// Height of the content, if not all rows are loaded
    pub content_height: OptionPixelValueNoPercent,
    /// Context menu for the columns (usually opens a context menu
    /// to select which columns to show)
    pub column_context_menu: OptionMenu,
    /// Indicates that this `ListView` is being lazily loaded, allows
    /// control over what happens when the user scrolls the `ListView`.
    pub on_lazy_load_scroll: OptionListViewOnLazyLoadScroll,
    /// What to do when the user left-clicks the column
    /// (usually used for storing which column to sort by)
    pub on_column_click: OptionListViewOnColumnClick,
    /// What to do when the user left-clicks a row
    /// (usually used for selecting the row depending on the state)
    pub on_row_click: OptionListViewOnRowClick,
}

impl Default for ListView {
    fn default() -> Self {
        Self {
            columns: StringVec::from_const_slice(&[]),
            rows: ListViewRowVec::from_const_slice(&[]),
            sorted_by: None.into(),
            scroll_offset: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            content_height: None.into(),
            column_context_menu: None.into(),
            on_lazy_load_scroll: None.into(),
            on_column_click: None.into(),
            on_row_click: None.into(),
        }
    }
}

/// Row of the `ListView`
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ListViewRow {
    /// Each cell is an opaque Dom object
    pub cells: DomVec,
    /// Height of the row, if known beforehand
    pub height: OptionPixelValueNoPercent,
}

impl_option!(ListViewRow, OptionListViewRow, copy = false, [Debug, Clone]);
impl_vec!(
    ListViewRow,
    ListViewRowVec,
    ListViewRowVecDestructor,
    ListViewRowVecDestructorType,
    ListViewRowVecSlice,
    OptionListViewRow
);
impl_vec_clone!(ListViewRow, ListViewRowVec, ListViewRowVecDestructor);
impl_vec_mut!(ListViewRow, ListViewRowVec);
impl_vec_debug!(ListViewRow, ListViewRowVec);

impl ListView {
    #[must_use]
    pub fn create(columns: StringVec) -> Self {
        Self {
            columns,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut m = Self::default();
        core::mem::swap(&mut m, self);
        m
    }

    #[must_use]
    pub fn with_columns(mut self, columns: StringVec) -> Self {
        self.set_columns(columns);
        self
    }

    pub fn set_columns(&mut self, columns: StringVec) {
        self.columns = columns;
    }

    #[must_use]
    pub fn with_rows(mut self, rows: ListViewRowVec) -> Self {
        self.set_rows(rows);
        self
    }

    pub fn set_rows(&mut self, rows: ListViewRowVec) {
        self.rows = rows;
    }

    /// The half-open range `[first, last)` of row indices visible in a
    /// vertically-scrolled, fixed-row-height list — the windowing core for
    /// virtualizing a long `ListView` (render only these rows instead of all of
    /// them, the way the `MapWidget`'s `VirtualView` renders only visible tiles).
    /// `scroll_y` is pixels scrolled past the top, `viewport_height` the visible
    /// height; one extra row is included so a row straddling the bottom edge
    /// still renders. Returns `(0, 0)` for degenerate input (no rows, a
    /// non-positive/non-finite height, or non-finite scroll), and an empty range
    /// `(total, total)` once scrolled past the end.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded layout/render numeric cast
    #[must_use]
    pub fn visible_row_range(
        scroll_y: f32,
        viewport_height: f32,
        row_height: f32,
        total_rows: usize,
    ) -> (usize, usize) {
        if total_rows == 0
            || !row_height.is_finite()
            || row_height <= 0.0
            || !viewport_height.is_finite()
            || viewport_height <= 0.0
            || !scroll_y.is_finite()
        {
            return (0, 0);
        }
        let first = (scroll_y.max(0.0) / row_height).floor() as usize;
        if first >= total_rows {
            return (total_rows, total_rows);
        }
        // Saturating: a sub-pixel `row_height` makes `viewport_height / row_height`
        // astronomically large, whose `as usize` cast saturates to `usize::MAX`, so
        // `+ 1` (and `first + visible`) would overflow. The `.min(total_rows)` clamp
        // makes the saturated value harmless.
        let visible = ((viewport_height / row_height).ceil() as usize).saturating_add(1);
        let last = first.saturating_add(visible).min(total_rows);
        (first, last)
    }

    #[must_use]
    pub const fn with_sorted_by(mut self, sorted_by: OptionUsize) -> Self {
        self.set_sorted_by(sorted_by);
        self
    }

    pub const fn set_sorted_by(&mut self, sorted_by: OptionUsize) {
        self.sorted_by = sorted_by;
    }

    #[must_use]
    pub const fn with_scroll_offset(mut self, scroll_offset: PixelValueNoPercent) -> Self {
        self.set_scroll_offset(scroll_offset);
        self
    }

    pub const fn set_scroll_offset(&mut self, scroll_offset: PixelValueNoPercent) {
        self.scroll_offset = scroll_offset;
    }

    #[must_use]
    pub fn with_content_height(mut self, content_height: PixelValueNoPercent) -> Self {
        self.set_content_height(content_height);
        self
    }

    pub fn set_content_height(&mut self, content_height: PixelValueNoPercent) {
        self.content_height = Some(content_height).into();
    }

    #[must_use]
    pub fn with_column_context_menu(mut self, context_menu: Menu) -> Self {
        self.set_column_context_menu(context_menu);
        self
    }

    pub fn set_column_context_menu(&mut self, column_context_menu: Menu) {
        self.column_context_menu = Some(column_context_menu).into();
    }

    #[must_use]
    pub fn with_on_column_click<C: Into<ListViewOnColumnClickCallback>>(
        mut self,
        refany: RefAny,
        on_column_click: C,
    ) -> Self {
        self.set_on_column_click(refany, on_column_click);
        self
    }

    pub fn set_on_column_click<C: Into<ListViewOnColumnClickCallback>>(
        &mut self,
        refany: RefAny,
        on_column_click: C,
    ) {
        self.on_column_click = Some(ListViewOnColumnClick {
            refany,
            callback: on_column_click.into(),
        })
        .into();
    }

    #[must_use]
    pub fn with_on_row_click<C: Into<ListViewOnRowClickCallback>>(
        mut self,
        refany: RefAny,
        on_row_click: C,
    ) -> Self {
        self.set_on_row_click(refany, on_row_click);
        self
    }

    pub fn set_on_row_click<C: Into<ListViewOnRowClickCallback>>(
        &mut self,
        refany: RefAny,
        on_row_click: C,
    ) {
        self.on_row_click = Some(ListViewOnRowClick {
            refany,
            callback: on_row_click.into(),
        })
        .into();
    }

    #[must_use]
    pub fn dom(self) -> Dom {
        // Snapshot the state handed to row/column click callbacks. Runtime-only
        // fields (scroll position / content height) aren't known at build time,
        // so they default to zero; columns/sorted_by/row-count/scroll-offset are.
        let state = ListViewState {
            columns: self.columns.clone(),
            sorted_by: self.sorted_by,
            current_row_count: self.rows.as_ref().len(),
            scroll_offset: self.scroll_offset,
            current_scroll_position: LogicalPosition::zero(),
            current_content_height: LogicalSize::zero(),
        };
        let on_column_click = self.on_column_click.clone();
        let on_row_click = self.on_row_click.clone();

        Dom::create_div()
            .with_css_props(CSS_MATCH_17553577885456905601)
            .with_ids_and_classes(LIST_VIEW_CONTAINER_CLASS)
            .with_children(DomVec::from_vec(vec![
                // header
                Dom::create_div()
                    .with_css_props(CSS_MATCH_15315949193378715186)
                    .with_ids_and_classes(HEADER_CONTAINER_CLASS)
                    .with_children(
                        self.columns
                            .iter()
                            .enumerate()
                            .map(|(col_index, col)| {
                                let col_dom = Dom::create_div()
                                    .with_css_props(CSS_MATCH_12498280255863106397)
                                    .with_ids_and_classes(COLUMN_NAME_CLASS)
                                    .with_child({
                                        crate::widgets::widget_p_with_text(col.clone())
                                            .with_css_props(CSS_MATCH_15673486787900743642)
                                    });
                                // Wire the click only when the app set a handler.
                                match &on_column_click {
                                    OptionListViewOnColumnClick::Some(_) => col_dom.with_callbacks(
                                        vec![CoreCallbackData {
                                            event: EventFilter::Hover(HoverEventFilter::MouseUp),
                                            refany: RefAny::new(ColumnClickData {
                                                col_index,
                                                state: state.clone(),
                                                on_column_click: on_column_click.clone(),
                                            }),
                                            callback: CoreCallback {
                                                cb: on_list_view_column_click as usize,
                                                ctx: OptionRefAny::None,
                                            },
                                        }]
                                        .into(),
                                    ),
                                    OptionListViewOnColumnClick::None => col_dom,
                                }
                            })
                            .collect::<Vec<_>>()
                            .into(),
                    ),
                // rows
                Dom::create_div()
                    .with_css_props(CSS_MATCH_4852927511892172364)
                    .with_ids_and_classes(ROW_CONTAINER_CLASS)
                    .with_children(
                        self.rows
                            .into_iter()
                            .enumerate()
                            .map(|(row_index, row)| {
                                let row_dom = Dom::create_div()
                                    .with_css_props(CSS_MATCH_7894335449545988724)
                                    .with_ids_and_classes(ROW_CLASS)
                                    .with_tab_index(TabIndex::Auto)
            // Role so the accessibility tree knows what this IS:
            // a list, so a reader can say "3 of 12". The NAME comes from the widget's own text,
            // which azul derives when a readable label is present.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::List,
                ..Default::default()
            })
                                    .with_children(
                                        row.cells
                                            .as_ref()
                                            .iter()
                                            .map(|cell| {
                                                Dom::create_div()
                                                    .with_css_props(CSS_MATCH_12980082330151137475)
                                                    .with_ids_and_classes(CELL_CLASS)
                                                    .with_child(cell.clone())
                                            })
                                            .collect::<Vec<_>>()
                                            .into(),
                                    );
                                match &on_row_click {
                                    OptionListViewOnRowClick::Some(_) => row_dom.with_callbacks(
                                        vec![CoreCallbackData {
                                            event: EventFilter::Hover(HoverEventFilter::MouseUp),
                                            refany: RefAny::new(RowClickData {
                                                row_index,
                                                state: state.clone(),
                                                on_row_click: on_row_click.clone(),
                                            }),
                                            callback: CoreCallback {
                                                cb: on_list_view_row_click as usize,
                                                ctx: OptionRefAny::None,
                                            },
                                        }]
                                        .into(),
                                    ),
                                    OptionListViewOnRowClick::None => row_dom,
                                }
                            })
                            .collect::<Vec<_>>()
                            .into(),
                    ),
            ]))
    }
}

/// Per-row data carried to the internal `MouseUp` handler (the row index plus a
/// snapshot of the list state and the app's `on_row_click` hook).
struct RowClickData {
    row_index: usize,
    state: ListViewState,
    on_row_click: OptionListViewOnRowClick,
}

/// Per-column equivalent of [`RowClickData`].
struct ColumnClickData {
    col_index: usize,
    state: ListViewState,
    on_column_click: OptionListViewOnColumnClick,
}

/// `MouseUp` on a row → invoke the app's `on_row_click(state, row_index)`.
extern "C" fn on_list_view_row_click(mut refany: RefAny, info: CallbackInfo) -> Update {
    let Some(data) = refany.downcast_ref::<RowClickData>() else {
        return Update::DoNothing;
    };
    match data.on_row_click.as_ref() {
        Some(ListViewOnRowClick {
            refany: user_data,
            callback,
        }) => (callback.cb)(user_data.clone(), info, data.state.clone(), data.row_index),
        None => Update::DoNothing,
    }
}

/// `MouseUp` on a column header → invoke the app's `on_column_click(state, col_index)`.
extern "C" fn on_list_view_column_click(mut refany: RefAny, info: CallbackInfo) -> Update {
    let Some(data) = refany.downcast_ref::<ColumnClickData>() else {
        return Update::DoNothing;
    };
    match data.on_column_click.as_ref() {
        Some(ListViewOnColumnClick {
            refany: user_data,
            callback,
        }) => (callback.cb)(user_data.clone(), info, data.state.clone(), data.col_index),
        None => Update::DoNothing,
    }
}

#[cfg(test)]
mod list_view_click_tests {
    use super::*;

    /// The windowing core for `ListView` virtualization: only the visible rows
    /// (+1 straddling the bottom) are in range, the range tracks scroll, clamps
    /// to the row count, and degenerate input yields an empty range.
    #[test]
    fn visible_row_range_windows_correctly() {
        // 100 rows x 20px, 200px viewport → 10 full rows + 1 partial.
        assert_eq!(ListView::visible_row_range(0.0, 200.0, 20.0, 100), (0, 11));
        // Scrolled 50px → first row = floor(50/20) = 2.
        assert_eq!(ListView::visible_row_range(50.0, 200.0, 20.0, 100), (2, 13));
        // Near the end → clamped to the row count.
        assert_eq!(
            ListView::visible_row_range(1900.0, 200.0, 20.0, 100),
            (95, 100)
        );
        // Scrolled past the end → empty range at the tail.
        assert_eq!(
            ListView::visible_row_range(5000.0, 200.0, 20.0, 100),
            (100, 100)
        );
        // Degenerate inputs → empty.
        assert_eq!(ListView::visible_row_range(0.0, 200.0, 20.0, 0), (0, 0));
        assert_eq!(ListView::visible_row_range(0.0, 200.0, 0.0, 100), (0, 0));
        assert_eq!(
            ListView::visible_row_range(f32::NAN, 200.0, 20.0, 100),
            (0, 0)
        );
    }

    extern "C" fn noop_row(_: RefAny, _: CallbackInfo, _: ListViewState, _: usize) -> Update {
        Update::DoNothing
    }

    fn empty_row() -> ListViewRow {
        ListViewRow {
            cells: DomVec::from_const_slice(&[]),
            height: None.into(),
        }
    }

    /// Rows must carry a click callback exactly when `on_row_click` is set —
    /// previously `dom()` wired nothing, so the hook was dead.
    #[test]
    #[allow(clippy::field_reassign_with_default)] // struct built incrementally / test setup; a struct literal is not clearer here
    fn rows_get_a_click_callback_only_when_on_row_click_is_set() {
        let mut lv = ListView::default();
        lv.rows = ListViewRowVec::from_vec(vec![empty_row(), empty_row()]);
        let on_row_click: ListViewOnRowClickCallbackType = noop_row;
        lv.set_on_row_click(RefAny::new(()), on_row_click);
        let dom = lv.dom();
        // children = [header, rows]; each row div carries the MouseUp callback.
        let rows = dom.children.as_ref()[1].children.as_ref();
        assert_eq!(rows.len(), 2);
        for row in rows {
            assert_eq!(
                row.root.callbacks.as_ref().len(),
                1,
                "row must carry the click callback when on_row_click is set"
            );
        }

        // Without the hook → no callbacks (opt-in, no wasted dispatch).
        let mut bare = ListView::default();
        bare.rows = ListViewRowVec::from_vec(vec![empty_row()]);
        let dom2 = bare.dom();
        let rows2 = dom2.children.as_ref()[1].children.as_ref();
        assert_eq!(rows2.len(), 1);
        assert!(
            rows2[0].root.callbacks.as_ref().is_empty(),
            "no callback when on_row_click is unset"
        );
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact float compares are deliberate here (saturation / identity checks)
mod autotest_generated {
    use azul_core::{
        dom::NodeType,
        menu::{MenuItem, MenuItemVec},
    };

    use super::*;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn cols(names: &[&str]) -> StringVec {
        StringVec::from_vec(names.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    /// A row with `n` text cells (`n == 0` is allowed and deliberately used).
    fn row_with(n: usize) -> ListViewRow {
        ListViewRow {
            cells: DomVec::from_vec(
                (0..n)
                    .map(|i| {
                        Dom::create_text_do_not_use_without_block_level_wrapper(format!("c{i}"))
                    })
                    .collect::<Vec<_>>(),
            ),
            height: None.into(),
        }
    }

    fn px(v: f32) -> PixelValueNoPercent {
        PixelValueNoPercent {
            inner: PixelValue::px(v),
        }
    }

    extern "C" fn noop_row_cb(_: RefAny, _: CallbackInfo, _: ListViewState, _: usize) -> Update {
        Update::DoNothing
    }

    extern "C" fn noop_col_cb(_: RefAny, _: CallbackInfo, _: ListViewState, _: usize) -> Update {
        Update::DoNothing
    }

    /// The `[header, rows]` container pair every `ListView` DOM is built from.
    fn header_and_rows(dom: &Dom) -> (&Dom, &Dom) {
        let ch = dom.children.as_ref();
        assert_eq!(ch.len(), 2, "list view DOM = [header, rows]");
        (&ch[0], &ch[1])
    }

    /// The text of a text node, looking through the `<p>` block wrapper the
    /// label convention mandates (`p > text`).
    fn text_of(dom: &Dom) -> &str {
        match &dom.root.node_type {
            NodeType::Text(s) => s.as_ref().as_str(),
            NodeType::P => match dom.children.as_ref() {
                [only] => text_of(only),
                _ => panic!("a label <p> must wrap exactly one text node"),
            },
            _ => panic!("expected a text node"),
        }
    }

    // ------------------------------------------------------------------
    // `visible_row_range` — numeric core (zero / negative / NaN / limits)
    // ------------------------------------------------------------------

    /// Every degenerate input documented as "empty" really returns `(0, 0)`,
    /// including `-0.0` (which must count as non-positive, not as a valid height).
    #[test]
    fn visible_row_range_zero_and_degenerate_inputs_are_empty() {
        assert_eq!(ListView::visible_row_range(0.0, 100.0, 10.0, 0), (0, 0));
        assert_eq!(ListView::visible_row_range(0.0, 100.0, 0.0, 5), (0, 0));
        assert_eq!(ListView::visible_row_range(0.0, 100.0, -0.0, 5), (0, 0));
        assert_eq!(ListView::visible_row_range(0.0, 100.0, -10.0, 5), (0, 0));
        assert_eq!(ListView::visible_row_range(0.0, 0.0, 10.0, 5), (0, 0));
        assert_eq!(ListView::visible_row_range(0.0, -0.0, 10.0, 5), (0, 0));
        assert_eq!(ListView::visible_row_range(0.0, -100.0, 10.0, 5), (0, 0));
        assert_eq!(ListView::visible_row_range(0.0, 0.0, 0.0, 0), (0, 0));
    }

    /// A negative scroll offset (rubber-band / over-scroll) must clamp to the
    /// top window rather than wrapping through the `as usize` cast.
    #[test]
    fn visible_row_range_negative_scroll_clamps_to_the_top_window() {
        let top = ListView::visible_row_range(0.0, 200.0, 20.0, 100);
        assert_eq!(top, (0, 11), "10 full rows + 1 straddling the bottom edge");
        for s in [-0.0_f32, -1.0, -0.5, -1e9, -f32::MAX, -f32::MIN_POSITIVE] {
            assert_eq!(
                ListView::visible_row_range(s, 200.0, 20.0, 100),
                top,
                "negative scroll {s} must clamp to the top window"
            );
        }
    }

    /// NaN / ±inf in any float argument yields the documented empty range — no
    /// panic, no garbage index out of the float→int cast.
    #[test]
    fn visible_row_range_nan_and_infinite_inputs_are_empty() {
        for b in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                ListView::visible_row_range(b, 200.0, 20.0, 100),
                (0, 0),
                "scroll_y = {b}"
            );
            assert_eq!(
                ListView::visible_row_range(0.0, b, 20.0, 100),
                (0, 0),
                "viewport_height = {b}"
            );
            assert_eq!(
                ListView::visible_row_range(0.0, 200.0, b, 100),
                (0, 0),
                "row_height = {b}"
            );
        }
        // All-NaN with the largest possible row count is still empty.
        assert_eq!(
            ListView::visible_row_range(f32::NAN, f32::NAN, f32::NAN, usize::MAX),
            (0, 0)
        );
    }

    /// Scrolling past the end returns the empty tail range, and a `scroll_y`
    /// large enough to saturate the `as usize` cast takes the same path.
    #[test]
    fn visible_row_range_past_the_end_is_an_empty_tail_range() {
        // 10 rows x 20px = 200px of content; scrolling exactly to the end.
        assert_eq!(
            ListView::visible_row_range(200.0, 100.0, 20.0, 10),
            (10, 10)
        );
        assert_eq!(
            ListView::visible_row_range(1e9, 200.0, 20.0, 100),
            (100, 100)
        );
        // f32::MAX / 20 overflows usize; the saturating cast keeps it >= total.
        assert_eq!(
            ListView::visible_row_range(f32::MAX, 200.0, 20.0, 100),
            (100, 100)
        );
        // ... one float ULP before the end is still a live window.
        let (first, last) = ListView::visible_row_range(199.0, 100.0, 20.0, 10);
        assert!(first < last, "just before the end the range is non-empty");
        assert_eq!(last, 10, "and clamps to the row count");
    }

    /// A huge `total_rows` must not make the window huge — only the viewport
    /// decides how many rows are returned.
    #[test]
    fn visible_row_range_window_size_is_bounded_by_the_viewport_not_the_row_count() {
        for total in [1_usize, 2, 1000, u32::MAX as usize, usize::MAX] {
            let (first, last) = ListView::visible_row_range(0.0, 200.0, 20.0, total);
            assert_eq!(first, 0);
            assert_eq!(
                last,
                11.min(total),
                "window stays viewport-sized for total_rows = {total}"
            );
        }
    }

    /// Property: whatever the inputs, the returned window is well-ordered,
    /// clamped to the row count, and actually covers the visible strip.
    #[test]
    fn visible_row_range_window_always_covers_the_viewport() {
        let total = 1000_usize;
        for &h in &[1.0_f32, 7.5, 20.0, 33.3] {
            for &vp in &[1.0_f32, 17.0, 200.0, 999.0] {
                for &s in &[0.0_f32, 0.1, 19.0, 123.456, 5000.0] {
                    let (first, last) = ListView::visible_row_range(s, vp, h, total);
                    assert!(first <= last, "well-ordered range for ({s}, {vp}, {h})");
                    assert!(last <= total, "range clamps to the row count");
                    if first == last {
                        continue; // empty tail range: nothing to cover
                    }
                    let top = first as f32 * h;
                    let bottom = last as f32 * h;
                    assert!(top <= s, "window starts at or above scroll {s} (top {top})");
                    let needed = (s + vp).min(total as f32 * h);
                    assert!(
                        bottom >= needed,
                        "window bottom {bottom} must reach {needed} for ({s}, {vp}, {h})"
                    );
                }
            }
        }
    }

    /// KNOWN BUG — pinned deliberately, do NOT weaken to make it pass.
    ///
    /// `visible_row_range` computes `(viewport_height / row_height).ceil() as
    /// usize + 1` and then `first + visible`. The float→int cast *saturates* to
    /// `usize::MAX`, so the `+ 1` (and the later addition) overflow and panic
    /// under the default dev/test `overflow-checks`. Both inputs below are
    /// finite, positive and pass every existing guard. Expected safe behaviour
    /// is to saturate and clamp to `total_rows`, as the doc comment promises.
    #[test]
    fn visible_row_range_does_not_overflow_on_extreme_but_finite_input() {
        // Sub-pixel row height (degenerate zoom): 1000 / 1e-30 = 1e33, which
        // saturates the cast to usize::MAX -> `+ 1` overflows.
        assert_eq!(ListView::visible_row_range(0.0, 1000.0, 1e-30, 10), (0, 10));

        // `first + visible` overflows even when neither term alone saturates:
        // ~1e19 + ~1e19 > usize::MAX (~1.84e19).
        let (first, last) = ListView::visible_row_range(1.0e19, 1.0e19, 1.0, usize::MAX);
        assert!(first > 0, "a huge scroll lands deep in the list");
        assert_eq!(last, usize::MAX, "the window must clamp to the row count");
    }

    // ------------------------------------------------------------------
    // Constructors / setters — round-trip + invariants
    // ------------------------------------------------------------------

    #[test]
    fn create_sets_columns_and_leaves_everything_else_default() {
        let lv = ListView::create(cols(&["a", "b", "c"]));
        assert_eq!(lv.columns.len(), 3);
        assert_eq!(lv.columns.as_ref()[1].as_str(), "b");
        assert!(lv.rows.is_empty());
        assert!(lv.sorted_by.is_none());
        assert!(lv.content_height.is_none());
        assert!(lv.column_context_menu.is_none());
        assert!(lv.on_lazy_load_scroll.is_none());
        assert!(lv.on_column_click.is_none());
        assert!(lv.on_row_click.is_none());
        assert_eq!(lv.scroll_offset, PixelValueNoPercent::zero());

        // An empty column list is accepted, not rejected or defaulted.
        let empty = ListView::create(StringVec::from_const_slice(&[]));
        assert!(empty.columns.is_empty());
    }

    #[test]
    fn with_and_set_columns_replace_rather_than_append() {
        let b = cols(&["x", "y"]);
        let lv = ListView::default()
            .with_columns(cols(&["one"]))
            .with_columns(b.clone());
        assert_eq!(lv.columns, b, "the last write wins");

        let mut m = ListView::default();
        m.set_columns(b.clone());
        assert_eq!(m.columns, b);
        m.set_columns(StringVec::from_const_slice(&[]));
        assert!(m.columns.is_empty(), "columns can be cleared again");
    }

    /// Column names are opaque payload: empty strings, interior NULs, astral /
    /// ZWJ / RTL / combining sequences and very long strings must survive the
    /// builder *and* the DOM build byte-for-byte.
    #[test]
    fn columns_round_trip_unicode_and_pathological_strings() {
        let names: Vec<String> = vec![
            String::new(),
            "\u{0}".to_string(),
            "🦀👨‍👩‍👧‍👦".to_string(),
            "مرحبا بالعالم".to_string(),
            "e\u{301}\u{301}\u{301}".to_string(),
            "\u{feff}leading BOM".to_string(),
            "line\nbreak\ttab".to_string(),
            "x".repeat(10_000),
        ];
        let sv = StringVec::from_vec(
            names
                .iter()
                .map(|s| AzString::from(s.clone()))
                .collect::<Vec<_>>(),
        );
        let lv = ListView::default().with_columns(sv);
        assert_eq!(lv.columns.len(), names.len());
        for (got, want) in lv.columns.iter().zip(names.iter()) {
            assert_eq!(got.as_str(), want.as_str(), "column name must round-trip");
        }

        let dom = lv.dom();
        let (header, _) = header_and_rows(&dom);
        let hdr = header.children.as_ref();
        assert_eq!(hdr.len(), names.len());
        for (col, want) in hdr.iter().zip(names.iter()) {
            let text = col.children.as_ref();
            assert_eq!(text.len(), 1, "each header cell holds one text node");
            assert_eq!(text_of(&text[0]), want.as_str());
        }
    }

    #[test]
    fn with_and_set_rows_round_trip_including_empty_and_ragged_rows() {
        let lv = ListView::default().with_rows(ListViewRowVec::from_vec(vec![
            row_with(0),
            row_with(1),
            row_with(5),
        ]));
        assert_eq!(lv.rows.len(), 3);
        assert_eq!(lv.rows.as_ref()[0].cells.len(), 0);
        assert_eq!(lv.rows.as_ref()[2].cells.len(), 5);

        let mut m = ListView::default().with_rows(ListViewRowVec::from_vec(vec![row_with(2)]));
        m.set_rows(ListViewRowVec::from_const_slice(&[]));
        assert!(m.rows.is_empty(), "rows can be cleared again");
    }

    /// `sorted_by` is a raw column index with no validation — an out-of-range
    /// value is stored verbatim and must not break the DOM build.
    #[test]
    fn sorted_by_is_stored_verbatim_even_when_out_of_range() {
        for v in [None, Some(0_usize), Some(2), Some(usize::MAX)] {
            let lv = ListView::create(cols(&["a", "b"])).with_sorted_by(v.into());
            assert_eq!(
                lv.sorted_by.as_ref().copied(),
                v,
                "sorted_by is not validated against the column count"
            );
            let dom = lv.dom();
            let (header, _) = header_and_rows(&dom);
            assert_eq!(header.children.as_ref().len(), 2);
        }

        let mut m = ListView::default();
        m.set_sorted_by(Some(7_usize).into());
        assert_eq!(m.sorted_by.as_ref().copied(), Some(7));
        m.set_sorted_by(None.into());
        assert!(m.sorted_by.is_none(), "sorted_by can be reset to None");
    }

    /// The fixed-point `PixelValue` encoding saturates (NaN -> 0, ±inf -> the
    /// isize limits) instead of trapping, and the setter stores the value bit
    /// for bit.
    #[test]
    fn scroll_offset_round_trips_extreme_and_non_finite_values() {
        for v in [
            0.0_f32,
            -0.0,
            1.0,
            -1.0,
            0.001,
            -12345.678,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let p = px(v);
            let lv = ListView::default().with_scroll_offset(p);
            assert_eq!(lv.scroll_offset, p, "scroll_offset stored verbatim ({v})");
            let got = lv.scroll_offset.inner.number.get();
            assert!(
                got.is_finite(),
                "encoded offset stays finite for {v} (got {got})"
            );
        }

        assert_eq!(px(f32::NAN).inner.number.get(), 0.0, "NaN saturates to 0");
        assert!(px(f32::INFINITY).inner.number.get() > 0.0);
        assert!(px(f32::NEG_INFINITY).inner.number.get() < 0.0);

        let mut m = ListView::default();
        m.set_scroll_offset(px(5.0));
        m.set_scroll_offset(px(-5.0));
        assert_eq!(m.scroll_offset, px(-5.0), "the last write wins");
    }

    /// `PixelValueNoPercent` does not actually reject a `%` metric — the setter
    /// takes whatever it is handed. Pinned so a future validation change is a
    /// deliberate decision, not a silent one.
    #[test]
    fn scroll_offset_accepts_a_percent_metric_despite_the_type_name() {
        let percent = PixelValueNoPercent {
            inner: PixelValue::percent(50.0),
        };
        let lv = ListView::default().with_scroll_offset(percent);
        assert_eq!(lv.scroll_offset, percent);
        assert_eq!(lv.scroll_offset.inner.metric, SizeMetric::Percent);
    }

    #[test]
    fn content_height_wraps_in_some_and_has_no_clearing_setter() {
        let mut m = ListView::default();
        assert!(m.content_height.is_none(), "unset by default");
        for v in [0.0_f32, -1.0, f32::MAX, f32::NAN, f32::NEG_INFINITY] {
            let p = px(v);
            m.set_content_height(p);
            assert_eq!(m.content_height.as_ref(), Some(&p), "stores {v} verbatim");
            assert!(m
                .content_height
                .as_ref()
                .expect("just set")
                .inner
                .number
                .get()
                .is_finite());
        }
        assert!(
            m.content_height.is_some(),
            "Some() is sticky — no unset API"
        );

        let lv = ListView::default().with_content_height(px(42.0));
        assert_eq!(lv.content_height.as_ref(), Some(&px(42.0)));
    }

    #[test]
    fn column_context_menu_is_stored_and_replaced() {
        let mut m = ListView::default();
        assert!(m.column_context_menu.is_none());

        let empty_menu = Menu::create(MenuItemVec::from_const_slice(&[]));
        m.set_column_context_menu(empty_menu.clone());
        assert_eq!(
            m.column_context_menu.as_ref(),
            Some(&empty_menu),
            "an empty menu is accepted, not silently dropped"
        );

        let full = Menu::create(MenuItemVec::from_vec(vec![MenuItem::Separator; 256]));
        m.set_column_context_menu(full.clone());
        assert_eq!(
            m.column_context_menu.as_ref(),
            Some(&full),
            "last write wins"
        );
        assert!(
            m.column_context_menu.is_some(),
            "there is no way to unset it"
        );

        let lv = ListView::default().with_column_context_menu(full.clone());
        assert_eq!(lv.column_context_menu.as_ref(), Some(&full));
        // The menu is metadata only — it must not alter the DOM shape.
        let dom = lv.dom();
        let (header, rows) = header_and_rows(&dom);
        assert!(header.children.as_ref().is_empty());
        assert!(rows.children.as_ref().is_empty());
    }

    #[test]
    fn click_hook_setters_replace_rather_than_accumulate() {
        let rcb: ListViewOnRowClickCallbackType = noop_row_cb;
        let ccb: ListViewOnColumnClickCallbackType = noop_col_cb;

        let mut m = ListView::default();
        assert!(m.on_row_click.is_none());
        assert!(m.on_column_click.is_none());
        m.set_on_row_click(RefAny::new(1_u32), rcb);
        m.set_on_row_click(RefAny::new(2_u32), rcb);
        assert!(m.on_row_click.is_some());
        let mut payload = m.on_row_click.as_ref().expect("just set").refany.clone();
        assert_eq!(
            *payload.downcast_ref::<u32>().expect("u32 payload"),
            2,
            "the second registration replaces the first"
        );

        let lv = ListView::default()
            .with_on_row_click(RefAny::new(()), rcb)
            .with_on_column_click(RefAny::new(()), ccb);
        assert!(lv.on_row_click.is_some());
        assert!(lv.on_column_click.is_some());
        assert!(
            lv.on_lazy_load_scroll.is_none(),
            "unrelated hooks stay unset"
        );
    }

    // ------------------------------------------------------------------
    // `swap_with_default`
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_moves_state_out_and_leaves_a_pristine_default() {
        let rcb: ListViewOnRowClickCallbackType = noop_row_cb;
        let mut lv = ListView::create(cols(&["a", "b"]))
            .with_rows(ListViewRowVec::from_vec(vec![row_with(1)]))
            .with_sorted_by(Some(1_usize).into())
            .with_scroll_offset(px(9.0))
            .with_content_height(px(1000.0))
            .with_column_context_menu(Menu::create(MenuItemVec::from_const_slice(&[])))
            .with_on_row_click(RefAny::new(()), rcb);

        let taken = lv.swap_with_default();
        assert_eq!(taken.columns.len(), 2);
        assert_eq!(taken.rows.len(), 1);
        assert_eq!(taken.sorted_by.as_ref().copied(), Some(1));
        assert_eq!(taken.scroll_offset, px(9.0));
        assert!(taken.content_height.is_some());
        assert!(taken.column_context_menu.is_some());
        assert!(taken.on_row_click.is_some());

        assert!(lv.columns.is_empty());
        assert!(lv.rows.is_empty());
        assert!(lv.sorted_by.is_none());
        assert_eq!(lv.scroll_offset, PixelValueNoPercent::zero());
        assert!(lv.content_height.is_none());
        assert!(lv.column_context_menu.is_none());
        assert!(lv.on_row_click.is_none());
        assert!(lv.on_column_click.is_none());
        assert!(lv.on_lazy_load_scroll.is_none());

        // Repeated swaps of an already-default value are a no-op, not a
        // double-free of the moved-out heap buffers.
        let again = lv.swap_with_default();
        assert!(again.columns.is_empty() && again.rows.is_empty());
        let third = lv.swap_with_default();
        assert!(third.columns.is_empty() && third.rows.is_empty());
        drop(taken);
    }

    // ------------------------------------------------------------------
    // `dom()` — shape + callback wiring
    // ------------------------------------------------------------------

    #[test]
    fn dom_shape_matches_the_column_and_row_counts() {
        let empty = ListView::default().dom();
        let (h, r) = header_and_rows(&empty);
        assert!(h.children.as_ref().is_empty());
        assert!(r.children.as_ref().is_empty());

        // Columns without rows and rows without columns are both legal.
        let no_rows = ListView::create(cols(&["a", "b"])).dom();
        let (h, r) = header_and_rows(&no_rows);
        assert_eq!(h.children.as_ref().len(), 2);
        assert!(r.children.as_ref().is_empty());

        let no_cols = ListView::default()
            .with_rows(ListViewRowVec::from_vec(vec![row_with(3)]))
            .dom();
        let (h, r) = header_and_rows(&no_cols);
        assert!(h.children.as_ref().is_empty());
        assert_eq!(r.children.as_ref().len(), 1);
        assert_eq!(
            r.children.as_ref()[0].children.as_ref().len(),
            3,
            "cells are rendered even with no matching column headers"
        );

        // Ragged rows keep their own cell counts (no padding to the column count).
        let ragged = ListView::create(cols(&["a", "b", "c"]))
            .with_rows(ListViewRowVec::from_vec(vec![
                row_with(0),
                row_with(1),
                row_with(3),
                row_with(7),
            ]))
            .dom();
        let (h, r) = header_and_rows(&ragged);
        assert_eq!(h.children.as_ref().len(), 3);
        let rows = r.children.as_ref();
        assert_eq!(rows.len(), 4);
        for (row, want) in rows.iter().zip([0_usize, 1, 3, 7]) {
            assert_eq!(row.children.as_ref().len(), want);
            for cell in row.children.as_ref() {
                assert_eq!(cell.children.as_ref().len(), 1, "one child per cell");
            }
        }
    }

    /// The wired-in `MouseUp` handler must receive the *right* index and a
    /// faithful snapshot of the list state — this exercises the exact
    /// `downcast_ref` path `on_list_view_{row,column}_click` take.
    #[test]
    fn dom_wires_click_payloads_with_the_right_index_and_state_snapshot() {
        let rcb: ListViewOnRowClickCallbackType = noop_row_cb;
        let ccb: ListViewOnColumnClickCallbackType = noop_col_cb;
        let lv = ListView::create(cols(&["c0", "c1"]))
            .with_rows(ListViewRowVec::from_vec(vec![
                row_with(2),
                row_with(2),
                row_with(2),
            ]))
            .with_sorted_by(Some(1_usize).into())
            .with_scroll_offset(px(-17.5))
            .with_on_row_click(RefAny::new(()), rcb)
            .with_on_column_click(RefAny::new(()), ccb);

        let dom = lv.dom();
        let (header, rows) = header_and_rows(&dom);

        for (i, col) in header.children.as_ref().iter().enumerate() {
            let cbs = col.root.callbacks.as_ref();
            assert_eq!(cbs.len(), 1);
            assert!(matches!(
                cbs[0].event,
                EventFilter::Hover(HoverEventFilter::MouseUp)
            ));
            assert_eq!(cbs[0].callback.cb, on_list_view_column_click as usize);
            let mut any = cbs[0].refany.clone();
            let data = any
                .downcast_ref::<ColumnClickData>()
                .expect("ColumnClickData payload");
            assert_eq!(data.col_index, i, "each header carries its own index");
            assert_eq!(data.state.current_row_count, 3);
            assert_eq!(data.state.columns.len(), 2);
            assert_eq!(data.state.sorted_by.as_ref().copied(), Some(1));
            assert_eq!(data.state.scroll_offset, px(-17.5));
            assert!(data.on_column_click.is_some());
        }

        for (i, row) in rows.children.as_ref().iter().enumerate() {
            let cbs = row.root.callbacks.as_ref();
            assert_eq!(cbs.len(), 1);
            assert!(matches!(
                cbs[0].event,
                EventFilter::Hover(HoverEventFilter::MouseUp)
            ));
            assert_eq!(cbs[0].callback.cb, on_list_view_row_click as usize);
            let mut any = cbs[0].refany.clone();
            let data = any
                .downcast_ref::<RowClickData>()
                .expect("RowClickData payload");
            assert_eq!(data.row_index, i, "each row carries its own index");
            assert_eq!(data.state.current_row_count, 3);
            assert!(data.on_row_click.is_some());
        }
    }

    /// Row and column hooks are wired independently — setting one must not
    /// attach a dispatcher to the other.
    #[test]
    fn click_callbacks_are_wired_per_hook_and_only_when_set() {
        let rcb: ListViewOnRowClickCallbackType = noop_row_cb;
        let ccb: ListViewOnColumnClickCallbackType = noop_col_cb;

        let bare = ListView::create(cols(&["a", "b"])).dom();
        let (h, _) = header_and_rows(&bare);
        for col in h.children.as_ref() {
            assert!(
                col.root.callbacks.as_ref().is_empty(),
                "no hook, no dispatch"
            );
        }

        let row_only = ListView::create(cols(&["a"]))
            .with_rows(ListViewRowVec::from_vec(vec![row_with(1)]))
            .with_on_row_click(RefAny::new(()), rcb)
            .dom();
        let (h, r) = header_and_rows(&row_only);
        assert!(h.children.as_ref()[0].root.callbacks.as_ref().is_empty());
        assert_eq!(r.children.as_ref()[0].root.callbacks.as_ref().len(), 1);

        let col_only = ListView::create(cols(&["a"]))
            .with_rows(ListViewRowVec::from_vec(vec![row_with(1)]))
            .with_on_column_click(RefAny::new(()), ccb)
            .dom();
        let (h, r) = header_and_rows(&col_only);
        assert_eq!(h.children.as_ref()[0].root.callbacks.as_ref().len(), 1);
        assert!(r.children.as_ref()[0].root.callbacks.as_ref().is_empty());
    }

    /// A wrong-typed payload must make the internal handlers bail out rather
    /// than reinterpreting foreign memory. `CallbackInfo` cannot be built here
    /// without the full `LayoutWindow` harness, so this pins the guard that
    /// runs *before* any `CallbackInfo` use: the `downcast_ref` type check.
    #[test]
    fn click_handler_payload_downcast_rejects_foreign_types() {
        let mut wrong = RefAny::new(0_u64);
        assert!(
            wrong.downcast_ref::<RowClickData>().is_none(),
            "handler must not accept a foreign payload type"
        );
        assert!(wrong.downcast_ref::<ColumnClickData>().is_none());

        // ... and a RowClickData payload is not mistaken for a ColumnClickData.
        let mut row_payload = RefAny::new(RowClickData {
            row_index: 0,
            state: ListViewState {
                columns: StringVec::from_const_slice(&[]),
                sorted_by: None.into(),
                current_row_count: 0,
                scroll_offset: PixelValueNoPercent::zero(),
                current_scroll_position: LogicalPosition::zero(),
                current_content_height: LogicalSize::zero(),
            },
            on_row_click: None.into(),
        });
        assert!(row_payload.downcast_ref::<ColumnClickData>().is_none());
        assert!(row_payload.downcast_ref::<RowClickData>().is_some());
    }

    #[test]
    fn dom_survives_a_large_column_and_row_count() {
        const N_COLS: usize = 64;
        const N_ROWS: usize = 64;
        let rcb: ListViewOnRowClickCallbackType = noop_row_cb;
        let names = (0..N_COLS)
            .map(|i| AzString::from(format!("col{i}")))
            .collect::<Vec<_>>();
        let rows = (0..N_ROWS).map(|_| row_with(N_COLS)).collect::<Vec<_>>();
        let dom = ListView::create(StringVec::from_vec(names))
            .with_rows(ListViewRowVec::from_vec(rows))
            .with_on_row_click(RefAny::new(()), rcb)
            .dom();

        let (h, r) = header_and_rows(&dom);
        assert_eq!(h.children.as_ref().len(), N_COLS);
        assert_eq!(r.children.as_ref().len(), N_ROWS);
        for row in r.children.as_ref() {
            assert_eq!(row.children.as_ref().len(), N_COLS);
            assert_eq!(row.root.callbacks.as_ref().len(), 1);
        }
    }
}
