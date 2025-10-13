//! TODO: Move these to CSS module

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    ui_solver::ResolvedOffsets,
};
use azul_css::props::layout::LayoutWritingMode;

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
    pub fn main_start(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.top,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.left,
        }
    }

    /// Returns the size of the edge at the end of the main/block axis.
    pub fn main_end(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.bottom,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.right,
        }
    }

    /// Returns the sum of the start and end sizes on the main/block axis.
    pub fn main_sum(&self, wm: LayoutWritingMode) -> f32 {
        self.main_start(wm) + self.main_end(wm)
    }

    /// Returns the size of the edge at the start of the cross/inline axis.
    pub fn cross_start(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.left,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.top,
        }
    }

    /// Returns the size of the edge at the end of the cross/inline axis.
    pub fn cross_end(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.right,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.bottom,
        }
    }

    /// Returns the sum of the start and end sizes on the cross/inline axis.
    pub fn cross_sum(&self, wm: LayoutWritingMode) -> f32 {
        self.cross_start(wm) + self.cross_end(wm)
    }
}

/// A fully resolved representation of a a node's box model properties.
#[derive(Debug, Clone, Copy, Default)]
pub struct BoxProps {
    pub margin: EdgeSizes,
    pub padding: EdgeSizes,
    pub border: EdgeSizes,
}

impl BoxProps {
    /// Calculates the inner content-box size from an outer border-box size,
    /// correctly accounting for the specified writing mode.
    pub fn inner_size(&self, outer_size: LogicalSize, wm: LayoutWritingMode) -> LogicalSize {
        let outer_main = outer_size.main(wm);
        let outer_cross = outer_size.cross(wm);

        // The sum of padding and border along the cross (inline) axis.
        let cross_axis_spacing = self.padding.cross_sum(wm) + self.border.cross_sum(wm);

        // The sum of padding and border along the main (block) axis.
        let main_axis_spacing = self.padding.main_sum(wm) + self.border.main_sum(wm);

        let inner_main = (outer_main - main_axis_spacing).max(0.0);
        let inner_cross = (outer_cross - cross_axis_spacing).max(0.0);

        LogicalSize::from_main_cross(inner_main, inner_cross, wm)
    }
}

// Verwende die Typen aus azul_css für float und clear
pub use azul_css::props::layout::{LayoutClear, LayoutFloat};

/// Represents the intrinsic sizing information for an element, calculated
/// without knowledge of the final containing block size.
#[derive(Debug, Clone, Copy, Default)]
pub struct IntrinsicSizes {
    /// The narrowest possible width, e.g., the width of the longest word.
    pub min_content_width: f32,
    /// The preferred width if infinite horizontal space is available.
    pub max_content_width: f32,
    /// The width specified by CSS properties, if any.
    pub preferred_width: Option<f32>,
    /// The height of the element at its `min_content_width`.
    pub min_content_height: f32,
    /// The height of the element at its `max_content_width`.
    pub max_content_height: f32,
    /// The height specified by CSS properties, if any.
    pub preferred_height: Option<f32>,
}

impl IntrinsicSizes {
    /// Creates a zero-sized IntrinsicSizes.
    pub fn zero() -> Self {
        Self::default()
    }
}
