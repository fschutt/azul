//! CSS `max-height` property

use crate::css::{CssPropertyValue, PrintAsCssValue};
use crate::css_properties::{PixelValue, SizeMetric}; // Assuming PixelValue and SizeMetric are here or pub use from common
#[cfg(feature = "parser")]
use crate::parser::{CssPixelValueParseError, FormatAsCssValue, typed_pixel_value_parser};
use core::fmt;
#[cfg(feature = "parser")]
use crate::css_debug_log; // Corrected import
#[cfg(feature = "parser")]
use crate::{LayoutDebugMessage, parser::InvalidValueErr};
#[cfg(feature = "parser")]
use alloc::vec::Vec;

/// Represents a `max-height` attribute
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutMaxHeight {
    pub inner: PixelValue,
}

crate::impl_pixel_value!(LayoutMaxHeight);

impl Default for LayoutMaxHeight {
    fn default() -> Self {
        // As per CSS spec, default for max-height is "none", which acts as no limit.
        // Representing "none" with a very large pixel value or a specific enum variant
        // can be a design choice. Here, using f32::MAX for "none".
        // Note: In cssparser, "none" for max-height is parsed as Percentage(1.0) for NonNegativeLengthPercentage::MaxContent
        // which might imply a different handling or that "none" is not a simple PixelValue.
        // For now, following the pattern of LayoutMaxWidth if it uses f32::MAX.
        // If "none" should be distinct, LayoutMaxHeight might need to be an enum { None, Exact(PixelValue) }
        Self {
            inner: PixelValue::px(core::f32::MAX), // Represents "none" (no limit)
        }
    }
}

impl PrintAsCssValue for LayoutMaxHeight {
    fn print_as_css_value(&self) -> String {
        if self.inner.metric == SizeMetric::Px && self.inner.number.get() == core::f32::MAX {
            "none".into()
        } else {
            alloc::format!("{}", self.inner)
        }
    }
}

pub type LayoutMaxHeightValue = CssPropertyValue<LayoutMaxHeight>;

crate::impl_option!(
    LayoutMaxHeightValue,
    OptionLayoutMaxHeightValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;

    typed_pixel_value_parser!(parse_max_height_inner, LayoutMaxHeight);

    impl FormatAsCssValue for LayoutMaxHeight {
        fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
            if self.inner.metric == SizeMetric::Px && self.inner.number.get() == core::f32::MAX {
                write!(f, "none")
            } else {
                self.inner.format_as_css_value(f)
            }
        }
    }

    pub fn parse<'a>(value_str: &'a str, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> Result<LayoutMaxHeight, InvalidValueErr<'a>> {
        css_debug_log!(debug_messages, "max-height: parsing \"{}\"", value_str);
        let trimmed_value = value_str.trim();
        if trimmed_value.eq_ignore_ascii_case("none") {
             return Ok(LayoutMaxHeight::default());
        }
        match parse_max_height_inner(trimmed_value) {
            Ok(val) => Ok(val),
            Err(e) => {
                css_debug_log!(debug_messages, "max-height: parse failed for \"{}\" with error: {:?}", trimmed_value, e);
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
    fn test_parse_max_height() {
        let mut logs = Some(Vec::new());
        assert_eq!(parser::parse("300px", &mut logs), Ok(LayoutMaxHeight::px(300.0)));
        assert_eq!(parser::parse("none", &mut logs), Ok(LayoutMaxHeight::default()));
        assert!(parser::parse("auto", &mut logs).is_err()); // auto is invalid for max-height
    }
}
