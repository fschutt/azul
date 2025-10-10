//! CSS properties for `display` and `float`.

use alloc::string::{String, ToString};

#[cfg(feature = "parser")]
use crate::parser::{impl_debug_as_display, impl_display};
use crate::props::formatter::PrintAsCssValue;

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

    FlowRoot,

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

    pub fn creates_flex_context(&self) -> bool {
        matches!(self, LayoutDisplay::Flex | LayoutDisplay::InlineFlex)
    }

    pub fn creates_table_context(&self) -> bool {
        matches!(self, LayoutDisplay::Table | LayoutDisplay::InlineTable)
    }

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
        String::from(match self {
            LayoutDisplay::None => "none",
            LayoutDisplay::Block => "block",
            LayoutDisplay::Inline => "inline",
            LayoutDisplay::InlineBlock => "inline-block",
            LayoutDisplay::Flex => "flex",
            LayoutDisplay::InlineFlex => "inline-flex",
            LayoutDisplay::Table => "table",
            LayoutDisplay::InlineTable => "inline-table",
            LayoutDisplay::TableRowGroup => "table-row-group",
            LayoutDisplay::TableHeaderGroup => "table-header-group",
            LayoutDisplay::TableFooterGroup => "table-footer-group",
            LayoutDisplay::TableRow => "table-row",
            LayoutDisplay::TableColumnGroup => "table-column-group",
            LayoutDisplay::TableColumn => "table-column",
            LayoutDisplay::TableCell => "table-cell",
            LayoutDisplay::TableCaption => "table-caption",
            LayoutDisplay::ListItem => "list-item",
            LayoutDisplay::RunIn => "run-in",
            LayoutDisplay::Marker => "marker",
            LayoutDisplay::FlowRoot => "flow-root",
            LayoutDisplay::Grid => "grid",
            LayoutDisplay::InlineGrid => "inline-grid",
            LayoutDisplay::Initial => "initial",
            LayoutDisplay::Inherit => "inherit",
        })
    }
}

/// Represents a `float` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutFloat {
    Left,
    Right,
    None,
}

impl Default for LayoutFloat {
    fn default() -> Self {
        LayoutFloat::None
    }
}

impl PrintAsCssValue for LayoutFloat {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutFloat::Left => "left",
            LayoutFloat::Right => "right",
            LayoutFloat::None => "none",
        })
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutDisplayParseErrorOwned {
    InvalidValue(String),
}

#[cfg(feature = "parser")]
impl<'a> LayoutDisplayParseError<'a> {
    pub fn to_contained(&self) -> LayoutDisplayParseErrorOwned {
        match self {
            Self::InvalidValue(s) => LayoutDisplayParseErrorOwned::InvalidValue(s.to_string()),
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutDisplayParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutDisplayParseError<'a> {
        match self {
            Self::InvalidValue(s) => LayoutDisplayParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_display<'a>(
    input: &'a str,
) -> Result<LayoutDisplay, LayoutDisplayParseError<'a>> {
    let input = input.trim();
    match input {
        "none" => Ok(LayoutDisplay::None),
        "block" => Ok(LayoutDisplay::Block),
        "inline" => Ok(LayoutDisplay::Inline),
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
        "initial" => Ok(LayoutDisplay::Initial),
        "inherit" => Ok(LayoutDisplay::Inherit),
        "flow-root" => Ok(LayoutDisplay::FlowRoot),
        _ => Err(LayoutDisplayParseError::InvalidValue(input)),
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutFloatParseErrorOwned {
    InvalidValue(String),
}

#[cfg(feature = "parser")]
impl<'a> LayoutFloatParseError<'a> {
    pub fn to_contained(&self) -> LayoutFloatParseErrorOwned {
        match self {
            Self::InvalidValue(s) => LayoutFloatParseErrorOwned::InvalidValue(s.to_string()),
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutFloatParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> LayoutFloatParseError<'a> {
        match self {
            Self::InvalidValue(s) => LayoutFloatParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_float<'a>(input: &'a str) -> Result<LayoutFloat, LayoutFloatParseError<'a>> {
    let input = input.trim();
    match input {
        "left" => Ok(LayoutFloat::Left),
        "right" => Ok(LayoutFloat::Right),
        "none" => Ok(LayoutFloat::None),
        _ => Err(LayoutFloatParseError::InvalidValue(input)),
    }
}
