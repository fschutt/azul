//! User-Agent Default Stylesheet for Azul
//!
//! This module provides the default CSS styling that browsers apply to HTML elements
//! before any author stylesheets are processed. It ensures consistent baseline behavior
//! across all Azul applications.
//!
//! # Purpose
//!
//! The user-agent stylesheet serves several critical functions:
//!
//! 1. **Prevents Layout Collapse**: Ensures root elements (`<html>`, `<body>`) have
//!    default dimensions so that percentage-based child sizing can work correctly.
//!
//! 2. **Establishes Display Types**: Defines the default `display` property for all
//!    HTML elements (e.g., `<div>` is `block`, `<span>` is `inline`).
//!
//! 3. **Provides Baseline Typography**: Sets reasonable defaults for font sizes,
//!    margins, and text styling for headings, paragraphs, and other text elements.
//!
//! 4. **Normalizes Browser Behavior**: Incorporates principles from normalize.css to
//!    provide consistent rendering across different platforms.
//!
//! # Implementation Details
//!
//! Unlike traditional user-agent stylesheets that are parsed at runtime, this module
//! uses compile-time constants. Each CSS property is represented as a strongly-typed
//! Rust constant, eliminating parsing overhead and providing type safety.
//!
//! The API uses a lookup function that takes:
//! - `NodeType`: The type of DOM element (e.g., `Body`, `H1`, `Div`)
//! - `CssPropertyType`: The specific CSS property being queried (e.g., `Width`, `Display`)
//!
//! And returns an `Option<CssProperty>` with the default value, or `None` if no
//! default is defined for that combination.
//!
//! # Example
//!
//! ```ignore
//! use azul_core::ua_css::get_ua_property;
//! use azul_core::dom::NodeType;
//! use azul_css::props::property::CssPropertyType;
//!
//! // Get the default width for <body>
//! if let Some(width) = get_ua_property(NodeType::Body, CssPropertyType::Width) {
//!     // width is CssProperty::Width(LayoutWidthValue::Exact(LayoutWidth::Percent(...)))
//! }
//! ```
//!
//! # Licensing
//!
//! This user-agent stylesheet integrates principles from normalize.css v8.0.1:
//!
//! - **normalize.css License**: MIT License
//!   Copyright (c) Nicolas Gallagher and Jonathan Neal
//!   https://github.com/necolas/normalize.css
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

use crate::dom::NodeType;
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{length::PercentageValue, pixel::PixelValue, StyleFontSize, font::StyleFontWeight},
        layout::{
            display::LayoutDisplay,
            dimensions::{LayoutWidth, LayoutHeight},
            spacing::{
                LayoutMarginTop, LayoutMarginBottom, LayoutMarginLeft, LayoutMarginRight,
                LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight,
            },
        },
        style::{StyleTextAlign, StyleVerticalAlign, lists::StyleListStyleType},
        property::{CssProperty, CssPropertyType},
    },
};

/// 100% width
static WIDTH_100_PERCENT: CssProperty = CssProperty::Width(CssPropertyValue::Exact(
    LayoutWidth::Px(PixelValue::const_percent(100)),
));

/// 100% height
static HEIGHT_100_PERCENT: CssProperty = CssProperty::Height(CssPropertyValue::Exact(
    LayoutHeight::Px(PixelValue::const_percent(100)),
));

/// display: block
static DISPLAY_BLOCK: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::Block,
));

/// display: inline
static DISPLAY_INLINE: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::Inline,
));

/// display: inline-block
static DISPLAY_INLINE_BLOCK: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::InlineBlock,
));

/// display: none
static DISPLAY_NONE: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::None,
));

/// display: table
static DISPLAY_TABLE: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::Table,
));

/// display: table-row
static DISPLAY_TABLE_ROW: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::TableRow,
));

/// display: table-cell
static DISPLAY_TABLE_CELL: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::TableCell,
));

/// display: table-header-group
static DISPLAY_TABLE_HEADER_GROUP: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::TableHeaderGroup,
));

/// display: table-row-group
static DISPLAY_TABLE_ROW_GROUP: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::TableRowGroup,
));

/// display: table-footer-group
static DISPLAY_TABLE_FOOTER_GROUP: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::TableFooterGroup,
));

/// display: table-caption
static DISPLAY_TABLE_CAPTION: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::TableCaption,
));

/// display: table-column-group
static DISPLAY_TABLE_COLUMN_GROUP: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::TableColumnGroup,
));

/// display: table-column
static DISPLAY_TABLE_COLUMN: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::TableColumn,
));

/// display: list-item
static DISPLAY_LIST_ITEM: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::ListItem,
));

/// margin-top: 0
static MARGIN_TOP_ZERO: CssProperty = CssProperty::MarginTop(CssPropertyValue::Exact(
    LayoutMarginTop {
        inner: PixelValue::const_px(0),
    },
));

/// margin-bottom: 0
static MARGIN_BOTTOM_ZERO: CssProperty = CssProperty::MarginBottom(CssPropertyValue::Exact(
    LayoutMarginBottom {
        inner: PixelValue::const_px(0),
    },
));

/// margin-left: 0
static MARGIN_LEFT_ZERO: CssProperty = CssProperty::MarginLeft(CssPropertyValue::Exact(
    LayoutMarginLeft {
        inner: PixelValue::const_px(0),
    },
));

/// margin-right: 0
static MARGIN_RIGHT_ZERO: CssProperty = CssProperty::MarginRight(CssPropertyValue::Exact(
    LayoutMarginRight {
        inner: PixelValue::const_px(0),
    },
));

// Chrome User-Agent Stylesheet: body { margin: 8px; }
/// margin-top: 8px (Chrome UA default for body)
static MARGIN_TOP_8PX: CssProperty = CssProperty::MarginTop(CssPropertyValue::Exact(
    LayoutMarginTop {
        inner: PixelValue::const_px(8),
    },
));

/// margin-bottom: 8px (Chrome UA default for body)
static MARGIN_BOTTOM_8PX: CssProperty = CssProperty::MarginBottom(CssPropertyValue::Exact(
    LayoutMarginBottom {
        inner: PixelValue::const_px(8),
    },
));

/// margin-left: 8px (Chrome UA default for body)
static MARGIN_LEFT_8PX: CssProperty = CssProperty::MarginLeft(CssPropertyValue::Exact(
    LayoutMarginLeft {
        inner: PixelValue::const_px(8),
    },
));

/// margin-right: 8px (Chrome UA default for body)
static MARGIN_RIGHT_8PX: CssProperty = CssProperty::MarginRight(CssPropertyValue::Exact(
    LayoutMarginRight {
        inner: PixelValue::const_px(8),
    },
));

/// font-size: 2em (for H1)
static FONT_SIZE_2EM: CssProperty = CssProperty::FontSize(CssPropertyValue::Exact(
    StyleFontSize {
        inner: PixelValue::const_em(2),
    },
));

/// font-size: 1.5em (for H2)
static FONT_SIZE_1_5EM: CssProperty = CssProperty::FontSize(CssPropertyValue::Exact(
    StyleFontSize {
        inner: PixelValue::const_em_fractional(1, 5),
    },
));

/// font-size: 1.17em (for H3)
static FONT_SIZE_1_17EM: CssProperty = CssProperty::FontSize(CssPropertyValue::Exact(
    StyleFontSize {
        inner: PixelValue::const_em_fractional(1, 17),
    },
));

/// font-size: 1em (for H4)
static FONT_SIZE_1EM: CssProperty = CssProperty::FontSize(CssPropertyValue::Exact(
    StyleFontSize {
        inner: PixelValue::const_em(1),
    },
));

/// font-size: 0.83em (for H5)
static FONT_SIZE_0_83EM: CssProperty = CssProperty::FontSize(CssPropertyValue::Exact(
    StyleFontSize {
        inner: PixelValue::const_em_fractional(0, 83),
    },
));

/// font-size: 0.67em (for H6)
static FONT_SIZE_0_67EM: CssProperty = CssProperty::FontSize(CssPropertyValue::Exact(
    StyleFontSize {
        inner: PixelValue::const_em_fractional(0, 67),
    },
));

/// margin-top: 1em (for P)
static MARGIN_TOP_1EM: CssProperty = CssProperty::MarginTop(CssPropertyValue::Exact(
    LayoutMarginTop {
        inner: PixelValue::const_em(1),
    },
));

/// margin-bottom: 1em (for P)
static MARGIN_BOTTOM_1EM: CssProperty = CssProperty::MarginBottom(CssPropertyValue::Exact(
    LayoutMarginBottom {
        inner: PixelValue::const_em(1),
    },
));

/// margin-top: 0.67em (for H1)
static MARGIN_TOP_0_67EM: CssProperty = CssProperty::MarginTop(CssPropertyValue::Exact(
    LayoutMarginTop {
        inner: PixelValue::const_em_fractional(0, 67),
    },
));

/// margin-bottom: 0.67em (for H1)
static MARGIN_BOTTOM_0_67EM: CssProperty = CssProperty::MarginBottom(CssPropertyValue::Exact(
    LayoutMarginBottom {
        inner: PixelValue::const_em_fractional(0, 67),
    },
));

/// margin-top: 0.83em (for H2)
static MARGIN_TOP_0_83EM: CssProperty = CssProperty::MarginTop(CssPropertyValue::Exact(
    LayoutMarginTop {
        inner: PixelValue::const_em_fractional(0, 83),
    },
));

/// margin-bottom: 0.83em (for H2)
static MARGIN_BOTTOM_0_83EM: CssProperty = CssProperty::MarginBottom(CssPropertyValue::Exact(
    LayoutMarginBottom {
        inner: PixelValue::const_em_fractional(0, 83),
    },
));

/// margin-top: 1.33em (for H4)
static MARGIN_TOP_1_33EM: CssProperty = CssProperty::MarginTop(CssPropertyValue::Exact(
    LayoutMarginTop {
        inner: PixelValue::const_em_fractional(1, 33),
    },
));

/// margin-bottom: 1.33em (for H4)
static MARGIN_BOTTOM_1_33EM: CssProperty = CssProperty::MarginBottom(CssPropertyValue::Exact(
    LayoutMarginBottom {
        inner: PixelValue::const_em_fractional(1, 33),
    },
));

/// margin-top: 1.67em (for H5)
static MARGIN_TOP_1_67EM: CssProperty = CssProperty::MarginTop(CssPropertyValue::Exact(
    LayoutMarginTop {
        inner: PixelValue::const_em_fractional(1, 67),
    },
));

/// margin-bottom: 1.67em (for H5)
static MARGIN_BOTTOM_1_67EM: CssProperty = CssProperty::MarginBottom(CssPropertyValue::Exact(
    LayoutMarginBottom {
        inner: PixelValue::const_em_fractional(1, 67),
    },
));

/// margin-top: 2.33em (for H6)
static MARGIN_TOP_2_33EM: CssProperty = CssProperty::MarginTop(CssPropertyValue::Exact(
    LayoutMarginTop {
        inner: PixelValue::const_em_fractional(2, 33),
    },
));

/// margin-bottom: 2.33em (for H6)
static MARGIN_BOTTOM_2_33EM: CssProperty = CssProperty::MarginBottom(CssPropertyValue::Exact(
    LayoutMarginBottom {
        inner: PixelValue::const_em_fractional(2, 33),
    },
));

/// font-weight: bold (for headings)
static FONT_WEIGHT_BOLD: CssProperty = CssProperty::FontWeight(CssPropertyValue::Exact(
    StyleFontWeight::Bold,
));

/// font-weight: bolder
static FONT_WEIGHT_BOLDER: CssProperty = CssProperty::FontWeight(CssPropertyValue::Exact(
    StyleFontWeight::Bolder,
));

// Table cell padding - Chrome UA CSS default: 1px
static PADDING_1PX: CssProperty = CssProperty::PaddingTop(CssPropertyValue::Exact(LayoutPaddingTop {
    inner: PixelValue::const_px(1),
}));

static PADDING_TOP_1PX: CssProperty = CssProperty::PaddingTop(CssPropertyValue::Exact(LayoutPaddingTop {
    inner: PixelValue::const_px(1),
}));

static PADDING_BOTTOM_1PX: CssProperty = CssProperty::PaddingBottom(CssPropertyValue::Exact(LayoutPaddingBottom {
    inner: PixelValue::const_px(1),
}));

static PADDING_LEFT_1PX: CssProperty = CssProperty::PaddingLeft(CssPropertyValue::Exact(LayoutPaddingLeft {
    inner: PixelValue::const_px(1),
}));

static PADDING_RIGHT_1PX: CssProperty = CssProperty::PaddingRight(CssPropertyValue::Exact(LayoutPaddingRight {
    inner: PixelValue::const_px(1),
}));

/// text-align: center (for th elements)
static TEXT_ALIGN_CENTER: CssProperty = CssProperty::TextAlign(CssPropertyValue::Exact(
    StyleTextAlign::Center,
));

/// vertical-align: center (maps to CSS vertical-align: middle for table elements)
static VERTICAL_ALIGN_CENTER: CssProperty = CssProperty::VerticalAlign(CssPropertyValue::Exact(
    StyleVerticalAlign::Center,
));

/// list-style-type: disc (default for <ul>)
static LIST_STYLE_TYPE_DISC: CssProperty = CssProperty::ListStyleType(CssPropertyValue::Exact(
    StyleListStyleType::Disc,
));

/// list-style-type: decimal (default for <ol>)
static LIST_STYLE_TYPE_DECIMAL: CssProperty = CssProperty::ListStyleType(CssPropertyValue::Exact(
    StyleListStyleType::Decimal,
));

// TODO: Uncomment when TextDecoration is implemented in azul-css
// const TEXT_DECORATION_UNDERLINE: CssProperty = CssProperty::TextDecoration(
//     StyleTextDecorationValue::Exact(StyleTextDecoration::Underline),
// ));

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
///
/// # Examples
///
/// ```ignore
/// use azul_core::ua_css::get_ua_property;
/// use azul_core::dom::NodeType;
/// use azul_css::props::property::CssPropertyType;
///
/// // Get default width for <body> - returns 100%
/// let width = get_ua_property(NodeType::Body, CssPropertyType::Width);
/// assert!(width.is_some());
///
/// // Get default display for <div> - returns block
/// let display = get_ua_property(NodeType::Div, CssPropertyType::Display);
/// assert!(display.is_some());
///
/// // Get undefined property - returns None
/// let undefined = get_ua_property(NodeType::Span, CssPropertyType::Width);
/// assert!(undefined.is_none());
/// ```
pub fn get_ua_property(node_type: &NodeType, property_type: CssPropertyType) -> Option<&'static CssProperty> {
    use NodeType as NT;
    use CssPropertyType as PT;

    let result = match (node_type, property_type) {
        // HTML Element
        // (Html, PT::LineHeight) => Some(&LINE_HEIGHT_1_15),

        // Body Element - CRITICAL for preventing layout collapse
        (NT::Body, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Body, PT::Width) => Some(&WIDTH_100_PERCENT),
        // (NT::Body, PT::Height) => Some(&HEIGHT_100_PERCENT),
        (NT::Body, PT::MarginTop) => Some(&MARGIN_TOP_8PX),
        (NT::Body, PT::MarginBottom) => Some(&MARGIN_BOTTOM_8PX),
        (NT::Body, PT::MarginLeft) => Some(&MARGIN_LEFT_8PX),
        (NT::Body, PT::MarginRight) => Some(&MARGIN_RIGHT_8PX),

        // Block-level Elements
        (NT::Div, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Div, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::P, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::P, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::P, PT::MarginTop) => Some(&MARGIN_TOP_1EM),
        (NT::P, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1EM),
        (NT::Main, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Main, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Header, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Header, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Footer, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Footer, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Section, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Section, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Article, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Article, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Aside, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Aside, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Nav, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Nav, PT::Width) => Some(&WIDTH_100_PERCENT),

        // Headings - Chrome UA CSS values
        (NT::H1, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::H1, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::H1, PT::FontSize) => Some(&FONT_SIZE_2EM),
        (NT::H1, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::H1, PT::MarginTop) => Some(&MARGIN_TOP_0_67EM),
        (NT::H1, PT::MarginBottom) => Some(&MARGIN_BOTTOM_0_67EM),

        (NT::H2, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::H2, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::H2, PT::FontSize) => Some(&FONT_SIZE_1_5EM),
        (NT::H2, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::H2, PT::MarginTop) => Some(&MARGIN_TOP_0_83EM),
        (NT::H2, PT::MarginBottom) => Some(&MARGIN_BOTTOM_0_83EM),

        (NT::H3, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::H3, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::H3, PT::FontSize) => Some(&FONT_SIZE_1_17EM),
        (NT::H3, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::H3, PT::MarginTop) => Some(&MARGIN_TOP_1EM),
        (NT::H3, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1EM),

        (NT::H4, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::H4, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::H4, PT::FontSize) => Some(&FONT_SIZE_1EM),
        (NT::H4, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::H4, PT::MarginTop) => Some(&MARGIN_TOP_1_33EM),
        (NT::H4, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1_33EM),

        (NT::H5, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::H5, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::H5, PT::FontSize) => Some(&FONT_SIZE_0_83EM),
        (NT::H5, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::H5, PT::MarginTop) => Some(&MARGIN_TOP_1_67EM),
        (NT::H5, PT::MarginBottom) => Some(&MARGIN_BOTTOM_1_67EM),

        (NT::H6, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::H6, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::H6, PT::FontSize) => Some(&FONT_SIZE_0_67EM),
        (NT::H6, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::H6, PT::MarginTop) => Some(&MARGIN_TOP_2_33EM),
        (NT::H6, PT::MarginBottom) => Some(&MARGIN_BOTTOM_2_33EM),

        // Lists
        (NT::Ul, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Ul, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Ul, PT::ListStyleType) => Some(&LIST_STYLE_TYPE_DISC),
        (NT::Ol, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Ol, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Ol, PT::ListStyleType) => Some(&LIST_STYLE_TYPE_DECIMAL),
        (NT::Li, PT::Display) => Some(&DISPLAY_LIST_ITEM),
        (NT::Dl, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Dl, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Dt, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Dt, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Dd, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Dd, PT::Width) => Some(&WIDTH_100_PERCENT),

        // Inline Elements
        (NT::Span, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::A, PT::Display) => Some(&DISPLAY_INLINE),
        // (A, PT::TextDecoration) => Some(&TEXT_DECORATION_UNDERLINE),
        (NT::Strong, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Strong, PT::FontWeight) => Some(&FONT_WEIGHT_BOLDER),
        (NT::Em, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::B, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::B, PT::FontWeight) => Some(&FONT_WEIGHT_BOLDER),
        (NT::I, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::U, PT::Display) => Some(&DISPLAY_INLINE),
        // (U, PT::TextDecoration) => Some(&TEXT_DECORATION_UNDERLINE),
        (NT::Small, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Code, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Kbd, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Samp, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Sub, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Sup, PT::Display) => Some(&DISPLAY_INLINE),

        // Text Content
        (NT::Pre, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Pre, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::BlockQuote, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::BlockQuote, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Hr, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Hr, PT::Width) => Some(&WIDTH_100_PERCENT),

        // Table Elements
        (NT::Table, PT::Display) => Some(&DISPLAY_TABLE),
        (NT::Table, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::THead, PT::Display) => Some(&DISPLAY_TABLE_HEADER_GROUP),
        (NT::THead, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_CENTER),
        (NT::TBody, PT::Display) => Some(&DISPLAY_TABLE_ROW_GROUP),
        (NT::TBody, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_CENTER),
        (NT::TFoot, PT::Display) => Some(&DISPLAY_TABLE_FOOTER_GROUP),
        (NT::TFoot, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_CENTER),
        (NT::Tr, PT::Display) => Some(&DISPLAY_TABLE_ROW),
        (NT::Tr, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_CENTER),
        (NT::Th, PT::Display) => Some(&DISPLAY_TABLE_CELL),
        (NT::Th, PT::TextAlign) => Some(&TEXT_ALIGN_CENTER),
        (NT::Th, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),
        (NT::Th, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_CENTER),
        (NT::Th, PT::PaddingTop) => Some(&PADDING_TOP_1PX),
        (NT::Th, PT::PaddingBottom) => Some(&PADDING_BOTTOM_1PX),
        (NT::Th, PT::PaddingLeft) => Some(&PADDING_LEFT_1PX),
        (NT::Th, PT::PaddingRight) => Some(&PADDING_RIGHT_1PX),
        (NT::Td, PT::Display) => Some(&DISPLAY_TABLE_CELL),
        (NT::Td, PT::VerticalAlign) => Some(&VERTICAL_ALIGN_CENTER),
        (NT::Td, PT::PaddingTop) => Some(&PADDING_TOP_1PX),
        (NT::Td, PT::PaddingBottom) => Some(&PADDING_BOTTOM_1PX),
        (NT::Td, PT::PaddingLeft) => Some(&PADDING_LEFT_1PX),
        (NT::Td, PT::PaddingRight) => Some(&PADDING_RIGHT_1PX),

        // Form Elements
        (NT::Form, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Form, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Input, PT::Display) => Some(&DISPLAY_INLINE_BLOCK),
        (NT::Button, PT::Display) => Some(&DISPLAY_INLINE_BLOCK),
        (NT::Select, PT::Display) => Some(&DISPLAY_INLINE_BLOCK),
        (NT::TextArea, PT::Display) => Some(&DISPLAY_INLINE_BLOCK),
        (NT::Label, PT::Display) => Some(&DISPLAY_INLINE),
        // Hidden Elements
        (NT::Head, PT::Display) => Some(&DISPLAY_NONE),
        (NT::Title, PT::Display) => Some(&DISPLAY_NONE),
        (NT::Script, PT::Display) => Some(&DISPLAY_NONE),
        (NT::Style, PT::Display) => Some(&DISPLAY_NONE),
        (NT::Link, PT::Display) => Some(&DISPLAY_NONE),

        // Special Elements
        (NT::Br, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Image(_), PT::Display) => Some(&DISPLAY_INLINE),

        // Media Elements
        (NT::Video, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Audio, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Canvas, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::Svg, PT::Display) => Some(&DISPLAY_INLINE),
        (NT::IFrame(_), PT::Display) => Some(&DISPLAY_INLINE),
        
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
        (NT::FieldSet, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::FieldSet, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Figure, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Figure, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::FigCaption, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::FigCaption, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Details, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Details, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Summary, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Summary, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Dialog, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Dialog, PT::Width) => Some(&WIDTH_100_PERCENT),
        
        // Table Caption
        (NT::Caption, PT::Display) => Some(&DISPLAY_TABLE_CAPTION),
        (NT::ColGroup, PT::Display) => Some(&DISPLAY_TABLE_COLUMN_GROUP),
        (NT::Col, PT::Display) => Some(&DISPLAY_TABLE_COLUMN),
        
        // Legacy/Deprecated Elements
        (NT::Menu, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Menu, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Dir, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Dir, PT::Width) => Some(&WIDTH_100_PERCENT),
        
        // Generic Container
        (NT::Html, PT::Display) => Some(&DISPLAY_BLOCK),
        (NT::Html, PT::Width) => Some(&WIDTH_100_PERCENT),
        (NT::Html, PT::Height) => Some(&HEIGHT_100_PERCENT),

        // Universal fallback for display property
        // Per CSS spec, unknown/custom elements should default to inline
        // Text nodes will be filtered out before this function is called
        (_, PT::Display) => Some(&DISPLAY_INLINE),

        // No default defined for other combinations
        _ => None,
    };
    
    // Debug output for Body and H1 elements
    if matches!(node_type, NT::Body | NT::H1) {
        println!("[UA_CSS] get_ua_property({:?}, {:?}) -> {:?}", 
            node_type, property_type, result.is_some());
        if let Some(prop) = result {
            println!("[UA_CSS]   Value: {:?}", prop);
        }
    }
    
    result
}
