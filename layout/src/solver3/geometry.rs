//! TODO: Move these to CSS module

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    ui_solver::ResolvedOffsets,
};
use azul_css::props::{
    basic::{pixel::PixelValue, PhysicalSize, PropertyContext, ResolutionContext, SizeMetric},
    layout::LayoutWritingMode,
};

/// Represents the CSS `box-sizing` property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
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

// ============================================================================
// UNRESOLVED VALUE TYPES (for lazy resolution during layout)
// ============================================================================

/// An unresolved CSS margin value.
///
/// Margins can be `auto` (for centering) or a length value that needs
/// resolution against the containing block.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum UnresolvedMargin {
    /// margin: 0 (default)
    #[default]
    Zero,
    /// margin: auto (for centering, CSS 2.2 § 10.3.3)
    Auto,
    /// A length value (px, %, em, vh, etc.)
    Length(PixelValue),
}

impl UnresolvedMargin {
    /// Returns true if this is an auto margin
    pub fn is_auto(&self) -> bool {
        matches!(self, UnresolvedMargin::Auto)
    }

    /// Resolve this margin value to pixels.
    ///
    /// - `Auto` returns 0.0 (actual auto margin calculation happens in layout)
    /// - `Zero` returns 0.0
    /// - `Length` is resolved using the resolution context
    pub fn resolve(&self, ctx: &ResolutionContext) -> f32 {
        match self {
            UnresolvedMargin::Zero => 0.0,
            UnresolvedMargin::Auto => 0.0, // Auto is handled separately in layout
            UnresolvedMargin::Length(pv) => pv.resolve_with_context(ctx, PropertyContext::Margin),
        }
    }
}

/// Unresolved edge sizes for margin/padding/border.
///
/// This stores the raw CSS values before resolution, allowing us to
/// defer resolution until the containing block size is known.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnresolvedEdge<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T> UnresolvedEdge<T> {
    pub fn new(top: T, right: T, bottom: T, left: T) -> Self {
        Self { top, right, bottom, left }
    }
}

impl UnresolvedEdge<UnresolvedMargin> {
    /// Resolve all margin edges to pixel values.
    pub fn resolve(&self, ctx: &ResolutionContext) -> EdgeSizes {
        EdgeSizes {
            top: self.top.resolve(ctx),
            right: self.right.resolve(ctx),
            bottom: self.bottom.resolve(ctx),
            left: self.left.resolve(ctx),
        }
    }

    /// Extract which margins are set to `auto`.
    pub fn get_margin_auto(&self) -> MarginAuto {
        MarginAuto {
            top: self.top.is_auto(),
            right: self.right.is_auto(),
            bottom: self.bottom.is_auto(),
            left: self.left.is_auto(),
        }
    }
}

impl UnresolvedEdge<PixelValue> {
    /// Resolve all edges to pixel values.
    pub fn resolve(&self, ctx: &ResolutionContext, prop_ctx: PropertyContext) -> EdgeSizes {
        EdgeSizes {
            top: self.top.resolve_with_context(ctx, prop_ctx),
            right: self.right.resolve_with_context(ctx, prop_ctx),
            bottom: self.bottom.resolve_with_context(ctx, prop_ctx),
            left: self.left.resolve_with_context(ctx, prop_ctx),
        }
    }
}

/// Parameters needed to resolve CSS values to pixels.
#[derive(Debug, Clone, Copy)]
pub struct ResolutionParams {
    /// The containing block size (for % resolution)
    pub containing_block: LogicalSize,
    /// The viewport size (for vh/vw resolution)
    pub viewport_size: LogicalSize,
    /// The element's computed font-size (for em resolution)
    pub element_font_size: f32,
    /// The root element's font-size (for rem resolution)
    pub root_font_size: f32,
}

impl ResolutionParams {
    /// Create a ResolutionContext from these parameters.
    pub fn to_resolution_context(&self) -> ResolutionContext {
        ResolutionContext {
            element_font_size: self.element_font_size,
            parent_font_size: self.element_font_size, // For em in non-font properties
            root_font_size: self.root_font_size,
            element_size: None,
            containing_block_size: PhysicalSize::new(
                self.containing_block.width,
                self.containing_block.height,
            ),
            viewport_size: PhysicalSize::new(
                self.viewport_size.width,
                self.viewport_size.height,
            ),
        }
    }
}

// ============================================================================
// UNRESOLVED BOX PROPS (new design)
// ============================================================================

/// Box properties with unresolved CSS values.
///
/// This stores the raw CSS values as parsed, deferring resolution until
/// layout time when the containing block size is known.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnresolvedBoxProps {
    pub margin: UnresolvedEdge<UnresolvedMargin>,
    pub padding: UnresolvedEdge<PixelValue>,
    pub border: UnresolvedEdge<PixelValue>,
}

impl UnresolvedBoxProps {
    /// Resolve all box properties to pixel values.
    pub fn resolve(&self, params: &ResolutionParams) -> ResolvedBoxProps {
        let ctx = params.to_resolution_context();
        ResolvedBoxProps {
            margin: self.margin.resolve(&ctx),
            padding: self.padding.resolve(&ctx, PropertyContext::Padding),
            border: self.border.resolve(&ctx, PropertyContext::BorderWidth),
            margin_auto: self.margin.get_margin_auto(),
        }
    }
}

// ============================================================================
// RESOLVED BOX PROPS (legacy name: BoxProps)
// ============================================================================

/// Tracks which margins are set to `auto` (for centering calculations).
#[derive(Debug, Clone, Copy, Default)]
pub struct MarginAuto {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

/// A fully resolved representation of a node's box model properties.
///
/// All values are in pixels. This is the result of resolving `UnresolvedBoxProps`
/// against a containing block.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResolvedBoxProps {
    pub margin: EdgeSizes,
    pub padding: EdgeSizes,
    pub border: EdgeSizes,
    /// Tracks which margins are set to `auto`.
    /// CSS 2.2 § 10.3.3: If both margin-left and margin-right are auto,
    /// their used values are equal, centering the element within its container.
    pub margin_auto: MarginAuto,
}

impl ResolvedBoxProps {
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

/// Type alias for backwards compatibility.
/// TODO: Remove this once all code uses ResolvedBoxProps directly.
pub type BoxProps = ResolvedBoxProps;

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
