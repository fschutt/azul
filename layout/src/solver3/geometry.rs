//! TODO: Move these to CSS module

use azul_core::{
    ui_solver::ResolvedOffsets,
    window::{LogicalPosition, LogicalRect, LogicalSize, WritingMode},
};

#[derive(Debug, PartialEq, Eq, Default)]
pub enum DisplayType {
    #[default]
    Block,
    InlineBlock,
    Inline,
    Table,
    TableRow,
    TableRowGroup,
    TableCell,
    // Add Flex, Grid, etc.
}

/// Represents the CSS `box-sizing` property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

/// Represents a size that may be defined in various units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CssSize {
    Auto,
    Px(f32),
    Percent(f32),
    MinContent,
    MaxContent,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct PositionedRectangle {
    /// The outer bounds of the rectangle
    pub bounds: LogicalRect,
    /// Margin of the rectangle.
    pub margin: ResolvedOffsets,
    /// Border widths of the rectangle.
    pub border: ResolvedOffsets,
    /// Padding of the rectangle.
    pub padding: ResolvedOffsets,
}

/// Represents the four edges of a box for properties like margin, padding, border.
#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeSizes {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl EdgeSizes {
    /// Returns the size of the edge at the start of the main/block axis.
    pub fn main_start(&self, wm: WritingMode) -> f32 {
        match wm {
            WritingMode::HorizontalTb => self.top,
            WritingMode::VerticalRl | WritingMode::VerticalLr => self.left,
        }
    }

    /// Returns the size of the edge at the end of the main/block axis.
    pub fn main_end(&self, wm: WritingMode) -> f32 {
        match wm {
            WritingMode::HorizontalTb => self.bottom,
            WritingMode::VerticalRl | WritingMode::VerticalLr => self.right,
        }
    }

    /// Returns the sum of the start and end sizes on the main/block axis.
    pub fn main_sum(&self, wm: WritingMode) -> f32 {
        self.main_start(wm) + self.main_end(wm)
    }

    /// Returns the sum of the start and end sizes on the cross/inline axis.
    pub fn cross_sum(&self, wm: WritingMode) -> f32 {
        match wm {
            WritingMode::HorizontalTb => self.left + self.right,
            WritingMode::VerticalRl | WritingMode::VerticalLr => self.top + self.bottom,
        }
    }
}

/// A fully resolved representation of a a node's box model properties.
#[derive(Debug, Clone, Copy, Default)]
pub struct BoxProps {
    pub margin: EdgeSizes,
    pub padding: EdgeSizes,
    pub border: EdgeSizes,
}

/// Represents the CSS `float` property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Float {
    #[default]
    None,
    Left,
    Right,
}

/// Represents the CSS `clear` property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Clear {
    #[default]
    None,
    Left,
    Right,
    Both,
}
