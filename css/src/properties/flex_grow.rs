//! `flex-grow` CSS property

use crate::{
    css_properties::{parser_input_span, FloatValue}, // Assuming FloatValue is here
    error::Error,
    parser::CssParsable, // Will need a parser for f32
    print_css::PrintAsCssValue,
};
use cssparser::Parser;

/// `flex-grow` CSS property
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutFlexGrow {
    pub inner: FloatValue,
}

impl Default for LayoutFlexGrow {
    fn default() -> Self {
        LayoutFlexGrow {
            inner: FloatValue::const_new(0), // Default flex-grow is 0
        }
    }
}

// Use the existing macro from css_properties.rs
// NOTE: This assumes impl_float_value is globally visible or imported.
// If it's a private macro, this will fail and it might need to be moved/copied.
crate::impl_float_value!(LayoutFlexGrow);

impl PrintAsCssValue for LayoutFlexGrow {
    fn print_as_css_value<W: core::fmt::Write>(&self, formatter: &mut W) -> core::fmt::Result {
        self.inner.print_as_css_value(formatter)
    }
}

impl<'i> CssParsable<'i> for LayoutFlexGrow {
    fn parse(input: &mut Parser<'i, '_>) -> Result<Self, cssparser::ParseError<'i, Error<'i>>> {
        let value = input.expect_number()?;
        if value < 0.0 {
            Err(input.new_custom_error(Error::InvalidValue("flex-grow cannot be negative".into())))
        } else {
            Ok(LayoutFlexGrow::new(value))
        }
    }
}

crate::impl_option!(
    LayoutFlexGrow,
    OptionLayoutFlexGrowValue,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Value of the `flex-grow` CSS property
pub type LayoutFlexGrowValue = OptionLayoutFlexGrowValue;

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;
    use crate::{
        parser::{multi_type_parser, ParseContext},
    };

    // Using multi_type_parser, but it might need a specific parser if strict type checking is needed
    // For now, assuming CssParsable's impl is sufficient.
    multi_type_parser!(
        LayoutFlexGrow,
        parse_flex_grow,
        "flex-grow",
        "LayoutFlexGrow"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_common::*;

    #[test]
    fn test_flex_grow_parsing() {
        assert_parse_ok!(LayoutFlexGrow, "0", LayoutFlexGrow::new(0.0));
        assert_parse_ok!(LayoutFlexGrow, "1", LayoutFlexGrow::new(1.0));
        assert_parse_ok!(LayoutFlexGrow, "2.5", LayoutFlexGrow::new(2.5));
        assert_parse_error!(LayoutFlexGrow, "-1"); // Negative not allowed
        assert_parse_error!(LayoutFlexGrow, "auto");
    }

    #[test]
    fn test_flex_grow_default() {
        assert_eq!(LayoutFlexGrow::default(), LayoutFlexGrow::new(0.0));
    }

    #[test]
    fn test_flex_grow_print_as_css() {
        assert_print_as_css!(LayoutFlexGrow::new(0.0), "0");
        assert_print_as_css!(LayoutFlexGrow::new(1.5), "1.5");
    }
}
