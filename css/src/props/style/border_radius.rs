//! CSS properties for border radius.

use alloc::string::{String, ToString};
use core::fmt;
use crate::corety::AzString;

use crate::{
    css::PrintAsCssValue,
    props::{
        basic::pixel::{
            parse_pixel_value, CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue,
        },
        macros::PixelValueTaker,
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

        impl ::core::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                write!(f, "{}", self.inner)
            }
        }

        impl PixelValueTaker for $struct_name {
            fn from_pixel_value(inner: PixelValue) -> Self {
                Self { inner }
            }
        }

        impl_pixel_value!($struct_name);
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

// Type alias for compatibility with old code
pub type CssStyleBorderRadiusParseError<'a> = CssBorderRadiusParseError<'a>;

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
#[repr(C, u8)]
pub enum CssBorderRadiusParseErrorOwned {
    TooManyValues(AzString),
    PixelValue(CssPixelValueParseErrorOwned),
}

// Type alias for compatibility with old code
pub type CssStyleBorderRadiusParseErrorOwned = CssBorderRadiusParseErrorOwned;

impl<'a> CssBorderRadiusParseError<'a> {
    pub fn to_contained(&self) -> CssBorderRadiusParseErrorOwned {
        match self {
            CssBorderRadiusParseError::TooManyValues(s) => {
                CssBorderRadiusParseErrorOwned::TooManyValues(s.to_string().into())
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

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_border_radius_shorthand() {
        // One value
        let result = parse_style_border_radius("10px").unwrap();
        assert_eq!(result.top_left, PixelValue::px(10.0));
        assert_eq!(result.top_right, PixelValue::px(10.0));
        assert_eq!(result.bottom_right, PixelValue::px(10.0));
        assert_eq!(result.bottom_left, PixelValue::px(10.0));

        // Two values
        let result = parse_style_border_radius("10px 5%").unwrap();
        assert_eq!(result.top_left, PixelValue::px(10.0));
        assert_eq!(result.top_right, PixelValue::percent(5.0));
        assert_eq!(result.bottom_right, PixelValue::px(10.0));
        assert_eq!(result.bottom_left, PixelValue::percent(5.0));

        // Three values
        let result = parse_style_border_radius("2px 4px 8px").unwrap();
        assert_eq!(result.top_left, PixelValue::px(2.0));
        assert_eq!(result.top_right, PixelValue::px(4.0));
        assert_eq!(result.bottom_right, PixelValue::px(8.0));
        assert_eq!(result.bottom_left, PixelValue::px(4.0));

        // Four values
        let result = parse_style_border_radius("1px 0 3px 4px").unwrap();
        assert_eq!(result.top_left, PixelValue::px(1.0));
        assert_eq!(result.top_right, PixelValue::px(0.0));
        assert_eq!(result.bottom_right, PixelValue::px(3.0));
        assert_eq!(result.bottom_left, PixelValue::px(4.0));

        // Weird whitespace
        let result = parse_style_border_radius("  1em   2em  ").unwrap();
        assert_eq!(result.top_left, PixelValue::em(1.0));
        assert_eq!(result.top_right, PixelValue::em(2.0));
    }

    #[test]
    fn test_parse_border_radius_shorthand_errors() {
        assert!(parse_style_border_radius("").is_err());
        assert!(parse_style_border_radius("1px 2px 3px 4px 5px").is_err());
        assert!(parse_style_border_radius("1px bad 3px").is_err());
    }

    #[test]
    fn test_parse_longhand_radius() {
        let result = parse_style_border_top_left_radius("25%").unwrap();
        assert_eq!(result.inner, PixelValue::percent(25.0));
    }
}
