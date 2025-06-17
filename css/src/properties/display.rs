//! CSS `display` property

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

/// Represents a `display` CSS property value, determining how an element is rendered.
///
/// [MDN Reference](https://developer.mozilla.org/en-US/docs/Web/CSS/display)
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutDisplay {
    /// The element generates no boxes.
    None,
    /// The element generates a block box. (Default value)
    #[default]
    Block,
    /// The element generates an inline box.
    Inline,
    /// The element generates a block box that is flowed with surrounding content as if it were a single inline box.
    InlineBlock,
    /// The element behaves like a block element and lays out its content according to the flexbox model.
    Flex,
    /// The element behaves like an inline element and lays out its content according to the flexbox model.
    InlineFlex,
    /// The element behaves like a `<table>` HTML element. It defines a block-level box.
    Table,
    /// The element behaves like a `<table>` HTML element, but as an inline box.
    InlineTable,
    /// The element behaves like a `<tbody>` HTML element.
    TableRowGroup,
    /// The element behaves like a `<thead>` HTML element.
    TableHeaderGroup,
    /// The element behaves like a `<tfoot>` HTML element.
    TableFooterGroup,
    /// The element behaves like a `<tr>` HTML element.
    TableRow,
    /// The element behaves like a `<colgroup>` HTML element.
    TableColumnGroup,
    /// The element behaves like a `<col>` HTML element.
    TableColumn,
    /// The element behaves like a `<td>` HTML element.
    TableCell,
    /// The element behaves like a `<caption>` HTML element.
    TableCaption,
    /// The element generates a block box for the content and a separate list-item inline box.
    ListItem,
    /// The element generates a run-in box. Run-in elements act like inlines or blocks, depending on the surrounding elements.
    RunIn,
    /// The element itself does not generate a box, but its ::marker pseudo-element does.
    Marker,
    /// The element behaves like a block element and lays out its content according to the grid model.
    Grid,
    /// The element behaves like an inline element and lays out its content according to the grid model.
    InlineGrid,
    // NOTE: `Initial` and `Inherit` are handled by `CssPropertyValue` wrapper, not directly as enum variants here.
}

impl LayoutDisplay {
    /// Returns whether this display type creates a block formatting context.
    /// A block formatting context is a part of a visual CSS rendering of a Web page.
    /// It's the region in which the layout of block boxes occurs and in which floats interact with other elements.
    pub fn creates_block_context(&self) -> bool {
        matches!(
            self,
            LayoutDisplay::Block
                | LayoutDisplay::Flex
                | LayoutDisplay::Grid
                | LayoutDisplay::Table
                | LayoutDisplay::ListItem
        )
    }

    /// Returns whether this display type creates a flex formatting context.
    /// A flex formatting context is established by elements with `display: flex` or `display: inline-flex`.
    /// Inside a flex formatting context, children are laid out using the flex layout model.
    pub fn creates_flex_context(&self) -> bool {
        matches!(self, LayoutDisplay::Flex | LayoutDisplay::InlineFlex)
    }

    /// Returns whether this display type creates a table formatting context.
    /// A table formatting context is established by elements with `display: table` or `display: inline-table`.
    /// Inside a table formatting context, children are laid out using the table layout model.
    pub fn creates_table_context(&self) -> bool {
        matches!(self, LayoutDisplay::Table | LayoutDisplay::InlineTable)
    }

    /// Returns whether this is an inline-level display type.
    /// Inline-level elements do not start on new lines and only occupy as much width as necessary.
    pub fn is_inline_level(&self) -> bool {
        matches!(
            self,
            LayoutDisplay::Inline
                | LayoutDisplay::InlineBlock
                | LayoutDisplay::InlineFlex
                | LayoutDisplay::InlineTable
                | LayoutDisplay::InlineGrid
        )
    }
}

impl PrintAsCssValue for LayoutDisplay {
    fn print_as_css_value(&self) -> String {
        alloc::format!("{}", self)
    }
}

impl fmt::Display for LayoutDisplay {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LayoutDisplay::None => write!(f, "none"),
            LayoutDisplay::Block => write!(f, "block"),
            LayoutDisplay::Inline => write!(f, "inline"),
            LayoutDisplay::InlineBlock => write!(f, "inline-block"),
            LayoutDisplay::Flex => write!(f, "flex"),
            LayoutDisplay::InlineFlex => write!(f, "inline-flex"),
            LayoutDisplay::Table => write!(f, "table"),
            LayoutDisplay::InlineTable => write!(f, "inline-table"),
            LayoutDisplay::TableRowGroup => write!(f, "table-row-group"),
            LayoutDisplay::TableHeaderGroup => write!(f, "table-header-group"),
            LayoutDisplay::TableFooterGroup => write!(f, "table-footer-group"),
            LayoutDisplay::TableRow => write!(f, "table-row"),
            LayoutDisplay::TableColumnGroup => write!(f, "table-column-group"),
            LayoutDisplay::TableColumn => write!(f, "table-column"),
            LayoutDisplay::TableCell => write!(f, "table-cell"),
            LayoutDisplay::TableCaption => write!(f, "table-caption"),
            LayoutDisplay::ListItem => write!(f, "list-item"),
            LayoutDisplay::RunIn => write!(f, "run-in"),
            LayoutDisplay::Marker => write!(f, "marker"),
            LayoutDisplay::Grid => write!(f, "grid"),
            LayoutDisplay::InlineGrid => write!(f, "inline-grid"),
        }
    }
}

/// Typedef for `CssPropertyValue<LayoutDisplay>`.
pub type LayoutDisplayValue = CssPropertyValue<LayoutDisplay>;

crate::impl_option!(
    LayoutDisplayValue,
    OptionLayoutDisplayValue,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

#[cfg(feature = "parser")]
pub mod parser {
    use super::*;

    multi_type_parser!(
        parse_impl,
        LayoutDisplay,
        ["none", LayoutDisplay::None],
        ["block", LayoutDisplay::Block],
        ["inline", LayoutDisplay::Inline],
        ["inline-block", LayoutDisplay::InlineBlock],
        ["flex", LayoutDisplay::Flex],
        ["inline-flex", LayoutDisplay::InlineFlex],
        ["table", LayoutDisplay::Table],
        ["inline-table", LayoutDisplay::InlineTable],
        ["table-row-group", LayoutDisplay::TableRowGroup],
        ["table-header-group", LayoutDisplay::TableHeaderGroup],
        ["table-footer-group", LayoutDisplay::TableFooterGroup],
        ["table-row", LayoutDisplay::TableRow],
        ["table-column-group", LayoutDisplay::TableColumnGroup],
        ["table-column", LayoutDisplay::TableColumn],
        ["table-cell", LayoutDisplay::TableCell],
        ["table-caption", LayoutDisplay::TableCaption],
        ["list-item", LayoutDisplay::ListItem],
        ["run-in", LayoutDisplay::RunIn],
        ["marker", LayoutDisplay::Marker],
        ["grid", LayoutDisplay::Grid],
        ["inline-grid", LayoutDisplay::InlineGrid]
        // "initial" and "inherit" are handled by CssPropertyValue parser
    );

    impl FormatAsCssValue for LayoutDisplay {
        fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
            fmt::Display::fmt(self, f)
        }
    }

    /// Parses the `display` CSS property.
    pub fn parse<'a>(value_str: &'a str, debug_messages: &mut Option<Vec<LayoutDebugMessage>>) -> Result<LayoutDisplay, InvalidValueErr<'a>> {
        css_debug_log!(debug_messages, "display: parsing \"{}\"", value_str);
        let trimmed_value = value_str.trim();
        let result = parse_impl(trimmed_value);
        if result.is_err() {
            css_debug_log!(debug_messages, "display: parse failed for \"{}\"", trimmed_value);
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
    fn test_parse_display_valid() {
        let mut debug_logs = Some(Vec::new());
        assert_eq!(parse("block", &mut debug_logs), Ok(LayoutDisplay::Block));
        assert_eq!(parse("inline-flex", &mut debug_logs), Ok(LayoutDisplay::InlineFlex));
        assert_eq!(parse("  table-cell  ", &mut debug_logs), Ok(LayoutDisplay::TableCell));

        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("display: parsing \"block\""));
        assert!(logs_str.contains("display: parsing \"inline-flex\""));
        assert!(logs_str.contains("display: parsing \"  table-cell  \""));
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_display_invalid() {
        let mut debug_logs = Some(Vec::new());
        assert!(parse("unknown-display", &mut debug_logs).is_err());
        assert!(parse("", &mut debug_logs).is_err());

        let logs_str = debug::format_debug_logs(&debug_logs);
        assert!(logs_str.contains("display: parse failed for \"unknown-display\""));
        assert!(logs_str.contains("display: parse failed for \"\""));
    }

    #[test]
    fn test_layout_display_default() {
        assert_eq!(LayoutDisplay::default(), LayoutDisplay::Block);
    }

    #[test]
    fn test_display_methods() {
        assert!(LayoutDisplay::Block.creates_block_context());
        assert!(!LayoutDisplay::Inline.creates_block_context());
        assert!(LayoutDisplay::Flex.creates_flex_context());
        assert!(LayoutDisplay::Table.creates_table_context());
        assert!(LayoutDisplay::Inline.is_inline_level());
        assert!(!LayoutDisplay::Block.is_inline_level());
    }

    #[test]
    fn test_display_format_display() {
        assert_eq!(format!("{}", LayoutDisplay::InlineBlock), "inline-block");
    }

    #[test]
    fn test_print_as_css_value() {
        assert_eq!(LayoutDisplay::Flex.print_as_css_value(), "flex");
    }

    #[test]
    #[cfg(feature = "parser")]
    fn test_parse_display_value_initial_inherit() {
        use crate::parser::parse_css_property_value;
        let mut debug_logs = Some(Vec::new());

        let res_initial = parse_css_property_value("display", "initial", &mut debug_logs);
        assert_eq!(res_initial, Ok(crate::CssProperty::Display(CssPropertyValue::Initial)));

        let res_inherit = parse_css_property_value("display", "inherit", &mut debug_logs);
        assert_eq!(res_inherit, Ok(crate::CssProperty::Display(CssPropertyValue::Inherit)));

        // Check that non-keyword values are parsed as Exact
        let res_block = parse_css_property_value("display", "block", &mut debug_logs);
        assert_eq!(res_block, Ok(crate::CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Block))));
    }
}
