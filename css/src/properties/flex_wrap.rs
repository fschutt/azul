//! `flex-wrap` CSS property

use crate::{
    css_properties::parser_input_span,
    error::Error,
    parser::{multi_type_parser, CssParsable},
    print_css::PrintAsCssValue,
};
use alloc::string::ToString;
use cssparser::Parser;

/// `flex-wrap` CSS property
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum LayoutFlexWrap {
    /// `wrap`
    Wrap,
    /// `nowrap`
    NoWrap,
}

impl Default for LayoutFlexWrap {
    fn default() -> Self {
        LayoutFlexWrap::NoWrap
    }
}

impl core::fmt::Display for LayoutFlexWrap {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                LayoutFlexWrap::Wrap => "wrap",
                LayoutFlexWrap::NoWrap => "nowrap",
            }
        )
    }
}

impl PrintAsCssValue for LayoutFlexWrap {
    fn print_as_css_value<W: core::fmt::Write>(&self, formatter: &mut W) -> core::fmt::Result {
        formatter.write_str(&self.to_string())
    }
}

impl<'i> CssParsable<'i> for LayoutFlexWrap {
    fn parse(input: &mut Parser<'i, '_>) -> Result<Self, cssparser::ParseError<'i, Error<'i>>> {
        input.expect_ident_matching("wrap").map(|_| LayoutFlexWrap::Wrap)
        .or_else(|_| input.expect_ident_matching("nowrap").map(|_| LayoutFlexWrap::NoWrap))
    }
}

crate::impl_option!(
    LayoutFlexWrap,
    OptionLayoutFlexWrapValue,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Value of the `flex-wrap` CSS property
pub type LayoutFlexWrapValue = OptionLayoutFlexWrapValue;

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;
    use crate::parser::ParseContext;

    multi_type_parser!(
        LayoutFlexWrap,
        parse_flex_wrap,
        "flex-wrap",
        "LayoutFlexWrap"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_common::*;

    #[test]
    fn test_flex_wrap_parsing() {
        assert_parse_ok!(LayoutFlexWrap, "wrap", LayoutFlexWrap::Wrap);
        assert_parse_ok!(LayoutFlexWrap, "nowrap", LayoutFlexWrap::NoWrap);
        assert_parse_error!(LayoutFlexWrap, "flex-start");
    }

    #[test]
    fn test_flex_wrap_display() {
        assert_eq!(LayoutFlexWrap::Wrap.to_string(), "wrap");
        assert_eq!(LayoutFlexWrap::NoWrap.to_string(), "nowrap");
    }

    #[test]
    fn test_flex_wrap_default() {
        assert_eq!(LayoutFlexWrap::default(), LayoutFlexWrap::NoWrap);
    }

    #[test]
    fn test_flex_wrap_print_as_css() {
        assert_print_as_css!(LayoutFlexWrap::Wrap, "wrap");
        assert_print_as_css!(LayoutFlexWrap::NoWrap, "nowrap");
    }
}
