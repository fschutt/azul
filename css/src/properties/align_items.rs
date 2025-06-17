//! `align-items` CSS property

use crate::{
    css_properties::parser_input_span,
    error::Error,
    parser::{multi_type_parser, CssParsable},
    print_css::PrintAsCssValue,
};
use alloc::string::ToString;
use cssparser::Parser;

/// `align-items` CSS property
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum LayoutAlignItems {
    /// Items are stretched to fit the container
    Stretch,
    /// Items are positioned at the center of the container
    Center,
    /// Items are positioned at the beginning of the container
    FlexStart,
    /// Items are positioned at the end of the container
    FlexEnd,
    // Note: "baseline" is not included as it's more complex and might not be used.
}

impl Default for LayoutAlignItems {
    fn default() -> Self {
        LayoutAlignItems::Stretch // CSS default is stretch
    }
}

impl core::fmt::Display for LayoutAlignItems {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                LayoutAlignItems::Stretch => "stretch",
                LayoutAlignItems::Center => "center",
                LayoutAlignItems::FlexStart => "flex-start",
                LayoutAlignItems::FlexEnd => "flex-end",
            }
        )
    }
}

impl PrintAsCssValue for LayoutAlignItems {
    fn print_as_css_value<W: core::fmt::Write>(&self, formatter: &mut W) -> core::fmt::Result {
        formatter.write_str(&self.to_string())
    }
}

impl<'i> CssParsable<'i> for LayoutAlignItems {
    fn parse(input: &mut Parser<'i, '_>) -> Result<Self, cssparser::ParseError<'i, Error<'i>>> {
        input.expect_ident_matching("stretch").map(|_| LayoutAlignItems::Stretch)
        .or_else(|_| input.expect_ident_matching("center").map(|_| LayoutAlignItems::Center))
        .or_else(|_| input.expect_ident_matching("flex-start").map(|_| LayoutAlignItems::FlexStart))
        .or_else(|_| input.expect_ident_matching("start").map(|_| LayoutAlignItems::FlexStart)) // "start" is an alias for "flex-start"
        .or_else(|_| input.expect_ident_matching("flex-end").map(|_| LayoutAlignItems::FlexEnd))
        .or_else(|_| input.expect_ident_matching("end").map(|_| LayoutAlignItems::FlexEnd)) // "end" is an alias for "flex-end"
        // "self-start", "self-end", "baseline", "first baseline", "last baseline" are other values, not implemented here
    }
}

crate::impl_option!(
    LayoutAlignItems,
    OptionLayoutAlignItemsValue,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Value of the `align-items` CSS property
pub type LayoutAlignItemsValue = OptionLayoutAlignItemsValue;

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;
    use crate::parser::ParseContext;

    multi_type_parser!(
        LayoutAlignItems,
        parse_align_items,
        "align-items",
        "LayoutAlignItems"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_common::*;

    #[test]
    fn test_align_items_parsing() {
        assert_parse_ok!(LayoutAlignItems, "stretch", LayoutAlignItems::Stretch);
        assert_parse_ok!(LayoutAlignItems, "center", LayoutAlignItems::Center);
        assert_parse_ok!(LayoutAlignItems, "flex-start", LayoutAlignItems::FlexStart);
        assert_parse_ok!(LayoutAlignItems, "start", LayoutAlignItems::FlexStart);
        assert_parse_ok!(LayoutAlignItems, "flex-end", LayoutAlignItems::FlexEnd);
        assert_parse_ok!(LayoutAlignItems, "end", LayoutAlignItems::FlexEnd);
        assert_parse_error!(LayoutAlignItems, "space-between");
    }

    #[test]
    fn test_align_items_default() {
        assert_eq!(LayoutAlignItems::default(), LayoutAlignItems::Stretch);
    }

    #[test]
    fn test_align_items_display() {
        assert_eq!(LayoutAlignItems::Stretch.to_string(), "stretch");
        assert_eq!(LayoutAlignItems::Center.to_string(), "center");
        assert_eq!(LayoutAlignItems::FlexStart.to_string(), "flex-start");
        assert_eq!(LayoutAlignItems::FlexEnd.to_string(), "flex-end");
    }

    #[test]
    fn test_align_items_print_as_css() {
        assert_print_as_css!(LayoutAlignItems::Stretch, "stretch");
        assert_print_as_css!(LayoutAlignItems::FlexStart, "flex-start");
    }
}
