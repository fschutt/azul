//! CSS properties for `display` and `float`.

use alloc::string::{String, ToString};
use crate::corety::AzString;

use crate::props::formatter::PrintAsCssValue;

/// Represents a `display` CSS property value
// +spec:display-property:472a62 - display property controls box generation types per CSS 2.2 §9.2
// +spec:display-property:cf1820 - display type enum defining box generation qualities
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutDisplay {
    // Basic display types
    None,
    // +spec:display-property:7d945d - outer display defaults to block, inner defaults to flow
    #[default]
    Block,
    Inline,
    InlineBlock,

    // Flex layout
    Flex,
    InlineFlex,

    // +spec:display-property:03b26a - Table display types mapping document elements to CSS table model
    // +spec:display-property:d40388 - layout-internal display types set both inner and outer display
    // +spec:display-property:dcf7f5 - table display values (table, inline-table, table-row, etc.) per CSS 2.2 §17
    // +spec:table-layout:7fdc60 - display property maps elements to table roles (CSS 2.2 §17.1)
    // Table layout
    // +spec:display-property:1554ad - Layout-internal display types for table layout
    // +spec:table-layout:6cc828 - <display-internal> and <display-legacy> table display types
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

    FlowRoot,

    // List layout
    ListItem,

    // Special displays
    RunIn,
    Marker,

    // CSS3 additions
    Grid,
    InlineGrid,

    // display:contents - element generates no box, children promoted to parent
    Contents,
}

impl LayoutDisplay {
    /// Returns true if this display type establishes a block formatting context.
    #[must_use] pub const fn creates_block_context(&self) -> bool {
        matches!(
            self,
            Self::Block
                | Self::FlowRoot
                | Self::Flex
                | Self::Grid
                | Self::Table
                | Self::ListItem
        )
    }

    /// Returns true if this display type establishes a flex formatting context.
    #[must_use] pub const fn creates_flex_context(&self) -> bool {
        matches!(self, Self::Flex | Self::InlineFlex)
    }

    // +spec:display-property:798b4f - table box establishes table formatting context (CSS 2.2 §17.4)
    /// Returns true if this display type establishes a table formatting context.
    #[must_use] pub const fn creates_table_context(&self) -> bool {
        matches!(self, Self::Table | Self::InlineTable)
    }

    /// Returns true for layout-internal display types (CSS Display 3 §2.4):
    /// table-row-group, table-header-group, table-footer-group, table-row,
    /// table-column-group, table-column, table-cell, table-caption.
    #[must_use] pub const fn is_layout_internal(&self) -> bool {
        matches!(
            self,
            Self::TableRowGroup
                | Self::TableHeaderGroup
                | Self::TableFooterGroup
                | Self::TableRow
                | Self::TableColumnGroup
                | Self::TableColumn
                | Self::TableCell
                | Self::TableCaption
        )
    }

    // +spec:display-property:101f27 - inline-level boxes (InlineBlock, InlineFlex, etc.) vs inline boxes (Inline)
    // +spec:display-property:18e77e - inner-only display keywords (flex, grid, table, flow-root) are not inline-level, defaulting outer display to block
    // +spec:display-property:a43e48 - inline-table is inline-level per CSS 2.2 §17.4
    /// Returns true if this display type generates an inline-level box.
    #[must_use] pub const fn is_inline_level(&self) -> bool {
        matches!(
            self,
            Self::Inline
                | Self::InlineBlock
                | Self::InlineFlex
                | Self::InlineTable
                | Self::InlineGrid
        )
    }
}

// +spec:display-property:cabaec - serialization uses short display keywords per CSSOM precedence rules
impl PrintAsCssValue for LayoutDisplay {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::None => "none",
            Self::Block => "block",
            Self::Inline => "inline",
            Self::InlineBlock => "inline-block",
            Self::Flex => "flex",
            Self::InlineFlex => "inline-flex",
            Self::Table => "table",
            Self::InlineTable => "inline-table",
            Self::TableRowGroup => "table-row-group",
            Self::TableHeaderGroup => "table-header-group",
            Self::TableFooterGroup => "table-footer-group",
            Self::TableRow => "table-row",
            Self::TableColumnGroup => "table-column-group",
            Self::TableColumn => "table-column",
            Self::TableCell => "table-cell",
            Self::TableCaption => "table-caption",
            Self::ListItem => "list-item",
            Self::RunIn => "run-in",
            Self::Marker => "marker",
            Self::FlowRoot => "flow-root",
            Self::Grid => "grid",
            Self::InlineGrid => "inline-grid",
            Self::Contents => "contents",
        })
    }
}

/// Represents a `float` attribute
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutFloat {
    Left,
    Right,
    #[default]
    None,
}

impl PrintAsCssValue for LayoutFloat {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::None => "none",
        })
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum LayoutDisplayParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(LayoutDisplayParseError<'a>);

#[cfg(feature = "parser")]
impl_display! { LayoutDisplayParseError<'a>, {
    InvalidValue(val) => format!("Invalid display value: \"{}\"", val),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutDisplayParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl LayoutDisplayParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> LayoutDisplayParseErrorOwned {
        match self {
            Self::InvalidValue(s) => LayoutDisplayParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutDisplayParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> LayoutDisplayParseError<'_> {
        match self {
            Self::InvalidValue(s) => LayoutDisplayParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_display(
    input: &str,
) -> Result<LayoutDisplay, LayoutDisplayParseError<'_>> {
    let input = input.trim();
    match input {
        "none" => Ok(LayoutDisplay::None),
        "block" => Ok(LayoutDisplay::Block),
        "inline" => Ok(LayoutDisplay::Inline),
        // +spec:display-property:f704ef - legacy single-keyword inline-level display values (inline-block, inline-table, inline-flex, inline-grid)
        "inline-block" => Ok(LayoutDisplay::InlineBlock),
        "flex" => Ok(LayoutDisplay::Flex),
        "inline-flex" => Ok(LayoutDisplay::InlineFlex),
        "table" => Ok(LayoutDisplay::Table),
        "inline-table" => Ok(LayoutDisplay::InlineTable),
        "table-row-group" => Ok(LayoutDisplay::TableRowGroup),
        "table-header-group" => Ok(LayoutDisplay::TableHeaderGroup),
        "table-footer-group" => Ok(LayoutDisplay::TableFooterGroup),
        "table-row" => Ok(LayoutDisplay::TableRow),
        "table-column-group" => Ok(LayoutDisplay::TableColumnGroup),
        "table-column" => Ok(LayoutDisplay::TableColumn),
        "table-cell" => Ok(LayoutDisplay::TableCell),
        "table-caption" => Ok(LayoutDisplay::TableCaption),
        "list-item" => Ok(LayoutDisplay::ListItem),
        "run-in" => Ok(LayoutDisplay::RunIn),
        "marker" => Ok(LayoutDisplay::Marker),
        "grid" => Ok(LayoutDisplay::Grid),
        "inline-grid" => Ok(LayoutDisplay::InlineGrid),
        "flow-root" => Ok(LayoutDisplay::FlowRoot),
        "contents" => Ok(LayoutDisplay::Contents),
        _ => Err(LayoutDisplayParseError::InvalidValue(input)),
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum LayoutFloatParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(LayoutFloatParseError<'a>);

#[cfg(feature = "parser")]
impl_display! { LayoutFloatParseError<'a>, {
    InvalidValue(val) => format!("Invalid float value: \"{}\"", val),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutFloatParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl LayoutFloatParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> LayoutFloatParseErrorOwned {
        match self {
            Self::InvalidValue(s) => LayoutFloatParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutFloatParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> LayoutFloatParseError<'_> {
        match self {
            Self::InvalidValue(s) => LayoutFloatParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_float(input: &str) -> Result<LayoutFloat, LayoutFloatParseError<'_>> {
    let input = input.trim();
    match input {
        "left" => Ok(LayoutFloat::Left),
        "right" => Ok(LayoutFloat::Right),
        "none" => Ok(LayoutFloat::None),
        _ => Err(LayoutFloatParseError::InvalidValue(input)),
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_layout_display() {
        assert_eq!(parse_layout_display("block").unwrap(), LayoutDisplay::Block);
        assert_eq!(
            parse_layout_display("inline").unwrap(),
            LayoutDisplay::Inline
        );
        assert_eq!(
            parse_layout_display("inline-block").unwrap(),
            LayoutDisplay::InlineBlock
        );
        assert_eq!(parse_layout_display("flex").unwrap(), LayoutDisplay::Flex);
        assert_eq!(
            parse_layout_display("inline-flex").unwrap(),
            LayoutDisplay::InlineFlex
        );
        assert_eq!(parse_layout_display("grid").unwrap(), LayoutDisplay::Grid);
        assert_eq!(
            parse_layout_display("inline-grid").unwrap(),
            LayoutDisplay::InlineGrid
        );
        assert_eq!(parse_layout_display("none").unwrap(), LayoutDisplay::None);
        assert_eq!(
            parse_layout_display("flow-root").unwrap(),
            LayoutDisplay::FlowRoot
        );
        assert_eq!(
            parse_layout_display("list-item").unwrap(),
            LayoutDisplay::ListItem
        );
        // Note: 'inherit' and 'initial' are handled by the CSS cascade system,
        // not as enum variants
        assert!(parse_layout_display("inherit").is_err());
        assert!(parse_layout_display("initial").is_err());

        // Table values
        assert_eq!(parse_layout_display("table").unwrap(), LayoutDisplay::Table);
        assert_eq!(
            parse_layout_display("inline-table").unwrap(),
            LayoutDisplay::InlineTable
        );
        assert_eq!(
            parse_layout_display("table-row").unwrap(),
            LayoutDisplay::TableRow
        );
        assert_eq!(
            parse_layout_display("table-cell").unwrap(),
            LayoutDisplay::TableCell
        );
        assert_eq!(
            parse_layout_display("table-caption").unwrap(),
            LayoutDisplay::TableCaption
        );
        assert_eq!(
            parse_layout_display("table-column-group").unwrap(),
            LayoutDisplay::TableColumnGroup
        );
        assert_eq!(
            parse_layout_display("table-header-group").unwrap(),
            LayoutDisplay::TableHeaderGroup
        );
        assert_eq!(
            parse_layout_display("table-footer-group").unwrap(),
            LayoutDisplay::TableFooterGroup
        );
        assert_eq!(
            parse_layout_display("table-row-group").unwrap(),
            LayoutDisplay::TableRowGroup
        );

        // Whitespace
        assert_eq!(
            parse_layout_display("  inline-flex  ").unwrap(),
            LayoutDisplay::InlineFlex
        );

        // Invalid values
        assert!(parse_layout_display("invalid-value").is_err());
        assert!(parse_layout_display("").is_err());
        assert!(parse_layout_display("display").is_err());
    }

    #[test]
    fn test_parse_layout_float() {
        assert_eq!(parse_layout_float("left").unwrap(), LayoutFloat::Left);
        assert_eq!(parse_layout_float("right").unwrap(), LayoutFloat::Right);
        assert_eq!(parse_layout_float("none").unwrap(), LayoutFloat::None);

        // Whitespace
        assert_eq!(parse_layout_float("  right  ").unwrap(), LayoutFloat::Right);

        // Invalid values
        assert!(parse_layout_float("center").is_err());
        assert!(parse_layout_float("").is_err());
        assert!(parse_layout_float("float-left").is_err());
    }
}
