//! CSS properties for multi-column layout.

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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum ColumnCount {
    Auto,
    Integer(u32),
}

impl Default for ColumnCount {
    fn default() -> Self {
        Self::Auto
    }
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum ColumnWidth {
    Auto,
    Length(PixelValue),
}

impl Default for ColumnWidth {
    fn default() -> Self {
        Self::Auto
    }
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ColumnSpan {
    None,
    All,
}

impl Default for ColumnSpan {
    fn default() -> Self {
        Self::None
    }
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ColumnFill {
    Auto,
    Balance,
}

impl Default for ColumnFill {
    fn default() -> Self {
        Self::Balance
    }
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ColumnRuleColor {
    pub inner: ColorU,
}

impl Default for ColumnRuleColor {
    fn default() -> Self {
        Self {
            inner: ColorU::BLACK,
        } // should be `currentcolor`
    }
}

impl PrintAsCssValue for ColumnRuleColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}

// Formatting to Rust code
impl crate::format_rust_code::FormatAsRustCode for ColumnCount {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            ColumnCount::Auto => String::from("ColumnCount::Auto"),
            ColumnCount::Integer(i) => format!("ColumnCount::Integer({})", i),
        }
    }
}

impl crate::format_rust_code::FormatAsRustCode for ColumnWidth {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        match self {
            ColumnWidth::Auto => String::from("ColumnWidth::Auto"),
            ColumnWidth::Length(px) => format!(
                "ColumnWidth::Length({})",
                crate::format_rust_code::format_pixel_value(px)
            ),
        }
    }
}

impl crate::format_rust_code::FormatAsRustCode for ColumnSpan {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            ColumnSpan::None => String::from("ColumnSpan::None"),
            ColumnSpan::All => String::from("ColumnSpan::All"),
        }
    }
}

impl crate::format_rust_code::FormatAsRustCode for ColumnFill {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            ColumnFill::Auto => String::from("ColumnFill::Auto"),
            ColumnFill::Balance => String::from("ColumnFill::Balance"),
        }
    }
}

impl crate::format_rust_code::FormatAsRustCode for ColumnRuleWidth {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "ColumnRuleWidth {{ inner: {} }}",
            crate::format_rust_code::format_pixel_value(&self.inner)
        )
    }
}

impl crate::format_rust_code::FormatAsRustCode for ColumnRuleStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!(
            "ColumnRuleStyle {{ inner: {} }}",
            self.inner.format_as_rust_code(tabs)
        )
    }
}

impl crate::format_rust_code::FormatAsRustCode for ColumnRuleColor {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "ColumnRuleColor {{ inner: {} }}",
            crate::format_rust_code::format_color_value(&self.inner)
        )
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;
    use crate::corety::AzString;

    // -- ColumnCount parser

    #[derive(Clone, PartialEq)]
    pub enum ColumnCountParseError<'a> {
        InvalidValue(&'a str),
        ParseInt(ParseIntError),
    }

    impl_debug_as_display!(ColumnCountParseError<'a>);
    impl_display! { ColumnCountParseError<'a>, {
        InvalidValue(v) => format!("Invalid column-count value: \"{}\"", v),
        ParseInt(e) => format!("Invalid integer for column-count: {}", e),
    }}

    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum ColumnCountParseErrorOwned {
        InvalidValue(AzString),
        ParseInt(AzString),
    }

    impl<'a> ColumnCountParseError<'a> {
        pub fn to_contained(&self) -> ColumnCountParseErrorOwned {
            match self {
                Self::InvalidValue(s) => ColumnCountParseErrorOwned::InvalidValue(s.to_string().into()),
                Self::ParseInt(e) => ColumnCountParseErrorOwned::ParseInt(e.to_string().into()),
            }
        }
    }

    impl ColumnCountParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> ColumnCountParseError<'a> {
            match self {
                Self::InvalidValue(s) => ColumnCountParseError::InvalidValue(s),
                Self::ParseInt(_) => ColumnCountParseError::InvalidValue("invalid integer"), /* Can't reconstruct */
            }
        }
    }

    pub fn parse_column_count<'a>(
        input: &'a str,
    ) -> Result<ColumnCount, ColumnCountParseError<'a>> {
        let trimmed = input.trim();
        if trimmed == "auto" {
            return Ok(ColumnCount::Auto);
        }
        let val: u32 = trimmed
            .parse()
            .map_err(|e| ColumnCountParseError::ParseInt(e))?;
        Ok(ColumnCount::Integer(val))
    }

    // -- ColumnWidth parser

    #[derive(Clone, PartialEq)]
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

    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum ColumnWidthParseErrorOwned {
        InvalidValue(AzString),
        PixelValue(CssPixelValueParseErrorOwned),
    }

    impl<'a> ColumnWidthParseError<'a> {
        pub fn to_contained(&self) -> ColumnWidthParseErrorOwned {
            match self {
                Self::InvalidValue(s) => ColumnWidthParseErrorOwned::InvalidValue(s.to_string().into()),
                Self::PixelValue(e) => ColumnWidthParseErrorOwned::PixelValue(e.to_contained()),
            }
        }
    }

    impl ColumnWidthParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> ColumnWidthParseError<'a> {
            match self {
                Self::InvalidValue(s) => ColumnWidthParseError::InvalidValue(s),
                Self::PixelValue(e) => ColumnWidthParseError::PixelValue(e.to_shared()),
            }
        }
    }

    pub fn parse_column_width<'a>(
        input: &'a str,
    ) -> Result<ColumnWidth, ColumnWidthParseError<'a>> {
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
            #[derive(Clone, PartialEq)]
            pub enum $error_name<'a> {
                InvalidValue(&'a str),
            }

            impl_debug_as_display!($error_name<'a>);
            impl_display! { $error_name<'a>, {
                InvalidValue(v) => format!("Invalid {} value: \"{}\"", $prop_name, v),
            }}

            #[derive(Debug, Clone, PartialEq)]
            #[repr(C, u8)]
            pub enum $error_owned_name {
                InvalidValue(AzString),
            }

            impl<'a> $error_name<'a> {
                pub fn to_contained(&self) -> $error_owned_name {
                    match self {
                        Self::InvalidValue(s) => $error_owned_name::InvalidValue(s.to_string().into()),
                    }
                }
            }

            impl $error_owned_name {
                pub fn to_shared<'a>(&'a self) -> $error_name<'a> {
                    match self {
                        Self::InvalidValue(s) => $error_name::InvalidValue(s.as_str()),
                    }
                }
            }

            pub fn $fn_name<'a>(input: &'a str) -> Result<$struct_name, $error_name<'a>> {
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

    #[derive(Clone, PartialEq)]
    pub enum ColumnRuleWidthParseError<'a> {
        Pixel(CssPixelValueParseError<'a>),
    }
    impl_debug_as_display!(ColumnRuleWidthParseError<'a>);
    impl_display! { ColumnRuleWidthParseError<'a>, { Pixel(e) => format!("{}", e) }}
    impl_from! { CssPixelValueParseError<'a>, ColumnRuleWidthParseError::Pixel }
    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum ColumnRuleWidthParseErrorOwned {
        Pixel(CssPixelValueParseErrorOwned),
    }
    impl<'a> ColumnRuleWidthParseError<'a> {
        pub fn to_contained(&self) -> ColumnRuleWidthParseErrorOwned {
            match self {
                ColumnRuleWidthParseError::Pixel(e) => {
                    ColumnRuleWidthParseErrorOwned::Pixel(e.to_contained())
                }
            }
        }
    }
    impl ColumnRuleWidthParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> ColumnRuleWidthParseError<'a> {
            match self {
                ColumnRuleWidthParseErrorOwned::Pixel(e) => {
                    ColumnRuleWidthParseError::Pixel(e.to_shared())
                }
            }
        }
    }
    pub fn parse_column_rule_width<'a>(
        input: &'a str,
    ) -> Result<ColumnRuleWidth, ColumnRuleWidthParseError<'a>> {
        Ok(ColumnRuleWidth {
            inner: parse_pixel_value(input)?,
        })
    }

    #[derive(Clone, PartialEq)]
    pub enum ColumnRuleStyleParseError<'a> {
        Style(CssBorderStyleParseError<'a>),
    }
    impl_debug_as_display!(ColumnRuleStyleParseError<'a>);
    impl_display! { ColumnRuleStyleParseError<'a>, { Style(e) => format!("{}", e) }}
    impl_from! { CssBorderStyleParseError<'a>, ColumnRuleStyleParseError::Style }
    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum ColumnRuleStyleParseErrorOwned {
        Style(CssBorderStyleParseErrorOwned),
    }
    impl<'a> ColumnRuleStyleParseError<'a> {
        pub fn to_contained(&self) -> ColumnRuleStyleParseErrorOwned {
            match self {
                ColumnRuleStyleParseError::Style(e) => {
                    ColumnRuleStyleParseErrorOwned::Style(e.to_contained())
                }
            }
        }
    }
    impl ColumnRuleStyleParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> ColumnRuleStyleParseError<'a> {
            match self {
                ColumnRuleStyleParseErrorOwned::Style(e) => {
                    ColumnRuleStyleParseError::Style(e.to_shared())
                }
            }
        }
    }
    pub fn parse_column_rule_style<'a>(
        input: &'a str,
    ) -> Result<ColumnRuleStyle, ColumnRuleStyleParseError<'a>> {
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
    impl<'a> ColumnRuleColorParseError<'a> {
        pub fn to_contained(&self) -> ColumnRuleColorParseErrorOwned {
            match self {
                ColumnRuleColorParseError::Color(e) => {
                    ColumnRuleColorParseErrorOwned::Color(e.to_contained())
                }
            }
        }
    }
    impl ColumnRuleColorParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> ColumnRuleColorParseError<'a> {
            match self {
                ColumnRuleColorParseErrorOwned::Color(e) => {
                    ColumnRuleColorParseError::Color(e.to_shared())
                }
            }
        }
    }
    pub fn parse_column_rule_color<'a>(
        input: &'a str,
    ) -> Result<ColumnRuleColor, ColumnRuleColorParseError<'a>> {
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
