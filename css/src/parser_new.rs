//! Main parsing entry points and utility functions

use alloc::{string::String, vec::Vec};

use crate::{
    error::CssParsingError,
    props::{get_css_key_map, CombinedCssPropertyType, CssProperty, CssPropertyType},
};

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

/// Checks wheter a given input is enclosed in parentheses, prefixed
/// by a certain number of stopwords.
///
/// On success, returns what the stopword was + the string inside the braces
/// on failure returns None.
///
/// ```rust
/// # use azul_css::parser::parse_parentheses;
/// # use azul_css::parser::ParenthesisParseError::*;
/// // Search for the nearest "abc()" brace
/// assert_eq!(
///     parse_parentheses("abc(def(g))", &["abc"]),
///     Ok(("abc", "def(g)"))
/// );
/// assert_eq!(
///     parse_parentheses("abc(def(g))", &["def"]),
///     Err(StopWordNotFound("abc"))
/// );
/// assert_eq!(
///     parse_parentheses("def(ghi(j))", &["def"]),
///     Ok(("def", "ghi(j)"))
/// );
/// assert_eq!(
///     parse_parentheses("abc(def(g))", &["abc", "def"]),
///     Ok(("abc", "def(g)"))
/// );
/// ```
pub fn parse_parentheses<'a>(
    input: &'a str,
    stopwords: &[&'static str],
) -> Result<(&'static str, &'a str), ParenthesisParseError<'a>> {
    use self::ParenthesisParseError::*;

    let input = input.trim();
    if input.is_empty() {
        return Err(EmptyInput);
    }

    let first_open_brace = input.find('(').ok_or(NoOpeningBraceFound)?;
    let found_stopword = &input[..first_open_brace];

    // CSS does not allow for space between the ( and the stopword, so no .trim() here
    let mut validated_stopword = None;
    for stopword in stopwords {
        if found_stopword == *stopword {
            validated_stopword = Some(stopword);
            break;
        }
    }

    let validated_stopword = validated_stopword.ok_or(StopWordNotFound(found_stopword))?;
    let last_closing_brace = input.rfind(')').ok_or(NoClosingBraceFound)?;

    Ok((
        validated_stopword,
        &input[(first_open_brace + 1)..last_closing_brace],
    ))
}

/// Utility function to split strings respecting commas (for parsing multiple values)
fn split_string_respect_comma<'a>(input: &'a str) -> Vec<&'a str> {
    /// Given a string, returns how many characters need to be skipped
    fn skip_next_braces(input: &str, target_char: char) -> Option<(usize, bool)> {
        let mut depth = 0;
        let mut last_character = 0;
        let mut character_was_found = false;

        if input.is_empty() {
            return None;
        }

        for (idx, ch) in input.char_indices() {
            last_character = idx;
            match ch {
                '(' => {
                    depth += 1;
                }
                ')' => {
                    depth -= 1;
                }
                c => {
                    if c == target_char && depth == 0 {
                        character_was_found = true;
                        break;
                    }
                }
            }
        }

        if last_character == 0 {
            // No more split by `,`
            None
        } else {
            Some((last_character, character_was_found))
        }
    }

    let mut comma_separated_items = Vec::<&str>::new();
    let mut current_input = &input[..];

    'outer: loop {
        let (skip_next_braces_result, character_was_found) =
            match skip_next_braces(&current_input, ',') {
                Some(s) => s,
                None => break 'outer,
            };
        let new_push_item = if character_was_found {
            &current_input[..skip_next_braces_result]
        } else {
            &current_input[..]
        };
        let new_current_input = &current_input[(skip_next_braces_result + 1)..];
        comma_separated_items.push(new_push_item);
        current_input = new_current_input;
        if !character_was_found {
            break 'outer;
        }
    }

    comma_separated_items
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
macro_rules! typed_pixel_value_parser {
    (
        $fn:ident, $fn_str:expr, $return:ident, $return_str:expr, $import_str:expr, $test_str:expr
    ) => {
        ///Parses a `
        #[doc = $return_str]
        ///` attribute from a `&str`
        ///
        ///# Example
        ///
        ///```rust
        #[doc = $import_str]
        #[doc = $test_str]
        ///```
        pub fn $fn<'a>(input: &'a str) -> Result<$return, CssPixelValueParseError<'a>> {
            parse_pixel_value(input).and_then(|e| Ok($return { inner: e }))
        }

        impl FormatAsCssValue for $return {
            fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
                self.inner.format_as_css_value(f)
            }
        }
    };
    ($fn:ident, $return:ident) => {
        typed_pixel_value_parser!(
            $fn,
            stringify!($fn),
            $return,
            stringify!($return),
            concat!(
                "# extern crate azul_css;",
                "\r\n",
                "# use azul_css::parser::",
                stringify!($fn),
                ";",
                "\r\n",
                "# use azul_css::{PixelValue, ",
                stringify!($return),
                "};"
            ),
            concat!(
                "assert_eq!(",
                stringify!($fn),
                "(\"5px\"), Ok(",
                stringify!($return),
                " { inner: PixelValue::px(5.0) }));"
            )
        );
    };
}
