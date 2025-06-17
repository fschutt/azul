//! CSS `flex-direction` property

use crate::css::{CssPropertyValue, PrintAsCssValue};
#[cfg(feature = "parser")]
use crate::parser::{InvalidValueErr, FormatAsCssValue, multi_type_parser};
use core::fmt;
#[cfg(feature = "parser")]
use crate::css_debug_log; // Moved to file level
#[cfg(feature = "parser")]
use crate::LayoutDebugMessage; // Moved to file level
#[cfg(feature = "parser")]
use alloc::vec::Vec;           // Moved to file level

/// Specifies how flex items are placed in the flex container defining the main axis and the direction (normal or reversed).
///
/// [MDN Reference](https://developer.mozilla.org/en-US/docs/Web/CSS/flex-direction)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutFlexDirection {
    /// Main axis is the same as the block axis. (Default value)
    Row,
    /// Main axis is the same as the block axis, but elements are placed in reverse order.
    RowReverse,
    /// Main axis is the same as the inline axis.
    Column,
    /// Main axis is the same as the inline axis, but elements are placed in reverse order.
    ColumnReverse,
}

impl Default for LayoutFlexDirection {
    /// The CSS default for `flex-direction` is `row`.
    /// However, for historical reasons or specific library needs, this implementation defaults to `Column`.
    /// This should be documented if it causes confusion with standard CSS behavior.
    fn default() -> Self {
        LayoutFlexDirection::Column
    }
}

/// Represents the main layout axis for flexbox, derived from `LayoutFlexDirection`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutAxis {
    Horizontal,
    Vertical,
}

impl LayoutFlexDirection {
    /// Returns the main axis (horizontal or vertical) for this flex direction.
    pub fn get_axis(&self) -> LayoutAxis {
        use self::{LayoutAxis::*, LayoutFlexDirection::*};
        match self {
            Row | RowReverse => Horizontal,
            Column | ColumnReverse => Vertical,
        }
    }

    /// Returns `true` if this direction is a `*-reverse` direction.
    pub fn is_reverse(&self) -> bool {
        *self == LayoutFlexDirection::RowReverse || *self == LayoutFlexDirection::ColumnReverse
    }
}

impl PrintAsCssValue for LayoutFlexDirection {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

impl fmt::Display for LayoutFlexDirection {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LayoutFlexDirection::Row => write!(f, "row"),
            LayoutFlexDirection::RowReverse => write!(f, "row-reverse"),
            LayoutFlexDirection::Column => write!(f, "column"),
            LayoutFlexDirection::ColumnReverse => write!(f, "column-reverse"),
        }
    }
}

/// Typedef for `CssPropertyValue<LayoutFlexDirection>`.
pub type LayoutFlexDirectionValue = CssPropertyValue<LayoutFlexDirection>;

/// Optional `LayoutFlexDirectionValue`.
pub type OptionLayoutFlexDirectionValue = Option<LayoutFlexDirectionValue>;

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;
    // use crate::css_debug_log; // Now at file level
    // use crate::LayoutDebugMessage; // Now at file level
    // use alloc::vec::Vec;           // Now at file level
    // Original function was parse_layout_direction
    multi_type_parser!(
        parse_impl, // internal name
        LayoutFlexDirection,
        ["row", Row],
        ["row-reverse", RowReverse],
        ["column", Column],
        ["column-reverse", ColumnReverse]
    );

    impl FormatAsCssValue for LayoutFlexDirection {
        fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
            fmt::Display::fmt(self, f)
        }
    }

    /// Parses the `flex-direction` CSS property.
    pub fn parse<'a>(value_str: &'a str, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> Result<LayoutFlexDirection, InvalidValueErr<'a>> {
        css_debug_log!(debug_messages, "flex-direction: parsing \"{}\"", value_str); // Use non-crate prefixed
        let trimmed_value = value_str.trim();
        let result = parse_impl(trimmed_value);
        if result.is_err() {
            css_debug_log!(debug_messages, "flex-direction: parse failed for \"{}\"", trimmed_value); // Use non-crate prefixed
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
    // Note: LayoutDebugMessage and Vec will be in scope due to file-level imports if parser feature is active.

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_flex_direction_valid() {
        let mut debug_logs = Some(Vec::new());
        assert_eq!(parse("row", &mut debug_logs), Ok(LayoutFlexDirection::Row));
        assert_eq!(parse("row-reverse", &mut debug_logs), Ok(LayoutFlexDirection::RowReverse));
        assert_eq!(parse("column", &mut debug_logs), Ok(LayoutFlexDirection::Column));
        assert_eq!(parse("column-reverse", &mut debug_logs), Ok(LayoutFlexDirection::ColumnReverse));
        assert_eq!(parse("  row  ", &mut debug_logs), Ok(LayoutFlexDirection::Row));
        // It's tricky to assert debug_log content here without a public getter that takes the Option
        // For now, ensuring parse works is the main goal. If crate::debug::get_debug_logs existed and worked with Option, it would be:
        // let logs = crate::debug::format_debug_logs(&debug_logs);
        // assert!(logs.contains("flex-direction: parsing \"row\""));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_flex_direction_invalid() {
        let mut debug_logs = Some(Vec::new());
        assert!(parse("col", &mut debug_logs).is_err());
        assert!(parse("", &mut debug_logs).is_err());
        // let logs = crate::debug::format_debug_logs(&debug_logs);
        // assert!(logs.contains("flex-direction: parse failed for \"col\""));
    }

    #[test]
    fn test_layout_flex_direction_default() {
        // Note: CSS default is "row", but this impl defaults to "column"
        assert_eq!(LayoutFlexDirection::default(), LayoutFlexDirection::Column);
    }

    #[test]
    fn test_get_axis() {
        assert_eq!(LayoutFlexDirection::Row.get_axis(), LayoutAxis::Horizontal);
        assert_eq!(LayoutFlexDirection::RowReverse.get_axis(), LayoutAxis::Horizontal);
        assert_eq!(LayoutFlexDirection::Column.get_axis(), LayoutAxis::Vertical);
        assert_eq!(LayoutFlexDirection::ColumnReverse.get_axis(), LayoutAxis::Vertical);
    }

    #[test]
    fn test_is_reverse() {
        assert!(!LayoutFlexDirection::Row.is_reverse());
        assert!(LayoutFlexDirection::RowReverse.is_reverse());
        assert!(!LayoutFlexDirection::Column.is_reverse());
        assert!(LayoutFlexDirection::ColumnReverse.is_reverse());
    }

    #[test]
    fn test_display_format_flex_direction() {
        assert_eq!(format!("{}", LayoutFlexDirection::RowReverse), "row-reverse");
    }
}
