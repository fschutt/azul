//! CSS properties for `margin` and `padding`.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

#[cfg(feature = "parser")]
use crate::props::basic::pixel::{parse_pixel_value_with_auto, PixelValueWithAuto};
use crate::{
    css::PrintAsCssValue,
    props::{
        basic::pixel::{CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue},
        macros::PixelValueTaker,
    },
};

// --- TYPE DEFINITIONS ---

macro_rules! define_spacing_property {
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

define_spacing_property!(LayoutPaddingTop);
define_spacing_property!(LayoutPaddingRight);
define_spacing_property!(LayoutPaddingBottom);
define_spacing_property!(LayoutPaddingLeft);
define_spacing_property!(LayoutPaddingInlineStart);
define_spacing_property!(LayoutPaddingInlineEnd);

define_spacing_property!(LayoutMarginTop);
define_spacing_property!(LayoutMarginRight);
define_spacing_property!(LayoutMarginBottom);
define_spacing_property!(LayoutMarginLeft);

define_spacing_property!(LayoutColumnGap);
define_spacing_property!(LayoutRowGap);

// --- PARSERS ---

// -- Padding Shorthand Parser --

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
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
#[derive(Clone, PartialEq)]
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

macro_rules! typed_pixel_value_parser {
    (
        $fn:ident, $fn_str:expr, $return:ident, $return_str:expr, $import_str:expr, $test_str:expr
    ) => {
        ///Parses a `
        #[doc = $return_str]
        ///` attribute from a `&str`
        ///
        ///# Example
        ///
        ///```rust
        #[doc = $import_str]
        #[doc = $test_str]
        ///```
        pub fn $fn<'a>(input: &'a str) -> Result<$return, CssPixelValueParseError<'a>> {
            crate::props::basic::parse_pixel_value(input).and_then(|e| Ok($return { inner: e }))
        }

        impl crate::props::formatter::FormatAsCssValue for $return {
            fn format_as_css_value(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                self.inner.format_as_css_value(f)
            }
        }
    };
    ($fn:ident, $return:ident) => {
        typed_pixel_value_parser!(
            $fn,
            stringify!($fn),
            $return,
            stringify!($return),
            concat!(
                "# extern crate azul_css;",
                "\r\n",
                "# use azul_css::props::layout::spacing::",
                stringify!($fn),
                ";",
                "\r\n",
                "# use azul_css::props::basic::pixel::PixelValue;\r\n",
                "# use azul_css::props::layout::spacing::",
                stringify!($return),
                ";\r\n"
            ),
            concat!(
                "assert_eq!(",
                stringify!($fn),
                "(\"5px\"), Ok(",
                stringify!($return),
                " { inner: PixelValue::px(5.0) }));"
            )
        );
    };
}

#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_padding_top, LayoutPaddingTop);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_padding_right, LayoutPaddingRight);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_padding_bottom, LayoutPaddingBottom);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_padding_left, LayoutPaddingLeft);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_padding_inline_start, LayoutPaddingInlineStart);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_padding_inline_end, LayoutPaddingInlineEnd);

#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_margin_top, LayoutMarginTop);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_margin_right, LayoutMarginRight);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_margin_bottom, LayoutMarginBottom);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_margin_left, LayoutMarginLeft);

#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_column_gap, LayoutColumnGap);
#[cfg(feature = "parser")]
typed_pixel_value_parser!(parse_layout_row_gap, LayoutRowGap);

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;
    use crate::props::basic::pixel::{PixelValue, PixelValueWithAuto};

    #[test]
    fn test_parse_layout_padding_shorthand() {
        // 1 value
        let result = parse_layout_padding("10px").unwrap();
        assert_eq!(result.top, PixelValueWithAuto::Exact(PixelValue::px(10.0)));
        assert_eq!(
            result.right,
            PixelValueWithAuto::Exact(PixelValue::px(10.0))
        );
        assert_eq!(
            result.bottom,
            PixelValueWithAuto::Exact(PixelValue::px(10.0))
        );
        assert_eq!(result.left, PixelValueWithAuto::Exact(PixelValue::px(10.0)));

        // 2 values
        let result = parse_layout_padding("5% 2em").unwrap();
        assert_eq!(
            result.top,
            PixelValueWithAuto::Exact(PixelValue::percent(5.0))
        );
        assert_eq!(result.right, PixelValueWithAuto::Exact(PixelValue::em(2.0)));
        assert_eq!(
            result.bottom,
            PixelValueWithAuto::Exact(PixelValue::percent(5.0))
        );
        assert_eq!(result.left, PixelValueWithAuto::Exact(PixelValue::em(2.0)));

        // 3 values
        let result = parse_layout_padding("1px 2px 3px").unwrap();
        assert_eq!(result.top, PixelValueWithAuto::Exact(PixelValue::px(1.0)));
        assert_eq!(result.right, PixelValueWithAuto::Exact(PixelValue::px(2.0)));
        assert_eq!(
            result.bottom,
            PixelValueWithAuto::Exact(PixelValue::px(3.0))
        );
        assert_eq!(result.left, PixelValueWithAuto::Exact(PixelValue::px(2.0)));

        // 4 values
        let result = parse_layout_padding("1px 2px 3px 4px").unwrap();
        assert_eq!(result.top, PixelValueWithAuto::Exact(PixelValue::px(1.0)));
        assert_eq!(result.right, PixelValueWithAuto::Exact(PixelValue::px(2.0)));
        assert_eq!(
            result.bottom,
            PixelValueWithAuto::Exact(PixelValue::px(3.0))
        );
        assert_eq!(result.left, PixelValueWithAuto::Exact(PixelValue::px(4.0)));

        // Whitespace
        let result = parse_layout_padding("  1px   2px  ").unwrap();
        assert_eq!(result.top, PixelValueWithAuto::Exact(PixelValue::px(1.0)));
        assert_eq!(result.right, PixelValueWithAuto::Exact(PixelValue::px(2.0)));
    }

    #[test]
    fn test_parse_layout_padding_errors() {
        assert!(matches!(
            parse_layout_padding("").err().unwrap(),
            LayoutPaddingParseError::TooFewValues
        ));
        assert!(matches!(
            parse_layout_padding("1px 2px 3px 4px 5px").err().unwrap(),
            LayoutPaddingParseError::TooManyValues
        ));
        assert!(matches!(
            parse_layout_padding("1px oops 3px").err().unwrap(),
            LayoutPaddingParseError::PixelValueParseError(_)
        ));
    }

    #[test]
    fn test_parse_layout_margin_shorthand() {
        // 1 value with auto
        let result = parse_layout_margin("auto").unwrap();
        assert_eq!(result.top, PixelValueWithAuto::Auto);
        assert_eq!(result.right, PixelValueWithAuto::Auto);
        assert_eq!(result.bottom, PixelValueWithAuto::Auto);
        assert_eq!(result.left, PixelValueWithAuto::Auto);

        // 2 values
        let result = parse_layout_margin("10px auto").unwrap();
        assert_eq!(result.top, PixelValueWithAuto::Exact(PixelValue::px(10.0)));
        assert_eq!(result.right, PixelValueWithAuto::Auto);
        assert_eq!(
            result.bottom,
            PixelValueWithAuto::Exact(PixelValue::px(10.0))
        );
        assert_eq!(result.left, PixelValueWithAuto::Auto);
    }

    #[test]
    fn test_parse_layout_margin_errors() {
        assert!(matches!(
            parse_layout_margin("").err().unwrap(),
            LayoutMarginParseError::TooFewValues
        ));
        assert!(matches!(
            parse_layout_margin("1px 2px 3px 4px 5px").err().unwrap(),
            LayoutMarginParseError::TooManyValues
        ));
        assert!(matches!(
            parse_layout_margin("1px invalid").err().unwrap(),
            LayoutMarginParseError::PixelValueParseError(_)
        ));
    }

    #[test]
    fn test_parse_longhand_spacing() {
        assert_eq!(
            parse_layout_padding_left("2em").unwrap(),
            LayoutPaddingLeft {
                inner: PixelValue::em(2.0)
            }
        );
        assert!(parse_layout_margin_top("auto").is_err()); // Longhands don't parse "auto"
        assert_eq!(
            parse_layout_column_gap("20px").unwrap(),
            LayoutColumnGap {
                inner: PixelValue::px(20.0)
            }
        );
    }
}
