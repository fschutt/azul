use alloc::vec::Vec;

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{
        Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec, NodeDataInlineCssProperty,
        NodeDataInlineCssPropertyVec, TabIndex,
    },
    geom::{LogicalPosition, LogicalSize},
    gl::OptionUsize,
    menu::{Menu, OptionMenu},
    refany::RefAny,
};
use azul_css::{
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::callbacks::{Callback, CallbackInfo};

const STRING_16146701490593874959: AzString = AzString::from_const_str("sans-serif");
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
            from: DirectionCorner::Top,
            to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_10827796861537038040_ITEMS,
        ),
    })];
const STYLE_BACKGROUND_CONTENT_7422581697888665934_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            from: DirectionCorner::Top,
            to: DirectionCorner::Bottom,
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
        color: ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(50),
        color: ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(51),
        color: ColorU {
            r: 247,
            g: 248,
            b: 250,
            a: 255,
        },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorU {
            r: 243,
            g: 244,
            b: 246,
            a: 255,
        },
    },
];
const LINEAR_COLOR_STOP_10827796861537038040_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorU {
            r: 247,
            g: 252,
            b: 254,
            a: 255,
        },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(50),
        color: ColorU {
            r: 247,
            g: 252,
            b: 254,
            a: 255,
        },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(51),
        color: ColorU {
            r: 232,
            g: 246,
            b: 254,
            a: 255,
        },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorU {
            r: 206,
            g: 231,
            b: 244,
            a: 255,
        },
    },
];

const CSS_MATCH_1085706216385961159_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header-arrow-down
    NodeDataInlineCssProperty::Normal(CssProperty::Transform(StyleTransformVecValue::Exact(
        StyleTransformVec::from_const_slice(STYLE_TRANSFORM_6162542744002865382_ITEMS),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(
        LayoutPosition::Absolute,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(3),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(3),
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
    NodeDataInlineCssProperty::Normal(CssProperty::JustifyContent(
        LayoutJustifyContentValue::Exact(LayoutJustifyContent::Center),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
];
const CSS_MATCH_1085706216385961159: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_1085706216385961159_PROPERTIES);

const CSS_MATCH_12498280255863106397_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header-item:hover
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 154,
                g: 223,
                b: 254,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_6112684430356720596_ITEMS,
        )),
    )),
    // .__azul_native-list-header-item:active
    NodeDataInlineCssProperty::Active(CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(
        StyleBoxShadow {
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
        },
    ))),
    NodeDataInlineCssProperty::Active(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(
        StyleBoxShadow {
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
        },
    ))),
    NodeDataInlineCssProperty::Active(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(
        StyleBoxShadow {
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
        },
    ))),
    NodeDataInlineCssProperty::Active(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(
        StyleBoxShadow {
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
        },
    ))),
    NodeDataInlineCssProperty::Active(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 194,
                g: 205,
                b: 219,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 194,
                g: 205,
                b: 219,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 194,
                g: 205,
                b: 219,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 194,
                g: 205,
                b: 219,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_3839348353894170136_ITEMS,
        )),
    )),
    // .__azul_native-list-header-item
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(
        LayoutPosition::Relative,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(7),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::MinWidth(LayoutMinWidthValue::Exact(
        LayoutMinWidth {
            inner: PixelValue::const_px(100),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
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
const CSS_MATCH_12498280255863106397: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_12498280255863106397_PROPERTIES);

const CSS_MATCH_12980082330151137475_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-rows-row-cell
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(7),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::MinWidth(LayoutMinWidthValue::Exact(
        LayoutMinWidth {
            inner: PixelValue::const_px(100),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
    ))),
];
const CSS_MATCH_12980082330151137475: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_12980082330151137475_PROPERTIES);

const CSS_MATCH_13758717721055992976_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header-arrow-down-inner
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
        LayoutWidth::Px(PixelValue::const_px(6)),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Transform(StyleTransformVecValue::Exact(
        StyleTransformVec::from_const_slice(STYLE_TRANSFORM_16978981723642914576_ITEMS),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowY(LayoutOverflowValue::Exact(
        LayoutOverflow::Hidden,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::OverflowX(LayoutOverflowValue::Exact(
        LayoutOverflow::Hidden,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(6)),
    ))),
];
const CSS_MATCH_13758717721055992976: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_13758717721055992976_PROPERTIES);

const CSS_MATCH_15295293133676720691_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header-dragwidth-drag
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
        LayoutWidth::Px(PixelValue::const_px(2)),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(
        LayoutPosition::Absolute,
    ))),
];
const CSS_MATCH_15295293133676720691: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_15295293133676720691_PROPERTIES);

const CSS_MATCH_15315949193378715186_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(25)),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_7422581697888665934_ITEMS,
        )),
    )),
];
const CSS_MATCH_15315949193378715186: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_15315949193378715186_PROPERTIES);

const CSS_MATCH_15673486787900743642_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header .__azul_native-list-header-item p
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::TextColor(StyleTextColorValue::Exact(
        StyleTextColor {
            inner: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::Center,
    ))),
];
const CSS_MATCH_15673486787900743642: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_15673486787900743642_PROPERTIES);

const CSS_MATCH_1574792189506859253_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header-arrow-down-inner-deco
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
        LayoutWidth::Px(PixelValue::const_px(12)),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Transform(StyleTransformVecValue::Exact(
        StyleTransformVec::from_const_slice(STYLE_TRANSFORM_17732691695785266054_ITEMS),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(12)),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(
        StyleBoxShadow {
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
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(
        StyleBoxShadow {
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
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(
        StyleBoxShadow {
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
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(
        StyleBoxShadow {
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
        },
    ))),
];
const CSS_MATCH_1574792189506859253: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_1574792189506859253_PROPERTIES);

const CSS_MATCH_17553577885456905601_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native_list-container
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_2444935983575427872_ITEMS,
        )),
    )),
];
const CSS_MATCH_17553577885456905601: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_17553577885456905601_PROPERTIES);

const CSS_MATCH_2883986488332352590_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // body
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
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_11062356617965867290_ITEMS,
        )),
    )),
];
const CSS_MATCH_2883986488332352590: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_2883986488332352590_PROPERTIES);

const CSS_MATCH_4852927511892172364_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-rows
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
];
const CSS_MATCH_4852927511892172364: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_4852927511892172364_PROPERTIES);

const CSS_MATCH_6002662151290653203_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-header-dragwidth
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
        LayoutWidth::Px(PixelValue::const_px(0)),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Position(LayoutPositionValue::Exact(
        LayoutPosition::Relative,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
];
const CSS_MATCH_6002662151290653203: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_6002662151290653203_PROPERTIES);

const CSS_MATCH_6827198030119836081_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-rows-row.selected
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
                r: 102,
                g: 167,
                b: 232,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 102,
                g: 167,
                b: 232,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 102,
                g: 167,
                b: 232,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 102,
                g: 167,
                b: 232,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_661302523448178568_ITEMS,
        )),
    )),
    // .__azul_native-list-rows-row:hover
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
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_3010057533077499049_ITEMS,
        )),
    )),
    // .__azul_native-list-rows-row
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(0),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(0),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(2),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(2),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
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
                r: 255,
                g: 255,
                b: 255,
                a: 0,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 0,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 0,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
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
const CSS_MATCH_6827198030119836081: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_6827198030119836081_PROPERTIES);

const CSS_MATCH_7894335449545988724_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-rows-row.focused
    NodeDataInlineCssProperty::Focus(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 38,
                g: 160,
                b: 218,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 38,
                g: 160,
                b: 218,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 38,
                g: 160,
                b: 218,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 38,
                g: 160,
                b: 218,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_11098930083828139815_ITEMS,
        )),
    )),
    // .__azul_native-list-rows-row:hover
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
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 101,
                g: 181,
                b: 220,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_3010057533077499049_ITEMS,
        )),
    )),
    // .__azul_native-list-rows-row
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(0),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(0),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(2),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(2),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
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
                r: 255,
                g: 255,
                b: 255,
                a: 0,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 0,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 0,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopColor(
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
const CSS_MATCH_7894335449545988724: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_7894335449545988724_PROPERTIES);

const CSS_MATCH_7937682281721781688_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-rows-row-cell
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(7),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::MinWidth(LayoutMinWidthValue::Exact(
        LayoutMinWidth {
            inner: PixelValue::const_px(100),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
    ))),
];
const CSS_MATCH_7937682281721781688: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_7937682281721781688_PROPERTIES);

const CSS_MATCH_8793836789597026811_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul_native-list-rows-row-cell
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(7),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::MinWidth(LayoutMinWidthValue::Exact(
        LayoutMinWidth {
            inner: PixelValue::const_px(100),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
    ))),
];
const CSS_MATCH_8793836789597026811: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_8793836789597026811_PROPERTIES);

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
impl_callback!(
    ListViewOnLazyLoadScroll,
    OptionListViewOnLazyLoadScroll,
    ListViewOnLazyLoadScrollCallback,
    ListViewOnLazyLoadScrollCallbackType
);

pub type ListViewOnColumnClickCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ListViewState, column_clicked: usize) -> Update;
impl_callback!(
    ListViewOnColumnClick,
    OptionListViewOnColumnClick,
    ListViewOnColumnClickCallback,
    ListViewOnColumnClickCallbackType
);

pub type ListViewOnRowClickCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ListViewState, row_clicked: usize) -> Update;
impl_callback!(
    ListViewOnRowClick,
    OptionListViewOnRowClick,
    ListViewOnRowClickCallback,
    ListViewOnRowClickCallbackType
);

/// State of the ListView, but without row data
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
    /// Current position where the user has scrolled the ListView to
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
    /// Currently rendered rows. Note that the ListView does not
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
    /// Indicates that this ListView is being lazily loaded, allows
    /// control over what happens when the user scrolls the ListView.
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

/// Row of the ListView
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ListViewRow {
    /// Each cell is an opaque Dom object
    pub cells: DomVec,
    /// Height of the row, if known beforehand
    pub height: OptionPixelValueNoPercent,
}

impl_vec!(ListViewRow, ListViewRowVec, ListViewRowVecDestructor);
impl_vec_clone!(ListViewRow, ListViewRowVec, ListViewRowVecDestructor);
impl_vec_mut!(ListViewRow, ListViewRowVec);
impl_vec_debug!(ListViewRow, ListViewRowVec);

impl ListView {
    pub fn new(columns: StringVec) -> Self {
        Self {
            columns,
            ..Default::default()
        }
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut m = Self::default();
        core::mem::swap(&mut m, self);
        m
    }

    pub fn with_columns(mut self, columns: StringVec) -> Self {
        self.set_columns(columns);
        self
    }

    pub fn set_columns(&mut self, columns: StringVec) {
        self.columns = columns;
    }

    pub fn with_rows(mut self, rows: ListViewRowVec) -> Self {
        self.set_rows(rows);
        self
    }

    pub fn set_rows(&mut self, rows: ListViewRowVec) {
        self.rows = rows;
    }

    pub fn with_sorted_by(mut self, sorted_by: OptionUsize) -> Self {
        self.set_sorted_by(sorted_by);
        self
    }

    pub fn set_sorted_by(&mut self, sorted_by: OptionUsize) {
        self.sorted_by = sorted_by;
    }

    pub fn with_scroll_offset(mut self, scroll_offset: PixelValueNoPercent) -> Self {
        self.set_scroll_offset(scroll_offset);
        self
    }

    pub fn set_scroll_offset(&mut self, scroll_offset: PixelValueNoPercent) {
        self.scroll_offset = scroll_offset;
    }

    pub fn with_content_height(mut self, content_height: PixelValueNoPercent) -> Self {
        self.set_content_height(content_height);
        self
    }

    pub fn set_content_height(&mut self, content_height: PixelValueNoPercent) {
        self.content_height = Some(content_height).into();
    }

    pub fn with_column_context_menu(mut self, context_menu: Menu) -> Self {
        self.set_column_context_menu(context_menu);
        self
    }

    pub fn set_column_context_menu(&mut self, column_context_menu: Menu) {
        self.column_context_menu = Some(column_context_menu).into();
    }

    pub fn with_on_column_click(
        mut self,
        data: RefAny,
        on_column_click: ListViewOnColumnClickCallbackType,
    ) -> Self {
        self.set_on_column_click(data, on_column_click);
        self
    }

    pub fn set_on_column_click(
        &mut self,
        data: RefAny,
        on_column_click: ListViewOnColumnClickCallbackType,
    ) {
        self.on_column_click = Some(ListViewOnColumnClick {
            data,
            callback: ListViewOnColumnClickCallback {
                cb: on_column_click,
            },
        })
        .into();
    }

    pub fn with_on_row_click(
        mut self,
        data: RefAny,
        on_row_click: ListViewOnRowClickCallbackType,
    ) -> Self {
        self.set_on_row_click(data, on_row_click);
        self
    }

    pub fn set_on_row_click(&mut self, data: RefAny, on_row_click: ListViewOnRowClickCallbackType) {
        self.on_row_click = Some(ListViewOnRowClick {
            data,
            callback: ListViewOnRowClickCallback { cb: on_row_click },
        })
        .into();
    }

    pub fn dom(self) -> Dom {
        Dom::div()
            .with_inline_css_props(CSS_MATCH_17553577885456905601)
            .with_ids_and_classes(LIST_VIEW_CONTAINER_CLASS)
            .with_children(DomVec::from_vec(vec![
                // header
                Dom::div()
                    .with_inline_css_props(CSS_MATCH_15315949193378715186)
                    .with_ids_and_classes(HEADER_CONTAINER_CLASS)
                    .with_children(
                        self.columns
                            .iter()
                            .map(|col| {
                                Dom::div()
                                    .with_inline_css_props(CSS_MATCH_12498280255863106397)
                                    .with_ids_and_classes(COLUMN_NAME_CLASS)
                                    .with_child({
                                        Dom::text(col.clone())
                                            .with_inline_css_props(CSS_MATCH_15673486787900743642)
                                    })
                            })
                            .collect::<Vec<_>>()
                            .into(),
                    ),
                // rows
                Dom::div()
                    .with_inline_css_props(CSS_MATCH_4852927511892172364)
                    .with_ids_and_classes(ROW_CONTAINER_CLASS)
                    .with_children(
                        self.rows
                            .into_iter()
                            .map(|row| {
                                Dom::div()
                                    .with_inline_css_props(CSS_MATCH_7894335449545988724)
                                    .with_tab_index(TabIndex::Auto)
                                    .with_ids_and_classes(ROW_CLASS.clone())
                                    .with_tab_index(TabIndex::Auto)
                                    .with_children(
                                        row.cells
                                            .as_ref()
                                            .iter()
                                            .map(|cell| {
                                                Dom::div()
                                                    .with_inline_css_props(
                                                        CSS_MATCH_12980082330151137475,
                                                    )
                                                    .with_ids_and_classes(CELL_CLASS)
                                                    .with_child(cell.clone())
                                            })
                                            .collect::<Vec<_>>()
                                            .into(),
                                    )
                            })
                            .collect::<Vec<_>>()
                            .into(),
                    ),
            ]))
    }
}
