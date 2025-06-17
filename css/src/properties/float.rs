//! CSS `float` property

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

/// Specifies whether an element should float to the left, right, or not at all.
///
/// [MDN Reference](https://developer.mozilla.org/en-US/docs/Web/CSS/float)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutFloat {
    /// The element floats to the left of its container.
    Left,
    /// The element floats to the right of its container.
    Right,
    /// The element does not float, and will be displayed where it occurs in the text. (Default value)
    None,
    // NOTE: `Initial` and `Inherit` are handled by `CssPropertyValue` wrapper.
}

impl Default for LayoutFloat {
    fn default() -> Self {
        LayoutFloat::None // CSS default is "none"
    }
}

impl PrintAsCssValue for LayoutFloat {
    fn print_as_css_value(&self) -> String {
        alloc::format!("{}", self)
    }
}

impl fmt::Display for LayoutFloat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LayoutFloat::Left => write!(f, "left"),
            LayoutFloat::Right => write!(f, "right"),
            LayoutFloat::None => write!(f, "none"),
        }
    }
}

/// Typedef for `CssPropertyValue<LayoutFloat>`.
pub type LayoutFloatValue = CssPropertyValue<LayoutFloat>;

crate::impl_option!(
    LayoutFloatValue,
    OptionLayoutFloatValue,
    copy = false, // Since CssPropertyValue contains a Box for Exact variant if T is not Copy
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;

    multi_type_parser!(
        parse_impl,
        LayoutFloat,
        ["left", LayoutFloat::Left],
        ["right", LayoutFloat::Right],
        ["none", LayoutFloat::None]
    );

    impl FormatAsCssValue for LayoutFloat {
        fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
            fmt::Display::fmt(self, f)
        }
    }

    /// Parses the `float` CSS property.
    pub fn parse<'a>(value_str: &'a str, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> Result<LayoutFloat, InvalidValueErr<'a>> {
        crate::css_debug_log!(debug_messages, "float: parsing \"{}\"", value_str);
        let trimmed_value = value_str.trim();
        let result = parse_impl(trimmed_value);
        if result.is_err() {
            crate::css_debug_log!(debug_messages, "float: parse failed for \"{}\"", trimmed_value);
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
    fn test_parse_float_valid() {
        let mut debug_logs = Some(Vec::new());
        assert_eq!(parse("left", &mut debug_logs), Ok(LayoutFloat::Left));
        assert_eq!(parse("right", &mut debug_logs), Ok(LayoutFloat::Right));
        assert_eq!(parse("none", &mut debug_logs), Ok(LayoutFloat::None));
        assert_eq!(parse("  left  ", &mut debug_logs), Ok(LayoutFloat::Left));

        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("float: parsing \"left\""));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_float_invalid() {
        let mut debug_logs = Some(Vec::new());
        assert!(parse("center", &mut debug_logs).is_err());
        assert!(parse("", &mut debug_logs).is_err());

        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("float: parse failed for \"center\""));
    }

    #[test]
    fn test_layout_float_default() {
        assert_eq!(LayoutFloat::default(), LayoutFloat::None);
    }

    #[test]
    fn test_float_display_format() {
        assert_eq!(format!("{}", LayoutFloat::Right), "right");
    }

    #[test]
    fn test_print_as_css_value() {
        assert_eq!(LayoutFloat::None.print_as_css_value(), "none");
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_float_value_initial_inherit() {
        use crate::parser::parse_css_property_value;
        let mut debug_logs = Some(Vec::new());

        let res_initial = parse_css_property_value("float", "initial", &mut debug_logs);
        assert_eq!(res_initial, Ok(crate::CssProperty::Float(CssPropertyValue::Initial)));

        let res_inherit = parse_css_property_value("float", "inherit", &mut debug_logs);
        assert_eq!(res_inherit, Ok(crate::CssProperty::Float(CssPropertyValue::Inherit)));

        let res_left = parse_css_property_value("float", "left", &mut debug_logs);
        assert_eq!(res_left, Ok(crate::CssProperty::Float(CssPropertyValue::Exact(LayoutFloat::Left))));
    }
}
