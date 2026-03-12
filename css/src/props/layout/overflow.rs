//! CSS properties for managing content overflow.

use alloc::string::{String, ToString};
use crate::corety::AzString;

use crate::props::formatter::PrintAsCssValue;

// +spec:overflow:647a7b - overflow property (visible/hidden/clip/scroll/auto), overflow-clip-margin, text-overflow defined in CSS Overflow 3
/// Represents an `overflow-x` or `overflow-y` property.
///
/// Determines what to do when content overflows an element's box.
// +spec:overflow:3526f7 - overflow property with scroll/clip/hidden/visible/auto values
// +spec:overflow:36c4f6 - overflow-x/overflow-y properties with clip value
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutOverflow {
    /// Always shows a scroll bar, overflows on scroll.
    Scroll,
    /// Shows a scroll bar only when content overflows.
    Auto,
    /// Clips overflowing content. The rest of the content will be invisible.
    Hidden,
    /// Content is not clipped and renders outside the element's box. This is the CSS default.
    // +spec:overflow:236100 - initial value of 'overflow' is 'visible'
    #[default]
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
    // +spec:overflow:2bf182 - overflow:scroll always shows scrollbar whether or not content is clipped
    // +spec:overflow:84cd40 - scroll value always displays scrollbar for accessing clipped content
    // +spec:overflow:8fcdd8 - auto causes scrolling mechanism for overflowing boxes (table exception is UA-level)
    pub fn needs_scrollbar(&self, currently_overflowing: bool) -> bool {
        match self {
            LayoutOverflow::Scroll => true,
            LayoutOverflow::Auto => currently_overflowing,
            LayoutOverflow::Hidden | LayoutOverflow::Visible | LayoutOverflow::Clip => false,
        }
    }

    // +spec:overflow:145749 - overflow:hidden clips content to containing element box
    // +spec:overflow:3dc18e - overflow:hidden clips content with no scrolling UI
    // +spec:overflow:81e306 - clipping region clips all aspects outside it; clipped content does not cause overflow
    // +spec:overflow:fd38ce - overflow properties specify whether a box's content is clipped / scroll container
    pub fn is_clipped(&self) -> bool {
        // All overflow values except 'visible' clip their content
        matches!(
            self,
            LayoutOverflow::Hidden
                | LayoutOverflow::Clip
                | LayoutOverflow::Auto
                | LayoutOverflow::Scroll
        )
    }

    // +spec:overflow:3be57c - overflow:hidden disables user scrolling but programmatic scrolling still works
    pub fn is_scroll(&self) -> bool {
        matches!(self, LayoutOverflow::Scroll)
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
#[derive(Clone, PartialEq, Eq)]
pub enum LayoutOverflowParseError<'a> {
    /// The provided value is not a valid `overflow` keyword.
    InvalidValue(&'a str),
}

impl_debug_as_display!(LayoutOverflowParseError<'a>);
impl_display! { LayoutOverflowParseError<'a>, {
    InvalidValue(val) => format!(
        "Invalid overflow value: \"{}\". Expected 'scroll', 'auto', 'hidden', 'visible', or 'clip'.", val
    ),
}}

/// An owned version of `LayoutOverflowParseError`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutOverflowParseErrorOwned {
    InvalidValue(AzString),
}

impl<'a> LayoutOverflowParseError<'a> {
    /// Converts the borrowed error into an owned error.
    pub fn to_contained(&self) -> LayoutOverflowParseErrorOwned {
        match self {
            LayoutOverflowParseError::InvalidValue(s) => {
                LayoutOverflowParseErrorOwned::InvalidValue(s.to_string().into())
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
        "auto" | "overlay" => Ok(LayoutOverflow::Auto), // +spec:overflow:6120e6 - "overlay" is a legacy value alias of "auto"
        "hidden" => Ok(LayoutOverflow::Hidden),
        "visible" => Ok(LayoutOverflow::Visible),
        "clip" => Ok(LayoutOverflow::Clip),
        _ => Err(LayoutOverflowParseError::InvalidValue(input)),
    }
}

// -- StyleScrollbarGutter --
// +spec:box-model:e98b7c - scrollbar gutter: space between inner border edge and outer padding edge

/// Represents the `scrollbar-gutter` CSS property.
///
/// Controls whether space is reserved for the scrollbar, preventing
/// layout shifts when content overflows.
// +spec:overflow:da4bbc - scrollbar-gutter affects gutter presence, not scrollbar visibility
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleScrollbarGutter {
    /// No scrollbar gutter is reserved.
    #[default]
    Auto,
    /// Space is reserved for the scrollbar on one edge.
    Stable,
    /// Space is reserved for the scrollbar on both edges.
    StableBothEdges,
}

impl PrintAsCssValue for StyleScrollbarGutter {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleScrollbarGutter::Auto => "auto",
            StyleScrollbarGutter::Stable => "stable",
            StyleScrollbarGutter::StableBothEdges => "stable both-edges",
        })
    }
}

// -- Parser for StyleScrollbarGutter

/// Error returned when parsing a `scrollbar-gutter` property fails.
#[derive(Clone, PartialEq, Eq)]
pub enum StyleScrollbarGutterParseError<'a> {
    /// The provided value is not a valid `scrollbar-gutter` keyword.
    InvalidValue(&'a str),
}

impl_debug_as_display!(StyleScrollbarGutterParseError<'a>);
impl_display! { StyleScrollbarGutterParseError<'a>, {
    InvalidValue(val) => format!(
        "Invalid scrollbar-gutter value: \"{}\". Expected 'auto', 'stable', or 'stable both-edges'.", val
    ),
}}

/// An owned version of `StyleScrollbarGutterParseError`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleScrollbarGutterParseErrorOwned {
    InvalidValue(AzString),
}

impl<'a> StyleScrollbarGutterParseError<'a> {
    /// Converts the borrowed error into an owned error.
    pub fn to_contained(&self) -> StyleScrollbarGutterParseErrorOwned {
        match self {
            StyleScrollbarGutterParseError::InvalidValue(s) => {
                StyleScrollbarGutterParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}

impl StyleScrollbarGutterParseErrorOwned {
    /// Converts the owned error back into a borrowed error.
    pub fn to_shared<'a>(&'a self) -> StyleScrollbarGutterParseError<'a> {
        match self {
            StyleScrollbarGutterParseErrorOwned::InvalidValue(s) => {
                StyleScrollbarGutterParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// Parses a `StyleScrollbarGutter` from a string slice.
pub fn parse_style_scrollbar_gutter<'a>(
    input: &'a str,
) -> Result<StyleScrollbarGutter, StyleScrollbarGutterParseError<'a>> {
    let input_trimmed = input.trim();
    match input_trimmed {
        "auto" => Ok(StyleScrollbarGutter::Auto),
        "stable" => Ok(StyleScrollbarGutter::Stable),
        "stable both-edges" => Ok(StyleScrollbarGutter::StableBothEdges),
        _ => Err(StyleScrollbarGutterParseError::InvalidValue(input)),
    }
}

// -- StyleOverflowClipMargin --

/// Represents the `overflow-clip-margin` CSS property.
///
/// Determines how far outside the element's box the content may paint
/// before being clipped when `overflow: clip` is used.
// +spec:overflow:455786 - overflow-clip-margin has no effect on hidden/scroll, only on clip
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleOverflowClipMargin {
    /// The clip margin distance.
    pub inner: crate::props::basic::pixel::PixelValue,
}

impl PrintAsCssValue for StyleOverflowClipMargin {
    fn print_as_css_value(&self) -> String {
        self.inner.print_as_css_value()
    }
}

/// Error returned when parsing an `overflow-clip-margin` property fails.
#[derive(Clone, PartialEq, Eq)]
pub enum StyleOverflowClipMarginParseError<'a> {
    InvalidValue(&'a str),
}

impl_debug_as_display!(StyleOverflowClipMarginParseError<'a>);
impl_display! { StyleOverflowClipMarginParseError<'a>, {
    InvalidValue(val) => format!("Invalid overflow-clip-margin value: \"{}\"", val),
}}

/// An owned version of `StyleOverflowClipMarginParseError`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleOverflowClipMarginParseErrorOwned {
    InvalidValue(AzString),
}

impl<'a> StyleOverflowClipMarginParseError<'a> {
    pub fn to_contained(&self) -> StyleOverflowClipMarginParseErrorOwned {
        match self {
            StyleOverflowClipMarginParseError::InvalidValue(s) => {
                StyleOverflowClipMarginParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}

impl StyleOverflowClipMarginParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> StyleOverflowClipMarginParseError<'a> {
        match self {
            StyleOverflowClipMarginParseErrorOwned::InvalidValue(s) => {
                StyleOverflowClipMarginParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// Parses a `StyleOverflowClipMargin` from a string slice.
pub fn parse_style_overflow_clip_margin<'a>(
    input: &'a str,
) -> Result<StyleOverflowClipMargin, StyleOverflowClipMarginParseError<'a>> {
    use crate::props::basic::pixel::parse_pixel_value;
    match parse_pixel_value(input) {
        Ok(pv) => Ok(StyleOverflowClipMargin { inner: pv }),
        Err(_) => Err(StyleOverflowClipMarginParseError::InvalidValue(input)),
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_layout_overflow_valid() {
        assert_eq!(
            parse_layout_overflow("visible").unwrap(),
            LayoutOverflow::Visible
        );
        assert_eq!(
            parse_layout_overflow("hidden").unwrap(),
            LayoutOverflow::Hidden
        );
        assert_eq!(parse_layout_overflow("clip").unwrap(), LayoutOverflow::Clip);
        assert_eq!(
            parse_layout_overflow("scroll").unwrap(),
            LayoutOverflow::Scroll
        );
        assert_eq!(parse_layout_overflow("auto").unwrap(), LayoutOverflow::Auto);
    }

    #[test]
    fn test_parse_layout_overflow_whitespace() {
        assert_eq!(
            parse_layout_overflow("  scroll  ").unwrap(),
            LayoutOverflow::Scroll
        );
    }

    #[test]
    fn test_parse_layout_overflow_invalid() {
        assert!(parse_layout_overflow("none").is_err());
        assert!(parse_layout_overflow("").is_err());
        assert!(parse_layout_overflow("auto scroll").is_err());
        assert!(parse_layout_overflow("hidden-x").is_err());
    }

    #[test]
    fn test_needs_scrollbar() {
        assert!(LayoutOverflow::Scroll.needs_scrollbar(false));
        assert!(LayoutOverflow::Scroll.needs_scrollbar(true));
        assert!(LayoutOverflow::Auto.needs_scrollbar(true));
        assert!(!LayoutOverflow::Auto.needs_scrollbar(false));
        assert!(!LayoutOverflow::Hidden.needs_scrollbar(true));
        assert!(!LayoutOverflow::Visible.needs_scrollbar(true));
        assert!(!LayoutOverflow::Clip.needs_scrollbar(true));
    }
}
