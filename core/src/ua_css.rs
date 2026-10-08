//! User-Agent Default Stylesheet for Azul
//!
//! This module provides the default CSS styling that browsers apply to HTML elements
//! before any author stylesheets are processed. It ensures consistent baseline behavior
//! across all applications.
//!
//! The user-agent stylesheet serves several critical functions:
//!
//! 1. **Prevents Layout Collapse**: Ensures root elements (`<html>`, `<body>`) have default
//!    dimensions so that percentage-based child sizing can work correctly.
//!
//! 2. **Establishes Display Types**: Defines the default `display` property for all HTML elements
//!    (e.g., `<div>` is `block`, `<span>` is `inline`).
//!
//! 3. **Provides Baseline Typography**: Sets reasonable defaults for font sizes, margins, and text
//!    styling for headings, paragraphs, and other text elements.
//!
//! 4. **Normalizes Browser Behavior**: Incorporates principles from normalize.css to provide
//!    consistent rendering across different platforms.
//!
//! # Licensing
//!
//! Based on principles from [normalize.css](https://github.com/necolas/normalize.css)
//! (MIT License, Copyright Nicolas Gallagher and Jonathan Neal).
//! This is NOT a direct copy but incorporates its principles and approach.
//!
//! # References
//!
//! - CSS 2.1 Specification: https://www.w3.org/TR/CSS21/
//! - HTML Living Standard: https://html.spec.whatwg.org/
//! - normalize.css: https://necolas.github.io/normalize.css/

use azul_css::{
    css::CssPropertyValue,
    dynamic_selector::{
        CssPropertyWithConditions, DynamicSelector, DynamicSelectorContext, OsCondition,
        ThemeCondition,
    },
    props::{
        basic::{
            font::{StyleFontFamily, StyleFontFamilyVec, StyleFontStyle, StyleFontWeight},
            pixel::PixelValue,
            ColorU, StyleFontSize,
        },
        layout::{
            dimensions::LayoutHeight,
            display::LayoutDisplay,
            fragmentation::{BreakInside, PageBreak},
            spacing::{
                LayoutMarginBottom, LayoutMarginLeft, LayoutMarginRight, LayoutMarginTop,
                LayoutPaddingBottom, LayoutPaddingInlineEnd, LayoutPaddingInlineStart,
                LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop,
            },
        },
        property::{CssProperty, CssPropertyType},
        style::{
            background::{StyleBackgroundContent, StyleBackgroundContentVec},
            border::{
                BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth,
                LayoutBorderRightWidth, LayoutBorderTopWidth, StyleBorderBottomColor,
                StyleBorderBottomStyle, StyleBorderLeftColor, StyleBorderLeftStyle,
                StyleBorderRightColor, StyleBorderRightStyle, StyleBorderTopColor,
                StyleBorderTopStyle,
            },
            content::CounterReset,
            effects::StyleCursor,
            lists::StyleListStyleType,
            scrollbar::{
                LayoutScrollbarWidth, ScrollbarColorCustom, ScrollbarFadeDelay,
                ScrollbarFadeDuration, ScrollbarVisibilityMode, StyleScrollbarColor,
            },
            text::{StyleTextColor, StyleTextDecoration},
            StyleTextAlign, StyleVerticalAlign,
        },
    },
    AzString,
};

use crate::dom::{AttributeType, NodeData, NodeType};

/// `white-space: pre` — the `<pre>` default (HTML rendering §15.3.3).
static WHITE_SPACE_PRE: CssProperty = CssProperty::WhiteSpace(CssPropertyValue::Exact(
    azul_css::props::style::StyleWhiteSpace::Pre,
));
/// `white-space: pre-wrap` — `<textarea>` (HTML rendering §15.5.14) and every
/// editing host (see [`get_ua_editing_host_property`]).
static WHITE_SPACE_PRE_WRAP: CssProperty = CssProperty::WhiteSpace(CssPropertyValue::Exact(
    azul_css::props::style::StyleWhiteSpace::PreWrap,
));
/// `overflow-wrap: break-word` — editable text wraps an overlong word instead
/// of running it out of the box (the textarea / contenteditable default in
/// every engine).
static OVERFLOW_WRAP_BREAK_WORD: CssProperty = CssProperty::OverflowWrap(
    CssPropertyValue::Exact(azul_css::props::style::StyleOverflowWrap::BreakWord),
);

/// 100% height
static HEIGHT_100_PERCENT: CssProperty = CssProperty::Height(CssPropertyValue::Exact(
    LayoutHeight::Px(PixelValue::const_percent(100)),
));

/// display: block
static DISPLAY_BLOCK: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Block));
static OVERFLOW_X_AUTO: CssProperty = CssProperty::OverflowX(CssPropertyValue::Exact(
    azul_css::props::layout::LayoutOverflow::Auto,
));
static OVERFLOW_Y_AUTO: CssProperty = CssProperty::OverflowY(CssPropertyValue::Exact(
    azul_css::props::layout::LayoutOverflow::Auto,
));

/// display: inline
static DISPLAY_INLINE: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Inline));

/// display: inline-block
static DISPLAY_INLINE_BLOCK: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::InlineBlock));

/// display: none
// <transient-window> UA defaults. `position: absolute; top: 100%` is the web
// fallback in full: a closed popup is display:none, an open one is a block
// anchored to the bottom edge of its parent. The NATIVE path does not read
// these — it positions a real surface from the anchor edge instead — but they
// are the same statement in two dialects, so an app that opts into neither
// gets a popup that opens below its anchor on every target.
static POSITION_ABSOLUTE: CssProperty = CssProperty::Position(CssPropertyValue::Exact(
    azul_css::props::layout::position::LayoutPosition::Absolute,
));
static TOP_100_PERCENT: CssProperty = CssProperty::Top(CssPropertyValue::Exact(
    azul_css::props::layout::position::LayoutTop {
        inner: PixelValue::const_percent(100),
    },
));
static DISPLAY_NONE: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::None));

/// break-before: page (the canonical `<pagebreak/>` element)
static BREAK_BEFORE_PAGE: CssProperty =
    CssProperty::BreakBefore(CssPropertyValue::Exact(PageBreak::Page));

/// display: table
static DISPLAY_TABLE: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Table));

/// display: table-row
static DISPLAY_TABLE_ROW: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::TableRow));

/// display: table-cell
static DISPLAY_TABLE_CELL: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::TableCell));

/// display: table-header-group
static DISPLAY_TABLE_HEADER_GROUP: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::TableHeaderGroup));

/// display: table-row-group
static DISPLAY_TABLE_ROW_GROUP: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::TableRowGroup));

/// display: table-footer-group
static DISPLAY_TABLE_FOOTER_GROUP: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::TableFooterGroup));

/// display: table-caption
static DISPLAY_TABLE_CAPTION: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::TableCaption));

/// display: table-column-group
static DISPLAY_TABLE_COLUMN_GROUP: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::TableColumnGroup));

/// display: table-column
static DISPLAY_TABLE_COLUMN: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::TableColumn));

/// display: list-item
static DISPLAY_LIST_ITEM: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::ListItem));

/// cursor: pointer (for clickable elements like buttons, links)
static CURSOR_POINTER: CssProperty =
    CssProperty::Cursor(CssPropertyValue::Exact(StyleCursor::Pointer));

/// cursor: text (for selectable text elements)
static CURSOR_TEXT: CssProperty = CssProperty::Cursor(CssPropertyValue::Exact(StyleCursor::Text));

/// margin-top: 0
static MARGIN_TOP_ZERO: CssProperty =
    CssProperty::MarginTop(CssPropertyValue::Exact(LayoutMarginTop {
        inner: PixelValue::const_px(0),
    }));

/// margin-bottom: 0
static MARGIN_BOTTOM_ZERO: CssProperty =
    CssProperty::MarginBottom(CssPropertyValue::Exact(LayoutMarginBottom {
        inner: PixelValue::const_px(0),
    }));

/// margin-left: 0
static MARGIN_LEFT_ZERO: CssProperty =
    CssProperty::MarginLeft(CssPropertyValue::Exact(LayoutMarginLeft {
        inner: PixelValue::const_px(0),
    }));

/// margin-right: 0
static MARGIN_RIGHT_ZERO: CssProperty =
    CssProperty::MarginRight(CssPropertyValue::Exact(LayoutMarginRight {
        inner: PixelValue::const_px(0),
    }));

// Chrome User-Agent Stylesheet: body { margin: 8px; }
/// margin-top: 8px (Chrome UA default for body)
static MARGIN_TOP_8PX: CssProperty =
    CssProperty::MarginTop(CssPropertyValue::Exact(LayoutMarginTop {
        inner: PixelValue::const_px(8),
    }));

/// margin-bottom: 8px (Chrome UA default for body)
static MARGIN_BOTTOM_8PX: CssProperty =
    CssProperty::MarginBottom(CssPropertyValue::Exact(LayoutMarginBottom {
        inner: PixelValue::const_px(8),
    }));

/// margin-left: 8px (Chrome UA default for body)
static MARGIN_LEFT_8PX: CssProperty =
    CssProperty::MarginLeft(CssPropertyValue::Exact(LayoutMarginLeft {
        inner: PixelValue::const_px(8),
    }));

/// margin-right: 8px (Chrome UA default for body)
static MARGIN_RIGHT_8PX: CssProperty =
    CssProperty::MarginRight(CssPropertyValue::Exact(LayoutMarginRight {
        inner: PixelValue::const_px(8),
    }));

/// font-size: 2em (for H1)
static FONT_SIZE_2EM: CssProperty = CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
    inner: PixelValue::const_em(2),
}));

/// font-size: 1.5em (for H2)
static FONT_SIZE_1_5EM: CssProperty =
    CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
        inner: PixelValue::const_em_fractional(1, 5),
    }));

/// font-size: 1.17em (for H3)
static FONT_SIZE_1_17EM: CssProperty =
    CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
        inner: PixelValue::const_em_fractional(1, 17),
    }));

/// font-size: 1em (for H4)
static FONT_SIZE_1EM: CssProperty = CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
    inner: PixelValue::const_em(1),
}));

/// font-size: 0.83em (for H5)
static FONT_SIZE_0_83EM: CssProperty =
    CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
        inner: PixelValue::const_em_fractional(0, 83),
    }));

/// font-size: 0.67em (for H6)
static FONT_SIZE_0_67EM: CssProperty =
    CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
        inner: PixelValue::const_em_fractional(0, 67),
    }));

/// margin-top: 1em (for P)
static MARGIN_TOP_1EM: CssProperty =
    CssProperty::MarginTop(CssPropertyValue::Exact(LayoutMarginTop {
        inner: PixelValue::const_em(1),
    }));

/// margin-bottom: 1em (for P)
static MARGIN_BOTTOM_1EM: CssProperty =
    CssProperty::MarginBottom(CssPropertyValue::Exact(LayoutMarginBottom {
        inner: PixelValue::const_em(1),
    }));

/// margin-top: 0.67em (for H1)
static MARGIN_TOP_0_67EM: CssProperty =
    CssProperty::MarginTop(CssPropertyValue::Exact(LayoutMarginTop {
        inner: PixelValue::const_em_fractional(0, 67),
    }));

/// margin-bottom: 0.67em (for H1)
static MARGIN_BOTTOM_0_67EM: CssProperty =
    CssProperty::MarginBottom(CssPropertyValue::Exact(LayoutMarginBottom {
        inner: PixelValue::const_em_fractional(0, 67),
    }));

/// margin-top: 0.83em (for H2)
static MARGIN_TOP_0_83EM: CssProperty =
    CssProperty::MarginTop(CssPropertyValue::Exact(LayoutMarginTop {
        inner: PixelValue::const_em_fractional(0, 83),
    }));

/// margin-bottom: 0.83em (for H2)
static MARGIN_BOTTOM_0_83EM: CssProperty =
    CssProperty::MarginBottom(CssPropertyValue::Exact(LayoutMarginBottom {
        inner: PixelValue::const_em_fractional(0, 83),
    }));

/// margin-top: 1.33em (for H4)
static MARGIN_TOP_1_33EM: CssProperty =
    CssProperty::MarginTop(CssPropertyValue::Exact(LayoutMarginTop {
        inner: PixelValue::const_em_fractional(1, 33),
    }));

/// margin-bottom: 1.33em (for H4)
static MARGIN_BOTTOM_1_33EM: CssProperty =
    CssProperty::MarginBottom(CssPropertyValue::Exact(LayoutMarginBottom {
        inner: PixelValue::const_em_fractional(1, 33),
    }));

/// margin-top: 1.67em (for H5)
static MARGIN_TOP_1_67EM: CssProperty =
    CssProperty::MarginTop(CssPropertyValue::Exact(LayoutMarginTop {
        inner: PixelValue::const_em_fractional(1, 67),
    }));

/// margin-bottom: 1.67em (for H5)
static MARGIN_BOTTOM_1_67EM: CssProperty =
    CssProperty::MarginBottom(CssPropertyValue::Exact(LayoutMarginBottom {
        inner: PixelValue::const_em_fractional(1, 67),
    }));

/// margin-top: 2.33em (for H6)
static MARGIN_TOP_2_33EM: CssProperty =
    CssProperty::MarginTop(CssPropertyValue::Exact(LayoutMarginTop {
        inner: PixelValue::const_em_fractional(2, 33),
    }));

/// margin-bottom: 2.33em (for H6)
static MARGIN_BOTTOM_2_33EM: CssProperty =
    CssProperty::MarginBottom(CssPropertyValue::Exact(LayoutMarginBottom {
        inner: PixelValue::const_em_fractional(2, 33),
    }));

/// font-weight: bold (for headings)
static FONT_WEIGHT_BOLD: CssProperty =
    CssProperty::FontWeight(CssPropertyValue::Exact(StyleFontWeight::Bold));

/// font-weight: bolder
static FONT_WEIGHT_BOLDER: CssProperty =
    CssProperty::FontWeight(CssPropertyValue::Exact(StyleFontWeight::Bolder));

// Table cell padding - Chrome UA CSS default: 1px
static PADDING_TOP_1PX: CssProperty =
    CssProperty::PaddingTop(CssPropertyValue::Exact(LayoutPaddingTop {
        inner: PixelValue::const_px(1),
    }));

static PADDING_BOTTOM_1PX: CssProperty =
    CssProperty::PaddingBottom(CssPropertyValue::Exact(LayoutPaddingBottom {
        inner: PixelValue::const_px(1),
    }));

static PADDING_LEFT_1PX: CssProperty =
    CssProperty::PaddingLeft(CssPropertyValue::Exact(LayoutPaddingLeft {
        inner: PixelValue::const_px(1),
    }));

static PADDING_RIGHT_1PX: CssProperty =
    CssProperty::PaddingRight(CssPropertyValue::Exact(LayoutPaddingRight {
        inner: PixelValue::const_px(1),
    }));

/// text-align: center (for th elements)
static TEXT_ALIGN_CENTER: CssProperty =
    CssProperty::TextAlign(CssPropertyValue::Exact(StyleTextAlign::Center));

/// vertical-align: middle (for table elements)
static VERTICAL_ALIGN_MIDDLE: CssProperty =
    CssProperty::VerticalAlign(CssPropertyValue::Exact(StyleVerticalAlign::Middle));

// HTML rendering 15.3.8: `table { box-sizing: border-box; border-spacing:
// 2px; border-color: gray }` - a `<table width="600" border="1">` is 600px
// wide outside, and the cells of an unstyled table sit 2px apart.

/// box-sizing: border-box (for table)
static BOX_SIZING_BORDER_BOX: CssProperty = CssProperty::BoxSizing(CssPropertyValue::Exact(
    azul_css::props::layout::dimensions::LayoutBoxSizing::BorderBox,
));

/// border-spacing: 2px (for table)
static BORDER_SPACING_2PX: CssProperty = CssProperty::BorderSpacing(CssPropertyValue::Exact(
    azul_css::props::layout::table::LayoutBorderSpacing {
        horizontal: PixelValue::const_px(2),
        vertical: PixelValue::const_px(2),
    },
));

/// The table's border colour: gray (a `<table border>`'s outset frame).
const TABLE_BORDER_GRAY: ColorU = ColorU {
    r: 128,
    g: 128,
    b: 128,
    a: 255,
};
static TABLE_BORDER_TOP_COLOR: CssProperty =
    CssProperty::BorderTopColor(CssPropertyValue::Exact(StyleBorderTopColor {
        inner: TABLE_BORDER_GRAY,
    }));
static TABLE_BORDER_RIGHT_COLOR: CssProperty =
    CssProperty::BorderRightColor(CssPropertyValue::Exact(StyleBorderRightColor {
        inner: TABLE_BORDER_GRAY,
    }));
static TABLE_BORDER_BOTTOM_COLOR: CssProperty =
    CssProperty::BorderBottomColor(CssPropertyValue::Exact(StyleBorderBottomColor {
        inner: TABLE_BORDER_GRAY,
    }));
static TABLE_BORDER_LEFT_COLOR: CssProperty =
    CssProperty::BorderLeftColor(CssPropertyValue::Exact(StyleBorderLeftColor {
        inner: TABLE_BORDER_GRAY,
    }));

/// list-style-type: disc (default for <ul>)
static LIST_STYLE_TYPE_DISC: CssProperty =
    CssProperty::ListStyleType(CssPropertyValue::Exact(StyleListStyleType::Disc));

/// list-style-type: decimal (default for <ol>)
static LIST_STYLE_TYPE_DECIMAL: CssProperty =
    CssProperty::ListStyleType(CssPropertyValue::Exact(StyleListStyleType::Decimal));

// --- HR Element Defaults ---
// HTML Living Standard 15.3.11 "The hr element" (Chrome draws exactly this):
//   hr { color: gray; border-style: inset; border-width: 1px;
//        margin-block: 0.5em; margin-inline: auto; overflow: hidden; }
// A 2px rule - the top and the bottom border around no content - as wide as
// its block (`width` auto), centred when an author narrows it. `overflow:
// hidden` is left out: the box is empty, and a clip would give every rule a
// clip of its own. (It was the top border alone at `width: 100%`: 1px short
// of Chrome, and a rule with a side margin overflowed its block by it.)

/// margin-top: 0.5em (for hr)
static MARGIN_TOP_0_5EM: CssProperty =
    CssProperty::MarginTop(CssPropertyValue::Exact(LayoutMarginTop {
        inner: PixelValue::const_em_fractional(0, 5),
    }));

/// margin-bottom: 0.5em (for hr)
static MARGIN_BOTTOM_0_5EM: CssProperty =
    CssProperty::MarginBottom(CssPropertyValue::Exact(LayoutMarginBottom {
        inner: PixelValue::const_em_fractional(0, 5),
    }));

/// margin-left: auto (for hr - `margin-inline: auto`)
static MARGIN_LEFT_AUTO: CssProperty = CssProperty::MarginLeft(CssPropertyValue::Auto);

/// margin-right: auto (for hr - `margin-inline: auto`)
static MARGIN_RIGHT_AUTO: CssProperty = CssProperty::MarginRight(CssPropertyValue::Auto);

/// border-*-style: inset (for hr)
static BORDER_TOP_STYLE_INSET: CssProperty =
    CssProperty::BorderTopStyle(CssPropertyValue::Exact(StyleBorderTopStyle {
        inner: BorderStyle::Inset,
    }));
static BORDER_BOTTOM_STYLE_INSET: CssProperty =
    CssProperty::BorderBottomStyle(CssPropertyValue::Exact(StyleBorderBottomStyle {
        inner: BorderStyle::Inset,
    }));
static BORDER_LEFT_STYLE_INSET: CssProperty =
    CssProperty::BorderLeftStyle(CssPropertyValue::Exact(StyleBorderLeftStyle {
        inner: BorderStyle::Inset,
    }));
static BORDER_RIGHT_STYLE_INSET: CssProperty =
    CssProperty::BorderRightStyle(CssPropertyValue::Exact(StyleBorderRightStyle {
        inner: BorderStyle::Inset,
    }));

/// The hr's `color: gray` - its borders' colour (`currentcolor`).
const HR_GRAY: ColorU = ColorU {
    r: 128,
    g: 128,
    b: 128,
    a: 255,
};

/// The hr's colour on a DARK window: a subtle divider, like the platforms'
/// own separators on dark (#5a5a5a), where the light rule's mid grey would
/// read as a bright bar.
const HR_GRAY_DARK: ColorU = ColorU {
    r: 90,
    g: 90,
    b: 90,
    a: 255,
};

/// border-*-color: gray (for hr)
static BORDER_TOP_COLOR_GRAY: CssProperty =
    CssProperty::BorderTopColor(CssPropertyValue::Exact(StyleBorderTopColor {
        inner: HR_GRAY,
    }));
static BORDER_BOTTOM_COLOR_GRAY: CssProperty =
    CssProperty::BorderBottomColor(CssPropertyValue::Exact(StyleBorderBottomColor {
        inner: HR_GRAY,
    }));
static BORDER_LEFT_COLOR_GRAY: CssProperty =
    CssProperty::BorderLeftColor(CssPropertyValue::Exact(StyleBorderLeftColor {
        inner: HR_GRAY,
    }));
static BORDER_RIGHT_COLOR_GRAY: CssProperty =
    CssProperty::BorderRightColor(CssPropertyValue::Exact(StyleBorderRightColor {
        inner: HR_GRAY,
    }));

/// border-*-color for hr on a DARK window (`HR_GRAY_DARK`)
static BORDER_TOP_COLOR_GRAY_DARK: CssProperty =
    CssProperty::BorderTopColor(CssPropertyValue::Exact(StyleBorderTopColor {
        inner: HR_GRAY_DARK,
    }));
static BORDER_BOTTOM_COLOR_GRAY_DARK: CssProperty =
    CssProperty::BorderBottomColor(CssPropertyValue::Exact(StyleBorderBottomColor {
        inner: HR_GRAY_DARK,
    }));
static BORDER_LEFT_COLOR_GRAY_DARK: CssProperty =
    CssProperty::BorderLeftColor(CssPropertyValue::Exact(StyleBorderLeftColor {
        inner: HR_GRAY_DARK,
    }));
static BORDER_RIGHT_COLOR_GRAY_DARK: CssProperty =
    CssProperty::BorderRightColor(CssPropertyValue::Exact(StyleBorderRightColor {
        inner: HR_GRAY_DARK,
    }));
/// height: 0 (for hr - the line comes from the border, not height)
static HEIGHT_ZERO: CssProperty = CssProperty::Height(CssPropertyValue::Exact(LayoutHeight::Px(
    PixelValue::const_px(0),
)));

/// counter-reset: list-item 0 (default for <ul>, <ol>)
/// Per CSS Lists Module Level 3, list containers automatically reset the list-item counter
static COUNTER_RESET_LIST_ITEM: CssProperty =
    CssProperty::CounterReset(CssPropertyValue::Exact(CounterReset::list_item()));

// CSS Fragmentation (Page Breaking) Properties
//
// Per CSS Fragmentation Level 3 and paged media best practices,
// certain elements should avoid page breaks inside them

/// break-inside: avoid
/// Used for elements that should not be split across page boundaries
/// Applied to: h1-h6, table, thead, tbody, tfoot, figure, figcaption
static BREAK_INSIDE_AVOID: CssProperty = CssProperty::break_inside(BreakInside::Avoid);

/// break-after: avoid
/// Avoids a page break after the element (useful for headings)
static BREAK_AFTER_AVOID: CssProperty = CssProperty::break_after(PageBreak::Avoid);

/// padding-inline-start: 40px (default for <li>)
///
/// Creates space for list markers in the inline-start direction (left in LTR, right in RTL)
/// padding-inline-start: 40px for list items per CSS Lists Module Level 3
/// Applied to <li> items to create gutter space for `::marker` pseudo-elements
///
/// NOTE: This should be on the list items, not the container, because:
///
/// 1. `::marker` pseudo-elements are children of <li>, not <ul>/<ol>
/// 2. The marker needs to be positioned relative to the list item's content box
/// 3. Padding on <li> creates space between the marker and the text content TODO: Change to
///    `PaddingInlineStart` once logical property resolution is implemented
static PADDING_INLINE_START_40PX: CssProperty =
    CssProperty::PaddingLeft(CssPropertyValue::Exact(LayoutPaddingLeft {
        inner: PixelValue::const_px(40),
    }));

/// Text decoration: underline - used for <a> and <u> elements
static TEXT_DECORATION_UNDERLINE: CssProperty =
    CssProperty::TextDecoration(CssPropertyValue::Exact(StyleTextDecoration::Underline));

// --- Phrasing and flow content (HTML Living Standard, rendering 15.3.3 / 15.3.4) ---
//
// What mail HTML leans on: `<em>` is italic, `<s>` struck through, `<code>`
// monospace, a `<blockquote>` indented, a link blue. The XML loaders read the
// legacy `<strike>` as `s` and `<tt>` as `code` (`tag_to_node_type`).

/// `address, cite, dfn, em, i, var { font-style: italic }`
static FONT_STYLE_ITALIC: CssProperty =
    CssProperty::FontStyle(CssPropertyValue::Exact(StyleFontStyle::Italic));

/// `del, s, strike { text-decoration: line-through }`
static TEXT_DECORATION_LINE_THROUGH: CssProperty =
    CssProperty::TextDecoration(CssPropertyValue::Exact(StyleTextDecoration::LineThrough));

/// The generic `monospace` family, resolved like an author's `font-family: monospace`.
const MONOSPACE_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(
    AzString::from_const_str("monospace"),
)];

/// `code, kbd, pre, samp, tt { font-family: monospace }`
static FONT_FAMILY_MONOSPACE: CssProperty = CssProperty::FontFamily(CssPropertyValue::Exact(
    StyleFontFamilyVec::from_const_slice(MONOSPACE_FAMILIES),
));

/// `blockquote, figure { margin-inline: 40px }`, `dd { margin-inline-start: 40px }`
/// (the left edge in LTR, as `PADDING_INLINE_START_40PX` does for lists).
static MARGIN_LEFT_40PX: CssProperty =
    CssProperty::MarginLeft(CssPropertyValue::Exact(LayoutMarginLeft {
        inner: PixelValue::const_px(40),
    }));

/// `blockquote, figure { margin-inline: 40px }` (the right edge).
static MARGIN_RIGHT_40PX: CssProperty =
    CssProperty::MarginRight(CssPropertyValue::Exact(LayoutMarginRight {
        inner: PixelValue::const_px(40),
    }));

/// `small, sub, sup { font-size: smaller }`. CSS Fonts leaves the ratio to
/// the UA; 0.83em is the step the heading table uses (`h5`) and the browsers'
/// 1/1.2 - the value an author's `font-size: smaller` parses to, so the two
/// are the same size.
static FONT_SIZE_SMALLER: CssProperty =
    CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
        inner: azul_css::props::basic::font::FONT_SIZE_SMALLER,
    }));

/// `big { font-size: larger }`: 1.2em, the inverse step.
static FONT_SIZE_LARGER: CssProperty =
    CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
        inner: azul_css::props::basic::font::FONT_SIZE_LARGER,
    }));

/// `sub { vertical-align: sub }`
static VERTICAL_ALIGN_SUB: CssProperty =
    CssProperty::VerticalAlign(CssPropertyValue::Exact(StyleVerticalAlign::Sub));

/// `sup { vertical-align: super }`
static VERTICAL_ALIGN_SUPER: CssProperty =
    CssProperty::VerticalAlign(CssPropertyValue::Exact(StyleVerticalAlign::Superscript));

/// `mark { background: yellow }`
const MARK_BACKGROUND_LAYERS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ColorU {
        r: 255,
        g: 255,
        b: 0,
        a: 255,
    })];
static MARK_BACKGROUND: CssProperty = CssProperty::BackgroundContent(CssPropertyValue::Exact(
    StyleBackgroundContentVec::from_const_slice(MARK_BACKGROUND_LAYERS),
));

/// `mark { color: black }` - in either mode: the highlight stays yellow.
static MARK_TEXT_COLOR: CssProperty =
    CssProperty::TextColor(CssPropertyValue::Exact(StyleTextColor {
        inner: ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
    }));

/// `:link { color: #0000EE }` - a link (`<a href>`, see [`is_link`]).
static LINK_COLOR: CssProperty = CssProperty::TextColor(CssPropertyValue::Exact(StyleTextColor {
    inner: ColorU {
        r: 0x00,
        g: 0x00,
        b: 0xee,
        a: 255,
    },
}));

/// The link colour in the DARK mode: #9E9EFF, the browsers' dark
/// `LinkText` - #0000EE is unreadable on a dark background.
static LINK_COLOR_DARK: CssProperty =
    CssProperty::TextColor(CssPropertyValue::Exact(StyleTextColor {
        inner: ColorU {
            r: 0x9e,
            g: 0x9e,
            b: 0xff,
            a: 255,
        },
    }));

// --- Button Element Defaults ---
// Per browser UA CSS, <button> has padding, border, and a system font size.
// These ensure a button is visible even without author CSS.

/// font-size: 13px (standard button font size on macOS/Linux)
static FONT_SIZE_13PX: CssProperty =
    CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
        inner: PixelValue::const_px(13),
    }));

/// padding-top: 5px (button)
static PADDING_TOP_5PX: CssProperty =
    CssProperty::PaddingTop(CssPropertyValue::Exact(LayoutPaddingTop {
        inner: PixelValue::const_px(5),
    }));

/// padding-bottom: 5px (button)
static PADDING_BOTTOM_5PX: CssProperty =
    CssProperty::PaddingBottom(CssPropertyValue::Exact(LayoutPaddingBottom {
        inner: PixelValue::const_px(5),
    }));

/// padding-left: 10px (button)
static PADDING_LEFT_10PX: CssProperty =
    CssProperty::PaddingLeft(CssPropertyValue::Exact(LayoutPaddingLeft {
        inner: PixelValue::const_px(10),
    }));

/// padding-right: 10px (button)
static PADDING_RIGHT_10PX: CssProperty =
    CssProperty::PaddingRight(CssPropertyValue::Exact(LayoutPaddingRight {
        inner: PixelValue::const_px(10),
    }));

/// Border color for button: #c8c8c8 (light gray)
static BUTTON_BORDER_COLOR: ColorU = ColorU {
    r: 200,
    g: 200,
    b: 200,
    a: 255,
};

/// Border color for a native button on a DARK window: #5a5a5a. The light
/// rule's #c8c8c8 is chosen for a white surface and glares on a dark one.
static BUTTON_BORDER_COLOR_DARK: ColorU = ColorU {
    r: 90,
    g: 90,
    b: 90,
    a: 255,
};
static BUTTON_BORDER_TOP_COLOR_DARK: CssProperty =
    CssProperty::BorderTopColor(CssPropertyValue::Exact(StyleBorderTopColor {
        inner: BUTTON_BORDER_COLOR_DARK,
    }));
static BUTTON_BORDER_BOTTOM_COLOR_DARK: CssProperty =
    CssProperty::BorderBottomColor(CssPropertyValue::Exact(StyleBorderBottomColor {
        inner: BUTTON_BORDER_COLOR_DARK,
    }));
static BUTTON_BORDER_LEFT_COLOR_DARK: CssProperty =
    CssProperty::BorderLeftColor(CssPropertyValue::Exact(StyleBorderLeftColor {
        inner: BUTTON_BORDER_COLOR_DARK,
    }));
static BUTTON_BORDER_RIGHT_COLOR_DARK: CssProperty =
    CssProperty::BorderRightColor(CssPropertyValue::Exact(StyleBorderRightColor {
        inner: BUTTON_BORDER_COLOR_DARK,
    }));
static BUTTON_BORDER_TOP_COLOR: CssProperty =
    CssProperty::BorderTopColor(CssPropertyValue::Exact(StyleBorderTopColor {
        inner: BUTTON_BORDER_COLOR,
    }));
static BUTTON_BORDER_BOTTOM_COLOR: CssProperty =
    CssProperty::BorderBottomColor(CssPropertyValue::Exact(StyleBorderBottomColor {
        inner: BUTTON_BORDER_COLOR,
    }));
static BUTTON_BORDER_LEFT_COLOR: CssProperty =
    CssProperty::BorderLeftColor(CssPropertyValue::Exact(StyleBorderLeftColor {
        inner: BUTTON_BORDER_COLOR,
    }));
static BUTTON_BORDER_RIGHT_COLOR: CssProperty =
    CssProperty::BorderRightColor(CssPropertyValue::Exact(StyleBorderRightColor {
        inner: BUTTON_BORDER_COLOR,
    }));

static BUTTON_BORDER_TOP_STYLE: CssProperty =
    CssProperty::BorderTopStyle(CssPropertyValue::Exact(StyleBorderTopStyle {
        inner: BorderStyle::Solid,
    }));
static BUTTON_BORDER_BOTTOM_STYLE: CssProperty =
    CssProperty::BorderBottomStyle(CssPropertyValue::Exact(StyleBorderBottomStyle {
        inner: BorderStyle::Solid,
    }));
static BUTTON_BORDER_LEFT_STYLE: CssProperty =
    CssProperty::BorderLeftStyle(CssPropertyValue::Exact(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    }));
static BUTTON_BORDER_RIGHT_STYLE: CssProperty =
    CssProperty::BorderRightStyle(CssPropertyValue::Exact(StyleBorderRightStyle {
        inner: BorderStyle::Solid,
    }));

/// border-*-width: 1px - the button's border and the hr's rule (one static
/// per side; the hr's top and the button's four were twins).
static BORDER_TOP_WIDTH_1PX: CssProperty =
    CssProperty::BorderTopWidth(CssPropertyValue::Exact(LayoutBorderTopWidth {
        inner: PixelValue::const_px(1),
    }));
static BORDER_BOTTOM_WIDTH_1PX: CssProperty =
    CssProperty::BorderBottomWidth(CssPropertyValue::Exact(LayoutBorderBottomWidth {
        inner: PixelValue::const_px(1),
    }));
static BORDER_LEFT_WIDTH_1PX: CssProperty =
    CssProperty::BorderLeftWidth(CssPropertyValue::Exact(LayoutBorderLeftWidth {
        inner: PixelValue::const_px(1),
    }));
static BORDER_RIGHT_WIDTH_1PX: CssProperty =
    CssProperty::BorderRightWidth(CssPropertyValue::Exact(LayoutBorderRightWidth {
        inner: PixelValue::const_px(1),
    }));

/// Returns the default user-agent CSS property value for a given node type and property.
///
/// This function provides the baseline styling that should be applied before any author
/// styles. It ensures that elements have sensible defaults that prevent layout issues.
///
/// # Arguments
///
/// * `node_type` - The type of DOM node (e.g., `Body`, `H1`, `Div`)
/// * `property_type` - The specific CSS property to query (e.g., `Width`, `Display`)
///
/// # Returns
///
/// `Some(CssProperty)` if a default value is defined for this combination, otherwise `None`.
// Exhaustive (node-type, property-type) → default-value lookup table: many
// element types share a default (e.g. all block elements → DISPLAY_BLOCK). One
// arm per (NT, PT) case is intentional for readability; merging into giant
// or-patterns would collapse the UA stylesheet table.
#[allow(clippy::match_same_arms)]
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
#[must_use]
pub fn get_ua_property(
    node_type: &NodeType,
    property_type: CssPropertyType,
) -> Option<&'static CssProperty> {
    use CssPropertyType as PT;
    use NodeType as NT;

    match (node_type, property_type) {
        // Body Element - CRITICAL for preventing layout collapse
        (NT::Body, PT::Display) => Some(&DISPLAY_BLOCK),
        // NOTE: Body does NOT have width: 100% in standard UA CSS - it inherits from ICB
        // (NT::Body, PT::Height) => Some(&HEIGHT_100_PERCENT),
        (NT::Body, PT::MarginTop) => Some(&MARGIN_TOP_8PX),
        (NT::Body, PT::MarginBottom) => Some(&MARGIN_BOTTOM_8PX),
        (NT::Body, PT::MarginLeft) => Some(&MARGIN_LEFT_8PX),
        (NT::Body, PT::MarginRight) => Some(&MARGIN_RIGHT_8PX),

        // Block-level Elements
        // NOTE: Do NOT set width: 100% here! Block elements have width: auto by default
        // in CSS spec. width: auto for blocks means "fill available width" but it's NOT
        // the same as width: 100%. The difference is critical for flexbox: width: auto
        // allows flex-grow/flex-shrink to control sizing, while width: 100% prevents it.
        (NT::Div, PT::Display) => Some(&DISPLAY_BLOCK),
        // <transient-window>: see the statics' comment. `display` is resolved
        // per node from `TransientWindowConfig::open` in the layout tree, so
        // the UA value here is only the OPEN case's block-ness; a closed one is
        // cut out before display is consulted.
        (NT::TransientWindow(_), PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::TransientWindow(_), PT::Position) => Some(&POSITION_ABSOLUTE),
        (NT::TransientWindow(_), PT::Top) => Some(&TOP_100_PERCENT),
        (NT::P, PT::Display) => Some(&DISPLAY_BLOCK),
        // REMOVED - blocks have width: auto by default
        // (NT::Div, PT::Width) => Some(&WIDTH_100_PERCENT),
        // REMOVED - blocks have width: auto by default
        // (NT::P, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::P, PT::MarginTop) => Some(&MARGIN_TOP_1EM),
        (NT::P, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1EM),
        (NT::Main, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Header, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Footer, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Section, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Article, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Aside, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Nav, PT::Display) => Some(&DISPLAY_BLOCK),

        // Headings - Chrome UA CSS values
        // Per CSS Fragmentation Level 3: headings should avoid page breaks inside
        // and after them (to keep heading with following content)
        (NT::H1, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::H1, PT::FontSize) => Some(&FONT_SIZE_2EM),
        (NT::H1, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::H1, PT::MarginTop) => Some(&MARGIN_TOP_0_67EM),
        (NT::H1, PT::MarginBottom) => Some(&MARGIN_BOTTOM_0_67EM),
        (NT::H1, PT::BreakInside) => Some(&BREAK_INSIDE_AVOID),
        (NT::H1, PT::BreakAfter) => Some(&BREAK_AFTER_AVOID),

        (NT::H2, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::H2, PT::FontSize) => Some(&FONT_SIZE_1_5EM),
        (NT::H2, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::H2, PT::MarginTop) => Some(&MARGIN_TOP_0_83EM),
        (NT::H2, PT::MarginBottom) => Some(&MARGIN_BOTTOM_0_83EM),
        (NT::H2, PT::BreakInside) => Some(&BREAK_INSIDE_AVOID),
        (NT::H2, PT::BreakAfter) => Some(&BREAK_AFTER_AVOID),

        (NT::H3, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::H3, PT::FontSize) => Some(&FONT_SIZE_1_17EM),
        (NT::H3, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::H3, PT::MarginTop) => Some(&MARGIN_TOP_1EM),
        (NT::H3, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1EM),
        (NT::H3, PT::BreakInside) => Some(&BREAK_INSIDE_AVOID),
        (NT::H3, PT::BreakAfter) => Some(&BREAK_AFTER_AVOID),

        (NT::H4, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::H4, PT::FontSize) => Some(&FONT_SIZE_1EM),
        (NT::H4, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::H4, PT::MarginTop) => Some(&MARGIN_TOP_1_33EM),
        (NT::H4, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1_33EM),
        (NT::H4, PT::BreakInside) => Some(&BREAK_INSIDE_AVOID),
        (NT::H4, PT::BreakAfter) => Some(&BREAK_AFTER_AVOID),

        (NT::H5, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::H5, PT::FontSize) => Some(&FONT_SIZE_0_83EM),
        (NT::H5, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::H5, PT::MarginTop) => Some(&MARGIN_TOP_1_67EM),
        (NT::H5, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1_67EM),
        (NT::H5, PT::BreakInside) => Some(&BREAK_INSIDE_AVOID),
        (NT::H5, PT::BreakAfter) => Some(&BREAK_AFTER_AVOID),

        (NT::H6, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::H6, PT::FontSize) => Some(&FONT_SIZE_0_67EM),
        (NT::H6, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::H6, PT::MarginTop) => Some(&MARGIN_TOP_2_33EM),
        (NT::H6, PT::MarginBottom) => Some(&MARGIN_BOTTOM_2_33EM),
        (NT::H6, PT::BreakInside) => Some(&BREAK_INSIDE_AVOID),
        (NT::H6, PT::BreakAfter) => Some(&BREAK_AFTER_AVOID),

        // Lists - padding on container creates gutter for markers
        (NT::Ul, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Ul, PT::ListStyleType) => Some(&LIST_STYLE_TYPE_DISC),
        (NT::Ul, PT::CounterReset) => Some(&COUNTER_RESET_LIST_ITEM),
        (NT::Ul, PT::PaddingLeft) => Some(&PADDING_INLINE_START_40PX),
        (NT::Ul, PT::MarginTop) => Some(&MARGIN_TOP_1EM),
        (NT::Ul, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1EM),
        (NT::Ol, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Ol, PT::ListStyleType) => Some(&LIST_STYLE_TYPE_DECIMAL),
        (NT::Ol, PT::CounterReset) => Some(&COUNTER_RESET_LIST_ITEM),
        (NT::Ol, PT::PaddingLeft) => Some(&PADDING_INLINE_START_40PX),
        (NT::Ol, PT::MarginTop) => Some(&MARGIN_TOP_1EM),
        (NT::Ol, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1EM),
        (NT::Li, PT::Display) => Some(&DISPLAY_LIST_ITEM),
        (NT::Dl, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Dl, PT::MarginTop) => Some(&MARGIN_TOP_1EM),
        (NT::Dl, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1EM),
        (NT::Dt, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Dd, PT::Display) => Some(&DISPLAY_BLOCK),
        // `dd { margin-inline-start: 40px }` (the left edge in LTR).
        (NT::Dd, PT::MarginLeft) => Some(&MARGIN_LEFT_40PX),

        // Inline Elements
        (NT::Span, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::A, PT::Display) => Some(&DISPLAY_INLINE),
        // No underline here: only a LINK (`<a href>`) is underlined, see
        // `get_ua_link_property`.
        (NT::Strong, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Strong, PT::FontWeight) => Some(&FONT_WEIGHT_BOLDER),
        (NT::Em, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Em, PT::FontStyle) => Some(&FONT_STYLE_ITALIC),
        (NT::B, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::B, PT::FontWeight) => Some(&FONT_WEIGHT_BOLDER),
        (NT::I, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::I, PT::FontStyle) => Some(&FONT_STYLE_ITALIC),
        (NT::U, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::U, PT::TextDecoration) => Some(&TEXT_DECORATION_UNDERLINE),
        // `del, s, strike { text-decoration: line-through }` (`<strike>` is
        // read as `s`).
        (NT::S, PT::TextDecoration) => Some(&TEXT_DECORATION_LINE_THROUGH),
        (NT::Small, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Small, PT::FontSize) => Some(&FONT_SIZE_SMALLER),
        (NT::Big, PT::FontSize) => Some(&FONT_SIZE_LARGER),
        // `code, kbd, pre, samp, tt { font-family: monospace }` (`<tt>` is
        // read as `code`).
        (NT::Code, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Code, PT::FontFamily) => Some(&FONT_FAMILY_MONOSPACE),
        (NT::Kbd, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Kbd, PT::FontFamily) => Some(&FONT_FAMILY_MONOSPACE),
        (NT::Samp, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Samp, PT::FontFamily) => Some(&FONT_FAMILY_MONOSPACE),
        (NT::Sub, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Sub, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_SUB),
        (NT::Sub, PT::FontSize) => Some(&FONT_SIZE_SMALLER),
        (NT::Sup, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Sup, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_SUPER),
        (NT::Sup, PT::FontSize) => Some(&FONT_SIZE_SMALLER),

        // Text Content
        // `pre { margin-block: 1em; font-family: monospace; white-space: pre }`
        (NT::Pre, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Pre, PT::WhiteSpace) => Some(&WHITE_SPACE_PRE),
        (NT::Pre, PT::FontFamily) => Some(&FONT_FAMILY_MONOSPACE),
        (NT::Pre, PT::MarginTop) => Some(&MARGIN_TOP_1EM),
        (NT::Pre, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1EM),
        // `blockquote, figure { margin-block: 1em; margin-inline: 40px }`
        (NT::BlockQuote, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::BlockQuote, PT::MarginTop) => Some(&MARGIN_TOP_1EM),
        (NT::BlockQuote, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1EM),
        (NT::BlockQuote, PT::MarginLeft) => Some(&MARGIN_LEFT_40PX),
        (NT::BlockQuote, PT::MarginRight) => Some(&MARGIN_RIGHT_40PX),
        // `address { display: block; font-style: italic }`
        (NT::Address, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Address, PT::FontStyle) => Some(&FONT_STYLE_ITALIC),
        // HTML 15.3.11: a 1px inset gray border on all four sides, width auto.
        (NT::Hr, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Hr, PT::Height) => Some(&HEIGHT_ZERO),
        (NT::Hr, PT::MarginTop) => Some(&MARGIN_TOP_0_5EM),
        (NT::Hr, PT::MarginBottom) => Some(&MARGIN_BOTTOM_0_5EM),
        (NT::Hr, PT::MarginLeft) => Some(&MARGIN_LEFT_AUTO),
        (NT::Hr, PT::MarginRight) => Some(&MARGIN_RIGHT_AUTO),
        (NT::Hr, PT::BorderTopStyle) => Some(&BORDER_TOP_STYLE_INSET),
        (NT::Hr, PT::BorderBottomStyle) => Some(&BORDER_BOTTOM_STYLE_INSET),
        (NT::Hr, PT::BorderLeftStyle) => Some(&BORDER_LEFT_STYLE_INSET),
        (NT::Hr, PT::BorderRightStyle) => Some(&BORDER_RIGHT_STYLE_INSET),
        (NT::Hr, PT::BorderTopWidth) => Some(&BORDER_TOP_WIDTH_1PX),
        (NT::Hr, PT::BorderBottomWidth) => Some(&BORDER_BOTTOM_WIDTH_1PX),
        (NT::Hr, PT::BorderLeftWidth) => Some(&BORDER_LEFT_WIDTH_1PX),
        (NT::Hr, PT::BorderRightWidth) => Some(&BORDER_RIGHT_WIDTH_1PX),
        (NT::Hr, PT::BorderTopColor) => Some(&BORDER_TOP_COLOR_GRAY),
        (NT::Hr, PT::BorderBottomColor) => Some(&BORDER_BOTTOM_COLOR_GRAY),
        (NT::Hr, PT::BorderLeftColor) => Some(&BORDER_LEFT_COLOR_GRAY),
        (NT::Hr, PT::BorderRightColor) => Some(&BORDER_RIGHT_COLOR_GRAY),

        // Table Elements
        // Per CSS Fragmentation Level 3: table ROWS should avoid breaks inside
        // Tables themselves should NOT have break-inside: avoid (they can span pages)
        (NT::Table, PT::Display) => Some(&DISPLAY_TABLE),
        (NT::Table, PT::BoxSizing) => Some(&BOX_SIZING_BORDER_BOX),
        (NT::Table, PT::BorderSpacing) => Some(&BORDER_SPACING_2PX),
        (NT::Table, PT::BorderTopColor) => Some(&TABLE_BORDER_TOP_COLOR),
        (NT::Table, PT::BorderRightColor) => Some(&TABLE_BORDER_RIGHT_COLOR),
        (NT::Table, PT::BorderBottomColor) => Some(&TABLE_BORDER_BOTTOM_COLOR),
        (NT::Table, PT::BorderLeftColor) => Some(&TABLE_BORDER_LEFT_COLOR),
        // NOTE: Removed break-inside: avoid from Table - tables CAN break across pages
        (NT::PageBreak, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::PageBreak, PT::BreakBefore) => Some(&BREAK_BEFORE_PAGE),
        (NT::THead, PT::Display) => Some(&DISPLAY_TABLE_HEADER_GROUP),
        (NT::THead, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_MIDDLE),
        (NT::THead, PT::BreakInside) => Some(&BREAK_INSIDE_AVOID),
        (NT::TBody, PT::Display) => Some(&DISPLAY_TABLE_ROW_GROUP),
        (NT::TBody, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_MIDDLE),
        // NOTE: Removed break-inside: avoid from TBody - tbody CAN break across pages
        (NT::TFoot, PT::Display) => Some(&DISPLAY_TABLE_FOOTER_GROUP),
        (NT::TFoot, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_MIDDLE),
        (NT::TFoot, PT::BreakInside) => Some(&BREAK_INSIDE_AVOID),
        (NT::Tr, PT::Display) => Some(&DISPLAY_TABLE_ROW),
        (NT::Tr, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_MIDDLE),
        (NT::Tr, PT::BreakInside) => Some(&BREAK_INSIDE_AVOID),
        (NT::Th, PT::Display) => Some(&DISPLAY_TABLE_CELL),
        (NT::Th, PT::TextAlign) => Some(&TEXT_ALIGN_CENTER),
        (NT::Th, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::Th, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_MIDDLE),
        (NT::Th, PT::PaddingTop) => Some(&PADDING_TOP_1PX),
        (NT::Th, PT::PaddingBottom) => Some(&PADDING_BOTTOM_1PX),
        (NT::Th, PT::PaddingLeft) => Some(&PADDING_LEFT_1PX),
        (NT::Th, PT::PaddingRight) => Some(&PADDING_RIGHT_1PX),
        (NT::Td, PT::Display) => Some(&DISPLAY_TABLE_CELL),
        (NT::Td, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_MIDDLE),
        (NT::Td, PT::PaddingTop) => Some(&PADDING_TOP_1PX),
        (NT::Td, PT::PaddingBottom) => Some(&PADDING_BOTTOM_1PX),
        (NT::Td, PT::PaddingLeft) => Some(&PADDING_LEFT_1PX),
        (NT::Td, PT::PaddingRight) => Some(&PADDING_RIGHT_1PX),

        // Form Elements
        (NT::Form, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Input, PT::Display) => Some(&DISPLAY_INLINE_BLOCK),
        (NT::Button, PT::Display) => Some(&DISPLAY_INLINE_BLOCK),
        (NT::Button, PT::Cursor) => Some(&CURSOR_POINTER),
        (NT::Button, PT::FontSize) => Some(&FONT_SIZE_13PX),
        (NT::Button, PT::PaddingTop) => Some(&PADDING_TOP_5PX),
        (NT::Button, PT::PaddingBottom) => Some(&PADDING_BOTTOM_5PX),
        (NT::Button, PT::PaddingLeft) => Some(&PADDING_LEFT_10PX),
        (NT::Button, PT::PaddingRight) => Some(&PADDING_RIGHT_10PX),
        (NT::Button, PT::BorderTopWidth) => Some(&BORDER_TOP_WIDTH_1PX),
        (NT::Button, PT::BorderBottomWidth) => Some(&BORDER_BOTTOM_WIDTH_1PX),
        (NT::Button, PT::BorderLeftWidth) => Some(&BORDER_LEFT_WIDTH_1PX),
        (NT::Button, PT::BorderRightWidth) => Some(&BORDER_RIGHT_WIDTH_1PX),
        (NT::Button, PT::BorderTopStyle) => Some(&BUTTON_BORDER_TOP_STYLE),
        (NT::Button, PT::BorderBottomStyle) => Some(&BUTTON_BORDER_BOTTOM_STYLE),
        (NT::Button, PT::BorderLeftStyle) => Some(&BUTTON_BORDER_LEFT_STYLE),
        (NT::Button, PT::BorderRightStyle) => Some(&BUTTON_BORDER_RIGHT_STYLE),
        (NT::Button, PT::BorderTopColor) => Some(&BUTTON_BORDER_TOP_COLOR),
        (NT::Button, PT::BorderBottomColor) => Some(&BUTTON_BORDER_BOTTOM_COLOR),
        (NT::Button, PT::BorderLeftColor) => Some(&BUTTON_BORDER_LEFT_COLOR),
        (NT::Button, PT::BorderRightColor) => Some(&BUTTON_BORDER_RIGHT_COLOR),
        // Text nodes get I-beam cursor for text selection
        // The cursor resolution algorithm ensures that explicit cursor properties
        // on parent elements (e.g., cursor:pointer on button) take precedence
        (NT::Text(_), PT::Cursor) => Some(&CURSOR_TEXT),
        (NT::Select, PT::Display) => Some(&DISPLAY_INLINE_BLOCK),
        (NT::TextArea, PT::Display) => Some(&DISPLAY_INLINE_BLOCK),
        (NT::TextArea, PT::WhiteSpace) => Some(&WHITE_SPACE_PRE_WRAP),
        (NT::TextArea, PT::OverflowWrap) => Some(&OVERFLOW_WRAP_BREAK_WORD),
        // TextArea gets I-beam cursor since it's an editable text field
        (NT::TextArea, PT::Cursor) => Some(&CURSOR_TEXT),
        (NT::Label, PT::Display) => Some(&DISPLAY_INLINE),
        // Hidden Elements
        (NT::Head, PT::Display) => Some(&DISPLAY_NONE),
        (NT::Title, PT::Display) => Some(&DISPLAY_NONE),
        (NT::Script, PT::Display) => Some(&DISPLAY_NONE),
        (NT::Style, PT::Display) => Some(&DISPLAY_NONE),
        (NT::Link, PT::Display) => Some(&DISPLAY_NONE),

        // Special Elements
        // <br> is an inline-level element that forces a line break WITHIN the
        // inline formatting context (HTML §4.5.28). Giving it `display: block`
        // made `<p>text<br>more</p>` split into three stacked block boxes (an
        // extra empty <br> box between two anonymous paragraphs), over-advancing
        // vertically and, inside a table cell, dropping the line after the break.
        // As inline it is turned into a hard `LineBreak` by the IFC collectors.
        (NT::Br, PT::Display) => Some(&DISPLAY_INLINE),
        // Images are replaced elements - inline-block so they respect width/height
        (NT::Image(_), PT::Display) => Some(&DISPLAY_INLINE_BLOCK),

        // Media Elements
        (NT::Video, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Audio, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Canvas, PT::Display) => Some(&DISPLAY_INLINE),
        // An SVG SHAPE is a painted box, not inline content. The default
        // display is `inline`, and `paint_node_background_and_border` skips
        // inline boxes on purpose - their backgrounds belong to text layout,
        // which knows nothing about these - so a shape's fill was dropped and
        // an SVG in a DOM painted nothing at all. Block is what an absolutely
        // positioned box is anyway (CSS blockifies `position: absolute`).
        (
            NT::SvgPath
            | NT::SvgCircle
            | NT::SvgRect
            | NT::SvgEllipse
            | NT::SvgLine
            | NT::SvgPolygon
            | NT::SvgPolyline
            | NT::SvgG
            | NT::SvgUse,
            PT::Display,
        ) => Some(&DISPLAY_BLOCK),
        // `<defs>` and friends define, they do not draw. `<desc>` DESCRIBES
        // the drawing - an icon theme's file carries one, and it was rendered
        // as prose beside the glyph, the same defect as the `<metadata>`
        // block the XML builder now drops.
        (
            NT::SvgDefs | NT::SvgSymbol | NT::SvgClipPathElement | NT::SvgDesc,
            PT::Display,
        ) => Some(&DISPLAY_NONE),
        // An `<svg>` is a REPLACED element, like `<img>` above - inline-level,
        // but with a box of its own. `inline` is what it used to be, and an
        // inline box has no width or height, so the intrinsic size the parser
        // now takes off `viewBox`/`width`/`height` was inert and the element
        // took no space at all: an SVG in a DOM laid out as nothing and
        // painted nothing.
        (NT::Svg, PT::Display) => Some(&DISPLAY_INLINE_BLOCK),
        // VirtualView is a block-level replaced element (like div) — must be block
        // so it participates in flex layout (flex-grow, etc.)
        (NT::VirtualView, PT::Display) => Some(&DISPLAY_BLOCK),
        // A VirtualView exists to virtualize scrollable content, so scrolling
        // is its DEFAULT: `auto` gets it a scroll id (wheel target) and — via
        // the virtual-size-aware necessity rule — a scrollbar exactly when
        // the published `virtual_scroll_size` overflows the viewport. A VV
        // that must NOT wheel-scroll (the map pans+zooms, the video widget)
        // opts out explicitly with `overflow: hidden`, which both already do.
        (NT::VirtualView, PT::OverflowX) => Some(&OVERFLOW_X_AUTO),
        (NT::VirtualView, PT::OverflowY) => Some(&OVERFLOW_Y_AUTO),

        // Icon Elements - inline-block so they have width/height but flow inline
        (NT::Icon(_), PT::Display) => Some(&DISPLAY_INLINE_BLOCK),

        (NT::SelectOption, PT::Display) => Some(&DISPLAY_NONE),
        (NT::OptGroup, PT::Display) => Some(&DISPLAY_NONE),

        // Other Inline Elements
        (NT::Abbr, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Cite, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Cite, PT::FontStyle) => Some(&FONT_STYLE_ITALIC),
        (NT::Del, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Del, PT::TextDecoration) => Some(&TEXT_DECORATION_LINE_THROUGH),
        (NT::Ins, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Ins, PT::TextDecoration) => Some(&TEXT_DECORATION_UNDERLINE),
        // `mark { background: yellow; color: black }`
        (NT::Mark, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Mark, PT::BackgroundContent) => Some(&MARK_BACKGROUND),
        (NT::Mark, PT::TextColor) => Some(&MARK_TEXT_COLOR),
        (NT::Q, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Dfn, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Dfn, PT::FontStyle) => Some(&FONT_STYLE_ITALIC),
        (NT::Var, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Var, PT::FontStyle) => Some(&FONT_STYLE_ITALIC),
        (NT::Time, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Data, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Wbr, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Bdi, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Bdo, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Rp, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Rt, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Rtc, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Ruby, PT::Display) => Some(&DISPLAY_INLINE),

        // Block Container Elements
        // Per CSS Fragmentation Level 3: figures should avoid page breaks inside
        (NT::FieldSet, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Figure, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Figure, PT::BreakInside) => Some(&BREAK_INSIDE_AVOID),
        (NT::Figure, PT::MarginTop) => Some(&MARGIN_TOP_1EM),
        (NT::Figure, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1EM),
        (NT::Figure, PT::MarginLeft) => Some(&MARGIN_LEFT_40PX),
        (NT::Figure, PT::MarginRight) => Some(&MARGIN_RIGHT_40PX),
        (NT::FigCaption, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::FigCaption, PT::BreakInside) => Some(&BREAK_INSIDE_AVOID),
        (NT::Details, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Summary, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Dialog, PT::Display) => Some(&DISPLAY_BLOCK),

        // Table Caption: `caption { text-align: center }`
        (NT::Caption, PT::Display) => Some(&DISPLAY_TABLE_CAPTION),
        (NT::Caption, PT::TextAlign) => Some(&TEXT_ALIGN_CENTER),
        (NT::ColGroup, PT::Display) => Some(&DISPLAY_TABLE_COLUMN_GROUP),
        (NT::Col, PT::Display) => Some(&DISPLAY_TABLE_COLUMN),

        // Legacy/Deprecated Elements: `dir, menu` are lists
        // (`margin-block: 1em; padding-inline-start: 40px`).
        (NT::Menu, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Menu, PT::PaddingLeft) => Some(&PADDING_INLINE_START_40PX),
        (NT::Menu, PT::MarginTop) => Some(&MARGIN_TOP_1EM),
        (NT::Menu, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1EM),
        (NT::Dir, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Dir, PT::PaddingLeft) => Some(&PADDING_INLINE_START_40PX),
        (NT::Dir, PT::MarginTop) => Some(&MARGIN_TOP_1EM),
        (NT::Dir, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1EM),

        // Html (root) Element
        //
        // In browsers, the viewport itself provides scrolling when <html> overflows.
        // Since Azul has no separate viewport scroll mechanism, we set `height: 100%`
        // on the <html> element so it fills the Initial Containing Block (the viewport).
        // This constrains child elements like <body> to the viewport height, enabling
        // overflow:scroll on <body> to create scrollable content areas.
        //
        // Without this, <html> has height:auto and grows to fit all content,
        // making container_size == content_size, which results in a useless 100% scrollbar.
        (NT::Html, PT::Display) => Some(&DISPLAY_BLOCK),
        // ⚠ DIAG (2026-06-02, REVERT): the lifted get_ua_property jump table mis-dispatches
        // (Text/Button, Height) → THIS (Html, Height) arm → children wrongly get height:100%
        // → fill parent (600) instead of content. Commenting it out tests whether removing the
        // ONLY HEIGHT_100_PERCENT producer makes the children auto-height (confirms the chain).
        // REAL fix = the node_type jump-table dispatch/table-mirror in the lift, not this.
        // (NT::Html, PT::Height) => Some(&HEIGHT_100_PERCENT),

        // Universal fallback for display property
        // Per CSS spec, unknown/custom elements should default to inline
        // Text nodes will be filtered out before this function is called
        (_, PT::Display) => Some(&DISPLAY_INLINE),

        // No default defined for other combinations
        _ => None,
    }
}

/// Every property type the UA sheet can answer.
///
/// For the cascade passes that walk the table per node:
/// `CssPropertyCache::apply_ua_css` (the `cascaded_props` / `computed_values`
/// reader) and the compact-cache builder (`apply_ua_css_to_compact`, the
/// layout fast path).
///
/// ONE list for both, on purpose. Each pass used to carry its own copy with
/// the instruction to keep them in sync, and they were not: the compact list
/// had the three non-top button border edges and `TextColor`, the cascaded
/// list had `FontFamily` and the heading `break-*` defaults, and neither had
/// everything — so the two readers disagreed about a node's computed value
/// depending on which one a getter happened to ask (the `VirtualView` overflow
/// default was the first instance found). A type only one pass can store is
/// harmless in the other: `apply_css_property_to_compact` ignores what has no
/// compact slot, and a cascaded entry nothing reads costs one push.
pub const UA_PROPERTY_TYPES: &[CssPropertyType] = &[
    // Tier1 enum properties
    CssPropertyType::Display,
    CssPropertyType::Position,
    CssPropertyType::Float,
    CssPropertyType::Clear,
    CssPropertyType::OverflowX,
    CssPropertyType::OverflowY,
    CssPropertyType::BoxSizing,
    CssPropertyType::FlexDirection,
    CssPropertyType::FlexWrap,
    CssPropertyType::JustifyContent,
    CssPropertyType::AlignItems,
    CssPropertyType::AlignContent,
    CssPropertyType::WritingMode,
    CssPropertyType::FontWeight,
    CssPropertyType::FontStyle,
    CssPropertyType::TextAlign,
    CssPropertyType::Visibility,
    CssPropertyType::WhiteSpace,
    // Only the editing-host / textarea defaults set it; the compact tier has
    // no slot for it, which the doc above covers (the slow path stores it).
    CssPropertyType::OverflowWrap,
    CssPropertyType::Direction,
    CssPropertyType::VerticalAlign,
    CssPropertyType::BorderCollapse,
    // `table { border-spacing: 2px }`.
    CssPropertyType::BorderSpacing,
    // Tier2 dimension properties
    CssPropertyType::Width,
    CssPropertyType::Height,
    CssPropertyType::FontSize,
    CssPropertyType::FontFamily,
    CssPropertyType::MarginTop,
    CssPropertyType::MarginBottom,
    CssPropertyType::MarginLeft,
    CssPropertyType::MarginRight,
    CssPropertyType::PaddingTop,
    CssPropertyType::PaddingBottom,
    CssPropertyType::PaddingLeft,
    CssPropertyType::PaddingRight,
    CssPropertyType::BorderTopWidth,
    CssPropertyType::BorderTopStyle,
    CssPropertyType::BorderTopColor,
    CssPropertyType::BorderRightWidth,
    CssPropertyType::BorderRightStyle,
    CssPropertyType::BorderRightColor,
    CssPropertyType::BorderBottomWidth,
    CssPropertyType::BorderBottomStyle,
    CssPropertyType::BorderBottomColor,
    CssPropertyType::BorderLeftWidth,
    CssPropertyType::BorderLeftStyle,
    CssPropertyType::BorderLeftColor,
    // Fragmentation (headings avoid breaks)
    CssPropertyType::BreakInside,
    CssPropertyType::BreakAfter,
    CssPropertyType::BreakBefore,
    // Text properties
    CssPropertyType::TextColor,
    // `mark { background: yellow }`.
    CssPropertyType::BackgroundContent,
    CssPropertyType::LineHeight,
    CssPropertyType::LetterSpacing,
    CssPropertyType::WordSpacing,
    CssPropertyType::TextDecoration,
    CssPropertyType::Cursor,
    CssPropertyType::ListStyleType,
    // Counters: the UA sheet resets `list-item` on <ol>/<ul> so each list
    // restarts numbering. Without these here the has_counter fast-path bit
    // stays unset for list containers, compute_counters skips the reset, and
    // the list-item counter runs globally (a <ul> then <ol> numbered 1,2 then
    // 3,4 instead of restarting at 1).
    CssPropertyType::CounterReset,
    CssPropertyType::CounterIncrement,
];

/// The UA defaults of the DOCUMENT ROOT (the node at index 0), themed.
///
/// These are the defaults that hold for the whole document and reach every
/// node by inheritance rather than by node type — today that is the inherited
/// text colour, `color`, from [`UA_ROOT_TEXT_COLOR_CSS`]: black on a light
/// window, near-white on a dark one. Keyed on the root's POSITION, not on
/// `<body>`/`<html>`: a body-rooted subtree appended under another document
/// must keep inheriting its new parent's colour, and a document rooted in a
/// `<div>` (tests, popups) needs the default as much as one rooted in
/// `<body>`.
///
/// Consumed by the same three readers as [`get_ua_property_themed`], so the
/// default is IN the resolved style (`cascaded_props` on the root,
/// `computed_values` below it, the compact text tier everywhere) and no
/// paint-time reader has to invent it. `None` for the context answers the
/// light table, like the per-type resolver.
#[must_use]
pub fn get_ua_root_property_themed(
    property_type: CssPropertyType,
    ctx: Option<&DynamicSelectorContext>,
) -> Option<&'static CssProperty> {
    if property_type != CssPropertyType::TextColor {
        return None;
    }
    UA_ROOT_TEXT_COLOR_CSS
        .iter()
        // No window yet: only the unconditional entry applies.
        .find(|prop| ctx.map_or_else(|| !prop.is_conditional(), |c| prop.matches(c)))
        .map(|prop| &prop.property)
}

/// THE UA default for one node and one property: the per-type table
/// ([`get_ua_property_themed`]) or, on the document root, the document-wide
/// table ([`get_ua_root_property_themed`]).
///
/// The one lookup all three readers share — `CssPropertyCache::apply_ua_css`,
/// the compact-cache builder and `get_property_slow`'s last-resort fallback —
/// so they cannot disagree about a default. `is_root` is "node index 0".
///
/// Order: the element's own row first (`<pre contenteditable>` keeps `pre`),
/// then the editing-host defaults, then the document root's.
#[must_use]
pub fn get_ua_default(
    node: &NodeData,
    is_root: bool,
    property_type: CssPropertyType,
    ctx: Option<&DynamicSelectorContext>,
) -> Option<&'static CssProperty> {
    get_ua_property_themed(&node.node_type, property_type, ctx)
        .or_else(|| {
            if is_link(node) {
                get_ua_link_property(property_type, ctx)
            } else {
                None
            }
        })
        .or_else(|| {
            if node.is_contenteditable() {
                get_ua_editing_host_property(property_type)
            } else {
                None
            }
        })
        .or_else(|| {
            if is_root {
                get_ua_root_property_themed(property_type, ctx)
            } else {
                None
            }
        })
}

/// Is `node` a LINK - the `:link` of the UA sheet: an `<a>` with an `href`
/// (HTML 4.8.1: without one it is a placeholder, not a hyperlink).
#[must_use]
pub fn is_link(node: &NodeData) -> bool {
    matches!(node.node_type, NodeType::A)
        && node
            .attributes()
            .as_ref()
            .iter()
            .any(|a| matches!(a, AttributeType::Href(_)))
}

/// UA defaults of a LINK ([`is_link`]): `:link { color: #0000EE; cursor:
/// pointer; text-decoration: underline }` (HTML rendering 15.3.4), the colour
/// themed - #9E9EFF in the dark mode, where #0000EE cannot be read. An `<a>`
/// without an `href` is a placeholder and gets none of them (the mail
/// sanitizer drops the hrefs it cannot follow, and Chrome shows those
/// anchors plain).
#[must_use]
pub fn get_ua_link_property(
    property_type: CssPropertyType,
    ctx: Option<&DynamicSelectorContext>,
) -> Option<&'static CssProperty> {
    let dark = ctx.is_some_and(|c| c.mode == azul_css::system::DarkLightMode::Dark);
    match property_type {
        CssPropertyType::TextColor if dark => Some(&LINK_COLOR_DARK),
        CssPropertyType::TextColor => Some(&LINK_COLOR),
        CssPropertyType::Cursor => Some(&CURSOR_POINTER),
        CssPropertyType::TextDecoration => Some(&TEXT_DECORATION_UNDERLINE),
        _ => None,
    }
}

/// UA defaults of an EDITING HOST (a `contenteditable` node), inherited by
/// everything inside it.
///
/// `white-space: pre-wrap`, because an editor must show exactly what the user
/// typed. Under the initial `normal`, CSS Text 3 removes a space at the end of
/// a line and collapses a run of spaces to one. The typed space then had no
/// position, so the caret did not move and nothing repainted until the next
/// letter. `pre-wrap` preserves every space and HANGS the trailing ones: they
/// keep their advance for the caret but do not count toward fitting the line
/// (CSS Text 3 §4.1.2). Browsers get there by rewriting typed spaces to NBSP
/// under `normal` (Gecko, `WebKit`) or by forcing `pre-wrap` on
/// `contenteditable=plaintext-only` (Chromium); whatwg/html#11350 proposes
/// exactly this UA rule, `<textarea>` has always had it, and the editor
/// frameworks (`ProseMirror`, Lexical) require it on their root. azul stores
/// plain spaces, so it takes the UA rule. Author CSS overrides it.
///
/// `overflow-wrap: break-word`: an overlong word in an editor wraps instead of
/// overflowing the box (Chromium's contenteditable default).
#[must_use]
pub fn get_ua_editing_host_property(
    property_type: CssPropertyType,
) -> Option<&'static CssProperty> {
    match property_type {
        CssPropertyType::WhiteSpace => Some(&WHITE_SPACE_PRE_WRAP),
        CssPropertyType::OverflowWrap => Some(&OVERFLOW_WRAP_BREAK_WORD),
        _ => None,
    }
}

/// [`get_ua_property`], with the theme taken into account.
///
/// The UA sheet is a static table, which is what keeps the property cache a
/// pure function of node type — but a handful of its defaults are COLOURS, and
/// a colour default cannot be one constant: a button border or an `<hr>` rule
/// chosen for a white window is wrong on a dark one. Those few resolve through
/// here: with a dark context they answer their dark twin, otherwise the plain
/// table. The inherited text colour has the same shape in
/// [`UA_ROOT_TEXT_COLOR_CSS`], answered by [`get_ua_root_property_themed`].
///
/// This pair is THE themed UA table. All three readers go through it —
/// `apply_ua_css` (cascaded/computed values), the compact-cache builder (the
/// layout fast path) and `get_property_slow`'s last-resort fallback — so a
/// theme flip changes the same answer everywhere, and a getter that asks the
/// compact tier gets what the slow path would have said.
///
/// `None` for the context means "no window yet" and falls back to the light
/// table, which is what every caller did before this existed.
#[must_use]
pub fn get_ua_property_themed(
    node_type: &NodeType,
    property_type: CssPropertyType,
    ctx: Option<&DynamicSelectorContext>,
) -> Option<&'static CssProperty> {
    use CssPropertyType as PT;

    use crate::dom::NodeType as NT;

    let dark = ctx.is_some_and(|c| c.mode == azul_css::system::DarkLightMode::Dark);
    if dark {
        let twin = match (node_type, property_type) {
            (NT::Hr, PT::BorderTopColor) => Some(&BORDER_TOP_COLOR_GRAY_DARK),
            (NT::Hr, PT::BorderBottomColor) => Some(&BORDER_BOTTOM_COLOR_GRAY_DARK),
            (NT::Hr, PT::BorderLeftColor) => Some(&BORDER_LEFT_COLOR_GRAY_DARK),
            (NT::Hr, PT::BorderRightColor) => Some(&BORDER_RIGHT_COLOR_GRAY_DARK),
            (NT::Button, PT::BorderTopColor) => Some(&BUTTON_BORDER_TOP_COLOR_DARK),
            (NT::Button, PT::BorderBottomColor) => Some(&BUTTON_BORDER_BOTTOM_COLOR_DARK),
            (NT::Button, PT::BorderLeftColor) => Some(&BUTTON_BORDER_LEFT_COLOR_DARK),
            (NT::Button, PT::BorderRightColor) => Some(&BUTTON_BORDER_RIGHT_COLOR_DARK),
            _ => None,
        };
        if twin.is_some() {
            return twin;
        }
    }
    get_ua_property(node_type, property_type)
}

// ============================================================================
// UA Scrollbar Defaults — individual CssPropertyWithConditions
// ============================================================================
//
// These rules define the default scrollbar appearance per OS and theme,
// using the same `@os` / `@theme` condition system as author CSS.
// Each entry is a single CSS property (scrollbar-color or scrollbar-width)
// with its conditions.  Rules are evaluated first-match-wins per property type.
//
// Conceptually equivalent to:
//
//   @os macos                { scrollbar-width: thin; }
//   @os ios                  { scrollbar-width: thin; }
//   @os android              { scrollbar-width: thin; }
//   /* default */            { scrollbar-width: auto; }
//
//   @os macos                { -azul-scrollbar-visibility: when-scrolling; }
//   @os ios                  { -azul-scrollbar-visibility: when-scrolling; }
//   @os android              { -azul-scrollbar-visibility: when-scrolling; }
//   /* default */            { -azul-scrollbar-visibility: always; }
//
//   @os macos                { -azul-scrollbar-fade-delay: 500ms; }
//   @os ios                  { -azul-scrollbar-fade-delay: 500ms; }
//   @os android              { -azul-scrollbar-fade-delay: 300ms; }
//   /* default */            { -azul-scrollbar-fade-delay: 0; }
//
//   @os macos                { -azul-scrollbar-fade-duration: 200ms; }
//   @os ios                  { -azul-scrollbar-fade-duration: 200ms; }
//   @os android              { -azul-scrollbar-fade-duration: 150ms; }
//   /* default */            { -azul-scrollbar-fade-duration: 0; }
//
//   @os macos @theme dark    { scrollbar-color: rgba(180,180,180,0.78) rgba(40,40,40,0.31); }
//   @os macos @theme light   { scrollbar-color: rgba(80,80,80,0.78) rgba(200,200,200,0.31); }
//   @os windows @theme dark  { scrollbar-color: #6e6e6e #202020; }
//   @os windows @theme light { scrollbar-color: #828282 #f1f1f1; }
//   @os ios @theme dark      { scrollbar-color: rgba(255,255,255,0.4) transparent; }
//   @os ios @theme light     { scrollbar-color: rgba(0,0,0,0.4) transparent; }
//   @os android @theme dark  { scrollbar-color: rgba(255,255,255,0.3) transparent; }
//   @os android @theme light { scrollbar-color: rgba(0,0,0,0.3) transparent; }
//   @theme dark              { scrollbar-color: #646464 #2d2d2d; }
//   /* default */            { scrollbar-color: #c1c1c1 #f1f1f1; }

/// Helper to create a const `scrollbar-color` `CssProperty`.
const fn scrollbar_color(thumb: ColorU, track: ColorU) -> CssProperty {
    CssProperty::ScrollbarColor(CssPropertyValue::Exact(StyleScrollbarColor::Custom(
        ScrollbarColorCustom { thumb, track },
    )))
}

/// Helper to create a const `scrollbar-width` `CssProperty`.
const fn scrollbar_width(w: LayoutScrollbarWidth) -> CssProperty {
    CssProperty::ScrollbarWidth(CssPropertyValue::Exact(w))
}

/// Helper to create a const `-azul-scrollbar-visibility` `CssProperty`.
const fn scrollbar_visibility(v: ScrollbarVisibilityMode) -> CssProperty {
    CssProperty::ScrollbarVisibility(CssPropertyValue::Exact(v))
}

/// Helper to create a const `-azul-scrollbar-fade-delay` `CssProperty`.
const fn scrollbar_fade_delay(ms: u32) -> CssProperty {
    CssProperty::ScrollbarFadeDelay(CssPropertyValue::Exact(ScrollbarFadeDelay::new(ms)))
}

/// Helper to create a const `-azul-scrollbar-fade-duration` `CssProperty`.
const fn scrollbar_fade_duration(ms: u32) -> CssProperty {
    CssProperty::ScrollbarFadeDuration(CssPropertyValue::Exact(ScrollbarFadeDuration::new(ms)))
}

/// UA scrollbar CSS properties with `@os` / `@theme` conditions.
///
/// Ordered most-specific first.  The evaluation function picks the
/// first matching entry for each property type (`scrollbar-color`,
/// `scrollbar-width`, `-azul-scrollbar-visibility`,
/// `-azul-scrollbar-fade-delay`, `-azul-scrollbar-fade-duration`).
pub(crate) static UA_SCROLLBAR_CSS: &[CssPropertyWithConditions] = &[
    // ── scrollbar-width per OS ──────────────────────────────────────────
    // macOS → thin (overlay)
    CssPropertyWithConditions::with_single_condition(
        scrollbar_width(LayoutScrollbarWidth::Thin),
        &[DynamicSelector::Os(OsCondition::MacOS)],
    ),
    // iOS → thin
    CssPropertyWithConditions::with_single_condition(
        scrollbar_width(LayoutScrollbarWidth::Thin),
        &[DynamicSelector::Os(OsCondition::IOS)],
    ),
    // Android → thin
    CssPropertyWithConditions::with_single_condition(
        scrollbar_width(LayoutScrollbarWidth::Thin),
        &[DynamicSelector::Os(OsCondition::Android)],
    ),
    // default → auto (classic)
    CssPropertyWithConditions::simple(scrollbar_width(LayoutScrollbarWidth::Auto)),
    // ── scrollbar-visibility per OS ─────────────────────────────────────
    // macOS → overlay (show only when scrolling)
    CssPropertyWithConditions::with_single_condition(
        scrollbar_visibility(ScrollbarVisibilityMode::WhenScrolling),
        &[DynamicSelector::Os(OsCondition::MacOS)],
    ),
    // iOS → overlay
    CssPropertyWithConditions::with_single_condition(
        scrollbar_visibility(ScrollbarVisibilityMode::WhenScrolling),
        &[DynamicSelector::Os(OsCondition::IOS)],
    ),
    // Android → overlay
    CssPropertyWithConditions::with_single_condition(
        scrollbar_visibility(ScrollbarVisibilityMode::WhenScrolling),
        &[DynamicSelector::Os(OsCondition::Android)],
    ),
    // default → always visible (classic)
    CssPropertyWithConditions::simple(scrollbar_visibility(ScrollbarVisibilityMode::Always)),
    // ── scrollbar-fade-delay per OS ─────────────────────────────────────
    CssPropertyWithConditions::with_single_condition(
        scrollbar_fade_delay(500),
        &[DynamicSelector::Os(OsCondition::MacOS)],
    ),
    CssPropertyWithConditions::with_single_condition(
        scrollbar_fade_delay(500),
        &[DynamicSelector::Os(OsCondition::IOS)],
    ),
    CssPropertyWithConditions::with_single_condition(
        scrollbar_fade_delay(300),
        &[DynamicSelector::Os(OsCondition::Android)],
    ),
    // default → 0 (no fade)
    CssPropertyWithConditions::simple(scrollbar_fade_delay(0)),
    // ── scrollbar-fade-duration per OS ──────────────────────────────────
    CssPropertyWithConditions::with_single_condition(
        scrollbar_fade_duration(200),
        &[DynamicSelector::Os(OsCondition::MacOS)],
    ),
    CssPropertyWithConditions::with_single_condition(
        scrollbar_fade_duration(200),
        &[DynamicSelector::Os(OsCondition::IOS)],
    ),
    CssPropertyWithConditions::with_single_condition(
        scrollbar_fade_duration(150),
        &[DynamicSelector::Os(OsCondition::Android)],
    ),
    // default → 0 (instant)
    CssPropertyWithConditions::simple(scrollbar_fade_duration(0)),
    // ── scrollbar-color per OS + theme ──────────────────────────────────
    // macOS dark: light grey thumb on dark semi-transparent track
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU {
                r: 180,
                g: 180,
                b: 180,
                a: 200,
            },
            ColorU {
                r: 40,
                g: 40,
                b: 40,
                a: 80,
            },
        ),
        &[
            DynamicSelector::Os(OsCondition::MacOS),
            DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark),
        ],
    ),
    // macOS light: dark grey thumb on light semi-transparent track
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU {
                r: 80,
                g: 80,
                b: 80,
                a: 200,
            },
            ColorU {
                r: 200,
                g: 200,
                b: 200,
                a: 80,
            },
        ),
        &[
            DynamicSelector::Os(OsCondition::MacOS),
            DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Light),
        ],
    ),
    // Windows dark
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU {
                r: 110,
                g: 110,
                b: 110,
                a: 255,
            },
            ColorU {
                r: 32,
                g: 32,
                b: 32,
                a: 255,
            },
        ),
        &[
            DynamicSelector::Os(OsCondition::Windows),
            DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark),
        ],
    ),
    // Windows light
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU {
                r: 130,
                g: 130,
                b: 130,
                a: 255,
            },
            ColorU {
                r: 241,
                g: 241,
                b: 241,
                a: 255,
            },
        ),
        &[
            DynamicSelector::Os(OsCondition::Windows),
            DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Light),
        ],
    ),
    // iOS dark
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 100,
            },
            ColorU::TRANSPARENT,
        ),
        &[
            DynamicSelector::Os(OsCondition::IOS),
            DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark),
        ],
    ),
    // iOS light
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 100,
            },
            ColorU::TRANSPARENT,
        ),
        &[
            DynamicSelector::Os(OsCondition::IOS),
            DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Light),
        ],
    ),
    // Android dark
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 77,
            },
            ColorU::TRANSPARENT,
        ),
        &[
            DynamicSelector::Os(OsCondition::Android),
            DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark),
        ],
    ),
    // Android light
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 77,
            },
            ColorU::TRANSPARENT,
        ),
        &[
            DynamicSelector::Os(OsCondition::Android),
            DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Light),
        ],
    ),
    // Linux / unknown dark fallback
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU {
                r: 100,
                g: 100,
                b: 100,
                a: 255,
            },
            ColorU {
                r: 45,
                g: 45,
                b: 45,
                a: 255,
            },
        ),
        &[DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)],
    ),
    // Unconditional fallback (classic light)
    CssPropertyWithConditions::simple(scrollbar_color(
        ColorU {
            r: 193,
            g: 193,
            b: 193,
            a: 255,
        },
        ColorU {
            r: 241,
            g: 241,
            b: 241,
            a: 255,
        },
    )),
];

/// Resolved UA scrollbar defaults after evaluating conditions.
///
/// All fields are guaranteed to resolve because `UA_SCROLLBAR_CSS`
/// contains unconditional fallback entries for every property type.
#[derive(Debug, Copy, Clone)]
pub struct ResolvedUaScrollbar {
    pub color: StyleScrollbarColor,
    pub width: LayoutScrollbarWidth,
    pub visibility: ScrollbarVisibilityMode,
    pub fade_delay: ScrollbarFadeDelay,
    pub fade_duration: ScrollbarFadeDuration,
}

/// Evaluate UA scrollbar CSS rules against a `DynamicSelectorContext`.
///
/// Iterates `UA_SCROLLBAR_CSS` and picks the first matching entry per
/// property type.  Unconditional fallback entries in the table guarantee
/// that every field resolves.
/// The default `color` for text that inherits none, as a static table the
/// dynamic context resolves.
///
/// A UA default cannot be one constant: black text is correct on the light
/// window background and invisible on the dark one, and which of those the
/// window has is not known until a `DynamicSelectorContext` exists. Declaring
/// BOTH here — the plain rule and its `@theme dark` variant — keeps the table
/// static (so the UA property cache stays a pure function of node type) and
/// defers the choice to the same evaluation step the scrollbar defaults use.
///
/// Ordered most-specific first, like [`UA_SCROLLBAR_CSS`].
pub(crate) static UA_ROOT_TEXT_COLOR_CSS: &[CssPropertyWithConditions] = &[
    // Dark window background -> near-white text, matching the platform's own
    // "label" colour rather than pure white, which glares.
    CssPropertyWithConditions::with_single_condition(
        CssProperty::TextColor(CssPropertyValue::Exact(
            StyleTextColor {
                inner: ColorU {
                    r: 0xe8,
                    g: 0xe8,
                    b: 0xe8,
                    a: 255,
                },
            },
        )),
        &[DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)],
    ),
    // default -> opaque black, the CSS initial value.
    CssPropertyWithConditions::simple(CssProperty::TextColor(CssPropertyValue::Exact(
        azul_css::defaults::DEFAULT_TEXT_COLOR,
    ))),
];

/// The inherited-text-colour default for `ctx`'s theme, as a value.
///
/// The cascade consumes the same table through
/// [`get_ua_root_property_themed`], so after a cascade has run this answer is
/// already in every node's resolved style; the remaining callers are the
/// `debug_assert!`-guarded paint-time fallbacks, which should never be
/// reached on a cascaded DOM. Falls back to the CSS initial value if the
/// table somehow matches nothing, so this can never return "no colour".
#[must_use]
pub fn evaluate_ua_root_text_color(
    ctx: &DynamicSelectorContext,
) -> StyleTextColor {
    match get_ua_root_property_themed(CssPropertyType::TextColor, Some(ctx)) {
        Some(CssProperty::TextColor(CssPropertyValue::Exact(c))) => *c,
        _ => azul_css::defaults::DEFAULT_TEXT_COLOR,
    }
}

#[must_use]
pub fn evaluate_ua_scrollbar_css(ctx: &DynamicSelectorContext) -> ResolvedUaScrollbar {
    let mut color: Option<StyleScrollbarColor> = None;
    let mut width: Option<LayoutScrollbarWidth> = None;
    let mut visibility: Option<ScrollbarVisibilityMode> = None;
    let mut fade_delay: Option<ScrollbarFadeDelay> = None;
    let mut fade_duration: Option<ScrollbarFadeDuration> = None;

    for prop in UA_SCROLLBAR_CSS {
        if !prop.matches(ctx) {
            continue;
        }
        match &prop.property {
            CssProperty::ScrollbarColor(CssPropertyValue::Exact(c)) => {
                if color.is_none() {
                    color = Some(*c);
                }
            }
            CssProperty::ScrollbarWidth(CssPropertyValue::Exact(w)) => {
                if width.is_none() {
                    width = Some(*w);
                }
            }
            CssProperty::ScrollbarVisibility(CssPropertyValue::Exact(v)) => {
                if visibility.is_none() {
                    visibility = Some(*v);
                }
            }
            CssProperty::ScrollbarFadeDelay(CssPropertyValue::Exact(d)) => {
                if fade_delay.is_none() {
                    fade_delay = Some(*d);
                }
            }
            CssProperty::ScrollbarFadeDuration(CssPropertyValue::Exact(d)) => {
                if fade_duration.is_none() {
                    fade_duration = Some(*d);
                }
            }
            _ => {}
        }
        if color.is_some()
            && width.is_some()
            && visibility.is_some()
            && fade_delay.is_some()
            && fade_duration.is_some()
        {
            break;
        }
    }

    // Unconditional `simple` entries in UA_SCROLLBAR_CSS guarantee all
    // fields resolve; these defaults match those entries as a safety net.
    ResolvedUaScrollbar {
        color: color.unwrap_or(StyleScrollbarColor::Custom(ScrollbarColorCustom {
            thumb: ColorU {
                r: 193,
                g: 193,
                b: 193,
                a: 255,
            },
            track: ColorU {
                r: 241,
                g: 241,
                b: 241,
                a: 255,
            },
        })),
        width: width.unwrap_or(LayoutScrollbarWidth::Auto),
        visibility: visibility.unwrap_or(ScrollbarVisibilityMode::Always),
        fade_delay: fade_delay.unwrap_or(ScrollbarFadeDelay { ms: 0 }),
        fade_duration: fade_duration.unwrap_or(ScrollbarFadeDuration { ms: 0 }),
    }
}

#[cfg(test)]
#[path = "ua_css_test.rs"]
mod ua_css_test;
