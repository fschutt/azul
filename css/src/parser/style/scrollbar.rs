use crate::{css_properties::*, parser::*};

/// Holds info necessary for layouting / styling scrollbars (-webkit-scrollbar)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ScrollbarInfo {
    /// Total width (or height for vertical scrollbars) of the scrollbar in pixels
    pub width: LayoutWidth,
    /// Padding of the scrollbar tracker, in pixels. The inner bar is `width - padding` pixels
    /// wide.
    pub padding_left: LayoutPaddingLeft,
    /// Padding of the scrollbar (right)
    pub padding_right: LayoutPaddingRight,
    /// Style of the scrollbar background
    /// (`-webkit-scrollbar` / `-webkit-scrollbar-track` / `-webkit-scrollbar-track-piece`
    /// combined)
    pub track: StyleBackgroundContent,
    /// Style of the scrollbar thumbs (the "up" / "down" arrows), (`-webkit-scrollbar-thumb`)
    pub thumb: StyleBackgroundContent,
    /// Styles the directional buttons on the scrollbar (`-webkit-scrollbar-button`)
    pub button: StyleBackgroundContent,
    /// If two scrollbars are present, addresses the (usually) bottom corner
    /// of the scrollable element, where two scrollbars might meet (`-webkit-scrollbar-corner`)
    pub corner: StyleBackgroundContent,
    /// Addresses the draggable resizing handle that appears above the
    /// `corner` at the bottom corner of some elements (`-webkit-resizer`)
    pub resizer: StyleBackgroundContent,
}

impl Default for ScrollbarInfo {
    fn default() -> Self {
        ScrollbarInfo {
            width: LayoutWidth::px(17.0),
            padding_left: LayoutPaddingLeft::px(2.0),
            padding_right: LayoutPaddingRight::px(2.0),
            track: StyleBackgroundContent::Color(ColorU {
                r: 241,
                g: 241,
                b: 241,
                a: 255,
            }),
            thumb: StyleBackgroundContent::Color(ColorU {
                r: 193,
                g: 193,
                b: 193,
                a: 255,
            }),
            button: StyleBackgroundContent::Color(ColorU {
                r: 163,
                g: 163,
                b: 163,
                a: 255,
            }),
            corner: StyleBackgroundContent::default(),
            resizer: StyleBackgroundContent::default(),
        }
    }
}

/// Scrollbar style
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ScrollbarStyle {
    /// Vertical scrollbar style, if any
    pub horizontal: ScrollbarInfo,
    /// Horizontal scrollbar style, if any
    pub vertical: ScrollbarInfo,
}

#[derive(Clone, PartialEq)]
pub enum CssScrollbarStyleParseError<'a> {
    Invalid(&'a str),
}

impl_debug_as_display!(CssScrollbarStyleParseError<'a>);
impl_display! { CssScrollbarStyleParseError<'a>, {
    Invalid(e) => format!("Invalid scrollbar style: \"{}\"", e),
}}

/// Owned version of CssScrollbarStyleParseError.
#[derive(Debug, Clone, PartialEq)]
pub enum CssScrollbarStyleParseErrorOwned {
    Invalid(String),
}

impl<'a> CssScrollbarStyleParseError<'a> {
    pub fn to_contained(&self) -> CssScrollbarStyleParseErrorOwned {
        match self {
            CssScrollbarStyleParseError::Invalid(s) => {
                CssScrollbarStyleParseErrorOwned::Invalid(s.to_string())
            }
        }
    }
}

impl CssScrollbarStyleParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssScrollbarStyleParseError<'a> {
        match self {
            CssScrollbarStyleParseErrorOwned::Invalid(s) => {
                CssScrollbarStyleParseError::Invalid(s.as_str())
            }
        }
    }
}

pub fn parse_scrollbar_style<'a>(
    input: &'a str,
) -> Result<ScrollbarStyle, CssScrollbarStyleParseError<'a>> {
    Ok(ScrollbarStyle::default()) // TODO!
}
