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

// +spec:width-calculation-p039 - §8.1: each box has content area and optional surrounding padding, border, margin areas
// +spec:width-calculation-p042 - §8.1: each box has content area and optional surrounding padding, border, margin areas; width is part of content edge
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

// +spec:width-calculation-p039 - §8.1: size of each area (margin, padding, border) specified by per-side properties
// +spec:width-calculation-p042 - §8.1: margin/border/padding broken into top, right, bottom, left segments
/// Represents the four edges of a box for properties like margin, padding, border.
#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeSizes {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl EdgeSizes {
    pub fn zero() -> Self {
        Self { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 }
    }

    /// Sum of horizontal edges (left + right).
    pub fn horizontal_sum(&self) -> f32 {
        self.left + self.right
    }

    /// Sum of vertical edges (top + bottom).
    pub fn vertical_sum(&self) -> f32 {
        self.top + self.bottom
    }

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

    // +spec:box-model-p041 - §8.3 baseline: item with largest distance between baseline and cross-start margin edge placed flush against cross-start edge
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
            // +spec:width-calculation-p001 - §10.3.9: for inline-block, auto margin-left/margin-right becomes used value of 0
            // +spec:width-calculation-p007 - §10.3.1/§10.3.2: auto margin-left/margin-right on inline elements becomes 0
            // +spec:width-calculation-p008 - §10.3.1: auto margin-left/margin-right on inline non-replaced elements becomes used value 0
            // +spec:inline-block-p043 - §10.3.9: auto margins on inline-block non-replaced elements become 0
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

// +spec:box-model-p048 - §8.1 example: each element stores its own margin/padding/border per-side (UL/LI nesting)
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

// +spec:box-model-p048 - §8.1 example: resolved per-element margin/padding/border for correct parent-child box nesting
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
    // +spec:width-calculation-p039 - §8.1: content edge surrounds content area; padding/border/margin edges nest outward
    // +spec:width-calculation-p042 - §8.1: content edge surrounds rectangle given by width; padding/border/margin edges nest outward
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

    /// Returns the content-box rect from a border-box rect.
    /// Shrinks inward by border + padding on each side.
    pub fn content_box(&self, border_box: LogicalRect) -> LogicalRect {
        let x = border_box.origin.x + self.border.left + self.padding.left;
        let y = border_box.origin.y + self.border.top + self.padding.top;
        let w = (border_box.size.width - self.border.horizontal_sum() - self.padding.horizontal_sum()).max(0.0);
        let h = (border_box.size.height - self.border.vertical_sum() - self.padding.vertical_sum()).max(0.0);
        LogicalRect { origin: LogicalPosition { x, y }, size: LogicalSize { width: w, height: h } }
    }

    /// Returns the padding-box rect from a border-box rect.
    /// Shrinks inward by border on each side.
    pub fn padding_box(&self, border_box: LogicalRect) -> LogicalRect {
        let x = border_box.origin.x + self.border.left;
        let y = border_box.origin.y + self.border.top;
        let w = (border_box.size.width - self.border.horizontal_sum()).max(0.0);
        let h = (border_box.size.height - self.border.vertical_sum()).max(0.0);
        LogicalRect { origin: LogicalPosition { x, y }, size: LogicalSize { width: w, height: h } }
    }

    /// Returns the margin-box rect from a border-box rect.
    /// Expands outward by margin on each side.
    pub fn margin_box(&self, border_box: LogicalRect) -> LogicalRect {
        let x = border_box.origin.x - self.margin.left;
        let y = border_box.origin.y - self.margin.top;
        let w = border_box.size.width + self.margin.horizontal_sum();
        let h = border_box.size.height + self.margin.vertical_sum();
        LogicalRect { origin: LogicalPosition { x, y }, size: LogicalSize { width: w, height: h } }
    }

    /// Total horizontal space consumed by margin + border + padding.
    pub fn horizontal_mbp(&self) -> f32 {
        self.margin.horizontal_sum() + self.border.horizontal_sum() + self.padding.horizontal_sum()
    }

    /// Total vertical space consumed by margin + border + padding.
    pub fn vertical_mbp(&self) -> f32 {
        self.margin.vertical_sum() + self.border.vertical_sum() + self.padding.vertical_sum()
    }

    /// Total horizontal space consumed by border + padding only (no margin).
    pub fn horizontal_bp(&self) -> f32 {
        self.border.horizontal_sum() + self.padding.horizontal_sum()
    }

    /// Total vertical space consumed by border + padding only (no margin).
    pub fn vertical_bp(&self) -> f32 {
        self.border.vertical_sum() + self.padding.vertical_sum()
    }
}

/// Type alias for backwards compatibility.
/// TODO: Remove this once all code uses ResolvedBoxProps directly.
pub type BoxProps = ResolvedBoxProps;

/// Shrink a rect inward by the given edge sizes.
pub fn shrink_rect_by_edges(rect: LogicalRect, edges: &EdgeSizes) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition {
            x: rect.origin.x + edges.left,
            y: rect.origin.y + edges.top,
        },
        size: LogicalSize {
            width: (rect.size.width - edges.horizontal_sum()).max(0.0),
            height: (rect.size.height - edges.vertical_sum()).max(0.0),
        },
    }
}

/// Expand a rect outward by the given edge sizes.
pub fn expand_rect_by_edges(rect: LogicalRect, edges: &EdgeSizes) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition {
            x: rect.origin.x - edges.left,
            y: rect.origin.y - edges.top,
        },
        size: LogicalSize {
            width: rect.size.width + edges.horizontal_sum(),
            height: rect.size.height + edges.vertical_sum(),
        },
    }
}

// Verwende die Typen aus azul_css für float und clear
pub use azul_css::props::layout::{LayoutClear, LayoutFloat};

// +spec:intrinsic-sizing-p024 - css-sizing-3 §2.2/§2.3: min-content contribution, max-content contribution,
// min-content constraint, max-content constraint definitions
// +spec:intrinsic-sizing-p032 - §2.1 css-sizing-3: defines min-content, max-content,
// and fit-content sizes for both inline and block axes
// +spec:intrinsic-sizing-p047 - css-sizing-3 index: IntrinsicSizes struct implements min-content, max-content intrinsic size terms
// +spec:width-calculation-p040 - css-sizing-3 §2.1: intrinsic size = max-content or min-content size
/// Represents the intrinsic sizing information for an element, calculated
/// without knowledge of the final containing block size.
#[derive(Debug, Clone, Copy, Default)]
pub struct IntrinsicSizes {
    /// §2.1 min-content inline size: inline size fitting contents if all soft wraps taken.
    pub min_content_width: f32,
    /// §2.1 max-content inline size: narrowest inline size if no soft wraps taken.
    pub max_content_width: f32,
    /// The width specified by CSS properties, if any.
    pub preferred_width: Option<f32>,
    /// §2.1 min-content block size: for block containers, tables, and inline boxes,
    /// equivalent to max-content block size.
    pub min_content_height: f32,
    /// §2.1 max-content block size: "ideal" block size, usually content height after layout.
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
