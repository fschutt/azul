//! CSS properties for controlling fragmentation (page/column breaks).
//!
//! Defines [`PageBreak`], [`BreakInside`], [`Widows`], [`Orphans`], and
//! [`BoxDecorationBreak`]. The `parser` sub-module (behind the `parser`
//! feature) provides CSS-value parsing for each type.

use alloc::string::{String, ToString};

use crate::props::formatter::PrintAsCssValue;

// --- break-before / break-after ---

/// Represents a `break-before` or `break-after` CSS property value.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum PageBreak {
    #[default]
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
#[derive(Default)]
pub enum BreakInside {
    #[default]
    Auto,
    Avoid,
    AvoidPage,
    AvoidColumn,
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
#[derive(Default)]
pub enum BoxDecorationBreak {
    #[default]
    Slice,
    Clone,
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
impl crate::codegen::format::FormatAsRustCode for PageBreak {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("PageBreak::Auto"),
            Self::Avoid => String::from("PageBreak::Avoid"),
            Self::Always => String::from("PageBreak::Always"),
            Self::All => String::from("PageBreak::All"),
            Self::Page => String::from("PageBreak::Page"),
            Self::AvoidPage => String::from("PageBreak::AvoidPage"),
            Self::Left => String::from("PageBreak::Left"),
            Self::Right => String::from("PageBreak::Right"),
            Self::Recto => String::from("PageBreak::Recto"),
            Self::Verso => String::from("PageBreak::Verso"),
            Self::Column => String::from("PageBreak::Column"),
            Self::AvoidColumn => String::from("PageBreak::AvoidColumn"),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for BreakInside {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Auto => String::from("BreakInside::Auto"),
            Self::Avoid => String::from("BreakInside::Avoid"),
            Self::AvoidPage => String::from("BreakInside::AvoidPage"),
            Self::AvoidColumn => String::from("BreakInside::AvoidColumn"),
        }
    }
}

impl crate::codegen::format::FormatAsRustCode for Widows {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("Widows {{ inner: {} }}", self.inner)
    }
}

impl crate::codegen::format::FormatAsRustCode for Orphans {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("Orphans {{ inner: {} }}", self.inner)
    }
}

impl crate::codegen::format::FormatAsRustCode for BoxDecorationBreak {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::Slice => String::from("BoxDecorationBreak::Slice"),
            Self::Clone => String::from("BoxDecorationBreak::Clone"),
        }
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
pub mod parser {
    #[allow(clippy::wildcard_imports)] // parser submodule reuses the parent module's value types
    use super::*;
    use core::num::ParseIntError;
    use crate::corety::AzString;
    use crate::props::layout::position::ParseIntErrorWithInput;

    // -- PageBreak parser (`break-before`, `break-after`)

    /// Error returned when parsing a `break-before` or `break-after` value.
    #[derive(Clone, PartialEq, Eq)]
    pub enum PageBreakParseError<'a> {
        InvalidValue(&'a str),
    }

    impl_debug_as_display!(PageBreakParseError<'a>);
    impl_display! { PageBreakParseError<'a>, {
        InvalidValue(v) => format!("Invalid break value: \"{}\"", v),
    }}

    /// Owned version of [`PageBreakParseError`] for FFI and storage.
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum PageBreakParseErrorOwned {
        InvalidValue(AzString),
    }

    impl PageBreakParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> PageBreakParseErrorOwned {
            match self {
                Self::InvalidValue(s) => PageBreakParseErrorOwned::InvalidValue((*s).to_string().into()),
            }
        }
    }

    impl PageBreakParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> PageBreakParseError<'_> {
            match self {
                Self::InvalidValue(s) => PageBreakParseError::InvalidValue(s.as_str()),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `page-break` value.
    pub fn parse_page_break(input: &str) -> Result<PageBreak, PageBreakParseError<'_>> {
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

    /// Error returned when parsing a `break-inside` value.
    #[derive(Clone, PartialEq, Eq)]
    pub enum BreakInsideParseError<'a> {
        InvalidValue(&'a str),
    }

    impl_debug_as_display!(BreakInsideParseError<'a>);
    impl_display! { BreakInsideParseError<'a>, {
        InvalidValue(v) => format!("Invalid break-inside value: \"{}\"", v),
    }}

    /// Owned version of [`BreakInsideParseError`] for FFI and storage.
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum BreakInsideParseErrorOwned {
        InvalidValue(AzString),
    }

    impl BreakInsideParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> BreakInsideParseErrorOwned {
            match self {
                Self::InvalidValue(s) => BreakInsideParseErrorOwned::InvalidValue((*s).to_string().into()),
            }
        }
    }

    impl BreakInsideParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> BreakInsideParseError<'_> {
            match self {
                Self::InvalidValue(s) => BreakInsideParseError::InvalidValue(s.as_str()),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `break-inside` value.
    pub fn parse_break_inside(
        input: &str,
    ) -> Result<BreakInside, BreakInsideParseError<'_>> {
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
            #[derive(Clone, PartialEq, Eq)]
            pub enum $error_name<'a> {
                ParseInt(ParseIntError, &'a str),
                ParseIntOwned(&'a str, &'a str),
                NegativeValue(&'a str),
            }

            impl_debug_as_display!($error_name<'a>);
            impl_display! { $error_name<'a>, {
                ParseInt(e, s) => format!("Invalid integer for {}: \"{}\". Reason: {}", $prop_name, s, e),
                ParseIntOwned(e, s) => format!("Invalid integer for {}: \"{}\". Reason: {}", $prop_name, s, e),
                NegativeValue(s) => format!("Invalid value for {}: \"{}\". Value cannot be negative.", $prop_name, s),
            }}

            #[derive(Debug, Clone, PartialEq, Eq)]
            #[repr(C, u8)]
            pub enum $error_owned_name {
                ParseInt(ParseIntErrorWithInput),
                NegativeValue(AzString),
            }

            impl $error_name<'_> {
                #[must_use] pub fn to_contained(&self) -> $error_owned_name {
                    match self {
                        Self::ParseInt(e, s) => $error_owned_name::ParseInt(ParseIntErrorWithInput { error: e.to_string().into(), input: s.to_string().into() }),
                        Self::ParseIntOwned(e, s) => $error_owned_name::ParseInt(ParseIntErrorWithInput { error: e.to_string().into(), input: s.to_string().into() }),
                        Self::NegativeValue(s) => $error_owned_name::NegativeValue(s.to_string().into()),
                    }
                }
            }

            impl $error_owned_name {
                #[must_use] pub fn to_shared(&self) -> $error_name<'_> {
                     match self {
                        Self::ParseInt(e) => $error_name::ParseIntOwned(e.error.as_str(), e.input.as_str()),
                        Self::NegativeValue(s) => $error_name::NegativeValue(s),
                    }
                }
            }

            pub fn $fn_name(input: &str) -> Result<$struct_name, $error_name<'_>> {
                let trimmed = input.trim();
                let val: i32 = trimmed.parse().map_err(|e| $error_name::ParseInt(e, trimmed))?;
                if val < 0 {
                    return Err($error_name::NegativeValue(trimmed));
                }
                Ok($struct_name { inner: u32::try_from(val).unwrap_or(0) })
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

    /// Error returned when parsing a `box-decoration-break` value.
    #[derive(Clone, PartialEq, Eq)]
    pub enum BoxDecorationBreakParseError<'a> {
        InvalidValue(&'a str),
    }

    impl_debug_as_display!(BoxDecorationBreakParseError<'a>);
    impl_display! { BoxDecorationBreakParseError<'a>, {
        InvalidValue(v) => format!("Invalid box-decoration-break value: \"{}\"", v),
    }}

    /// Owned version of [`BoxDecorationBreakParseError`] for FFI and storage.
    #[derive(Debug, Clone, PartialEq, Eq)]
    #[repr(C, u8)]
    pub enum BoxDecorationBreakParseErrorOwned {
        InvalidValue(AzString),
    }

    impl BoxDecorationBreakParseError<'_> {
        #[must_use] pub fn to_contained(&self) -> BoxDecorationBreakParseErrorOwned {
            match self {
                Self::InvalidValue(s) => {
                    BoxDecorationBreakParseErrorOwned::InvalidValue((*s).to_string().into())
                }
            }
        }
    }

    impl BoxDecorationBreakParseErrorOwned {
        #[must_use] pub fn to_shared(&self) -> BoxDecorationBreakParseError<'_> {
            match self {
                Self::InvalidValue(s) => BoxDecorationBreakParseError::InvalidValue(s.as_str()),
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error if `input` is not a valid CSS `box-decoration-break` value.
    pub fn parse_box_decoration_break(
        input: &str,
    ) -> Result<BoxDecorationBreak, BoxDecorationBreakParseError<'_>> {
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
