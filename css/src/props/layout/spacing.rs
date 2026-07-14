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
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
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
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
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

#[cfg(all(test, feature = "parser"))]
mod autotest_generated {
    #![allow(clippy::float_cmp)] // fixed-point quantisation makes exact f32 compares meaningful here

    use std::collections::hash_map::DefaultHasher;

    #[allow(clippy::wildcard_imports)]
    use super::*;
    use alloc::format;
    use core::{
        fmt,
        hash::{Hash, Hasher},
    };

    use crate::props::{
        basic::{
            length::SizeMetric,
            pixel::{CssPixelValueParseError, PixelValue, PixelValueWithAuto},
        },
        formatter::FormatAsCssValue,
    };

    /// Renders a value through `FormatAsCssValue` so it can be compared against the
    /// `String`-returning `PrintAsCssValue` path.
    #[allow(missing_debug_implementations)]
    struct AsCss<'a, T: FormatAsCssValue>(&'a T);

    impl<T: FormatAsCssValue> fmt::Display for AsCss<'_, T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.format_as_css_value(f)
        }
    }

    fn hash_of<T: Hash>(value: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    fn exact_px(value: f32) -> PixelValueWithAuto {
        PixelValueWithAuto::Exact(PixelValue::px(value))
    }

    /// `parse(print(parse(s))) == parse(s)` — the encode/decode fixed point. Also
    /// checks the two formatting traits agree, since they are separate impls.
    macro_rules! assert_css_roundtrip {
        ($parse:ident, $input:expr) => {{
            let parsed = $parse($input).expect("positive control must parse");
            let printed = parsed.print_as_css_value();
            let reparsed = $parse(printed.as_str()).expect("printed value must re-parse");
            assert_eq!(
                parsed, reparsed,
                "{} did not survive {:?} -> {:?}",
                stringify!($parse),
                $input,
                printed
            );
            assert_eq!(
                printed,
                format!("{}", AsCss(&parsed)),
                "FormatAsCssValue and PrintAsCssValue disagree for {:?}",
                $input
            );
        }};
    }

    macro_rules! assert_all_longhands_err {
        ($input:expr) => {{
            assert!(parse_layout_padding_top($input).is_err(), "padding-top accepted {:?}", $input);
            assert!(parse_layout_padding_right($input).is_err(), "padding-right accepted {:?}", $input);
            assert!(parse_layout_padding_bottom($input).is_err(), "padding-bottom accepted {:?}", $input);
            assert!(parse_layout_padding_left($input).is_err(), "padding-left accepted {:?}", $input);
            assert!(parse_layout_padding_inline_start($input).is_err(), "padding-inline-start accepted {:?}", $input);
            assert!(parse_layout_padding_inline_end($input).is_err(), "padding-inline-end accepted {:?}", $input);
            assert!(parse_layout_margin_top($input).is_err(), "margin-top accepted {:?}", $input);
            assert!(parse_layout_margin_right($input).is_err(), "margin-right accepted {:?}", $input);
            assert!(parse_layout_margin_bottom($input).is_err(), "margin-bottom accepted {:?}", $input);
            assert!(parse_layout_margin_left($input).is_err(), "margin-left accepted {:?}", $input);
            assert!(parse_layout_column_gap($input).is_err(), "column-gap accepted {:?}", $input);
            assert!(parse_layout_row_gap($input).is_err(), "row-gap accepted {:?}", $input);
        }};
    }

    // --- parsers: positive controls -----------------------------------------

    #[test]
    fn minimal_valid_inputs_parse_to_the_documented_values() {
        assert_eq!(
            parse_layout_padding("0").unwrap(),
            LayoutPadding {
                top: exact_px(0.0),
                right: exact_px(0.0),
                bottom: exact_px(0.0),
                left: exact_px(0.0),
            }
        );
        assert_eq!(
            parse_layout_margin("1px").unwrap(),
            LayoutMargin {
                top: exact_px(1.0),
                right: exact_px(1.0),
                bottom: exact_px(1.0),
                left: exact_px(1.0),
            }
        );
    }

    #[test]
    fn shorthand_expansion_follows_the_css_1_to_4_value_rules() {
        let one = parse_layout_padding("7px").unwrap();
        assert_eq!(one.top, exact_px(7.0));
        assert_eq!(one.right, one.top);
        assert_eq!(one.bottom, one.top);
        assert_eq!(one.left, one.top);

        let two = parse_layout_padding("1px 2px").unwrap();
        assert_eq!((two.top, two.bottom), (exact_px(1.0), exact_px(1.0)));
        assert_eq!((two.right, two.left), (exact_px(2.0), exact_px(2.0)));

        // The third value is the *bottom*, and left mirrors right. Getting this
        // expansion backwards is the classic shorthand bug, so pin all four sides.
        let three = parse_layout_padding("1px 2px 3px").unwrap();
        assert_eq!(three.top, exact_px(1.0));
        assert_eq!(three.right, exact_px(2.0));
        assert_eq!(three.bottom, exact_px(3.0));
        assert_eq!(three.left, exact_px(2.0));

        // Four values run clockwise: top, right, bottom, left.
        let four = parse_layout_padding("1px 2px 3px 4px").unwrap();
        assert_eq!(four.top, exact_px(1.0));
        assert_eq!(four.right, exact_px(2.0));
        assert_eq!(four.bottom, exact_px(3.0));
        assert_eq!(four.left, exact_px(4.0));
    }

    #[test]
    fn margin_is_a_faithful_mirror_of_padding() {
        for input in [
            "0",
            "10px",
            "5% 2em",
            "1px 2px 3px",
            "1px 2px 3px 4px",
            "auto",
            "10px auto",
            "auto 0 inherit 2em",
        ] {
            let p = parse_layout_padding(input).unwrap();
            let m = parse_layout_margin(input).unwrap();
            assert_eq!(m.top, p.top, "top differs for {input:?}");
            assert_eq!(m.right, p.right, "right differs for {input:?}");
            assert_eq!(m.bottom, p.bottom, "bottom differs for {input:?}");
            assert_eq!(m.left, p.left, "left differs for {input:?}");
        }

        // ...and every error variant maps 1:1 through the delegation.
        assert!(matches!(
            parse_layout_margin(""),
            Err(LayoutMarginParseError::TooFewValues)
        ));
        assert!(matches!(
            parse_layout_margin("1 2 3 4 5"),
            Err(LayoutMarginParseError::TooManyValues)
        ));
        assert!(matches!(
            parse_layout_margin("nope"),
            Err(LayoutMarginParseError::PixelValueParseError(_))
        ));
    }

    // --- parsers: malformed / boundary / unicode ----------------------------

    #[test]
    fn empty_and_whitespace_only_input_is_too_few_values() {
        for input in ["", " ", "   ", "\t", "\n", "\r\n", "\x0b", "\x0c", " \t\r\n "] {
            assert!(
                matches!(
                    parse_layout_padding(input),
                    Err(LayoutPaddingParseError::TooFewValues)
                ),
                "padding {input:?}"
            );
            assert!(
                matches!(
                    parse_layout_margin(input),
                    Err(LayoutMarginParseError::TooFewValues)
                ),
                "margin {input:?}"
            );
        }
    }

    #[test]
    fn garbage_is_rejected_without_panicking() {
        for input in [
            "oops",
            "px",
            "%",
            "-",
            "+",
            ".",
            "e",
            "--",
            "10px;",
            "10px,20px",
            "10px, 20px",
            "10px!important",
            "calc(1px + 2px)",
            "1px/2px",
            "#10px",
            "0x10px",
            "1_000px",
            "auto auto auto auto auto",
        ] {
            assert!(
                parse_layout_padding(input).is_err(),
                "padding accepted {input:?}"
            );
            assert!(
                parse_layout_margin(input).is_err(),
                "margin accepted {input:?}"
            );
        }
    }

    #[test]
    fn shorthand_rejects_a_unit_split_from_its_number() {
        // `parse_pixel_value` trims *inside* a token, so "10 px" is a valid longhand.
        // The shorthand splits on whitespace first, so the same text is two values
        // and the bare unit is what fails -- a divergence worth pinning.
        let err = parse_layout_padding("10 px").unwrap_err();
        assert!(
            matches!(
                err,
                LayoutPaddingParseError::PixelValueParseError(
                    CssPixelValueParseError::NoValueGiven("px", SizeMetric::Px)
                )
            ),
            "expected NoValueGiven(\"px\", Px), got {err:?}"
        );
        assert_eq!(
            parse_layout_padding_top("10 px").unwrap(),
            LayoutPaddingTop::px(10.0)
        );
    }

    #[test]
    fn value_errors_are_reported_before_arity_errors() {
        // Every token is parsed before the count is checked, so a malformed value in
        // an over-long list surfaces as a value error, not TooManyValues.
        assert!(matches!(
            parse_layout_padding("1px 2px 3px 4px 5px"),
            Err(LayoutPaddingParseError::TooManyValues)
        ));
        assert!(matches!(
            parse_layout_padding("1px 2px 3px 4px bogus"),
            Err(LayoutPaddingParseError::PixelValueParseError(_))
        ));
        assert!(matches!(
            parse_layout_margin("1px 2px 3px 4px bogus"),
            Err(LayoutMarginParseError::PixelValueParseError(_))
        ));
    }

    #[test]
    fn boundary_numbers_saturate_instead_of_overflowing() {
        // Signed zero collapses onto one encoding.
        let zero = parse_layout_padding("0").unwrap();
        assert_eq!(parse_layout_padding("-0").unwrap(), zero);
        assert_eq!(zero.top, PixelValueWithAuto::Exact(PixelValue::zero()));

        // Overflowing literals become +/-inf in `f32::from_str` and must then saturate
        // to the isize bounds: nothing infinite may escape into layout arithmetic.
        for input in [
            "1e39px",
            "-1e39px",
            "inf",
            "-inf",
            "infinity",
            "9223372036854775807",
            "-9223372036854775808",
            "340282350000000000000000000000000000000px",
        ] {
            let parsed =
                parse_layout_padding(input).unwrap_or_else(|e| panic!("{input:?} failed: {e}"));
            let PixelValueWithAuto::Exact(value) = parsed.top else {
                panic!("{input:?} did not parse to an exact length");
            };
            let raw = value.number.get();
            assert!(raw.is_finite(), "{input:?} produced a non-finite length: {raw}");
        }

        // SPEC DIVERGENCE (pinned, not endorsed): CSS has no `NaN` value, but Rust's
        // `f32::from_str` accepts it, so `padding: NaN` parses. The saturating cast at
        // least keeps it safe -- it quantises to exactly 0px rather than poisoning
        // layout with a NaN.
        let nan = parse_layout_padding("NaN").unwrap();
        assert_eq!(nan.top, PixelValueWithAuto::Exact(PixelValue::zero()));
        assert_eq!(nan.right, nan.top);
        assert_eq!(nan.bottom, nan.top);
        assert_eq!(nan.left, nan.top);
    }

    #[test]
    fn sub_milli_unit_values_truncate_toward_zero() {
        // Lengths are stored as thousandths in an isize, and the cast truncates.
        assert_eq!(
            parse_layout_padding_top("0.0004px").unwrap(),
            LayoutPaddingTop::zero()
        );
        assert_eq!(
            parse_layout_padding_top("-0.0009px").unwrap(),
            LayoutPaddingTop::zero()
        );
        assert_eq!(
            parse_layout_padding_top("1e-40px").unwrap(),
            LayoutPaddingTop::zero()
        );
        // Truncation, not rounding: 1.9999px stays below 2px.
        assert_eq!(
            parse_layout_padding_top("1.9999px").unwrap().inner.number.get(),
            1.999
        );
    }

    #[test]
    fn unicode_junk_is_rejected_without_panicking() {
        for input in [
            "\u{1F600}",                        // emoji alone
            "10px\u{1F600}",                    // emoji glued to a valid value
            "\u{FF11}\u{FF10}\u{FF50}\u{FF58}", // fullwidth "10px"
            "10px\u{0301}",                     // combining acute on the unit
            "10\u{200b}px",                     // zero-width space inside the number
            "\u{661}\u{660}px",                 // arabic-indic digits
            "\u{202E}10px",                     // RTL-override prefix
        ] {
            assert!(
                parse_layout_padding(input).is_err(),
                "padding accepted {input:?}"
            );
            assert!(
                parse_layout_margin(input).is_err(),
                "margin accepted {input:?}"
            );
        }
    }

    #[test]
    fn unicode_whitespace_separates_values_even_though_css_only_splits_on_ascii() {
        // `split_whitespace()` follows the Unicode White_Space property, so U+00A0
        // (NO-BREAK SPACE) and U+2003 (EM SPACE) split a declaration into two values,
        // where CSS would treat the whole thing as one malformed token. Stated as an
        // invariant against `split_whitespace()` itself, so the test pins the parser's
        // contract instead of restating a Unicode table.
        for sep in [" ", "\u{a0}", "\u{2003}"] {
            let input = format!("10px{sep}20px");
            let tokens = input.split_whitespace().count();
            let parsed = parse_layout_padding(&input);
            assert_eq!(
                parsed.is_ok(),
                tokens == 2,
                "{input:?} split into {tokens} token(s)"
            );
            if let Ok(p) = parsed {
                assert_eq!(p.top, exact_px(10.0));
                assert_eq!(p.right, exact_px(20.0));
                assert_eq!(p.bottom, p.top);
                assert_eq!(p.left, p.right);
            }
        }
    }

    #[test]
    fn extremely_long_inputs_terminate_without_panicking() {
        // 200_000 well-formed values: an ordinary arity error, not a hang.
        let many = "1px ".repeat(200_000);
        assert!(matches!(
            parse_layout_padding(&many),
            Err(LayoutPaddingParseError::TooManyValues)
        ));
        assert!(matches!(
            parse_layout_margin(&many),
            Err(LayoutMarginParseError::TooManyValues)
        ));

        // A single 100_000-digit number overflows f32 to +inf, then saturates.
        let huge = format!("{}px", "9".repeat(100_000));
        let parsed = parse_layout_padding(&huge).unwrap();
        let PixelValueWithAuto::Exact(value) = parsed.top else {
            panic!("a huge number did not parse to an exact length");
        };
        assert!(value.number.get().is_finite());
        assert!(value.number.get() > 0.0);

        // A 1_000_000-char garbage token is rejected, not scanned forever.
        let junk = "z".repeat(1_000_000);
        assert!(parse_layout_padding(&junk).is_err());
        assert!(parse_layout_margin(&junk).is_err());
    }

    #[test]
    fn deeply_nested_brackets_do_not_stack_overflow() {
        // The grammar is flat, so nesting must be rejected by the float parser rather
        // than recursed into.
        let nested = format!("{}1px{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(
            matches!(
                parse_layout_padding(&nested),
                Err(LayoutPaddingParseError::PixelValueParseError(
                    CssPixelValueParseError::InvalidPixelValue(_)
                ))
            ),
            "deeply nested input was not rejected as an invalid pixel value"
        );

        let spread = format!("{n} {n} {n} {n}", n = "(".repeat(1_000));
        assert!(parse_layout_padding(&spread).is_err());
        assert!(parse_layout_margin(&spread).is_err());
    }

    #[test]
    fn shorthands_accept_css_wide_keywords_per_side() {
        // SPEC DIVERGENCE (pinned, not endorsed): the shorthands run every side through
        // `parse_pixel_value_with_auto`, so `padding: auto` parses (CSS has no such
        // value) and `initial`/`inherit` are accepted per side rather than only as a
        // whole declaration. Pinned so that tightening this is a visible change.
        assert_eq!(
            parse_layout_padding("auto").unwrap().top,
            PixelValueWithAuto::Auto
        );
        assert_eq!(
            parse_layout_padding("none").unwrap().top,
            PixelValueWithAuto::None
        );

        let mixed = parse_layout_padding("initial 10px inherit auto").unwrap();
        assert_eq!(mixed.top, PixelValueWithAuto::Initial);
        assert_eq!(mixed.right, exact_px(10.0));
        assert_eq!(mixed.bottom, PixelValueWithAuto::Inherit);
        assert_eq!(mixed.left, PixelValueWithAuto::Auto);
    }

    // --- errors -------------------------------------------------------------

    #[test]
    fn arity_error_messages_name_the_right_property() {
        // `parse_layout_margin` delegates to the padding parser and re-wraps the error;
        // a mis-mapped variant would be invisible to callers but wrong for users.
        let pad_many = format!("{}", LayoutPaddingParseError::TooManyValues);
        let pad_few = format!("{}", LayoutPaddingParseError::TooFewValues);
        assert!(
            pad_many.contains("padding") && pad_many.contains("at most 4"),
            "{pad_many}"
        );
        assert!(pad_few.contains("padding"), "{pad_few}");

        let margin_many = format!("{}", LayoutMarginParseError::TooManyValues);
        let margin_few = format!("{}", LayoutMarginParseError::TooFewValues);
        assert!(
            margin_many.contains("margin") && !margin_many.contains("padding"),
            "{margin_many}"
        );
        assert!(
            margin_few.contains("margin") && !margin_few.contains("padding"),
            "{margin_few}"
        );

        // The live parser surfaces those same messages.
        assert_eq!(
            format!("{}", parse_layout_margin("1 2 3 4 5").unwrap_err()),
            margin_many
        );
        assert_eq!(
            format!("{}", parse_layout_padding("").unwrap_err()),
            pad_few
        );
    }

    #[test]
    fn owned_and_shared_error_forms_round_trip() {
        assert_eq!(
            LayoutPaddingParseError::TooManyValues.to_contained(),
            LayoutPaddingParseErrorOwned::TooManyValues
        );
        assert_eq!(
            LayoutPaddingParseErrorOwned::TooFewValues.to_shared(),
            LayoutPaddingParseError::TooFewValues
        );
        assert_eq!(
            LayoutMarginParseError::TooFewValues.to_contained(),
            LayoutMarginParseErrorOwned::TooFewValues
        );
        assert_eq!(
            LayoutMarginParseErrorOwned::TooManyValues.to_shared(),
            LayoutMarginParseError::TooManyValues
        );

        // The borrowed payload must survive the owned round-trip as the same variant,
        // even though it holds a `&str` into the (now dropped) input.
        let owned = parse_layout_padding("1px oops").unwrap_err().to_contained();
        assert!(matches!(
            &owned,
            LayoutPaddingParseErrorOwned::PixelValueParseError(_)
        ));
        assert!(matches!(
            owned.to_shared(),
            LayoutPaddingParseError::PixelValueParseError(_)
        ));

        let owned_margin = parse_layout_margin("1px oops").unwrap_err().to_contained();
        assert!(matches!(
            owned_margin.to_shared(),
            LayoutMarginParseError::PixelValueParseError(_)
        ));
    }

    // --- longhands: parse + round-trip --------------------------------------

    #[test]
    fn longhand_parsers_reject_keywords_and_empty_input() {
        // The longhands go through `parse_pixel_value`, which -- unlike the shorthands
        // -- has no keyword table.
        for input in ["", "   ", "auto", "none", "initial", "inherit", "oops", "10px 20px"] {
            assert_all_longhands_err!(input);
        }
    }

    #[test]
    fn every_longhand_spacing_parser_accepts_a_minimal_value() {
        assert_eq!(parse_layout_padding_top("0").unwrap(), LayoutPaddingTop::px(0.0));
        assert_eq!(parse_layout_padding_right("1px").unwrap(), LayoutPaddingRight::px(1.0));
        assert_eq!(parse_layout_padding_bottom("2pt").unwrap(), LayoutPaddingBottom::pt(2.0));
        assert_eq!(parse_layout_padding_left("2em").unwrap(), LayoutPaddingLeft::em(2.0));
        assert_eq!(
            parse_layout_padding_inline_start("3px").unwrap(),
            LayoutPaddingInlineStart::px(3.0)
        );
        assert_eq!(
            parse_layout_padding_inline_end("4px").unwrap(),
            LayoutPaddingInlineEnd::px(4.0)
        );
        assert_eq!(parse_layout_margin_top("-5px").unwrap(), LayoutMarginTop::px(-5.0));
        assert_eq!(parse_layout_margin_right("6%").unwrap(), LayoutMarginRight::percent(6.0));
        assert_eq!(parse_layout_margin_bottom("7px").unwrap(), LayoutMarginBottom::px(7.0));
        assert_eq!(parse_layout_margin_left("8px").unwrap(), LayoutMarginLeft::px(8.0));
        assert_eq!(parse_layout_column_gap("20px").unwrap(), LayoutColumnGap::px(20.0));
        assert_eq!(parse_layout_row_gap("1.5em").unwrap(), LayoutRowGap::em(1.5));
    }

    #[test]
    fn every_longhand_spacing_parser_round_trips_through_its_printed_form() {
        // Only exactly-representable values here: the point is the encode/decode fixed
        // point, not the quantisation (covered by `sub_milli_unit_values_*`).
        for input in [
            "0", "1px", "10.5px", "1.5em", "2rem", "-20pt", "50%", "0.125px", "3.25in", "12.75mm",
            "2.54cm", "0.5vmin", "4vmax", "8vw", "100vh",
        ] {
            assert_css_roundtrip!(parse_layout_padding_top, input);
            assert_css_roundtrip!(parse_layout_padding_right, input);
            assert_css_roundtrip!(parse_layout_padding_bottom, input);
            assert_css_roundtrip!(parse_layout_padding_left, input);
            assert_css_roundtrip!(parse_layout_padding_inline_start, input);
            assert_css_roundtrip!(parse_layout_padding_inline_end, input);
            assert_css_roundtrip!(parse_layout_margin_top, input);
            assert_css_roundtrip!(parse_layout_margin_right, input);
            assert_css_roundtrip!(parse_layout_margin_bottom, input);
            assert_css_roundtrip!(parse_layout_margin_left, input);
            assert_css_roundtrip!(parse_layout_column_gap, input);
            assert_css_roundtrip!(parse_layout_row_gap, input);
        }
    }

    #[test]
    fn printed_css_matches_the_source_text_for_representable_values() {
        assert_eq!(
            parse_layout_padding_top("10px").unwrap().print_as_css_value(),
            "10px"
        );
        assert_eq!(
            parse_layout_column_gap("50%").unwrap().print_as_css_value(),
            "50%"
        );
        assert_eq!(
            parse_layout_margin_left("-2.5em").unwrap().print_as_css_value(),
            "-2.5em"
        );
        assert_eq!(LayoutRowGap::zero().print_as_css_value(), "0px");
        // A unitless number is a px length, and prints back *with* the unit.
        assert_eq!(
            parse_layout_padding_bottom("3").unwrap().print_as_css_value(),
            "3px"
        );
    }

    // --- constructors, ordering, hashing, interpolation ----------------------

    #[test]
    fn const_and_runtime_constructors_agree() {
        assert_eq!(LayoutPaddingLeft::const_px(5), LayoutPaddingLeft::px(5.0));
        assert_eq!(LayoutPaddingLeft::const_em(2), LayoutPaddingLeft::em(2.0));
        assert_eq!(LayoutPaddingLeft::const_pt(-3), LayoutPaddingLeft::pt(-3.0));
        assert_eq!(
            LayoutPaddingLeft::const_percent(50),
            LayoutPaddingLeft::percent(50.0)
        );
        assert_eq!(
            LayoutColumnGap::const_from_metric(SizeMetric::Vh, 7),
            LayoutColumnGap::from_metric(SizeMetric::Vh, 7.0)
        );
        assert_eq!(
            LayoutColumnGap::const_in(1),
            LayoutColumnGap::from_metric(SizeMetric::In, 1.0)
        );
        assert_eq!(
            LayoutColumnGap::const_cm(2),
            LayoutColumnGap::from_metric(SizeMetric::Cm, 2.0)
        );
        assert_eq!(
            LayoutColumnGap::const_mm(3),
            LayoutColumnGap::from_metric(SizeMetric::Mm, 3.0)
        );

        // `PixelValueTaker` is what the shorthand macros build these types through.
        assert_eq!(
            LayoutRowGap::from_pixel_value(PixelValue::em(2.0)).inner,
            PixelValue::em(2.0)
        );

        assert_eq!(LayoutPaddingBottom::zero(), LayoutPaddingBottom::default());
        assert_eq!(
            LayoutPaddingBottom::default().inner.metric,
            SizeMetric::Px,
            "the default spacing metric is px"
        );
    }

    #[test]
    fn ordering_is_metric_major_not_physical_length() {
        // `Ord` is derived over (metric, number), so 1000px sorts *below* 0pt. Anything
        // ranking spacing by real size has to resolve to px first -- this is a trap, and
        // the test exists to state it.
        assert!(LayoutPaddingTop::px(1000.0) < LayoutPaddingTop::pt(0.0));
        assert!(LayoutPaddingTop::pt(0.0) < LayoutPaddingTop::em(0.0));

        // Within one metric the ordering is numeric, as expected.
        assert!(LayoutPaddingTop::px(1.0) < LayoutPaddingTop::px(2.0));
        assert!(LayoutMarginLeft::const_px(-5) < LayoutMarginLeft::zero());
    }

    #[test]
    fn equal_values_hash_equal_across_signed_zero_and_quantisation() {
        let pos = LayoutMarginTop::px(0.0);
        let neg = LayoutMarginTop::px(-0.0);
        assert_eq!(pos, neg, "signed zero must have one canonical encoding");
        assert_eq!(hash_of(&pos), hash_of(&neg));
        assert_eq!(pos, LayoutMarginTop::zero());

        // Two values that quantise to the same thousandth are Eq, so they must hash
        // alike -- otherwise they would behave inconsistently as HashMap keys.
        let a = LayoutMarginTop::px(1.0001);
        let b = LayoutMarginTop::px(1.0009);
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));

        // Same number, different metric: not equal.
        assert_ne!(LayoutMarginTop::px(1.0), LayoutMarginTop::em(1.0));
    }

    #[test]
    fn debug_renders_the_value_as_css() {
        assert_eq!(format!("{:?}", LayoutPaddingTop::px(10.0)), "10px");
        assert_eq!(format!("{:?}", LayoutColumnGap::percent(50.0)), "50%");
        assert_eq!(format!("{:?}", LayoutRowGap::zero()), "0px");
    }

    #[test]
    fn interpolate_hits_its_endpoints_and_survives_nan_and_huge_t() {
        let a = LayoutRowGap::px(0.0);
        let b = LayoutRowGap::px(10.0);
        assert_eq!(a.interpolate(&b, 0.0), a);
        assert_eq!(a.interpolate(&b, 1.0), b);
        assert_eq!(a.interpolate(&b, 0.5), LayoutRowGap::px(5.0));

        // A NaN `t` (a degenerate zero-length animation, say) must not leak a NaN length
        // into layout: the saturating cast maps it to 0.
        let nan = a.interpolate(&b, f32::NAN);
        assert_eq!(nan.inner.metric, SizeMetric::Px);
        assert_eq!(nan.inner.number.get(), 0.0);

        // Out-of-range `t` saturates rather than wrapping the fixed-point encoding.
        for t in [1e30_f32, -1e30_f32, f32::INFINITY, f32::NEG_INFINITY] {
            let out = a.interpolate(&b, t);
            assert!(
                out.inner.number.get().is_finite(),
                "t = {t} produced a non-finite length"
            );
        }

        // Mismatched metrics fall back to px instead of silently keeping the left metric.
        let mixed = LayoutRowGap::px(0.0).interpolate(&LayoutRowGap::em(1.0), 1.0);
        assert_eq!(mixed.inner.metric, SizeMetric::Px);
        assert!(mixed.inner.number.get().is_finite());
    }
}
