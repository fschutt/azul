//! CSS `width` property

use crate::css::{CssPropertyValue, PrintAsCssValue};
use crate::css_properties::PixelValue; // Already defines PixelValue
#[cfg(feature = "parser")]
use crate::parser::{CssPixelValueParseError, FormatAsCssValue, typed_pixel_value_parser};
use core::fmt;
#[cfg(feature = "parser")]
use crate::css_debug_log;
#[cfg(feature = "parser")]
use crate::{LayoutDebugMessage, parser::InvalidValueErr};
#[cfg(feature = "parser")]
use alloc::vec::Vec;

/// Represents a `width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutWidth {
    pub inner: PixelValue,
}

// Use the existing macro from css_properties.rs (or move it to a common place if needed)
// For now, assuming it's accessible or will be made accessible.
// If not, the content of impl_pixel_value! would be replicated here.
crate::impl_pixel_value!(LayoutWidth);

impl PrintAsCssValue for LayoutWidth {
    fn print_as_css_value(&self) -> String {
        alloc::format!("{}", self.inner)
    }
}

/// Typedef for `CssPropertyValue<LayoutWidth>`.
pub type LayoutWidthValue = CssPropertyValue<LayoutWidth>;

crate::impl_option!(
    LayoutWidthValue,
    OptionLayoutWidthValue,
    copy = false, // CssPropertyValue is not Copy
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;

    // Use the existing typed_pixel_value_parser macro
    typed_pixel_value_parser!(parse_width_inner, LayoutWidth);

    impl FormatAsCssValue for LayoutWidth {
        fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
            self.inner.format_as_css_value(f)
        }
    }

    /// Parses the `width` CSS property.
    pub fn parse<'a>(value_str: &'a str, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> Result<LayoutWidth, InvalidValueErr<'a>> {
        css_debug_log!(debug_messages, "width: parsing \"{}\"", value_str);
        let trimmed_value = value_str.trim();

        // Check for "auto" before attempting pixel parsing
        if trimmed_value.eq_ignore_ascii_case("auto") {
             // "auto" for width would typically resolve to a different mechanism
             // or be represented by a specific variant if LayoutWidth could be other than PixelValue.
             // For now, if "auto" needs to be an Exact(PixelValue(...)), it needs a concrete pixel mapping or error.
             // Assuming "auto" is not directly parsable into a simple LayoutWidth { inner: PixelValue }
             // without more context on how "auto" should be resolved to pixels.
             // Let's treat "auto" as an invalid direct value for LayoutWidth for now,
             // as CssPropertyValue handles "auto" at a higher level.
            css_debug_log!(debug_messages, "width: parse failed for \"{}\", 'auto' not directly convertible to PixelValue here", trimmed_value);
            return Err(InvalidValueErr(value_str));
        }

        match parse_width_inner(trimmed_value) {
            Ok(val) => Ok(val),
            Err(e) => {
                css_debug_log!(debug_messages, "width: parse failed for \"{}\" with error: {:?}", trimmed_value, e);
                Err(InvalidValueErr(value_str)) // Convert error type
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
    fn test_parse_width_valid() {
        let mut debug_logs = Some(Vec::new());
        assert_eq!(parse("100px", &mut debug_logs), Ok(LayoutWidth::px(100.0)));
        assert_eq!(parse("50%", &mut debug_logs), Ok(LayoutWidth::percent(50.0)));
        assert_eq!(parse("  25em  ", &mut debug_logs), Ok(LayoutWidth::em(25.0)));

        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("width: parsing \"100px\""));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_width_invalid_unit() {
        let mut debug_logs = Some(Vec::new());
        assert!(parse("100parsecs", &mut debug_logs).is_err());
        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("width: parse failed for \"100parsecs\""));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_width_empty() {
        let mut debug_logs = Some(Vec::new());
        assert!(parse("", &mut debug_logs).is_err());
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_width_auto_direct_fail() {
        // Direct parsing of "auto" into LayoutWidth is expected to fail
        // as LayoutWidth only holds PixelValue. "auto" is handled by CssPropertyValue.
        let mut debug_logs = Some(Vec::new());
        assert!(parse("auto", &mut debug_logs).is_err());
        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("width: parse failed for \"auto\""));
    }

    #[test]
    fn test_layout_width_default() {
        // Default for PixelValue-based structs is 0px usually
        assert_eq!(LayoutWidth::default(), LayoutWidth::px(0.0));
    }

    #[test]
    fn test_width_display_format() {
        assert_eq!(format!("{}", LayoutWidth::em(12.5)), "12.5pt"); // Note: impl_pixel_value uses "pt" for "em" in display
    }

    #[test]
    fn test_print_as_css_value() {
        assert_eq!(LayoutWidth::px(10.0).print_as_css_value(), "10px");
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_width_value_keywords() {
        let mut debug_logs = Some(Vec::new());

        let res_auto = parse_css_property_value("width", "auto", &mut debug_logs);
        assert_eq!(res_auto, Ok(CssProperty::Width(CssPropertyValue::Auto)));

        let res_initial = parse_css_property_value("width", "initial", &mut debug_logs);
        assert_eq!(res_initial, Ok(CssProperty::Width(CssPropertyValue::Initial)));

        let res_inherit = parse_css_property_value("width", "inherit", &mut debug_logs);
        assert_eq!(res_inherit, Ok(CssProperty::Width(CssPropertyValue::Inherit)));

        let res_px = parse_css_property_value("width", "200px", &mut debug_logs);
        assert_eq!(res_px, Ok(CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::px(200.0)))));
    }
}
