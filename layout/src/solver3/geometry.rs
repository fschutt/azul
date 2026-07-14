//! Box model geometry types and writing-mode support for the layout solver.
//!
//! Provides edge-size types (`EdgeSizes`, `ResolvedBoxProps`, `PackedBoxProps`),
//! CSS value resolution (`UnresolvedMargin`, `UnresolvedEdge`, `ResolutionParams`),
//! intrinsic sizing (`IntrinsicSizes`), and writing-mode context (`WritingModeContext`).

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    ui_solver::ResolvedOffsets,
};
use azul_css::props::{
    basic::{pixel::PixelValue, PhysicalSize, PropertyContext, ResolutionContext, SizeMetric},
    layout::LayoutWritingMode,
    style::{StyleDirection, StyleTextOrientation},
};

#[derive(Copy, Debug, Clone, PartialEq, PartialOrd)]
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

// +spec:box-model:83b3b8 - Box dimensions: content area with optional padding, border, margin areas
/// Represents the four edges of a box for properties like margin, padding, border.
// +spec:box-model:3b155c - "4 values assigned to sides" pattern (top, right, bottom, left) matching margin/inset shorthands
// +spec:width-calculation:37f9e7 - CSS 2.2 §8.1 box dimensions: content, padding, border, margin areas with top/right/bottom/left segments
#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeSizes {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl EdgeSizes {
    /// Sum of horizontal edges (left + right).
    #[must_use] pub fn horizontal_sum(&self) -> f32 {
        self.left + self.right
    }

    /// Sum of vertical edges (top + bottom).
    #[must_use] pub fn vertical_sum(&self) -> f32 {
        self.top + self.bottom
    }

    // +spec:block-formatting-context:440282 - vertical writing modes use analogous layout via main/cross axis abstraction
    // +spec:block-formatting-context:a49f9e - line-relative directions mapped via writing mode
    // +spec:block-formatting-context:387117 - writing-mode property maps block flow to vertical/horizontal axes
    // +spec:box-model:4c01a3 - dimensional mapping: main=block axis, cross=inline axis per writing mode
    // +spec:box-model:4c1a9f - physical-to-logical mapping of margin/padding/border for vertical writing modes
    // +spec:box-model:9414ab - flow-relative mapping of box edges (margin/padding/border) per writing mode
    // +spec:inline-formatting-context:2de457 - block/inline dimension mapping via writing mode
    // +spec:inline-formatting-context:c6b91e - line-relative "over"/"under" mapped to physical top/bottom via writing mode
    // +spec:writing-modes:00a918 - Abstract-to-physical mappings for block/inline to top/right/bottom/left
    // +spec:writing-modes:14e6f0 - block-start/end depend only on writing-mode; inline-start/end also depend on direction (handled in positioning.rs)
    // +spec:writing-modes:1c2101 - Abstract directional terms (top/right/bottom/left) to logical axes (main/cross) based on writing-mode
    // +spec:writing-modes:1c5155 - line-relative mappings: over/under/line-left/line-right → top/bottom/left/right in horizontal-tb
    // +spec:writing-modes:70daf1 - block/inline axis mapping per writing-mode for edge sizes
    // +spec:writing-modes:f9af71 - flow-relative directions: block-start/end and inline-start/end mapped to physical edges
    // +spec:writing-modes:60b023 - abstract-to-physical mapping: block axis = main, inline axis = cross
    // +spec:writing-modes:829cd7 - flow-relative directions: block-start/end from writing-mode, inline-start/end from writing-mode+direction
    // +spec:writing-modes:a2113d - block/inline axis mapping for writing modes (block-axis, inline-axis, block-start/end, inline-start/end)
    // +spec:writing-modes:c0ae9c - abstract directional mappings from writing-mode/direction
    // +spec:writing-modes:c91130 - Abstract box terminology: block/inline axis mapping per writing-mode
    // +spec:writing-modes:cd31ce - flow-relative directions mapped to physical via writing mode
    // +spec:writing-modes:fd8c18 - block/inline axis mapping based on writing mode
    // +spec:writing-modes:0e549a - writing-mode computed value influences physical/logical axis mapping
    /// Returns the size of the edge at the start of the main/block axis.
    #[must_use] pub const fn main_start(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.top,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.left,
        }
    }

    /// Returns the size of the edge at the end of the main/block axis.
    #[must_use] pub const fn main_end(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.bottom,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.right,
        }
    }

    /// Returns the sum of the start and end sizes on the main/block axis.
    #[must_use] pub fn main_sum(&self, wm: LayoutWritingMode) -> f32 {
        self.main_start(wm) + self.main_end(wm)
    }

    // +spec:block-formatting-context:6225cb - line-relative directions: vertical modes map line-over/under to top/bottom
    /// Returns the size of the edge at the start of the cross/inline axis.
    #[must_use] pub const fn cross_start(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.left,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.top,
        }
    }

    /// Returns the size of the edge at the end of the cross/inline axis.
    #[must_use] pub const fn cross_end(&self, wm: LayoutWritingMode) -> f32 {
        match wm {
            LayoutWritingMode::HorizontalTb => self.right,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => self.bottom,
        }
    }

    /// Returns the sum of the start and end sizes on the cross/inline axis.
    #[must_use] pub fn cross_sum(&self, wm: LayoutWritingMode) -> f32 {
        self.cross_start(wm) + self.cross_end(wm)
    }
}

// ============================================================================
// UNRESOLVED VALUE TYPES (for lazy resolution during layout)
// ============================================================================

/// An unresolved CSS margin value.
// +spec:box-model:ff1730 - margin properties apply to both continuous and paged media
///
/// Margins can be `auto` (for centering) or a length value that needs
/// resolution against the containing block.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
    #[must_use] pub const fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }

    /// Resolve this margin value to pixels.
    ///
    /// - `Auto` returns 0.0 (actual auto margin calculation happens in layout)
    /// - `Zero` returns 0.0
    /// - `Length` is resolved using the resolution context
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    #[must_use] pub fn resolve(&self, ctx: &ResolutionContext) -> f32 {
        match self {
            Self::Zero => 0.0,
            // +spec:box-model:c921aa - auto margin-top/bottom used value is 0 for block-level non-replaced elements in normal flow
            // +spec:box-model:e25fdc - auto margins treated as zero for abspos size computation
            Self::Auto => 0.0, // Auto is handled separately in layout
            Self::Length(pv) => pv.resolve_with_context(ctx, PropertyContext::Margin),
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
    pub const fn new(top: T, right: T, bottom: T, left: T) -> Self {
        Self { top, right, bottom, left }
    }
}

impl UnresolvedEdge<UnresolvedMargin> {
    /// Resolve all margin edges to pixel values.
    #[must_use] pub fn resolve(&self, ctx: &ResolutionContext) -> EdgeSizes {
        EdgeSizes {
            top: self.top.resolve(ctx),
            right: self.right.resolve(ctx),
            bottom: self.bottom.resolve(ctx),
            left: self.left.resolve(ctx),
        }
    }

    /// Extract which margins are set to `auto`.
    #[must_use] pub const fn get_margin_auto(&self) -> MarginAuto {
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
    #[must_use] pub fn resolve(&self, ctx: &ResolutionContext, prop_ctx: PropertyContext) -> EdgeSizes {
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
    // +spec:inline-formatting-context:26c933 - LogicalSize maps inline/block dimensions to physical width/height per writing mode
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
    /// Create a `ResolutionContext` from these parameters.
    #[must_use] pub const fn to_resolution_context(&self) -> ResolutionContext {
        ResolutionContext {
            element_font_size: self.element_font_size,
            // For non-font properties, `em` resolves against the element's own
            // computed font-size, so parent_font_size == element_font_size here.
            // Do NOT use this context for font-size resolution itself.
            parent_font_size: self.element_font_size,
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
    #[must_use] pub fn resolve(&self, params: &ResolutionParams) -> ResolvedBoxProps {
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
#[allow(clippy::struct_excessive_bools)] // one independent bool per margin edge (auto flags)
pub struct MarginAuto {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

/// A fully resolved representation of a node's box model properties.
// +spec:box-model:3e083b - content/padding/border/margin box model layers
// +spec:box-model:a227ff - content/padding/border/margin edges defining box extents for overflow
// +spec:containing-block:bca691 - box model edges: padding/border/margin boxes with content-box, padding-box, margin-box methods
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
    // +spec:box-model:be08c6 - inner size (content-box) from outer size minus border+padding, floored at zero
    // +spec:writing-modes:a58616 - abstract dimensions: inline size maps to physical width/height per writing-mode
    /// Calculates the inner content-box size from an outer border-box size,
    /// correctly accounting for the specified writing mode.
    #[must_use] pub fn inner_size(&self, outer_size: LogicalSize, wm: LayoutWritingMode) -> LogicalSize {
        let outer_main = outer_size.main(wm);
        let outer_cross = outer_size.cross(wm);

        // The sum of padding and border along the cross (inline) axis.
        let cross_axis_spacing = self.padding.cross_sum(wm) + self.border.cross_sum(wm);

        // The sum of padding and border along the main (block) axis.
        let main_axis_spacing = self.padding.main_sum(wm) + self.border.main_sum(wm);

        // +spec:box-model:2589b1 - content size = border-box - border - padding, floored at zero
        // +spec:box-model:3ab53d - if padding+border > border-box, content floors at 0px
        let inner_main = (outer_main - main_axis_spacing).max(0.0);
        let inner_cross = (outer_cross - cross_axis_spacing).max(0.0);

        LogicalSize::from_main_cross(inner_main, inner_cross, wm)
    }

    // +spec:box-model:aa585e - Content/padding/border/margin edge relationships
    // +spec:height-calculation:6c9abb - box model edges: margin > border > padding > content
    /// Returns the content-box rect from a border-box rect.
    /// Shrinks inward by border + padding on each side.
    // +spec:box-model:1720a5 - content of a block box is confined to its content edges
    #[must_use] pub fn content_box(&self, border_box: LogicalRect) -> LogicalRect {
        let x = border_box.origin.x + self.border.left + self.padding.left;
        let y = border_box.origin.y + self.border.top + self.padding.top;
        let w = (border_box.size.width - self.border.horizontal_sum() - self.padding.horizontal_sum()).max(0.0);
        let h = (border_box.size.height - self.border.vertical_sum() - self.padding.vertical_sum()).max(0.0);
        LogicalRect { origin: LogicalPosition { x, y }, size: LogicalSize { width: w, height: h } }
    }

    /// Returns the padding-box rect from a border-box rect.
    /// Shrinks inward by border on each side.
    #[must_use] pub fn padding_box(&self, border_box: LogicalRect) -> LogicalRect {
        let x = border_box.origin.x + self.border.left;
        let y = border_box.origin.y + self.border.top;
        let w = (border_box.size.width - self.border.horizontal_sum()).max(0.0);
        let h = (border_box.size.height - self.border.vertical_sum()).max(0.0);
        LogicalRect { origin: LogicalPosition { x, y }, size: LogicalSize { width: w, height: h } }
    }

    /// Returns the margin-box rect from a border-box rect.
    /// Expands outward by margin on each side.
    #[must_use] pub fn margin_box(&self, border_box: LogicalRect) -> LogicalRect {
        let x = border_box.origin.x - self.margin.left;
        let y = border_box.origin.y - self.margin.top;
        let w = border_box.size.width + self.margin.horizontal_sum();
        let h = border_box.size.height + self.margin.vertical_sum();
        LogicalRect { origin: LogicalPosition { x, y }, size: LogicalSize { width: w, height: h } }
    }

    // +spec:box-model:0e75c1 - margin, padding, border contribute to layout bounds (default line-fit-edge: leading uses line-height model)
    /// Total horizontal space consumed by margin + border + padding.
    #[must_use] pub fn horizontal_mbp(&self) -> f32 {
        self.margin.horizontal_sum() + self.border.horizontal_sum() + self.padding.horizontal_sum()
    }

    /// Total vertical space consumed by margin + border + padding.
    #[must_use] pub fn vertical_mbp(&self) -> f32 {
        self.margin.vertical_sum() + self.border.vertical_sum() + self.padding.vertical_sum()
    }

    /// Total horizontal space consumed by border + padding only (no margin).
    #[must_use] pub fn horizontal_bp(&self) -> f32 {
        self.border.horizontal_sum() + self.padding.horizontal_sum()
    }

    /// Total vertical space consumed by border + padding only (no margin).
    #[must_use] pub fn vertical_bp(&self) -> f32 {
        self.border.vertical_sum() + self.padding.vertical_sum()
    }
}

/// Type alias for backwards compatibility.
/// TODO: Remove this once all code uses `ResolvedBoxProps` directly.
pub type BoxProps = ResolvedBoxProps;

/// Packed representation of box model properties using i16×10 encoding.
///
/// Stores margin/padding/border as i16 values scaled by 10 (0.1px precision),
/// reducing the hot struct from 52B to 26B. Range: ±3276.7px per edge.
///
/// Only used for storage in `LayoutNodeHot`. The layout solver unpacks to
/// `ResolvedBoxProps` (f32) for computation.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct PackedBoxProps {
    pub margin: [i16; 4],     // top, right, bottom, left — ×10
    pub padding: [i16; 4],    // ×10
    pub border: [i16; 4],     // ×10
    pub margin_auto: MarginAuto,
}

impl PackedBoxProps {
    /// Pack a `ResolvedBoxProps` into compact i16×10 encoding.
    #[inline]
    #[must_use] pub fn pack(bp: &ResolvedBoxProps) -> Self {
        Self {
            margin: Self::pack_edge(&bp.margin),
            padding: Self::pack_edge(&bp.padding),
            border: Self::pack_edge(&bp.border),
            margin_auto: bp.margin_auto,
        }
    }

    /// Unpack to full `ResolvedBoxProps` with f32 values.
    #[inline]
    #[must_use] pub fn unpack(&self) -> ResolvedBoxProps {
        ResolvedBoxProps {
            margin: Self::unpack_edge(&self.margin),
            padding: Self::unpack_edge(&self.padding),
            border: Self::unpack_edge(&self.border),
            margin_auto: self.margin_auto,
        }
    }

    /// Convenience: unpack and call `inner_size` on the result.
    #[inline]
    #[must_use] pub fn inner_size(&self, outer_size: LogicalSize, wm: LayoutWritingMode) -> LogicalSize {
        self.unpack().inner_size(outer_size, wm)
    }

    /// Convenience: unpack and call `content_box` on the result.
    #[inline]
    #[must_use] pub fn content_box(&self, border_box: LogicalRect) -> LogicalRect {
        self.unpack().content_box(border_box)
    }

    /// Convenience: unpack and call `padding_box` on the result.
    #[inline]
    #[must_use] pub fn padding_box(&self, border_box: LogicalRect) -> LogicalRect {
        self.unpack().padding_box(border_box)
    }

    /// Convenience: unpack and call `margin_box` on the result.
    #[inline]
    #[must_use] pub fn margin_box(&self, border_box: LogicalRect) -> LogicalRect {
        self.unpack().margin_box(border_box)
    }

    /// Convenience: unpack and return horizontal MBP.
    #[inline]
    #[must_use] pub fn horizontal_mbp(&self) -> f32 {
        self.unpack().horizontal_mbp()
    }

    /// Convenience: unpack and return vertical MBP.
    #[inline]
    #[must_use] pub fn vertical_mbp(&self) -> f32 {
        self.unpack().vertical_mbp()
    }

    /// Convenience: unpack and return horizontal BP.
    #[inline]
    #[must_use] pub fn horizontal_bp(&self) -> f32 {
        self.unpack().horizontal_bp()
    }

    /// Convenience: unpack and return vertical BP.
    #[inline]
    #[must_use] pub fn vertical_bp(&self) -> f32 {
        self.unpack().vertical_bp()
    }

    #[inline]
    #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/counter/fixed-point cast
    fn pack_edge(e: &EdgeSizes) -> [i16; 4] {
        const MIN: f32 = i16::MIN as f32;
        const MAX: f32 = i16::MAX as f32;
        [
            (e.top * 10.0).round().clamp(MIN, MAX) as i16,
            (e.right * 10.0).round().clamp(MIN, MAX) as i16,
            (e.bottom * 10.0).round().clamp(MIN, MAX) as i16,
            (e.left * 10.0).round().clamp(MIN, MAX) as i16,
        ]
    }

    #[inline]
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
    fn unpack_edge(e: &[i16; 4]) -> EdgeSizes {
        EdgeSizes {
            top: f32::from(e[0]) * 0.1,
            right: f32::from(e[1]) * 0.1,
            bottom: f32::from(e[2]) * 0.1,
            left: f32::from(e[3]) * 0.1,
        }
    }
}

// Re-export float and clear types from azul_css
pub use azul_css::props::layout::{LayoutClear, LayoutFloat};

// +spec:intrinsic-sizing:af39b6 - min-content, max-content, and stretch fit size definitions
// min-content constraint, max-content constraint definitions
// and fit-content sizes for both inline and block axes
// +spec:height-calculation:e9ec84 - replaced elements have natural dimensions (width, height, ratio)
/// Represents the intrinsic sizing information for an element, calculated
/// without knowledge of the final containing block size.
// +spec:intrinsic-sizing:127a10 - min-content, max-content, fit-content size definitions (css-sizing-3 §2.1)
// +spec:intrinsic-sizing:21f2cb - defines min-content, max-content, and stretch-fit size terminology
// +spec:width-calculation:1583c4 - min-content, max-content, fit-content intrinsic size definitions (§2.1)
#[derive(Debug, Clone, Copy, Default)]
pub struct IntrinsicSizes {
    // +spec:width-calculation:b83d0a - min-content width ("preferred minimum width" in CSS2.1§10.3.5)
    // +spec:writing-modes:1583c4 - min-content size in inline axis = size fitting contents with all soft wraps taken
    /// §2.1 min-content inline size: inline size fitting contents if all soft wraps taken.
    pub min_content_width: f32,
    // +spec:width-calculation:0c74d3 - max-content width ("preferred width" in CSS2.1§10.3.5)
    // +spec:writing-modes:6e85d3 - max-content inline size is the "ideal" size in the inline axis (writing-mode-dependent)
    /// §2.1 max-content inline size: narrowest inline size if no soft wraps taken.
    pub max_content_width: f32,
    /// The width specified by CSS properties, if any.
    pub preferred_width: Option<f32>,
    /// §2.1 min-content block size: for block containers, tables, and inline boxes,
    /// equivalent to max-content block size.
    pub min_content_height: f32,
    // +spec:writing-modes:8c94e2 - max-content block size is the "ideal" block size after layout
    /// §2.1 max-content block size: "ideal" block size, usually content height after layout.
    pub max_content_height: f32,
    /// The height specified by CSS properties, if any.
    pub preferred_height: Option<f32>,
}

// ============================================================================
// WRITING MODE SUPPORT
// ============================================================================

/// Captures the resolved writing mode context for a node.
///
/// This struct bundles together all the CSS properties that affect how
/// logical directions (inline/block) map to physical directions (x/y).
/// Spec agents should use this struct to implement writing-mode-aware layout.
///
/// # CSS Writing Modes Level 4
///
/// - `writing-mode` determines the block flow direction and inline base direction
/// - `direction` determines the inline base direction (ltr or rtl)
/// - `text-orientation` determines glyph orientation in vertical writing modes
// +spec:block-formatting-context:333dcb - typographic mode captured by text_orientation field
// +spec:block-formatting-context:66eb6d - text-orientation property (mixed|upright|sideways) integrated into WritingModeContext
// +spec:block-formatting-context:8be1b0 - writing modes and vertical text orientation context (UTN#22)
// +spec:display-property:0a39dc - text-orientation affects inline-level alignment via WritingModeContext
// +spec:display-property:591355 - bidirectionality support via direction property in WritingModeContext
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WritingModeContext {
    pub writing_mode: LayoutWritingMode,
    pub direction: StyleDirection,
    // +spec:block-formatting-context:925cfe - text-orientation mixed/upright for horizontal scripts in vertical mode
    pub text_orientation: StyleTextOrientation,
}

impl Default for WritingModeContext {
    fn default() -> Self {
        Self {
            writing_mode: LayoutWritingMode::HorizontalTb,
            direction: StyleDirection::Ltr,
            text_orientation: StyleTextOrientation::Mixed,
        }
    }
}

impl WritingModeContext {
    /// Constructs a `WritingModeContext`, applying spec-mandated overrides.
    // +spec:writing-modes:8307e4 - text-orientation: upright forces used direction to ltr
    #[must_use] pub fn new(
        writing_mode: LayoutWritingMode,
        direction: StyleDirection,
        text_orientation: StyleTextOrientation,
    ) -> Self {
        // CSS Writing Modes Level 4 §5.1: text-orientation: upright causes
        // the used value of direction to be ltr, and all characters to be
        // treated as strong LTR for bidi reordering purposes.
        let used_direction = if text_orientation == StyleTextOrientation::Upright {
            StyleDirection::Ltr
        } else {
            direction
        };
        Self {
            writing_mode,
            direction: used_direction,
            text_orientation,
        }
    }

    // +spec:writing-modes:458d31 - text-orientation:upright forces used direction to ltr
    /// Returns the used value of `direction`.
    ///
    /// The upright override is already applied in `new()`, so this just
    /// returns the stored direction.
    #[must_use] pub const fn used_direction(&self) -> StyleDirection {
        self.direction
    }

    // +spec:containing-block:c205e5 - orthogonal flow: child writing mode perpendicular to containing block's

    // +spec:block-formatting-context:6225cb - vertical writing modes: line-over is right, line-under is left
    // +spec:block-formatting-context:9a4269 - vertical vs horizontal script classification
    /// Returns true if the writing mode is horizontal (`HorizontalTb`).
    ///
    /// When true, the inline axis is horizontal and the block axis is vertical.
    #[must_use] pub const fn is_horizontal(&self) -> bool {
        matches!(self.writing_mode, LayoutWritingMode::HorizontalTb)
    }

    /// Returns true if the inline size corresponds to the physical width.
    ///
    /// In horizontal writing modes, inline size = width.
    /// In vertical writing modes, inline size = height.
    // +spec:block-formatting-context:bb9845 - orthogonal flows: inline/block axis mapping
    #[must_use] pub const fn inline_size_is_width(&self) -> bool {
        self.is_horizontal()
    }

    /// Returns true if the block size corresponds to the physical height.
    ///
    /// In horizontal writing modes, block size = height.
    /// In vertical writing modes, block size = width.
    #[must_use] pub const fn block_size_is_height(&self) -> bool {
        self.is_horizontal()
    }

    // +spec:writing-modes:32541a - direction property controls inline text direction via stylesheet
    /// Returns true if the inline direction is reversed (RTL in horizontal,
    /// or bottom-to-top in certain vertical modes).
    #[must_use] pub fn is_inline_reversed(&self) -> bool {
        self.used_direction() == StyleDirection::Rtl
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)]
mod autotest_generated {
    use super::*;

    // ---------------------------------------------------------------- helpers

    /// All writing modes, so every mapping test can be run exhaustively.
    const ALL_WM: [LayoutWritingMode; 3] = [
        LayoutWritingMode::HorizontalTb,
        LayoutWritingMode::VerticalRl,
        LayoutWritingMode::VerticalLr,
    ];

    fn edges(top: f32, right: f32, bottom: f32, left: f32) -> EdgeSizes {
        EdgeSizes { top, right, bottom, left }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition { x, y },
            size: LogicalSize { width: w, height: h },
        }
    }

    /// `EdgeSizes`/`ResolvedBoxProps` carry no `PartialEq`, and the packed
    /// encoding is lossy by design, so float comparisons need a tolerance.
    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    fn params(cb: LogicalSize, vp: LogicalSize, font: f32, root: f32) -> ResolutionParams {
        ResolutionParams {
            containing_block: cb,
            viewport_size: vp,
            element_font_size: font,
            root_font_size: root,
        }
    }

    /// 800x600 containing block, 1000x500 viewport, 16px element / 10px root font.
    /// Every dimension is distinct so a transposed axis cannot pass by accident.
    fn distinct_params() -> ResolutionParams {
        params(
            LogicalSize::new(800.0, 600.0),
            LogicalSize::new(1000.0, 500.0),
            16.0,
            10.0,
        )
    }

    fn props(margin: EdgeSizes, padding: EdgeSizes, border: EdgeSizes) -> ResolvedBoxProps {
        ResolvedBoxProps { margin, padding, border, margin_auto: MarginAuto::default() }
    }

    // ================================================================
    // EdgeSizes: sums
    // ================================================================

    #[test]
    fn edge_sizes_sums_pick_the_right_pair_of_edges() {
        let e = edges(1.0, 2.0, 4.0, 8.0); // top, right, bottom, left
        assert_eq!(e.horizontal_sum(), 10.0, "horizontal = left + right");
        assert_eq!(e.vertical_sum(), 5.0, "vertical = top + bottom");
    }

    #[test]
    fn edge_sizes_default_is_all_zero() {
        let e = EdgeSizes::default();
        assert_eq!(e.horizontal_sum(), 0.0);
        assert_eq!(e.vertical_sum(), 0.0);
        for wm in ALL_WM {
            assert_eq!(e.main_sum(wm), 0.0);
            assert_eq!(e.cross_sum(wm), 0.0);
        }
    }

    #[test]
    fn edge_sizes_sums_saturate_to_infinity_instead_of_panicking() {
        // f32::MAX + f32::MAX overflows to +inf; it must not panic or wrap.
        let e = edges(f32::MAX, f32::MAX, f32::MAX, f32::MAX);
        assert!(e.horizontal_sum().is_infinite() && e.horizontal_sum() > 0.0);
        assert!(e.vertical_sum().is_infinite() && e.vertical_sum() > 0.0);
    }

    #[test]
    fn edge_sizes_opposing_infinities_produce_nan_not_a_panic() {
        // inf + (-inf) is NaN by IEEE-754. The point is that it is deterministic
        // and does not trap — nothing downstream may assume a finite sum.
        let e = edges(f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        assert!(e.horizontal_sum().is_nan());
        assert!(e.vertical_sum().is_nan());
    }

    #[test]
    fn edge_sizes_nan_edges_propagate_without_panicking() {
        let e = edges(f32::NAN, 1.0, 2.0, 3.0);
        assert!(e.vertical_sum().is_nan());
        assert_eq!(e.horizontal_sum(), 4.0, "a NaN top must not poison left+right");
        for wm in ALL_WM {
            let _ = e.main_sum(wm);
            let _ = e.cross_sum(wm);
        }
    }

    // ================================================================
    // EdgeSizes: writing-mode axis mapping
    // ================================================================

    #[test]
    fn edge_sizes_axis_mapping_is_exact_for_every_writing_mode() {
        let e = edges(1.0, 2.0, 4.0, 8.0); // top, right, bottom, left

        // horizontal-tb: block (main) axis is vertical, inline (cross) axis is horizontal.
        let h = LayoutWritingMode::HorizontalTb;
        assert_eq!(e.main_start(h), 1.0, "main-start = top");
        assert_eq!(e.main_end(h), 4.0, "main-end = bottom");
        assert_eq!(e.cross_start(h), 8.0, "cross-start = left");
        assert_eq!(e.cross_end(h), 2.0, "cross-end = right");

        // vertical-*: block (main) axis is horizontal, inline (cross) axis is vertical.
        for wm in [LayoutWritingMode::VerticalRl, LayoutWritingMode::VerticalLr] {
            assert_eq!(e.main_start(wm), 8.0, "main-start = left");
            assert_eq!(e.main_end(wm), 2.0, "main-end = right");
            assert_eq!(e.cross_start(wm), 1.0, "cross-start = top");
            assert_eq!(e.cross_end(wm), 4.0, "cross-end = bottom");
        }
    }

    #[test]
    fn edge_sizes_main_and_cross_sums_partition_the_four_edges() {
        // Whatever the writing mode, main+cross must account for each edge exactly
        // once — a transposition bug in one arm would break this identity.
        let e = edges(1.0, 2.0, 4.0, 8.0);
        for wm in ALL_WM {
            assert_eq!(e.main_start(wm) + e.main_end(wm), e.main_sum(wm));
            assert_eq!(e.cross_start(wm) + e.cross_end(wm), e.cross_sum(wm));
            assert_eq!(
                e.main_sum(wm) + e.cross_sum(wm),
                e.horizontal_sum() + e.vertical_sum(),
                "{wm:?}: main+cross must cover all four edges"
            );
        }
    }

    #[test]
    fn edge_sizes_axis_accessors_survive_extreme_values() {
        let e = edges(f32::MAX, f32::MIN, f32::INFINITY, f32::NEG_INFINITY);
        for wm in ALL_WM {
            let _ = e.main_start(wm);
            let _ = e.main_end(wm);
            let _ = e.cross_start(wm);
            let _ = e.cross_end(wm);
            let _ = e.main_sum(wm);
            let _ = e.cross_sum(wm);
        }
    }

    // ================================================================
    // UnresolvedMargin
    // ================================================================

    #[test]
    fn unresolved_margin_is_auto_only_for_the_auto_variant() {
        assert!(UnresolvedMargin::Auto.is_auto());
        assert!(!UnresolvedMargin::Zero.is_auto());
        assert!(!UnresolvedMargin::Length(PixelValue::const_px(10)).is_auto());
        // A zero-valued length is still *not* `auto`.
        assert!(!UnresolvedMargin::Length(PixelValue::zero()).is_auto());
        assert!(!UnresolvedMargin::default().is_auto(), "default is Zero, not Auto");
    }

    #[test]
    fn unresolved_margin_auto_and_zero_both_resolve_to_zero_px() {
        let ctx = distinct_params().to_resolution_context();
        assert_eq!(UnresolvedMargin::Zero.resolve(&ctx), 0.0);
        // `auto` is resolved later by the layout algorithm; here it must read as 0.
        assert_eq!(UnresolvedMargin::Auto.resolve(&ctx), 0.0);
    }

    #[test]
    fn unresolved_margin_length_resolves_by_metric() {
        let ctx = distinct_params().to_resolution_context(); // 800x600 cb, 16px font, 10px root
        let r = |pv: PixelValue| UnresolvedMargin::Length(pv).resolve(&ctx);

        assert_eq!(r(PixelValue::px(12.5)), 12.5);
        assert_eq!(r(PixelValue::em(2.0)), 32.0, "em = 2 x element font-size");
        assert_eq!(r(PixelValue::rem(2.0)), 20.0, "rem = 2 x root font-size");
    }

    #[test]
    fn unresolved_margin_percent_resolves_against_containing_block_width() {
        // CSS 2.1 §8.3: percentage margins ALWAYS refer to the containing block
        // *width* — never the height. With a 800x600 block, 50% must be 400, not 300.
        let ctx = distinct_params().to_resolution_context();
        let got = UnresolvedMargin::Length(PixelValue::percent(50.0)).resolve(&ctx);
        assert_eq!(got, 400.0);
        assert_ne!(got, 300.0, "percentage margin must not use the block height");
    }

    #[test]
    fn unresolved_margin_nan_length_resolves_to_zero_not_nan() {
        // PixelValue stores a fixed-point isize, and `f32 as isize` saturates NaN
        // to 0 — so a NaN can never escape into layout as a NaN margin.
        let ctx = distinct_params().to_resolution_context();
        let got = UnresolvedMargin::Length(PixelValue::px(f32::NAN)).resolve(&ctx);
        assert!(!got.is_nan(), "a NaN px value must not survive resolution");
        assert_eq!(got, 0.0);
    }

    #[test]
    fn unresolved_margin_huge_length_saturates_to_a_finite_value() {
        let ctx = distinct_params().to_resolution_context();
        for v in [f32::MAX, f32::MIN, f32::INFINITY, f32::NEG_INFINITY] {
            let got = UnresolvedMargin::Length(PixelValue::px(v)).resolve(&ctx);
            assert!(got.is_finite(), "px({v}) resolved to a non-finite {got}");
        }
    }

    #[test]
    fn unresolved_margin_percent_of_a_nan_containing_block_is_nan_not_a_panic() {
        // The containing block is a raw f32 and is NOT sanitized, so a NaN block
        // size *does* propagate. Documenting the real behaviour: deterministic NaN,
        // no panic — the guard has to live at the caller, not here.
        let p = params(
            LogicalSize::new(f32::NAN, f32::NAN),
            LogicalSize::new(1000.0, 500.0),
            16.0,
            10.0,
        );
        let got = UnresolvedMargin::Length(PixelValue::percent(50.0)).resolve(&p.to_resolution_context());
        assert!(got.is_nan());
    }

    // ================================================================
    // UnresolvedEdge
    // ================================================================

    #[test]
    fn unresolved_edge_new_assigns_fields_in_top_right_bottom_left_order() {
        // The classic CSS shorthand transposition bug: argument order is TRBL.
        let e = UnresolvedEdge::new(1_u8, 2, 4, 8);
        assert_eq!(e.top, 1);
        assert_eq!(e.right, 2);
        assert_eq!(e.bottom, 4);
        assert_eq!(e.left, 8);
    }

    #[test]
    fn unresolved_edge_new_accepts_extreme_payloads() {
        let e = UnresolvedEdge::new(f32::NAN, f32::INFINITY, f32::MAX, f32::MIN);
        assert!(e.top.is_nan());
        assert!(e.right.is_infinite());
        assert_eq!(e.bottom, f32::MAX);
        assert_eq!(e.left, f32::MIN);
    }

    #[test]
    fn get_margin_auto_flags_exactly_the_auto_sides() {
        let e = UnresolvedEdge::new(
            UnresolvedMargin::Zero,                                 // top
            UnresolvedMargin::Auto,                                 // right
            UnresolvedMargin::Length(PixelValue::const_px(5)),      // bottom
            UnresolvedMargin::Auto,                                 // left
        );
        let a = e.get_margin_auto();
        assert!(!a.top);
        assert!(a.right);
        assert!(!a.bottom);
        assert!(a.left);
    }

    #[test]
    fn get_margin_auto_on_default_edge_flags_nothing() {
        let a = UnresolvedEdge::<UnresolvedMargin>::default().get_margin_auto();
        assert!(!a.top && !a.right && !a.bottom && !a.left);
    }

    #[test]
    fn unresolved_margin_edge_resolve_keeps_each_side_separate() {
        let ctx = distinct_params().to_resolution_context();
        let e = UnresolvedEdge::new(
            UnresolvedMargin::Length(PixelValue::px(1.0)),   // top
            UnresolvedMargin::Length(PixelValue::px(2.0)),   // right
            UnresolvedMargin::Auto,                          // bottom -> 0
            UnresolvedMargin::Length(PixelValue::px(8.0)),   // left
        );
        let r = e.resolve(&ctx);
        assert_eq!(r.top, 1.0);
        assert_eq!(r.right, 2.0);
        assert_eq!(r.bottom, 0.0, "auto resolves to 0 px here");
        assert_eq!(r.left, 8.0);
    }

    #[test]
    fn pixel_edge_resolve_drops_percentages_on_border_width() {
        // CSS Backgrounds 3 §4.1: `%` is not a valid border-width. The resolver
        // must yield 0 rather than silently resolving against the containing block.
        let ctx = distinct_params().to_resolution_context();
        let e = UnresolvedEdge::new(
            PixelValue::percent(50.0),
            PixelValue::percent(50.0),
            PixelValue::percent(50.0),
            PixelValue::percent(50.0),
        );
        let r = e.resolve(&ctx, PropertyContext::BorderWidth);
        assert_eq!(r.top, 0.0);
        assert_eq!(r.right, 0.0);
        assert_eq!(r.bottom, 0.0);
        assert_eq!(r.left, 0.0);
    }

    #[test]
    fn pixel_edge_resolve_uses_block_width_for_vertical_padding_percentages() {
        // CSS 2.1 §8.4: padding-top/bottom percentages also refer to the containing
        // block WIDTH. With 800x600, every side must land on 80, never 60.
        let ctx = distinct_params().to_resolution_context();
        let e = UnresolvedEdge::new(
            PixelValue::percent(10.0),
            PixelValue::percent(10.0),
            PixelValue::percent(10.0),
            PixelValue::percent(10.0),
        );
        let r = e.resolve(&ctx, PropertyContext::Padding);
        assert_eq!(r.top, 80.0, "padding-top % must use the width");
        assert_eq!(r.bottom, 80.0, "padding-bottom % must use the width");
        assert_eq!(r.left, 80.0);
        assert_eq!(r.right, 80.0);
    }

    #[test]
    fn pixel_edge_resolve_of_viewport_units_uses_the_viewport_not_the_block() {
        let ctx = distinct_params().to_resolution_context(); // viewport 1000x500
        let e = UnresolvedEdge::new(
            PixelValue::from_metric(SizeMetric::Vw, 10.0),
            PixelValue::from_metric(SizeMetric::Vh, 10.0),
            PixelValue::from_metric(SizeMetric::Vmin, 10.0),
            PixelValue::from_metric(SizeMetric::Vmax, 10.0),
        );
        let r = e.resolve(&ctx, PropertyContext::Padding);
        assert_eq!(r.top, 100.0, "10vw of 1000");
        assert_eq!(r.right, 50.0, "10vh of 500");
        assert_eq!(r.bottom, 50.0, "10vmin of min(1000,500)");
        assert_eq!(r.left, 100.0, "10vmax of max(1000,500)");
    }

    // ================================================================
    // ResolutionParams
    // ================================================================

    #[test]
    fn to_resolution_context_maps_every_field() {
        let ctx = distinct_params().to_resolution_context();
        assert_eq!(ctx.element_font_size, 16.0);
        assert_eq!(ctx.root_font_size, 10.0);
        assert_eq!(ctx.containing_block_size.width, 800.0);
        assert_eq!(ctx.containing_block_size.height, 600.0);
        assert_eq!(ctx.viewport_size.width, 1000.0);
        assert_eq!(ctx.viewport_size.height, 500.0);
        assert!(ctx.element_size.is_none(), "element size is unknown pre-layout");
    }

    #[test]
    fn to_resolution_context_aliases_parent_font_size_onto_the_element_font_size() {
        // Documented quirk: for non-font properties `em` resolves against the
        // element's OWN font-size, so the context deliberately reports
        // parent == element. Using it for font-size resolution would be wrong.
        let ctx = distinct_params().to_resolution_context();
        assert_eq!(ctx.parent_font_size, ctx.element_font_size);
        assert_eq!(ctx.parent_font_size, 16.0);
    }

    #[test]
    fn to_resolution_context_passes_extreme_values_through_unchanged() {
        let p = params(
            LogicalSize::new(f32::MAX, f32::NAN),
            LogicalSize::new(f32::INFINITY, 0.0),
            0.0,
            f32::MIN,
        );
        let ctx = p.to_resolution_context();
        assert_eq!(ctx.containing_block_size.width, f32::MAX);
        assert!(ctx.containing_block_size.height.is_nan());
        assert!(ctx.viewport_size.width.is_infinite());
        assert_eq!(ctx.element_font_size, 0.0);
        assert_eq!(ctx.root_font_size, f32::MIN);
    }

    #[test]
    fn zero_font_size_context_resolves_em_to_zero_without_dividing_by_it() {
        let p = params(LogicalSize::zero(), LogicalSize::zero(), 0.0, 0.0);
        let ctx = p.to_resolution_context();
        assert_eq!(PixelValue::em(10.0).resolve_with_context(&ctx, PropertyContext::Margin), 0.0);
        assert_eq!(PixelValue::rem(10.0).resolve_with_context(&ctx, PropertyContext::Margin), 0.0);
        // A zero-sized viewport must not turn vmin/vmax into NaN.
        let vmin = PixelValue::from_metric(SizeMetric::Vmin, 50.0);
        assert_eq!(vmin.resolve_with_context(&ctx, PropertyContext::Padding), 0.0);
    }

    // ================================================================
    // UnresolvedBoxProps
    // ================================================================

    #[test]
    fn unresolved_box_props_default_resolves_to_an_all_zero_box() {
        let r = UnresolvedBoxProps::default().resolve(&distinct_params());
        assert_eq!(r.horizontal_mbp(), 0.0);
        assert_eq!(r.vertical_mbp(), 0.0);
        assert!(!r.margin_auto.left && !r.margin_auto.right);
    }

    #[test]
    fn unresolved_box_props_resolve_applies_the_right_property_context_per_edge() {
        let p = distinct_params(); // 800x600 block
        let b = UnresolvedBoxProps {
            margin: UnresolvedEdge::new(
                UnresolvedMargin::Auto,
                UnresolvedMargin::Length(PixelValue::percent(10.0)), // -> 80 (width)
                UnresolvedMargin::Zero,
                UnresolvedMargin::Auto,
            ),
            padding: UnresolvedEdge::new(
                PixelValue::percent(10.0), // -> 80 (width, even on the top edge)
                PixelValue::px(4.0),
                PixelValue::px(4.0),
                PixelValue::px(4.0),
            ),
            border: UnresolvedEdge::new(
                PixelValue::percent(10.0), // -> 0 (percent is invalid on border-width)
                PixelValue::px(2.0),
                PixelValue::px(2.0),
                PixelValue::px(2.0),
            ),
        };
        let r = b.resolve(&p);

        assert_eq!(r.margin.top, 0.0, "auto margin resolves to 0 px");
        assert_eq!(r.margin.right, 80.0);
        assert_eq!(r.padding.top, 80.0);
        assert_eq!(r.border.top, 0.0, "percent border-width must collapse to 0");
        assert_eq!(r.border.left, 2.0);

        // The auto flags must survive resolution — they are what drives centering.
        assert!(r.margin_auto.top);
        assert!(r.margin_auto.left);
        assert!(!r.margin_auto.right);
        assert!(!r.margin_auto.bottom);
    }

    // ================================================================
    // ResolvedBoxProps::inner_size
    // ================================================================

    #[test]
    fn inner_size_of_a_zero_box_is_zero() {
        let bp = ResolvedBoxProps::default();
        for wm in ALL_WM {
            let s = bp.inner_size(LogicalSize::zero(), wm);
            assert_eq!(s.width, 0.0);
            assert_eq!(s.height, 0.0);
        }
    }

    #[test]
    fn inner_size_subtracts_border_and_padding_but_not_margin() {
        let bp = props(
            edges(100.0, 100.0, 100.0, 100.0), // margin — must be ignored
            edges(1.0, 2.0, 4.0, 8.0),         // padding
            edges(10.0, 20.0, 30.0, 40.0),     // border
        );
        // width  loses left+right: (8+2) + (40+20) = 70
        // height loses top+bottom: (1+4) + (10+30) = 45
        let s = bp.inner_size(LogicalSize::new(200.0, 100.0), LayoutWritingMode::HorizontalTb);
        assert_eq!(s.width, 130.0);
        assert_eq!(s.height, 55.0);
    }

    #[test]
    fn inner_size_is_identical_in_every_writing_mode() {
        // Physically, content-box = border-box minus the same four edges regardless
        // of writing mode. The main/cross indirection must cancel out exactly; a
        // transposed arm in one mode would show up right here.
        let bp = props(
            EdgeSizes::default(),
            edges(1.0, 2.0, 4.0, 8.0),
            edges(10.0, 20.0, 30.0, 40.0),
        );
        let outer = LogicalSize::new(200.0, 100.0);
        let base = bp.inner_size(outer, LayoutWritingMode::HorizontalTb);
        for wm in ALL_WM {
            let s = bp.inner_size(outer, wm);
            assert_eq!(s.width, base.width, "{wm:?} width diverged");
            assert_eq!(s.height, base.height, "{wm:?} height diverged");
        }
    }

    #[test]
    fn inner_size_floors_at_zero_when_border_and_padding_exceed_the_box() {
        // CSS: if padding+border overflow the border-box, content is 0 — never negative.
        let bp = props(
            EdgeSizes::default(),
            edges(100.0, 100.0, 100.0, 100.0),
            edges(100.0, 100.0, 100.0, 100.0),
        );
        for wm in ALL_WM {
            let s = bp.inner_size(LogicalSize::new(10.0, 10.0), wm);
            assert_eq!(s.width, 0.0, "{wm:?}");
            assert_eq!(s.height, 0.0, "{wm:?}");
            assert!(s.width >= 0.0 && s.height >= 0.0);
        }
    }

    #[test]
    fn inner_size_never_returns_nan() {
        // `.max(0.0)` discards a NaN operand, so NaN can only ever collapse to 0.
        let nan_bp = props(EdgeSizes::default(), edges(f32::NAN, 0.0, 0.0, 0.0), EdgeSizes::default());
        let cases = [
            (ResolvedBoxProps::default(), LogicalSize::new(f32::NAN, f32::NAN)),
            (nan_bp, LogicalSize::new(100.0, 100.0)),
            (
                // inf - inf = NaN, which must still floor to 0 rather than escape.
                props(EdgeSizes::default(), edges(f32::INFINITY, 0.0, f32::INFINITY, 0.0), EdgeSizes::default()),
                LogicalSize::new(f32::INFINITY, f32::INFINITY),
            ),
        ];
        for (bp, outer) in cases {
            for wm in ALL_WM {
                let s = bp.inner_size(outer, wm);
                assert!(!s.width.is_nan(), "{wm:?}: NaN width escaped inner_size");
                assert!(!s.height.is_nan(), "{wm:?}: NaN height escaped inner_size");
                assert!(s.width >= 0.0 && s.height >= 0.0);
            }
        }
    }

    #[test]
    fn inner_size_at_f32_max_stays_finite_and_non_negative() {
        let bp = props(EdgeSizes::default(), edges(1.0, 2.0, 4.0, 8.0), edges(1.0, 1.0, 1.0, 1.0));
        for wm in ALL_WM {
            let s = bp.inner_size(LogicalSize::new(f32::MAX, f32::MAX), wm);
            assert!(s.width.is_finite() && s.height.is_finite());
            assert!(s.width > 0.0 && s.height > 0.0);
        }
    }

    #[test]
    fn inner_size_with_negative_outer_size_floors_to_zero() {
        let bp = ResolvedBoxProps::default();
        for wm in ALL_WM {
            let s = bp.inner_size(LogicalSize::new(-100.0, -50.0), wm);
            assert_eq!(s.width, 0.0, "{wm:?}");
            assert_eq!(s.height, 0.0, "{wm:?}");
        }
    }

    #[test]
    fn inner_size_with_negative_padding_grows_the_content_box() {
        // Negative border/padding is not reachable from CSS, but the struct permits
        // it — assert the arithmetic is plain subtraction rather than something that
        // silently clamps mid-way.
        let bp = props(EdgeSizes::default(), edges(-5.0, -5.0, -5.0, -5.0), EdgeSizes::default());
        let s = bp.inner_size(LogicalSize::new(100.0, 100.0), LayoutWritingMode::HorizontalTb);
        assert_eq!(s.width, 110.0);
        assert_eq!(s.height, 110.0);
    }

    // ================================================================
    // ResolvedBoxProps: box rects
    // ================================================================

    #[test]
    fn content_box_shrinks_by_border_plus_padding() {
        let bp = props(
            edges(100.0, 100.0, 100.0, 100.0), // margin is irrelevant here
            edges(1.0, 2.0, 4.0, 8.0),         // padding TRBL
            edges(10.0, 20.0, 30.0, 40.0),     // border TRBL
        );
        let got = bp.content_box(rect(1000.0, 2000.0, 200.0, 100.0));
        // origin moves in by border+padding on the start edges: left 40+8, top 10+1
        // size shrinks by both sides: width 200-(40+20)-(8+2)=130, height 100-(10+30)-(1+4)=55
        assert_eq!(got, rect(1048.0, 2011.0, 130.0, 55.0));
    }

    #[test]
    fn padding_box_shrinks_by_border_only() {
        let bp = props(
            EdgeSizes::default(),
            edges(1.0, 2.0, 4.0, 8.0),
            edges(10.0, 20.0, 30.0, 40.0),
        );
        let got = bp.padding_box(rect(1000.0, 2000.0, 200.0, 100.0));
        assert_eq!(got, rect(1040.0, 2010.0, 140.0, 60.0));
    }

    #[test]
    fn margin_box_expands_by_margin_only() {
        let bp = props(
            edges(1.0, 2.0, 4.0, 8.0), // margin TRBL
            edges(99.0, 99.0, 99.0, 99.0),
            edges(99.0, 99.0, 99.0, 99.0),
        );
        let got = bp.margin_box(rect(1000.0, 2000.0, 200.0, 100.0));
        // origin moves OUT by left/top margin; size grows by both sides.
        assert_eq!(got, rect(992.0, 1999.0, 210.0, 105.0));
    }

    #[test]
    fn box_rects_nest_content_inside_padding_inside_border_inside_margin() {
        let bp = props(
            edges(5.0, 6.0, 7.0, 8.0),
            edges(1.0, 2.0, 3.0, 4.0),
            edges(9.0, 10.0, 11.0, 12.0),
        );
        let border_box = rect(50.0, 60.0, 400.0, 300.0);
        let content = bp.content_box(border_box);
        let padding = bp.padding_box(border_box);
        let margin = bp.margin_box(border_box);

        // Each layer must sit strictly inside the next one out.
        assert!(margin.origin.x <= border_box.origin.x);
        assert!(border_box.origin.x <= padding.origin.x);
        assert!(padding.origin.x <= content.origin.x);
        assert!(margin.origin.y <= border_box.origin.y);
        assert!(border_box.origin.y <= padding.origin.y);
        assert!(padding.origin.y <= content.origin.y);

        assert!(content.size.width <= padding.size.width);
        assert!(padding.size.width <= border_box.size.width);
        assert!(border_box.size.width <= margin.size.width);
        assert!(content.size.height <= padding.size.height);
        assert!(padding.size.height <= border_box.size.height);
        assert!(border_box.size.height <= margin.size.height);
    }

    #[test]
    fn content_box_size_floors_at_zero_but_the_origin_still_moves_in() {
        let bp = props(
            EdgeSizes::default(),
            edges(500.0, 500.0, 500.0, 500.0),
            edges(500.0, 500.0, 500.0, 500.0),
        );
        let got = bp.content_box(rect(0.0, 0.0, 10.0, 10.0));
        assert_eq!(got.size.width, 0.0, "size must clamp, not go negative");
        assert_eq!(got.size.height, 0.0);
        assert_eq!(got.origin.x, 1000.0, "origin is not clamped");
        assert_eq!(got.origin.y, 1000.0);
    }

    #[test]
    fn padding_box_size_floors_at_zero_for_an_oversized_border() {
        let bp = props(EdgeSizes::default(), EdgeSizes::default(), edges(99.0, 99.0, 99.0, 99.0));
        let got = bp.padding_box(rect(0.0, 0.0, 10.0, 10.0));
        assert_eq!(got.size.width, 0.0);
        assert_eq!(got.size.height, 0.0);
    }

    #[test]
    fn margin_box_with_negative_margins_can_shrink_below_zero() {
        // Unlike content_box/padding_box, margin_box does NOT floor at 0 — negative
        // margins are legal CSS and the box genuinely inverts. Pinning the real
        // behaviour so a later "helpful" clamp cannot land unnoticed.
        let bp = props(edges(-10.0, -10.0, -10.0, -10.0), EdgeSizes::default(), EdgeSizes::default());
        let got = bp.margin_box(rect(0.0, 0.0, 10.0, 10.0));
        assert_eq!(got.size.width, -10.0);
        assert_eq!(got.size.height, -10.0);
        assert_eq!(got.origin.x, 10.0, "a negative left margin pushes the origin right");
        assert_eq!(got.origin.y, 10.0);
    }

    #[test]
    fn box_rects_do_not_panic_on_nan_or_infinite_geometry() {
        let bp = props(
            edges(f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX),
            edges(f32::NAN, f32::MAX, 0.0, f32::MIN),
            edges(f32::INFINITY, 0.0, f32::NAN, 1.0),
        );
        let r = rect(f32::NAN, f32::INFINITY, f32::MAX, f32::MIN);
        let c = bp.content_box(r);
        let p = bp.padding_box(r);
        let m = bp.margin_box(r);
        // The clamped sizes are the only guaranteed invariant: never negative, never NaN.
        for s in [c.size, p.size] {
            assert!(!s.width.is_nan() && !s.height.is_nan());
            assert!(s.width >= 0.0 && s.height >= 0.0);
        }
        let _ = m;
    }

    // ================================================================
    // ResolvedBoxProps: mbp / bp getters
    // ================================================================

    #[test]
    fn mbp_and_bp_getters_sum_the_expected_layers() {
        let bp = props(
            edges(1.0, 2.0, 4.0, 8.0),      // margin: h=10, v=5
            edges(10.0, 20.0, 40.0, 80.0),  // padding: h=100, v=50
            edges(100.0, 200.0, 400.0, 800.0), // border: h=1000, v=500
        );
        assert_eq!(bp.horizontal_bp(), 1100.0);
        assert_eq!(bp.vertical_bp(), 550.0);
        assert_eq!(bp.horizontal_mbp(), 1110.0);
        assert_eq!(bp.vertical_mbp(), 555.0);

        // mbp is exactly bp plus the margins — the two must never drift apart.
        assert_eq!(bp.horizontal_mbp(), bp.horizontal_bp() + bp.margin.horizontal_sum());
        assert_eq!(bp.vertical_mbp(), bp.vertical_bp() + bp.margin.vertical_sum());
    }

    #[test]
    fn mbp_and_bp_getters_are_zero_on_a_default_box() {
        let bp = ResolvedBoxProps::default();
        assert_eq!(bp.horizontal_mbp(), 0.0);
        assert_eq!(bp.vertical_mbp(), 0.0);
        assert_eq!(bp.horizontal_bp(), 0.0);
        assert_eq!(bp.vertical_bp(), 0.0);
    }

    #[test]
    fn mbp_getters_do_not_panic_on_extreme_boxes() {
        let bp = props(
            edges(f32::MAX, f32::MAX, f32::MAX, f32::MAX),
            edges(f32::NAN, 0.0, 0.0, 0.0),
            edges(f32::NEG_INFINITY, 0.0, 0.0, 0.0),
        );
        let _ = bp.horizontal_mbp();
        let _ = bp.vertical_mbp();
        let _ = bp.horizontal_bp();
        let _ = bp.vertical_bp();
    }

    // ================================================================
    // PackedBoxProps: encoding
    // ================================================================

    #[test]
    fn pack_edge_encodes_tenths_of_a_pixel() {
        let e = edges(0.0, 1.0, 2.5, 3276.7);
        let p = PackedBoxProps::pack_edge(&e);
        assert_eq!(p[0], 0);
        assert_eq!(p[1], 10);
        assert_eq!(p[2], 25);
        assert_eq!(p[3], 32767, "the documented +3276.7px maximum");
    }

    #[test]
    fn pack_edge_rounds_to_the_nearest_tenth_rather_than_truncating() {
        let p = PackedBoxProps::pack_edge(&edges(1.04, 1.06, -1.04, -1.06));
        assert_eq!(p[0], 10, "1.04 rounds down");
        assert_eq!(p[1], 11, "1.06 rounds up — truncation would give 10");
        assert_eq!(p[2], -10);
        assert_eq!(p[3], -11);
    }

    #[test]
    fn pack_edge_saturates_out_of_range_values_instead_of_wrapping() {
        // Wrapping would turn a huge positive margin into a huge NEGATIVE one —
        // the single nastiest failure mode of this encoding.
        let p = PackedBoxProps::pack_edge(&edges(10_000.0, -10_000.0, 1e30, -1e30));
        assert_eq!(p[0], i16::MAX);
        assert_eq!(p[1], i16::MIN);
        assert_eq!(p[2], i16::MAX);
        assert_eq!(p[3], i16::MIN);
        assert!(p.iter().all(|v| *v == i16::MAX || *v == i16::MIN));
    }

    #[test]
    fn pack_edge_clamps_infinities_to_the_i16_bounds() {
        let p = PackedBoxProps::pack_edge(&edges(f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN));
        assert_eq!(p[0], i16::MAX);
        assert_eq!(p[1], i16::MIN);
        assert_eq!(p[2], i16::MAX);
        assert_eq!(p[3], i16::MIN);
    }

    #[test]
    fn pack_edge_maps_nan_to_zero_without_panicking() {
        // `f32::clamp` passes NaN through (it only panics on a NaN *bound*), and the
        // subsequent `as i16` saturates NaN to 0. So a NaN edge encodes as 0px.
        let p = PackedBoxProps::pack_edge(&edges(f32::NAN, f32::NAN, f32::NAN, f32::NAN));
        assert_eq!(p, [0, 0, 0, 0]);
    }

    #[test]
    fn pack_edge_maps_negative_zero_to_zero() {
        let p = PackedBoxProps::pack_edge(&edges(-0.0, -0.0, -0.0, -0.0));
        assert_eq!(p, [0, 0, 0, 0]);
    }

    #[test]
    fn unpack_edge_decodes_tenths_and_preserves_the_trbl_order() {
        let e = PackedBoxProps::unpack_edge(&[10, 25, -10, 32767]);
        assert!(close(e.top, 1.0, 1e-4), "top was {}", e.top);
        assert!(close(e.right, 2.5, 1e-4), "right was {}", e.right);
        assert!(close(e.bottom, -1.0, 1e-4), "bottom was {}", e.bottom);
        assert!(close(e.left, 3276.7, 1e-2), "left was {}", e.left);
    }

    #[test]
    fn unpack_edge_at_the_i16_extremes_stays_finite() {
        let e = PackedBoxProps::unpack_edge(&[i16::MAX, i16::MIN, 0, 1]);
        assert!(e.top.is_finite() && e.right.is_finite());
        assert!(close(e.top, 3276.7, 1e-2));
        assert!(close(e.right, -3276.8, 1e-2));
        assert_eq!(e.bottom, 0.0);
        assert!(close(e.left, 0.1, 1e-6));
    }

    // ================================================================
    // PackedBoxProps: round-trip
    // ================================================================

    #[test]
    fn every_i16_encoding_survives_unpack_then_pack_unchanged() {
        // Exhaustive decode->encode identity over the WHOLE i16 domain: if the
        // f32 round-off in `unpack_edge` ever drifted by more than half a tenth,
        // `pack_edge` would land on a neighbouring code and this would catch it.
        for n in i16::MIN..=i16::MAX {
            let encoded = [n; 4];
            let round_tripped = PackedBoxProps::pack_edge(&PackedBoxProps::unpack_edge(&encoded));
            assert_eq!(round_tripped, encoded, "i16 code {n} did not round-trip");
        }
    }

    #[test]
    fn pack_then_unpack_preserves_values_to_a_tenth_of_a_pixel() {
        let bp = props(
            edges(1.0, 2.5, 0.1, 12.3),
            edges(0.0, 100.25, 3276.7, -3276.8),
            edges(0.5, 0.05, 7.0, 0.0),
        );
        let out = PackedBoxProps::pack(&bp).unpack();
        for (got, want) in [
            (out.margin.top, bp.margin.top),
            (out.margin.right, bp.margin.right),
            (out.margin.bottom, bp.margin.bottom),
            (out.margin.left, bp.margin.left),
            (out.padding.top, bp.padding.top),
            (out.padding.right, bp.padding.right),
            (out.padding.bottom, bp.padding.bottom),
            (out.padding.left, bp.padding.left),
            (out.border.top, bp.border.top),
            (out.border.right, bp.border.right),
            (out.border.bottom, bp.border.bottom),
            (out.border.left, bp.border.left),
        ] {
            // Half a quantum (0.05) plus a little float slack at the 3276.x extreme.
            assert!(close(got, want, 0.051), "{want} round-tripped to {got}");
        }
    }

    #[test]
    fn pack_carries_the_margin_auto_flags_through_verbatim() {
        // margin_auto is NOT part of the lossy i16 encoding, so it must come back bit-exact.
        let bp = ResolvedBoxProps {
            margin_auto: MarginAuto { left: true, right: false, top: true, bottom: false },
            ..ResolvedBoxProps::default()
        };
        let out = PackedBoxProps::pack(&bp).unpack();
        assert!(out.margin_auto.left);
        assert!(!out.margin_auto.right);
        assert!(out.margin_auto.top);
        assert!(!out.margin_auto.bottom);
    }

    #[test]
    fn pack_keeps_the_three_edge_groups_apart() {
        // A copy-paste slip in `pack` would splice margin into padding or border.
        let bp = props(
            edges(1.0, 1.0, 1.0, 1.0),
            edges(2.0, 2.0, 2.0, 2.0),
            edges(3.0, 3.0, 3.0, 3.0),
        );
        let p = PackedBoxProps::pack(&bp);
        assert_eq!(p.margin, [10; 4]);
        assert_eq!(p.padding, [20; 4]);
        assert_eq!(p.border, [30; 4]);
    }

    #[test]
    fn pack_of_an_out_of_range_box_clamps_rather_than_flipping_sign() {
        let bp = props(
            edges(5000.0, 5000.0, 5000.0, 5000.0), // beyond +3276.7
            EdgeSizes::default(),
            EdgeSizes::default(),
        );
        let out = PackedBoxProps::pack(&bp).unpack();
        assert!(out.margin.top > 0.0, "a huge margin must not decode as negative");
        assert!(close(out.margin.top, 3276.7, 1e-2));
    }

    #[test]
    fn packed_default_is_an_all_zero_box() {
        let p = PackedBoxProps::default();
        assert_eq!(p.margin, [0; 4]);
        assert_eq!(p.horizontal_mbp(), 0.0);
        assert_eq!(p.vertical_mbp(), 0.0);
        assert_eq!(p.horizontal_bp(), 0.0);
        assert_eq!(p.vertical_bp(), 0.0);
        let s = p.inner_size(LogicalSize::new(10.0, 10.0), LayoutWritingMode::HorizontalTb);
        assert_eq!(s.width, 10.0);
        assert_eq!(s.height, 10.0);
    }

    // ================================================================
    // PackedBoxProps: convenience methods must agree with unpack()
    // ================================================================

    #[test]
    fn packed_convenience_methods_match_the_unpacked_equivalents() {
        let bp = props(
            edges(1.0, 2.5, 4.0, 8.5),
            edges(0.5, 1.5, 2.5, 3.5),
            edges(2.0, 4.0, 6.0, 8.0),
        );
        let packed = PackedBoxProps::pack(&bp);
        let unpacked = packed.unpack();
        let r = rect(10.0, 20.0, 300.0, 200.0);

        assert_eq!(packed.content_box(r), unpacked.content_box(r));
        assert_eq!(packed.padding_box(r), unpacked.padding_box(r));
        assert_eq!(packed.margin_box(r), unpacked.margin_box(r));
        assert_eq!(packed.horizontal_mbp(), unpacked.horizontal_mbp());
        assert_eq!(packed.vertical_mbp(), unpacked.vertical_mbp());
        assert_eq!(packed.horizontal_bp(), unpacked.horizontal_bp());
        assert_eq!(packed.vertical_bp(), unpacked.vertical_bp());

        for wm in ALL_WM {
            let a = packed.inner_size(r.size, wm);
            let b = unpacked.inner_size(r.size, wm);
            assert_eq!(a.width, b.width, "{wm:?}");
            assert_eq!(a.height, b.height, "{wm:?}");
        }
    }

    #[test]
    fn packed_inner_size_floors_at_zero_for_a_tiny_box() {
        let bp = props(
            EdgeSizes::default(),
            edges(50.0, 50.0, 50.0, 50.0),
            edges(50.0, 50.0, 50.0, 50.0),
        );
        let packed = PackedBoxProps::pack(&bp);
        for wm in ALL_WM {
            let s = packed.inner_size(LogicalSize::new(1.0, 1.0), wm);
            assert_eq!(s.width, 0.0, "{wm:?}");
            assert_eq!(s.height, 0.0, "{wm:?}");
        }
    }

    #[test]
    fn packed_box_rects_do_not_panic_on_extreme_input_rects() {
        let packed = PackedBoxProps::pack(&props(
            edges(3276.7, 3276.7, 3276.7, 3276.7),
            edges(3276.7, 3276.7, 3276.7, 3276.7),
            edges(3276.7, 3276.7, 3276.7, 3276.7),
        ));
        for r in [
            rect(0.0, 0.0, 0.0, 0.0),
            rect(f32::MAX, f32::MIN, f32::MAX, f32::MAX),
            rect(f32::NAN, f32::NAN, f32::NAN, f32::NAN),
            rect(-1e30, -1e30, -1e30, -1e30),
        ] {
            let c = packed.content_box(r);
            let p = packed.padding_box(r);
            let _ = packed.margin_box(r);
            assert!(c.size.width >= 0.0 && !c.size.width.is_nan());
            assert!(p.size.height >= 0.0 && !p.size.height.is_nan());
        }
    }

    // ================================================================
    // WritingModeContext
    // ================================================================

    #[test]
    fn writing_mode_context_new_stores_what_it_was_given() {
        let c = WritingModeContext::new(
            LayoutWritingMode::VerticalRl,
            StyleDirection::Rtl,
            StyleTextOrientation::Mixed,
        );
        assert_eq!(c.writing_mode, LayoutWritingMode::VerticalRl);
        assert_eq!(c.direction, StyleDirection::Rtl);
        assert_eq!(c.text_orientation, StyleTextOrientation::Mixed);
    }

    #[test]
    fn text_orientation_upright_forces_the_used_direction_to_ltr() {
        // CSS Writing Modes 4 §5.1: `text-orientation: upright` makes the USED value
        // of `direction` ltr, even when the author wrote `direction: rtl`.
        let c = WritingModeContext::new(
            LayoutWritingMode::VerticalRl,
            StyleDirection::Rtl,
            StyleTextOrientation::Upright,
        );
        assert_eq!(c.used_direction(), StyleDirection::Ltr);
        assert!(!c.is_inline_reversed(), "upright must cancel the RTL reversal");
    }

    #[test]
    fn only_upright_overrides_the_direction() {
        for orientation in [StyleTextOrientation::Mixed, StyleTextOrientation::Sideways] {
            let c = WritingModeContext::new(
                LayoutWritingMode::VerticalRl,
                StyleDirection::Rtl,
                orientation,
            );
            assert_eq!(
                c.used_direction(),
                StyleDirection::Rtl,
                "{orientation:?} must not touch the direction"
            );
            assert!(c.is_inline_reversed());
        }
    }

    #[test]
    fn writing_mode_context_new_is_idempotent() {
        // Re-feeding a context's own fields back into `new()` must be a fixed point,
        // otherwise the upright override would compound across re-resolutions.
        for wm in ALL_WM {
            for dir in [StyleDirection::Ltr, StyleDirection::Rtl] {
                for or in [
                    StyleTextOrientation::Mixed,
                    StyleTextOrientation::Upright,
                    StyleTextOrientation::Sideways,
                ] {
                    let once = WritingModeContext::new(wm, dir, or);
                    let twice = WritingModeContext::new(
                        once.writing_mode,
                        once.direction,
                        once.text_orientation,
                    );
                    assert_eq!(once, twice, "{wm:?}/{dir:?}/{or:?} is not a fixed point");
                }
            }
        }
    }

    #[test]
    fn is_horizontal_is_true_only_for_horizontal_tb() {
        let mk = |wm| WritingModeContext::new(wm, StyleDirection::Ltr, StyleTextOrientation::Mixed);
        assert!(mk(LayoutWritingMode::HorizontalTb).is_horizontal());
        assert!(!mk(LayoutWritingMode::VerticalRl).is_horizontal());
        assert!(!mk(LayoutWritingMode::VerticalLr).is_horizontal());
    }

    #[test]
    fn inline_and_block_axis_predicates_track_is_horizontal() {
        // In horizontal-tb, inline size is the width and block size is the height;
        // both flip together in vertical modes. They may never disagree.
        for wm in ALL_WM {
            for dir in [StyleDirection::Ltr, StyleDirection::Rtl] {
                let c = WritingModeContext::new(wm, dir, StyleTextOrientation::Mixed);
                assert_eq!(c.inline_size_is_width(), c.is_horizontal(), "{wm:?}");
                assert_eq!(c.block_size_is_height(), c.is_horizontal(), "{wm:?}");
                assert_eq!(
                    c.inline_size_is_width(),
                    c.block_size_is_height(),
                    "{wm:?}: the two axes must flip together"
                );
            }
        }
    }

    #[test]
    fn is_inline_reversed_follows_the_used_direction_not_the_writing_mode() {
        for wm in ALL_WM {
            let ltr = WritingModeContext::new(wm, StyleDirection::Ltr, StyleTextOrientation::Mixed);
            let rtl = WritingModeContext::new(wm, StyleDirection::Rtl, StyleTextOrientation::Mixed);
            assert!(!ltr.is_inline_reversed(), "{wm:?} ltr");
            assert!(rtl.is_inline_reversed(), "{wm:?} rtl");
        }
    }

    #[test]
    fn default_writing_mode_context_is_horizontal_ltr_mixed() {
        let c = WritingModeContext::default();
        assert_eq!(c.writing_mode, LayoutWritingMode::HorizontalTb);
        assert_eq!(c.used_direction(), StyleDirection::Ltr);
        assert_eq!(c.text_orientation, StyleTextOrientation::Mixed);
        assert!(c.is_horizontal());
        assert!(c.inline_size_is_width());
        assert!(c.block_size_is_height());
        assert!(!c.is_inline_reversed());
    }

    #[test]
    fn default_context_matches_new_with_the_same_arguments() {
        let built = WritingModeContext::new(
            LayoutWritingMode::HorizontalTb,
            StyleDirection::Ltr,
            StyleTextOrientation::Mixed,
        );
        assert_eq!(WritingModeContext::default(), built);
    }
}
