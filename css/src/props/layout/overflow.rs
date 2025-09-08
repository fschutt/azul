//! Layout overflow properties

use alloc::string::String;
use core::fmt;

use crate::{error::CssParsingError, props::formatter::FormatAsCssValue};

/// CSS overflow property values
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutOverflow {
    /// Content is not clipped and may be rendered outside padding box
    Visible,
    /// Content is clipped and no scrollbars are provided
    Hidden,
    /// Content is clipped and scrollbars are provided when needed
    Scroll,
    /// Content is clipped and scrollbars are provided automatically when needed
    Auto,
}

impl Default for LayoutOverflow {
    fn default() -> Self {
        LayoutOverflow::Visible
    }
}

impl fmt::Display for LayoutOverflow {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::LayoutOverflow::*;
        let s = match self {
            Visible => "visible",
            Hidden => "hidden",
            Scroll => "scroll",
            Auto => "auto",
        };
        write!(f, "{}", s)
    }
}

impl FormatAsCssValue for LayoutOverflow {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

/// Parse layout overflow value
#[cfg(feature = "parser")]
pub fn parse_layout_overflow<'a>(input: &'a str) -> Result<LayoutOverflow, CssParsingError<'a>> {
    use self::LayoutOverflow::*;
    match input.trim() {
        "visible" => Ok(Visible),
        "hidden" => Ok(Hidden),
        "scroll" => Ok(Scroll),
        "auto" => Ok(Auto),
        _ => Err(CssParsingError::InvalidValue(input)),
    }
}
