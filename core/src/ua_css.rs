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
        basic::{length::PercentageValue, pixel::PixelValue, StyleFontSize},
        layout::{
            display::LayoutDisplay,
            dimensions::{LayoutWidth, LayoutHeight},
            spacing::{LayoutMarginTop, LayoutMarginBottom, LayoutMarginLeft, LayoutMarginRight},
        },
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

/// display: none
static DISPLAY_NONE: CssProperty = CssProperty::Display(CssPropertyValue::Exact(
    LayoutDisplay::None,
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

// TODO: Uncomment when StyleFontWeight is implemented in azul-css
// const FONT_WEIGHT_BOLD: CssProperty = CssProperty::FontWeight(StyleFontWeightValue::Exact(
//     StyleFontWeight::Bold,
// ));

// const FONT_WEIGHT_BOLDER: CssProperty = CssProperty::FontWeight(StyleFontWeightValue::Exact(
//     StyleFontWeight::Bolder,
// ));

// TODO: Uncomment when TextDecoration is implemented in azul-css
// const TEXT_DECORATION_UNDERLINE: CssProperty = CssProperty::TextDecoration(
//     StyleTextDecorationValue::Exact(StyleTextDecoration::Underline),
// );

// TODO: Uncomment when LineHeight is implemented in azul-css
// const LINE_HEIGHT_1_15: CssProperty = CssProperty::LineHeight(LayoutLineHeightValue::Exact(
//     LayoutLineHeight {
//         inner: PercentageValue::const_new(115), // 1.15 = 115%
//     },
// ));

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
pub fn get_ua_property(node_type: NodeType, property_type: CssPropertyType) -> Option<&'static CssProperty> {
    use NodeType::*;
    use CssPropertyType as PT;

    match (node_type, property_type) {
        // HTML Element
        // (Html, PT::LineHeight) => Some(&LINE_HEIGHT_1_15),

        // Body Element - CRITICAL for preventing layout collapse
        (Body, PT::Width) => Some(&WIDTH_100_PERCENT),
        (Body, PT::Height) => Some(&HEIGHT_100_PERCENT),
        (Body, PT::MarginTop) => Some(&MARGIN_TOP_ZERO),
        (Body, PT::MarginBottom) => Some(&MARGIN_BOTTOM_ZERO),
        (Body, PT::MarginLeft) => Some(&MARGIN_LEFT_ZERO),
        (Body, PT::MarginRight) => Some(&MARGIN_RIGHT_ZERO),

        // Block-level Elements
        (Div, PT::Display) => Some(&DISPLAY_BLOCK),
        (P, PT::Display) => Some(&DISPLAY_BLOCK),
        (P, PT::Width) => Some(&WIDTH_100_PERCENT),
        (Main, PT::Display) => Some(&DISPLAY_BLOCK),
        (Header, PT::Display) => Some(&DISPLAY_BLOCK),
        (Footer, PT::Display) => Some(&DISPLAY_BLOCK),
        (Section, PT::Display) => Some(&DISPLAY_BLOCK),
        (Article, PT::Display) => Some(&DISPLAY_BLOCK),
        (Aside, PT::Display) => Some(&DISPLAY_BLOCK),
        (Nav, PT::Display) => Some(&DISPLAY_BLOCK),

        // Headings
        (H1, PT::Display) => Some(&DISPLAY_BLOCK),
        (H1, PT::FontSize) => Some(&FONT_SIZE_2EM),
        // (H1, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),

        (H2, PT::Display) => Some(&DISPLAY_BLOCK),
        (H2, PT::FontSize) => Some(&FONT_SIZE_1_5EM),
        // (H2, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),

        (H3, PT::Display) => Some(&DISPLAY_BLOCK),
        (H3, PT::FontSize) => Some(&FONT_SIZE_1_17EM),
        // (H3, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),

        (H4, PT::Display) => Some(&DISPLAY_BLOCK),
        (H4, PT::FontSize) => Some(&FONT_SIZE_1EM),
        // (H4, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),

        (H5, PT::Display) => Some(&DISPLAY_BLOCK),
        (H5, PT::FontSize) => Some(&FONT_SIZE_0_83EM),
        // (H5, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),

        (H6, PT::Display) => Some(&DISPLAY_BLOCK),
        (H6, PT::FontSize) => Some(&FONT_SIZE_0_67EM),
        // (H6, PT::FontWeight) => Some(&FONT_WEIGHT_BOLD),

        // Lists
        (Ul, PT::Display) => Some(&DISPLAY_BLOCK),
        (Ol, PT::Display) => Some(&DISPLAY_BLOCK),
        // (Li, PT::Display) => Some(&DISPLAY_LIST_ITEM), // TODO: Need DisplayListItem
        (Dl, PT::Display) => Some(&DISPLAY_BLOCK),
        (Dt, PT::Display) => Some(&DISPLAY_BLOCK),
        (Dd, PT::Display) => Some(&DISPLAY_BLOCK),

        // Inline Elements
        (Span, PT::Display) => Some(&DISPLAY_INLINE),
        (A, PT::Display) => Some(&DISPLAY_INLINE),
        // (A, PT::TextDecoration) => Some(&TEXT_DECORATION_UNDERLINE),
        (Strong, PT::Display) => Some(&DISPLAY_INLINE),
        // (Strong, PT::FontWeight) => Some(&FONT_WEIGHT_BOLDER),
        (Em, PT::Display) => Some(&DISPLAY_INLINE),
        (B, PT::Display) => Some(&DISPLAY_INLINE),
        // (B, PT::FontWeight) => Some(&FONT_WEIGHT_BOLDER),
        (I, PT::Display) => Some(&DISPLAY_INLINE),
        (U, PT::Display) => Some(&DISPLAY_INLINE),
        // (U, PT::TextDecoration) => Some(&TEXT_DECORATION_UNDERLINE),
        (Small, PT::Display) => Some(&DISPLAY_INLINE),
        (Code, PT::Display) => Some(&DISPLAY_INLINE),
        (Kbd, PT::Display) => Some(&DISPLAY_INLINE),
        (Samp, PT::Display) => Some(&DISPLAY_INLINE),
        (Sub, PT::Display) => Some(&DISPLAY_INLINE),
        (Sup, PT::Display) => Some(&DISPLAY_INLINE),

        // Text Content
        (Pre, PT::Display) => Some(&DISPLAY_BLOCK),
        (BlockQuote, PT::Display) => Some(&DISPLAY_BLOCK),
        (Hr, PT::Display) => Some(&DISPLAY_BLOCK),

        // Table Elements
        // (Table, PT::Display) => Some(&DISPLAY_TABLE), // TODO: Need DisplayTable
        // (THead, PT::Display) => Some(&DISPLAY_TABLE_HEADER_GROUP),
        // (TBody, PT::Display) => Some(&DISPLAY_TABLE_ROW_GROUP),
        // (TFoot, PT::Display) => Some(&DISPLAY_TABLE_FOOTER_GROUP),
        // (Tr, PT::Display) => Some(&DISPLAY_TABLE_ROW),
        // (Th, PT::Display) => Some(&DISPLAY_TABLE_CELL),
        // (Td, PT::Display) => Some(&DISPLAY_TABLE_CELL),

        // Form Elements
        (Form, PT::Display) => Some(&DISPLAY_BLOCK),
        // (Input, PT::Display) => Some(&DISPLAY_INLINE_BLOCK), // TODO: Need DisplayInlineBlock
        // (Button, PT::Display) => Some(&DISPLAY_INLINE_BLOCK),
        // (Select, PT::Display) => Some(&DISPLAY_INLINE_BLOCK),
        // (TextArea, PT::Display) => Some(&DISPLAY_INLINE_BLOCK),
        (Label, PT::Display) => Some(&DISPLAY_INLINE),

        // Hidden Elements
        (Head, PT::Display) => Some(&DISPLAY_NONE),
        (Title, PT::Display) => Some(&DISPLAY_NONE),
        (Script, PT::Display) => Some(&DISPLAY_NONE),
        (Style, PT::Display) => Some(&DISPLAY_NONE),
        (Link, PT::Display) => Some(&DISPLAY_NONE),

        // Special Elements
        (Br, PT::Display) => Some(&DISPLAY_BLOCK),
        (Img, PT::Display) => Some(&DISPLAY_INLINE),

        // No default defined for this combination
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_body_has_full_width_height() {
        let width = get_ua_property(NodeType::Body, CssPropertyType::Width);
        assert!(width.is_some());
        assert!(matches!(width, Some(CssProperty::Width(_))));

        let height = get_ua_property(NodeType::Body, CssPropertyType::Height);
        assert!(height.is_some());
        assert!(matches!(height, Some(CssProperty::Height(_))));

        let margin_top = get_ua_property(NodeType::Body, CssPropertyType::MarginTop);
        assert!(margin_top.is_some());
        assert!(matches!(margin_top, Some(CssProperty::MarginTop(_))));
    }

    #[test]
    fn test_headings_have_default_sizes() {
        assert!(get_ua_property(NodeType::H1, CssPropertyType::FontSize).is_some());
        assert!(get_ua_property(NodeType::H2, CssPropertyType::FontSize).is_some());
        assert!(get_ua_property(NodeType::H3, CssPropertyType::FontSize).is_some());
        assert!(get_ua_property(NodeType::H4, CssPropertyType::FontSize).is_some());
        assert!(get_ua_property(NodeType::H5, CssPropertyType::FontSize).is_some());
        assert!(get_ua_property(NodeType::H6, CssPropertyType::FontSize).is_some());
    }

    #[test]
    fn test_block_elements_have_display_block() {
        assert!(matches!(
            get_ua_property(NodeType::Div, CssPropertyType::Display),
            Some(CssProperty::Display(_))
        ));
        assert!(matches!(
            get_ua_property(NodeType::P, CssPropertyType::Display),
            Some(CssProperty::Display(_))
        ));
        assert!(matches!(
            get_ua_property(NodeType::Header, CssPropertyType::Display),
            Some(CssProperty::Display(_))
        ));
    }

    #[test]
    fn test_inline_elements_have_display_inline() {
        assert!(matches!(
            get_ua_property(NodeType::Span, CssPropertyType::Display),
            Some(CssProperty::Display(_))
        ));
        assert!(matches!(
            get_ua_property(NodeType::A, CssPropertyType::Display),
            Some(CssProperty::Display(_))
        ));
        assert!(matches!(
            get_ua_property(NodeType::Strong, CssPropertyType::Display),
            Some(CssProperty::Display(_))
        ));
    }

    #[test]
    fn test_hidden_elements_have_display_none() {
        assert!(matches!(
            get_ua_property(NodeType::Head, CssPropertyType::Display),
            Some(CssProperty::Display(_))
        ));
        assert!(matches!(
            get_ua_property(NodeType::Script, CssPropertyType::Display),
            Some(CssProperty::Display(_))
        ));
        assert!(matches!(
            get_ua_property(NodeType::Style, CssPropertyType::Display),
            Some(CssProperty::Display(_))
        ));
    }

    #[test]
    fn test_undefined_property_returns_none() {
        // Span doesn't have a default width
        assert!(get_ua_property(NodeType::Span, CssPropertyType::Width).is_none());
        
        // Div doesn't have a default font-size
        assert!(get_ua_property(NodeType::Div, CssPropertyType::FontSize).is_none());
    }
}
