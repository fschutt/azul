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
        CssPropertyWithConditions,
        DynamicSelector, DynamicSelectorContext, OsCondition, ThemeCondition,
    },
    props::{
        basic::{
            font::StyleFontWeight, pixel::PixelValue, ColorU,
            StyleFontSize,
        },
        layout::{
            dimensions::{LayoutHeight, LayoutWidth},
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
            border::{
                BorderStyle,
                LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, LayoutBorderTopWidth,
                StyleBorderBottomColor, StyleBorderBottomStyle,
                StyleBorderLeftColor, StyleBorderLeftStyle,
                StyleBorderRightColor, StyleBorderRightStyle,
                StyleBorderTopColor, StyleBorderTopStyle,
            },
            content::CounterReset,
            effects::StyleCursor,
            lists::StyleListStyleType,
            scrollbar::{
                LayoutScrollbarWidth, ScrollbarColorCustom, ScrollbarFadeDelay,
                ScrollbarFadeDuration, ScrollbarVisibilityMode, StyleScrollbarColor,
            },
            text::StyleTextDecoration,
            StyleTextAlign, StyleVerticalAlign,
        },
    },
};

use crate::dom::NodeType;

/// 100% width
static WIDTH_100_PERCENT: CssProperty = CssProperty::Width(CssPropertyValue::Exact(
    LayoutWidth::Px(PixelValue::const_percent(100)),
));

/// 100% height
static HEIGHT_100_PERCENT: CssProperty = CssProperty::Height(CssPropertyValue::Exact(
    LayoutHeight::Px(PixelValue::const_percent(100)),
));

/// display: block
static DISPLAY_BLOCK: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Block));

/// display: inline
static DISPLAY_INLINE: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Inline));

/// display: inline-block
static DISPLAY_INLINE_BLOCK: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::InlineBlock));

/// display: none
static DISPLAY_NONE: CssProperty =
    CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::None));

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
static CURSOR_TEXT: CssProperty =
    CssProperty::Cursor(CssPropertyValue::Exact(StyleCursor::Text));

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

/// list-style-type: disc (default for <ul>)
static LIST_STYLE_TYPE_DISC: CssProperty =
    CssProperty::ListStyleType(CssPropertyValue::Exact(StyleListStyleType::Disc));

/// list-style-type: decimal (default for <ol>)
static LIST_STYLE_TYPE_DECIMAL: CssProperty =
    CssProperty::ListStyleType(CssPropertyValue::Exact(StyleListStyleType::Decimal));

// --- HR Element Defaults ---
// Per HTML spec, <hr> renders as a horizontal line with inset border style

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

/// border-top-style: inset (for hr - default browser style)
static BORDER_TOP_STYLE_INSET: CssProperty =
    CssProperty::BorderTopStyle(CssPropertyValue::Exact(StyleBorderTopStyle {
        inner: BorderStyle::Inset,
    }));

/// border-top-width: 1px (for hr)
static BORDER_TOP_WIDTH_1PX: CssProperty =
    CssProperty::BorderTopWidth(CssPropertyValue::Exact(LayoutBorderTopWidth {
        inner: PixelValue::const_px(1),
    }));

/// border-top-color: gray (for hr - default visible color)
static BORDER_TOP_COLOR_GRAY: CssProperty =
    CssProperty::BorderTopColor(CssPropertyValue::Exact(StyleBorderTopColor {
        inner: ColorU {
            r: 128,
            g: 128,
            b: 128,
            a: 255,
        },
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
/// 3. Padding on <li> creates space between the marker and the text content
///    TODO: Change to `PaddingInlineStart` once logical property resolution is implemented
static PADDING_INLINE_START_40PX: CssProperty =
    CssProperty::PaddingLeft(CssPropertyValue::Exact(LayoutPaddingLeft {
        inner: PixelValue::const_px(40),
    }));

/// Text decoration: underline - used for <a> and <u> elements
static TEXT_DECORATION_UNDERLINE: CssProperty = CssProperty::TextDecoration(
    CssPropertyValue::Exact(StyleTextDecoration::Underline),
);

// --- Button Element Defaults ---
// Per browser UA CSS, <button> has padding, border, and a system font size.
// These ensure a button is visible even without author CSS.

/// font-size: 13px (standard button font size on macOS/Linux)
static FONT_SIZE_13PX: CssProperty = CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
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
static BUTTON_BORDER_COLOR: ColorU = ColorU { r: 200, g: 200, b: 200, a: 255 };

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

static BUTTON_BORDER_TOP_WIDTH: CssProperty =
    CssProperty::BorderTopWidth(CssPropertyValue::Exact(LayoutBorderTopWidth {
        inner: PixelValue::const_px(1),
    }));
static BUTTON_BORDER_BOTTOM_WIDTH: CssProperty =
    CssProperty::BorderBottomWidth(CssPropertyValue::Exact(LayoutBorderBottomWidth {
        inner: PixelValue::const_px(1),
    }));
static BUTTON_BORDER_LEFT_WIDTH: CssProperty =
    CssProperty::BorderLeftWidth(CssPropertyValue::Exact(LayoutBorderLeftWidth {
        inner: PixelValue::const_px(1),
    }));
static BUTTON_BORDER_RIGHT_WIDTH: CssProperty =
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
#[must_use] pub fn get_ua_property(
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
        (NT::Ol, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Ol, PT::ListStyleType) => Some(&LIST_STYLE_TYPE_DECIMAL),
        (NT::Ol, PT::CounterReset) => Some(&COUNTER_RESET_LIST_ITEM),
        (NT::Ol, PT::PaddingLeft) => Some(&PADDING_INLINE_START_40PX),
        (NT::Li, PT::Display) => Some(&DISPLAY_LIST_ITEM),
        (NT::Dl, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Dt, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Dd, PT::Display) => Some(&DISPLAY_BLOCK),

        // Inline Elements
        (NT::Span, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::A, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::A, PT::TextDecoration) => Some(&TEXT_DECORATION_UNDERLINE),
        (NT::Strong, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Strong, PT::FontWeight) => Some(&FONT_WEIGHT_BOLDER),
        (NT::Em, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::B, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::B, PT::FontWeight) => Some(&FONT_WEIGHT_BOLDER),
        (NT::I, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::U, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::U, PT::TextDecoration) => Some(&TEXT_DECORATION_UNDERLINE),
        (NT::Small, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Code, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Kbd, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Samp, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Sub, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Sup, PT::Display) => Some(&DISPLAY_INLINE),

        // Text Content
        (NT::Pre, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::BlockQuote, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Hr, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Hr, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Hr, PT::Height) => Some(&HEIGHT_ZERO),
        (NT::Hr, PT::MarginTop) => Some(&MARGIN_TOP_0_5EM),
        (NT::Hr, PT::MarginBottom) => Some(&MARGIN_BOTTOM_0_5EM),
        (NT::Hr, PT::BorderTopStyle) => Some(&BORDER_TOP_STYLE_INSET),
        (NT::Hr, PT::BorderTopWidth) => Some(&BORDER_TOP_WIDTH_1PX),
        (NT::Hr, PT::BorderTopColor) => Some(&BORDER_TOP_COLOR_GRAY),

        // Table Elements
        // Per CSS Fragmentation Level 3: table ROWS should avoid breaks inside
        // Tables themselves should NOT have break-inside: avoid (they can span pages)
        (NT::Table, PT::Display) => Some(&DISPLAY_TABLE),
        // NOTE: Removed break-inside: avoid from Table - tables CAN break across pages
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
        (NT::Button, PT::BorderTopWidth) => Some(&BUTTON_BORDER_TOP_WIDTH),
        (NT::Button, PT::BorderBottomWidth) => Some(&BUTTON_BORDER_BOTTOM_WIDTH),
        (NT::Button, PT::BorderLeftWidth) => Some(&BUTTON_BORDER_LEFT_WIDTH),
        (NT::Button, PT::BorderRightWidth) => Some(&BUTTON_BORDER_RIGHT_WIDTH),
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
        (NT::Svg, PT::Display) => Some(&DISPLAY_INLINE),
        // VirtualView is a block-level replaced element (like div) — must be block
        // so it participates in flex layout (flex-grow, etc.)
        (NT::VirtualView, PT::Display) => Some(&DISPLAY_BLOCK),

        // Icon Elements - inline-block so they have width/height but flow inline
        (NT::Icon(_), PT::Display) => Some(&DISPLAY_INLINE_BLOCK),

        (NT::SelectOption, PT::Display) => Some(&DISPLAY_NONE),
        (NT::OptGroup, PT::Display) => Some(&DISPLAY_NONE),

        // Other Inline Elements
        (NT::Abbr, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Cite, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Del, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Ins, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Mark, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Q, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Dfn, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Var, PT::Display) => Some(&DISPLAY_INLINE),
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
        (NT::FigCaption, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::FigCaption, PT::BreakInside) => Some(&BREAK_INSIDE_AVOID),
        (NT::Details, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Summary, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Dialog, PT::Display) => Some(&DISPLAY_BLOCK),

        // Table Caption
        (NT::Caption, PT::Display) => Some(&DISPLAY_TABLE_CAPTION),
        (NT::ColGroup, PT::Display) => Some(&DISPLAY_TABLE_COLUMN_GROUP),
        (NT::Col, PT::Display) => Some(&DISPLAY_TABLE_COLUMN),

        // Legacy/Deprecated Elements
        (NT::Menu, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Dir, PT::Display) => Some(&DISPLAY_BLOCK),

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
    CssProperty::ScrollbarColor(CssPropertyValue::Exact(
        StyleScrollbarColor::Custom(ScrollbarColorCustom { thumb, track }),
    ))
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
    CssPropertyWithConditions::simple(
        scrollbar_width(LayoutScrollbarWidth::Auto),
    ),

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
    CssPropertyWithConditions::simple(
        scrollbar_visibility(ScrollbarVisibilityMode::Always),
    ),

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
    CssPropertyWithConditions::simple(
        scrollbar_fade_delay(0),
    ),

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
    CssPropertyWithConditions::simple(
        scrollbar_fade_duration(0),
    ),

    // ── scrollbar-color per OS + theme ──────────────────────────────────
    // macOS dark: light grey thumb on dark semi-transparent track
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU { r: 180, g: 180, b: 180, a: 200 },
            ColorU { r: 40, g: 40, b: 40, a: 80 },
        ),
        &[DynamicSelector::Os(OsCondition::MacOS), DynamicSelector::Theme(ThemeCondition::Dark)],
    ),
    // macOS light: dark grey thumb on light semi-transparent track
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU { r: 80, g: 80, b: 80, a: 200 },
            ColorU { r: 200, g: 200, b: 200, a: 80 },
        ),
        &[DynamicSelector::Os(OsCondition::MacOS), DynamicSelector::Theme(ThemeCondition::Light)],
    ),
    // Windows dark
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU { r: 110, g: 110, b: 110, a: 255 },
            ColorU { r: 32, g: 32, b: 32, a: 255 },
        ),
        &[DynamicSelector::Os(OsCondition::Windows), DynamicSelector::Theme(ThemeCondition::Dark)],
    ),
    // Windows light
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU { r: 130, g: 130, b: 130, a: 255 },
            ColorU { r: 241, g: 241, b: 241, a: 255 },
        ),
        &[DynamicSelector::Os(OsCondition::Windows), DynamicSelector::Theme(ThemeCondition::Light)],
    ),
    // iOS dark
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU { r: 255, g: 255, b: 255, a: 100 },
            ColorU::TRANSPARENT,
        ),
        &[DynamicSelector::Os(OsCondition::IOS), DynamicSelector::Theme(ThemeCondition::Dark)],
    ),
    // iOS light
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU { r: 0, g: 0, b: 0, a: 100 },
            ColorU::TRANSPARENT,
        ),
        &[DynamicSelector::Os(OsCondition::IOS), DynamicSelector::Theme(ThemeCondition::Light)],
    ),
    // Android dark
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU { r: 255, g: 255, b: 255, a: 77 },
            ColorU::TRANSPARENT,
        ),
        &[DynamicSelector::Os(OsCondition::Android), DynamicSelector::Theme(ThemeCondition::Dark)],
    ),
    // Android light
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU { r: 0, g: 0, b: 0, a: 77 },
            ColorU::TRANSPARENT,
        ),
        &[DynamicSelector::Os(OsCondition::Android), DynamicSelector::Theme(ThemeCondition::Light)],
    ),
    // Linux / unknown dark fallback
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU { r: 100, g: 100, b: 100, a: 255 },
            ColorU { r: 45, g: 45, b: 45, a: 255 },
        ),
        &[DynamicSelector::Theme(ThemeCondition::Dark)],
    ),
    // Unconditional fallback (classic light)
    CssPropertyWithConditions::simple(
        scrollbar_color(
            ColorU { r: 193, g: 193, b: 193, a: 255 },
            ColorU { r: 241, g: 241, b: 241, a: 255 },
        ),
    ),
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
#[must_use] pub fn evaluate_ua_scrollbar_css(ctx: &DynamicSelectorContext) -> ResolvedUaScrollbar {
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
        if color.is_some() && width.is_some() && visibility.is_some()
            && fade_delay.is_some() && fade_duration.is_some()
        {
            break;
        }
    }

    // Unconditional `simple` entries in UA_SCROLLBAR_CSS guarantee all
    // fields resolve; these defaults match those entries as a safety net.
    ResolvedUaScrollbar {
        color: color.unwrap_or(StyleScrollbarColor::Custom(ScrollbarColorCustom {
            thumb: ColorU { r: 193, g: 193, b: 193, a: 255 },
            track: ColorU { r: 241, g: 241, b: 241, a: 255 },
        })),
        width: width.unwrap_or(LayoutScrollbarWidth::Auto),
        visibility: visibility.unwrap_or(ScrollbarVisibilityMode::Always),
        fade_delay: fade_delay.unwrap_or(ScrollbarFadeDelay { ms: 0 }),
        fade_duration: fade_duration.unwrap_or(ScrollbarFadeDuration { ms: 0 }),
    }
}

#[cfg(test)]
mod autotest_generated {
    use alloc::{string::String, vec, vec::Vec};

    use azul_css::{corety::AzString, css::BoxOrStatic, props::basic::length::SizeMetric};

    use super::*;
    use crate::resources::{ImageRef, RawImageFormat};

    // ------------------------------------------------------------------
    // Constructors / helpers
    // ------------------------------------------------------------------

    fn text_node(s: &str) -> NodeType {
        NodeType::Text(BoxOrStatic::heap(AzString::from(s)))
    }

    fn icon_node(s: &str) -> NodeType {
        NodeType::Icon(BoxOrStatic::heap(AzString::from(s)))
    }

    fn image_node() -> NodeType {
        NodeType::Image(BoxOrStatic::heap(ImageRef::null_image(
            1,
            1,
            RawImageFormat::RGBA8,
            Vec::new(),
        )))
    }

    /// Broad (not literally exhaustive) sample of `NodeType`, covering every
    /// variant that has an arm in `get_ua_property` plus a spread of variants
    /// that have none, so the catch-all arms get exercised too.
    fn sample_node_types() -> Vec<NodeType> {
        use crate::dom::NodeType as NT;
        vec![
            // matched arms
            NT::Html, NT::Head, NT::Body, NT::Div, NT::P, NT::Main, NT::Header,
            NT::Footer, NT::Section, NT::Article, NT::Aside, NT::Nav,
            NT::H1, NT::H2, NT::H3, NT::H4, NT::H5, NT::H6,
            NT::Ul, NT::Ol, NT::Li, NT::Dl, NT::Dt, NT::Dd,
            NT::Span, NT::A, NT::Strong, NT::Em, NT::B, NT::I, NT::U, NT::Small,
            NT::Code, NT::Kbd, NT::Samp, NT::Sub, NT::Sup,
            NT::Pre, NT::BlockQuote, NT::Hr,
            NT::Table, NT::THead, NT::TBody, NT::TFoot, NT::Tr, NT::Th, NT::Td,
            NT::Caption, NT::ColGroup, NT::Col,
            NT::Form, NT::Input, NT::Button, NT::Select, NT::TextArea, NT::Label,
            NT::Title, NT::Script, NT::Style, NT::Link,
            NT::Br, NT::Video, NT::Audio, NT::Canvas, NT::Svg, NT::VirtualView,
            NT::SelectOption, NT::OptGroup,
            NT::Abbr, NT::Cite, NT::Del, NT::Ins, NT::Mark, NT::Q, NT::Dfn,
            NT::Var, NT::Time, NT::Data, NT::Wbr, NT::Bdi, NT::Bdo,
            NT::Rp, NT::Rt, NT::Rtc, NT::Ruby,
            NT::FieldSet, NT::Figure, NT::FigCaption, NT::Details, NT::Summary,
            NT::Dialog, NT::Menu, NT::Dir,
            // unmatched arms (must fall through to the catch-alls)
            NT::Address, NT::Legend, NT::Output, NT::Progress, NT::Meter,
            NT::DataList, NT::MenuItem, NT::S, NT::Big, NT::Acronym,
            NT::Object, NT::Param, NT::Embed, NT::Source, NT::Track, NT::Map,
            NT::Area, NT::Meta, NT::Base, NT::Before, NT::After, NT::Marker,
            NT::Placeholder, NT::SvgG, NT::SvgPath, NT::SvgRect,
            NT::SvgText(AzString::from("svg-text")),
            // payload-carrying variants
            text_node(""),
            text_node("hello"),
            icon_node("home"),
            image_node(),
        ]
    }

    fn all_os() -> Vec<OsCondition> {
        vec![
            OsCondition::Any,
            OsCondition::Apple,
            OsCondition::MacOS,
            OsCondition::IOS,
            OsCondition::Linux,
            OsCondition::Windows,
            OsCondition::Android,
            OsCondition::Web,
        ]
    }

    fn all_themes() -> Vec<ThemeCondition> {
        vec![
            ThemeCondition::Light,
            ThemeCondition::Dark,
            ThemeCondition::Custom(AzString::from("neon")),
            ThemeCondition::SystemPreferred,
        ]
    }

    fn ctx(os: OsCondition, theme: ThemeCondition) -> DynamicSelectorContext {
        DynamicSelectorContext {
            os,
            theme,
            ..DynamicSelectorContext::default()
        }
    }

    const CLASSIC_LIGHT_THUMB: ColorU = ColorU { r: 193, g: 193, b: 193, a: 255 };
    const CLASSIC_LIGHT_TRACK: ColorU = ColorU { r: 241, g: 241, b: 241, a: 255 };

    fn custom_color(thumb: ColorU, track: ColorU) -> StyleScrollbarColor {
        StyleScrollbarColor::Custom(ScrollbarColorCustom { thumb, track })
    }

    /// Extract the `(thumb, track)` pair, panicking if the property is not a
    /// `Custom` scrollbar color.
    fn unwrap_custom(c: StyleScrollbarColor) -> (ColorU, ColorU) {
        match c {
            StyleScrollbarColor::Custom(c) => (c.thumb, c.track),
            StyleScrollbarColor::Auto => panic!("expected a Custom scrollbar color, got Auto"),
        }
    }

    fn display_of(nt: &NodeType) -> LayoutDisplay {
        match get_ua_property(nt, CssPropertyType::Display) {
            Some(CssProperty::Display(CssPropertyValue::Exact(d))) => *d,
            other => panic!("{nt:?}: expected an exact display value, got {other:?}"),
        }
    }

    fn font_size_em(nt: &NodeType) -> f32 {
        match get_ua_property(nt, CssPropertyType::FontSize) {
            Some(CssProperty::FontSize(CssPropertyValue::Exact(fs))) => {
                assert_eq!(fs.inner.metric, SizeMetric::Em, "{nt:?}: font-size must be em-relative");
                fs.inner.number.get()
            }
            other => panic!("{nt:?}: expected an exact em font-size, got {other:?}"),
        }
    }

    // ==================================================================
    // get_ua_property — table-wide invariants
    // ==================================================================

    /// The single most important invariant of the lookup table: the property
    /// that comes back must be *the property that was asked for*. A copy-paste
    /// slip in the ~200-arm table (e.g. `(H1, MarginBottom) => &MARGIN_TOP_...`)
    /// would silently mis-style elements; nothing else in the codebase checks it.
    #[test]
    fn returned_property_always_has_the_requested_type() {
        for nt in sample_node_types() {
            for pt in CssPropertyType::ALL {
                if let Some(prop) = get_ua_property(&nt, *pt) {
                    assert_eq!(
                        prop.get_type(),
                        *pt,
                        "get_ua_property({nt:?}, {pt:?}) returned a {:?} property",
                        prop.get_type()
                    );
                }
            }
        }
    }

    #[test]
    fn full_cross_product_never_panics_and_is_deterministic() {
        for nt in sample_node_types() {
            for pt in CssPropertyType::ALL {
                let a = get_ua_property(&nt, *pt);
                let b = get_ua_property(&nt, *pt);
                match (a, b) {
                    (Some(a), Some(b)) => assert!(
                        core::ptr::eq(a, b),
                        "{nt:?}/{pt:?}: repeated lookups must hand back the same static"
                    ),
                    (None, None) => {}
                    _ => panic!("{nt:?}/{pt:?}: lookup is not deterministic"),
                }
            }
        }
    }

    /// Documented contract: the `(_, Display)` catch-all means *every* node type
    /// resolves a display value, so layout never sees a node without one.
    #[test]
    fn display_resolves_for_every_node_type() {
        for nt in sample_node_types() {
            assert!(
                get_ua_property(&nt, CssPropertyType::Display).is_some(),
                "{nt:?} has no default display"
            );
        }
    }

    #[test]
    fn unknown_elements_default_to_inline_display() {
        // Per CSS spec, unknown/custom elements are inline.
        for nt in [NodeType::Address, NodeType::Legend, NodeType::Meter, NodeType::SvgPath] {
            assert_eq!(display_of(&nt), LayoutDisplay::Inline, "{nt:?}");
        }
    }

    /// `cursor` is deliberately defined for exactly three node types; anything
    /// else must return `None` so the cursor-resolution walk can inherit.
    #[test]
    fn cursor_default_exists_only_for_button_textarea_and_text() {
        for nt in sample_node_types() {
            let has_cursor = get_ua_property(&nt, CssPropertyType::Cursor).is_some();
            let expected = matches!(nt, NodeType::Button | NodeType::TextArea | NodeType::Text(_));
            assert_eq!(has_cursor, expected, "{nt:?}: unexpected cursor default");
        }
    }

    // ==================================================================
    // get_ua_property — payload-carrying node types (unicode / huge / empty)
    // ==================================================================

    #[test]
    fn text_node_defaults_are_independent_of_the_payload() {
        let huge = "🦀".repeat(100_000);
        let payloads: Vec<String> = vec![
            String::new(),
            "\0".into(),
            "\u{202E}\u{200B}\u{FEFF}".into(), // RTL override, ZWSP, BOM
            "مرحبا بالعالم".into(),
            "🇩🇪👨‍👩‍👧‍👦".into(),
            "\u{FFFD}".into(),
            huge,
        ];

        for p in payloads {
            let nt = text_node(&p);
            assert_eq!(
                display_of(&nt),
                LayoutDisplay::Inline,
                "text node display must not depend on its content"
            );
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::Cursor),
                Some(&CURSOR_TEXT),
                "text node cursor must not depend on its content"
            );
            // Text nodes define no box properties of their own.
            assert!(get_ua_property(&nt, CssPropertyType::Width).is_none());
            assert!(get_ua_property(&nt, CssPropertyType::Height).is_none());
            assert!(get_ua_property(&nt, CssPropertyType::MarginTop).is_none());
        }
    }

    #[test]
    fn icon_and_image_nodes_are_inline_block_regardless_of_payload() {
        let huge_name = "x".repeat(50_000);
        let names: [&str; 4] = ["", "home", "🏠", huge_name.as_str()];
        for name in names {
            assert_eq!(display_of(&icon_node(name)), LayoutDisplay::InlineBlock, "icon {name:?}");
        }
        assert_eq!(display_of(&image_node()), LayoutDisplay::InlineBlock);
    }

    // ==================================================================
    // get_ua_property — specific, load-bearing defaults
    // ==================================================================

    /// Regression guard for the 2026-06-02 DIAG revert documented in the table:
    /// `(Html, Height) => HEIGHT_100_PERCENT` is commented out on purpose. If it
    /// comes back without the jump-table dispatch fix, children wrongly inherit
    /// `height: 100%`.
    #[test]
    fn html_has_no_default_height() {
        assert_eq!(get_ua_property(&NodeType::Html, CssPropertyType::Display), Some(&DISPLAY_BLOCK));
        assert!(
            get_ua_property(&NodeType::Html, CssPropertyType::Height).is_none(),
            "the (Html, Height) arm is intentionally disabled — see the DIAG note"
        );
    }

    /// `body { margin: 8px }` (Chrome UA), and crucially *no* width/height:
    /// giving body a size would break percentage sizing of its children.
    #[test]
    fn body_has_8px_margins_and_no_intrinsic_size() {
        assert_eq!(display_of(&NodeType::Body), LayoutDisplay::Block);
        assert_eq!(get_ua_property(&NodeType::Body, CssPropertyType::MarginTop), Some(&MARGIN_TOP_8PX));
        assert_eq!(get_ua_property(&NodeType::Body, CssPropertyType::MarginBottom), Some(&MARGIN_BOTTOM_8PX));
        assert_eq!(get_ua_property(&NodeType::Body, CssPropertyType::MarginLeft), Some(&MARGIN_LEFT_8PX));
        assert_eq!(get_ua_property(&NodeType::Body, CssPropertyType::MarginRight), Some(&MARGIN_RIGHT_8PX));
        assert!(get_ua_property(&NodeType::Body, CssPropertyType::Width).is_none());
        assert!(get_ua_property(&NodeType::Body, CssPropertyType::Height).is_none());
    }

    /// Block elements must have `width: auto`, not `width: 100%` — the comment in
    /// the table calls this out as critical for flexbox (100% defeats flex-grow).
    #[test]
    fn block_elements_have_no_default_width() {
        for nt in [NodeType::Div, NodeType::P, NodeType::Section, NodeType::Main, NodeType::VirtualView] {
            assert_eq!(display_of(&nt), LayoutDisplay::Block, "{nt:?}");
            assert!(
                get_ua_property(&nt, CssPropertyType::Width).is_none(),
                "{nt:?} must be width:auto so it can flex-grow"
            );
        }
    }

    #[test]
    fn div_defines_only_a_display_default() {
        for pt in CssPropertyType::ALL {
            let got = get_ua_property(&NodeType::Div, *pt);
            if *pt == CssPropertyType::Display {
                assert!(got.is_some());
            } else {
                assert!(got.is_none(), "Div should not define a UA default for {pt:?}");
            }
        }
    }

    #[test]
    fn metadata_elements_are_display_none() {
        for nt in [NodeType::Head, NodeType::Title, NodeType::Script, NodeType::Style, NodeType::Link] {
            assert_eq!(display_of(&nt), LayoutDisplay::None, "{nt:?} must not render");
        }
    }

    #[test]
    fn heading_font_sizes_are_strictly_decreasing() {
        let sizes: Vec<f32> = [NodeType::H1, NodeType::H2, NodeType::H3, NodeType::H4, NodeType::H5, NodeType::H6]
            .iter()
            .map(font_size_em)
            .collect();

        // Chrome UA values — also verifies `const_em_fractional(1, 5)` really
        // encodes 1.5 (and not 1.05), which the digit-count encoding makes subtle.
        let expected = [2.0_f32, 1.5, 1.17, 1.0, 0.83, 0.67];
        for (i, (got, want)) in sizes.iter().zip(expected.iter()).enumerate() {
            assert!(
                (got - want).abs() < 1e-4,
                "H{} font-size: got {got}em, want {want}em",
                i + 1
            );
        }
        for w in sizes.windows(2) {
            assert!(w[0] > w[1], "heading font sizes must strictly decrease, got {sizes:?}");
        }
    }

    #[test]
    fn headings_are_bold_blocks_that_avoid_page_breaks() {
        for nt in [NodeType::H1, NodeType::H2, NodeType::H3, NodeType::H4, NodeType::H5, NodeType::H6] {
            assert_eq!(display_of(&nt), LayoutDisplay::Block, "{nt:?}");
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::FontWeight),
                Some(&FONT_WEIGHT_BOLD),
                "{nt:?}"
            );
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::BreakInside),
                Some(&BREAK_INSIDE_AVOID),
                "{nt:?}"
            );
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::BreakAfter),
                Some(&BREAK_AFTER_AVOID),
                "{nt:?}"
            );
            // Both margins must exist and be em-relative (they scale with font-size).
            for pt in [CssPropertyType::MarginTop, CssPropertyType::MarginBottom] {
                assert!(get_ua_property(&nt, pt).is_some(), "{nt:?} is missing {pt:?}");
            }
        }
    }

    /// Tables *can* break across pages; their rows/headers/footers cannot. The
    /// table comments say so explicitly, so lock the asymmetry in.
    #[test]
    fn tables_may_break_across_pages_but_rows_may_not() {
        assert!(get_ua_property(&NodeType::Table, CssPropertyType::BreakInside).is_none());
        assert!(get_ua_property(&NodeType::TBody, CssPropertyType::BreakInside).is_none());
        for nt in [NodeType::THead, NodeType::TFoot, NodeType::Tr] {
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::BreakInside),
                Some(&BREAK_INSIDE_AVOID),
                "{nt:?}"
            );
        }
    }

    #[test]
    fn table_display_types_are_not_crossed() {
        assert_eq!(display_of(&NodeType::Table), LayoutDisplay::Table);
        assert_eq!(display_of(&NodeType::THead), LayoutDisplay::TableHeaderGroup);
        assert_eq!(display_of(&NodeType::TBody), LayoutDisplay::TableRowGroup);
        assert_eq!(display_of(&NodeType::TFoot), LayoutDisplay::TableFooterGroup);
        assert_eq!(display_of(&NodeType::Tr), LayoutDisplay::TableRow);
        assert_eq!(display_of(&NodeType::Th), LayoutDisplay::TableCell);
        assert_eq!(display_of(&NodeType::Td), LayoutDisplay::TableCell);
        assert_eq!(display_of(&NodeType::Caption), LayoutDisplay::TableCaption);
        assert_eq!(display_of(&NodeType::ColGroup), LayoutDisplay::TableColumnGroup);
        assert_eq!(display_of(&NodeType::Col), LayoutDisplay::TableColumn);
    }

    #[test]
    fn table_cells_have_1px_padding_on_all_four_sides() {
        for nt in [NodeType::Th, NodeType::Td] {
            assert_eq!(get_ua_property(&nt, CssPropertyType::PaddingTop), Some(&PADDING_TOP_1PX), "{nt:?}");
            assert_eq!(get_ua_property(&nt, CssPropertyType::PaddingBottom), Some(&PADDING_BOTTOM_1PX), "{nt:?}");
            assert_eq!(get_ua_property(&nt, CssPropertyType::PaddingLeft), Some(&PADDING_LEFT_1PX), "{nt:?}");
            assert_eq!(get_ua_property(&nt, CssPropertyType::PaddingRight), Some(&PADDING_RIGHT_1PX), "{nt:?}");
            assert_eq!(get_ua_property(&nt, CssPropertyType::VerticalAlign), Some(&VERTICAL_ALIGN_MIDDLE), "{nt:?}");
        }
        // Only <th> is centered + bold.
        assert_eq!(get_ua_property(&NodeType::Th, CssPropertyType::TextAlign), Some(&TEXT_ALIGN_CENTER));
        assert_eq!(get_ua_property(&NodeType::Th, CssPropertyType::FontWeight), Some(&FONT_WEIGHT_BOLD));
        assert!(get_ua_property(&NodeType::Td, CssPropertyType::TextAlign).is_none());
        assert!(get_ua_property(&NodeType::Td, CssPropertyType::FontWeight).is_none());
    }

    /// A button's border is symmetric. Crossed sides (e.g. `BorderLeftWidth`
    /// answered with the *top* static) would render an asymmetric button, so
    /// check that each side carries the value the table promises.
    #[test]
    fn button_border_is_symmetric_on_all_four_sides() {
        let widths = [
            (CssPropertyType::BorderTopWidth, &BUTTON_BORDER_TOP_WIDTH),
            (CssPropertyType::BorderBottomWidth, &BUTTON_BORDER_BOTTOM_WIDTH),
            (CssPropertyType::BorderLeftWidth, &BUTTON_BORDER_LEFT_WIDTH),
            (CssPropertyType::BorderRightWidth, &BUTTON_BORDER_RIGHT_WIDTH),
        ];
        for (pt, want) in widths {
            assert_eq!(get_ua_property(&NodeType::Button, pt), Some(want), "{pt:?}");
        }

        let styles = [
            (CssPropertyType::BorderTopStyle, &BUTTON_BORDER_TOP_STYLE),
            (CssPropertyType::BorderBottomStyle, &BUTTON_BORDER_BOTTOM_STYLE),
            (CssPropertyType::BorderLeftStyle, &BUTTON_BORDER_LEFT_STYLE),
            (CssPropertyType::BorderRightStyle, &BUTTON_BORDER_RIGHT_STYLE),
        ];
        for (pt, want) in styles {
            assert_eq!(get_ua_property(&NodeType::Button, pt), Some(want), "{pt:?}");
        }

        let colors = [
            (CssPropertyType::BorderTopColor, &BUTTON_BORDER_TOP_COLOR),
            (CssPropertyType::BorderBottomColor, &BUTTON_BORDER_BOTTOM_COLOR),
            (CssPropertyType::BorderLeftColor, &BUTTON_BORDER_LEFT_COLOR),
            (CssPropertyType::BorderRightColor, &BUTTON_BORDER_RIGHT_COLOR),
        ];
        for (pt, want) in colors {
            assert_eq!(get_ua_property(&NodeType::Button, pt), Some(want), "{pt:?}");
        }

        assert_eq!(display_of(&NodeType::Button), LayoutDisplay::InlineBlock);
        assert_eq!(get_ua_property(&NodeType::Button, CssPropertyType::Cursor), Some(&CURSOR_POINTER));
    }

    /// `<hr>` draws its line from the *border*, not from a height — height must
    /// be exactly 0px, and the width exactly 100%.
    #[test]
    fn hr_line_comes_from_the_border_not_from_height() {
        match get_ua_property(&NodeType::Hr, CssPropertyType::Height) {
            Some(CssProperty::Height(CssPropertyValue::Exact(LayoutHeight::Px(pv)))) => {
                assert_eq!(pv.metric, SizeMetric::Px);
                assert!((pv.number.get() - 0.0).abs() < 1e-6, "hr height must be 0px");
            }
            other => panic!("hr height: {other:?}"),
        }
        match get_ua_property(&NodeType::Hr, CssPropertyType::Width) {
            Some(CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::Px(pv)))) => {
                assert_eq!(pv.metric, SizeMetric::Percent);
                assert!((pv.number.get() - 100.0).abs() < 1e-4, "hr width must be 100%");
            }
            other => panic!("hr width: {other:?}"),
        }
        assert_eq!(get_ua_property(&NodeType::Hr, CssPropertyType::BorderTopStyle), Some(&BORDER_TOP_STYLE_INSET));
        assert_eq!(get_ua_property(&NodeType::Hr, CssPropertyType::BorderTopWidth), Some(&BORDER_TOP_WIDTH_1PX));
        assert_eq!(get_ua_property(&NodeType::Hr, CssPropertyType::BorderTopColor), Some(&BORDER_TOP_COLOR_GRAY));
    }

    #[test]
    fn list_containers_reset_the_counter_and_reserve_marker_space() {
        for (nt, marker) in [
            (NodeType::Ul, &LIST_STYLE_TYPE_DISC),
            (NodeType::Ol, &LIST_STYLE_TYPE_DECIMAL),
        ] {
            assert_eq!(display_of(&nt), LayoutDisplay::Block, "{nt:?}");
            assert_eq!(get_ua_property(&nt, CssPropertyType::ListStyleType), Some(marker), "{nt:?}");
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::CounterReset),
                Some(&COUNTER_RESET_LIST_ITEM),
                "{nt:?} must reset the list-item counter"
            );
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::PaddingLeft),
                Some(&PADDING_INLINE_START_40PX),
                "{nt:?}"
            );
        }
        assert_eq!(display_of(&NodeType::Li), LayoutDisplay::ListItem);
    }

    #[test]
    fn inline_emphasis_and_link_defaults() {
        assert_eq!(get_ua_property(&NodeType::A, CssPropertyType::TextDecoration), Some(&TEXT_DECORATION_UNDERLINE));
        assert_eq!(get_ua_property(&NodeType::U, CssPropertyType::TextDecoration), Some(&TEXT_DECORATION_UNDERLINE));
        assert_eq!(get_ua_property(&NodeType::Strong, CssPropertyType::FontWeight), Some(&FONT_WEIGHT_BOLDER));
        assert_eq!(get_ua_property(&NodeType::B, CssPropertyType::FontWeight), Some(&FONT_WEIGHT_BOLDER));
        // <em>/<i> are italic via font-style, which the UA table does not define.
        assert!(get_ua_property(&NodeType::Em, CssPropertyType::FontWeight).is_none());
        assert!(get_ua_property(&NodeType::I, CssPropertyType::FontWeight).is_none());
    }

    // ==================================================================
    // const scrollbar helpers — numeric round-trips / boundaries
    // ==================================================================

    #[test]
    fn scrollbar_fade_delay_round_trips_every_boundary() {
        for ms in [0_u32, 1, 2, 299, 300, 500, u32::from(u16::MAX), i32::MAX as u32, u32::MAX - 1, u32::MAX] {
            match scrollbar_fade_delay(ms) {
                CssProperty::ScrollbarFadeDelay(CssPropertyValue::Exact(d)) => {
                    assert_eq!(d.ms, ms, "fade-delay must round-trip losslessly");
                }
                other => panic!("scrollbar_fade_delay({ms}) built a {other:?}"),
            }
        }
    }

    #[test]
    fn scrollbar_fade_duration_round_trips_every_boundary() {
        for ms in [0_u32, 1, 150, 200, u32::from(u16::MAX), i32::MAX as u32, u32::MAX - 1, u32::MAX] {
            match scrollbar_fade_duration(ms) {
                CssProperty::ScrollbarFadeDuration(CssPropertyValue::Exact(d)) => {
                    assert_eq!(d.ms, ms, "fade-duration must round-trip losslessly");
                }
                other => panic!("scrollbar_fade_duration({ms}) built a {other:?}"),
            }
        }
    }

    /// `u32::MAX` in a `const` item: if either helper ever grew an arithmetic
    /// conversion (ms → ns, ms → seconds), this fails to *compile* rather than
    /// silently wrapping in release and panicking in debug.
    #[test]
    fn scrollbar_fade_helpers_are_const_evaluable_at_u32_max() {
        const MAX_DELAY: CssProperty = scrollbar_fade_delay(u32::MAX);
        const MAX_DURATION: CssProperty = scrollbar_fade_duration(u32::MAX);
        const ZERO_DELAY: CssProperty = scrollbar_fade_delay(0);

        assert_eq!(MAX_DELAY, scrollbar_fade_delay(u32::MAX));
        assert_eq!(MAX_DURATION, scrollbar_fade_duration(u32::MAX));
        assert_eq!(ZERO_DELAY, scrollbar_fade_delay(0));
    }

    /// The two helpers take the same `u32` and differ only in the wrapper type —
    /// exactly the shape a copy-paste bug likes. Assert they stay distinct.
    #[test]
    fn fade_delay_and_fade_duration_produce_distinct_property_types() {
        assert_eq!(scrollbar_fade_delay(42).get_type(), CssPropertyType::ScrollbarFadeDelay);
        assert_eq!(scrollbar_fade_duration(42).get_type(), CssPropertyType::ScrollbarFadeDuration);
        assert_ne!(scrollbar_fade_delay(42), scrollbar_fade_duration(42));
    }

    /// A `0` delay means "never fades" (per the `ScrollbarFadeDelay` docs), so it
    /// must be stored as a literal zero, not as a sentinel.
    #[test]
    fn zero_fade_delay_and_duration_are_literal_zero() {
        assert_eq!(
            scrollbar_fade_delay(0),
            CssProperty::ScrollbarFadeDelay(CssPropertyValue::Exact(ScrollbarFadeDelay::ZERO))
        );
        assert_eq!(
            scrollbar_fade_duration(0),
            CssProperty::ScrollbarFadeDuration(CssPropertyValue::Exact(ScrollbarFadeDuration::ZERO))
        );
    }

    #[test]
    fn scrollbar_color_never_swaps_thumb_and_track() {
        let cases = [
            (ColorU { r: 1, g: 2, b: 3, a: 4 }, ColorU { r: 5, g: 6, b: 7, a: 8 }),
            (ColorU { r: 0, g: 0, b: 0, a: 0 }, ColorU { r: 255, g: 255, b: 255, a: 255 }),
            (ColorU { r: 255, g: 255, b: 255, a: 255 }, ColorU::TRANSPARENT),
            (ColorU::TRANSPARENT, ColorU::TRANSPARENT),
        ];
        for (thumb, track) in cases {
            match scrollbar_color(thumb, track) {
                CssProperty::ScrollbarColor(CssPropertyValue::Exact(StyleScrollbarColor::Custom(c))) => {
                    assert_eq!(c.thumb, thumb, "thumb was not preserved");
                    assert_eq!(c.track, track, "track was not preserved (arguments swapped?)");
                }
                other => panic!("scrollbar_color built a {other:?}"),
            }
        }
    }

    #[test]
    fn scrollbar_width_and_visibility_round_trip_every_variant() {
        for w in [LayoutScrollbarWidth::Auto, LayoutScrollbarWidth::Thin, LayoutScrollbarWidth::None] {
            match scrollbar_width(w) {
                CssProperty::ScrollbarWidth(CssPropertyValue::Exact(got)) => assert_eq!(got, w),
                other => panic!("scrollbar_width({w:?}) built a {other:?}"),
            }
        }
        for v in [
            ScrollbarVisibilityMode::Always,
            ScrollbarVisibilityMode::WhenScrolling,
            ScrollbarVisibilityMode::Auto,
        ] {
            match scrollbar_visibility(v) {
                CssProperty::ScrollbarVisibility(CssPropertyValue::Exact(got)) => assert_eq!(got, v),
                other => panic!("scrollbar_visibility({v:?}) built a {other:?}"),
            }
        }
    }

    // ==================================================================
    // UA_SCROLLBAR_CSS — table shape invariants
    // ==================================================================

    /// `evaluate_ua_scrollbar_css` matches on exactly five property kinds and
    /// silently drops everything else via `_ => {}`. A sixth property added to
    /// the table would therefore never take effect — fail loudly here instead.
    #[test]
    fn table_contains_only_property_kinds_the_evaluator_understands() {
        let understood = [
            CssPropertyType::ScrollbarColor,
            CssPropertyType::ScrollbarWidth,
            CssPropertyType::ScrollbarVisibility,
            CssPropertyType::ScrollbarFadeDelay,
            CssPropertyType::ScrollbarFadeDuration,
        ];
        for (i, entry) in UA_SCROLLBAR_CSS.iter().enumerate() {
            let ty = entry.property.get_type();
            assert!(
                understood.contains(&ty),
                "UA_SCROLLBAR_CSS[{i}] is a {ty:?}, which evaluate_ua_scrollbar_css ignores"
            );
        }
    }

    /// The evaluator only reads `CssPropertyValue::Exact`; an `Auto`/`Inherit`
    /// entry would be skipped without a trace.
    #[test]
    fn every_table_entry_carries_an_exact_value() {
        for (i, entry) in UA_SCROLLBAR_CSS.iter().enumerate() {
            let is_exact = matches!(
                &entry.property,
                CssProperty::ScrollbarColor(CssPropertyValue::Exact(_))
                    | CssProperty::ScrollbarWidth(CssPropertyValue::Exact(_))
                    | CssProperty::ScrollbarVisibility(CssPropertyValue::Exact(_))
                    | CssProperty::ScrollbarFadeDelay(CssPropertyValue::Exact(_))
                    | CssProperty::ScrollbarFadeDuration(CssPropertyValue::Exact(_))
            );
            assert!(is_exact, "UA_SCROLLBAR_CSS[{i}] is not an Exact value: {:?}", entry.property);
        }
    }

    /// The documented guarantee ("unconditional fallback entries … guarantee that
    /// every field resolves") plus the ordering rule it depends on: under
    /// first-match-wins, an unconditional entry that is *not* last for its
    /// property type would make every rule after it dead code.
    #[test]
    fn each_property_type_has_exactly_one_unconditional_entry_and_it_is_last() {
        for ty in [
            CssPropertyType::ScrollbarColor,
            CssPropertyType::ScrollbarWidth,
            CssPropertyType::ScrollbarVisibility,
            CssPropertyType::ScrollbarFadeDelay,
            CssPropertyType::ScrollbarFadeDuration,
        ] {
            let of_type: Vec<&CssPropertyWithConditions> = UA_SCROLLBAR_CSS
                .iter()
                .filter(|e| e.property.get_type() == ty)
                .collect();
            assert!(!of_type.is_empty(), "{ty:?} has no entry at all");

            let unconditional: Vec<usize> = of_type
                .iter()
                .enumerate()
                .filter(|(_, e)| e.apply_if.as_slice().is_empty())
                .map(|(i, _)| i)
                .collect();

            assert_eq!(
                unconditional.len(),
                1,
                "{ty:?} must have exactly one unconditional fallback, found {}",
                unconditional.len()
            );
            assert_eq!(
                unconditional[0],
                of_type.len() - 1,
                "{ty:?}: the unconditional fallback must come last, otherwise the \
                 {} rule(s) after it are dead under first-match-wins",
                of_type.len() - 1 - unconditional[0]
            );
        }
    }

    // ==================================================================
    // evaluate_ua_scrollbar_css
    // ==================================================================

    #[test]
    fn default_context_resolves_to_the_classic_light_scrollbar() {
        let r = evaluate_ua_scrollbar_css(&DynamicSelectorContext::default());
        assert_eq!(r.width, LayoutScrollbarWidth::Auto);
        assert_eq!(r.visibility, ScrollbarVisibilityMode::Always);
        assert_eq!(r.fade_delay.ms, 0);
        assert_eq!(r.fade_duration.ms, 0);
        assert_eq!(unwrap_custom(r.color), (CLASSIC_LIGHT_THUMB, CLASSIC_LIGHT_TRACK));
    }

    #[test]
    fn per_os_and_theme_defaults_are_what_the_table_promises() {
        let cases: Vec<(OsCondition, ThemeCondition, LayoutScrollbarWidth, ScrollbarVisibilityMode, u32, u32, StyleScrollbarColor)> = vec![
            (
                OsCondition::MacOS, ThemeCondition::Dark,
                LayoutScrollbarWidth::Thin, ScrollbarVisibilityMode::WhenScrolling, 500, 200,
                custom_color(ColorU { r: 180, g: 180, b: 180, a: 200 }, ColorU { r: 40, g: 40, b: 40, a: 80 }),
            ),
            (
                OsCondition::MacOS, ThemeCondition::Light,
                LayoutScrollbarWidth::Thin, ScrollbarVisibilityMode::WhenScrolling, 500, 200,
                custom_color(ColorU { r: 80, g: 80, b: 80, a: 200 }, ColorU { r: 200, g: 200, b: 200, a: 80 }),
            ),
            (
                OsCondition::Windows, ThemeCondition::Dark,
                LayoutScrollbarWidth::Auto, ScrollbarVisibilityMode::Always, 0, 0,
                custom_color(ColorU { r: 110, g: 110, b: 110, a: 255 }, ColorU { r: 32, g: 32, b: 32, a: 255 }),
            ),
            (
                OsCondition::Windows, ThemeCondition::Light,
                LayoutScrollbarWidth::Auto, ScrollbarVisibilityMode::Always, 0, 0,
                custom_color(ColorU { r: 130, g: 130, b: 130, a: 255 }, ColorU { r: 241, g: 241, b: 241, a: 255 }),
            ),
            (
                OsCondition::IOS, ThemeCondition::Dark,
                LayoutScrollbarWidth::Thin, ScrollbarVisibilityMode::WhenScrolling, 500, 200,
                custom_color(ColorU { r: 255, g: 255, b: 255, a: 100 }, ColorU::TRANSPARENT),
            ),
            (
                OsCondition::IOS, ThemeCondition::Light,
                LayoutScrollbarWidth::Thin, ScrollbarVisibilityMode::WhenScrolling, 500, 200,
                custom_color(ColorU { r: 0, g: 0, b: 0, a: 100 }, ColorU::TRANSPARENT),
            ),
            (
                OsCondition::Android, ThemeCondition::Dark,
                LayoutScrollbarWidth::Thin, ScrollbarVisibilityMode::WhenScrolling, 300, 150,
                custom_color(ColorU { r: 255, g: 255, b: 255, a: 77 }, ColorU::TRANSPARENT),
            ),
            (
                OsCondition::Android, ThemeCondition::Light,
                LayoutScrollbarWidth::Thin, ScrollbarVisibilityMode::WhenScrolling, 300, 150,
                custom_color(ColorU { r: 0, g: 0, b: 0, a: 77 }, ColorU::TRANSPARENT),
            ),
            (
                // Linux has no OS-specific colour rule: dark falls through to the
                // generic dark entry.
                OsCondition::Linux, ThemeCondition::Dark,
                LayoutScrollbarWidth::Auto, ScrollbarVisibilityMode::Always, 0, 0,
                custom_color(ColorU { r: 100, g: 100, b: 100, a: 255 }, ColorU { r: 45, g: 45, b: 45, a: 255 }),
            ),
            (
                OsCondition::Linux, ThemeCondition::Light,
                LayoutScrollbarWidth::Auto, ScrollbarVisibilityMode::Always, 0, 0,
                custom_color(CLASSIC_LIGHT_THUMB, CLASSIC_LIGHT_TRACK),
            ),
            (
                OsCondition::Web, ThemeCondition::Dark,
                LayoutScrollbarWidth::Auto, ScrollbarVisibilityMode::Always, 0, 0,
                custom_color(ColorU { r: 100, g: 100, b: 100, a: 255 }, ColorU { r: 45, g: 45, b: 45, a: 255 }),
            ),
        ];

        for (os, theme, width, visibility, delay, duration, color) in cases {
            let r = evaluate_ua_scrollbar_css(&ctx(os, theme.clone()));
            assert_eq!(r.width, width, "{os:?}/{theme:?}: width");
            assert_eq!(r.visibility, visibility, "{os:?}/{theme:?}: visibility");
            assert_eq!(r.fade_delay.ms, delay, "{os:?}/{theme:?}: fade-delay");
            assert_eq!(r.fade_duration.ms, duration, "{os:?}/{theme:?}: fade-duration");
            assert_eq!(r.color, color, "{os:?}/{theme:?}: color");
        }
    }

    /// `match_theme` compares by equality (except when the *condition* is
    /// `SystemPreferred`), so a context theme of `Custom(..)` / `SystemPreferred`
    /// matches no `@theme` rule at all — every such context must still resolve a
    /// colour, via the unconditional fallback.
    #[test]
    fn unrecognised_context_themes_fall_back_instead_of_failing() {
        for theme in [ThemeCondition::Custom(AzString::from("")), ThemeCondition::Custom(AzString::from("🎨")), ThemeCondition::SystemPreferred] {
            // OS-conditioned properties still apply — only the theme rules miss.
            let r = evaluate_ua_scrollbar_css(&ctx(OsCondition::MacOS, theme.clone()));
            assert_eq!(r.width, LayoutScrollbarWidth::Thin, "{theme:?}");
            assert_eq!(r.visibility, ScrollbarVisibilityMode::WhenScrolling, "{theme:?}");
            assert_eq!(
                unwrap_custom(r.color),
                (CLASSIC_LIGHT_THUMB, CLASSIC_LIGHT_TRACK),
                "{theme:?}: must fall back to the unconditional colour"
            );
        }
    }

    /// `OsCondition::Apple` is condition-side sugar (it *matches* MacOS/IOS); as a
    /// *context* value it equals neither, so an `Apple` context gets the generic
    /// defaults. `DynamicSelectorContext::from_system_style` never produces it, so
    /// this pins down the (slightly surprising) behaviour rather than blessing it.
    #[test]
    fn apple_as_a_context_os_matches_no_macos_or_ios_rule() {
        let r = evaluate_ua_scrollbar_css(&ctx(OsCondition::Apple, ThemeCondition::Dark));
        assert_eq!(r.width, LayoutScrollbarWidth::Auto);
        assert_eq!(r.visibility, ScrollbarVisibilityMode::Always);
        assert_eq!(r.fade_delay.ms, 0);
        assert_eq!(r.fade_duration.ms, 0);
    }

    /// Overlay scrollbars are a package deal: `thin` ⇔ `when-scrolling` ⇔ a
    /// non-zero fade delay ⇔ a non-zero fade duration. A per-OS rule added to one
    /// group but forgotten in another would produce an overlay scrollbar that
    /// never fades (or a classic one that does).
    #[test]
    fn overlay_scrollbar_fields_stay_consistent_across_every_os_and_theme() {
        for os in all_os() {
            for theme in all_themes() {
                let r = evaluate_ua_scrollbar_css(&ctx(os, theme.clone()));
                let thin = r.width == LayoutScrollbarWidth::Thin;
                let overlay = r.visibility == ScrollbarVisibilityMode::WhenScrolling;

                assert_eq!(thin, overlay, "{os:?}/{theme:?}: thin/when-scrolling disagree");
                assert_eq!(
                    overlay,
                    r.fade_delay.ms > 0,
                    "{os:?}/{theme:?}: an overlay scrollbar needs a fade delay"
                );
                assert_eq!(
                    overlay,
                    r.fade_duration.ms > 0,
                    "{os:?}/{theme:?}: an overlay scrollbar needs a fade duration"
                );
                // The table only ever supplies Custom colours.
                assert!(
                    matches!(r.color, StyleScrollbarColor::Custom(_)),
                    "{os:?}/{theme:?}: colour resolved to Auto"
                );
            }
        }
    }

    /// The evaluator `break`s early once all five fields are filled. Cross-check
    /// it against a straight first-match-wins scan with no early exit: the two
    /// must agree for every context, or the optimisation changed the semantics.
    #[test]
    fn early_break_does_not_change_the_first_match_result() {
        for os in all_os() {
            for theme in all_themes() {
                let c = ctx(os, theme.clone());
                let got = evaluate_ua_scrollbar_css(&c);

                let mut want_color = None;
                let mut want_width = None;
                let mut want_vis = None;
                let mut want_delay = None;
                let mut want_dur = None;
                for entry in UA_SCROLLBAR_CSS.iter().filter(|e| e.matches(&c)) {
                    match &entry.property {
                        CssProperty::ScrollbarColor(CssPropertyValue::Exact(v)) => {
                            if want_color.is_none() {
                                want_color = Some(*v);
                            }
                        }
                        CssProperty::ScrollbarWidth(CssPropertyValue::Exact(v)) => {
                            if want_width.is_none() {
                                want_width = Some(*v);
                            }
                        }
                        CssProperty::ScrollbarVisibility(CssPropertyValue::Exact(v)) => {
                            if want_vis.is_none() {
                                want_vis = Some(*v);
                            }
                        }
                        CssProperty::ScrollbarFadeDelay(CssPropertyValue::Exact(v)) => {
                            if want_delay.is_none() {
                                want_delay = Some(*v);
                            }
                        }
                        CssProperty::ScrollbarFadeDuration(CssPropertyValue::Exact(v)) => {
                            if want_dur.is_none() {
                                want_dur = Some(*v);
                            }
                        }
                        _ => {}
                    }
                }

                let label = alloc::format!("{os:?}/{theme:?}");
                assert_eq!(Some(got.color), want_color, "{label}: color");
                assert_eq!(Some(got.width), want_width, "{label}: width");
                assert_eq!(Some(got.visibility), want_vis, "{label}: visibility");
                assert_eq!(Some(got.fade_delay), want_delay, "{label}: fade-delay");
                assert_eq!(Some(got.fade_duration), want_dur, "{label}: fade-duration");
            }
        }
    }

    /// Degenerate / hostile context values (NaN, infinities, empty and huge
    /// strings) must not panic, and every field must still resolve.
    #[test]
    fn degenerate_context_values_do_not_panic() {
        let hostile = [
            (f32::NAN, f32::NAN),
            (0.0, 0.0),
            (-0.0, -1.0),
            (f32::INFINITY, f32::NEG_INFINITY),
            (f32::MAX, f32::MIN),
            (f32::MIN_POSITIVE, f32::EPSILON),
        ];

        for (w, h) in hostile {
            let c = DynamicSelectorContext {
                os: OsCondition::MacOS,
                theme: ThemeCondition::Dark,
                de_version: u32::MAX,
                viewport_width: w,
                viewport_height: h,
                container_width: h,
                container_height: w,
                language: AzString::from(""),
                ..DynamicSelectorContext::default()
            };
            let r = evaluate_ua_scrollbar_css(&c);
            // macOS/dark rules are OS+theme-only, so viewport garbage cannot
            // perturb them.
            assert_eq!(r.width, LayoutScrollbarWidth::Thin, "viewport {w}x{h}");
            assert_eq!(r.fade_delay.ms, 500, "viewport {w}x{h}");
            assert!(matches!(r.color, StyleScrollbarColor::Custom(_)), "viewport {w}x{h}");
        }
    }

    #[test]
    fn evaluate_is_deterministic() {
        for os in all_os() {
            for theme in all_themes() {
                let c = ctx(os, theme.clone());
                let a = evaluate_ua_scrollbar_css(&c);
                let b = evaluate_ua_scrollbar_css(&c);
                assert_eq!(a.color, b.color, "{os:?}/{theme:?}");
                assert_eq!(a.width, b.width, "{os:?}/{theme:?}");
                assert_eq!(a.visibility, b.visibility, "{os:?}/{theme:?}");
                assert_eq!(a.fade_delay, b.fade_delay, "{os:?}/{theme:?}");
                assert_eq!(a.fade_duration, b.fade_duration, "{os:?}/{theme:?}");
            }
        }
    }
}
