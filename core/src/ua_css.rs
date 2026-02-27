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
//! This user-agent stylesheet integrates principles from normalize.css v8.0.1:
//!
//! - **normalize.css License**: MIT License Copyright (c) Nicolas Gallagher and
//    Jonathan Neal https://github.com/necolas/normalize.css
//!
//! The normalize.css project is licensed under the MIT License, which permits
//! commercial use, modification, distribution, and private use. The full license
//! text is as follows:
//!
//! ```text
//! MIT License
//!
//! Copyright (c) Nicolas Gallagher and Jonathan Neal
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.
//! ```
//!
//! This implementation is NOT a direct copy of normalize.css but incorporates its
//! principles and approach. The Azul project's overall license applies to this
//! implementation.
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
            font::StyleFontWeight, length::PercentageValue, pixel::PixelValue, ColorU,
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
static PADDING_1PX: CssProperty =
    CssProperty::PaddingTop(CssPropertyValue::Exact(LayoutPaddingTop {
        inner: PixelValue::const_px(1),
    }));

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

/// break-before: page
/// Forces a page break before the element
static BREAK_BEFORE_PAGE: CssProperty = CssProperty::break_before(PageBreak::Page);

/// break-after: page
/// Forces a page break after the element
static BREAK_AFTER_PAGE: CssProperty = CssProperty::break_after(PageBreak::Page);

/// break-before: avoid
/// Avoids a page break before the element
static BREAK_BEFORE_AVOID: CssProperty = CssProperty::break_before(PageBreak::Avoid);

/// break-after: avoid
/// Avoids a page break after the element (useful for headings)
static BREAK_AFTER_AVOID: CssProperty = CssProperty::break_after(PageBreak::Avoid);

/// padding-inline-start: 40px (default for <li>)
///
/// Creates space for list markers in the inline-start direction (left in LTR, right in RTL)
/// padding-inline-start: 40px for list items per CSS Lists Module Level 3
/// Applied to <li> items to create gutter space for ::marker pseudo-elements
///
/// NOTE: This should be on the list items, not the container, because:
///
/// 1. ::marker pseudo-elements are children of <li>, not <ul>/<ol>
/// 2. The marker needs to be positioned relative to the list item's content box
/// 3. Padding on <li> creates space between the marker and the text content
/// TODO: Change to PaddingInlineStart once logical property resolution is implemented
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

/*
const LINE_HEIGHT_1_15: CssProperty = CssProperty::LineHeight(LayoutLineHeightValue::Exact(
    LayoutLineHeight {
        inner: PercentageValue::const_new(115), // 1.15 = 115%
    },
));
*/

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
pub fn get_ua_property(
    node_type: &NodeType,
    property_type: CssPropertyType,
) -> Option<&'static CssProperty> {
    use CssPropertyType as PT;
    use NodeType as NT;

    let result = match (node_type, property_type) {
        // HTML Element
        // (Html, PT::LineHeight) => Some(&LINE_HEIGHT_1_15),

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
        (NT::Br, PT::Display) => Some(&DISPLAY_BLOCK),
        // Images are replaced elements - inline-block so they respect width/height
        (NT::Image(_), PT::Display) => Some(&DISPLAY_INLINE_BLOCK),

        // Media Elements
        (NT::Video, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Audio, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Canvas, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Svg, PT::Display) => Some(&DISPLAY_INLINE),
        // IFrame is a block-level replaced element (like div) — must be block
        // so it participates in flex layout (flex-grow, etc.)
        (NT::IFrame(_), PT::Display) => Some(&DISPLAY_BLOCK),

        // Icon Elements - inline-block so they have width/height but flow inline
        (NT::Icon(_), PT::Display) => Some(&DISPLAY_INLINE_BLOCK),

        // Form Input Elements (inline-block behavior approximated as inline)
        (NT::Input, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Button, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Select, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::TextArea, PT::Display) => Some(&DISPLAY_INLINE),
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
        (NT::Html, PT::Height) => Some(&HEIGHT_100_PERCENT),

        // Universal fallback for display property
        // Per CSS spec, unknown/custom elements should default to inline
        // Text nodes will be filtered out before this function is called
        (_, PT::Display) => Some(&DISPLAY_INLINE),

        // No default defined for other combinations
        _ => None,
    };

    result
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
//   @os macos @theme dark    { scrollbar-color: rgba(255,255,255,0.4) transparent; }
//   @os macos @theme light   { scrollbar-color: rgba(0,0,0,0.4) transparent; }
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
pub static UA_SCROLLBAR_CSS: &[CssPropertyWithConditions] = &[
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
    // macOS dark: semi-transparent white thumb, transparent track
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU { r: 255, g: 255, b: 255, a: 100 },
            ColorU::TRANSPARENT,
        ),
        &[DynamicSelector::Os(OsCondition::MacOS), DynamicSelector::Theme(ThemeCondition::Dark)],
    ),
    // macOS light: semi-transparent black thumb, transparent track
    CssPropertyWithConditions::with_single_condition(
        scrollbar_color(
            ColorU { r: 0, g: 0, b: 0, a: 100 },
            ColorU::TRANSPARENT,
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
pub struct ResolvedUaScrollbar {
    pub color: Option<StyleScrollbarColor>,
    pub width: Option<LayoutScrollbarWidth>,
    pub visibility: Option<ScrollbarVisibilityMode>,
    pub fade_delay: Option<ScrollbarFadeDelay>,
    pub fade_duration: Option<ScrollbarFadeDuration>,
}

/// Evaluate UA scrollbar CSS rules against a `DynamicSelectorContext`.
///
/// Iterates `UA_SCROLLBAR_CSS` and picks the first matching entry per
/// property type.  Returns `None` for a property if no rule matches
/// (should not happen since there are unconditional fallbacks).
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
        if color.is_some() && width.is_some() && visibility.is_some()
            && fade_delay.is_some() && fade_duration.is_some()
        {
            break;
        }
    }

    ResolvedUaScrollbar { color, width, visibility, fade_delay, fade_duration }
}
