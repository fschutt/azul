//! `justify-content` CSS property

use crate::{
    css_properties::parser_input_span,
    error::Error,
    parser::{multi_type_parser, CssParsable},
    print_css::PrintAsCssValue,
};
use alloc::string::ToString;
use cssparser::Parser;

/// `justify-content` CSS property
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum LayoutJustifyContent {
    /// Default value. Items are positioned at the beginning of the container
    Start,
    /// Items are positioned at the end of the container
    End,
    /// Items are positioned at the center of the container
    Center,
    /// Items are positioned with space between the lines
    SpaceBetween,
    /// Items are positioned with space before, between, and after the lines
    SpaceAround,
    /// Items are distributed so that the spacing between any two adjacent alignment subjects,
    /// before the first alignment subject, and after the last alignment subject is the same
    SpaceEvenly,
}

impl Default for LayoutJustifyContent {
    fn default() -> Self {
        LayoutJustifyContent::Start
    }
}

impl core::fmt::Display for LayoutJustifyContent {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                LayoutJustifyContent::Start => "flex-start", // CSS uses "flex-start"
                LayoutJustifyContent::End => "flex-end",     // CSS uses "flex-end"
                LayoutJustifyContent::Center => "center",
                LayoutJustifyContent::SpaceBetween => "space-between",
                LayoutJustifyContent::SpaceAround => "space-around",
                LayoutJustifyContent::SpaceEvenly => "space-evenly",
            }
        )
    }
}

impl PrintAsCssValue for LayoutJustifyContent {
    fn print_as_css_value<W: core::fmt::Write>(&self, formatter: &mut W) -> core::fmt::Result {
        formatter.write_str(&self.to_string())
    }
}

impl<'i> CssParsable<'i> for LayoutJustifyContent {
    fn parse(input: &mut Parser<'i, '_>) -> Result<Self, cssparser::ParseError<'i, Error<'i>>> {
        input.expect_ident_matching("flex-start").map(|_| LayoutJustifyContent::Start)
        .or_else(|_| input.expect_ident_matching("start").map(|_| LayoutJustifyContent::Start))
        .or_else(|_| input.expect_ident_matching("flex-end").map(|_| LayoutJustifyContent::End))
        .or_else(|_| input.expect_ident_matching("end").map(|_| LayoutJustifyContent::End))
        .or_else(|_| input.expect_ident_matching("center").map(|_| LayoutJustifyContent::Center))
        .or_else(|_| input.expect_ident_matching("space-between").map(|_| LayoutJustifyContent::SpaceBetween))
        .or_else(|_| input.expect_ident_matching("space-around").map(|_| LayoutJustifyContent::SpaceAround))
        .or_else(|_| input.expect_ident_matching("space-evenly").map(|_| LayoutJustifyContent::SpaceEvenly))
    }
}

crate::impl_option!(
    LayoutJustifyContent,
    OptionLayoutJustifyContentValue,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Value of the `justify-content` CSS property
pub type LayoutJustifyContentValue = OptionLayoutJustifyContentValue;

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;
    use crate::parser::ParseContext;

    multi_type_parser!(
        LayoutJustifyContent,
        parse_justify_content,
        "justify-content",
        "LayoutJustifyContent"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_common::*;

    #[test]
    fn test_justify_content_parsing() {
        assert_parse_ok!(LayoutJustifyContent, "flex-start", LayoutJustifyContent::Start);
        assert_parse_ok!(LayoutJustifyContent, "start", LayoutJustifyContent::Start);
        assert_parse_ok!(LayoutJustifyContent, "flex-end", LayoutJustifyContent::End);
        assert_parse_ok!(LayoutJustifyContent, "end", LayoutJustifyContent::End);
        assert_parse_ok!(LayoutJustifyContent, "center", LayoutJustifyContent::Center);
        assert_parse_ok!(LayoutJustifyContent, "space-between", LayoutJustifyContent::SpaceBetween);
        assert_parse_ok!(LayoutJustifyContent, "space-around", LayoutJustifyContent::SpaceAround);
        assert_parse_ok!(LayoutJustifyContent, "space-evenly", LayoutJustifyContent::SpaceEvenly);
        assert_parse_error!(LayoutJustifyContent, "stretch");
    }

    #[test]
    fn test_justify_content_default() {
        assert_eq!(LayoutJustifyContent::default(), LayoutJustifyContent::Start);
    }

    #[test]
    fn test_justify_content_display() {
        assert_eq!(LayoutJustifyContent::Start.to_string(), "flex-start");
        assert_eq!(LayoutJustifyContent::End.to_string(), "flex-end");
        assert_eq!(LayoutJustifyContent::Center.to_string(), "center");
        assert_eq!(LayoutJustifyContent::SpaceBetween.to_string(), "space-between");
        assert_eq!(LayoutJustifyContent::SpaceAround.to_string(), "space-around");
        assert_eq!(LayoutJustifyContent::SpaceEvenly.to_string(), "space-evenly");
    }

    #[test]
    fn test_justify_content_print_as_css() {
        assert_print_as_css!(LayoutJustifyContent::Start, "flex-start");
        assert_print_as_css!(LayoutJustifyContent::SpaceBetween, "space-between");
    }
}
