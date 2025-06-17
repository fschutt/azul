//! `align-content` CSS property

use crate::{
    css_properties::parser_input_span,
    error::Error,
    parser::{multi_type_parser, CssParsable},
    print_css::PrintAsCssValue,
};
use alloc::string::ToString;
use cssparser::Parser;

/// `align-content` CSS property
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum LayoutAlignContent {
    /// Default value. Lines stretch to take up the remaining space
    Stretch,
    /// Lines are packed toward the center of the flex container
    Center,
    /// Lines are packed toward the start of the flex container
    Start, // Alias for FlexStart in some contexts
    /// Lines are packed toward the end of the flex container
    End, // Alias for FlexEnd in some contexts
    /// Lines are evenly distributed in the flex container
    SpaceBetween,
    /// Lines are evenly distributed in the flex container, with half-size spaces on either end
    SpaceAround,
    // SpaceEvenly is also a valid value, but might be less common / more complex.
    // Baseline / first baseline / last baseline also exist.
}

impl Default for LayoutAlignContent {
    fn default() -> Self {
        LayoutAlignContent::Stretch // CSS default is stretch
    }
}

impl core::fmt::Display for LayoutAlignContent {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                LayoutAlignContent::Stretch => "stretch",
                LayoutAlignContent::Center => "center",
                LayoutAlignContent::Start => "flex-start", // Typically maps to flex-start
                LayoutAlignContent::End => "flex-end",     // Typically maps to flex-end
                LayoutAlignContent::SpaceBetween => "space-between",
                LayoutAlignContent::SpaceAround => "space-around",
            }
        )
    }
}

impl PrintAsCssValue for LayoutAlignContent {
    fn print_as_css_value<W: core::fmt::Write>(&self, formatter: &mut W) -> core::fmt::Result {
        formatter.write_str(&self.to_string())
    }
}

impl<'i> CssParsable<'i> for LayoutAlignContent {
    fn parse(input: &mut Parser<'i, '_>) -> Result<Self, cssparser::ParseError<'i, Error<'i>>> {
        input.expect_ident_matching("stretch").map(|_| LayoutAlignContent::Stretch)
        .or_else(|_| input.expect_ident_matching("center").map(|_| LayoutAlignContent::Center))
        .or_else(|_| input.expect_ident_matching("flex-start").map(|_| LayoutAlignContent::Start))
        .or_else(|_| input.expect_ident_matching("start").map(|_| LayoutAlignContent::Start))
        .or_else(|_| input.expect_ident_matching("flex-end").map(|_| LayoutAlignContent::End))
        .or_else(|_| input.expect_ident_matching("end").map(|_| LayoutAlignContent::End))
        .or_else(|_| input.expect_ident_matching("space-between").map(|_| LayoutAlignContent::SpaceBetween))
        .or_else(|_| input.expect_ident_matching("space-around").map(|_| LayoutAlignContent::SpaceAround))
    }
}

crate::impl_option!(
    LayoutAlignContent,
    OptionLayoutAlignContentValue,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Value of the `align-content` CSS property
pub type LayoutAlignContentValue = OptionLayoutAlignContentValue;

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;
    use crate::parser::ParseContext;

    multi_type_parser!(
        LayoutAlignContent,
        parse_align_content,
        "align-content",
        "LayoutAlignContent"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_common::*;

    #[test]
    fn test_align_content_parsing() {
        assert_parse_ok!(LayoutAlignContent, "stretch", LayoutAlignContent::Stretch);
        assert_parse_ok!(LayoutAlignContent, "center", LayoutAlignContent::Center);
        assert_parse_ok!(LayoutAlignContent, "flex-start", LayoutAlignContent::Start);
        assert_parse_ok!(LayoutAlignContent, "start", LayoutAlignContent::Start);
        assert_parse_ok!(LayoutAlignContent, "flex-end", LayoutAlignContent::End);
        assert_parse_ok!(LayoutAlignContent, "end", LayoutAlignContent::End);
        assert_parse_ok!(LayoutAlignContent, "space-between", LayoutAlignContent::SpaceBetween);
        assert_parse_ok!(LayoutAlignContent, "space-around", LayoutAlignContent::SpaceAround);
        assert_parse_error!(LayoutAlignContent, "space-evenly"); // Not implemented in this enum
    }

    #[test]
    fn test_align_content_default() {
        assert_eq!(LayoutAlignContent::default(), LayoutAlignContent::Stretch);
    }

    #[test]
    fn test_align_content_display() {
        assert_eq!(LayoutAlignContent::Stretch.to_string(), "stretch");
        assert_eq!(LayoutAlignContent::Center.to_string(), "center");
        assert_eq!(LayoutAlignContent::Start.to_string(), "flex-start");
        assert_eq!(LayoutAlignContent::End.to_string(), "flex-end");
        assert_eq!(LayoutAlignContent::SpaceBetween.to_string(), "space-between");
        assert_eq!(LayoutAlignContent::SpaceAround.to_string(), "space-around");
    }

    #[test]
    fn test_align_content_print_as_css() {
        assert_print_as_css!(LayoutAlignContent::Stretch, "stretch");
        assert_print_as_css!(LayoutAlignContent::SpaceBetween, "space-between");
    }
}
