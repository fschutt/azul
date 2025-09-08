//! Layout display property

use crate::error::CssParsingError;
use crate::props::formatter::FormatAsCssValue;
use alloc::string::String;
use core::fmt;

/// CSS display property values
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutDisplay {
    /// Element is not displayed
    None,
    /// Block-level element
    Block,
    /// Inline element
    Inline,
    /// Inline block element
    InlineBlock,
    /// Flex container
    Flex,
    /// Inline flex container
    InlineFlex,
    /// Grid container
    Grid,
    /// Inline grid container
    InlineGrid,
    /// Table
    Table,
    /// Table row
    TableRow,
    /// Table cell
    TableCell,
    /// Table header group
    TableHeaderGroup,
    /// Table footer group
    TableFooterGroup,
    /// Table row group
    TableRowGroup,
    /// Table column
    TableColumn,
    /// Table column group
    TableColumnGroup,
    /// Table caption
    TableCaption,
    /// List item
    ListItem,
    /// Run-in
    RunIn,
    /// Contents (display contents of children)
    Contents,
}

impl Default for LayoutDisplay {
    fn default() -> Self {
        LayoutDisplay::Block
    }
}

impl fmt::Display for LayoutDisplay {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::LayoutDisplay::*;
        let s = match self {
            None => "none",
            Block => "block",
            Inline => "inline",
            InlineBlock => "inline-block",
            Flex => "flex",
            InlineFlex => "inline-flex",
            Grid => "grid",
            InlineGrid => "inline-grid",
            Table => "table",
            TableRow => "table-row",
            TableCell => "table-cell",
            TableHeaderGroup => "table-header-group",
            TableFooterGroup => "table-footer-group",
            TableRowGroup => "table-row-group",
            TableColumn => "table-column",
            TableColumnGroup => "table-column-group",
            TableCaption => "table-caption",
            ListItem => "list-item",
            RunIn => "run-in",
            Contents => "contents",
        };
        write!(f, "{}", s)
    }
}

impl FormatAsCssValue for LayoutDisplay {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

/// Parse layout display value
pub fn parse_layout_display<'a>(input: &'a str) -> Result<LayoutDisplay, CssParsingError<'a>> {
    use self::LayoutDisplay::*;
    let input = input.trim();
    match input {
        "none" => Ok(None),
        "block" => Ok(Block),
        "inline" => Ok(Inline),
        "inline-block" => Ok(InlineBlock),
        "flex" => Ok(Flex),
        "inline-flex" => Ok(InlineFlex),
        "grid" => Ok(Grid),
        "inline-grid" => Ok(InlineGrid),
        "table" => Ok(Table),
        "table-row" => Ok(TableRow),
        "table-cell" => Ok(TableCell),
        "table-header-group" => Ok(TableHeaderGroup),
        "table-footer-group" => Ok(TableFooterGroup),
        "table-row-group" => Ok(TableRowGroup),
        "table-column" => Ok(TableColumn),
        "table-column-group" => Ok(TableColumnGroup),
        "table-caption" => Ok(TableCaption),
        "list-item" => Ok(ListItem),
        "run-in" => Ok(RunIn),
        "contents" => Ok(Contents),
        _ => Err(CssParsingError::InvalidValue(input)),
    }
}
