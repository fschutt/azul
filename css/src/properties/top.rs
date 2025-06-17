//! CSS `top` property

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

/// Represents a `top` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutTop {
    pub inner: PixelValue,
}

crate::impl_pixel_value!(LayoutTop);

impl PrintAsCssValue for LayoutTop {
    fn print_as_css_value(&self) -> String {
        alloc::format!("{}", self.inner)
    }
}

/// Typedef for `CssPropertyValue<LayoutTop>`.
pub type LayoutTopValue = CssPropertyValue<LayoutTop>;

crate::impl_option!(
    LayoutTopValue,
    OptionLayoutTopValue,
    copy = false, // CssPropertyValue is not Copy
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;

    typed_pixel_value_parser!(parse_top_inner, LayoutTop);

    impl FormatAsCssValue for LayoutTop {
        fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
            self.inner.format_as_css_value(f)
        }
    }

    /// Parses the `top` CSS property.
    pub fn parse<'a>(value_str: &'a str, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> Result<LayoutTop, InvalidValueErr<'a>> {
        css_debug_log!(debug_messages, "top: parsing \"{}\"", value_str);
        let trimmed_value = value_str.trim();

        if trimmed_value.eq_ignore_ascii_case("auto") {
            css_debug_log!(debug_messages, "top: parse failed for \"{}\", 'auto' not directly convertible to PixelValue here for LayoutTop", trimmed_value);
            return Err(InvalidValueErr(value_str));
        }

        match parse_top_inner(trimmed_value) {
            Ok(val) => Ok(val),
            Err(e) => {
                css_debug_log!(debug_messages, "top: parse failed for \"{}\" with error: {:?}", trimmed_value, e);
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
    use crate::{debug, CssProperty, parser::parse_css_property_value};

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_top() {
        let mut logs = Some(Vec::new());
        assert_eq!(parser::parse("10px", &mut logs), Ok(LayoutTop::px(10.0)));
        assert_eq!(parser::parse("50%", &mut logs), Ok(LayoutTop::percent(50.0)));
        assert!(parser::parse("auto", &mut logs).is_err());
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_top_keywords() {
        let mut debug_logs = Some(Vec::new());

        let res_auto = parse_css_property_value("top", "auto", &mut debug_logs);
        assert_eq!(res_auto, Ok(CssProperty::Top(CssPropertyValue::Auto)));

        let res_initial = parse_css_property_value("top", "initial", &mut debug_logs);
        assert_eq!(res_initial, Ok(CssProperty::Top(CssPropertyValue::Initial)));

        let res_inherit = parse_css_property_value("top", "inherit", &mut debug_logs);
        assert_eq!(res_inherit, Ok(CssProperty::Top(CssPropertyValue::Inherit)));

        let res_px = parse_css_property_value("top", "20px", &mut debug_logs);
        assert_eq!(res_px, Ok(CssProperty::Top(CssPropertyValue::Exact(LayoutTop::px(20.0)))));
    }
}
