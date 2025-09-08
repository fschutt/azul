//! Border radius CSS properties

use crate::props::basic::value::PixelValue;
use crate::props::formatter::FormatAsCssValue;
use alloc::string::String;
use core::fmt;

// Macro for creating debug/display implementations for wrapper types
macro_rules! impl_pixel_value {
    ($struct:ident) => {
        impl fmt::Debug for $struct {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{:?}", self.inner)
            }
        }

        impl fmt::Display for $struct {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{}", self.inner)
            }
        }

        impl $struct {
            pub fn scale_for_dpi(&mut self, scale_factor: f32) {
                self.inner.scale_for_dpi(scale_factor);
            }
        }

        impl FormatAsCssValue for $struct {
            fn format_as_css_value(&self) -> String {
                self.inner.format_as_css_value()
            }
        }
    };
}

/// CSS border-top-left-radius property
#[derive(Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleBorderTopLeftRadius {
    pub inner: PixelValue,
}

/// CSS border-bottom-left-radius property
#[derive(Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleBorderBottomLeftRadius {
    pub inner: PixelValue,
}

/// CSS border-top-right-radius property
#[derive(Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleBorderTopRightRadius {
    pub inner: PixelValue,
}

/// CSS border-bottom-right-radius property
#[derive(Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleBorderBottomRightRadius {
    pub inner: PixelValue,
}

impl_pixel_value!(StyleBorderTopLeftRadius);
impl_pixel_value!(StyleBorderBottomLeftRadius);
impl_pixel_value!(StyleBorderTopRightRadius);
impl_pixel_value!(StyleBorderBottomRightRadius);

/// Aggregated border radius for all corners
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct StyleBorderRadius {
    pub top_left: StyleBorderTopLeftRadius,
    pub top_right: StyleBorderTopRightRadius,
    pub bottom_left: StyleBorderBottomLeftRadius,
    pub bottom_right: StyleBorderBottomRightRadius,
}

impl Default for StyleBorderRadius {
    fn default() -> Self {
        Self {
            top_left: StyleBorderTopLeftRadius::default(),
            top_right: StyleBorderTopRightRadius::default(),
            bottom_left: StyleBorderBottomLeftRadius::default(),
            bottom_right: StyleBorderBottomRightRadius::default(),
        }
    }
}

impl StyleBorderRadius {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.top_left.scale_for_dpi(scale_factor);
        self.top_right.scale_for_dpi(scale_factor);
        self.bottom_left.scale_for_dpi(scale_factor);
        self.bottom_right.scale_for_dpi(scale_factor);
    }
}

impl FormatAsCssValue for StyleBorderRadius {
    fn format_as_css_value(&self) -> String {
        // CSS border-radius shorthand: top-left top-right bottom-right bottom-left
        format!(
            "{} {} {} {}",
            self.top_left.format_as_css_value(),
            self.top_right.format_as_css_value(),
            self.bottom_right.format_as_css_value(),
            self.bottom_left.format_as_css_value()
        )
    }
}

// TODO: Add parsing functions
// fn parse_style_border_radius<'a>(input: &'a str) -> Result<StyleBorderRadius, CssStyleBorderRadiusParseError<'a>>
// fn parse_style_border_top_left_radius<'a>(input: &'a str) -> Result<StyleBorderTopLeftRadius, CssPixelValueParseError<'a>>
// etc.
