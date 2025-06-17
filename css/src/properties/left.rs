//! CSS `left` property

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

/// Represents a `left` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutLeft {
    pub inner: PixelValue,
}

crate::impl_pixel_value!(LayoutLeft);

impl PrintAsCssValue for LayoutLeft {
    fn print_as_css_value(&self) -> String {
        alloc::format!("{}", self.inner)
    }
}

/// Typedef for `CssPropertyValue<LayoutLeft>`.
pub type LayoutLeftValue = CssPropertyValue<LayoutLeft>;

crate::impl_option!(
    LayoutLeftValue,
    OptionLayoutLeftValue,
    copy = false, // CssPropertyValue is not Copy
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;

    typed_pixel_value_parser!(parse_left_inner, LayoutLeft);

    impl FormatAsCssValue for LayoutLeft {
        fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
            self.inner.format_as_css_value(f)
        }
    }

    /// Parses the `left` CSS property.
    pub fn parse<'a>(value_str: &'a str, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> Result<LayoutLeft, InvalidValueErr<'a>> {
        css_debug_log!(debug_messages, "left: parsing \"{}\"", value_str);
        let trimmed_value = value_str.trim();

        if trimmed_value.eq_ignore_ascii_case("auto") {
            css_debug_log!(debug_messages, "left: parse failed for \"{}\", 'auto' not directly convertible to PixelValue here for LayoutLeft", trimmed_value);
            return Err(InvalidValueErr(value_str));
        }

        match parse_left_inner(trimmed_value) {
            Ok(val) => Ok(val),
            Err(e) => {
                css_debug_log!(debug_messages, "left: parse failed for \"{}\" with error: {:?}", trimmed_value, e);
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
    fn test_parse_left() {
        let mut logs = Some(Vec::new());
        assert_eq!(parser::parse("10px", &mut logs), Ok(LayoutLeft::px(10.0)));
        assert_eq!(parser::parse("50%", &mut logs), Ok(LayoutLeft::percent(50.0)));
        assert!(parser::parse("auto", &mut logs).is_err());
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_left_keywords() {
        let mut debug_logs = Some(Vec::new());

        let res_auto = parse_css_property_value("left", "auto", &mut debug_logs);
        assert_eq!(res_auto, Ok(CssProperty::Left(CssPropertyValue::Auto)));

        let res_initial = parse_css_property_value("left", "initial", &mut debug_logs);
        assert_eq!(res_initial, Ok(CssProperty::Left(CssPropertyValue::Initial)));

        let res_inherit = parse_css_property_value("left", "inherit", &mut debug_logs);
        assert_eq!(res_inherit, Ok(CssProperty::Left(CssPropertyValue::Inherit)));

        let res_px = parse_css_property_value("left", "20px", &mut debug_logs);
        assert_eq!(res_px, Ok(CssProperty::Left(CssPropertyValue::Exact(LayoutLeft::px(20.0)))));
    }
}
