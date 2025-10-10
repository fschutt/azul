//! CSS properties for managing content overflow.

use alloc::string::{String, ToString};

use crate::{
    parser::{impl_debug_as_display, impl_display},
    props::formatter::PrintAsCssValue,
};

/// Represents an `overflow-x` or `overflow-y` property.
///
/// Determines what to do when content overflows an element's box.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutOverflow {
    /// Always shows a scroll bar, overflows on scroll.
    Scroll,
    /// Shows a scroll bar only when content overflows. This is the default.
    #[default]
    Auto,
    /// Clips overflowing content. The rest of the content will be invisible.
    Hidden,
    /// Content is not clipped and renders outside the element's box.
    Visible,
    /// Similar to `hidden`, clips the content at the box's edge.
    Clip,
}

impl LayoutOverflow {
    /// Returns whether this overflow value requires a scrollbar to be displayed.
    ///
    /// - `overflow: scroll` always shows the scrollbar.
    /// - `overflow: auto` only shows the scrollbar if the content is currently overflowing.
    /// - `overflow: hidden`, `overflow: visible`, and `overflow: clip` do not show any scrollbars.
    pub fn needs_scrollbar(&self, currently_overflowing: bool) -> bool {
        match self {
            LayoutOverflow::Scroll => true,
            LayoutOverflow::Auto => currently_overflowing,
            LayoutOverflow::Hidden | LayoutOverflow::Visible | LayoutOverflow::Clip => false,
        }
    }

    /// Returns `true` if the overflow type is `visible`, which is the only
    /// overflow type that doesn't clip its children.
    pub fn is_overflow_visible(&self) -> bool {
        *self == LayoutOverflow::Visible
    }

    /// Returns `true` if the overflow type is `hidden`.
    pub fn is_overflow_hidden(&self) -> bool {
        *self == LayoutOverflow::Hidden
    }
}

impl PrintAsCssValue for LayoutOverflow {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutOverflow::Scroll => "scroll",
            LayoutOverflow::Auto => "auto",
            LayoutOverflow::Hidden => "hidden",
            LayoutOverflow::Visible => "visible",
            LayoutOverflow::Clip => "clip",
        })
    }
}

// -- Parser

/// Error returned when parsing an `overflow` property fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutOverflowParseError<'a> {
    /// The provided value is not a valid `overflow` keyword.
    InvalidValue(&'a str),
}

impl_debug_as_display!(LayoutOverflowParseError<'a>);
impl_display! { LayoutOverflowParseError<'a>, {
    InvalidValue(val) => format!("Invalid overflow value: \"{}\". Expected 'scroll', 'auto', 'hidden', 'visible', or 'clip'.", val),
}}

/// An owned version of `LayoutOverflowParseError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutOverflowParseErrorOwned {
    InvalidValue(String),
}

impl<'a> LayoutOverflowParseError<'a> {
    /// Converts the borrowed error into an owned error.
    pub fn to_contained(&self) -> LayoutOverflowParseErrorOwned {
        match self {
            LayoutOverflowParseError::InvalidValue(s) => {
                LayoutOverflowParseErrorOwned::InvalidValue(s.to_string())
            }
        }
    }
}

impl LayoutOverflowParseErrorOwned {
    /// Converts the owned error back into a borrowed error.
    pub fn to_shared<'a>(&'a self) -> LayoutOverflowParseError<'a> {
        match self {
            LayoutOverflowParseErrorOwned::InvalidValue(s) => {
                LayoutOverflowParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// Parses a `LayoutOverflow` from a string slice.
pub fn parse_layout_overflow<'a>(
    input: &'a str,
) -> Result<LayoutOverflow, LayoutOverflowParseError<'a>> {
    let input_trimmed = input.trim();
    match input_trimmed {
        "scroll" => Ok(LayoutOverflow::Scroll),
        "auto" => Ok(LayoutOverflow::Auto),
        "hidden" => Ok(LayoutOverflow::Hidden),
        "visible" => Ok(LayoutOverflow::Visible),
        "clip" => Ok(LayoutOverflow::Clip),
        _ => Err(LayoutOverflowParseError::InvalidValue(input)),
    }
}
