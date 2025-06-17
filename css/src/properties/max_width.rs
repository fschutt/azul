//! CSS `max-width` property

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

/// Represents a `max-width` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMaxWidth {
    pub inner: PixelValue,
}

impl Default for LayoutMaxWidth {
    fn default() -> Self {
        Self {
            // Corresponds to "none" in CSS, effectively no limit.
            inner: PixelValue::px(core::f32::MAX),
        }
    }
}

crate::impl_pixel_value!(LayoutMaxWidth);

impl PrintAsCssValue for LayoutMaxWidth {
    fn print_as_css_value(&self) -> String {
        if self.inner == PixelValue::px(core::f32::MAX) { // A bit of a hack to represent "none"
            "none".into()
        } else {
            alloc::format!("{}", self.inner)
        }
    }
}

/// Typedef for `CssPropertyValue<LayoutMaxWidth>`.
pub type LayoutMaxWidthValue = CssPropertyValue<LayoutMaxWidth>;

crate::impl_option!(
    LayoutMaxWidthValue,
    OptionLayoutMaxWidthValue,
    copy = false, // CssPropertyValue is not Copy
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;

    typed_pixel_value_parser!(parse_max_width_inner, LayoutMaxWidth);

    impl FormatAsCssValue for LayoutMaxWidth {
        fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
            if self.inner == PixelValue::px(core::f32::MAX) {
                write!(f, "none")
            } else {
                self.inner.format_as_css_value(f)
            }
        }
    }

    /// Parses the `max-width` CSS property.
    pub fn parse<'a>(value_str: &'a str, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> Result<LayoutMaxWidth, InvalidValueErr<'a>> {
        css_debug_log!(debug_messages, "max-width: parsing \"{}\"", value_str);
        let trimmed_value = value_str.trim();

        if trimmed_value.eq_ignore_ascii_case("none") {
            css_debug_log!(debug_messages, "max-width: parsed \"none\"");
            return Ok(LayoutMaxWidth::default());
        }
        // "auto" is not a valid value for max-width, unlike width.
        // It behaves like "none".
        if trimmed_value.eq_ignore_ascii_case("auto") {
             css_debug_log!(debug_messages, "max-width: parsed \"auto\", treating as \"none\"");
            return Ok(LayoutMaxWidth::default());
        }

        match parse_max_width_inner(trimmed_value) {
            Ok(val) => Ok(val),
            Err(e) => {
                css_debug_log!(debug_messages, "max-width: parse failed for \"{}\" with error: {:?}", trimmed_value, e);
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
    fn test_parse_max_width_valid() {
        let mut debug_logs = Some(Vec::new());
        assert_eq!(parse("100px", &mut debug_logs), Ok(LayoutMaxWidth::px(100.0)));
        assert_eq!(parse("50%", &mut debug_logs), Ok(LayoutMaxWidth::percent(50.0)));
        assert_eq!(parse("none", &mut debug_logs), Ok(LayoutMaxWidth::default()));
        assert_eq!(parse("auto", &mut debug_logs), Ok(LayoutMaxWidth::default())); // "auto" behaves as "none"

        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("max-width: parsing \"100px\""));
        assert!(logs_str.contains("max-width: parsed \"none\""));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_max_width_invalid_unit() {
        let mut debug_logs = Some(Vec::new());
        assert!(parse("100parsecs", &mut debug_logs).is_err());
    }

    #[test]
    fn test_layout_max_width_default() {
        assert_eq!(LayoutMaxWidth::default(), LayoutMaxWidth { inner: PixelValue::px(core::f32::MAX) });
    }

    #[test]
    fn test_max_width_display_format() {
        assert_eq!(format!("{}", LayoutMaxWidth::em(12.5)), "12.5pt");
        assert_eq!(format!("{}", LayoutMaxWidth::default()), "none");
    }

    #[test]
    fn test_print_as_css_value() {
        assert_eq!(LayoutMaxWidth::px(10.0).print_as_css_value(), "10px");
        assert_eq!(LayoutMaxWidth::default().print_as_css_value(), "none");
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_max_width_value_keywords() {
        let mut debug_logs = Some(Vec::new());

        let res_none = parse_css_property_value("max-width", "none", &mut debug_logs);
        assert_eq!(res_none, Ok(CssProperty::MaxWidth(CssPropertyValue::Exact(LayoutMaxWidth::default()))));

        // "auto" for max-width is treated as "none". CssPropertyValue itself doesn't have "NoneValue"
        // so it should resolve to the Exact(default) which is how "none" is represented.
        let res_auto = parse_css_property_value("max-width", "auto", &mut debug_logs);
         assert_eq!(res_auto, Ok(CssProperty::MaxWidth(CssPropertyValue::Exact(LayoutMaxWidth::default()))));

        let res_initial = parse_css_property_value("max-width", "initial", &mut debug_logs);
        assert_eq!(res_initial, Ok(CssProperty::MaxWidth(CssPropertyValue::Initial)));

        let res_inherit = parse_css_property_value("max-width", "inherit", &mut debug_logs);
        assert_eq!(res_inherit, Ok(CssProperty::MaxWidth(CssPropertyValue::Inherit)));

        let res_px = parse_css_property_value("max-width", "200px", &mut debug_logs);
        assert_eq!(res_px, Ok(CssProperty::MaxWidth(CssPropertyValue::Exact(LayoutMaxWidth::px(200.0)))));
    }
}
