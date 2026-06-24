//! CSS properties for `margin`, `padding`, and `gap` (column-gap / row-gap).
//!
//! Shorthand parsers (`parse_layout_padding`, `parse_layout_margin`) and
//! longhand per-side parsers are gated behind `#[cfg(feature = "parser")]`.

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

// Spacing properties - wrapper structs around PixelValue for type safety

macro_rules! impl_spacing_type_impls {
    ($name:ident) => {
        impl ::core::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                write!(f, "{}", self.inner)
            }
        }

        impl PixelValueTaker for $name {
            fn from_pixel_value(inner: PixelValue) -> Self {
                Self { inner }
            }
        }

        impl_pixel_value!($name);
    };
}

/// Layout padding top value
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutPaddingTop {
    pub inner: PixelValue,
}
impl_spacing_type_impls!(LayoutPaddingTop);

/// Layout padding right value
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutPaddingRight {
    pub inner: PixelValue,
}
impl_spacing_type_impls!(LayoutPaddingRight);

/// Layout padding bottom value
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutPaddingBottom {
    pub inner: PixelValue,
}
impl_spacing_type_impls!(LayoutPaddingBottom);

/// Layout padding left value
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutPaddingLeft {
    pub inner: PixelValue,
}
impl_spacing_type_impls!(LayoutPaddingLeft);

/// Layout padding inline start value (for RTL/LTR support)
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutPaddingInlineStart {
    pub inner: PixelValue,
}
impl_spacing_type_impls!(LayoutPaddingInlineStart);

/// Layout padding inline end value (for RTL/LTR support)
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutPaddingInlineEnd {
    pub inner: PixelValue,
}
impl_spacing_type_impls!(LayoutPaddingInlineEnd);

/// Layout margin top value
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMarginTop {
    pub inner: PixelValue,
}
impl_spacing_type_impls!(LayoutMarginTop);

/// Layout margin right value
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMarginRight {
    pub inner: PixelValue,
}
impl_spacing_type_impls!(LayoutMarginRight);

/// Layout margin bottom value
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMarginBottom {
    pub inner: PixelValue,
}
impl_spacing_type_impls!(LayoutMarginBottom);

/// Layout margin left value
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMarginLeft {
    pub inner: PixelValue,
}
impl_spacing_type_impls!(LayoutMarginLeft);

/// Layout column gap value (for flexbox/grid)
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutColumnGap {
    pub inner: PixelValue,
}
impl_spacing_type_impls!(LayoutColumnGap);

/// Layout row gap value (for flexbox/grid)
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutRowGap {
    pub inner: PixelValue,
}
impl_spacing_type_impls!(LayoutRowGap);

// --- PARSERS ---

#[cfg(feature = "parser")]
macro_rules! impl_spacing_parse_error {
    ($borrowed:ident, $owned:ident, $property_name:expr) => {
        #[cfg(feature = "parser")]
        impl_debug_as_display!($borrowed<'a>);

        #[cfg(feature = "parser")]
        impl_display! { $borrowed<'a>, {
            PixelValueParseError(e) => format!("Could not parse pixel value: {}", e),
            TooManyValues => concat!("Too many values: ", $property_name, " property accepts at most 4 values."),
            TooFewValues => concat!("Too few values: ", $property_name, " property requires at least 1 value."),
        }}

        #[cfg(feature = "parser")]
        impl_from!(
            CssPixelValueParseError<'a>,
            $borrowed::PixelValueParseError
        );

        #[cfg(feature = "parser")]
        impl $borrowed<'_> {
            #[must_use] pub fn to_contained(&self) -> $owned {
                match self {
                    $borrowed::PixelValueParseError(e) => {
                        $owned::PixelValueParseError(e.to_contained())
                    }
                    $borrowed::TooManyValues => $owned::TooManyValues,
                    $borrowed::TooFewValues => $owned::TooFewValues,
                }
            }
        }

        #[cfg(feature = "parser")]
        impl $owned {
            #[must_use] pub fn to_shared(&self) -> $borrowed<'_> {
                match self {
                    $owned::PixelValueParseError(e) => {
                        $borrowed::PixelValueParseError(e.to_shared())
                    }
                    $owned::TooManyValues => $borrowed::TooManyValues,
                    $owned::TooFewValues => $borrowed::TooFewValues,
                }
            }
        }
    };
}

// -- Padding Shorthand Parser --

/// Error from parsing a CSS `padding` shorthand value.
#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum LayoutPaddingParseError<'a> {
    PixelValueParseError(CssPixelValueParseError<'a>),
    TooManyValues,
    TooFewValues,
}

/// Owned variant of [`LayoutPaddingParseError`].
#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutPaddingParseErrorOwned {
    PixelValueParseError(CssPixelValueParseErrorOwned),
    TooManyValues,
    TooFewValues,
}

#[cfg(feature = "parser")]
impl_spacing_parse_error!(LayoutPaddingParseError, LayoutPaddingParseErrorOwned, "padding");

/// Result of parsing the CSS `padding` shorthand property (1–4 values).
#[cfg(feature = "parser")]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutPadding {
    pub top: PixelValueWithAuto,
    pub bottom: PixelValueWithAuto,
    pub left: PixelValueWithAuto,
    pub right: PixelValueWithAuto,
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `padding` value.
pub fn parse_layout_padding(
    input: &str,
) -> Result<LayoutPadding, LayoutPaddingParseError<'_>> {
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

/// Error from parsing a CSS `margin` shorthand value.
#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum LayoutMarginParseError<'a> {
    PixelValueParseError(CssPixelValueParseError<'a>),
    TooManyValues,
    TooFewValues,
}

/// Owned variant of [`LayoutMarginParseError`].
#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutMarginParseErrorOwned {
    PixelValueParseError(CssPixelValueParseErrorOwned),
    TooManyValues,
    TooFewValues,
}

#[cfg(feature = "parser")]
impl_spacing_parse_error!(LayoutMarginParseError, LayoutMarginParseErrorOwned, "margin");

/// Result of parsing the CSS `margin` shorthand property (1–4 values).
#[cfg(feature = "parser")]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutMargin {
    pub top: PixelValueWithAuto,
    pub bottom: PixelValueWithAuto,
    pub left: PixelValueWithAuto,
    pub right: PixelValueWithAuto,
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `margin` value.
pub fn parse_layout_margin(input: &str) -> Result<LayoutMargin, LayoutMarginParseError<'_>> {
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
        /// # Errors
        ///
        /// Returns an error if `input` is not a valid CSS value for this property.
        pub fn $fn(input: &str) -> Result<$return, CssPixelValueParseError<'_>> {
            crate::props::basic::parse_pixel_value(input).map(|e| $return { inner: e })
        }

        impl crate::props::formatter::FormatAsCssValue for $return {
            fn format_as_css_value(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
