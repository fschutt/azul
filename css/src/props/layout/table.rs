//! CSS properties for table layout and styling.
//!
//! This module contains properties specific to CSS table formatting:
//! - `table-layout`: Controls the algorithm used to layout table cells, rows, and columns
//! - `border-collapse`: Specifies whether cell borders are collapsed into a single border or separated
//! - `border-spacing`: Sets the distance between borders of adjacent cells (separate borders only)
//! - `caption-side`: Specifies the placement of a table caption
//! - `empty-cells`: Specifies whether or not to display borders on empty cells in a table

use alloc::string::{String, ToString};
use crate::{
    format_rust_code::FormatAsRustCode,
    props::{
        basic::pixel::{PixelValue, CssPixelValueParseError, CssPixelValueParseErrorOwned},
        formatter::PrintAsCssValue,
        macros::PixelValueTaker,
    },
};

// ========== table-layout ==========

/// Controls the algorithm used to lay out table cells, rows, and columns.
///
/// The `table-layout` property determines whether the browser should use:
/// - **auto**: Column widths are determined by the content (slower but flexible)
/// - **fixed**: Column widths are determined by the first row (faster and predictable)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutTableLayout {
    /// Use automatic table layout algorithm (content-based, default).
    /// Column width is set by the widest unbreakable content in the cells.
    Auto,
    /// Use fixed table layout algorithm (first-row-based).
    /// Column width is set by the width property of the column or first-row cell.
    /// Renders faster than auto.
    Fixed,
}

impl Default for LayoutTableLayout {
    fn default() -> Self {
        Self::Auto
    }
}

impl PrintAsCssValue for LayoutTableLayout {
    fn print_as_css_value(&self) -> String {
        match self {
            LayoutTableLayout::Auto => "auto".to_string(),
            LayoutTableLayout::Fixed => "fixed".to_string(),
        }
    }
}

impl FormatAsRustCode for LayoutTableLayout {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            LayoutTableLayout::Auto => "LayoutTableLayout::Auto".to_string(),
            LayoutTableLayout::Fixed => "LayoutTableLayout::Fixed".to_string(),
        }
    }
}

// ========== border-collapse ==========

/// Specifies whether cell borders are collapsed into a single border or separated.
///
/// The `border-collapse` property determines the border rendering model:
/// - **separate**: Each cell has its own border (default, uses border-spacing)
/// - **collapse**: Adjacent cells share borders (ignores border-spacing)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleBorderCollapse {
    /// Borders are separated (default). Each cell has its own border.
    /// The `border-spacing` property defines the distance between borders.
    Separate,
    /// Borders are collapsed. Adjacent cells share a single border.
    /// Border conflict resolution rules apply when borders differ.
    Collapse,
}

impl Default for StyleBorderCollapse {
    fn default() -> Self {
        Self::Separate
    }
}

impl PrintAsCssValue for StyleBorderCollapse {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleBorderCollapse::Separate => "separate".to_string(),
            StyleBorderCollapse::Collapse => "collapse".to_string(),
        }
    }
}

impl FormatAsRustCode for StyleBorderCollapse {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            StyleBorderCollapse::Separate => "StyleBorderCollapse::Separate".to_string(),
            StyleBorderCollapse::Collapse => "StyleBorderCollapse::Collapse".to_string(),
        }
    }
}

// ========== border-spacing ==========

/// Sets the distance between the borders of adjacent cells.
///
/// The `border-spacing` property is only applicable when `border-collapse` is set to `separate`.
/// It can have one or two values:
/// - One value: Sets both horizontal and vertical spacing
/// - Two values: First is horizontal, second is vertical
///
/// This struct represents a single spacing value (either horizontal or vertical).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBorderSpacing {
    /// Horizontal spacing between cell borders
    pub horizontal: PixelValue,
    /// Vertical spacing between cell borders
    pub vertical: PixelValue,
}

impl Default for LayoutBorderSpacing {
    fn default() -> Self {
        // Default border-spacing is 0 (no spacing)
        Self {
            horizontal: PixelValue::const_px(0),
            vertical: PixelValue::const_px(0),
        }
    }
}

impl LayoutBorderSpacing {
    /// Creates a new border spacing with the same value for horizontal and vertical
    pub const fn new(spacing: PixelValue) -> Self {
        Self {
            horizontal: spacing,
            vertical: spacing,
        }
    }

    /// Creates a new border spacing with different horizontal and vertical values
    pub const fn new_separate(horizontal: PixelValue, vertical: PixelValue) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }

    /// Creates a border spacing with zero spacing
    pub const fn zero() -> Self {
        Self {
            horizontal: PixelValue::const_px(0),
            vertical: PixelValue::const_px(0),
        }
    }
}

impl PrintAsCssValue for LayoutBorderSpacing {
    fn print_as_css_value(&self) -> String {
        if self.horizontal == self.vertical {
            // Single value: same for both dimensions
            self.horizontal.to_string()
        } else {
            // Two values: horizontal vertical
            format!("{} {}", self.horizontal, self.vertical)
        }
    }
}

impl FormatAsRustCode for LayoutBorderSpacing {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        use crate::format_rust_code::format_pixel_value;
        format!(
            "LayoutBorderSpacing {{ horizontal: {}, vertical: {} }}",
            format_pixel_value(&self.horizontal),
            format_pixel_value(&self.vertical)
        )
    }
}

// ========== caption-side ==========

/// Specifies the placement of a table caption.
///
/// The `caption-side` property positions the caption either above or below the table.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleCaptionSide {
    /// Caption is placed above the table (default)
    Top,
    /// Caption is placed below the table
    Bottom,
}

impl Default for StyleCaptionSide {
    fn default() -> Self {
        Self::Top
    }
}

impl PrintAsCssValue for StyleCaptionSide {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleCaptionSide::Top => "top".to_string(),
            StyleCaptionSide::Bottom => "bottom".to_string(),
        }
    }
}

impl FormatAsRustCode for StyleCaptionSide {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            StyleCaptionSide::Top => "StyleCaptionSide::Top".to_string(),
            StyleCaptionSide::Bottom => "StyleCaptionSide::Bottom".to_string(),
        }
    }
}

// ========== empty-cells ==========

/// Specifies whether or not to display borders and background on empty cells.
///
/// The `empty-cells` property only applies when `border-collapse` is set to `separate`.
/// A cell is considered empty if it contains no visible content.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleEmptyCells {
    /// Show borders and background on empty cells (default)
    Show,
    /// Hide borders and background on empty cells
    Hide,
}

impl Default for StyleEmptyCells {
    fn default() -> Self {
        Self::Show
    }
}

impl PrintAsCssValue for StyleEmptyCells {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleEmptyCells::Show => "show".to_string(),
            StyleEmptyCells::Hide => "hide".to_string(),
        }
    }
}

impl FormatAsRustCode for StyleEmptyCells {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            StyleEmptyCells::Show => "StyleEmptyCells::Show".to_string(),
            StyleEmptyCells::Hide => "StyleEmptyCells::Hide".to_string(),
        }
    }
}

// ========== Parsing Functions ==========

/// Parse errors for table-layout property
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutTableLayoutParseError<'a> {
    InvalidKeyword(&'a str),
}

/// Parse a table-layout value from a string
pub fn parse_table_layout<'a>(input: &'a str) -> Result<LayoutTableLayout, LayoutTableLayoutParseError<'a>> {
    match input.trim() {
        "auto" => Ok(LayoutTableLayout::Auto),
        "fixed" => Ok(LayoutTableLayout::Fixed),
        other => Err(LayoutTableLayoutParseError::InvalidKeyword(other)),
    }
}

/// Parse errors for border-collapse property
#[derive(Debug, Clone, PartialEq)]
pub enum StyleBorderCollapseParseError<'a> {
    InvalidKeyword(&'a str),
}

/// Parse a border-collapse value from a string
pub fn parse_border_collapse<'a>(input: &'a str) -> Result<StyleBorderCollapse, StyleBorderCollapseParseError<'a>> {
    match input.trim() {
        "separate" => Ok(StyleBorderCollapse::Separate),
        "collapse" => Ok(StyleBorderCollapse::Collapse),
        other => Err(StyleBorderCollapseParseError::InvalidKeyword(other)),
    }
}

/// Parse errors for border-spacing property
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutBorderSpacingParseError<'a> {
    PixelValue(CssPixelValueParseError<'a>),
    InvalidFormat,
}

/// Parse a border-spacing value from a string
/// Accepts: "5px" or "5px 10px"
pub fn parse_border_spacing<'a>(input: &'a str) -> Result<LayoutBorderSpacing, LayoutBorderSpacingParseError<'a>> {
    use crate::props::basic::parse_pixel_value;
    
    let parts: Vec<&str> = input.trim().split_whitespace().collect();
    
    match parts.len() {
        1 => {
            // Single value: use for both horizontal and vertical
            let value = parse_pixel_value(parts[0])
                .map_err(LayoutBorderSpacingParseError::PixelValue)?;
            Ok(LayoutBorderSpacing::new(value))
        }
        2 => {
            // Two values: horizontal vertical
            let horizontal = parse_pixel_value(parts[0])
                .map_err(LayoutBorderSpacingParseError::PixelValue)?;
            let vertical = parse_pixel_value(parts[1])
                .map_err(LayoutBorderSpacingParseError::PixelValue)?;
            Ok(LayoutBorderSpacing::new_separate(horizontal, vertical))
        }
        _ => Err(LayoutBorderSpacingParseError::InvalidFormat),
    }
}

/// Parse errors for caption-side property
#[derive(Debug, Clone, PartialEq)]
pub enum StyleCaptionSideParseError<'a> {
    InvalidKeyword(&'a str),
}

/// Parse a caption-side value from a string
pub fn parse_caption_side<'a>(input: &'a str) -> Result<StyleCaptionSide, StyleCaptionSideParseError<'a>> {
    match input.trim() {
        "top" => Ok(StyleCaptionSide::Top),
        "bottom" => Ok(StyleCaptionSide::Bottom),
        other => Err(StyleCaptionSideParseError::InvalidKeyword(other)),
    }
}

/// Parse errors for empty-cells property
#[derive(Debug, Clone, PartialEq)]
pub enum StyleEmptyCellsParseError<'a> {
    InvalidKeyword(&'a str),
}

/// Parse an empty-cells value from a string
pub fn parse_empty_cells<'a>(input: &'a str) -> Result<StyleEmptyCells, StyleEmptyCellsParseError<'a>> {
    match input.trim() {
        "show" => Ok(StyleEmptyCells::Show),
        "hide" => Ok(StyleEmptyCells::Hide),
        other => Err(StyleEmptyCellsParseError::InvalidKeyword(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_table_layout() {
        assert_eq!(parse_table_layout("auto").unwrap(), LayoutTableLayout::Auto);
        assert_eq!(parse_table_layout("fixed").unwrap(), LayoutTableLayout::Fixed);
        assert!(parse_table_layout("invalid").is_err());
    }

    #[test]
    fn test_parse_border_collapse() {
        assert_eq!(parse_border_collapse("separate").unwrap(), StyleBorderCollapse::Separate);
        assert_eq!(parse_border_collapse("collapse").unwrap(), StyleBorderCollapse::Collapse);
        assert!(parse_border_collapse("invalid").is_err());
    }

    #[test]
    fn test_parse_border_spacing() {
        let spacing1 = parse_border_spacing("5px").unwrap();
        assert_eq!(spacing1.horizontal, PixelValue::const_px(5));
        assert_eq!(spacing1.vertical, PixelValue::const_px(5));

        let spacing2 = parse_border_spacing("5px 10px").unwrap();
        assert_eq!(spacing2.horizontal, PixelValue::const_px(5));
        assert_eq!(spacing2.vertical, PixelValue::const_px(10));
    }

    #[test]
    fn test_parse_caption_side() {
        assert_eq!(parse_caption_side("top").unwrap(), StyleCaptionSide::Top);
        assert_eq!(parse_caption_side("bottom").unwrap(), StyleCaptionSide::Bottom);
        assert!(parse_caption_side("invalid").is_err());
    }

    #[test]
    fn test_parse_empty_cells() {
        assert_eq!(parse_empty_cells("show").unwrap(), StyleEmptyCells::Show);
        assert_eq!(parse_empty_cells("hide").unwrap(), StyleEmptyCells::Hide);
        assert!(parse_empty_cells("invalid").is_err());
    }
}
