//! CSS properties related to dimensions and sizing.

use alloc::string::{String, ToString};

use crate::props::{
    basic::pixel::{CssPixelValueParseError, CssPixelValueParseErrorOwned, PixelValue},
    formatter::PrintAsCssValue,
    macros::PixelValueTaker,
};

// -- Type Definitions --

macro_rules! define_dimension_property {
    ($struct_name:ident, $default_fn:expr) => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(C)]
        pub struct $struct_name {
            pub inner: PixelValue,
        }

        impl Default for $struct_name {
            fn default() -> Self {
                $default_fn()
            }
        }

        impl PixelValueTaker for $struct_name {
            fn from_pixel_value(inner: PixelValue) -> Self {
                Self { inner }
            }
        }

        impl_pixel_value!($struct_name);

        impl PrintAsCssValue for $struct_name {
            fn print_as_css_value(&self) -> String {
                self.inner.to_string()
            }
        }
    };
}

define_dimension_property!(LayoutWidth, || Self {
    inner: PixelValue::zero()
});
define_dimension_property!(LayoutHeight, || Self {
    inner: PixelValue::zero()
});
define_dimension_property!(LayoutMinWidth, || Self {
    inner: PixelValue::zero()
});
define_dimension_property!(LayoutMinHeight, || Self {
    inner: PixelValue::zero()
});
define_dimension_property!(LayoutMaxWidth, || Self {
    inner: PixelValue::px(core::f32::MAX)
});
define_dimension_property!(LayoutMaxHeight, || Self {
    inner: PixelValue::px(core::f32::MAX)
});

/// Represents a `box-sizing` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutBoxSizing {
    ContentBox,
    BorderBox,
}

impl Default for LayoutBoxSizing {
    fn default() -> Self {
        LayoutBoxSizing::ContentBox
    }
}

impl PrintAsCssValue for LayoutBoxSizing {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutBoxSizing::ContentBox => "content-box",
            LayoutBoxSizing::BorderBox => "border-box",
        })
    }
}

// -- Parser --

#[cfg(feature = "parser")]
mod parser {

    use alloc::string::ToString;

    use super::*;
    use crate::props::basic::pixel::parse_pixel_value;

    macro_rules! define_pixel_dimension_parser {
        ($fn_name:ident, $struct_name:ident, $error_name:ident, $error_owned_name:ident) => {
            #[derive(Clone, PartialEq)]
            pub enum $error_name<'a> {
                PixelValue(CssPixelValueParseError<'a>),
            }

            impl_debug_as_display!($error_name<'a>);
            impl_display! { $error_name<'a>, {
                PixelValue(e) => format!("{}", e),
            }}

            impl_from! { CssPixelValueParseError<'a>, $error_name::PixelValue }

            #[derive(Debug, Clone, PartialEq)]
            pub enum $error_owned_name {
                PixelValue(CssPixelValueParseErrorOwned),
            }

            impl<'a> $error_name<'a> {
                pub fn to_contained(&self) -> $error_owned_name {
                    match self {
                        $error_name::PixelValue(e) => {
                            $error_owned_name::PixelValue(e.to_contained())
                        }
                    }
                }
            }

            impl $error_owned_name {
                pub fn to_shared<'a>(&'a self) -> $error_name<'a> {
                    match self {
                        $error_owned_name::PixelValue(e) => $error_name::PixelValue(e.to_shared()),
                    }
                }
            }

            pub fn $fn_name<'a>(input: &'a str) -> Result<$struct_name, $error_name<'a>> {
                parse_pixel_value(input)
                    .map(|v| $struct_name { inner: v })
                    .map_err($error_name::PixelValue)
            }
        };
    }

    define_pixel_dimension_parser!(
        parse_layout_width,
        LayoutWidth,
        LayoutWidthParseError,
        LayoutWidthParseErrorOwned
    );
    define_pixel_dimension_parser!(
        parse_layout_height,
        LayoutHeight,
        LayoutHeightParseError,
        LayoutHeightParseErrorOwned
    );
    define_pixel_dimension_parser!(
        parse_layout_min_width,
        LayoutMinWidth,
        LayoutMinWidthParseError,
        LayoutMinWidthParseErrorOwned
    );
    define_pixel_dimension_parser!(
        parse_layout_min_height,
        LayoutMinHeight,
        LayoutMinHeightParseError,
        LayoutMinHeightParseErrorOwned
    );
    define_pixel_dimension_parser!(
        parse_layout_max_width,
        LayoutMaxWidth,
        LayoutMaxWidthParseError,
        LayoutMaxWidthParseErrorOwned
    );
    define_pixel_dimension_parser!(
        parse_layout_max_height,
        LayoutMaxHeight,
        LayoutMaxHeightParseError,
        LayoutMaxHeightParseErrorOwned
    );

    // -- Box Sizing Parser --

    #[derive(Clone, PartialEq)]
    pub enum LayoutBoxSizingParseError<'a> {
        InvalidValue(&'a str),
    }

    impl_debug_as_display!(LayoutBoxSizingParseError<'a>);
    impl_display! { LayoutBoxSizingParseError<'a>, {
        InvalidValue(v) => format!("Invalid box-sizing value: \"{}\"", v),
    }}

    #[derive(Debug, Clone, PartialEq)]
    pub enum LayoutBoxSizingParseErrorOwned {
        InvalidValue(String),
    }

    impl<'a> LayoutBoxSizingParseError<'a> {
        pub fn to_contained(&self) -> LayoutBoxSizingParseErrorOwned {
            match self {
                LayoutBoxSizingParseError::InvalidValue(s) => {
                    LayoutBoxSizingParseErrorOwned::InvalidValue(s.to_string())
                }
            }
        }
    }

    impl LayoutBoxSizingParseErrorOwned {
        pub fn to_shared<'a>(&'a self) -> LayoutBoxSizingParseError<'a> {
            match self {
                LayoutBoxSizingParseErrorOwned::InvalidValue(s) => {
                    LayoutBoxSizingParseError::InvalidValue(s)
                }
            }
        }
    }

    pub fn parse_layout_box_sizing<'a>(
        input: &'a str,
    ) -> Result<LayoutBoxSizing, LayoutBoxSizingParseError<'a>> {
        match input.trim() {
            "content-box" => Ok(LayoutBoxSizing::ContentBox),
            "border-box" => Ok(LayoutBoxSizing::BorderBox),
            other => Err(LayoutBoxSizingParseError::InvalidValue(other)),
        }
    }
}

#[cfg(feature = "parser")]
pub use self::parser::*;

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;
    use crate::props::basic::pixel::PixelValue;

    #[test]
    fn test_parse_layout_width() {
        assert_eq!(
            parse_layout_width("150px").unwrap(),
            LayoutWidth {
                inner: PixelValue::px(150.0)
            }
        );
        assert_eq!(
            parse_layout_width("2.5em").unwrap(),
            LayoutWidth {
                inner: PixelValue::em(2.5)
            }
        );
        assert_eq!(
            parse_layout_width("75%").unwrap(),
            LayoutWidth {
                inner: PixelValue::percent(75.0)
            }
        );
        assert_eq!(
            parse_layout_width("0").unwrap(),
            LayoutWidth {
                inner: PixelValue::px(0.0)
            }
        );
        assert_eq!(
            parse_layout_width("  100pt  ").unwrap(),
            LayoutWidth {
                inner: PixelValue::pt(100.0)
            }
        );
    }

    #[test]
    fn test_parse_layout_height_invalid() {
        assert!(parse_layout_height("auto").is_err());
        assert!(parse_layout_height("150 px").is_err());
        assert!(parse_layout_height("px").is_err());
        assert!(parse_layout_height("invalid").is_err());
    }

    #[test]
    fn test_parse_layout_box_sizing() {
        assert_eq!(
            parse_layout_box_sizing("content-box").unwrap(),
            LayoutBoxSizing::ContentBox
        );
        assert_eq!(
            parse_layout_box_sizing("border-box").unwrap(),
            LayoutBoxSizing::BorderBox
        );
        assert_eq!(
            parse_layout_box_sizing("  border-box  ").unwrap(),
            LayoutBoxSizing::BorderBox
        );
    }

    #[test]
    fn test_parse_layout_box_sizing_invalid() {
        assert!(parse_layout_box_sizing("padding-box").is_err());
        assert!(parse_layout_box_sizing("borderbox").is_err());
        assert!(parse_layout_box_sizing("").is_err());
    }
}
