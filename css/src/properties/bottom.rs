//! CSS `bottom` property

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

/// Represents a `bottom` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBottom {
    pub inner: PixelValue,
}

crate::impl_pixel_value!(LayoutBottom);

impl PrintAsCssValue for LayoutBottom {
    fn print_as_css_value(&self) -> String {
        alloc::format!("{}", self.inner)
    }
}

/// Typedef for `CssPropertyValue<LayoutBottom>`.
pub type LayoutBottomValue = CssPropertyValue<LayoutBottom>;

crate::impl_option!(
    LayoutBottomValue,
    OptionLayoutBottomValue,
    copy = false, // CssPropertyValue is not Copy
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;

    typed_pixel_value_parser!(parse_bottom_inner, LayoutBottom);

    impl FormatAsCssValue for LayoutBottom {
        fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
            self.inner.format_as_css_value(f)
        }
    }

    /// Parses the `bottom` CSS property.
    pub fn parse<'a>(value_str: &'a str, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> Result<LayoutBottom, InvalidValueErr<'a>> {
        css_debug_log!(debug_messages, "bottom: parsing \"{}\"", value_str);
        let trimmed_value = value_str.trim();

        if trimmed_value.eq_ignore_ascii_case("auto") {
            css_debug_log!(debug_messages, "bottom: parse failed for \"{}\", 'auto' not directly convertible to PixelValue here for LayoutBottom", trimmed_value);
            return Err(InvalidValueErr(value_str));
        }

        match parse_bottom_inner(trimmed_value) {
            Ok(val) => Ok(val),
            Err(e) => {
                css_debug_log!(debug_messages, "bottom: parse failed for \"{}\" with error: {:?}", trimmed_value, e);
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
    fn test_parse_bottom() {
        let mut logs = Some(Vec::new());
        assert_eq!(parser::parse("10px", &mut logs), Ok(LayoutBottom::px(10.0)));
        assert_eq!(parser::parse("50%", &mut logs), Ok(LayoutBottom::percent(50.0)));
        assert!(parser::parse("auto", &mut logs).is_err());
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_bottom_keywords() {
        let mut debug_logs = Some(Vec::new());

        let res_auto = parse_css_property_value("bottom", "auto", &mut debug_logs);
        assert_eq!(res_auto, Ok(CssProperty::Bottom(CssPropertyValue::Auto)));

        let res_initial = parse_css_property_value("bottom", "initial", &mut debug_logs);
        assert_eq!(res_initial, Ok(CssProperty::Bottom(CssPropertyValue::Initial)));

        let res_inherit = parse_css_property_value("bottom", "inherit", &mut debug_logs);
        assert_eq!(res_inherit, Ok(CssProperty::Bottom(CssPropertyValue::Inherit)));

        let res_px = parse_css_property_value("bottom", "20px", &mut debug_logs);
        assert_eq!(res_px, Ok(CssProperty::Bottom(CssPropertyValue::Exact(LayoutBottom::px(20.0)))));
    }
}
