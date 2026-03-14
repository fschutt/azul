//! TODO: Move these to CSS module

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    ui_solver::ResolvedOffsets,
};
use azul_css::props::{
    basic::{pixel::PixelValue, PhysicalSize, PropertyContext, ResolutionContext, SizeMetric},
    layout::LayoutWritingMode,
    style::{StyleDirection, StyleTextOrientation},
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

    // +spec:block-formatting-context:6225cb - line-relative directions: vertical modes map line-over/under to top/bottom
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
// +spec:box-model:ff1730 - margin properties apply to both continuous and paged media
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
            // +spec:box-model:c921aa - auto margin-top/bottom used value is 0 for block-level non-replaced elements in normal flow
            // +spec:box-model:e25fdc - auto margins treated as zero for abspos size computation
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
    pub fn inner_size(&self, outer_size: LogicalSize, wm: LayoutWritingMode) -> LogicalSize {
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

    // +spec:box-model:0e75c1 - margin, padding, border contribute to layout bounds (default line-fit-edge: leading uses line-height model)
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
    pub fn pack(bp: &ResolvedBoxProps) -> Self {
        Self {
            margin: Self::pack_edge(&bp.margin),
            padding: Self::pack_edge(&bp.padding),
            border: Self::pack_edge(&bp.border),
            margin_auto: bp.margin_auto,
        }
    }

    /// Unpack to full `ResolvedBoxProps` with f32 values.
    #[inline]
    pub fn unpack(&self) -> ResolvedBoxProps {
        ResolvedBoxProps {
            margin: Self::unpack_edge(&self.margin),
            padding: Self::unpack_edge(&self.padding),
            border: Self::unpack_edge(&self.border),
            margin_auto: self.margin_auto,
        }
    }

    /// Convenience: unpack and call `inner_size` on the result.
    #[inline]
    pub fn inner_size(&self, outer_size: LogicalSize, wm: LayoutWritingMode) -> LogicalSize {
        self.unpack().inner_size(outer_size, wm)
    }

    /// Convenience: unpack and call `content_box` on the result.
    #[inline]
    pub fn content_box(&self, border_box: LogicalRect) -> LogicalRect {
        self.unpack().content_box(border_box)
    }

    /// Convenience: unpack and call `padding_box` on the result.
    #[inline]
    pub fn padding_box(&self, border_box: LogicalRect) -> LogicalRect {
        self.unpack().padding_box(border_box)
    }

    /// Convenience: unpack and call `margin_box` on the result.
    #[inline]
    pub fn margin_box(&self, border_box: LogicalRect) -> LogicalRect {
        self.unpack().margin_box(border_box)
    }

    /// Convenience: unpack and return horizontal MBP.
    #[inline]
    pub fn horizontal_mbp(&self) -> f32 {
        self.unpack().horizontal_mbp()
    }

    /// Convenience: unpack and return vertical MBP.
    #[inline]
    pub fn vertical_mbp(&self) -> f32 {
        self.unpack().vertical_mbp()
    }

    /// Convenience: unpack and return horizontal BP.
    #[inline]
    pub fn horizontal_bp(&self) -> f32 {
        self.unpack().horizontal_bp()
    }

    /// Convenience: unpack and return vertical BP.
    #[inline]
    pub fn vertical_bp(&self) -> f32 {
        self.unpack().vertical_bp()
    }

    #[inline(always)]
    fn pack_edge(e: &EdgeSizes) -> [i16; 4] {
        [
            (e.top * 10.0).round() as i16,
            (e.right * 10.0).round() as i16,
            (e.bottom * 10.0).round() as i16,
            (e.left * 10.0).round() as i16,
        ]
    }

    #[inline(always)]
    fn unpack_edge(e: &[i16; 4]) -> EdgeSizes {
        EdgeSizes {
            top: e[0] as f32 * 0.1,
            right: e[1] as f32 * 0.1,
            bottom: e[2] as f32 * 0.1,
            left: e[3] as f32 * 0.1,
        }
    }
}

// Verwende die Typen aus azul_css für float und clear
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

impl IntrinsicSizes {
    /// Creates a zero-sized IntrinsicSizes.
    pub fn zero() -> Self {
        Self::default()
    }
}

// ============================================================================
// WRITING MODE SUPPORT
// ============================================================================

/// Returns true if the writing mode is horizontal (HorizontalTb).
///
/// This is the main entry point for code that needs to check whether layout
/// should proceed in horizontal or vertical mode. In horizontal mode, the
/// inline axis is horizontal (left-to-right or right-to-left) and the block
/// axis is vertical (top-to-bottom). In vertical modes, these are swapped.
// +spec:block-formatting-context:6225cb - vertical writing modes: line-over is right, line-under is left
// +spec:block-formatting-context:9a4269 - vertical vs horizontal script classification
pub fn is_horizontal(writing_mode: LayoutWritingMode) -> bool {
    matches!(writing_mode, LayoutWritingMode::HorizontalTb)
}

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
    pub fn new(
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
    /// Returns the used value of `direction`, accounting for `text-orientation: upright`
    /// which forces direction to `ltr` in vertical writing modes per CSS Writing Modes 4 §5.1.
    pub fn used_direction(&self) -> StyleDirection {
        match self.writing_mode {
            LayoutWritingMode::HorizontalTb => self.direction,
            LayoutWritingMode::VerticalRl | LayoutWritingMode::VerticalLr => {
                if self.text_orientation == StyleTextOrientation::Upright {
                    StyleDirection::Ltr
                } else {
                    self.direction
                }
            }
        }
    }

    // +spec:containing-block:c205e5 - orthogonal flow: child writing mode perpendicular to containing block's

    /// Returns true if the writing mode is horizontal (HorizontalTb).
    ///
    /// When true, the inline axis is horizontal and the block axis is vertical.
    pub fn is_horizontal(&self) -> bool {
        is_horizontal(self.writing_mode)
    }

    /// Returns true if the inline size corresponds to the physical width.
    ///
    /// In horizontal writing modes, inline size = width.
    /// In vertical writing modes, inline size = height.
    // +spec:block-formatting-context:bb9845 - orthogonal flows: inline/block axis mapping
    pub fn inline_size_is_width(&self) -> bool {
        self.is_horizontal()
    }

    /// Returns true if the block size corresponds to the physical height.
    ///
    /// In horizontal writing modes, block size = height.
    /// In vertical writing modes, block size = width.
    pub fn block_size_is_height(&self) -> bool {
        self.is_horizontal()
    }

    // +spec:writing-modes:32541a - direction property controls inline text direction via stylesheet
    /// Returns true if the inline direction is reversed (RTL in horizontal,
    /// or bottom-to-top in certain vertical modes).
    pub fn is_inline_reversed(&self) -> bool {
        self.used_direction() == StyleDirection::Rtl
    }
}
