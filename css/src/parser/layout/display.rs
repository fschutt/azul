use crate::{css_properties::*, parser::*};

/// Represents a `display` CSS property value
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutDisplay {
    // Basic display types
    None,
    #[default]
    Block,
    Inline,
    InlineBlock,

    // Flex layout
    Flex,
    InlineFlex,

    // Table layout
    Table,
    InlineTable,
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableRow,
    TableColumnGroup,
    TableColumn,
    TableCell,
    TableCaption,

    // List layout
    ListItem,

    // Special displays
    RunIn,
    Marker,

    // CSS3 additions
    Grid,
    InlineGrid,

    // Initial/Inherit values
    Initial,
    Inherit,
}

impl LayoutDisplay {
    /// Returns whether this display type creates a block formatting context
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

    /// Returns whether this display type creates a flex formatting context
    pub fn creates_flex_context(&self) -> bool {
        matches!(self, LayoutDisplay::Flex | LayoutDisplay::InlineFlex)
    }

    /// Returns whether this display type creates a table formatting context
    pub fn creates_table_context(&self) -> bool {
        matches!(self, LayoutDisplay::Table | LayoutDisplay::InlineTable)
    }

    /// Returns whether this is an inline-level display type
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

multi_type_parser!(
    parse_layout_display,
    LayoutDisplay,
    ["none", None],
    ["block", Block],
    ["inline", Inline],
    ["inline-block", InlineBlock],
    ["flex", Flex],
    ["inline-flex", InlineFlex],
    ["table", Table],
    ["inline-table", InlineTable],
    ["table-row-group", TableRowGroup],
    ["table-header-group", TableHeaderGroup],
    ["table-footer-group", TableFooterGroup],
    ["table-row", TableRow],
    ["table-column-group", TableColumnGroup],
    ["table-column", TableColumn],
    ["table-cell", TableCell],
    ["table-caption", TableCaption],
    ["list-item", ListItem],
    ["run-in", RunIn],
    ["marker", Marker],
    ["grid", Grid],
    ["inline-grid", InlineGrid],
    ["initial", Initial],
    ["inherit", Inherit]
);
