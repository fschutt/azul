//! CSS properties for border radius.

use alloc::string::{String, ToString};
use core::fmt;
use crate::{
    parser::{impl_debug_as_display, impl_display, impl_from},
    props::{
        basic::value::{
            parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue,
        },
        formatter::PrintAsCssValue,
        macros::{impl_pixel_value, PixelValueTaker},
    },
};

// --- Property Struct Definitions ---

macro_rules! define_border_radius_property {
    ($struct_name:ident) => {
        #[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(C)]
        pub struct $struct_name {
            pub inner: PixelValue,
        }

        impl PixelValueTaker for $struct_name {
            fn from_pixel_value(inner: PixelValue) -> Self {
                Self { inner }
            }
        }

        impl_pixel_value!($struct_name);

        impl PrintAsCssValue for $struct_name {
            fn print_as_css_value(&self) -> String {
                format!("{}", self.inner)
            }
        }
    };
}

define_border_radius_property!(StyleBorderTopLeftRadius);
define_border_radius_property!(StyleBorderTopRightRadius);
define_border_radius_property!(StyleBorderBottomLeftRadius);
define_border_radius_property!(StyleBorderBottomRightRadius);

// --- Parser-only Struct ---

/// A temporary struct used only during the parsing of the `border-radius` shorthand property.
#[cfg(feature = "parser")]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderRadius {
    pub top_left: PixelValue,
    pub top_right: PixelValue,
    pub bottom_left: PixelValue,
    pub bottom_right: PixelValue,
}

// --- Error Types ---

/// Error for the shorthand `border-radius` property.
#[derive(Clone, PartialEq)]
pub enum CssBorderRadiusParseError<'a> {
    /// Too many values were provided (max is 4).
    TooManyValues(&'a str),
    /// An underlying pixel value could not be parsed.
    PixelValue(CssPixelValueParseError<'a>),
}

impl_debug_as_display!(CssBorderRadiusParseError<'a>);
impl_display! { CssBorderRadiusParseError<'a>, {
    TooManyValues(val) => format!("Too many values for border-radius: \"{}\"", val),
    PixelValue(e) => format!("{}", e),
}}
impl_from!(
    CssPixelValueParseError<'a>,
    CssBorderRadiusParseError::PixelValue
);

/// Owned version of `CssBorderRadiusParseError`.
#[derive(Debug, Clone, PartialEq)]
pub enum CssBorderRadiusParseErrorOwned {
    TooManyValues(String),
    PixelValue(CssPixelValueParseErrorOwned),
}

impl<'a> CssBorderRadiusParseError<'a> {
    pub fn to_contained(&self) -> CssBorderRadiusParseErrorOwned {
        match self {
            CssBorderRadiusParseError::TooManyValues(s) => {
                CssBorderRadiusParseErrorOwned::TooManyValues(s.to_string())
            }
            CssBorderRadiusParseError::PixelValue(e) => {
                CssBorderRadiusParseErrorOwned::PixelValue(e.to_contained())
            }
        }
    }
}

impl CssBorderRadiusParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssBorderRadiusParseError<'a> {
        match self {
            CssBorderRadiusParseErrorOwned::TooManyValues(s) => {
                CssBorderRadiusParseError::TooManyValues(s)
            }
            CssBorderRadiusParseErrorOwned::PixelValue(e) => {
                CssBorderRadiusParseError::PixelValue(e.to_shared())
            }
        }
    }
}

/// Macro to generate error types for individual radius properties.
macro_rules! define_border_radius_parse_error {
    ($error_name:ident, $error_name_owned:ident) => {
        #[derive(Clone, PartialEq)]
        pub enum $error_name<'a> {
            PixelValue(CssPixelValueParseError<'a>),
        }

        impl_debug_as_display!($error_name<'a>);
        impl_display! { $error_name<'a>, {
            PixelValue(e) => format!("{}", e),
        }}

        impl_from!(CssPixelValueParseError<'a>, $error_name::PixelValue);

        #[derive(Debug, Clone, PartialEq)]
        pub enum $error_name_owned {
            PixelValue(CssPixelValueParseErrorOwned),
        }

        impl<'a> $error_name<'a> {
            pub fn to_contained(&self) -> $error_name_owned {
                match self {
                    $error_name::PixelValue(e) => $error_name_owned::PixelValue(e.to_contained()),
                }
            }
        }

        impl $error_name_owned {
            pub fn to_shared<'a>(&'a self) -> $error_name<'a> {
                match self {
                    $error_name_owned::PixelValue(e) => $error_name::PixelValue(e.to_shared()),
                }
            }
        }
    };
}

define_border_radius_parse_error!(
    StyleBorderTopLeftRadiusParseError,
    StyleBorderTopLeftRadiusParseErrorOwned
);
define_border_radius_parse_error!(
    StyleBorderTopRightRadiusParseError,
    StyleBorderTopRightRadiusParseErrorOwned
);
define_border_radius_parse_error!(
    StyleBorderBottomLeftRadiusParseError,
    StyleBorderBottomLeftRadiusParseErrorOwned
);
define_border_radius_parse_error!(
    StyleBorderBottomRightRadiusParseError,
    StyleBorderBottomRightRadiusParseErrorOwned
);

// --- Parsing Functions ---

#[cfg(feature = "parser")]
pub fn parse_style_border_radius<'a>(
    input: &'a str,
) -> Result<StyleBorderRadius, CssBorderRadiusParseError<'a>> {
    let components: Vec<_> = input.split_whitespace().collect();
    let mut values = Vec::with_capacity(components.len());
    for comp in components.iter() {
        values.push(parse_pixel_value(comp)?);
    }

    match values.len() {
        1 => Ok(StyleBorderRadius {
            top_left: values[0],
            top_right: values[0],
            bottom_right: values[0],
            bottom_left: values[0],
        }),
        2 => Ok(StyleBorderRadius {
            top_left: values[0],
            top_right: values[1],
            bottom_right: values[0],
            bottom_left: values[1],
        }),
        3 => Ok(StyleBorderRadius {
            top_left: values[0],
            top_right: values[1],
            bottom_right: values[2],
            bottom_left: values[1],
        }),
        4 => Ok(StyleBorderRadius {
            top_left: values[0],
            top_right: values[1],
            bottom_right: values[2],
            bottom_left: values[3],
        }),
        _ => Err(CssBorderRadiusParseError::TooManyValues(input)),
    }
}

#[cfg(feature = "parser")]
pub fn parse_style_border_top_left_radius<'a>(
    input: &'a str,
) -> Result<StyleBorderTopLeftRadius, StyleBorderTopLeftRadiusParseError<'a>> {
    let pixel_value = parse_pixel_value(input)?;
    Ok(StyleBorderTopLeftRadius { inner: pixel_value })
}

#[cfg(feature = "parser")]
pub fn parse_style_border_top_right_radius<'a>(
    input: &'a str,
) -> Result<StyleBorderTopRightRadius, StyleBorderTopRightRadiusParseError<'a>> {
    let pixel_value = parse_pixel_value(input)?;
    Ok(StyleBorderTopRightRadius { inner: pixel_value })
}

#[cfg(feature = "parser")]
pub fn parse_style_border_bottom_left_radius<'a>(
    input: &'a str,
) -> Result<StyleBorderBottomLeftRadius, StyleBorderBottomLeftRadiusParseError<'a>> {
    let pixel_value = parse_pixel_value(input)?;
    Ok(StyleBorderBottomLeftRadius { inner: pixel_value })
}

#[cfg(feature = "parser")]
pub fn parse_style_border_bottom_right_radius<'a>(
    input: &'a str,
) -> Result<StyleBorderBottomRightRadius, StyleBorderBottomRightRadiusParseError<'a>> {
    let pixel_value = parse_pixel_value(input)?;
    Ok(StyleBorderBottomRightRadius { inner: pixel_value })
}
