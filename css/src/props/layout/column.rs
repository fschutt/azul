//! CSS properties for multi-column layout.
//!
//! Covers `column-count`, `column-width`, `column-span`, `column-fill`,
//! `column-rule-width`, `column-rule-style`, and `column-rule-color`.
//! Types are consumed via the `CssProperty` enum in the CSS property system.

use alloc::string::{String, ToString};
use core::num::ParseIntError;

use crate::props::{
    basic::{
        color::{parse_css_color, ColorU, CssColorParseError, CssColorParseErrorOwned},
        pixel::{
            parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue,
        },
    },
    formatter::PrintAsCssValue,
    style::border::{
        parse_border_style, BorderStyle, CssBorderStyleParseError, CssBorderStyleParseErrorOwned,
    },
};

// --- column-count ---

/// CSS `column-count` property: specifies the number of columns in a multi-column layout.
///
/// Values: `auto` or a positive integer.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum ColumnCount {
    #[default]
    Auto,
    Integer(u32),
}


impl PrintAsCssValue for ColumnCount {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Integer(i) => i.to_string(),
        }
    }
}

// --- column-width ---

/// CSS `column-width` property: specifies the optimal width of columns.
///
/// Values: `auto` or a length value (e.g. `200px`, `15em`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum ColumnWidth {
    #[default]
    Auto,
    Length(PixelValue),
}


impl PrintAsCssValue for ColumnWidth {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Length(px) => px.print_as_css_value(),
        }
    }
}

// --- column-span ---

/// CSS `column-span` property: whether an element spans across all columns.
///
/// Values: `none` (default) or `all`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum ColumnSpan {
    #[default]
    None,
    All,
}


impl PrintAsCssValue for ColumnSpan {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::None => "none",
            Self::All => "all",
        })
    }
}

// --- column-fill ---

/// CSS `column-fill` property: how content is distributed across columns.
///
/// Values: `balance` (default) or `auto`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum ColumnFill {
    Auto,
    #[default]
    Balance,
}


impl PrintAsCssValue for ColumnFill {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Balance => "balance",
        })
    }
}

// --- column-rule ---

/// CSS `column-rule-width` property: the width of the rule between columns.
///
/// Defaults to `medium` (3px).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ColumnRuleWidth {
    pub inner: PixelValue,
}

impl Default for ColumnRuleWidth {
    fn default() -> Self {
        Self {
            inner: PixelValue::const_px(3),
        }
    }
}

impl PrintAsCssValue for ColumnRuleWidth {
    fn print_as_css_value(&self) -> String {
        self.inner.print_as_css_value()
    }
}

/// CSS `column-rule-style` property: the style of the rule between columns.
///
/// Uses `BorderStyle` values (e.g. `none`, `solid`, `dotted`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ColumnRuleStyle {
    pub inner: BorderStyle,
}

impl Default for ColumnRuleStyle {
    fn default() -> Self {
        Self {
            inner: BorderStyle::None,
        }
    }
}

impl PrintAsCssValue for ColumnRuleStyle {
    fn print_as_css_value(&self) -> String {
        self.inner.print_as_css_value()
    }
}

/// CSS `column-rule-color` property: the color of the rule between columns.
///
/// Per the CSS spec this should default to `currentcolor`, but currently
/// defaults to black as `currentcolor` requires a resolved-value pass at
/// layout time.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ColumnRuleColor {
    pub inner: ColorU,
}

impl Default for ColumnRuleColor {
    fn default() -> Self {
        // NOTE: should be `currentcolor` per CSS spec, see doc comment on type
        Self {
            inner: ColorU::BLACK,
        }
    }
}

impl PrintAsCssValue for ColumnRuleColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}

// Formatting to Rust code
impl crate::codegen::format::FormatAsRustCode for ColumnCount {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("ColumnCount::Auto"),
            Self::Integer(i) => format!("ColumnCount::Integer({i})"),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for ColumnWidth {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("ColumnWidth::Auto"),
            Self::Length(px) => format!(
                "ColumnWidth::Length({})",
                crate::codegen::format::format_pixel_value(px)
            ),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for ColumnSpan {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::None => String::from("ColumnSpan::None"),
            Self::All => String::from("ColumnSpan::All"),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for ColumnFill {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("ColumnFill::Auto"),
            Self::Balance => String::from("ColumnFill::Balance"),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for ColumnRuleWidth {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "ColumnRuleWidth {{ inner: {} }}",
            crate::codegen::format::format_pixel_value(&self.inner)
        )
    }
}

impl crate::codegen::format::FormatAsRustCode for ColumnRuleStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!(
            "ColumnRuleStyle {{ inner: {} }}",
            self.inner.format_as_rust_code(tabs)
        )
    }
}

impl crate::codegen::format::FormatAsRustCode for ColumnRuleColor {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "ColumnRuleColor {{ inner: {} }}",
            crate::codegen::format::format_color_value(&self.inner)
        )
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
pub mod parser {
    #[allow(clippy::wildcard_imports)] // parser submodule reuses the parent module's value types
    use super::*;
    use crate::corety::AzString;

    // -- ColumnCount parser

    #[derive(Clone, PartialEq, Eq)]
    pub enum ColumnCountParseError<'a> {
        InvalidValue(&'a str),
        ParseInt(ParseIntError),
    }

    impl_debug_as_display!(ColumnCountParseError<'a>);
    impl_display! { ColumnCountParseError<'a>, {
        InvalidValue(v) => format!("Invalid column-count value: \"{}\"", v),
        ParseInt(e) => format!("Invalid integer for column-count: {}", e),
    }}

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum ColumnCountParseErrorOwned {
        InvalidValue(AzString),
        ParseInt(AzString),
    }

    impl ColumnCountParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> ColumnCountParseErrorOwned {
            match self {
                Self::InvalidValue(s) => ColumnCountParseErrorOwned::InvalidValue((*s).to_string().into()),
                Self::ParseInt(e) => ColumnCountParseErrorOwned::ParseInt(e.to_string().into()),
            }
        }
    }

    impl ColumnCountParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> ColumnCountParseError<'_> {
            match self {
                Self::InvalidValue(s) => ColumnCountParseError::InvalidValue(s),
                // ParseIntError cannot be reconstructed from its Display string,
                // so we fall back to a generic message. The original error text
                // is preserved in the owned `AzString` but not round-trippable.
                Self::ParseInt(_) => ColumnCountParseError::InvalidValue("invalid integer"),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `column-count` value.
    pub fn parse_column_count(
        input: &str,
    ) -> Result<ColumnCount, ColumnCountParseError<'_>> {
        let trimmed = input.trim();
        if trimmed == "auto" {
            return Ok(ColumnCount::Auto);
        }
        let val: u32 = trimmed
            .parse()
            .map_err(ColumnCountParseError::ParseInt)?;
        Ok(ColumnCount::Integer(val))
    }

    // -- ColumnWidth parser

    #[derive(Clone, PartialEq, Eq)]
    pub enum ColumnWidthParseError<'a> {
        InvalidValue(&'a str),
        PixelValue(CssPixelValueParseError<'a>),
    }

    impl_debug_as_display!(ColumnWidthParseError<'a>);
    impl_display! { ColumnWidthParseError<'a>, {
        InvalidValue(v) => format!("Invalid column-width value: \"{}\"", v),
        PixelValue(e) => format!("{}", e),
    }}
    impl_from! { CssPixelValueParseError<'a>, ColumnWidthParseError::PixelValue }

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum ColumnWidthParseErrorOwned {
        InvalidValue(AzString),
        PixelValue(CssPixelValueParseErrorOwned),
    }

    impl ColumnWidthParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> ColumnWidthParseErrorOwned {
            match self {
                Self::InvalidValue(s) => ColumnWidthParseErrorOwned::InvalidValue((*s).to_string().into()),
                Self::PixelValue(e) => ColumnWidthParseErrorOwned::PixelValue(e.to_contained()),
            }
        }
    }

    impl ColumnWidthParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> ColumnWidthParseError<'_> {
            match self {
                Self::InvalidValue(s) => ColumnWidthParseError::InvalidValue(s),
                Self::PixelValue(e) => ColumnWidthParseError::PixelValue(e.to_shared()),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `column-width` value.
    pub fn parse_column_width(
        input: &str,
    ) -> Result<ColumnWidth, ColumnWidthParseError<'_>> {
        let trimmed = input.trim();
        if trimmed == "auto" {
            return Ok(ColumnWidth::Auto);
        }
        Ok(ColumnWidth::Length(parse_pixel_value(trimmed)?))
    }

    // -- Other column parsers...
    macro_rules! define_simple_column_parser {
        (
            $fn_name:ident,
            $struct_name:ident,
            $error_name:ident,
            $error_owned_name:ident,
            $prop_name:expr,
            $($val:expr => $variant:path),+
        ) => {
            #[derive(Clone, PartialEq, Eq)]
            pub enum $error_name<'a> {
                InvalidValue(&'a str),
            }

            impl_debug_as_display!($error_name<'a>);
            impl_display! { $error_name<'a>, {
                InvalidValue(v) => format!("Invalid {} value: \"{}\"", $prop_name, v),
            }}

            #[derive(Debug, Clone, PartialEq, Eq)]
            #[repr(C, u8)]
            pub enum $error_owned_name {
                InvalidValue(AzString),
            }

            impl $error_name<'_> {
                #[must_use] pub fn to_contained(&self) -> $error_owned_name {
                    match self {
                        Self::InvalidValue(s) => $error_owned_name::InvalidValue(s.to_string().into()),
                    }
                }
            }

            impl $error_owned_name {
                #[must_use] pub fn to_shared(&self) -> $error_name<'_> {
                    match self {
                        Self::InvalidValue(s) => $error_name::InvalidValue(s.as_str()),
                    }
                }
            }

            pub fn $fn_name(input: &str) -> Result<$struct_name, $error_name<'_>> {
                match input.trim() {
                    $( $val => Ok($variant), )+
                    _ => Err($error_name::InvalidValue(input)),
                }
            }
        };
    }

    define_simple_column_parser!(
        parse_column_span,
        ColumnSpan,
        ColumnSpanParseError,
        ColumnSpanParseErrorOwned,
        "column-span",
        "none" => ColumnSpan::None,
        "all" => ColumnSpan::All
    );

    define_simple_column_parser!(
        parse_column_fill,
        ColumnFill,
        ColumnFillParseError,
        ColumnFillParseErrorOwned,
        "column-fill",
        "auto" => ColumnFill::Auto,
        "balance" => ColumnFill::Balance
    );

    // Parsers for column-rule-*

    #[derive(Clone, PartialEq, Eq)]
    pub enum ColumnRuleWidthParseError<'a> {
        Pixel(CssPixelValueParseError<'a>),
    }
    impl_debug_as_display!(ColumnRuleWidthParseError<'a>);
    impl_display! { ColumnRuleWidthParseError<'a>, { Pixel(e) => format!("{}", e) }}
    impl_from! { CssPixelValueParseError<'a>, ColumnRuleWidthParseError::Pixel }
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum ColumnRuleWidthParseErrorOwned {
        Pixel(CssPixelValueParseErrorOwned),
    }
    impl ColumnRuleWidthParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> ColumnRuleWidthParseErrorOwned {
            match self {
                ColumnRuleWidthParseError::Pixel(e) => {
                    ColumnRuleWidthParseErrorOwned::Pixel(e.to_contained())
                }
            }
        }
    }
    impl ColumnRuleWidthParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> ColumnRuleWidthParseError<'_> {
            match self {
                Self::Pixel(e) => {
                    ColumnRuleWidthParseError::Pixel(e.to_shared())
                }
            }
        }
    }
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `column-rule-width` value.
    pub fn parse_column_rule_width(
        input: &str,
    ) -> Result<ColumnRuleWidth, ColumnRuleWidthParseError<'_>> {
        Ok(ColumnRuleWidth {
            inner: parse_pixel_value(input)?,
        })
    }

    #[derive(Clone, PartialEq, Eq)]
    pub enum ColumnRuleStyleParseError<'a> {
        Style(CssBorderStyleParseError<'a>),
    }
    impl_debug_as_display!(ColumnRuleStyleParseError<'a>);
    impl_display! { ColumnRuleStyleParseError<'a>, { Style(e) => format!("{}", e) }}
    impl_from! { CssBorderStyleParseError<'a>, ColumnRuleStyleParseError::Style }
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum ColumnRuleStyleParseErrorOwned {
        Style(CssBorderStyleParseErrorOwned),
    }
    impl ColumnRuleStyleParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> ColumnRuleStyleParseErrorOwned {
            match self {
                ColumnRuleStyleParseError::Style(e) => {
                    ColumnRuleStyleParseErrorOwned::Style(e.to_contained())
                }
            }
        }
    }
    impl ColumnRuleStyleParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> ColumnRuleStyleParseError<'_> {
            match self {
                Self::Style(e) => {
                    ColumnRuleStyleParseError::Style(e.to_shared())
                }
            }
        }
    }
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `column-rule-style` value.
    pub fn parse_column_rule_style(
        input: &str,
    ) -> Result<ColumnRuleStyle, ColumnRuleStyleParseError<'_>> {
        Ok(ColumnRuleStyle {
            inner: parse_border_style(input)?,
        })
    }

    #[derive(Clone, PartialEq)]
    pub enum ColumnRuleColorParseError<'a> {
        Color(CssColorParseError<'a>),
    }
    impl_debug_as_display!(ColumnRuleColorParseError<'a>);
    impl_display! { ColumnRuleColorParseError<'a>, { Color(e) => format!("{}", e) }}
    impl_from! { CssColorParseError<'a>, ColumnRuleColorParseError::Color }
    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum ColumnRuleColorParseErrorOwned {
        Color(CssColorParseErrorOwned),
    }
    impl ColumnRuleColorParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> ColumnRuleColorParseErrorOwned {
            match self {
                ColumnRuleColorParseError::Color(e) => {
                    ColumnRuleColorParseErrorOwned::Color(e.to_contained())
                }
            }
        }
    }
    impl ColumnRuleColorParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> ColumnRuleColorParseError<'_> {
            match self {
                Self::Color(e) => {
                    ColumnRuleColorParseError::Color(e.to_shared())
                }
            }
        }
    }
    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `column-rule-color` value.
    pub fn parse_column_rule_color(
        input: &str,
    ) -> Result<ColumnRuleColor, ColumnRuleColorParseError<'_>> {
        Ok(ColumnRuleColor {
            inner: parse_css_color(input)?,
        })
    }
}

#[cfg(feature = "parser")]
pub use parser::*;

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_column_count() {
        assert_eq!(parse_column_count("auto").unwrap(), ColumnCount::Auto);
        assert_eq!(parse_column_count("3").unwrap(), ColumnCount::Integer(3));
        assert!(parse_column_count("none").is_err());
        assert!(parse_column_count("2.5").is_err());
    }

    #[test]
    fn test_parse_column_width() {
        assert_eq!(parse_column_width("auto").unwrap(), ColumnWidth::Auto);
        assert_eq!(
            parse_column_width("200px").unwrap(),
            ColumnWidth::Length(PixelValue::px(200.0))
        );
        assert_eq!(
            parse_column_width("15em").unwrap(),
            ColumnWidth::Length(PixelValue::em(15.0))
        );
        assert!(parse_column_width("50%").is_ok()); // Percentage is valid for column-width
    }

    #[test]
    fn test_parse_column_span() {
        assert_eq!(parse_column_span("none").unwrap(), ColumnSpan::None);
        assert_eq!(parse_column_span("all").unwrap(), ColumnSpan::All);
        assert!(parse_column_span("2").is_err());
    }

    #[test]
    fn test_parse_column_fill() {
        assert_eq!(parse_column_fill("auto").unwrap(), ColumnFill::Auto);
        assert_eq!(parse_column_fill("balance").unwrap(), ColumnFill::Balance);
        assert!(parse_column_fill("none").is_err());
    }

    #[test]
    fn test_parse_column_rule() {
        assert_eq!(
            parse_column_rule_width("5px").unwrap().inner,
            PixelValue::px(5.0)
        );
        assert_eq!(
            parse_column_rule_style("dotted").unwrap().inner,
            BorderStyle::Dotted
        );
        assert_eq!(parse_column_rule_color("blue").unwrap().inner, ColorU::BLUE);
    }
}
