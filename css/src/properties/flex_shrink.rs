//! `flex-shrink` CSS property

use crate::{
    css_properties::{parser_input_span, FloatValue}, // Assuming FloatValue is here
    error::Error,
    parser::CssParsable, // Will need a parser for f32
    print_css::PrintAsCssValue,
};
use cssparser::Parser;

/// `flex-shrink` CSS property
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutFlexShrink {
    pub inner: FloatValue,
}

impl Default for LayoutFlexShrink {
    fn default() -> Self {
        LayoutFlexShrink {
            inner: FloatValue::const_new(1), // Default flex-shrink is 1
        }
    }
}

// Use the existing macro from css_properties.rs
crate::impl_float_value!(LayoutFlexShrink);

impl PrintAsCssValue for LayoutFlexShrink {
    fn print_as_css_value<W: core::fmt::Write>(&self, formatter: &mut W) -> core::fmt::Result {
        self.inner.print_as_css_value(formatter)
    }
}

impl<'i> CssParsable<'i> for LayoutFlexShrink {
    fn parse(input: &mut Parser<'i, '_>) -> Result<Self, cssparser::ParseError<'i, Error<'i>>> {
        let value = input.expect_number()?;
        if value < 0.0 {
            Err(input.new_custom_error(Error::InvalidValue("flex-shrink cannot be negative".into())))
        } else {
            Ok(LayoutFlexShrink::new(value))
        }
    }
}

crate::impl_option!(
    LayoutFlexShrink,
    OptionLayoutFlexShrinkValue,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Value of the `flex-shrink` CSS property
pub type LayoutFlexShrinkValue = OptionLayoutFlexShrinkValue;

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;
    use crate::{
        parser::{multi_type_parser, ParseContext},
    };

    multi_type_parser!(
        LayoutFlexShrink,
        parse_flex_shrink,
        "flex-shrink",
        "LayoutFlexShrink"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_common::*;

    #[test]
    fn test_flex_shrink_parsing() {
        assert_parse_ok!(LayoutFlexShrink, "0", LayoutFlexShrink::new(0.0));
        assert_parse_ok!(LayoutFlexShrink, "1", LayoutFlexShrink::new(1.0));
        assert_parse_ok!(LayoutFlexShrink, "2.5", LayoutFlexShrink::new(2.5));
        assert_parse_error!(LayoutFlexShrink, "-1"); // Negative not allowed
        assert_parse_error!(LayoutFlexShrink, "auto");
    }

    #[test]
    fn test_flex_shrink_default() {
        assert_eq!(LayoutFlexShrink::default(), LayoutFlexShrink::new(1.0));
    }

    #[test]
    fn test_flex_shrink_print_as_css() {
        assert_print_as_css!(LayoutFlexShrink::new(0.0), "0");
        assert_print_as_css!(LayoutFlexShrink::new(1.5), "1.5");
    }
}
