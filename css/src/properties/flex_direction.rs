//! CSS `flex-direction` property

use crate::css::{CssPropertyValue, PrintAsCssValue};
#[cfg(feature = "parser")]
use crate::parser::{InvalidValueErr, FormatAsCssValue, multi_type_parser};
use core::fmt;

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

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;
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
    pub fn parse<'a>(value_str: &'a str) -> Result<LayoutFlexDirection, InvalidValueErr<'a>> {
        css_debug_log!("flex-direction: parsing \"{}\"", value_str);
        let trimmed_value = value_str.trim();
        let result = parse_impl(trimmed_value);
        if result.is_err() {
            css_debug_log!("flex-direction: parse failed for \"{}\"", trimmed_value);
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

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_flex_direction_valid() {
        crate::debug::clear_debug_logs();
        assert_eq!(parse("row"), Ok(LayoutFlexDirection::Row));
        assert_eq!(parse("row-reverse"), Ok(LayoutFlexDirection::RowReverse));
        assert_eq!(parse("column"), Ok(LayoutFlexDirection::Column));
        assert_eq!(parse("column-reverse"), Ok(LayoutFlexDirection::ColumnReverse));
        assert_eq!(parse("  row  "), Ok(LayoutFlexDirection::Row));
        let logs = crate::debug::get_debug_logs();
        assert!(logs.iter().any(|log| log.contains("flex-direction: parsing \"row\"")));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_flex_direction_invalid() {
        crate::debug::clear_debug_logs();
        assert!(parse("col").is_err());
        assert!(parse("").is_err());
        let logs = crate::debug::get_debug_logs();
        assert!(logs.iter().any(|log| log.contains("flex-direction: parse failed for \"col\"")));
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
