use azul_core::{
    callbacks::{
        CoreCallback, CoreCallbackData, LayoutCallback, LayoutCallbackInfo,
        MarshaledLayoutCallback, MarshaledLayoutCallbackInner, Update,
    },
    dom::{
        Dom, DomVec, EventFilter, FocusEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec,
        NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec, TabIndex, WindowEventFilter,
    },
    refany::RefAny,
    styled_dom::StyledDom,
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
use azul_layout::callbacks::{Callback, CallbackInfo};

const STRING_16146701490593874959: AzString = AzString::from_const_str("sans-serif");
const STYLE_BACKGROUND_CONTENT_4857374953508308215_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            from: DirectionCorner::Top,
            to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_8909964754681718371_ITEMS,
        ),
    })];
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
const STYLE_BACKGROUND_CONTENT_16125239329823337131_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            from: DirectionCorner::Top,
            to: DirectionCorner::Bottom,
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
const LINEAR_COLOR_STOP_8010235203234495977_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorU {
            r: 218,
            g: 236,
            b: 252,
            a: 255,
        },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorU {
            r: 196,
            g: 224,
            b: 252,
            a: 255,
        },
    },
];
const LINEAR_COLOR_STOP_8909964754681718371_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorU {
            r: 235,
            g: 244,
            b: 252,
            a: 255,
        },
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorU {
            r: 220,
            g: 236,
            b: 252,
            a: 255,
        },
    },
];

const CSS_MATCH_10188117026223137249_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-dropdown-wrapper:focus
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
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Focus(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16125239329823337131_ITEMS,
        )),
    )),
    // .__azul-native-dropdown-wrapper:active
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
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 86,
                g: 157,
                b: 229,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Active(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16125239329823337131_ITEMS,
        )),
    )),
    // .__azul-native-dropdown-wrapper:hover
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
            STYLE_BACKGROUND_CONTENT_4857374953508308215_ITEMS,
        )),
    )),
    // .__azul-native-dropdown-wrapper
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(2),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(2),
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
    NodeDataInlineCssProperty::Normal(CssProperty::MinWidth(LayoutMinWidthValue::Exact(
        LayoutMinWidth {
            inner: PixelValue::const_px(120),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_18001933966972968559_ITEMS),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(0),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(
        LayoutDisplay::Block,
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
];
const CSS_MATCH_10188117026223137249: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_10188117026223137249_PROPERTIES);

const CSS_MATCH_16432538576103237591_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-dropdown-wrapper p
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(8),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Display(LayoutDisplayValue::Exact(
        LayoutDisplay::Block,
    ))),
];
const CSS_MATCH_16432538576103237591: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_16432538576103237591_PROPERTIES);

const CSS_MATCH_2883986488332352590_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // body
    NodeDataInlineCssProperty::Normal(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS,
        )),
    )),
];
const CSS_MATCH_2883986488332352590: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_2883986488332352590_PROPERTIES);

const CSS_MATCH_4428877324022630014_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-dropdown
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(0),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
];
const CSS_MATCH_4428877324022630014: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_4428877324022630014_PROPERTIES);

const CSS_MATCH_4687758758634879229_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-dropdown-arrow-wrapper
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
            inner: PixelValue::const_px(0),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(0),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::JustifyContent(
        LayoutJustifyContentValue::Exact(LayoutJustifyContent::End),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Row,
    ))),
];
const CSS_MATCH_4687758758634879229: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_4687758758634879229_PROPERTIES);

const CSS_MATCH_5369484915686807864_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-dropdown-arrow-content
    NodeDataInlineCssProperty::Normal(CssProperty::Width(LayoutWidthValue::Exact(
        LayoutWidth::Px(PixelValue::const_px(6)),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Transform(StyleTransformVecValue::Exact(
        StyleTransformVec::from_const_slice(STYLE_TRANSFORM_9499236770162623295_ITEMS),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(6)),
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(2),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 96,
                g: 96,
                b: 96,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(2),
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
                r: 96,
                g: 96,
                b: 96,
                a: 255,
            },
        }),
    )),
];
const CSS_MATCH_5369484915686807864: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_5369484915686807864_PROPERTIES);

const CSS_MATCH_6763840958685503000_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-dropdown-arrow
    NodeDataInlineCssProperty::Normal(CssProperty::MinWidth(LayoutMinWidthValue::Exact(
        LayoutMinWidth {
            inner: PixelValue::const_px(20),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::JustifyContent(
        LayoutJustifyContentValue::Exact(LayoutJustifyContent::Center),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(0),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
];
const CSS_MATCH_6763840958685503000: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_6763840958685503000_PROPERTIES);

const CSS_MATCH_7938442083662451131_PROPERTIES: &[NodeDataInlineCssProperty] = &[
    // .__azul-native-dropdown-focused-text:hover
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
            inner: BorderStyle::Dotted,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Dotted,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Dotted,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Dotted,
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
        }),
    )),
    NodeDataInlineCssProperty::Hover(CssProperty::BorderTopColor(
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
    NodeDataInlineCssProperty::Normal(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(15),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomRightRadius(
        StyleBorderBottomRightRadiusValue::Exact(StyleBorderBottomRightRadius {
            inner: PixelValue::const_px(2),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderBottomLeftRadius(
        StyleBorderBottomLeftRadiusValue::Exact(StyleBorderBottomLeftRadius {
            inner: PixelValue::const_px(2),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopRightRadius(
        StyleBorderTopRightRadiusValue::Exact(StyleBorderTopRightRadius {
            inner: PixelValue::const_px(2),
        }),
    )),
    NodeDataInlineCssProperty::Normal(CssProperty::BorderTopLeftRadius(
        StyleBorderTopLeftRadiusValue::Exact(StyleBorderTopLeftRadius {
            inner: PixelValue::const_px(2),
        }),
    )),
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
    NodeDataInlineCssProperty::Normal(CssProperty::AlignItems(LayoutAlignItemsValue::Exact(
        LayoutAlignItems::Center,
    ))),
];
const CSS_MATCH_7938442083662451131: NodeDataInlineCssPropertyVec =
    NodeDataInlineCssPropertyVec::from_const_slice(CSS_MATCH_7938442083662451131_PROPERTIES);

pub type DropDownOnChoiceChangeCallbackType =
    extern "C" fn(&mut RefAny, &mut CallbackInfo, usize) -> Update;
impl_callback!(
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
        let data = RefAny::new(self);

        Dom::div()
            .with_inline_css_props(CSS_MATCH_4428877324022630014)
            .with_ids_and_classes({
                const IDS_AND_CLASSES_9466018534284317754: &[IdOrClass] =
                    &[Class(AzString::from_const_str("__azul-native-dropdown"))];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_9466018534284317754)
            })
            .with_children(DomVec::from_vec(vec![Dom::div()
                .with_inline_css_props(CSS_MATCH_10188117026223137249)
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
                        data: data.clone(),
                        callback: CoreCallback {
                            cb: on_dropdown_click as usize,
                        },
                    }]
                    .into(),
                )
                .with_children(DomVec::from_vec(vec![
                    Dom::div()
                        .with_inline_css_props(CSS_MATCH_7938442083662451131)
                        .with_ids_and_classes({
                            const IDS_AND_CLASSES_11862789041977911489: &[IdOrClass] = &[Class(
                                AzString::from_const_str("__azul-native-dropdown-focused-text"),
                            )];
                            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_11862789041977911489)
                        })
                        .with_children(DomVec::from_vec(vec![Dom::text(
                            AzString::from_const_str("Checkbox"),
                        )
                        .with_inline_css_props(CSS_MATCH_16432538576103237591)])),
                    Dom::div()
                        .with_inline_css_props(CSS_MATCH_6763840958685503000)
                        .with_ids_and_classes({
                            const IDS_AND_CLASSES_17649077225810153180: &[IdOrClass] = &[Class(
                                AzString::from_const_str("__azul-native-dropdown-arrow"),
                            )];
                            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_17649077225810153180)
                        })
                        .with_children(DomVec::from_vec(vec![Dom::div()
                            .with_inline_css_props(CSS_MATCH_4687758758634879229)
                            .with_ids_and_classes({
                                const IDS_AND_CLASSES_17777388057004109464: &[IdOrClass] =
                                    &[Class(AzString::from_const_str(
                                        "__azul-native-dropdown-arrow-wrapper",
                                    ))];
                                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_17777388057004109464)
                            })
                            .with_children(DomVec::from_vec(vec![Dom::div()
                                .with_inline_css_props(CSS_MATCH_5369484915686807864)
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
    on_choice_change: OptionDropDownOnChoiceChange,
    width_px: f32,
}

extern "C" fn on_dropdown_click(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    use azul_core::window::WindowPosition;
    use azul_layout::window_state::WindowCreateOptions;

    println!("dropdown clicked!");

    let data = match data.downcast_ref::<DropDown>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    println!("1!");

    let size = match info.get_node_size(info.get_hit_node()) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    println!("2!");

    let position = match info.get_node_position(info.get_hit_node()) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    println!("3!");

    let mut child_window_state = info.get_current_window_state();

    // align the child window to the bottom of the checkbox
    let mut pos = position;
    pos.y += size.height;

    let window_pos = match child_window_state.position {
        WindowPosition::Uninitialized => return Update::DoNothing,
        WindowPosition::Initialized(window_top_left_corner) => {
            let mut child_window_pos = window_top_left_corner.clone();
            child_window_pos.x += pos.x as i32;
            child_window_pos.y += pos.y as i32;
            WindowPosition::Initialized(child_window_pos)
        }
    };

    println!("4!");

    child_window_state.position = window_pos;

    #[cfg(target_os = "windows")]
    {
        use azul_core::window::{OptionHwndHandle, RawWindowHandle, WindowsHandle};
        let raw_window_handle = match info.get_current_window_handle() {
            RawWindowHandle::Windows(WindowsHandle { hwnd, hinstance }) => {
                OptionHwndHandle::Some(hwnd)
            }
            _ => return Update::DoNothing,
        };

        // parent window = this window
        child_window_state
            .platform_specific_options
            .windows_options
            .parent_window = raw_window_handle;
    }

    // special callback to pass in the dropdown choices to the child window
    child_window_state.layout_callback = LayoutCallback::Marshaled(MarshaledLayoutCallback {
        marshal_data: RefAny::new(DropDownLocalDataset {
            choices: data.choices.clone(),
            on_choice_change: data.on_choice_change.clone(),
            width_px: size.width,
        }),
        cb: MarshaledLayoutCallbackInner {
            cb: dropdownWindowLayoutFn,
        },
    });

    println!("4!");

    info.create_window(WindowCreateOptions {
        state: child_window_state,
        size_to_content: true,
        renderer: None.into(),
        theme: None.into(),
        create_callback: None.into(),
        hot_reload: false,
    });

    println!("5!");

    Update::DoNothing
}

struct ChoiceChangeLocalDataset {
    choice_id: usize,
    on_choice_change: OptionDropDownOnChoiceChange,
}

#[allow(non_snake_case)]
extern "C" fn dropdownWindowLayoutFn(
    data: &mut RefAny,
    _: &mut RefAny,
    _info: &mut LayoutCallbackInfo,
) -> StyledDom {
    println!("rendering window!");

    let data_clone = data.clone();
    let dropdown_local_dataset = match data.downcast_ref::<DropDownLocalDataset>() {
        Some(s) => s,
        None => return StyledDom::default(),
    };

    dropdown_local_dataset
        .choices
        .iter()
        .enumerate()
        .map(|(choice_id, choice)| {
            Dom::text(choice.clone())
                .with_tab_index(TabIndex::Auto)
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Focus(FocusEventFilter::FocusReceived),
                        data: RefAny::new(ChoiceChangeLocalDataset {
                            choice_id,
                            on_choice_change: dropdown_local_dataset.on_choice_change.clone(),
                        }),
                        callback: CoreCallback {
                            cb: on_choice_change as usize,
                        },
                    }]
                    .into(),
                )
        })
        .collect::<Dom>()
        .with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Window(WindowEventFilter::WindowFocusLost),
                data: data_clone,
                callback: CoreCallback {
                    cb: close_choice_window as usize,
                },
            }]
            .into(),
        )
        .style(Default::default())
    // .style(CssApiWrapper::empty())
}

extern "C" fn on_choice_change(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let result = {
        let mut data = match data.downcast_mut::<ChoiceChangeLocalDataset>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };

        let choice_id = data.choice_id;

        match data.on_choice_change.as_mut() {
            Some(DropDownOnChoiceChange { data, callback }) => (callback.cb)(data, info, choice_id),
            None => Update::DoNothing,
        }
    };

    close_choice_window(data, info);

    result
}

extern "C" fn close_choice_window(_: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut flags = info.get_current_window_flags();
    flags.is_about_to_close = true;
    info.set_window_flags(flags);
    Update::RefreshDom
}
