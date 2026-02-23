//! CSS properties for controlling fragmentation (page/column breaks).

use alloc::string::{String, ToString};
use core::num::ParseIntError;

use crate::props::formatter::PrintAsCssValue;

// --- break-before / break-after ---

/// Represents a `break-before` or `break-after` CSS property value.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum PageBreak {
    Auto,
    Avoid,
    Always,
    All,
    Page,
    AvoidPage,
    Left,
    Right,
    Recto,
    Verso,
    Column,
    AvoidColumn,
}

impl Default for PageBreak {
    fn default() -> Self {
        Self::Auto
    }
}

impl PrintAsCssValue for PageBreak {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Avoid => "avoid",
            Self::Always => "always",
            Self::All => "all",
            Self::Page => "page",
            Self::AvoidPage => "avoid-page",
            Self::Left => "left",
            Self::Right => "right",
            Self::Recto => "recto",
            Self::Verso => "verso",
            Self::Column => "column",
            Self::AvoidColumn => "avoid-column",
        })
    }
}

// --- break-inside ---

/// Represents a `break-inside` CSS property value.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum BreakInside {
    Auto,
    Avoid,
    AvoidPage,
    AvoidColumn,
}

impl Default for BreakInside {
    fn default() -> Self {
        Self::Auto
    }
}

impl PrintAsCssValue for BreakInside {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Avoid => "avoid",
            Self::AvoidPage => "avoid-page",
            Self::AvoidColumn => "avoid-column",
        })
    }
}

// --- widows / orphans ---

/// CSS `widows` property - minimum number of lines in a block container
/// that must be shown at the top of a page, region, or column.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct Widows {
    pub inner: u32,
}

impl Default for Widows {
    fn default() -> Self {
        Self { inner: 2 }
    }
}

impl PrintAsCssValue for Widows {
    fn print_as_css_value(&self) -> String {
        self.inner.to_string()
    }
}

/// CSS `orphans` property - minimum number of lines in a block container
/// that must be shown at the bottom of a page, region, or column.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct Orphans {
    pub inner: u32,
}

impl Default for Orphans {
    fn default() -> Self {
        Self { inner: 2 }
    }
}

impl PrintAsCssValue for Orphans {
    fn print_as_css_value(&self) -> String {
        self.inner.to_string()
    }
}

// --- box-decoration-break ---

/// Represents a `box-decoration-break` CSS property value.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum BoxDecorationBreak {
    Slice,
    Clone,
}

impl Default for BoxDecorationBreak {
    fn default() -> Self {
        Self::Slice
    }
}

impl PrintAsCssValue for BoxDecorationBreak {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Slice => "slice",
            Self::Clone => "clone",
        })
    }
}

// Formatting to Rust code
impl crate::format_rust_code::FormatAsRustCode for PageBreak {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            PageBreak::Auto => String::from("PageBreak::Auto"),
            PageBreak::Avoid => String::from("PageBreak::Avoid"),
            PageBreak::Always => String::from("PageBreak::Always"),
            PageBreak::All => String::from("PageBreak::All"),
            PageBreak::Page => String::from("PageBreak::Page"),
            PageBreak::AvoidPage => String::from("PageBreak::AvoidPage"),
            PageBreak::Left => String::from("PageBreak::Left"),
            PageBreak::Right => String::from("PageBreak::Right"),
            PageBreak::Recto => String::from("PageBreak::Recto"),
            PageBreak::Verso => String::from("PageBreak::Verso"),
            PageBreak::Column => String::from("PageBreak::Column"),
            PageBreak::AvoidColumn => String::from("PageBreak::AvoidColumn"),
        }
    }
}

impl crate::format_rust_code::FormatAsRustCode for BreakInside {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            BreakInside::Auto => String::from("BreakInside::Auto"),
            BreakInside::Avoid => String::from("BreakInside::Avoid"),
            BreakInside::AvoidPage => String::from("BreakInside::AvoidPage"),
            BreakInside::AvoidColumn => String::from("BreakInside::AvoidColumn"),
        }
    }
}

impl crate::format_rust_code::FormatAsRustCode for Widows {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("Widows {{ inner: {} }}", self.inner)
    }
}

impl crate::format_rust_code::FormatAsRustCode for Orphans {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("Orphans {{ inner: {} }}", self.inner)
    }
}

impl crate::format_rust_code::FormatAsRustCode for BoxDecorationBreak {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            BoxDecorationBreak::Slice => String::from("BoxDecorationBreak::Slice"),
            BoxDecorationBreak::Clone => String::from("BoxDecorationBreak::Clone"),
        }
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;
    use crate::corety::AzString;
    use crate::props::layout::position::ParseIntErrorWithInput;

    // -- PageBreak parser (`break-before`, `break-after`)

    #[derive(Clone, PartialEq)]
    pub enum PageBreakParseError<'a> {
        InvalidValue(&'a str),
    }

    impl_debug_as_display!(PageBreakParseError<'a>);
    impl_display! { PageBreakParseError<'a>, {
        InvalidValue(v) => format!("Invalid break value: \"{}\"", v),
    }}

    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum PageBreakParseErrorOwned {
        InvalidValue(AzString),
    }

    impl<'a> PageBreakParseError<'a> {
        pub fn to_contained(&self) -> PageBreakParseErrorOwned {
            match self {
                Self::InvalidValue(s) => PageBreakParseErrorOwned::InvalidValue(s.to_string().into()),
            }
        }
    }

    impl PageBreakParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> PageBreakParseError<'a> {
            match self {
                Self::InvalidValue(s) => PageBreakParseError::InvalidValue(s.as_str()),
            }
        }
    }

    pub fn parse_page_break<'a>(input: &'a str) -> Result<PageBreak, PageBreakParseError<'a>> {
        match input.trim() {
            "auto" => Ok(PageBreak::Auto),
            "avoid" => Ok(PageBreak::Avoid),
            "always" => Ok(PageBreak::Always),
            "all" => Ok(PageBreak::All),
            "page" => Ok(PageBreak::Page),
            "avoid-page" => Ok(PageBreak::AvoidPage),
            "left" => Ok(PageBreak::Left),
            "right" => Ok(PageBreak::Right),
            "recto" => Ok(PageBreak::Recto),
            "verso" => Ok(PageBreak::Verso),
            "column" => Ok(PageBreak::Column),
            "avoid-column" => Ok(PageBreak::AvoidColumn),
            _ => Err(PageBreakParseError::InvalidValue(input)),
        }
    }

    // -- BreakInside parser

    #[derive(Clone, PartialEq)]
    pub enum BreakInsideParseError<'a> {
        InvalidValue(&'a str),
    }

    impl_debug_as_display!(BreakInsideParseError<'a>);
    impl_display! { BreakInsideParseError<'a>, {
        InvalidValue(v) => format!("Invalid break-inside value: \"{}\"", v),
    }}

    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum BreakInsideParseErrorOwned {
        InvalidValue(AzString),
    }

    impl<'a> BreakInsideParseError<'a> {
        pub fn to_contained(&self) -> BreakInsideParseErrorOwned {
            match self {
                Self::InvalidValue(s) => BreakInsideParseErrorOwned::InvalidValue(s.to_string().into()),
            }
        }
    }

    impl BreakInsideParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> BreakInsideParseError<'a> {
            match self {
                Self::InvalidValue(s) => BreakInsideParseError::InvalidValue(s.as_str()),
            }
        }
    }

    pub fn parse_break_inside<'a>(
        input: &'a str,
    ) -> Result<BreakInside, BreakInsideParseError<'a>> {
        match input.trim() {
            "auto" => Ok(BreakInside::Auto),
            "avoid" => Ok(BreakInside::Avoid),
            "avoid-page" => Ok(BreakInside::AvoidPage),
            "avoid-column" => Ok(BreakInside::AvoidColumn),
            _ => Err(BreakInsideParseError::InvalidValue(input)),
        }
    }

    // -- Widows / Orphans parsers

    macro_rules! define_widow_orphan_parser {
        ($fn_name:ident, $struct_name:ident, $error_name:ident, $error_owned_name:ident, $prop_name:expr) => {
            #[derive(Clone, PartialEq)]
            pub enum $error_name<'a> {
                ParseInt(ParseIntError, &'a str),
                NegativeValue(&'a str),
            }

            impl_debug_as_display!($error_name<'a>);
            impl_display! { $error_name<'a>, {
                ParseInt(e, s) => format!("Invalid integer for {}: \"{}\". Reason: {}", $prop_name, s, e),
                NegativeValue(s) => format!("Invalid value for {}: \"{}\". Value cannot be negative.", $prop_name, s),
            }}

            #[derive(Debug, Clone, PartialEq)]
            #[repr(C, u8)]
            pub enum $error_owned_name {
                ParseInt(ParseIntErrorWithInput),
                NegativeValue(AzString),
            }

            impl<'a> $error_name<'a> {
                pub fn to_contained(&self) -> $error_owned_name {
                    match self {
                        Self::ParseInt(e, s) => $error_owned_name::ParseInt(ParseIntErrorWithInput { error: e.to_string().into(), input: s.to_string().into() }),
                        Self::NegativeValue(s) => $error_owned_name::NegativeValue(s.to_string().into()),
                    }
                }
            }

            impl $error_owned_name {
                pub fn to_shared<'a>(&'a self) -> $error_name<'a> {
                     match self {
                        // Can't reconstruct ParseIntError
                        Self::ParseInt(e) => $error_name::NegativeValue(e.input.as_str()),
                        Self::NegativeValue(s) => $error_name::NegativeValue(s),
                    }
                }
            }

            pub fn $fn_name<'a>(input: &'a str) -> Result<$struct_name, $error_name<'a>> {
                let trimmed = input.trim();
                let val: i32 = trimmed.parse().map_err(|e| $error_name::ParseInt(e, trimmed))?;
                if val < 0 {
                    return Err($error_name::NegativeValue(trimmed));
                }
                Ok($struct_name { inner: val as u32 })
            }
        };
    }

    define_widow_orphan_parser!(
        parse_widows,
        Widows,
        WidowsParseError,
        WidowsParseErrorOwned,
        "widows"
    );
    define_widow_orphan_parser!(
        parse_orphans,
        Orphans,
        OrphansParseError,
        OrphansParseErrorOwned,
        "orphans"
    );

    // -- BoxDecorationBreak parser

    #[derive(Clone, PartialEq)]
    pub enum BoxDecorationBreakParseError<'a> {
        InvalidValue(&'a str),
    }

    impl_debug_as_display!(BoxDecorationBreakParseError<'a>);
    impl_display! { BoxDecorationBreakParseError<'a>, {
        InvalidValue(v) => format!("Invalid box-decoration-break value: \"{}\"", v),
    }}

    #[derive(Debug, Clone, PartialEq)]
    #[repr(C, u8)]
    pub enum BoxDecorationBreakParseErrorOwned {
        InvalidValue(AzString),
    }

    impl<'a> BoxDecorationBreakParseError<'a> {
        pub fn to_contained(&self) -> BoxDecorationBreakParseErrorOwned {
            match self {
                Self::InvalidValue(s) => {
                    BoxDecorationBreakParseErrorOwned::InvalidValue(s.to_string().into())
                }
            }
        }
    }

    impl BoxDecorationBreakParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> BoxDecorationBreakParseError<'a> {
            match self {
                Self::InvalidValue(s) => BoxDecorationBreakParseError::InvalidValue(s.as_str()),
            }
        }
    }

    pub fn parse_box_decoration_break<'a>(
        input: &'a str,
    ) -> Result<BoxDecorationBreak, BoxDecorationBreakParseError<'a>> {
        match input.trim() {
            "slice" => Ok(BoxDecorationBreak::Slice),
            "clone" => Ok(BoxDecorationBreak::Clone),
            _ => Err(BoxDecorationBreakParseError::InvalidValue(input)),
        }
    }
}

#[cfg(feature = "parser")]
pub use parser::*;

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_page_break() {
        assert_eq!(parse_page_break("auto").unwrap(), PageBreak::Auto);
        assert_eq!(parse_page_break("page").unwrap(), PageBreak::Page);
        assert_eq!(
            parse_page_break("avoid-column").unwrap(),
            PageBreak::AvoidColumn
        );
        assert!(parse_page_break("invalid").is_err());
    }

    #[test]
    fn test_parse_break_inside() {
        assert_eq!(parse_break_inside("auto").unwrap(), BreakInside::Auto);
        assert_eq!(parse_break_inside("avoid").unwrap(), BreakInside::Avoid);
        assert!(parse_break_inside("always").is_err());
    }

    #[test]
    fn test_parse_widows_orphans() {
        assert_eq!(parse_widows("3").unwrap().inner, 3);
        assert_eq!(parse_orphans("  1  ").unwrap().inner, 1);
        assert!(parse_widows("-2").is_err());
        assert!(parse_orphans("auto").is_err());
    }

    #[test]
    fn test_parse_box_decoration_break() {
        assert_eq!(
            parse_box_decoration_break("slice").unwrap(),
            BoxDecorationBreak::Slice
        );
        assert_eq!(
            parse_box_decoration_break("clone").unwrap(),
            BoxDecorationBreak::Clone
        );
        assert!(parse_box_decoration_break("copy").is_err());
    }
}
