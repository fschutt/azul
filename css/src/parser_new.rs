//! Main parsing entry points and utility functions

use crate::{
    error::CssParsingError,
    props::{get_css_key_map, CombinedCssPropertyType, CssProperty, CssPropertyType},
};
use alloc::{string::String, vec::Vec};

/// Parse a CSS property from a key-value pair
pub fn parse_css_property<'a>(
    key: &str,
    value: &'a str,
) -> Result<CssProperty, CssParsingError<'a>> {
    let key_map = get_css_key_map();

    if let Some(&prop_type) = key_map.normal_properties.get(key) {
        parse_property_by_type(prop_type, value)
    } else {
        Err(CssParsingError::InvalidValue(value))
    }
}

/// Parse a combined CSS property (shorthand)
pub fn parse_combined_css_property<'a>(
    key: &str,
    value: &'a str,
) -> Result<Vec<CssProperty>, CssParsingError<'a>> {
    let key_map = get_css_key_map();

    if let Some(&prop_type) = key_map.combined_properties.get(key) {
        parse_combined_property_by_type(prop_type, value)
    } else {
        Err(CssParsingError::InvalidValue(value))
    }
}

/// Parse a single property by its type
fn parse_property_by_type<'a>(
    prop_type: CssPropertyType,
    value: &'a str,
) -> Result<CssProperty, CssParsingError<'a>> {
    use CssProperty as Prop;
    use CssPropertyType::*;

    match prop_type {
        // Layout properties
        Display => Ok(Prop::Display(
            crate::props::layout::display::parse_layout_display(value)?,
        )),
        Width => Ok(Prop::Width(
            crate::props::layout::dimensions::parse_layout_width(value)?,
        )),
        Height => Ok(Prop::Height(
            crate::props::layout::dimensions::parse_layout_height(value)?,
        )),
        MinWidth => Ok(Prop::MinWidth(
            crate::props::layout::dimensions::parse_layout_min_width(value)?,
        )),
        MaxWidth => Ok(Prop::MaxWidth(
            crate::props::layout::dimensions::parse_layout_max_width(value)?,
        )),
        MinHeight => Ok(Prop::MinHeight(
            crate::props::layout::dimensions::parse_layout_min_height(value)?,
        )),
        MaxHeight => Ok(Prop::MaxHeight(
            crate::props::layout::dimensions::parse_layout_max_height(value)?,
        )),
        Position => Ok(Prop::Position(
            crate::props::layout::position::parse_layout_position(value)?,
        )),
        Top => Ok(Prop::Top(crate::props::layout::position::parse_layout_top(
            value,
        )?)),
        Right => Ok(Prop::Right(
            crate::props::layout::position::parse_layout_right(value)?,
        )),
        Bottom => Ok(Prop::Bottom(
            crate::props::layout::position::parse_layout_bottom(value)?,
        )),
        Left => Ok(Prop::Left(
            crate::props::layout::position::parse_layout_left(value)?,
        )),
        PaddingTop => Ok(Prop::PaddingTop(
            crate::props::layout::spacing::parse_layout_padding_top(value)?,
        )),
        PaddingRight => Ok(Prop::PaddingRight(
            crate::props::layout::spacing::parse_layout_padding_right(value)?,
        )),
        PaddingBottom => Ok(Prop::PaddingBottom(
            crate::props::layout::spacing::parse_layout_padding_bottom(value)?,
        )),
        PaddingLeft => Ok(Prop::PaddingLeft(
            crate::props::layout::spacing::parse_layout_padding_left(value)?,
        )),
        MarginTop => Ok(Prop::MarginTop(
            crate::props::layout::spacing::parse_layout_margin_top(value)?,
        )),
        MarginRight => Ok(Prop::MarginRight(
            crate::props::layout::spacing::parse_layout_margin_right(value)?,
        )),
        MarginBottom => Ok(Prop::MarginBottom(
            crate::props::layout::spacing::parse_layout_margin_bottom(value)?,
        )),
        MarginLeft => Ok(Prop::MarginLeft(
            crate::props::layout::spacing::parse_layout_margin_left(value)?,
        )),
        FlexDirection => Ok(Prop::FlexDirection(
            crate::props::layout::flex::parse_layout_flex_direction(value)?,
        )),
        FlexWrap => Ok(Prop::FlexWrap(
            crate::props::layout::flex::parse_layout_flex_wrap(value)?,
        )),
        JustifyContent => Ok(Prop::JustifyContent(
            crate::props::layout::flex::parse_layout_justify_content(value)?,
        )),
        AlignItems => Ok(Prop::AlignItems(
            crate::props::layout::flex::parse_layout_align_items(value)?,
        )),
        AlignContent => Ok(Prop::AlignContent(
            crate::props::layout::flex::parse_layout_align_content(value)?,
        )),
        FlexGrow => Ok(Prop::FlexGrow(
            crate::props::layout::flex::parse_layout_flex_grow(value)?,
        )),
        FlexShrink => Ok(Prop::FlexShrink(
            crate::props::layout::flex::parse_layout_flex_shrink(value)?,
        )),
        Overflow => Ok(Prop::Overflow(
            crate::props::layout::overflow::parse_layout_overflow(value)?,
        )),
        BoxSizing => Ok(Prop::BoxSizing(
            crate::props::layout::spacing::parse_layout_box_sizing(value)?,
        )),

        // Style properties
        TextColor => Ok(Prop::TextColor(
            crate::props::style::text::parse_style_text_color(value)?,
        )),
        FontSize => Ok(Prop::FontSize(
            crate::props::style::text::parse_style_font_size(value)?,
        )),
        TextAlign => Ok(Prop::TextAlign(
            crate::props::style::text::parse_style_text_align(value)?,
        )),
        LineHeight => Ok(Prop::LineHeight(
            crate::props::style::text::parse_style_line_height(value)?,
        )),
        LetterSpacing => Ok(Prop::LetterSpacing(
            crate::props::style::text::parse_style_letter_spacing(value)?,
        )),
    }
}

/// Parse a combined property by its type
fn parse_combined_property_by_type<'a>(
    prop_type: CombinedCssPropertyType,
    value: &'a str,
) -> Result<Vec<CssProperty>, CssParsingError<'a>> {
    use CombinedCssPropertyType::*;
    use CssProperty as Prop;

    match prop_type {
        Padding => {
            let padding = crate::props::layout::spacing::parse_layout_padding(value)?;
            Ok(vec![
                Prop::PaddingTop(crate::props::layout::spacing::LayoutPaddingTop {
                    inner: padding.top,
                }),
                Prop::PaddingRight(crate::props::layout::spacing::LayoutPaddingRight {
                    inner: padding.right,
                }),
                Prop::PaddingBottom(crate::props::layout::spacing::LayoutPaddingBottom {
                    inner: padding.bottom,
                }),
                Prop::PaddingLeft(crate::props::layout::spacing::LayoutPaddingLeft {
                    inner: padding.left,
                }),
            ])
        }
        Margin => {
            let margin = crate::props::layout::spacing::parse_layout_margin(value)?;
            Ok(vec![
                Prop::MarginTop(crate::props::layout::spacing::LayoutMarginTop {
                    inner: margin.top,
                }),
                Prop::MarginRight(crate::props::layout::spacing::LayoutMarginRight {
                    inner: margin.right,
                }),
                Prop::MarginBottom(crate::props::layout::spacing::LayoutMarginBottom {
                    inner: margin.bottom,
                }),
                Prop::MarginLeft(crate::props::layout::spacing::LayoutMarginLeft {
                    inner: margin.left,
                }),
            ])
        }
        _ => {
            // TODO: Implement other combined properties
            Err(CssParsingError::InvalidValue(value))
        }
    }
}

/// Utility function to parse parentheses-based CSS functions
pub fn parse_parentheses<'a>(
    input: &'a str,
    expected_functions: &[&str],
) -> Result<(&'a str, &'a str), ParenthesisParseError<'a>> {
    let input = input.trim();

    for &func_name in expected_functions {
        if input.starts_with(func_name) {
            let remaining = &input[func_name.len()..].trim();
            if remaining.starts_with('(') && remaining.ends_with(')') {
                let inner = &remaining[1..remaining.len() - 1];
                return Ok((func_name, inner));
            }
        }
    }

    Err(ParenthesisParseError::StopWordNotFound(input))
}

/// Utility function to split strings respecting commas (for parsing multiple values)
pub fn split_string_respect_comma(input: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;
    let mut in_quotes = false;
    let mut quote_char = '"';

    for ch in input.chars() {
        match ch {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = ch;
                current.push(ch);
            }
            ch if in_quotes && ch == quote_char => {
                in_quotes = false;
                current.push(ch);
            }
            '(' if !in_quotes => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' if !in_quotes => {
                paren_depth -= 1;
                current.push(ch);
            }
            ',' if paren_depth == 0 && !in_quotes => {
                result.push(current.trim());
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.trim().is_empty() {
        result.push(current.trim());
    }

    result.into_iter().collect()
}

/// Error type for parenthesis parsing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParenthesisParseError<'a> {
    UnclosedBraces,
    EmptyInput,
    StopWordNotFound(&'a str),
    NoClosingBraceFound,
    NoOpeningBraceFound,
}

// Macro for generating multi-type parsers (commonly used pattern)
#[macro_export]
macro_rules! multi_type_parser {
    ($enum_name:ident, $parse_fn_name:ident, $error_type:ident, [$(($variant:ident, $string:expr)),*]) => {
        pub fn $parse_fn_name<'a>(input: &'a str) -> Result<$enum_name, $error_type<'a>> {
            match input.trim() {
                $($string => Ok($enum_name::$variant),)*
                _ => Err($error_type::InvalidValue(input)),
            }
        }

        impl crate::props::formatter::FormatAsCssValue for $enum_name {
            fn format_as_css_value(&self) -> alloc::string::String {
                match self {
                    $($enum_name::$variant => $string.to_string(),)*
                }
            }
        }
    };
}

// Macro for generating typed pixel value parsers (commonly used pattern)
#[macro_export]
macro_rules! typed_pixel_value_parser {
    ($type_name:ident, $parse_fn_name:ident) => {
        pub fn $parse_fn_name<'a>(
            input: &'a str,
        ) -> Result<$type_name, crate::error::CssPixelValueParseError<'a>> {
            Ok($type_name {
                inner: crate::props::basic::value::parse_pixel_value(input)?,
            })
        }

        impl crate::props::formatter::FormatAsCssValue for $type_name {
            fn format_as_css_value(&self) -> alloc::string::String {
                self.inner.format_as_css_value()
            }
        }
    };
}
