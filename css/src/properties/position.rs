//! CSS `position` property

use crate::css::{CssPropertyValue, PrintAsCssValue};
#[cfg(feature = "parser")]
use crate::parser::{InvalidValueErr, FormatAsCssValue, multi_type_parser};
use core::fmt;
#[cfg(feature = "parser")]
use crate::css_debug_log; // Corrected import
#[cfg(feature = "parser")]
use crate::LayoutDebugMessage;
#[cfg(feature = "parser")]
use alloc::vec::Vec;

/// Specifies the type of positioning method used for an element.
///
/// [MDN Reference](https://developer.mozilla.org/en-US/docs/Web/CSS/position)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutPosition {
    /// The element is positioned according to the normal flow of the document. (Default value)
    Static,
    /// The element is positioned according to the normal flow of the document,
    /// and then offset relative to itself based on the values of `top`, `right`, `bottom`, and `left`.
    Relative,
    /// The element is removed from the normal document flow, and no space is created for
    /// the element in the page layout. It is positioned relative to its closest positioned
    /// ancestor, if any; otherwise, it is placed relative to the initial containing block.
    Absolute,
    /// The element is removed from the normal document flow, and no space is created for
    /// the element in the page layout. It is positioned relative to the initial containing block.
    Fixed,
    // NOTE: `Initial` and `Inherit` are handled by `CssPropertyValue` wrapper.
    // `sticky` is not yet supported.
}

impl Default for LayoutPosition {
    fn default() -> Self {
        LayoutPosition::Static
    }
}

impl LayoutPosition {
    pub fn is_positioned(&self) -> bool {
        *self != LayoutPosition::Static
    }
}

impl PrintAsCssValue for LayoutPosition {
    fn print_as_css_value(&self) -> String {
        alloc::format!("{}", self)
    }
}

impl fmt::Display for LayoutPosition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LayoutPosition::Static => write!(f, "static"),
            LayoutPosition::Relative => write!(f, "relative"),
            LayoutPosition::Absolute => write!(f, "absolute"),
            LayoutPosition::Fixed => write!(f, "fixed"),
        }
    }
}

/// Typedef for `CssPropertyValue<LayoutPosition>`.
pub type LayoutPositionValue = CssPropertyValue<LayoutPosition>;

crate::impl_option!(
    LayoutPositionValue,
    OptionLayoutPositionValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;

    multi_type_parser!(
        parse_impl,
        LayoutPosition,
        ["static", LayoutPosition::Static],
        ["relative", LayoutPosition::Relative],
        ["absolute", LayoutPosition::Absolute],
        ["fixed", LayoutPosition::Fixed]
    );

    impl FormatAsCssValue for LayoutPosition {
        fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
            fmt::Display::fmt(self, f)
        }
    }

    /// Parses the `position` CSS property.
    pub fn parse<'a>(value_str: &'a str, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> Result<LayoutPosition, InvalidValueErr<'a>> {
        css_debug_log!(debug_messages, "position: parsing \"{}\"", value_str);
        let trimmed_value = value_str.trim();
        let result = parse_impl(trimmed_value);
        if result.is_err() {
            css_debug_log!(debug_messages, "position: parse failed for \"{}\"", trimmed_value);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::CssPropertyValue;
    #[cfg(feature = "parser")]
    use super::parser::parse;
    #[cfg(feature = "parser")]
    use crate::{debug, parser::parse_css_property_value, CssProperty};

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_position_valid() {
        let mut debug_logs = Some(Vec::new());
        assert_eq!(parse("static", &mut debug_logs), Ok(LayoutPosition::Static));
        assert_eq!(parse("relative", &mut debug_logs), Ok(LayoutPosition::Relative));
        assert_eq!(parse("absolute", &mut debug_logs), Ok(LayoutPosition::Absolute));
        assert_eq!(parse("fixed", &mut debug_logs), Ok(LayoutPosition::Fixed));
        assert_eq!(parse("  relative  ", &mut debug_logs), Ok(LayoutPosition::Relative));

        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("position: parsing \"static\""));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_position_invalid() {
        let mut debug_logs = Some(Vec::new());
        assert!(parse("sticky", &mut debug_logs).is_err()); // sticky not supported yet
        assert!(parse("", &mut debug_logs).is_err());

        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("position: parse failed for \"sticky\""));
    }

    #[test]
    fn test_layout_position_default() {
        assert_eq!(LayoutPosition::default(), LayoutPosition::Static);
    }

    #[test]
    fn test_position_is_positioned() {
        assert!(!LayoutPosition::Static.is_positioned());
        assert!(LayoutPosition::Relative.is_positioned());
        assert!(LayoutPosition::Absolute.is_positioned());
        assert!(LayoutPosition::Fixed.is_positioned());
    }

    #[test]
    fn test_position_display_format() {
        assert_eq!(format!("{}", LayoutPosition::Absolute), "absolute");
    }

    #[test]
    fn test_print_as_css_value() {
        assert_eq!(LayoutPosition::Relative.print_as_css_value(), "relative");
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_position_value_keywords() {
        let mut debug_logs = Some(Vec::new());

        let res_initial = parse_css_property_value("position", "initial", &mut debug_logs);
        assert_eq!(res_initial, Ok(CssProperty::Position(CssPropertyValue::Initial)));

        let res_inherit = parse_css_property_value("position", "inherit", &mut debug_logs);
        assert_eq!(res_inherit, Ok(CssProperty::Position(CssPropertyValue::Inherit)));

        let res_static = parse_css_property_value("position", "static", &mut debug_logs);
        assert_eq!(res_static, Ok(CssProperty::Position(CssPropertyValue::Exact(LayoutPosition::Static))));
    }
}
