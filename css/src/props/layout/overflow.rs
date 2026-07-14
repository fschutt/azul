//! CSS properties for managing content overflow.

use alloc::string::{String, ToString};
use crate::corety::{AzString, OptionF32};

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
    #[must_use] pub const fn needs_scrollbar(&self, currently_overflowing: bool) -> bool {
        match self {
            Self::Scroll => true,
            Self::Auto => currently_overflowing,
            Self::Hidden | Self::Visible | Self::Clip => false,
        }
    }

    // +spec:overflow:145749 - overflow:hidden clips content to containing element box
    // +spec:overflow:3dc18e - overflow:hidden clips content with no scrolling UI
    // +spec:overflow:81e306 - clipping region clips all aspects outside it; clipped content does not cause overflow
    // +spec:overflow:fd38ce - overflow properties specify whether a box's content is clipped / scroll container
    /// Returns `true` if this overflow value clips content (everything except `visible`).
    #[must_use] pub const fn is_clipped(&self) -> bool {
        // All overflow values except 'visible' clip their content
        matches!(
            self,
            Self::Hidden
                | Self::Clip
                | Self::Auto
                | Self::Scroll
        )
    }

    // +spec:overflow:3be57c - overflow:hidden disables user scrolling but programmatic scrolling still works
    /// Returns `true` if the overflow type is `scroll`.
    #[must_use] pub const fn is_scroll(&self) -> bool {
        matches!(self, Self::Scroll)
    }

    /// Returns `true` if the overflow type is `visible`, which is the only
    /// overflow type that doesn't clip its children.
    #[must_use] pub fn is_overflow_visible(&self) -> bool {
        *self == Self::Visible
    }

    /// Returns `true` if the overflow type is `hidden`.
    #[must_use] pub fn is_overflow_hidden(&self) -> bool {
        *self == Self::Hidden
    }

    // +spec:overflow:833078 - visible/clip compute to auto/hidden if other axis is scrollable
    /// Resolves the computed value per CSS Overflow 3 § 3.1:
    /// visible/clip values compute to auto/hidden (respectively)
    /// if the other axis is neither visible nor clip.
    #[must_use] pub const fn resolve_computed(self, other_axis: Self) -> Self {
        let other_is_scrollable = !matches!(other_axis, Self::Visible | Self::Clip);
        if other_is_scrollable {
            match self {
                Self::Visible => Self::Auto,
                Self::Clip => Self::Hidden,
                other => other,
            }
        } else {
            self
        }
    }
}

impl PrintAsCssValue for LayoutOverflow {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Scroll => "scroll",
            Self::Auto => "auto",
            Self::Hidden => "hidden",
            Self::Visible => "visible",
            Self::Clip => "clip",
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

impl LayoutOverflowParseError<'_> {
    /// Converts the borrowed error into an owned error.
    #[must_use] pub fn to_contained(&self) -> LayoutOverflowParseErrorOwned {
        match self {
            LayoutOverflowParseError::InvalidValue(s) => {
                LayoutOverflowParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

impl LayoutOverflowParseErrorOwned {
    /// Converts the owned error back into a borrowed error.
    #[must_use] pub fn to_shared(&self) -> LayoutOverflowParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                LayoutOverflowParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// Parses a `LayoutOverflow` from a string slice.
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `overflow` value.
pub fn parse_layout_overflow(
    input: &str,
) -> Result<LayoutOverflow, LayoutOverflowParseError<'_>> {
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
            Self::Auto => "auto",
            Self::Stable => "stable",
            Self::StableBothEdges => "stable both-edges",
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

impl StyleScrollbarGutterParseError<'_> {
    /// Converts the borrowed error into an owned error.
    #[must_use] pub fn to_contained(&self) -> StyleScrollbarGutterParseErrorOwned {
        match self {
            StyleScrollbarGutterParseError::InvalidValue(s) => {
                StyleScrollbarGutterParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

impl StyleScrollbarGutterParseErrorOwned {
    /// Converts the owned error back into a borrowed error.
    #[must_use] pub fn to_shared(&self) -> StyleScrollbarGutterParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                StyleScrollbarGutterParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// Parses a `StyleScrollbarGutter` from a string slice.
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `scrollbar-gutter` value.
pub fn parse_style_scrollbar_gutter(
    input: &str,
) -> Result<StyleScrollbarGutter, StyleScrollbarGutterParseError<'_>> {
    let input_trimmed = input.trim();
    match input_trimmed {
        "auto" => Ok(StyleScrollbarGutter::Auto),
        "stable" => Ok(StyleScrollbarGutter::Stable),
        "stable both-edges" => Ok(StyleScrollbarGutter::StableBothEdges),
        _ => Err(StyleScrollbarGutterParseError::InvalidValue(input)),
    }
}

// -- VisualBox --

// +spec:overflow:f6955f - box edge origin for overflow-clip-margin
/// Represents the `<visual-box>` value used as the overflow clip edge origin.
///
/// Specifies which box edge to use as the starting point for the clip region.
/// Defaults to `padding-box` per CSS Overflow Module Level 3.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum VisualBox {
    /// Clip edge starts at the content box edge.
    ContentBox,
    /// Clip edge starts at the padding box edge (default).
    #[default]
    PaddingBox,
    /// Clip edge starts at the border box edge.
    BorderBox,
}

impl PrintAsCssValue for VisualBox {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::ContentBox => "content-box",
            Self::PaddingBox => "padding-box",
            Self::BorderBox => "border-box",
        })
    }
}

// -- StyleOverflowClipMargin --

/// Represents the `overflow-clip-margin` CSS property.
///
/// Determines how far outside the element's box the content may paint
/// before being clipped when `overflow: clip` is used.
/// Syntax: `<visual-box> || <length [0,∞]>`
// +spec:overflow:455786 - overflow-clip-margin has no effect on hidden/scroll, only on clip
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleOverflowClipMargin {
    /// The box edge to use as the clip origin (content-box, padding-box, or border-box).
    pub clip_edge: VisualBox,
    /// The clip margin distance beyond the clip edge.
    pub inner: crate::props::basic::pixel::PixelValue,
}

impl PrintAsCssValue for StyleOverflowClipMargin {
    fn print_as_css_value(&self) -> String {
        let edge = self.clip_edge.print_as_css_value();
        let len = self.inner.print_as_css_value();
        #[allow(clippy::float_cmp)] // exact zero check: value is default-initialized, not computed
        if self.inner.number.get() == 0.0 {
            edge
        } else if self.clip_edge == VisualBox::PaddingBox {
            len
        } else {
            format!("{edge} {len}")
        }
    }
}

/// Error returned when parsing an `overflow-clip-margin` property fails.
#[derive(Clone, PartialEq, Eq)]
pub enum StyleOverflowClipMarginParseError<'a> {
    /// The provided value is not a valid `overflow-clip-margin` value.
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

impl StyleOverflowClipMarginParseError<'_> {
    /// Converts the borrowed error into an owned error.
    #[must_use] pub fn to_contained(&self) -> StyleOverflowClipMarginParseErrorOwned {
        match self {
            StyleOverflowClipMarginParseError::InvalidValue(s) => {
                StyleOverflowClipMarginParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

impl StyleOverflowClipMarginParseErrorOwned {
    /// Converts the owned error back into a borrowed error.
    #[must_use] pub fn to_shared(&self) -> StyleOverflowClipMarginParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                StyleOverflowClipMarginParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// Parses a `StyleOverflowClipMargin` from a string slice.
///
/// Syntax: `<visual-box> || <length [0,∞]>`
/// The `<visual-box>` defaults to `padding-box` if omitted.
/// The `<length>` defaults to `0px` if omitted.
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `overflow-clip-margin` value.
pub fn parse_style_overflow_clip_margin(
    input: &str,
) -> Result<StyleOverflowClipMargin, StyleOverflowClipMarginParseError<'_>> {
    use crate::props::basic::pixel::parse_pixel_value;

    let input_trimmed = input.trim();
    let mut clip_edge = None;
    let mut length = None;

    for token in input_trimmed.split_whitespace() {
        match token {
            "content-box" if clip_edge.is_none() => clip_edge = Some(VisualBox::ContentBox),
            "padding-box" if clip_edge.is_none() => clip_edge = Some(VisualBox::PaddingBox),
            "border-box" if clip_edge.is_none() => clip_edge = Some(VisualBox::BorderBox),
            _ if length.is_none() => {
                match parse_pixel_value(token) {
                    Ok(pv) => length = Some(pv),
                    Err(_) => return Err(StyleOverflowClipMarginParseError::InvalidValue(input)),
                }
            }
            _ => return Err(StyleOverflowClipMarginParseError::InvalidValue(input)),
        }
    }

    if clip_edge.is_none() && length.is_none() {
        return Err(StyleOverflowClipMarginParseError::InvalidValue(input));
    }

    Ok(StyleOverflowClipMargin {
        clip_edge: clip_edge.unwrap_or_default(),
        inner: length.unwrap_or_default(),
    })
}

// -- StyleClipRect --

/// Represents the deprecated CSS `clip` property value `rect(top, right, bottom, left)`.
///
/// Each edge can be a length or `auto`. When `auto`, the edge matches the
/// element's generated border box edge:
/// - `auto` for top/left = 0
/// - `auto` for bottom = used height + vertical padding + vertical border
/// - `auto` for right = used width + horizontal padding + horizontal border
///
/// Negative lengths are permitted.
// +spec:overflow:297dc3 - clip rect() auto values resolve to border box edges
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleClipRect {
    /// Top edge offset in pixels. `None` means `auto` (= 0).
    pub top: OptionF32,
    /// Right edge offset in pixels. `None` means `auto` (= used width + horiz padding + horiz border).
    pub right: OptionF32,
    /// Bottom edge offset in pixels. `None` means `auto` (= used height + vert padding + vert border).
    pub bottom: OptionF32,
    /// Left edge offset in pixels. `None` means `auto` (= 0).
    pub left: OptionF32,
}

impl StyleClipRect {
    /// Resolves `auto` values to border box edges given the element's
    /// used width/height and padding/border sizes.
    ///
    /// Returns `(top, right, bottom, left)` in pixels.
    #[must_use] pub fn resolve(
        &self,
        used_width: f32,
        used_height: f32,
        padding_left: f32,
        padding_right: f32,
        padding_top: f32,
        padding_bottom: f32,
        border_left: f32,
        border_right: f32,
        border_top: f32,
        border_bottom: f32,
    ) -> (f32, f32, f32, f32) {
        let top = self.top.into_option().unwrap_or(0.0);
        let left = self.left.into_option().unwrap_or(0.0);
        let bottom = self
            .bottom
            .into_option()
            .unwrap_or(used_height + padding_top + padding_bottom + border_top + border_bottom);
        let right = self
            .right
            .into_option()
            .unwrap_or(used_width + padding_left + padding_right + border_left + border_right);
        (top, right, bottom, left)
    }
}

impl PrintAsCssValue for StyleClipRect {
    fn print_as_css_value(&self) -> String {
        fn fmt_edge(o: OptionF32) -> String {
            o.into_option()
                .map_or_else(|| String::from("auto"), |v| format!("{v}px"))
        }
        format!(
            "rect({}, {}, {}, {})",
            fmt_edge(self.top),
            fmt_edge(self.right),
            fmt_edge(self.bottom),
            fmt_edge(self.left)
        )
    }
}

// -- Parser for StyleClipRect

/// Error returned when parsing a CSS `clip` property value fails.
#[derive(Clone, PartialEq, Eq)]
pub enum StyleClipRectParseError<'a> {
    /// The provided value is not a valid `clip` value.
    InvalidValue(&'a str),
}

impl_debug_as_display!(StyleClipRectParseError<'a>);
impl_display! { StyleClipRectParseError<'a>, {
    InvalidValue(val) => format!(
        "Invalid clip value: \"{}\". Expected 'auto' or 'rect(<top>, <right>, <bottom>, <left>)'.", val
    ),
}}

/// An owned version of `StyleClipRectParseError`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum StyleClipRectParseErrorOwned {
    InvalidValue(AzString),
}

impl StyleClipRectParseError<'_> {
    /// Converts the borrowed error into an owned error.
    #[must_use] pub fn to_contained(&self) -> StyleClipRectParseErrorOwned {
        match self {
            StyleClipRectParseError::InvalidValue(s) => {
                StyleClipRectParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

impl StyleClipRectParseErrorOwned {
    /// Converts the owned error back into a borrowed error.
    #[must_use] pub fn to_shared(&self) -> StyleClipRectParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                StyleClipRectParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
fn parse_clip_edge(token: &str) -> Result<OptionF32, StyleClipRectParseError<'_>> {
    use crate::props::basic::pixel::parse_pixel_value;

    let token = token.trim();
    if token.eq_ignore_ascii_case("auto") {
        return Ok(OptionF32::None);
    }
    let pv = parse_pixel_value(token)
        .map_err(|_| StyleClipRectParseError::InvalidValue(token))?;
    Ok(OptionF32::Some(pv.number.get()))
}

#[cfg(feature = "parser")]
/// Parses a `StyleClipRect` from a string slice.
///
/// Accepts:
/// - `auto` — equivalent to `rect(auto, auto, auto, auto)`.
/// - `rect(<top>, <right>, <bottom>, <left>)` — comma-separated form.
/// - `rect(<top> <right> <bottom> <left>)` — legacy space-separated form.
///
/// Each edge is either `auto` or a `<length>`. Negative lengths are permitted.
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `clip-rect` value.
pub fn parse_clip_rect(input: &str) -> Result<StyleClipRect, StyleClipRectParseError<'_>> {
    let trimmed = input.trim();

    if trimmed.eq_ignore_ascii_case("auto") {
        return Ok(StyleClipRect::default());
    }

    let inner = trimmed
        .strip_prefix("rect(")
        .or_else(|| trimmed.strip_prefix("RECT("))
        .and_then(|s| s.strip_suffix(')'))
        .ok_or(StyleClipRectParseError::InvalidValue(input))?;

    let inner = inner.trim();
    let parts: Vec<&str> = if inner.contains(',') {
        inner.split(',').map(str::trim).collect()
    } else {
        inner.split_whitespace().collect()
    };

    if parts.len() != 4 {
        return Err(StyleClipRectParseError::InvalidValue(input));
    }

    Ok(StyleClipRect {
        top: parse_clip_edge(parts[0])?,
        right: parse_clip_edge(parts[1])?,
        bottom: parse_clip_edge(parts[2])?,
        left: parse_clip_edge(parts[3])?,
    })
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

    #[test]
    fn test_parse_clip_rect_auto_keyword() {
        let r = parse_clip_rect("auto").unwrap();
        assert_eq!(r.top, OptionF32::None);
        assert_eq!(r.right, OptionF32::None);
        assert_eq!(r.bottom, OptionF32::None);
        assert_eq!(r.left, OptionF32::None);
    }

    #[test]
    fn test_parse_clip_rect_all_auto_in_rect() {
        let r = parse_clip_rect("rect(auto, auto, auto, auto)").unwrap();
        assert_eq!(r.top, OptionF32::None);
        assert_eq!(r.right, OptionF32::None);
        assert_eq!(r.bottom, OptionF32::None);
        assert_eq!(r.left, OptionF32::None);
    }

    #[test]
    fn test_parse_clip_rect_mixed_auto_and_lengths() {
        let r = parse_clip_rect("rect(10px, auto, 30px, auto)").unwrap();
        assert_eq!(r.top, OptionF32::Some(10.0));
        assert_eq!(r.right, OptionF32::None);
        assert_eq!(r.bottom, OptionF32::Some(30.0));
        assert_eq!(r.left, OptionF32::None);
    }

    #[test]
    fn test_parse_clip_rect_negative_lengths() {
        let r = parse_clip_rect("rect(-5px, 0px, -10px, 0px)").unwrap();
        assert_eq!(r.top, OptionF32::Some(-5.0));
        assert_eq!(r.right, OptionF32::Some(0.0));
        assert_eq!(r.bottom, OptionF32::Some(-10.0));
        assert_eq!(r.left, OptionF32::Some(0.0));
    }

    #[test]
    fn test_parse_clip_rect_legacy_space_separated() {
        // Legacy CSS 2.1 syntax used spaces instead of commas.
        let r = parse_clip_rect("rect(1px 2px 3px 4px)").unwrap();
        assert_eq!(r.top, OptionF32::Some(1.0));
        assert_eq!(r.right, OptionF32::Some(2.0));
        assert_eq!(r.bottom, OptionF32::Some(3.0));
        assert_eq!(r.left, OptionF32::Some(4.0));
    }

    #[test]
    fn test_parse_clip_rect_malformed() {
        assert!(parse_clip_rect("").is_err());
        assert!(parse_clip_rect("none").is_err());
        // Wrong number of edges.
        assert!(parse_clip_rect("rect(10px, 20px, 30px)").is_err());
        // Missing closing paren.
        assert!(parse_clip_rect("rect(10px, 20px, 30px, 40px").is_err());
        // Garbage edge.
        assert!(parse_clip_rect("rect(10px, abc, 30px, 40px)").is_err());
    }
}

#[cfg(all(test, feature = "parser"))]
mod autotest_generated {
    use crate::props::basic::pixel::PixelValue;
    use crate::props::basic::length::SizeMetric;

    use super::*;

    // ---------------------------------------------------------------------
    // Variant tables. Each is kept honest by an exhaustive `match` below:
    // adding a variant to the enum stops the index fn from compiling.
    // ---------------------------------------------------------------------

    const ALL_OVERFLOW: [LayoutOverflow; 5] = [
        LayoutOverflow::Scroll,
        LayoutOverflow::Auto,
        LayoutOverflow::Hidden,
        LayoutOverflow::Visible,
        LayoutOverflow::Clip,
    ];

    const fn overflow_variant_index(o: LayoutOverflow) -> usize {
        match o {
            LayoutOverflow::Scroll => 0,
            LayoutOverflow::Auto => 1,
            LayoutOverflow::Hidden => 2,
            LayoutOverflow::Visible => 3,
            LayoutOverflow::Clip => 4,
        }
    }

    const ALL_GUTTER: [StyleScrollbarGutter; 3] = [
        StyleScrollbarGutter::Auto,
        StyleScrollbarGutter::Stable,
        StyleScrollbarGutter::StableBothEdges,
    ];

    const fn gutter_variant_index(g: StyleScrollbarGutter) -> usize {
        match g {
            StyleScrollbarGutter::Auto => 0,
            StyleScrollbarGutter::Stable => 1,
            StyleScrollbarGutter::StableBothEdges => 2,
        }
    }

    const ALL_VISUAL_BOX: [VisualBox; 3] = [
        VisualBox::ContentBox,
        VisualBox::PaddingBox,
        VisualBox::BorderBox,
    ];

    const fn visual_box_variant_index(v: VisualBox) -> usize {
        match v {
            VisualBox::ContentBox => 0,
            VisualBox::PaddingBox => 1,
            VisualBox::BorderBox => 2,
        }
    }

    /// A value is "scrollable" (per CSS Overflow 3 § 3.1) when it is neither
    /// `visible` nor `clip` — i.e. it establishes a scroll container.
    fn is_scrollable(o: LayoutOverflow) -> bool {
        !matches!(o, LayoutOverflow::Visible | LayoutOverflow::Clip)
    }

    #[test]
    fn variant_tables_cover_every_variant_exactly_once() {
        for (i, o) in ALL_OVERFLOW.iter().enumerate() {
            assert_eq!(overflow_variant_index(*o), i);
        }
        for (i, g) in ALL_GUTTER.iter().enumerate() {
            assert_eq!(gutter_variant_index(*g), i);
        }
        for (i, v) in ALL_VISUAL_BOX.iter().enumerate() {
            assert_eq!(visual_box_variant_index(*v), i);
        }
    }

    // ---------------------------------------------------------------------
    // LayoutOverflow — predicates & invariants
    // ---------------------------------------------------------------------

    #[test]
    fn needs_scrollbar_truth_table_is_monotone_in_currently_overflowing() {
        for o in ALL_OVERFLOW {
            let idle = o.needs_scrollbar(false);
            let overflowing = o.needs_scrollbar(true);

            // A scrollbar that is shown while *not* overflowing must also be
            // shown while overflowing — the flag can only ever add scrollbars.
            assert!(
                !idle || overflowing,
                "{o:?} shows a scrollbar when idle but hides it when overflowing"
            );

            // Only `scroll` shows a scrollbar unconditionally; only `auto`
            // reacts to the flag; nothing else ever shows one.
            let (expect_idle, expect_overflowing) = match o {
                LayoutOverflow::Scroll => (true, true),
                LayoutOverflow::Auto => (false, true),
                LayoutOverflow::Hidden | LayoutOverflow::Visible | LayoutOverflow::Clip => {
                    (false, false)
                }
            };
            assert_eq!(idle, expect_idle, "needs_scrollbar(false) wrong for {o:?}");
            assert_eq!(
                overflowing, expect_overflowing,
                "needs_scrollbar(true) wrong for {o:?}"
            );

            // Anything that can show a scrollbar must also clip.
            assert!(!overflowing || o.is_clipped());
        }
    }

    #[test]
    fn is_clipped_is_exactly_the_negation_of_is_overflow_visible() {
        for o in ALL_OVERFLOW {
            assert_eq!(
                o.is_clipped(),
                !o.is_overflow_visible(),
                "is_clipped/is_overflow_visible disagree for {o:?}"
            );
            // Deterministic: repeated calls on the same value never differ.
            assert_eq!(o.is_clipped(), o.is_clipped());
        }
        assert!(!LayoutOverflow::Visible.is_clipped());
        assert!(LayoutOverflow::Hidden.is_clipped());
    }

    #[test]
    fn is_scroll_and_is_overflow_hidden_match_exactly_one_variant_each() {
        let scrolls: Vec<LayoutOverflow> =
            ALL_OVERFLOW.into_iter().filter(LayoutOverflow::is_scroll).collect();
        assert_eq!(scrolls, vec![LayoutOverflow::Scroll]);

        let hiddens: Vec<LayoutOverflow> = ALL_OVERFLOW
            .into_iter()
            .filter(LayoutOverflow::is_overflow_hidden)
            .collect();
        assert_eq!(hiddens, vec![LayoutOverflow::Hidden]);

        // `auto` is not `scroll`, even though both can produce a scrollbar.
        assert!(!LayoutOverflow::Auto.is_scroll());
        assert!(LayoutOverflow::Auto.needs_scrollbar(true));
    }

    #[test]
    fn default_overflow_is_visible_and_neither_clips_nor_scrolls() {
        let d = LayoutOverflow::default();
        assert_eq!(d, LayoutOverflow::Visible);
        assert!(d.is_overflow_visible());
        assert!(!d.is_clipped());
        assert!(!d.is_scroll());
        assert!(!d.is_overflow_hidden());
        assert!(!d.needs_scrollbar(false));
        assert!(!d.needs_scrollbar(true));
    }

    // ---------------------------------------------------------------------
    // LayoutOverflow::resolve_computed — CSS Overflow 3 § 3.1
    // ---------------------------------------------------------------------

    #[test]
    fn resolve_computed_is_identity_when_the_other_axis_is_not_scrollable() {
        for other in [LayoutOverflow::Visible, LayoutOverflow::Clip] {
            for o in ALL_OVERFLOW {
                assert_eq!(
                    o.resolve_computed(other),
                    o,
                    "{o:?} must be untouched when the other axis is {other:?}"
                );
            }
        }
    }

    #[test]
    fn resolve_computed_promotes_visible_to_auto_and_clip_to_hidden() {
        for other in [
            LayoutOverflow::Scroll,
            LayoutOverflow::Auto,
            LayoutOverflow::Hidden,
        ] {
            assert_eq!(
                LayoutOverflow::Visible.resolve_computed(other),
                LayoutOverflow::Auto
            );
            assert_eq!(
                LayoutOverflow::Clip.resolve_computed(other),
                LayoutOverflow::Hidden
            );
            // Already-scrollable values are left alone.
            for o in [
                LayoutOverflow::Scroll,
                LayoutOverflow::Auto,
                LayoutOverflow::Hidden,
            ] {
                assert_eq!(o.resolve_computed(other), o);
            }
        }
    }

    #[test]
    fn resolve_computed_is_idempotent_and_never_removes_clipping() {
        for o in ALL_OVERFLOW {
            for other in ALL_OVERFLOW {
                let once = o.resolve_computed(other);
                assert_eq!(
                    once.resolve_computed(other),
                    once,
                    "resolve_computed not idempotent for ({o:?}, {other:?})"
                );
                // Resolution only ever adds clipping, never takes it away.
                assert!(
                    !o.is_clipped() || once.is_clipped(),
                    "({o:?}, {other:?}) lost clipping"
                );
                // ...and never turns a scroll container back into a non-scroller.
                assert!(!is_scrollable(o) || is_scrollable(once));
            }
        }
    }

    #[test]
    fn resolve_computed_leaves_both_axes_consistently_scrollable() {
        // The whole point of the rule: after resolving *both* axes against each
        // other you can never end up with one scrollable axis and one that is
        // still visible/clip (which would be unrenderable).
        for x in ALL_OVERFLOW {
            for y in ALL_OVERFLOW {
                let rx = x.resolve_computed(y);
                let ry = y.resolve_computed(x);
                assert_eq!(
                    is_scrollable(rx),
                    is_scrollable(ry),
                    "({x:?}, {y:?}) resolved to the mismatched pair ({rx:?}, {ry:?})"
                );
            }
        }

        // Spot-check the documented pairs.
        assert_eq!(
            LayoutOverflow::Visible.resolve_computed(LayoutOverflow::Scroll),
            LayoutOverflow::Auto
        );
        assert_eq!(
            LayoutOverflow::Scroll.resolve_computed(LayoutOverflow::Visible),
            LayoutOverflow::Scroll
        );
        // visible + clip is a legal pair and must survive untouched.
        assert_eq!(
            LayoutOverflow::Visible.resolve_computed(LayoutOverflow::Clip),
            LayoutOverflow::Visible
        );
        assert_eq!(
            LayoutOverflow::Clip.resolve_computed(LayoutOverflow::Visible),
            LayoutOverflow::Clip
        );
    }

    // ---------------------------------------------------------------------
    // parse_layout_overflow
    // ---------------------------------------------------------------------

    #[test]
    fn layout_overflow_round_trips_through_print_as_css_value() {
        for o in ALL_OVERFLOW {
            let printed = o.print_as_css_value();
            assert_eq!(
                parse_layout_overflow(&printed).unwrap(),
                o,
                "{o:?} printed as {printed:?} did not round-trip"
            );
            // The printed form is a bare keyword: no whitespace, all lowercase.
            assert!(!printed.is_empty());
            assert!(!printed.contains(char::is_whitespace));
            assert_eq!(printed, printed.to_lowercase());
        }
    }

    #[test]
    fn parse_layout_overflow_treats_overlay_as_a_one_way_alias_of_auto() {
        // "overlay" is a legacy alias that parses to Auto but is never printed,
        // so the round-trip is stable only after the first normalisation.
        assert_eq!(parse_layout_overflow("overlay").unwrap(), LayoutOverflow::Auto);
        let normalised = parse_layout_overflow("overlay").unwrap().print_as_css_value();
        assert_eq!(normalised, "auto");
        assert_eq!(
            parse_layout_overflow(&normalised).unwrap(),
            LayoutOverflow::Auto
        );
        for o in ALL_OVERFLOW {
            assert_ne!(o.print_as_css_value(), "overlay");
        }
    }

    #[test]
    fn parse_layout_overflow_rejects_empty_and_whitespace_only_input() {
        for input in ["", " ", "   ", "\t", "\n", "\r\n", "\t \n \r", "\u{00A0}"] {
            assert!(
                parse_layout_overflow(input).is_err(),
                "{input:?} must not parse"
            );
        }
    }

    #[test]
    fn parse_layout_overflow_error_carries_the_untrimmed_input() {
        // The parser trims for matching but reports the *original* slice.
        let err = parse_layout_overflow("  bogus  ").unwrap_err();
        assert_eq!(err, LayoutOverflowParseError::InvalidValue("  bogus  "));
        let msg = format!("{err}");
        assert!(msg.contains("bogus"), "{msg}");
        assert!(msg.contains("scroll"), "error should list the valid keywords: {msg}");
    }

    #[test]
    fn parse_layout_overflow_is_ascii_case_sensitive() {
        // NOTE: CSS keywords are ASCII case-insensitive, but this parser only
        // accepts the lowercase spelling (property *names* are lowercased
        // upstream, values are not). Characterised here so a future fix has to
        // update the test deliberately.
        for input in ["SCROLL", "Scroll", "sCrOlL", "AUTO", "Hidden", "VISIBLE", "Clip"] {
            assert!(
                parse_layout_overflow(input).is_err(),
                "{input:?} unexpectedly parsed"
            );
        }
        assert_eq!(parse_layout_overflow("scroll").unwrap(), LayoutOverflow::Scroll);
    }

    #[test]
    fn parse_layout_overflow_trims_unicode_whitespace_but_not_zero_width_chars() {
        // `str::trim` uses the Unicode White_Space property, which is wider than
        // CSS whitespace: NBSP and the ideographic space are stripped too.
        assert_eq!(
            parse_layout_overflow("\u{00A0}scroll\u{00A0}").unwrap(),
            LayoutOverflow::Scroll
        );
        assert_eq!(
            parse_layout_overflow("\u{3000}auto").unwrap(),
            LayoutOverflow::Auto
        );
        // ...but a zero-width space is not whitespace, so it stays and rejects.
        assert!(parse_layout_overflow("\u{200B}scroll").is_err());
        assert!(parse_layout_overflow("scroll\u{FEFF}").is_err());
    }

    #[test]
    fn parse_layout_overflow_rejects_garbage_unicode_and_boundary_numbers() {
        for input in [
            "none",
            "hidden-x",
            "auto scroll",
            "scroll;",
            "scroll garbage",
            "visible !important",
            "\0",
            "scroll\0",
            "!@#$%^&*()",
            "\u{1F600}",
            "scroll\u{1F600}",
            "e\u{0301}",
            "ｓｃｒｏｌｌ",
            "скролл",
            "0",
            "-0",
            "0.0",
            "NaN",
            "nan",
            "inf",
            "-inf",
            "infinity",
            "9223372036854775807",
            "-9223372036854775808",
            "1e400",
            "1e-400",
        ] {
            assert!(
                parse_layout_overflow(input).is_err(),
                "{input:?} unexpectedly parsed"
            );
        }
    }

    #[test]
    fn parse_layout_overflow_survives_extremely_long_and_deeply_nested_input() {
        let long = "scroll".repeat(200_000);
        assert!(parse_layout_overflow(&long).is_err());

        let junk = "a".repeat(1_000_000);
        assert!(parse_layout_overflow(&junk).is_err());

        let nested = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_layout_overflow(&nested).is_err());

        // A valid keyword buried in megabytes of padding is still just padding.
        let padded = format!("{}scroll{}", " ".repeat(500_000), " ".repeat(500_000));
        assert_eq!(parse_layout_overflow(&padded).unwrap(), LayoutOverflow::Scroll);
    }

    // ---------------------------------------------------------------------
    // parse_style_scrollbar_gutter
    // ---------------------------------------------------------------------

    #[test]
    fn scrollbar_gutter_round_trips_through_print_as_css_value() {
        for g in ALL_GUTTER {
            let printed = g.print_as_css_value();
            assert_eq!(
                parse_style_scrollbar_gutter(&printed).unwrap(),
                g,
                "{g:?} printed as {printed:?} did not round-trip"
            );
        }
        assert_eq!(
            StyleScrollbarGutter::StableBothEdges.print_as_css_value(),
            "stable both-edges"
        );
        assert_eq!(StyleScrollbarGutter::default(), StyleScrollbarGutter::Auto);
    }

    #[test]
    fn parse_style_scrollbar_gutter_matches_the_keyword_string_verbatim() {
        // The parser compares the whole trimmed string, so it accepts exactly
        // one ASCII space between `stable` and `both-edges`. Per the grammar
        // (`auto | stable && both-edges?`) the reversed order and collapsed
        // runs of whitespace should also be legal — characterising the gap.
        assert_eq!(
            parse_style_scrollbar_gutter("stable both-edges").unwrap(),
            StyleScrollbarGutter::StableBothEdges
        );
        for rejected in [
            "stable  both-edges", // two spaces
            "stable\tboth-edges",
            "stable\nboth-edges",
            "both-edges stable", // `&&` allows either order
            "both-edges",
            "STABLE",
            "Stable Both-Edges",
            "stable both-edges stable",
        ] {
            assert!(
                parse_style_scrollbar_gutter(rejected).is_err(),
                "{rejected:?} unexpectedly parsed"
            );
        }
        // Outer whitespace *is* trimmed.
        assert_eq!(
            parse_style_scrollbar_gutter("  stable both-edges \n").unwrap(),
            StyleScrollbarGutter::StableBothEdges
        );
    }

    #[test]
    fn parse_style_scrollbar_gutter_rejects_empty_garbage_unicode_and_numbers() {
        for input in [
            "", " ", "\t\n", "none", "auto stable", "auto;", "stable;", "0", "-0", "NaN", "inf",
            "9223372036854775807", "\u{1F600}", "ｓｔａｂｌｅ", "stable\0",
        ] {
            assert!(
                parse_style_scrollbar_gutter(input).is_err(),
                "{input:?} unexpectedly parsed"
            );
        }
        let err = parse_style_scrollbar_gutter("  nope  ").unwrap_err();
        assert_eq!(
            err,
            StyleScrollbarGutterParseError::InvalidValue("  nope  ")
        );
        assert!(format!("{err}").contains("scrollbar-gutter"));
    }

    #[test]
    fn parse_style_scrollbar_gutter_survives_long_and_nested_input() {
        let long = "stable ".repeat(200_000);
        assert!(parse_style_scrollbar_gutter(&long).is_err());
        let nested = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_style_scrollbar_gutter(&nested).is_err());
    }

    // ---------------------------------------------------------------------
    // parse_style_overflow_clip_margin
    // ---------------------------------------------------------------------

    #[test]
    fn parse_style_overflow_clip_margin_accepts_either_component_in_either_order() {
        // <visual-box> only — length defaults to 0.
        let only_box = parse_style_overflow_clip_margin("content-box").unwrap();
        assert_eq!(only_box.clip_edge, VisualBox::ContentBox);
        assert_eq!(only_box.inner, PixelValue::default());

        // <length> only — box defaults to padding-box.
        let only_len = parse_style_overflow_clip_margin("20px").unwrap();
        assert_eq!(only_len.clip_edge, VisualBox::PaddingBox);
        assert_eq!(only_len.inner, PixelValue::const_px(20));

        // `||` means either order is valid.
        let a = parse_style_overflow_clip_margin("border-box 10px").unwrap();
        let b = parse_style_overflow_clip_margin("10px border-box").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.clip_edge, VisualBox::BorderBox);
        assert_eq!(a.inner, PixelValue::const_px(10));

        // Interior whitespace is collapsed by split_whitespace.
        let c = parse_style_overflow_clip_margin("  border-box \t\n  10px  ").unwrap();
        assert_eq!(c, a);

        assert_eq!(VisualBox::default(), VisualBox::PaddingBox);
    }

    #[test]
    fn parse_style_overflow_clip_margin_rejects_empty_duplicates_and_garbage() {
        for input in [
            "",
            "   ",
            "\t\n",
            "content-box content-box", // duplicate box
            "10px 20px",               // duplicate length
            "content-box 10px 20px",
            "content-box padding-box",
            "content-box 10px border-box",
            "none",
            "auto",
            "margin-box",
            "10px;",
            "10 px extra",
            "px",
            "\u{1F600}",
            "10\u{1F600}",
            "ｃｏｎｔｅｎｔ-ｂｏｘ",
            "content_box",
            "CONTENT-BOX",
        ] {
            assert!(
                parse_style_overflow_clip_margin(input).is_err(),
                "{input:?} unexpectedly parsed"
            );
        }
        let err = parse_style_overflow_clip_margin("  nope  ").unwrap_err();
        assert_eq!(
            err,
            StyleOverflowClipMarginParseError::InvalidValue("  nope  ")
        );
        assert!(format!("{err}").contains("overflow-clip-margin"));
    }

    #[test]
    fn parse_style_overflow_clip_margin_accepts_out_of_range_lengths() {
        // The declared syntax is `<visual-box> || <length [0,∞]>`: negatives and
        // percentages are invalid CSS. The parser delegates to parse_pixel_value
        // and clamps nothing, so both are accepted. Characterised, not endorsed.
        let neg = parse_style_overflow_clip_margin("-5px").unwrap();
        assert!(neg.inner.number.get() < 0.0);

        let pct = parse_style_overflow_clip_margin("50%").unwrap();
        assert_eq!(pct.inner.metric, SizeMetric::Percent);
        assert_eq!(pct.inner.number.get(), 50.0);

        // Unitless non-zero numbers are also let through (CSS requires a unit).
        let unitless = parse_style_overflow_clip_margin("7").unwrap();
        assert_eq!(unitless.inner.metric, SizeMetric::Px);
        assert_eq!(unitless.inner.number.get(), 7.0);
    }

    #[test]
    fn parse_style_overflow_clip_margin_saturates_nan_and_infinity() {
        // Rust's f32 parser accepts "NaN"/"inf", so these reach PixelValue.
        // FloatValue stores milli-units in an isize: NaN saturates to 0 and the
        // infinities to the isize bounds — no non-finite value can escape into
        // layout, which is the property that actually matters.
        let nan = parse_style_overflow_clip_margin("NaN").unwrap();
        assert!(!nan.inner.number.get().is_nan());
        assert_eq!(nan.inner.number.get(), 0.0);

        let pos_inf = parse_style_overflow_clip_margin("inf").unwrap();
        assert!(pos_inf.inner.number.get().is_finite());
        assert!(pos_inf.inner.number.get() > 0.0);

        let neg_inf = parse_style_overflow_clip_margin("-inf").unwrap();
        assert!(neg_inf.inner.number.get().is_finite());
        assert!(neg_inf.inner.number.get() < 0.0);

        // A number far beyond f32 range overflows to inf during parsing and
        // then saturates the same way.
        let huge = format!("{}px", "9".repeat(4096));
        let huge = parse_style_overflow_clip_margin(&huge).unwrap();
        assert!(huge.inner.number.get().is_finite());

        // Sub-milli precision is quantised away rather than rounded up.
        let tiny = parse_style_overflow_clip_margin("0.0001px").unwrap();
        assert_eq!(tiny.inner.number.get(), 0.0);
    }

    #[test]
    fn parse_style_overflow_clip_margin_survives_long_and_nested_input() {
        let long_token = format!("{}px", "a".repeat(1_000_000));
        assert!(parse_style_overflow_clip_margin(&long_token).is_err());

        let many_tokens = "content-box ".repeat(100_000);
        assert!(parse_style_overflow_clip_margin(&many_tokens).is_err());

        let nested = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_style_overflow_clip_margin(&nested).is_err());
    }

    #[test]
    fn overflow_clip_margin_round_trips_through_print_as_css_value() {
        let lengths = [
            PixelValue::const_px(12),
            PixelValue::px(1.5),
            PixelValue::const_em(2),
            PixelValue::const_percent(50),
            PixelValue::px(-3.25),
        ];
        for edge in ALL_VISUAL_BOX {
            for inner in lengths {
                let original = StyleOverflowClipMargin {
                    clip_edge: edge,
                    inner,
                };
                let printed = original.print_as_css_value();
                let reparsed = parse_style_overflow_clip_margin(&printed).unwrap_or_else(|e| {
                    panic!("{original:?} printed as {printed:?} but failed to reparse: {e}")
                });
                assert_eq!(reparsed, original, "round-trip broke via {printed:?}");
            }
        }
    }

    #[test]
    fn overflow_clip_margin_zero_length_prints_only_the_box_and_forgets_the_unit() {
        // A zero length is elided from the printed form, so its unit is lost on
        // the way back (0em == 0px semantically, so this is benign — but the
        // struct is *not* preserved bit-for-bit, which a naive round-trip
        // assertion would trip over).
        let zero_em = StyleOverflowClipMargin {
            clip_edge: VisualBox::ContentBox,
            inner: PixelValue::const_em(0),
        };
        assert_eq!(zero_em.print_as_css_value(), "content-box");
        let back = parse_style_overflow_clip_margin(&zero_em.print_as_css_value()).unwrap();
        assert_eq!(back.clip_edge, VisualBox::ContentBox);
        assert_eq!(back.inner.number.get(), 0.0);
        assert_eq!(back.inner.metric, SizeMetric::Px);
        assert_ne!(back, zero_em);

        // The all-default value prints as the bare default box.
        let default = StyleOverflowClipMargin::default();
        assert_eq!(default.print_as_css_value(), "padding-box");
        assert_eq!(
            parse_style_overflow_clip_margin(&default.print_as_css_value()).unwrap(),
            default
        );

        // padding-box + non-zero length prints only the length.
        let padding_len = StyleOverflowClipMargin {
            clip_edge: VisualBox::PaddingBox,
            inner: PixelValue::const_px(4),
        };
        assert_eq!(padding_len.print_as_css_value(), "4px");
    }

    #[test]
    fn visual_box_round_trips_through_the_clip_margin_parser() {
        for v in ALL_VISUAL_BOX {
            let printed = v.print_as_css_value();
            let parsed = parse_style_overflow_clip_margin(&printed).unwrap();
            assert_eq!(parsed.clip_edge, v, "{printed:?} did not round-trip");
        }
    }

    // ---------------------------------------------------------------------
    // parse_clip_edge (private)
    // ---------------------------------------------------------------------

    #[test]
    fn parse_clip_edge_auto_is_ascii_case_insensitive_and_trimmed() {
        for input in ["auto", "AUTO", "Auto", "aUtO", "  auto  ", "\tauto\n"] {
            assert_eq!(
                parse_clip_edge(input).unwrap(),
                OptionF32::None,
                "{input:?} should be auto"
            );
        }
        // ...but only the whole token: `auto` glued to anything else is invalid.
        assert!(parse_clip_edge("auto5").is_err());
        assert!(parse_clip_edge("autopx").is_err());
        assert!(parse_clip_edge("auto auto").is_err());
    }

    #[test]
    fn parse_clip_edge_silently_discards_the_unit() {
        // BUG (characterised): the edge keeps only `PixelValue::number`, so the
        // metric is thrown away — `rect(5em, ...)` is treated as 5 *pixels*, and
        // percentages (invalid for `clip`) are accepted as raw numbers.
        for input in ["5px", "5em", "5rem", "5pt", "5in", "5cm", "5mm", "5vw", "5vh", "5%"] {
            assert_eq!(
                parse_clip_edge(input).unwrap(),
                OptionF32::Some(5.0),
                "{input:?} did not collapse to a bare 5.0"
            );
        }
        // A unitless number is accepted as well (CSS requires a unit here).
        assert_eq!(parse_clip_edge("5").unwrap(), OptionF32::Some(5.0));
        // And whitespace between number and unit is tolerated by the pixel parser.
        assert_eq!(parse_clip_edge("5 px").unwrap(), OptionF32::Some(5.0));
    }

    #[test]
    fn parse_clip_edge_quantises_to_thousandths_and_normalises_negative_zero() {
        // FloatValue is a fixed-point isize in milli-units: anything below 1/1000
        // truncates toward zero rather than rounding.
        assert_eq!(parse_clip_edge("0.001px").unwrap(), OptionF32::Some(0.001));
        assert_eq!(parse_clip_edge("0.0001px").unwrap(), OptionF32::Some(0.0));
        assert_eq!(parse_clip_edge("-0.0009px").unwrap(), OptionF32::Some(0.0));
        assert_eq!(parse_clip_edge("1.9999px").unwrap(), OptionF32::Some(1.999));

        // -0 loses its sign, so it can never poison downstream sign checks.
        let minus_zero = parse_clip_edge("-0px").unwrap().into_option().unwrap();
        assert_eq!(minus_zero, 0.0);
        assert!(minus_zero.is_sign_positive());

        // Negative lengths are explicitly legal for `clip`.
        assert_eq!(parse_clip_edge("-10px").unwrap(), OptionF32::Some(-10.0));
    }

    #[test]
    fn parse_clip_edge_saturates_nan_and_infinity_to_finite_values() {
        let nan = parse_clip_edge("NaN").unwrap().into_option().unwrap();
        assert!(!nan.is_nan(), "NaN must not survive into a clip edge");
        assert_eq!(nan, 0.0);

        let pos_inf = parse_clip_edge("inf").unwrap().into_option().unwrap();
        assert!(pos_inf.is_finite());
        assert!(pos_inf > 0.0);

        let neg_inf = parse_clip_edge("-infinity").unwrap().into_option().unwrap();
        assert!(neg_inf.is_finite());
        assert!(neg_inf < 0.0);

        let huge = format!("{}px", "9".repeat(4096));
        let huge = parse_clip_edge(&huge).unwrap().into_option().unwrap();
        assert!(huge.is_finite());
    }

    #[test]
    fn parse_clip_edge_rejects_empty_bare_units_and_garbage() {
        for input in [
            "",
            "   ",
            "\t\n",
            "px",
            "em",
            "%",
            "abc",
            "10px;",
            "10px 20px",
            "(10px)",
            "\0",
            "\u{1F600}",
            "１px",
            "1px\u{0301}",
            "0x10",
        ] {
            assert!(parse_clip_edge(input).is_err(), "{input:?} unexpectedly parsed");
        }
        // The error carries the *trimmed token*, not the surrounding input.
        assert_eq!(
            parse_clip_edge("  abc  ").unwrap_err(),
            StyleClipRectParseError::InvalidValue("abc")
        );
    }

    // ---------------------------------------------------------------------
    // parse_clip_rect
    // ---------------------------------------------------------------------

    #[test]
    fn clip_rect_round_trips_through_print_as_css_value() {
        let rects = [
            StyleClipRect::default(),
            StyleClipRect {
                top: OptionF32::Some(0.0),
                right: OptionF32::Some(-2.25),
                bottom: OptionF32::Some(1.5),
                left: OptionF32::None,
            },
            StyleClipRect {
                top: OptionF32::Some(10.0),
                right: OptionF32::Some(20.0),
                bottom: OptionF32::Some(30.0),
                left: OptionF32::Some(40.0),
            },
            StyleClipRect {
                top: OptionF32::None,
                right: OptionF32::Some(-1.0),
                bottom: OptionF32::None,
                left: OptionF32::Some(-1.0),
            },
        ];
        for original in rects {
            let printed = original.print_as_css_value();
            let reparsed = parse_clip_rect(&printed).unwrap_or_else(|e| {
                panic!("{original:?} printed as {printed:?} but failed to reparse: {e}")
            });
            assert_eq!(reparsed, original, "round-trip broke via {printed:?}");
        }
        assert_eq!(
            StyleClipRect::default().print_as_css_value(),
            "rect(auto, auto, auto, auto)"
        );
    }

    #[test]
    fn parse_clip_rect_accepts_the_auto_comma_and_legacy_space_forms() {
        let all_auto = StyleClipRect::default();
        for input in [
            "auto",
            "AUTO",
            "  auto  ",
            "\u{00A0}auto", // NBSP is Unicode whitespace, so `trim` eats it
            "rect(auto, auto, auto, auto)",
            "rect(auto auto auto auto)",
            "RECT(auto, auto, auto, auto)",
            "  rect( auto , auto , auto , auto )  ",
        ] {
            assert_eq!(
                parse_clip_rect(input).unwrap(),
                all_auto,
                "{input:?} should be all-auto"
            );
        }

        let mixed = parse_clip_rect("rect(1px, auto, -3px, 4px)").unwrap();
        assert_eq!(mixed.top, OptionF32::Some(1.0));
        assert_eq!(mixed.right, OptionF32::None);
        assert_eq!(mixed.bottom, OptionF32::Some(-3.0));
        assert_eq!(mixed.left, OptionF32::Some(4.0));

        // No space after the commas is fine too.
        assert_eq!(
            parse_clip_rect("rect(1px,2px,3px,4px)").unwrap(),
            StyleClipRect {
                top: OptionF32::Some(1.0),
                right: OptionF32::Some(2.0),
                bottom: OptionF32::Some(3.0),
                left: OptionF32::Some(4.0),
            }
        );
    }

    #[test]
    fn parse_clip_rect_rejects_wrong_arity_mixed_separators_and_trailing_junk() {
        for input in [
            "rect()",
            "rect(,,,)",
            "rect(1px)",
            "rect(1px, 2px, 3px)",
            "rect(1px, 2px, 3px, 4px, 5px)",
            "rect(1px, 2px, 3px, 4px,)",
            "rect(1px 2px, 3px 4px)", // half comma-separated, half not
            "rect(1px 2px 3px)",
            "rect(1px 2px 3px 4px 5px)",
            "rect(1px, 2px, 3px, 4px",  // no closing paren
            "rect 1px, 2px, 3px, 4px)", // no opening paren
            "rect (1px, 2px, 3px, 4px)", // space before the paren
            "rect(1px, 2px, 3px, 4px) trailing",
            "rect(1px, 2px, 3px, 4px);",
            "junk rect(1px, 2px, 3px, 4px)",
            "rect(auto, auto, auto, abc)",
            "",
            "   ",
            "none",
            "inherit",
            "0",
        ] {
            assert!(parse_clip_rect(input).is_err(), "{input:?} unexpectedly parsed");
        }
    }

    #[test]
    fn parse_clip_rect_function_name_accepts_only_all_lower_or_all_upper_case() {
        // `rect(` and `RECT(` are special-cased; every mixed casing is rejected,
        // even though CSS function names are ASCII case-insensitive.
        assert!(parse_clip_rect("rect(auto, auto, auto, auto)").is_ok());
        assert!(parse_clip_rect("RECT(auto, auto, auto, auto)").is_ok());
        for input in [
            "Rect(auto, auto, auto, auto)",
            "rECT(auto, auto, auto, auto)",
            "ReCt(auto, auto, auto, auto)",
        ] {
            assert!(parse_clip_rect(input).is_err(), "{input:?} unexpectedly parsed");
        }
    }

    #[test]
    fn parse_clip_rect_errors_point_at_the_offending_token() {
        // A bad *edge* reports just the token...
        let err = parse_clip_rect("rect(1px, abc, 3px, 4px)").unwrap_err();
        assert_eq!(err, StyleClipRectParseError::InvalidValue("abc"));
        let msg = format!("{err}");
        assert!(msg.contains("abc"), "{msg}");
        // (the message's own "Expected rect(...)" hint aside, none of the *input*
        // apart from the bad token is echoed back)
        assert!(!msg.contains("1px"), "message leaked the whole input: {msg}");

        // ...while a structural error reports the untrimmed input.
        let err = parse_clip_rect("  rect(1px)  ").unwrap_err();
        assert_eq!(err, StyleClipRectParseError::InvalidValue("  rect(1px)  "));
    }

    #[test]
    fn parse_clip_rect_survives_deep_nesting_and_huge_input() {
        // Not a recursive-descent parser, so nesting cannot blow the stack.
        let nested = format!("{}{}", "rect(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_clip_rect(&nested).is_err());

        let parens = format!("{}{}", "(".repeat(100_000), ")".repeat(100_000));
        assert!(parse_clip_rect(&parens).is_err());

        // 50k edges: rejected on arity, not by hanging.
        let wide = format!("rect({})", "1px,".repeat(50_000));
        assert!(parse_clip_rect(&wide).is_err());

        let long_token = format!("rect({}, auto, auto, auto)", "a".repeat(1_000_000));
        assert!(parse_clip_rect(&long_token).is_err());

        // A legitimately huge magnitude parses and saturates instead of overflowing.
        let huge = format!("rect({}px, auto, auto, auto)", "9".repeat(4096));
        let huge = parse_clip_rect(&huge).unwrap();
        let top = huge.top.into_option().unwrap();
        assert!(top.is_finite());
        assert!(top > 0.0);
    }

    #[test]
    fn parse_clip_rect_does_not_panic_on_multibyte_input() {
        for input in [
            "rect(\u{1F600}, \u{1F600}, \u{1F600}, \u{1F600})",
            "rect(1px\u{0301}, auto, auto, auto)",
            "réct(1px, 2px, 3px, 4px)",
            "rect(１px, auto, auto, auto)", // fullwidth digit
            "rect(1px, auto, auto, auto\u{200B})",
            "\u{1F600}",
            "автo",
            "rect(٣px, auto, auto, auto)", // arabic-indic digit
        ] {
            assert!(parse_clip_rect(input).is_err(), "{input:?} unexpectedly parsed");
        }
    }

    // ---------------------------------------------------------------------
    // StyleClipRect::resolve
    // ---------------------------------------------------------------------

    #[test]
    fn clip_rect_default_is_all_auto() {
        let d = StyleClipRect::default();
        assert_eq!(d.top, OptionF32::None);
        assert_eq!(d.right, OptionF32::None);
        assert_eq!(d.bottom, OptionF32::None);
        assert_eq!(d.left, OptionF32::None);
    }

    #[test]
    fn clip_rect_resolve_expands_auto_edges_to_the_border_box() {
        // auto: top/left = 0, bottom/right = the border-box extent.
        let (top, right, bottom, left) = StyleClipRect::default().resolve(
            100.0, 50.0, // used width / height
            1.0, 2.0, 3.0, 4.0, // padding l / r / t / b
            5.0, 6.0, 7.0, 8.0, // border  l / r / t / b
        );
        assert_eq!(top, 0.0);
        assert_eq!(left, 0.0);
        assert_eq!(right, 100.0 + 1.0 + 2.0 + 5.0 + 6.0);
        assert_eq!(bottom, 50.0 + 3.0 + 4.0 + 7.0 + 8.0);
    }

    #[test]
    fn clip_rect_resolve_at_zero_and_with_negative_geometry() {
        let all_zero = StyleClipRect::default().resolve(
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        );
        assert_eq!(all_zero, (0.0, 0.0, 0.0, 0.0));

        // Negative geometry is summed as-is (no clamping): deterministic, finite.
        let (top, right, bottom, left) = StyleClipRect::default().resolve(
            -10.0, -20.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, -1.0,
        );
        assert_eq!(top, 0.0);
        assert_eq!(left, 0.0);
        assert_eq!(right, -14.0);
        assert_eq!(bottom, -24.0);
    }

    #[test]
    fn clip_rect_resolve_ignores_the_geometry_for_explicit_edges() {
        let explicit = StyleClipRect {
            top: OptionF32::Some(1.0),
            right: OptionF32::Some(2.0),
            bottom: OptionF32::Some(3.0),
            left: OptionF32::Some(4.0),
        };
        // Even with hostile geometry the explicit edges come back untouched.
        for geometry in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
        ] {
            let resolved = explicit.resolve(
                geometry, geometry, geometry, geometry, geometry, geometry, geometry, geometry,
                geometry, geometry,
            );
            assert_eq!(
                resolved,
                (1.0, 2.0, 3.0, 4.0),
                "explicit edges were perturbed by geometry {geometry:?}"
            );
        }
    }

    #[test]
    fn clip_rect_resolve_saturates_at_f32_max_and_keeps_nan_contained() {
        // f32::MAX + f32::MAX overflows to +inf rather than panicking.
        let (top, right, bottom, left) = StyleClipRect::default().resolve(
            f32::MAX,
            f32::MAX,
            f32::MAX,
            f32::MAX,
            f32::MAX,
            f32::MAX,
            f32::MAX,
            f32::MAX,
            f32::MAX,
            f32::MAX,
        );
        assert_eq!(top, 0.0);
        assert_eq!(left, 0.0);
        assert!(right.is_infinite() && right.is_sign_positive());
        assert!(bottom.is_infinite() && bottom.is_sign_positive());

        // NaN geometry propagates into the auto edges only (documented result:
        // NaN in, NaN out — no panic, and the fixed edges stay clean).
        let (top, right, bottom, left) = StyleClipRect::default().resolve(
            f32::NAN,
            f32::NAN,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        assert_eq!(top, 0.0);
        assert_eq!(left, 0.0);
        assert!(right.is_nan());
        assert!(bottom.is_nan());

        // +inf added to -inf is NaN — still no panic.
        let (_, right, bottom, _) = StyleClipRect::default().resolve(
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            0.0,
            f32::NEG_INFINITY,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        assert!(right.is_nan());
        assert!(bottom.is_nan());
    }

    // ---------------------------------------------------------------------
    // Error types: to_contained / to_shared
    // ---------------------------------------------------------------------

    /// Payloads that an error may have to carry: empty, whitespace, multibyte,
    /// combining marks, an embedded NUL, and a large string.
    fn error_payloads() -> Vec<String> {
        vec![
            String::new(),
            String::from(" "),
            String::from("bogus"),
            String::from("\u{1F600}\u{0301}"),
            String::from("a\0b"),
            String::from("rect(1px, 2px, 3px, 4px)"),
            "x".repeat(100_000),
        ]
    }

    macro_rules! assert_error_round_trips {
        ($borrowed:ident) => {{
            for payload in error_payloads() {
                let borrowed = $borrowed::InvalidValue(payload.as_str());
                let owned = borrowed.to_contained();
                let back = owned.to_shared();
                assert_eq!(
                    back, borrowed,
                    "{}::InvalidValue({payload:?}) lost data on to_contained/to_shared",
                    stringify!($borrowed)
                );
                // ...and the owned form is stable under a second lap.
                assert_eq!(owned.to_shared().to_contained(), owned);
            }
        }};
    }

    #[test]
    fn parse_errors_round_trip_between_borrowed_and_owned_forms() {
        assert_error_round_trips!(LayoutOverflowParseError);
        assert_error_round_trips!(StyleScrollbarGutterParseError);
        assert_error_round_trips!(StyleOverflowClipMarginParseError);
        assert_error_round_trips!(StyleClipRectParseError);
    }

    #[test]
    fn parse_errors_produced_by_the_parsers_round_trip_too() {
        let e = parse_layout_overflow("nope").unwrap_err();
        assert_eq!(e.to_contained().to_shared(), e);

        let e = parse_style_scrollbar_gutter("nope").unwrap_err();
        assert_eq!(e.to_contained().to_shared(), e);

        let e = parse_style_overflow_clip_margin("nope nope").unwrap_err();
        assert_eq!(e.to_contained().to_shared(), e);

        let e = parse_clip_rect("rect(nope)").unwrap_err();
        assert_eq!(e.to_contained().to_shared(), e);
    }

    #[test]
    fn parse_error_messages_name_the_property_and_quote_the_value() {
        let msg = format!("{}", LayoutOverflowParseError::InvalidValue("zzz"));
        assert!(msg.contains("overflow") && msg.contains("zzz"), "{msg}");

        let msg = format!("{}", StyleScrollbarGutterParseError::InvalidValue("zzz"));
        assert!(msg.contains("scrollbar-gutter") && msg.contains("zzz"), "{msg}");

        let msg = format!("{}", StyleOverflowClipMarginParseError::InvalidValue("zzz"));
        assert!(
            msg.contains("overflow-clip-margin") && msg.contains("zzz"),
            "{msg}"
        );

        let msg = format!("{}", StyleClipRectParseError::InvalidValue("zzz"));
        assert!(msg.contains("clip") && msg.contains("zzz"), "{msg}");

        // Debug is wired to Display: it must not panic on hostile payloads.
        let weird = StyleClipRectParseError::InvalidValue("\u{1F600}\0\u{0301}");
        assert!(!format!("{weird:?}").is_empty());
    }
}
