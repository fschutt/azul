//! Native list view widget with column headers, row selection, and sorting indicators.

use alloc::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
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

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::themes::flat,
};

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
const STYLE_FONT_FAMILY_8122988506401935406_ITEMS: &[StyleFontFamily] =
    &[StyleFontFamily::System(STRING_16146701490593874959)];
const LINEAR_COLOR_STOP_513857305091467054_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
        offset: PercentageValue::const_new(50),
        color: ColorOrSystem::color(ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
        offset: PercentageValue::const_new(51),
        color: ColorOrSystem::color(ColorU {
            r: 247,
            g: 248,
            b: 250,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 243,
            g: 244,
            b: 246,
            a: 255,
        }),
    },
];

const CSS_MATCH_12498280255863106397_PROPERTIES: &[CssPropertyWithConditions] = &[
    // A flex container: `flex-direction` / `justify-content` / `align-items`
    // below do nothing without it (this rule had none, so the box laid out as
    // a block and its children stacked vertically).
    CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
        LayoutDisplay::Flex,
    ))),
    // .__azul_native-list-header-item:hover and :active, light AND dark.
    // Declared in the theme module — see `themes::flat::LIST_HEADER_HOVER_BG`
    // — because the dark half of each pair needs a palette this file cannot
    // see; declared here they could only ever name the light-mode colour,
    // which is how a hovered header kept its light face on a dark surface.
    // Each light rule is followed by its dark twin so the twin wins in dark
    // mode (inline CSS is last-wins).
    flat::LIST_HOVER_BORDER_BOTTOM_WIDTH,
    flat::LIST_HOVER_BORDER_BOTTOM_WIDTH_DARK,
    flat::LIST_HOVER_BORDER_BOTTOM_STYLE,
    flat::LIST_HOVER_BORDER_BOTTOM_STYLE_DARK,
    flat::LIST_HEADER_HOVER_LINE_COLOR,
    flat::LIST_HEADER_HOVER_LINE_COLOR_DARK,
    flat::LIST_HEADER_HOVER_BG,
    flat::LIST_HEADER_HOVER_BG_DARK,
    // .__azul_native-list-header-item:active
    flat::LIST_HEADER_ACTIVE_SHADOW_BOTTOM,
    flat::LIST_HEADER_ACTIVE_SHADOW_BOTTOM_DARK,
    flat::LIST_HEADER_ACTIVE_SHADOW_TOP,
    flat::LIST_HEADER_ACTIVE_SHADOW_TOP_DARK,
    flat::LIST_HEADER_ACTIVE_SHADOW_RIGHT,
    flat::LIST_HEADER_ACTIVE_SHADOW_RIGHT_DARK,
    flat::LIST_HEADER_ACTIVE_SHADOW_LEFT,
    flat::LIST_HEADER_ACTIVE_SHADOW_LEFT_DARK,
    flat::LIST_ACTIVE_BORDER_BOTTOM_WIDTH,
    flat::LIST_ACTIVE_BORDER_BOTTOM_WIDTH_DARK,
    flat::LIST_ACTIVE_BORDER_LEFT_WIDTH,
    flat::LIST_ACTIVE_BORDER_LEFT_WIDTH_DARK,
    flat::LIST_ACTIVE_BORDER_RIGHT_WIDTH,
    flat::LIST_ACTIVE_BORDER_RIGHT_WIDTH_DARK,
    flat::LIST_ACTIVE_BORDER_TOP_WIDTH,
    flat::LIST_ACTIVE_BORDER_TOP_WIDTH_DARK,
    flat::LIST_ACTIVE_BORDER_BOTTOM_STYLE,
    flat::LIST_ACTIVE_BORDER_BOTTOM_STYLE_DARK,
    flat::LIST_ACTIVE_BORDER_LEFT_STYLE,
    flat::LIST_ACTIVE_BORDER_LEFT_STYLE_DARK,
    flat::LIST_ACTIVE_BORDER_RIGHT_STYLE,
    flat::LIST_ACTIVE_BORDER_RIGHT_STYLE_DARK,
    flat::LIST_ACTIVE_BORDER_TOP_STYLE,
    flat::LIST_ACTIVE_BORDER_TOP_STYLE_DARK,
    flat::LIST_HEADER_ACTIVE_BORDER_BOTTOM_COLOR,
    flat::LIST_HEADER_ACTIVE_BORDER_BOTTOM_COLOR_DARK,
    flat::LIST_HEADER_ACTIVE_BORDER_LEFT_COLOR,
    flat::LIST_HEADER_ACTIVE_BORDER_LEFT_COLOR_DARK,
    flat::LIST_HEADER_ACTIVE_BORDER_RIGHT_COLOR,
    flat::LIST_HEADER_ACTIVE_BORDER_RIGHT_COLOR_DARK,
    flat::LIST_HEADER_ACTIVE_BORDER_TOP_COLOR,
    flat::LIST_HEADER_ACTIVE_BORDER_TOP_COLOR_DARK,
    flat::LIST_HEADER_ACTIVE_BG,
    flat::LIST_HEADER_ACTIVE_BG_DARK,
    // .__azul_native-list-header-item
    // Centre the label in the header's height: without it the text sat on the
    // top edge and the gradient's grey half read as an empty band under it.
    CssPropertyWithConditions::simple(CssProperty::JustifyContent(
        LayoutJustifyContentValue::Exact(LayoutJustifyContent::Center),
    )),
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
    // A cell holds one line of a record, like a table cell: it clips rather
    // than wrapping, so a row keeps the height the list gave it and a long
    // value cannot push the rows below it out of place.
    CssPropertyWithConditions::simple(CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
        StyleWhiteSpace::Nowrap,
    ))),
    CssPropertyWithConditions::simple(CssProperty::OverflowX(LayoutOverflowValue::Exact(
        LayoutOverflow::Hidden,
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
    CssPropertyWithConditions::simple(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
    ))),
    // Same reason as the tree's label: a conditional inline value is not
    // inherited, so the cell states its own dark colour.
    CssPropertyWithConditions::dark_mode(CssProperty::TextColor(StyleTextColorValue::Exact(
        StyleTextColor {
            inner: ColorU {
                r: 230,
                g: 230,
                b: 230,
                a: 255,
            },
        },
    ))),
];
const CSS_MATCH_12980082330151137475: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_12980082330151137475_PROPERTIES);

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
    // A flex container: `flex-direction` / `justify-content` / `align-items`
    // below do nothing without it (this rule had none, so the box laid out as
    // a block and its children stacked vertically).
    CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
        LayoutDisplay::Flex,
    ))),
    // .__azul_native-list-header
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(25)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_7422581697888665934_ITEMS,
        )),
    )),
    CssPropertyWithConditions::dark_mode(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(&[
            StyleBackgroundContent::Color(ColorU {
                r: 43,
                g: 43,
                b: 43,
                a: 255,
            }),
        ])),
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
    CssPropertyWithConditions::dark_mode(CssProperty::TextColor(StyleTextColorValue::Exact(
        StyleTextColor {
            inner: ColorU {
                r: 230,
                g: 230,
                b: 230,
                a: 255,
            },
        },
    ))),
];
const CSS_MATCH_15673486787900743642: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_15673486787900743642_PROPERTIES);

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
    // Dark defaults. Inline CSS takes `@theme dark` conditions and the last
    // matching property wins, so each dark value sits after the light one it
    // replaces. A list is a FIELD: in a dark window it must not stay white.
    CssPropertyWithConditions::dark_mode(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(&[
            StyleBackgroundContent::Color(ColorU {
                r: 31,
                g: 31,
                b: 31,
                a: 255,
            }),
        ])),
    )),
    CssPropertyWithConditions::dark_mode(CssProperty::TextColor(StyleTextColorValue::Exact(
        StyleTextColor {
            inner: ColorU {
                r: 230,
                g: 230,
                b: 230,
                a: 255,
            },
        },
    ))),
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
    // A flex container: `flex-direction` / `justify-content` / `align-items`
    // below do nothing without it (this rule had none, so the box laid out as
    // a block and its children stacked vertically).
    CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
        LayoutDisplay::Flex,
    ))),
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

const CSS_MATCH_7894335449545988724_PROPERTIES: &[CssPropertyWithConditions] = &[
    // A flex container: `flex-direction` / `justify-content` / `align-items`
    // below do nothing without it (this rule had none, so the box laid out as
    // a block and its children stacked vertically).
    CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
        LayoutDisplay::Flex,
    ))),
    // .__azul_native-list-rows-row.focused, light AND dark. Declared in the
    // theme module — see `themes::flat::LIST_ROW_FOCUS_BG` — because the dark
    // half of each pair needs a palette this file cannot see; declared here it
    // could only ever name the light-mode colour, which is how the focused row
    // kept its light ring on a dark surface. Each light rule is followed by
    // its dark twin so the twin wins in dark mode (inline CSS is last-wins).
    flat::LIST_FOCUS_BORDER_BOTTOM_WIDTH,
    flat::LIST_FOCUS_BORDER_BOTTOM_WIDTH_DARK,
    flat::LIST_FOCUS_BORDER_LEFT_WIDTH,
    flat::LIST_FOCUS_BORDER_LEFT_WIDTH_DARK,
    flat::LIST_FOCUS_BORDER_RIGHT_WIDTH,
    flat::LIST_FOCUS_BORDER_RIGHT_WIDTH_DARK,
    flat::LIST_FOCUS_BORDER_TOP_WIDTH,
    flat::LIST_FOCUS_BORDER_TOP_WIDTH_DARK,
    flat::LIST_FOCUS_BORDER_BOTTOM_STYLE,
    flat::LIST_FOCUS_BORDER_BOTTOM_STYLE_DARK,
    flat::LIST_FOCUS_BORDER_LEFT_STYLE,
    flat::LIST_FOCUS_BORDER_LEFT_STYLE_DARK,
    flat::LIST_FOCUS_BORDER_RIGHT_STYLE,
    flat::LIST_FOCUS_BORDER_RIGHT_STYLE_DARK,
    flat::LIST_FOCUS_BORDER_TOP_STYLE,
    flat::LIST_FOCUS_BORDER_TOP_STYLE_DARK,
    flat::LIST_ROW_FOCUS_BORDER_BOTTOM_COLOR,
    flat::LIST_ROW_FOCUS_BORDER_BOTTOM_COLOR_DARK,
    flat::LIST_ROW_FOCUS_BORDER_LEFT_COLOR,
    flat::LIST_ROW_FOCUS_BORDER_LEFT_COLOR_DARK,
    flat::LIST_ROW_FOCUS_BORDER_RIGHT_COLOR,
    flat::LIST_ROW_FOCUS_BORDER_RIGHT_COLOR_DARK,
    flat::LIST_ROW_FOCUS_BORDER_TOP_COLOR,
    flat::LIST_ROW_FOCUS_BORDER_TOP_COLOR_DARK,
    flat::LIST_ROW_FOCUS_BG,
    flat::LIST_ROW_FOCUS_BG_DARK,
    // .__azul_native-list-rows-row:hover, light AND dark. Declared in the
    // theme module — see `themes::flat::LIST_ROW_HOVER_BG` — because the dark
    // half of each pair needs a palette this file cannot see. Each light rule
    // is followed by its dark twin so the twin wins in dark mode.
    flat::LIST_HOVER_BORDER_BOTTOM_WIDTH,
    flat::LIST_HOVER_BORDER_BOTTOM_WIDTH_DARK,
    flat::LIST_HOVER_BORDER_LEFT_WIDTH,
    flat::LIST_HOVER_BORDER_LEFT_WIDTH_DARK,
    flat::LIST_HOVER_BORDER_RIGHT_WIDTH,
    flat::LIST_HOVER_BORDER_RIGHT_WIDTH_DARK,
    flat::LIST_HOVER_BORDER_TOP_WIDTH,
    flat::LIST_HOVER_BORDER_TOP_WIDTH_DARK,
    flat::LIST_HOVER_BORDER_BOTTOM_STYLE,
    flat::LIST_HOVER_BORDER_BOTTOM_STYLE_DARK,
    flat::LIST_HOVER_BORDER_LEFT_STYLE,
    flat::LIST_HOVER_BORDER_LEFT_STYLE_DARK,
    flat::LIST_HOVER_BORDER_RIGHT_STYLE,
    flat::LIST_HOVER_BORDER_RIGHT_STYLE_DARK,
    flat::LIST_HOVER_BORDER_TOP_STYLE,
    flat::LIST_HOVER_BORDER_TOP_STYLE_DARK,
    flat::LIST_ROW_HOVER_BORDER_BOTTOM_COLOR,
    flat::LIST_ROW_HOVER_BORDER_BOTTOM_COLOR_DARK,
    flat::LIST_ROW_HOVER_BORDER_LEFT_COLOR,
    flat::LIST_ROW_HOVER_BORDER_LEFT_COLOR_DARK,
    flat::LIST_ROW_HOVER_BORDER_RIGHT_COLOR,
    flat::LIST_ROW_HOVER_BORDER_RIGHT_COLOR_DARK,
    flat::LIST_ROW_HOVER_BORDER_TOP_COLOR,
    flat::LIST_ROW_HOVER_BORDER_TOP_COLOR_DARK,
    flat::LIST_ROW_HOVER_BG,
    flat::LIST_ROW_HOVER_BG_DARK,
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

/// The class every row carries: how the arrow-key handler finds the rows.
const ROW_CLASS_NAME: &str = "__azul_native-list-rows-row";
const IDS_AND_CLASSES_790316832563530605: &[IdOrClass] =
    &[Class(AzString::from_const_str(ROW_CLASS_NAME))];
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

const IDS_AND_CLASSES_SORT_ARROW: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul_native-list-header-arrow-down",
))];
const SORT_ARROW_CLASS: IdOrClassVec = IdOrClassVec::from_const_slice(IDS_AND_CLASSES_SORT_ARROW);

/// The sort indicator a header shows on the column the list is sorted by.
///
/// `sorted_by` used to change nothing on screen: the three
/// `__azul_native-list-header-arrow-down*` rules had been in this widget's
/// stylesheet from the start and no element ever matched them. They describe a
/// rotated square with two shadowed edges — a CSS triangle — and rendering it
/// produced a filled grey SQUARE, because nothing clips the rotated box. The
/// indicator is a glyph instead, the same way the tree view draws its
/// disclosure chevrons, so it is a triangle at any size and inherits the
/// header's own colour.
fn sort_arrow() -> Dom {
    Dom::create_icon(AzString::from_const_str("arrow_drop_up"))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
            SORT_ARROW_STYLE,
        ))
        .with_ids_and_classes(SORT_ARROW_CLASS)
}

/// A small glyph, vertically centred by the header item's own flex box, that
/// never competes with the label for width.
static SORT_ARROW_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: ColorU {
            r: 0x67,
            g: 0x67,
            b: 0x67,
            a: 255,
        },
    })),
];

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
    from_handle_byref_fn: AzListViewOnLazyLoadScrollCallback_createFromHostHandleByref,
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
    from_handle_byref_fn: AzListViewOnColumnClickCallback_createFromHostHandleByref,
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
    from_handle_byref_fn: AzListViewOnRowClickCallback_createFromHostHandleByref,
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
    /// The selected row, if any (default = None).
    ///
    /// The list is ONE stop in the Tab order (WAI-ARIA listbox): Tab lands on
    /// this row, or on the first row when none is selected, and the arrow keys
    /// move between the rows from there. Up/Down/Home/End report the row they
    /// land on through `on_row_click`, like a click - store it here on rebuild.
    pub selected_row: OptionUsize,
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
            selected_row: None.into(),
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
    pub(crate) fn visible_row_range(
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

    /// Builder form of [`Self::set_selected_row`].
    #[must_use]
    pub const fn with_selected_row(mut self, selected_row: OptionUsize) -> Self {
        self.set_selected_row(selected_row);
        self
    }

    /// Which row is selected: the row Tab lands on (the list is one Tab stop).
    /// `None` - or an index past the last row - puts the stop on the first row.
    pub const fn set_selected_row(&mut self, selected_row: OptionUsize) {
        self.selected_row = selected_row;
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

    /// Builder form of [`Self::set_on_lazy_load_scroll`].
    #[must_use]
    pub fn with_on_lazy_load_scroll<C: Into<ListViewOnLazyLoadScrollCallback>>(
        mut self,
        refany: RefAny,
        on_lazy_load_scroll: C,
    ) -> Self {
        self.set_on_lazy_load_scroll(refany, on_lazy_load_scroll);
        self
    }

    /// The lazy-load hook: hears every SETTLED scroll of the row box (the
    /// `ListViewState` it gets carries the live scroll position and the
    /// box's size, so `visible_row_range` tells which rows came into view
    /// and the app loads them and rebuilds). A settled gesture, not the
    /// wheel: the list never takes the wheel from the page.
    pub fn set_on_lazy_load_scroll<C: Into<ListViewOnLazyLoadScrollCallback>>(
        &mut self,
        refany: RefAny,
        on_lazy_load_scroll: C,
    ) {
        self.on_lazy_load_scroll = Some(ListViewOnLazyLoadScroll {
            refany,
            callback: on_lazy_load_scroll.into(),
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
        let on_lazy_load_scroll = self.on_lazy_load_scroll.clone();
        // WAI-ARIA listbox: the rows are ONE Tab stop - the selected row, or
        // the first when none is. The arrow keys move within them.
        let row_stop = crate::widgets::roving::stop_index(
            self.selected_row.into_option(),
            self.rows.as_ref().len(),
        );
        // The row the list announces as selected.
        let selected_row = self.selected_row.into_option();

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
                                let mut col_dom = Dom::create_div()
                                    .with_css_props(CSS_MATCH_12498280255863106397)
                                    .with_ids_and_classes(COLUMN_NAME_CLASS)
                                    .with_child({
                                        crate::widgets::widget_p_with_text(col.clone())
                                            .with_css_props(CSS_MATCH_15673486787900743642)
                                    });
                                if self.sorted_by.into_option() == Some(col_index) {
                                    col_dom = col_dom.with_child(sort_arrow());
                                }
                                // Wire the click only when the app set a handler.
                                match &on_column_click {
                                    OptionListViewOnColumnClick::Some(_) => col_dom.with_callbacks(
                                        vec![CoreCallbackData {
                                            event: EventFilter::Hover(HoverEventFilter::Click),
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
                lazy_rows(
                    on_lazy_load_scroll,
                    &state,
                    Dom::create_div()
                        .with_css_props(CSS_MATCH_4852927511892172364)
                        .with_ids_and_classes(ROW_CONTAINER_CLASS),
                )
                .with_children(
                        self.rows
                            .into_iter()
                            .enumerate()
                            .map(|(row_index, row)| {
                                let row_dom = Dom::create_div()
                                    .with_css_props(CSS_MATCH_7894335449545988724)
                                    .with_ids_and_classes(ROW_CLASS)
                                    .with_tab_index(crate::widgets::roving::item_tab_index(
                                        row_index, row_stop,
                                    ))
                                    // An ITEM of the list, so a reader can say
                                    // "3 of 12", and whether it is the selected
                                    // one. The NAME comes from the row's own text.
                                    .with_accessibility_info(
                                        azul_core::a11y::AccessibilityInfo {
                                            role: azul_core::a11y::AccessibilityRole::ListItem,
                                            states: if selected_row == Some(row_index) {
                                                azul_core::a11y::AccessibilityStateVec::from_vec(
                                                    vec![
                                                        azul_core::a11y::AccessibilityState::Selected,
                                                    ],
                                                )
                                            } else {
                                                azul_core::a11y::AccessibilityStateVec::from_const_slice(&[])
                                            },
                                            ..Default::default()
                                        },
                                    )
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
                                let row_data = RefAny::new(RowClickData {
                                    row_index,
                                    state: state.clone(),
                                    on_row_click: on_row_click.clone(),
                                });
                                // The arrow keys work on every list (focus moves
                                // even when nobody listens for the selection);
                                // the click is wired only when the app set a hook.
                                let key = CoreCallbackData {
                                    event: EventFilter::Focus(
                                        azul_core::events::FocusEventFilter::VirtualKeyDown,
                                    ),
                                    refany: row_data.clone(),
                                    callback: CoreCallback {
                                        cb: on_list_view_row_key as usize,
                                        ctx: OptionRefAny::None,
                                    },
                                };
                                match &on_row_click {
                                    OptionListViewOnRowClick::Some(_) => row_dom.with_callbacks(
                                        vec![
                                            CoreCallbackData {
                                                event: EventFilter::Hover(HoverEventFilter::Click),
                                                refany: row_data,
                                                callback: CoreCallback {
                                                    cb: on_list_view_row_click as usize,
                                                    ctx: OptionRefAny::None,
                                                },
                                            },
                                            key,
                                        ]
                                        .into(),
                                    ),
                                    OptionListViewOnRowClick::None => {
                                        row_dom.with_callbacks(vec![key].into())
                                    }
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
        }) => callback.invoke(user_data.clone(), info, data.state.clone(), data.row_index),
        None => Update::DoNothing,
    }
}

/// Arrow keys on the focused row (WAI-ARIA APG single-select listbox): Up and
/// Down move to the neighbouring row and hold at the ends, Home and End jump to
/// the first / last row. The target row is focused, becomes the list's one Tab
/// stop and is SELECTED - reported through `on_row_click` exactly like a click
/// on it (selection follows focus). Every handled key is `prevent_default`-ed,
/// an arrow at the end of the list included, so spatial navigation cannot walk
/// out of the list. Left/Right, every other key and any key held with Alt,
/// Ctrl, Cmd or Shift keep their default.
extern "C" fn on_list_view_row_key(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::window::VirtualKeyCode as K;

    use crate::widgets::roving::{self, Step};

    let step = match roving::plain_key(&info.get_current_keyboard_state()) {
        Some(K::Up) => Step::Previous,
        Some(K::Down) => Step::Next,
        Some(K::Home) => Step::First,
        Some(K::End) => Step::Last,
        _ => return Update::DoNothing,
    };

    let focused = info.get_hit_node();
    let Some(container) = info.get_parent(focused) else {
        return Update::DoNothing;
    };
    let rows = roving::items_of(&info, container, ROW_CLASS_NAME);
    let Some(current) = rows.iter().position(|n| *n == focused) else {
        return Update::DoNothing;
    };
    let Some(target) = roving::step_target(current, rows.len(), step, false) else {
        return Update::DoNothing;
    };
    let (state, on_row_click) = {
        let Some(data) = refany.downcast_ref::<RowClickData>() else {
            return Update::DoNothing;
        };
        (data.state.clone(), data.on_row_click.clone())
    };

    info.prevent_default();
    if target == current {
        // Already at that end: the key is the list's, but nothing moves.
        return Update::DoNothing;
    }
    // Moved BEFORE the app hears the selection, so a focus it asks for wins.
    roving::move_stop(&mut info, &rows, target);
    // Selection follows focus: the target row is the selected one from now
    // on, announced live - the app's rebuild (if it rebuilds) publishes it
    // again from `selected_row`.
    roving::announce_chosen(
        &mut info,
        &rows,
        target,
        azul_core::a11y::AccessibilityState::Selected,
        None,
    );
    // The rows are the row container's children in order, so the target's
    // position IS its row index.
    match on_row_click.as_ref() {
        Some(ListViewOnRowClick {
            refany: user_data,
            callback,
        }) => callback.invoke(user_data.clone(), info, state, target),
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
        }) => callback.invoke(user_data.clone(), info, data.state.clone(), data.col_index),
        None => Update::DoNothing,
    }
}

// ---- the scroll window of a virtualised list ----
//
// A list that shows thousands of rows renders only the ones in view and asks
// the app for the rest as the user scrolls. What it needs from the engine is
// WHERE its scroll box stands once a gesture settles - never the wheel
// itself, which stays the page's (`widgets::wheel_ownership`): the box
// listens for `ScrollEnd`, reads its offset and size, and the app maps them
// to rows (`ListView::visible_row_range`). Shared by every virtualised
// list (the `ListView`'s lazy-load hook, the mail `SummaryList`).

/// The hook a virtualised list's scroll box registers: `cb` runs with
/// `refany` when a scroll gesture over the box SETTLES.
pub(crate) fn scroll_settled_hook(
    cb: extern "C" fn(RefAny, CallbackInfo) -> Update,
    refany: RefAny,
) -> CoreCallbackData {
    CoreCallbackData {
        event: EventFilter::Hover(HoverEventFilter::ScrollEnd),
        callback: CoreCallback {
            cb: cb as usize,
            ctx: OptionRefAny::None,
        },
        refany,
    }
}

/// Where the scroll box `node` stands: its scroll offset and its size -
/// zero where the engine keeps no scroll state for it yet (a box that never
/// scrolled, a test harness), so the app still hears the settled gesture.
#[must_use]
pub(crate) fn scroll_window_of(
    info: &CallbackInfo,
    node: azul_core::dom::DomNodeId,
) -> (LogicalPosition, LogicalSize) {
    let offset = node
        .node
        .into_crate_internal()
        .and_then(|n| info.get_scroll_offset_for_node(node.dom, n))
        .unwrap_or_else(LogicalPosition::zero);
    let size = info.get_node_size(node).unwrap_or_else(LogicalSize::zero);
    (offset, size)
}

/// The row box `rows`, listening for a settled scroll when the list has a
/// lazy-load hook.
fn lazy_rows(hook: OptionListViewOnLazyLoadScroll, state: &ListViewState, rows: Dom) -> Dom {
    match hook.into_option() {
        Some(on_lazy_load_scroll) => rows.with_callbacks(
            vec![scroll_settled_hook(
                on_list_view_scroll_settled,
                RefAny::new(LazyLoadData {
                    state: state.clone(),
                    on_lazy_load_scroll,
                }),
            )]
            .into(),
        ),
        None => rows,
    }
}

/// The row box's payload: the list state at build time plus the app's
/// lazy-load hook.
struct LazyLoadData {
    state: ListViewState,
    on_lazy_load_scroll: ListViewOnLazyLoadScroll,
}

/// A scroll over the row box settled: hand the app the state with the live
/// scroll position and box size filled in.
extern "C" fn on_list_view_scroll_settled(mut refany: RefAny, info: CallbackInfo) -> Update {
    let (mut state, hook) = {
        let Some(data) = refany.downcast_ref::<LazyLoadData>() else {
            return Update::DoNothing;
        };
        (data.state.clone(), data.on_lazy_load_scroll.clone())
    };
    let (offset, size) = scroll_window_of(&info, info.get_hit_node());
    state.current_scroll_position = offset;
    state.current_content_height = size;
    let ListViewOnLazyLoadScroll {
        refany: user_data,
        callback,
    } = hook;
    callback.invoke(user_data, info, state)
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
    #[allow(clippy::field_reassign_with_default)] // struct built incrementally / test setup; a
                                                  // struct literal is not clearer here
    fn rows_get_a_click_callback_only_when_on_row_click_is_set() {
        let mut lv = ListView::default();
        lv.rows = ListViewRowVec::from_vec(vec![empty_row(), empty_row()]);
        let on_row_click: ListViewOnRowClickCallbackType = noop_row;
        lv.set_on_row_click(RefAny::new(()), on_row_click);
        let dom = lv.dom();
        let clicks = |row: &Dom| {
            row.root
                .callbacks
                .as_ref()
                .iter()
                .filter(|cb| cb.event == EventFilter::Hover(HoverEventFilter::Click))
                .count()
        };
        // children = [header, rows]; each row div carries the MouseUp callback.
        let rows = dom.children.as_ref()[1].children.as_ref();
        assert_eq!(rows.len(), 2);
        for row in rows {
            assert_eq!(
                clicks(row),
                1,
                "row must carry the click callback when on_row_click is set"
            );
        }

        // Without the hook → no click callback (opt-in, no wasted dispatch).
        // The arrow-key handler stays: focus still moves between the rows.
        let mut bare = ListView::default();
        bare.rows = ListViewRowVec::from_vec(vec![empty_row()]);
        let dom2 = bare.dom();
        let rows2 = dom2.children.as_ref()[1].children.as_ref();
        assert_eq!(rows2.len(), 1);
        assert_eq!(clicks(&rows2[0]), 0, "no click callback when on_row_click is unset");
        assert_eq!(rows2[0].root.callbacks.as_ref().len(), 1, "only the key handler");
    }
}

#[cfg(test)]
mod lazy_load_tests {
    //! The lazy-load hook: a list that loads rows as the user scrolls must
    //! HEAR the scroll. The hook sat on the struct but `dom()` never wired
    //! it, so no app could ever be told to load more.
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::*;
    use crate::widgets::roving::test_support as rv;

    type Log = Arc<Mutex<Vec<usize>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, state: ListViewState) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(state.current_row_count);
        }
        Update::RefreshDom
    }

    fn rows(n: usize) -> ListViewRowVec {
        ListViewRowVec::from_vec(
            (0..n)
                .map(|_| ListViewRow {
                    cells: DomVec::from_const_slice(&[]),
                    height: None.into(),
                })
                .collect(),
        )
    }

    #[test]
    fn a_lazily_loaded_list_hears_a_settled_scroll_and_a_plain_one_does_not() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let dom = ListView::default()
            .with_rows(rows(3))
            .with_on_lazy_load_scroll(
                RefAny::new(log.clone()),
                record as ListViewOnLazyLoadScrollCallbackType,
            )
            .dom();
        let row_box = &dom.children.as_ref()[1];
        let events: Vec<EventFilter> = row_box
            .root
            .get_callbacks()
            .as_ref()
            .iter()
            .map(|cb| cb.event)
            .collect();
        assert_eq!(
            events,
            vec![EventFilter::Hover(HoverEventFilter::ScrollEnd)],
            "the row box reports a scroll once it settles - never the wheel itself, which the \
             page keeps"
        );
        let plain = ListView::default().with_rows(rows(3)).dom();
        assert!(
            plain.children.as_ref()[1]
                .root
                .get_callbacks()
                .as_ref()
                .is_empty(),
            "no hook, no listener"
        );

        // root (0) > header (no columns: no children) > row box.
        let styled = StyledDom::create_from_dom(dom);
        let hierarchy = styled.node_hierarchy.as_ref();
        let header = hierarchy[0]
            .first_child_id(NodeId::new(0))
            .expect("the header");
        let row_box = hierarchy[header.index()]
            .next_sibling_id()
            .expect("the row box");
        let target = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(row_box)),
        };
        let (update, _) = rv::fire(&styled, target, EventFilter::Hover(HoverEventFilter::ScrollEnd))
            .expect("the hook runs");
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");
        assert_eq!(
            *log.lock().expect("log"),
            vec![3],
            "the app hears how many rows are loaded"
        );
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact float compares are deliberate here (saturation / identity
                            // checks)
mod autotest_generated {
    use azul_core::{
        dom::NodeType,
        menu::{MenuItem, MenuItemVec},
    };
    use azul_css::{
        dynamic_selector::{DynamicSelector, PseudoStateType, ThemeCondition},
        props::property::CssPropertyType,
    };

    use super::*;
    use crate::widgets::theme_probe;

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
        assert!(lv.selected_row.is_none());
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
                EventFilter::Hover(HoverEventFilter::Click)
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
            assert_eq!(cbs.len(), 2, "the click and the arrow-key callback");
            assert!(matches!(
                cbs[0].event,
                EventFilter::Hover(HoverEventFilter::Click)
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
        let is_click =
            |cb: &CoreCallbackData| cb.event == EventFilter::Hover(HoverEventFilter::Click);
        let (h, r) = header_and_rows(&row_only);
        assert!(h.children.as_ref()[0].root.callbacks.as_ref().is_empty());
        assert_eq!(
            r.children.as_ref()[0]
                .root
                .callbacks
                .as_ref()
                .iter()
                .filter(|&cb| is_click(cb))
                .count(),
            1
        );

        let col_only = ListView::create(cols(&["a"]))
            .with_rows(ListViewRowVec::from_vec(vec![row_with(1)]))
            .with_on_column_click(RefAny::new(()), ccb)
            .dom();
        let (h, r) = header_and_rows(&col_only);
        assert_eq!(h.children.as_ref()[0].root.callbacks.as_ref().len(), 1);
        assert!(
            !r.children.as_ref()[0]
                .root
                .callbacks
                .as_ref()
                .iter()
                .any(|cb| is_click(cb)),
            "a column hook must not attach a row click"
        );
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
            assert_eq!(row.root.callbacks.as_ref().len(), 2, "click + arrow keys");
        }
    }

    // ------------------------------------------------------------------
    // Interactive states — the theme's contribution
    // ------------------------------------------------------------------

    /// The property types of the declarations on `node` gated on `state`,
    /// split into the light half (no theme condition) and the dark twins
    /// (gated on the dark theme as well). Each entry carries its position in
    /// declaration order, and both halves are sorted by type so they line up.
    /// `(property type, count)` per declaration kind — one vec for the light
    /// state rules on a node, one for their dark twins. Named because clippy's
    /// `type_complexity` (pedantic, on in this crate) refuses the tuple inline.
    type StateCounts = Vec<(CssPropertyType, usize)>;

    fn state_halves(node: &Dom, state: PseudoStateType) -> (StateCounts, StateCounts) {
        let mut light = Vec::new();
        let mut dark = Vec::new();
        for (i, (p, conds)) in node.root.style.iter_inline_properties().enumerate() {
            let conds = conds.as_ref();
            let gated_on_state = conds
                .iter()
                .any(|c| matches!(c, DynamicSelector::PseudoState(s) if *s == state));
            if !gated_on_state {
                continue;
            }
            let dark_gated = conds
                .iter()
                .any(|c| matches!(c, DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)));
            if dark_gated {
                dark.push((p.get_type(), i));
            } else {
                light.push((p.get_type(), i));
            }
        }
        light.sort();
        dark.sort();
        (light, dark)
    }

    /// The hover / pressed / focus rules moved OUT of this file and into the
    /// theme module — a move nothing else in this suite would notice: no
    /// compiler error, and every other assertion still passes if a slice
    /// silently drops a rule or a dark twin. So this counts what a rendered
    /// column header and row actually carry, per state, light and dark.
    #[test]
    fn dom_carries_the_themes_header_and_row_states_with_dark_twins() {
        let dom = ListView::create(cols(&["a"]))
            .with_rows(ListViewRowVec::from_vec(vec![row_with(1)]))
            .dom();
        let (header, rows) = header_and_rows(&dom);
        let header_item = &header.children.as_ref()[0];
        let row = &rows.children.as_ref()[0];

        // (what, node, state, how many light rules it declares in that state)
        let expected = [
            // a hovered header: the bottom edge (width, style, colour) + the face
            ("header item", header_item, PseudoStateType::Hover, 4),
            // a pressed header: four inset shadows, four edges x (width, style,
            // colour), + the face
            ("header item", header_item, PseudoStateType::Active, 17),
            // a header does not take focus
            ("header item", header_item, PseudoStateType::Focus, 0),
            // a hovered row: four edges x (width, style, colour) + the fill
            ("row", row, PseudoStateType::Hover, 13),
            // the focused row: the same ring and fill in stronger colours
            ("row", row, PseudoStateType::Focus, 13),
            // a row is chosen on release, not on press
            ("row", row, PseudoStateType::Active, 0),
        ];

        for (what, node, state, want) in expected {
            let (light, dark) = state_halves(node, state);
            assert_eq!(
                light.len(),
                want,
                "{what} {state:?}: expected {want} light rule(s), got {light:?}",
            );
            let types = |half: &[(CssPropertyType, usize)]| {
                half.iter().map(|(t, _)| *t).collect::<Vec<_>>()
            };
            assert_eq!(
                types(light.as_slice()),
                types(dark.as_slice()),
                "{what} {state:?}: every light rule needs a dark twin on the same property, or \
                 the {what} keeps its light-mode look on a dark surface",
            );
            // Inline CSS is last-wins, so a twin must FOLLOW its light rule or
            // dark mode would never see it.
            for ((t, light_at), (_, dark_at)) in light.iter().zip(&dark) {
                assert!(
                    light_at < dark_at,
                    "{what} {state:?} {t}: the dark twin is declared before its light rule, so \
                     the light rule wins in dark mode",
                );
            }
        }

        // And the dark declarations really are gated, not unconditional.
        assert!(
            !theme_probe::dark(row).is_empty() && !theme_probe::dark(header_item).is_empty(),
            "the theme contributed no dark-mode declarations at all"
        );
    }
}

/// WAI-ARIA APG single-select listbox (P2-12): the rows are ONE Tab stop and
/// the arrow keys move focus - and the selection - between them.
#[cfg(test)]
mod roving_tabindex_tests {
    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, TabIndex},
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::VirtualKeyCode,
    };

    use super::*;
    use crate::{callbacks::CallbackChange, widgets::roving::test_support as rv};

    /// Every row index an `on_row_click` hears.
    struct RowLog {
        seen: Vec<usize>,
    }

    extern "C" fn record_row(
        mut data: RefAny,
        _: CallbackInfo,
        _: ListViewState,
        row: usize,
    ) -> Update {
        if let Some(mut log) = data.downcast_mut::<RowLog>() {
            log.seen.push(row);
        }
        Update::RefreshDom
    }

    fn rows_heard(log: &mut RefAny) -> Vec<usize> {
        log.downcast_ref::<RowLog>()
            .expect("payload must still be a RowLog")
            .seen
            .clone()
    }

    fn empty_rows(n: usize) -> ListViewRowVec {
        ListViewRowVec::from_vec(
            (0..n)
                .map(|_| ListViewRow {
                    cells: DomVec::from_const_slice(&[]),
                    height: None.into(),
                })
                .collect::<Vec<_>>(),
        )
    }

    /// A one-column list of `n` cell-less rows, with `on_row_click` logging
    /// into `log` when one is given.
    fn list(n: usize, log: Option<&RefAny>) -> ListView {
        let lv = ListView::create(StringVec::from_vec(vec![AzString::from("name")]))
            .with_rows(empty_rows(n));
        match log {
            Some(log) => {
                let rcb: ListViewOnRowClickCallbackType = record_row;
                lv.with_on_row_click(log.clone(), rcb)
            }
            None => lv,
        }
    }

    /// A plain tab stop, the list, another plain tab stop. Flattened: root 0,
    /// before 1, list container 2, header 3, column 4 (> label `<p>` 5 > text
    /// 6), rows container 7, row `i` at `8 + i`, after at `8 + n`.
    fn page(lv: ListView) -> StyledDom {
        let stop = || Dom::create_div().with_tab_index(TabIndex::Auto);
        let page = Dom::create_div().with_children(vec![stop(), lv.dom(), stop()].into());
        StyledDom::create_from_dom(page)
    }

    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    fn before() -> DomNodeId {
        node(1)
    }

    fn row(i: usize) -> DomNodeId {
        node(8 + i)
    }

    fn after(n: usize) -> DomNodeId {
        node(8 + n)
    }

    fn press_row(
        styled: &StyledDom,
        i: usize,
        key: VirtualKeyCode,
        held: &[VirtualKeyCode],
    ) -> (Update, Vec<CallbackChange>) {
        rv::press(styled, row(i), key, held)
            .expect("every list row must carry a key handler for the arrow keys")
    }

    #[test]
    fn with_no_row_selected_tab_lands_on_the_first_row_and_the_next_tab_leaves_the_list() {
        let styled = page(list(4, None));
        assert_eq!(
            rv::tab_walk(&styled, Some(before()), true, 2),
            vec![row(0), after(4)],
            "the list is ONE tab stop, not one per row",
        );
        assert_eq!(
            rv::tab_walk(&styled, Some(after(4)), false, 2),
            vec![row(0), before()],
        );
    }

    #[test]
    fn tab_lands_on_the_selected_row_and_an_out_of_range_selection_falls_back_to_the_first() {
        let styled = page(list(4, None).with_selected_row(Some(2_usize).into()));
        assert_eq!(
            rv::tab_walk(&styled, Some(before()), true, 2),
            vec![row(2), after(4)],
        );
        assert_eq!(
            rv::tab_walk(&styled, Some(after(4)), false, 2),
            vec![row(2), before()],
        );

        let styled = page(list(4, None).with_selected_row(Some(9_usize).into()));
        assert_eq!(
            rv::tab_walk(&styled, Some(before()), true, 2),
            vec![row(0), after(4)],
        );
    }

    #[test]
    fn set_selected_row_and_with_selected_row_agree() {
        let mut lv = list(3, None);
        lv.set_selected_row(Some(1_usize).into());
        assert_eq!(lv.selected_row.as_ref().copied(), Some(1));
        let lv = lv.with_selected_row(None.into());
        assert!(lv.selected_row.is_none());
    }

    #[test]
    fn arrow_down_on_a_row_focuses_and_selects_the_next_row() {
        let mut log = RefAny::new(RowLog { seen: Vec::new() });
        let styled = page(list(4, Some(&log)));

        let (update, changes) = press_row(&styled, 0, VirtualKeyCode::Down, &[]);

        assert_eq!(rows_heard(&mut log), vec![1], "selection follows focus");
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");
        assert_eq!(rv::focus_request(&changes), Some(row(1)));
        assert!(rv::prevented(&changes));
    }

    #[test]
    fn home_and_end_jump_to_the_ends_and_up_and_down_hold_there() {
        use azul_core::window::VirtualKeyCode as K;

        // (focused row, key, selected row - or None when nothing moves)
        for (from, key, to) in [
            (2, K::Up, Some(1)),
            (1, K::Down, Some(2)),
            (2, K::Home, Some(0)),
            (1, K::End, Some(3)),
            (0, K::Up, None),
            (3, K::Down, None),
            (0, K::Home, None),
            (3, K::End, None),
        ] {
            let mut log = RefAny::new(RowLog { seen: Vec::new() });
            let styled = page(list(4, Some(&log)));
            let (_, changes) = press_row(&styled, from, key, &[]);
            assert!(
                rv::prevented(&changes),
                "{key:?} on row {from} must stay inside the list"
            );
            match to {
                Some(to) => {
                    assert_eq!(rows_heard(&mut log), vec![to], "{key:?} on row {from}");
                    assert_eq!(rv::focus_request(&changes), Some(row(to)));
                }
                None => {
                    assert!(
                        rows_heard(&mut log).is_empty(),
                        "{key:?} at the end re-selected row {from}"
                    );
                    assert_eq!(rv::focus_request(&changes), None);
                }
            }
        }
    }

    #[test]
    fn after_an_arrow_the_focused_row_is_the_only_tab_stop() {
        let log = RefAny::new(RowLog { seen: Vec::new() });
        let mut styled = page(list(4, Some(&log)));
        let (_, changes) = press_row(&styled, 0, VirtualKeyCode::End, &[]);
        rv::apply_tab_index_writes(&mut styled, &changes);
        assert_eq!(
            rv::tab_walk(&styled, Some(before()), true, 2),
            vec![row(3), after(4)],
        );
        assert_eq!(
            rv::tab_walk(&styled, Some(row(3)), false, 1),
            vec![before()],
        );
    }

    #[test]
    fn a_list_without_on_row_click_still_moves_focus_with_the_arrows() {
        let styled = page(list(3, None));
        let (update, changes) = press_row(&styled, 0, VirtualKeyCode::Down, &[]);
        assert_eq!(update, Update::DoNothing);
        assert_eq!(rv::focus_request(&changes), Some(row(1)));
        assert!(rv::prevented(&changes));
    }

    #[test]
    fn sideways_modified_and_unused_keys_on_a_row_are_not_consumed() {
        use azul_core::window::VirtualKeyCode as K;

        for (key, held) in [
            (K::Left, None),
            (K::Right, None),
            (K::Tab, None),
            (K::Down, Some(K::LAlt)),
            (K::Up, Some(K::LControl)),
            (K::Down, Some(K::LShift)),
            (K::End, Some(K::LWin)),
        ] {
            let mut log = RefAny::new(RowLog { seen: Vec::new() });
            let styled = page(list(3, Some(&log)));
            let held: Vec<K> = held.into_iter().collect();
            let (update, changes) = press_row(&styled, 1, key, &held);
            assert_eq!(update, Update::DoNothing);
            assert!(
                rows_heard(&mut log).is_empty(),
                "{held:?}+{key:?} selected a row"
            );
            assert!(
                changes.is_empty(),
                "{held:?}+{key:?} must not be consumed: {changes:?}"
            );
        }
    }

    // ------------------------------------------------------------------
    // Accessibility: every row is an ITEM of the list (each row declared the
    // `List` role itself) and the selected one says so - also LIVE when an
    // arrow moves the selection, before the app rebuilds.
    // ------------------------------------------------------------------

    #[test]
    fn every_row_is_a_list_item_and_the_selected_one_says_so() {
        use azul_core::a11y::{AccessibilityRole::ListItem, AccessibilityState::Selected};

        let styled = page(list(3, None).with_selected_row(Some(1_usize).into()));
        for i in 0..3 {
            assert_eq!(
                rv::declared(&styled, row(i)),
                Some((ListItem, if i == 1 { vec![Selected] } else { Vec::new() })),
                "row {i}",
            );
        }
    }

    #[test]
    fn an_arrow_announces_the_row_it_selects() {
        use azul_core::a11y::AccessibilityState::Selected;

        let log = RefAny::new(RowLog { seen: Vec::new() });
        let styled = page(list(3, Some(&log)));
        let (_, changes) = press_row(&styled, 0, VirtualKeyCode::Down, &[]);
        assert_eq!(
            rv::announced_states(&changes),
            vec![
                (row(0), Vec::new()),
                (row(1), vec![Selected]),
                (row(2), Vec::new()),
            ],
        );
    }
}

/// The list FOLLOWS the app theme (the user, 2026-10-08: "the list view in
/// AzDrive (and in general?) doesn't follow the theme"). It painted one
/// hard-coded look under every theme - a #FCFCFC field, a white-to-grey
/// header, black titles, no stripes and no selection - so under flora it was
/// a flat island, and under flat it was not even flat's (Office 2010's)
/// paper. Now each theme paints the whole list in its own colours, by day
/// and by night: the ground, the header band and its titles, the stripes,
/// the selection; and every text reads.
#[cfg(test)]
mod theme_tests {
    use azul_css::{props::basic::color::ColorOrSystem, system::DarkLightMode};

    use super::*;
    use crate::widgets::themes::{
        decl, flat, flora,
        theme_blocks::checks::{live_inline, under, BOTH},
        theme_checks as tc, UiTheme,
    };

    fn cols(names: &[&str]) -> StringVec {
        StringVec::from_vec(names.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    /// One row of the caller's own cells (spans, as the C example builds them).
    fn row(cells: &[&str]) -> ListViewRow {
        ListViewRow {
            cells: DomVec::from_vec(
                cells
                    .iter()
                    .map(|t| Dom::create_span_with_text(AzString::from(*t)))
                    .collect::<Vec<_>>(),
            ),
            height: None.into(),
        }
    }

    /// Four files sorted by their size, the fourth selected: rows 1 and 3 are
    /// the stripes, row 3 is the selection AND the list's Tab stop.
    fn files() -> ListView {
        ListView::create(cols(&["Name", "Size", "Type"]))
            .with_rows(ListViewRowVec::from_vec(vec![
                row(&["report.pdf", "2 MB", "PDF"]),
                row(&["photo.png", "4 MB", "Image"]),
                row(&["notes.txt", "1 KB", "Text"]),
                row(&["budget.xlsx", "340 KB", "Sheet"]),
            ]))
            .with_sorted_by(Some(1_usize).into())
            .with_selected_row(Some(3_usize).into())
    }

    fn header(dom: &Dom) -> &Dom {
        &dom.children.as_ref()[0]
    }

    fn rows(dom: &Dom) -> &[Dom] {
        dom.children.as_ref()[1].children.as_ref()
    }

    /// A column header's title (`<p>`).
    fn title(column: &Dom) -> &Dom {
        &column.children.as_ref()[0]
    }

    fn fill(node: &Dom, dark: bool) -> Option<ColorU> {
        tc::background(node, dark).as_ref().and_then(tc::bg_color)
    }

    fn layers(node: &Dom, dark: bool) -> Vec<StyleBackgroundContent> {
        tc::background(node, dark)
            .map(|p| tc::bg_layers(&p))
            .unwrap_or_default()
    }

    fn property(node: &Dom, ty: CssPropertyType) -> Option<CssProperty> {
        tc::resolve(node, ty, false, None)
    }

    fn right_rule(node: &Dom, dark: bool) -> Option<ColorU> {
        tc::resolve(node, CssPropertyType::BorderRightColor, dark, None)
            .as_ref()
            .and_then(tc::border_color)
    }

    /// Every colour one declaration paints: a fill (a gradient's every stop),
    /// an ink, a border, a shadow.
    fn colours(p: &CssProperty) -> Vec<ColorU> {
        let stop = |c: &ColorOrSystem| match c {
            ColorOrSystem::Color(c) => Some(*c),
            ColorOrSystem::System(_) => None,
        };
        match p {
            CssProperty::BackgroundContent(v) => v
                .get_property()
                .map(|layers| {
                    layers
                        .as_ref()
                        .iter()
                        .flat_map(|layer| match layer {
                            StyleBackgroundContent::Color(c) => vec![*c],
                            StyleBackgroundContent::LinearGradient(g) => {
                                g.stops.as_ref().iter().filter_map(|s| stop(&s.color)).collect()
                            }
                            StyleBackgroundContent::RadialGradient(g) => {
                                g.stops.as_ref().iter().filter_map(|s| stop(&s.color)).collect()
                            }
                            StyleBackgroundContent::ConicGradient(g) => {
                                g.stops.as_ref().iter().filter_map(|s| stop(&s.color)).collect()
                            }
                            _ => Vec::new(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            CssProperty::TextColor(v) => v.get_property().map(|c| c.inner).into_iter().collect(),
            p => tc::border_color(p)
                .or_else(|| tc::shadow_color_and_reach(p).map(|(c, _)| c))
                .into_iter()
                .collect(),
        }
    }

    /// Every colour a theme module declares: its palette by day and by night,
    /// and the faces and states it names on top of it.
    fn palette(theme: UiTheme) -> Vec<ColorU> {
        match theme {
            UiTheme::Flat => {
                use flat::*;
                vec![
                    LIGHT_PG, LIGHT_SUR, LIGHT_DESK, LIGHT_STRIP, LIGHT_TRACK, LIGHT_BD, LIGHT_BD2,
                    LIGHT_BD3, LIGHT_BD4, LIGHT_BD5, LIGHT_SEP, LIGHT_SEP2, LIGHT_INK, LIGHT_INK2,
                    LIGHT_INTRO, LIGHT_SOFT1, LIGHT_SOFT2, LIGHT_SOFT3, LIGHT_ICON, LIGHT_RT,
                    LIGHT_RB, LIGHT_HT, LIGHT_HB, LIGHT_PT, LIGHT_PB, LIGHT_FLD, LIGHT_FLD2,
                    LIGHT_DISBG, LIGHT_DISTX, LIGHT_QT, LIGHT_QT2, LIGHT_ACC, LIGHT_DEEP,
                    LIGHT_SOFT, LIGHT_GLOW, LIGHT_ON_ACC, DARK_PG, DARK_SUR, DARK_DESK, DARK_STRIP,
                    DARK_TRACK, DARK_BD, DARK_BD2, DARK_BD3, DARK_BD4, DARK_BD5, DARK_SEP,
                    DARK_SEP2, DARK_INK, DARK_INK2, DARK_INTRO, DARK_SOFT1, DARK_SOFT2, DARK_SOFT3,
                    DARK_ICON, DARK_RT, DARK_RB, DARK_HT, DARK_HB, DARK_PT, DARK_PB, DARK_FLD,
                    DARK_FLD2, DARK_DISBG, DARK_DISTX, DARK_QT, DARK_QT2, DARK_ACC, DARK_DEEP,
                    DARK_SOFT, DARK_GLOW, DARK_ON_ACC,
                    // Office 2010's faces.
                    LIGHT_HOVER_BORDER, DARK_HOVER_BORDER, LIGHT_PRESSED_BORDER,
                    DARK_PRESSED_BORDER, LIGHT_SELECTION_TOP, LIGHT_SELECTION_BOTTOM,
                    LIGHT_SELECTION_BORDER, DARK_SELECTION_TOP, DARK_SELECTION_BOTTOM,
                    DARK_SELECTION_BORDER,
                    // The row and field states.
                    LIGHT_ROW_HOVER, DARK_ROW_HOVER, FIELD_RING,
                    // The list's own header and row states.
                    LIGHT_LIST_HEADER_HOVER_LINE, LIGHT_LIST_HEADER_HOVER_TOP,
                    LIGHT_LIST_HEADER_HOVER_MID, LIGHT_LIST_HEADER_HOVER_BOTTOM,
                    LIGHT_LIST_HEADER_PRESSED, LIGHT_LIST_HEADER_PRESSED_BORDER,
                    LIGHT_LIST_HEADER_PRESSED_SHADOW, LIGHT_LIST_ROW_HOVER_BORDER,
                ]
            }
            UiTheme::Flora => {
                use flora::*;
                vec![
                    LIGHT_PG, LIGHT_SUR, LIGHT_DESK, LIGHT_STRIP, LIGHT_TRACK, LIGHT_BD, LIGHT_BD2,
                    LIGHT_BD3, LIGHT_BD4, LIGHT_BD5, LIGHT_SEP, LIGHT_SEP2, LIGHT_INK, LIGHT_INK2,
                    LIGHT_INTRO, LIGHT_SOFT1, LIGHT_SOFT2, LIGHT_SOFT3, LIGHT_ICON, LIGHT_RT,
                    LIGHT_RB, LIGHT_HT, LIGHT_HB, LIGHT_PT, LIGHT_PB, LIGHT_FLD, LIGHT_FLD2,
                    LIGHT_DISBG, LIGHT_DISTX, LIGHT_QT, LIGHT_QT2, LIGHT_ACC, LIGHT_DEEP,
                    LIGHT_SOFT, LIGHT_GLOW, LIGHT_ON_ACC, DARK_PG, DARK_SUR, DARK_DESK, DARK_STRIP,
                    DARK_TRACK, DARK_BD, DARK_BD2, DARK_BD3, DARK_BD4, DARK_BD5, DARK_SEP,
                    DARK_SEP2, DARK_INK, DARK_INK2, DARK_INTRO, DARK_SOFT1, DARK_SOFT2, DARK_SOFT3,
                    DARK_ICON, DARK_RT, DARK_RB, DARK_HT, DARK_HB, DARK_PT, DARK_PB, DARK_FLD,
                    DARK_FLD2, DARK_DISBG, DARK_DISTX, DARK_QT, DARK_QT2, DARK_ACC, DARK_DEEP,
                    DARK_SOFT, DARK_GLOW, DARK_ON_ACC,
                    // The metal rule.
                    TAB_METAL,
                ]
            }
        }
    }

    /// Whether `c` is one of `palette`'s colours: the same RGB (a wash is a
    /// palette colour at an alpha - a spin keeps the alpha too), or nothing
    /// at all (a transparent ring slot).
    fn is_one_of(c: ColorU, palette: &[ColorU]) -> bool {
        c.a == 0 || palette.iter().any(|p| p.r == c.r && p.g == c.g && p.b == c.b)
    }

    #[test]
    fn under_either_theme_an_empty_list_is_that_themes_field_in_its_ink() {
        for theme in BOTH {
            let (paper, ink) = match theme {
                UiTheme::Flat => ([flat::LIGHT_PG, flat::DARK_PG], [flat::LIGHT_INK, flat::DARK_INK]),
                UiTheme::Flora => (
                    [flora::LIGHT_FLD, flora::DARK_FLD],
                    [flora::LIGHT_INK, flora::DARK_INK],
                ),
            };
            under(theme, || {
                let dom = ListView::create(cols(&["Name"])).dom();
                for (i, dark) in [false, true].into_iter().enumerate() {
                    assert_eq!(
                        fill(&dom, dark),
                        Some(paper[i]),
                        "{theme:?} (dark: {dark}): the list's ground"
                    );
                    assert_eq!(
                        tc::text_color(&dom, dark),
                        Some(ink[i]),
                        "{theme:?} (dark: {dark}): the list's ink"
                    );
                }
            });
        }
    }

    #[test]
    fn every_colour_a_list_paints_is_one_of_its_themes() {
        for theme in BOTH {
            let palette = palette(theme);
            under(theme, || {
                let dom = files().dom();
                let mut foreign = Vec::new();
                for (path, node) in tc::nodes(&dom) {
                    for (p, _) in live_inline(node) {
                        for c in colours(&p) {
                            if !is_one_of(c, &palette) {
                                foreign.push(alloc::format!("{path} {:?}: {c:?}", p.get_type()));
                            }
                        }
                    }
                }
                assert!(
                    foreign.is_empty(),
                    "a list built for {theme:?} paints colours its theme does not have:\n  {}",
                    foreign.join("\n  ")
                );
            });
        }
    }

    #[test]
    fn a_selected_row_wears_its_themes_selection_by_day_and_by_night() {
        for theme in BOTH {
            under(theme, || {
                let dom = files().dom();
                let picked = &rows(&dom)[3];
                for dark in [false, true] {
                    match theme {
                        UiTheme::Flat => {
                            let (top, foot, rim) = if dark {
                                (
                                    flat::DARK_SELECTION_TOP,
                                    flat::DARK_SELECTION_BOTTOM,
                                    flat::DARK_SELECTION_BORDER,
                                )
                            } else {
                                (
                                    flat::LIGHT_SELECTION_TOP,
                                    flat::LIGHT_SELECTION_BOTTOM,
                                    flat::LIGHT_SELECTION_BORDER,
                                )
                            };
                            assert_eq!(
                                layers(picked, dark),
                                vec![decl::face(top, foot)],
                                "flat (dark: {dark}): Office 2010's light-blue selection face"
                            );
                            assert_eq!(
                                tc::border_top_color(picked, dark, None),
                                Some(rim),
                                "flat (dark: {dark}): in its blue rim"
                            );
                        }
                        UiTheme::Flora => {
                            let (wash, ink) = if dark {
                                (flora::DARK_ACC, flora::DARK_ON_ACC)
                            } else {
                                (flora::LIGHT_SOFT, flora::LIGHT_DEEP)
                            };
                            assert_eq!(
                                fill(picked, dark),
                                Some(wash),
                                "flora (dark: {dark}): the accent's wash (flora.css ::selection)"
                            );
                            assert_eq!(
                                tc::text_color(picked, dark),
                                Some(ink),
                                "flora (dark: {dark}): written in the wash's own ink"
                            );
                        }
                    }
                    assert_ne!(
                        layers(&rows(&dom)[2], dark),
                        layers(picked, dark),
                        "{theme:?} (dark: {dark}): an unselected row is not the selection"
                    );
                }
            });
        }
    }

    #[test]
    fn every_other_row_is_striped_in_its_themes_band() {
        for theme in BOTH {
            let band = match theme {
                UiTheme::Flat => [flat::LIGHT_SUR, flat::DARK_SUR],
                UiTheme::Flora => [flora::LIGHT_SUR, flora::DARK_SUR],
            };
            under(theme, || {
                let dom = files().dom();
                for (i, dark) in [false, true].into_iter().enumerate() {
                    assert_eq!(
                        fill(&rows(&dom)[1], dark),
                        Some(band[i]),
                        "{theme:?} (dark: {dark}): the second row is a stripe"
                    );
                    for plain in [0, 2] {
                        assert_eq!(
                            fill(&rows(&dom)[plain], dark),
                            None,
                            "{theme:?} (dark: {dark}): row {plain} lies on the list's ground"
                        );
                    }
                }
            });
        }
    }

    #[test]
    fn the_header_is_its_themes_band_and_flora_titles_the_columns_in_garamond_capitals() {
        for theme in BOTH {
            under(theme, || {
                let dom = files().dom();
                let band = header(&dom);
                let name = &band.children.as_ref()[0];
                let size = &band.children.as_ref()[1];
                for dark in [false, true] {
                    let (face, ink, rule, accent) = match (theme, dark) {
                        (UiTheme::Flat, false) => (
                            decl::face(flat::LIGHT_RT, flat::LIGHT_RB),
                            flat::LIGHT_INK2,
                            flat::LIGHT_SEP,
                            flat::LIGHT_ACC,
                        ),
                        (UiTheme::Flat, true) => (
                            decl::face(flat::DARK_RT, flat::DARK_RB),
                            flat::DARK_INK2,
                            flat::DARK_SEP,
                            flat::DARK_SOFT,
                        ),
                        (UiTheme::Flora, false) => (
                            flora::RAISED_FACE_LIGHT,
                            flora::LIGHT_INTRO,
                            flora::LIGHT_SEP,
                            flora::LIGHT_ACC,
                        ),
                        (UiTheme::Flora, true) => (
                            flora::RAISED_FACE_DARK,
                            flora::DARK_INTRO,
                            flora::DARK_SEP,
                            flora::DARK_GLOW,
                        ),
                    };
                    assert_eq!(layers(band, dark), vec![face], "{theme:?} (dark: {dark}): the band");
                    assert_eq!(
                        tc::text_color(title(name), dark),
                        Some(ink),
                        "{theme:?} (dark: {dark}): a column's title"
                    );
                    assert_eq!(
                        right_rule(name, dark),
                        Some(rule),
                        "{theme:?} (dark: {dark}): the separator between two columns"
                    );
                    assert_eq!(
                        tc::text_color(title(size), dark),
                        Some(accent),
                        "{theme:?} (dark: {dark}): the sorted column's title is the accent"
                    );
                    assert_eq!(
                        tc::text_color(&size.children.as_ref()[1], dark),
                        Some(accent),
                        "{theme:?} (dark: {dark}): and so is its sort arrow"
                    );
                }
                if theme == UiTheme::Flora {
                    assert_eq!(
                        property(title(name), CssPropertyType::FontFamily),
                        Some(CssProperty::const_font_family(flora::FONT_CAPS)),
                        "flora sets its chrome in EB Garamond"
                    );
                    assert_eq!(
                        property(title(name), CssPropertyType::TextTransform),
                        Some(CssProperty::TextTransform(StyleTextTransform::Uppercase.into())),
                        "in capitals"
                    );
                }
            });
        }
    }

    #[test]
    fn every_text_of_a_list_reads_at_least_two_to_one_in_both_themes_by_day_and_by_night() {
        let mut bad = Vec::new();
        for theme in BOTH {
            for mode in [DarkLightMode::Light, DarkLightMode::Dark] {
                bad.extend(crate::widgets::theme_contrast::findings_under(
                    theme,
                    mode,
                    "list_view",
                    || files().dom(),
                ));
                bad.extend(crate::widgets::theme_contrast::findings_under(
                    theme,
                    mode,
                    "list_view (empty)",
                    || ListView::create(cols(&["Name", "Size"])).dom(),
                ));
            }
        }
        assert!(bad.is_empty(), "{} text(s) do not read:\n  {}", bad.len(), bad.join("\n  "));
    }
}
