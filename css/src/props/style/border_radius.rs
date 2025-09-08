//! Border radius CSS properties

use alloc::string::String;
use core::fmt;

use crate::props::{basic::value::PixelValue, formatter::FormatAsCssValue};
#[cfg(feature = "parser")]
use crate::{error::CssPixelValueParseError, props::basic::value::parse_pixel_value};

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

impl StyleBorderRadius {
    pub fn uniform(radius: PixelValue) -> Self {
        Self {
            top_left: StyleBorderTopLeftRadius { inner: radius },
            top_right: StyleBorderTopRightRadius { inner: radius },
            bottom_left: StyleBorderBottomLeftRadius { inner: radius },
            bottom_right: StyleBorderBottomRightRadius { inner: radius },
        }
    }
}

#[cfg(feature = "parser")]
pub mod parsing {
    use alloc::string::String;

    use super::*;

    #[derive(Clone, PartialEq)]
    pub enum CssStyleBorderRadiusParseError<'a> {
        TooManyValues(&'a str),
        CssPixelValueParseError(CssPixelValueParseError<'a>),
    }

    impl<'a> core::fmt::Display for CssStyleBorderRadiusParseError<'a> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            match self {
                CssStyleBorderRadiusParseError::TooManyValues(val) => {
                    write!(f, "Too many values: \"{}\"", val)
                }
                CssStyleBorderRadiusParseError::CssPixelValueParseError(e) => write!(f, "{}", e),
            }
        }
    }

    impl<'a> core::fmt::Debug for CssStyleBorderRadiusParseError<'a> {
        fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
            write!(f, "{}", self)
        }
    }

    impl<'a> From<CssPixelValueParseError<'a>> for CssStyleBorderRadiusParseError<'a> {
        fn from(e: CssPixelValueParseError<'a>) -> Self {
            CssStyleBorderRadiusParseError::CssPixelValueParseError(e)
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum CssStyleBorderRadiusParseErrorOwned {
        TooManyValues(String),
        CssPixelValueParseError(crate::error::CssPixelValueParseErrorOwned),
    }

    impl<'a> CssStyleBorderRadiusParseError<'a> {
        pub fn to_contained(&self) -> CssStyleBorderRadiusParseErrorOwned {
            match self {
                CssStyleBorderRadiusParseError::TooManyValues(s) => {
                    CssStyleBorderRadiusParseErrorOwned::TooManyValues(s.to_string())
                }
                CssStyleBorderRadiusParseError::CssPixelValueParseError(e) => {
                    CssStyleBorderRadiusParseErrorOwned::CssPixelValueParseError(e.to_contained())
                }
            }
        }
    }

    impl CssStyleBorderRadiusParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> CssStyleBorderRadiusParseError<'a> {
            match self {
                CssStyleBorderRadiusParseErrorOwned::TooManyValues(s) => {
                    CssStyleBorderRadiusParseError::TooManyValues(s)
                }
                CssStyleBorderRadiusParseErrorOwned::CssPixelValueParseError(e) => {
                    CssStyleBorderRadiusParseError::CssPixelValueParseError(e.to_shared())
                }
            }
        }
    }

    /// Parse the border-radius like "5px 10px" or "5px 10px 6px 10px"
    #[cfg(feature = "parser")]
    pub fn parse_style_border_radius<'a>(
        input: &'a str,
    ) -> Result<StyleBorderRadius, CssStyleBorderRadiusParseError<'a>> {
        let mut components = input.split_whitespace();
        let len = components.clone().count();

        match len {
            1 => {
                // One value - border-radius: 15px;
                // (the value applies to all four corners, which are rounded equally:
                let uniform_radius = parse_pixel_value(components.next().unwrap())?;
                Ok(StyleBorderRadius::uniform(uniform_radius))
            }
            2 => {
                // Two values - border-radius: 15px 50px;
                // (first value applies to top-left and bottom-right corners,
                // and the second value applies to top-right and bottom-left corners):
                let top_left_bottom_right = parse_pixel_value(components.next().unwrap())?;
                let top_right_bottom_left = parse_pixel_value(components.next().unwrap())?;

                Ok(StyleBorderRadius {
                    top_left: StyleBorderTopLeftRadius {
                        inner: top_left_bottom_right,
                    },
                    bottom_right: StyleBorderBottomRightRadius {
                        inner: top_left_bottom_right,
                    },
                    top_right: StyleBorderTopRightRadius {
                        inner: top_right_bottom_left,
                    },
                    bottom_left: StyleBorderBottomLeftRadius {
                        inner: top_right_bottom_left,
                    },
                })
            }
            3 => {
                // Three values - border-radius: 15px 50px 30px;
                // (first value applies to top-left corner,
                // second value applies to top-right and bottom-left corners,
                // and third value applies to bottom-right corner):
                let top_left = parse_pixel_value(components.next().unwrap())?;
                let top_right_bottom_left = parse_pixel_value(components.next().unwrap())?;
                let bottom_right = parse_pixel_value(components.next().unwrap())?;

                Ok(StyleBorderRadius {
                    top_left: StyleBorderTopLeftRadius { inner: top_left },
                    bottom_right: StyleBorderBottomRightRadius {
                        inner: bottom_right,
                    },
                    top_right: StyleBorderTopRightRadius {
                        inner: top_right_bottom_left,
                    },
                    bottom_left: StyleBorderBottomLeftRadius {
                        inner: top_right_bottom_left,
                    },
                })
            }
            4 => {
                // Four values - border-radius: 15px 50px 30px 5px;
                // first value applies to top-left corner,
                // second value applies to top-right corner,
                // third value applies to bottom-right corner,
                // fourth value applies to bottom-left corner
                let top_left = parse_pixel_value(components.next().unwrap())?;
                let top_right = parse_pixel_value(components.next().unwrap())?;
                let bottom_right = parse_pixel_value(components.next().unwrap())?;
                let bottom_left = parse_pixel_value(components.next().unwrap())?;

                Ok(StyleBorderRadius {
                    top_left: StyleBorderTopLeftRadius { inner: top_left },
                    top_right: StyleBorderTopRightRadius { inner: top_right },
                    bottom_right: StyleBorderBottomRightRadius {
                        inner: bottom_right,
                    },
                    bottom_left: StyleBorderBottomLeftRadius { inner: bottom_left },
                })
            }
            _ => Err(CssStyleBorderRadiusParseError::TooManyValues(input)),
        }
    }

    typed_pixel_value_parser!(parse_style_border_top_left_radius, StyleBorderTopLeftRadius);
    typed_pixel_value_parser!(
        parse_style_border_top_right_radius,
        StyleBorderTopRightRadius
    );
    typed_pixel_value_parser!(
        parse_style_border_bottom_left_radius,
        StyleBorderBottomLeftRadius
    );
    typed_pixel_value_parser!(
        parse_style_border_bottom_right_radius,
        StyleBorderBottomRightRadius
    );
}

#[cfg(feature = "parser")]
pub use parsing::*;
