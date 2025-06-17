//! CSS `box-sizing` property

use crate::css::{CssPropertyValue, PrintAsCssValue};
#[cfg(feature = "parser")]
use crate::parser::{InvalidValueErr, FormatAsCssValue, multi_type_parser};
use core::fmt;
#[cfg(feature = "parser")]
use crate::css_debug_log;
#[cfg(feature = "parser")]
use crate::LayoutDebugMessage;
#[cfg(feature = "parser")]
use alloc::vec::Vec;

/// Defines how the user agent should calculate the total width and height of an element.
///
/// [MDN Reference](https://developer.mozilla.org/en-US/docs/Web/CSS/box-sizing)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutBoxSizing {
    /// The width and height properties include the content, padding, and border, but not the margin. (Default value in CSS3)
    BorderBox,
    /// The width and height properties include only the content. Padding, border, and margin are outside. (Default value before CSS3)
    ContentBox,
    // NOTE: `Initial` and `Inherit` are handled by `CssPropertyValue` wrapper.
}

impl Default for LayoutBoxSizing {
    fn default() -> Self {
        LayoutBoxSizing::ContentBox // CSS default is `content-box`
    }
}

impl PrintAsCssValue for LayoutBoxSizing {
    fn print_as_css_value(&self) -> String {
        alloc::format!("{}", self)
    }
}

impl fmt::Display for LayoutBoxSizing {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LayoutBoxSizing::BorderBox => write!(f, "border-box"),
            LayoutBoxSizing::ContentBox => write!(f, "content-box"),
        }
    }
}

/// Typedef for `CssPropertyValue<LayoutBoxSizing>`.
pub type LayoutBoxSizingValue = CssPropertyValue<LayoutBoxSizing>;

crate::impl_option!(
    LayoutBoxSizingValue,
    OptionLayoutBoxSizingValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;

    multi_type_parser!(
        parse_impl,
        LayoutBoxSizing,
        ["border-box", LayoutBoxSizing::BorderBox],
        ["content-box", LayoutBoxSizing::ContentBox]
    );

    impl FormatAsCssValue for LayoutBoxSizing {
        fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
            fmt::Display::fmt(self, f)
        }
    }

    /// Parses the `box-sizing` CSS property.
    pub fn parse<'a>(value_str: &'a str, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> Result<LayoutBoxSizing, InvalidValueErr<'a>> {
        crate::css_debug_log!(debug_messages, "box-sizing: parsing \"{}\"", value_str);
        let trimmed_value = value_str.trim();
        let result = parse_impl(trimmed_value);
        if result.is_err() {
            crate::css_debug_log!(debug_messages, "box-sizing: parse failed for \"{}\"", trimmed_value);
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
    use crate::debug;

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_box_sizing_valid() {
        let mut debug_logs = Some(Vec::new());
        assert_eq!(parse("border-box", &mut debug_logs), Ok(LayoutBoxSizing::BorderBox));
        assert_eq!(parse("content-box", &mut debug_logs), Ok(LayoutBoxSizing::ContentBox));
        assert_eq!(parse("  border-box  ", &mut debug_logs), Ok(LayoutBoxSizing::BorderBox));

        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("box-sizing: parsing \"border-box\""));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_box_sizing_invalid() {
        let mut debug_logs = Some(Vec::new());
        assert!(parse("padding-box", &mut debug_logs).is_err()); // padding-box is valid in CSS but maybe not supported here
        assert!(parse("", &mut debug_logs).is_err());

        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("box-sizing: parse failed for \"padding-box\""));
    }

    #[test]
    fn test_layout_box_sizing_default() {
        assert_eq!(LayoutBoxSizing::default(), LayoutBoxSizing::ContentBox);
    }

    #[test]
    fn test_box_sizing_display_format() {
        assert_eq!(format!("{}", LayoutBoxSizing::BorderBox), "border-box");
    }

    #[test]
    fn test_print_as_css_value() {
        assert_eq!(LayoutBoxSizing::ContentBox.print_as_css_value(), "content-box");
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_box_sizing_value_initial_inherit() {
        use crate::parser::parse_css_property_value;
        let mut debug_logs = Some(Vec::new());

        let res_initial = parse_css_property_value("box-sizing", "initial", &mut debug_logs);
        assert_eq!(res_initial, Ok(crate::CssProperty::BoxSizing(CssPropertyValue::Initial)));

        let res_inherit = parse_css_property_value("box-sizing", "inherit", &mut debug_logs);
        assert_eq!(res_inherit, Ok(crate::CssProperty::BoxSizing(CssPropertyValue::Inherit)));

        let res_border_box = parse_css_property_value("box-sizing", "border-box", &mut debug_logs);
        assert_eq!(res_border_box, Ok(crate::CssProperty::BoxSizing(CssPropertyValue::Exact(LayoutBoxSizing::BorderBox))));
    }
}
