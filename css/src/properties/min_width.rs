//! CSS `min-width` property

use crate::css::{CssPropertyValue, PrintAsCssValue};
use crate::css_properties::PixelValue;
#[cfg(feature = "parser")]
use crate::parser::{CssPixelValueParseError, FormatAsCssValue, typed_pixel_value_parser};
use core::fmt;
#[cfg(feature = "parser")]
use crate::css_debug_log;
#[cfg(feature = "parser")]
use crate::{LayoutDebugMessage, parser::InvalidValueErr};
#[cfg(feature = "parser")]
use alloc::vec::Vec;

/// Represents a `min-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMinWidth {
    pub inner: PixelValue,
}

crate::impl_pixel_value!(LayoutMinWidth);

impl PrintAsCssValue for LayoutMinWidth {
    fn print_as_css_value(&self) -> String {
        alloc::format!("{}", self.inner)
    }
}

/// Typedef for `CssPropertyValue<LayoutMinWidth>`.
pub type LayoutMinWidthValue = CssPropertyValue<LayoutMinWidth>;

crate::impl_option!(
    LayoutMinWidthValue,
    OptionLayoutMinWidthValue,
    copy = false, // CssPropertyValue is not Copy
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;

    typed_pixel_value_parser!(parse_min_width_inner, LayoutMinWidth);

    impl FormatAsCssValue for LayoutMinWidth {
        fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
            self.inner.format_as_css_value(f)
        }
    }

    /// Parses the `min-width` CSS property.
    pub fn parse<'a>(value_str: &'a str, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> Result<LayoutMinWidth, InvalidValueErr<'a>> {
        css_debug_log!(debug_messages, "min-width: parsing \"{}\"", value_str);
        let trimmed_value = value_str.trim();

        if trimmed_value.eq_ignore_ascii_case("auto") {
            css_debug_log!(debug_messages, "min-width: parse failed for \"{}\", 'auto' not directly convertible to PixelValue here", trimmed_value);
            return Err(InvalidValueErr(value_str));
        }

        match parse_min_width_inner(trimmed_value) {
            Ok(val) => Ok(val),
            Err(e) => {
                css_debug_log!(debug_messages, "min-width: parse failed for \"{}\" with error: {:?}", trimmed_value, e);
                Err(InvalidValueErr(value_str))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css_properties::SizeMetric;
    #[cfg(feature = "parser")]
    use super::parser::parse;
    #[cfg(feature = "parser")]
    use crate::{debug, CssProperty, parser::parse_css_property_value};

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_min_width_valid() {
        let mut debug_logs = Some(Vec::new());
        assert_eq!(parse("100px", &mut debug_logs), Ok(LayoutMinWidth::px(100.0)));
        assert_eq!(parse("50%", &mut debug_logs), Ok(LayoutMinWidth::percent(50.0)));
        assert_eq!(parse("  25em  ", &mut debug_logs), Ok(LayoutMinWidth::em(25.0)));

        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("min-width: parsing \"100px\""));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_min_width_invalid_unit() {
        let mut debug_logs = Some(Vec::new());
        assert!(parse("100parsecs", &mut debug_logs).is_err());
        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("min-width: parse failed for \"100parsecs\""));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_min_width_empty() {
        let mut debug_logs = Some(Vec::new());
        assert!(parse("", &mut debug_logs).is_err());
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_min_width_auto_direct_fail() {
        let mut debug_logs = Some(Vec::new());
        assert!(parse("auto", &mut debug_logs).is_err());
        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("min-width: parse failed for \"auto\""));
    }

    #[test]
    fn test_layout_min_width_default() {
        assert_eq!(LayoutMinWidth::default(), LayoutMinWidth::px(0.0));
    }

    #[test]
    fn test_min_width_display_format() {
        assert_eq!(format!("{}", LayoutMinWidth::em(12.5)), "12.5pt");
    }

    #[test]
    fn test_print_as_css_value() {
        assert_eq!(LayoutMinWidth::px(10.0).print_as_css_value(), "10px");
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_min_width_value_keywords() {
        let mut debug_logs = Some(Vec::new());

        let res_auto = parse_css_property_value("min-width", "auto", &mut debug_logs);
        assert_eq!(res_auto, Ok(CssProperty::MinWidth(CssPropertyValue::Auto)));

        let res_initial = parse_css_property_value("min-width", "initial", &mut debug_logs);
        assert_eq!(res_initial, Ok(CssProperty::MinWidth(CssPropertyValue::Initial)));

        let res_inherit = parse_css_property_value("min-width", "inherit", &mut debug_logs);
        assert_eq!(res_inherit, Ok(CssProperty::MinWidth(CssPropertyValue::Inherit)));

        let res_px = parse_css_property_value("min-width", "200px", &mut debug_logs);
        assert_eq!(res_px, Ok(CssProperty::MinWidth(CssPropertyValue::Exact(LayoutMinWidth::px(200.0)))));
    }
}
