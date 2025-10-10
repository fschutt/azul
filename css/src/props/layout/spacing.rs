//! CSS properties for `margin` and `padding`.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

#[cfg(feature = "parser")]
use crate::parser::{
    impl_debug_as_display, impl_display, impl_from, parse_pixel_value_with_auto,
    typed_pixel_value_parser, PixelValueWithAuto,
};
use crate::props::{
    basic::value::{CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue},
    formatter::PrintAsCssValue,
    macros::{impl_pixel_value, PixelValueTaker},
};

// --- TYPE DEFINITIONS ---

macro_rules! define_spacing_property {
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

define_spacing_property!(LayoutPaddingTop);
define_spacing_property!(LayoutPaddingRight);
define_spacing_property!(LayoutPaddingBottom);
define_spacing_property!(LayoutPaddingLeft);

define_spacing_property!(LayoutMarginTop);
define_spacing_property!(LayoutMarginRight);
define_spacing_property!(LayoutMarginBottom);
define_spacing_property!(LayoutMarginLeft);

// --- PARSERS ---

// -- Padding Shorthand Parser --

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutPaddingParseError<'a> {
    PixelValueParseError(CssPixelValueParseError<'a>),
    TooManyValues,
    TooFewValues,
}

#[cfg(feature = "parser")]
impl_debug_as_display!(LayoutPaddingParseError<'a>);

#[cfg(feature = "parser")]
impl_display! { LayoutPaddingParseError<'a>, {
    PixelValueParseError(e) => format!("Could not parse pixel value: {}", e),
    TooManyValues => "Too many values: padding property accepts at most 4 values.",
    TooFewValues => "Too few values: padding property requires at least 1 value.",
}}

#[cfg(feature = "parser")]
impl_from!(
    CssPixelValueParseError<'a>,
    LayoutPaddingParseError::PixelValueParseError
);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutPaddingParseErrorOwned {
    PixelValueParseError(CssPixelValueParseErrorOwned),
    TooManyValues,
    TooFewValues,
}

#[cfg(feature = "parser")]
impl<'a> LayoutPaddingParseError<'a> {
    pub fn to_contained(&self) -> LayoutPaddingParseErrorOwned {
        match self {
            LayoutPaddingParseError::PixelValueParseError(e) => {
                LayoutPaddingParseErrorOwned::PixelValueParseError(e.to_contained())
            }
            LayoutPaddingParseError::TooManyValues => LayoutPaddingParseErrorOwned::TooManyValues,
            LayoutPaddingParseError::TooFewValues => LayoutPaddingParseErrorOwned::TooFewValues,
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutPaddingParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutPaddingParseError<'a> {
        match self {
            LayoutPaddingParseErrorOwned::PixelValueParseError(e) => {
                LayoutPaddingParseError::PixelValueParseError(e.to_shared())
            }
            LayoutPaddingParseErrorOwned::TooManyValues => LayoutPaddingParseError::TooManyValues,
            LayoutPaddingParseErrorOwned::TooFewValues => LayoutPaddingParseError::TooFewValues,
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutPadding {
    pub top: PixelValueWithAuto,
    pub bottom: PixelValueWithAuto,
    pub left: PixelValueWithAuto,
    pub right: PixelValueWithAuto,
}

#[cfg(feature = "parser")]
pub fn parse_layout_padding<'a>(
    input: &'a str,
) -> Result<LayoutPadding, LayoutPaddingParseError<'a>> {
    let values: Vec<_> = input.split_whitespace().collect();

    let parsed_values: Vec<PixelValueWithAuto> = values
        .iter()
        .map(|s| parse_pixel_value_with_auto(s))
        .collect::<Result<_, _>>()?;

    match parsed_values.len() {
        1 => {
            // top, right, bottom, left
            let all = parsed_values[0];
            Ok(LayoutPadding {
                top: all,
                right: all,
                bottom: all,
                left: all,
            })
        }
        2 => {
            // top/bottom, left/right
            let vertical = parsed_values[0];
            let horizontal = parsed_values[1];
            Ok(LayoutPadding {
                top: vertical,
                right: horizontal,
                bottom: vertical,
                left: horizontal,
            })
        }
        3 => {
            // top, left/right, bottom
            let top = parsed_values[0];
            let horizontal = parsed_values[1];
            let bottom = parsed_values[2];
            Ok(LayoutPadding {
                top,
                right: horizontal,
                bottom,
                left: horizontal,
            })
        }
        4 => {
            // top, right, bottom, left
            Ok(LayoutPadding {
                top: parsed_values[0],
                right: parsed_values[1],
                bottom: parsed_values[2],
                left: parsed_values[3],
            })
        }
        0 => Err(LayoutPaddingParseError::TooFewValues),
        _ => Err(LayoutPaddingParseError::TooManyValues),
    }
}

// -- Margin Shorthand Parser --

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutMarginParseError<'a> {
    PixelValueParseError(CssPixelValueParseError<'a>),
    TooManyValues,
    TooFewValues,
}

#[cfg(feature = "parser")]
impl_debug_as_display!(LayoutMarginParseError<'a>);

#[cfg(feature = "parser")]
impl_display! { LayoutMarginParseError<'a>, {
    PixelValueParseError(e) => format!("Could not parse pixel value: {}", e),
    TooManyValues => "Too many values: margin property accepts at most 4 values.",
    TooFewValues => "Too few values: margin property requires at least 1 value.",
}}

#[cfg(feature = "parser")]
impl_from!(
    CssPixelValueParseError<'a>,
    LayoutMarginParseError::PixelValueParseError
);

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutMarginParseErrorOwned {
    PixelValueParseError(CssPixelValueParseErrorOwned),
    TooManyValues,
    TooFewValues,
}

#[cfg(feature = "parser")]
impl<'a> LayoutMarginParseError<'a> {
    pub fn to_contained(&self) -> LayoutMarginParseErrorOwned {
        match self {
            LayoutMarginParseError::PixelValueParseError(e) => {
                LayoutMarginParseErrorOwned::PixelValueParseError(e.to_contained())
            }
            LayoutMarginParseError::TooManyValues => LayoutMarginParseErrorOwned::TooManyValues,
            LayoutMarginParseError::TooFewValues => LayoutMarginParseErrorOwned::TooFewValues,
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutMarginParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutMarginParseError<'a> {
        match self {
            LayoutMarginParseErrorOwned::PixelValueParseError(e) => {
                LayoutMarginParseError::PixelValueParseError(e.to_shared())
            }
            LayoutMarginParseErrorOwned::TooManyValues => LayoutMarginParseError::TooManyValues,
            LayoutMarginParseErrorOwned::TooFewValues => LayoutMarginParseError::TooFewValues,
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutMargin {
    pub top: PixelValueWithAuto,
    pub bottom: PixelValueWithAuto,
    pub left: PixelValueWithAuto,
    pub right: PixelValueWithAuto,
}

#[cfg(feature = "parser")]
pub fn parse_layout_margin<'a>(input: &'a str) -> Result<LayoutMargin, LayoutMarginParseError<'a>> {
    // Margin parsing logic is identical to padding, so we can reuse the padding parser
    // and just map the Ok and Err variants to the margin-specific types.
    match parse_layout_padding(input) {
        Ok(padding) => Ok(LayoutMargin {
            top: padding.top,
            left: padding.left,
            right: padding.right,
            bottom: padding.bottom,
        }),
        Err(e) => match e {
            LayoutPaddingParseError::PixelValueParseError(err) => {
                Err(LayoutMarginParseError::PixelValueParseError(err))
            }
            LayoutPaddingParseError::TooManyValues => Err(LayoutMarginParseError::TooManyValues),
            LayoutPaddingParseError::TooFewValues => Err(LayoutMarginParseError::TooFewValues),
        },
    }
}

// -- Longhand Property Parsers --

#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_padding_top, LayoutPaddingTop);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_padding_right, LayoutPaddingRight);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_padding_bottom, LayoutPaddingBottom);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_padding_left, LayoutPaddingLeft);

#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_margin_top, LayoutMarginTop);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_margin_right, LayoutMarginRight);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_margin_bottom, LayoutMarginBottom);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_margin_left, LayoutMarginLeft);
