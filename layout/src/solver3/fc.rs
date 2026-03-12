//! solver3/fc.rs - Formatting Context Layout
//!
//! This module implements the CSS Visual Formatting Model's formatting contexts:
//!
//! - **Block Formatting Context (BFC)**: CSS 2.2 § 9.4.1 Block-level boxes in normal flow, with
//!   margin collapsing and float positioning.
//!
//! - **Inline Formatting Context (IFC)**: CSS 2.2 § 9.4.2 Inline-level content (text,
//!   inline-blocks) laid out in line boxes.
//!
//! - **Table Formatting Context**: CSS 2.2 § 17 Table layout with column width calculation and cell
//!   positioning.
//!
//! - **Flex/Grid Formatting Contexts**: CSS Flexbox/Grid via Taffy Delegated to the Taffy layout
//!   engine for modern layout modes.
//!
//! # Module Organization
//!
//! 1. **Constants & Types** - Magic numbers as named constants, core types
//! 2. **Entry Point** - `layout_formatting_context` dispatcher
//! 3. **BFC Layout** - Block formatting context implementation
//! 4. **IFC Layout** - Inline formatting context implementation
//! 5. **Table Layout** - Table formatting context implementation
//! 6. **Flex/Grid Layout** - Taffy bridge wrappers
//! 7. **Helper Functions** - Property getters, margin collapsing, utilities

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use azul_core::{
    dom::{FormattingContext, NodeId, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{
            font::{StyleFontStyle, StyleFontWeight},
            pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
            ColorU, PhysicalSize, PropertyContext, ResolutionContext, SizeMetric,
        },
        layout::{
            ColumnCount, LayoutBorderSpacing, LayoutClear, LayoutDisplay, LayoutFloat,
            LayoutHeight, LayoutJustifyContent, LayoutOverflow, LayoutPosition, LayoutTableLayout,
            LayoutTextJustify, LayoutWidth, LayoutWritingMode, ShapeInside, ShapeOutside,
            StyleBorderCollapse, StyleCaptionSide, StyleEmptyCells,
        },
        property::CssProperty,
        style::{
            BorderStyle, StyleDirection, StyleHyphens, StyleLineBreak, StyleListStylePosition,
            StyleListStyleType, StyleOverflowWrap, StyleTextAlign, StyleTextAlignLast,
            StyleTextCombineUpright, StyleTextOrientation, StyleVerticalAlign, StyleVisibility,
            StyleWhiteSpace, StyleWordBreak,
        },
    },
};
use rust_fontconfig::FcWeight;
use taffy::{AvailableSpace, LayoutInput, Line, Size as TaffySize};

#[cfg(feature = "text_layout")]
use crate::text3;
use crate::{
    debug_ifc_layout, debug_info, debug_log, debug_table_layout, debug_warning,
    font_traits::{
        ContentIndex, FontLoaderTrait, ImageSource, InlineContent, InlineImage, InlineShape,
        LayoutFragment, ObjectFit, ParsedFontTrait, SegmentAlignment, ShapeBoundary,
        ShapeDefinition, ShapedItem, Size, StyleProperties, StyledRun, TextLayoutCache,
        UnifiedConstraints,
    },
    solver3::{
        geometry::{BoxProps, EdgeSizes, IntrinsicSizes},
        getters::{
            get_css_height, get_css_width, get_direction_property,
            get_display_property, get_element_font_size, get_float, get_clear,
            get_list_style_position, get_list_style_type, get_overflow_x, get_overflow_y,
            get_parent_font_size, get_root_font_size, get_style_properties,
            get_text_align, get_text_orientation_property, get_vertical_align_property,
            get_visibility, get_white_space_property, get_writing_mode, MultiValue,
        },
        layout_tree::{
            AnonymousBoxType, CachedInlineLayout, LayoutNode, LayoutTree, PseudoElement,
        },
        positioning::get_position_type,
        scrollbar::ScrollbarRequirements,
        sizing::extract_text_from_node,
        taffy_bridge, LayoutContext, LayoutDebugMessage, LayoutError, Result,
    },
    text3::cache::{AvailableSpace as Text3AvailableSpace, TextAlign as Text3TextAlign},
};

/// Default scrollbar width in pixels (CSS `scrollbar-width: auto`).
/// This is only used as a fallback when per-node CSS cannot be queried.
/// Prefer `getters::get_layout_scrollbar_width_px()` for per-node resolution.
pub const DEFAULT_SCROLLBAR_WIDTH_PX: f32 = 16.0;

// Note: DEFAULT_FONT_SIZE and PT_TO_PX are imported from pixel

/// Result of BFC layout with margin escape information
#[derive(Debug, Clone)]
pub(crate) struct BfcLayoutResult {
    /// Standard layout output (positions, overflow size, baseline)
    pub output: LayoutOutput,
    /// Top margin that escaped the BFC (for parent-child collapse)
    /// If Some, this margin should be used by parent instead of positioning this BFC
    pub escaped_top_margin: Option<f32>,
    /// Bottom margin that escaped the BFC (for parent-child collapse)
    /// If Some, this margin should collapse with next sibling
    pub escaped_bottom_margin: Option<f32>,
}

impl BfcLayoutResult {
    pub fn from_output(output: LayoutOutput) -> Self {
        Self {
            output,
            escaped_top_margin: None,
            escaped_bottom_margin: None,
        }
    }
}

/// The CSS `overflow` property behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowBehavior {
    Visible,
    Hidden,
    Clip,
    Scroll,
    Auto,
}

impl OverflowBehavior {
    pub fn is_clipped(&self) -> bool {
        matches!(self, Self::Hidden | Self::Clip | Self::Scroll | Self::Auto)
    }

    pub fn is_scroll(&self) -> bool {
        matches!(self, Self::Scroll | Self::Auto)
    }
}

/// Input constraints for a layout function.
#[derive(Debug)]
pub struct LayoutConstraints<'a> {
    /// The available space for the content, excluding padding and borders.
    pub available_size: LogicalSize,
    /// The CSS writing-mode of the context.
    pub writing_mode: LayoutWritingMode,
    /// Full writing mode context (writing-mode + direction + text-orientation).
    /// Used by writing-mode-aware layout code to correctly map inline/block
    /// dimensions to physical x/y coordinates.
    pub writing_mode_ctx: super::geometry::WritingModeContext,
    /// The state of the parent Block Formatting Context, if applicable.
    /// This is how state (like floats) is passed down.
    pub bfc_state: Option<&'a mut BfcState>,
    // Other properties like text-align would go here.
    pub text_align: TextAlign,
    /// The size of the containing block (parent's content box).
    /// This is used for resolving percentage-based sizes and as parent_size for Taffy.
    pub containing_block_size: LogicalSize,
    /// The semantic type of the available width constraint.
    ///
    /// This field is crucial for correct inline layout caching:
    /// - `Definite(w)`: Normal layout with a specific available width
    /// - `MinContent`: Intrinsic minimum width measurement (maximum wrapping)
    /// - `MaxContent`: Intrinsic maximum width measurement (no wrapping)
    ///
    /// When caching inline layouts, we must track which constraint type was used
    /// to compute the cached result. A layout computed with `MinContent` (width=0)
    /// must not be reused when the actual available width is known.
    pub available_width_type: Text3AvailableSpace,
}

/// Manages all layout state for a single Block Formatting Context.
/// This struct is created by the BFC root and lives for the duration of its layout.
#[derive(Debug, Clone)]
pub struct BfcState {
    /// The current position for the next in-flow block element.
    pub pen: LogicalPosition,
    /// The state of all floated elements within this BFC.
    pub floats: FloatingContext,
    /// The state of margin collapsing within this BFC.
    pub margins: MarginCollapseContext,
}

impl BfcState {
    pub fn new() -> Self {
        Self {
            pen: LogicalPosition::zero(),
            floats: FloatingContext::default(),
            margins: MarginCollapseContext::default(),
        }
    }
}

/// Manages vertical margin collapsing within a BFC.
#[derive(Debug, Default, Clone)]
pub struct MarginCollapseContext {
    /// The bottom margin of the last in-flow, block-level element.
    /// Can be positive or negative.
    pub last_in_flow_margin_bottom: f32,
}

/// The result of laying out a formatting context.
#[derive(Debug, Default, Clone)]
pub struct LayoutOutput {
    /// The final positions of child nodes, relative to the container's content-box origin.
    pub positions: BTreeMap<usize, LogicalPosition>,
    /// The total size occupied by the content, which may exceed `available_size`.
    pub overflow_size: LogicalSize,
    // +spec:inline-formatting-context:f7eebb - baseline along inline axis for glyph alignment
    /// The baseline of the context, if applicable, measured from the top of its content box.
    pub baseline: Option<f32>,
}

/// Text alignment options
#[derive(Debug, Clone, Copy, Default)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Center,
    Justify,
}

/// Represents a single floated element within a BFC.
#[derive(Debug, Clone, Copy)]
struct FloatBox {
    /// The type of float (Left or Right).
    kind: LayoutFloat,
    /// The rectangle of the float's content box (origin includes top/left margin offset).
    rect: LogicalRect,
    /// The margin sizes (needed to calculate true margin-box bounds).
    margin: EdgeSizes,
}

/// Manages the state of all floated elements within a Block Formatting Context.
// +spec:block-formatting-context:a4e6f9 - float rules reference only elements in the same BFC (scoped via BfcState)
// +spec:floats:2fa329 - Float positioning (left/right shift), content flow along sides, and clear property
/// +spec:floats:970b4c - Implements CSS2§9.5 float positioning and flow interaction
#[derive(Debug, Default, Clone)]
pub struct FloatingContext {
    /// All currently positioned floats within the BFC.
    pub floats: Vec<FloatBox>,
}

impl FloatingContext {
    /// Add a newly positioned float to the context
    pub fn add_float(&mut self, kind: LayoutFloat, rect: LogicalRect, margin: EdgeSizes) {
        self.floats.push(FloatBox { kind, rect, margin });
    }

    // +spec:box-model:0c9b13 - line boxes next to floats are shortened to make room
    // +spec:floats:148fcd - floating boxes reduce available line box width between containing block edges
    // +spec:floats:49a491 - Line boxes stacked with no separation except float clearance, never overlap
    // +spec:floats:8974e6 - text flows into vacated space by narrowing line boxes around floats
    // +spec:floats:af94f2 - content displaced by float: line boxes shrink to avoid float margin boxes
    // +spec:floats:e5961b - remaining text flows into vacated space via available_line_box_space
    // +spec:inline-formatting-context:7cbe58 - shortened line boxes due to floats; shift down if too small
    /// Finds the available space on the cross-axis for a line box at a given main-axis range.
    // +spec:containing-block:4b0c44 - line boxes shortened by floats resume containing block width after float
    ///
    /// Returns a tuple of (`cross_start_offset`, `cross_end_offset`) relative to the
    /// BFC content box, defining the available space for an in-flow element.
    // +spec:inline-formatting-context:e70328 - line box width reduced by floats between containing block edges
    pub fn available_line_box_space(
        &self,
        main_start: f32,
        main_end: f32,
        bfc_cross_size: f32,
        wm: LayoutWritingMode,
    ) -> (f32, f32) {
        let mut available_cross_start = 0.0_f32;
        let mut available_cross_end = bfc_cross_size;

        for float in &self.floats {
            // Get the logical main-axis span of the existing float's MARGIN BOX.
            let float_main_start = float.rect.origin.main(wm) - float.margin.main_start(wm);
            let float_main_end = float_main_start + float.rect.size.main(wm)
                + float.margin.main_start(wm) + float.margin.main_end(wm);

            // Check for overlap on the main axis.
            if main_end > float_main_start && main_start < float_main_end {
                // CSS 2.2 § 9.5: border box must not overlap MARGIN BOX of floats,
                // so we include the float's margins in the cross-axis bounds.
                let float_cross_start = float.rect.origin.cross(wm) - float.margin.cross_start(wm);
                let float_cross_end = float_cross_start + float.rect.size.cross(wm)
                    + float.margin.cross_start(wm) + float.margin.cross_end(wm);

                // +spec:floats:17a63f - float left/right map to line-left/line-right via logical coords
                // +spec:writing-modes:e55820 - line-relative mappings: left/right interpreted as line-left/line-right per writing mode
                if float.kind == LayoutFloat::Left {
                    // "line-left", i.e., cross-start
                    available_cross_start = available_cross_start.max(float_cross_end);
                } else {
                    // Float::Right, i.e., cross-end
                    available_cross_end = available_cross_end.min(float_cross_start);
                }
            }
        }
        (available_cross_start, available_cross_end)
    }

    // +spec:block-formatting-context:d06e6e - clearance computation for clear property on blocks and floats (CSS 2.2 § 9.5.2)
    // +spec:floats:31a3d5 - Clearance computation: places border edge even with bottom outer edge of lowest float to be cleared
    // +spec:floats:f9bef1 - clear property moves element below preceding floats
    /// Returns the main-axis offset needed to be clear of floats of the given type.
    // +spec:block-formatting-context:7f6bde - CSS 2.2 § 9.5.2 clear property: clearance places border edge below bottom outer edge of cleared floats
    // +spec:block-formatting-context:ef493f - clearance computation: places border edge even with bottom outer edge of lowest float to be cleared; inhibits margin collapsing
    // +spec:box-model:b118fe - top border edge must be below bottom outer edge of earlier floats
    // +spec:floats:415066 - Clear property: top border edge below bottom outer edge of cleared floats
    // +spec:floats:7e4ad6 - clear property: element box may not be adjacent to earlier floats; only considers floats in same BFC
    // +spec:floats:32e45d - clear:right causes sibling to flow below right floats
    // +spec:floats:7f417a - clear property prevents content from flowing next to floats
    // +spec:floats:d06304 - clear property moves element below floats, leaving blank space
    // +spec:overflow:1a7aff - clearance calculation (incl. negative clearance) and clear on floats (constraint #10)
    // +spec:positioning:1c2508 - clearance calculation: places border edge even with bottom outer edge of lowest cleared float (CSS 2.2 § 9.5.2)
    // +spec:positioning:fe0912 - clearance computation: places border edge below bottom outer edge of cleared floats
    // (clearance = amount to place border edge even with bottom outer edge of lowest
    // float to be cleared); clearance can be negative per spec example 2
    pub fn clearance_offset(
        &self,
        clear: LayoutClear,
        current_main_offset: f32,
        wm: LayoutWritingMode,
    ) -> f32 {
        let mut max_end_offset = 0.0_f32;

        let check_left = clear == LayoutClear::Left || clear == LayoutClear::Both;
        let check_right = clear == LayoutClear::Right || clear == LayoutClear::Both;

        for float in &self.floats {
            let should_clear_this_float = (check_left && float.kind == LayoutFloat::Left)
                || (check_right && float.kind == LayoutFloat::Right);

            if should_clear_this_float {
                // CSS 2.2 § 9.5.2: "the top border edge of the box be below the bottom outer edge"
                // Outer edge = margin-box boundary (content + padding + border + margin)
                let float_margin_box_end = float.rect.origin.main(wm)
                    + float.rect.size.main(wm)
                    + float.margin.main_end(wm);
                max_end_offset = max_end_offset.max(float_margin_box_end);
            }
        }

        if max_end_offset > current_main_offset {
            max_end_offset
        } else {
            current_main_offset
        }
    }
}

/// Encapsulates all state needed to lay out a single Block Formatting Context.
struct BfcLayoutState {
    /// The current position for the next in-flow block element.
    pen: LogicalPosition,
    floats: FloatingContext,
    margins: MarginCollapseContext,
    /// The writing mode of the BFC root.
    writing_mode: LayoutWritingMode,
}

/// Result of a formatting context layout operation
#[derive(Debug, Default)]
pub struct LayoutResult {
    pub positions: Vec<(usize, LogicalPosition)>,
    pub overflow_size: Option<LogicalSize>,
    pub baseline_offset: f32,
}

// Entry Point & Dispatcher

/// Main dispatcher for formatting context layout.
///
/// Routes layout to the appropriate formatting context handler based on the node's
/// `formatting_context` property. This is the main entry point for all layout operations.
///
/// # CSS Spec References
/// - CSS 2.2 § 9.4: Formatting contexts
/// - CSS Flexbox § 3: Flex formatting contexts
/// - CSS Grid § 5: Grid formatting contexts
// +spec:block-formatting-context:b04653 - dispatches layout by formatting context type (BFC, IFC, Table, Flex, Grid)
// +spec:block-formatting-context:e46499 - inner display type determines formatting context (BFC, IFC, table, flex, grid)
pub fn layout_formatting_context<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints,
    float_cache: &mut HashMap<usize, FloatingContext>,
) -> Result<BfcLayoutResult> {
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    debug_info!(
        ctx,
        "[layout_formatting_context] node_index={}, fc={:?}, available_size={:?}",
        node_index,
        node.formatting_context,
        constraints.available_size
    );

    // +spec:block-formatting-context:06a24f - CSS 2.2 § 9.4: block-level boxes → BFC, inline-level → IFC
    // +spec:block-formatting-context:9428cf - block container can establish both BFC and IFC simultaneously
    // +spec:inline-formatting-context:8bfe73 - display:flow generates inline box (Inline) or block container (Block) based on outer display type
    match node.formatting_context {
        FormattingContext::Block { .. } => {
            layout_bfc(ctx, tree, text_cache, node_index, constraints, float_cache)
        }
        // +spec:inline-formatting-context:a180ed - IFC establishment: inline-level boxes fragmented into line boxes with baseline alignment
        FormattingContext::Inline => layout_ifc(ctx, text_cache, tree, node_index, constraints)
            .map(BfcLayoutResult::from_output),
        FormattingContext::InlineBlock => {
            // +spec:display-property:1f5ddf - inline-level boxes with non-flow inner display establish new formatting context
            // +spec:inline-formatting-context:1ad004 - atomic inline (inline-block) establishes new formatting context
            // CSS 2.2 § 9.4.1: "inline-blocks... establish new block formatting contexts"
            // InlineBlock ALWAYS establishes a BFC for its contents.
            // The element itself participates as an atomic inline in its parent's IFC,
            // but its children are laid out in a BFC, not an IFC.
            let mut temp_float_cache = HashMap::new();
            layout_bfc(ctx, tree, text_cache, node_index, constraints, &mut temp_float_cache)
        }
        // +spec:table-layout:753687 - CSS 2.2 §17.2 table model: display values map to FormattingContext variants and dispatch table layout
        FormattingContext::Table => layout_table_fc(ctx, tree, text_cache, node_index, constraints)
            .map(BfcLayoutResult::from_output),
        // Table-internal flex items are blockified during tree construction
        // (blockify_flex_item_if_table_internal in layout_tree.rs), so they arrive
        // here as Block, not TableCell etc.
        FormattingContext::Flex | FormattingContext::Grid => {
            layout_flex_grid(ctx, tree, text_cache, node_index, constraints)
        }
        // that are not block boxes, so they establish new BFCs for their contents
        FormattingContext::TableCell | FormattingContext::TableCaption => {
            let mut temp_float_cache = HashMap::new();
            layout_bfc(ctx, tree, text_cache, node_index, constraints, &mut temp_float_cache)
        }
        _ => {
            // Unknown formatting context - fall back to BFC
            let mut temp_float_cache = HashMap::new();
            layout_bfc(
                ctx,
                tree,
                text_cache,
                node_index,
                constraints,
                &mut temp_float_cache,
            )
        }
    }
}

// Flex / grid layout (taffy Bridge)
// containing block determined by grid-placement properties; Taffy handles this internally
// (grid auto-placement §8.5 and abspos grid items use grid-area CB, not just padding box)

/// Lays out a Flex or Grid formatting context using the Taffy layout engine.
///
/// # CSS Spec References
///
/// - CSS Flexbox § 9: Flex Layout Algorithm
/// - CSS Grid § 12: Grid Layout Algorithm
// gutters on either side of collapsed tracks collapse including distributed alignment space,
// minimum contribution = outer size from min-width/min-height if specified size is auto else
// min-content contribution) — all handled by Taffy grid implementation
///
/// # Implementation Notes
///
/// - Resolves explicit CSS dimensions to pixel values for `known_dimensions`
/// - Uses `InherentSize` mode when explicit dimensions are set
/// - Uses `ContentSize` mode for auto-sizing (shrink-to-fit)
fn layout_flex_grid<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<BfcLayoutResult> {
    // Available space comes directly from constraints - margins are handled by Taffy
    let available_space = TaffySize {
        width: AvailableSpace::Definite(constraints.available_size.width),
        height: AvailableSpace::Definite(constraints.available_size.height),
    };

    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;

    // from flex line's cross size (clamped by min/max) when align-self:stretch, cross-size:auto,
    // and neither cross-axis margin is auto. Otherwise uses hypothetical cross size.
    // NOTE: visibility:collapse strut size for flex items is handled internally by Taffy.
    //
    // Resolve explicit CSS dimensions to pixel values.
    // This is CRITICAL for align-items: stretch to work correctly!
    // Taffy uses known_dimensions to calculate cross_axis_available_space for children.
    let (explicit_width, has_explicit_width) =
        resolve_explicit_dimension_width(ctx, node, constraints);
    let (explicit_height, has_explicit_height) =
        resolve_explicit_dimension_height(ctx, node, constraints);

    // FIX: For root nodes or nodes where the parent provides a definite size,
    // use the available_size as known_dimensions if no explicit CSS width/height is set.
    // This is critical for `align-self: stretch` to work - Taffy needs to know the
    // cross-axis size of the container to stretch children to fill it.
    let is_root = node.parent.is_none();
    
    // NOTE: For root nodes, margins are already handled by calculate_used_size_for_node()
    // which subtracts margin from the containing block width when resolving 'auto' width.
    // Therefore, constraints.available_size already reflects the margin-adjusted size.
    // We do NOT subtract margins again here - that would cause double subtraction.
    
    let effective_width = if has_explicit_width {
        explicit_width
    } else if is_root && constraints.available_size.width.is_finite() {
        // Root node: use available_size directly (margin already subtracted in sizing.rs)
        Some(constraints.available_size.width)
    } else {
        None
    };
    let effective_height = if has_explicit_height {
        explicit_height
    } else if is_root && constraints.available_size.height.is_finite() {
        // Root node: use available_size directly (margin already subtracted in sizing.rs)
        Some(constraints.available_size.height)
    } else {
        None
    };
    let has_effective_width = effective_width.is_some();
    let has_effective_height = effective_height.is_some();

    // FIX: Taffy interprets known_dimensions as Border Box size.
    // CSS width/height properties define Content Box size (by default, box-sizing: content-box).
    // We must add border and padding to the explicit dimensions to get the correct Border
    // Box size for Taffy.
    // NOTE: For root nodes using viewport size, no adjustment needed - viewport is already border-box.
    let width_adjustment = node.box_props.border.left
        + node.box_props.border.right
        + node.box_props.padding.left
        + node.box_props.padding.right;
    let height_adjustment = node.box_props.border.top
        + node.box_props.border.bottom
        + node.box_props.padding.top
        + node.box_props.padding.bottom;

    // Apply adjustment only if dimensions come from explicit CSS (convert content-box to border-box)
    // For root nodes using viewport size, no adjustment needed
    let adjusted_width = if has_explicit_width {
        explicit_width.map(|w| w + width_adjustment)
    } else {
        effective_width // Already in border-box for viewport
    };
    let adjusted_height = if has_explicit_height {
        explicit_height.map(|h| h + height_adjustment)
    } else {
        effective_height // Already in border-box for viewport
    };

    // CSS Flexbox § 9.2: Use InherentSize when explicit dimensions are set,
    // ContentSize for auto-sizing (shrink-to-fit behavior).
    let sizing_mode = if has_effective_width || has_effective_height {
        taffy::SizingMode::InherentSize
    } else {
        taffy::SizingMode::ContentSize
    };

    let known_dimensions = TaffySize {
        width: adjusted_width,
        height: adjusted_height,
    };

    // parent_size tells Taffy the size of the container's parent.
    // For root nodes, the "parent" is the viewport, but since margins are already
    // handled by calculate_used_size_for_node(), we use containing_block_size directly.
    // For non-root nodes, containing_block_size is already the parent's content-box.
    let parent_size = translate_taffy_size(constraints.containing_block_size);

    let taffy_inputs = LayoutInput {
        known_dimensions,
        parent_size,
        available_space,
        run_mode: taffy::RunMode::PerformLayout,
        sizing_mode,
        axis: taffy::RequestedAxis::Both,
        // Flex and Grid containers establish a new BFC, preventing margin collapse.
        vertical_margins_are_collapsible: Line::FALSE,
    };

    debug_info!(
        ctx,
        "CALLING LAYOUT_TAFFY FOR FLEX/GRID FC node_index={:?}",
        node_index
    );

    // Cache border values before the mutable borrow in layout_taffy_subtree
    let border_left = node.box_props.border.left;
    let border_top = node.box_props.border.top;

    let taffy_output =
        taffy_bridge::layout_taffy_subtree(ctx, tree, text_cache, node_index, taffy_inputs);

    // Collect child positions from the tree (Taffy stores results directly on nodes).
    let mut output = LayoutOutput::default();
    // Use content_size for overflow detection, not container size.
    // content_size represents the actual size of all children, which may exceed the container.
    //
    // Taffy's content_size is measured from (0,0) of the border-box, so it includes
    // border.top/left as a leading offset.  The scrollbar geometry and scroll clamp
    // both measure inside the padding-box (border stripped).  Subtract the start
    // border so that overflow_size is in the same coordinate space as the viewport
    // (padding-box), preventing extra scroll range equal to the border width.
    let raw = translate_taffy_size_back(taffy_output.content_size);
    output.overflow_size = LogicalSize::new(
        (raw.width - border_left).max(0.0),
        (raw.height - border_top).max(0.0),
    );

    let children: Vec<usize> = tree.children(node_index).to_vec();
    for &child_idx in &children {
        if let Some(child_node) = tree.get(child_idx) {
            if let Some(pos) = child_node.relative_position {
                output.positions.insert(child_idx, pos);
            }
        }
    }

    Ok(BfcLayoutResult::from_output(output))
}

/// Resolves explicit CSS width to pixel value for Taffy layout.
fn resolve_explicit_dimension_width<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
    constraints: &LayoutConstraints,
) -> (Option<f32>, bool) {
    node.dom_node_id
        .map(|id| {
            let width = get_css_width(
                ctx.styled_dom,
                id,
                &ctx.styled_dom.styled_nodes.as_container()[id].styled_node_state,
            );
            match width.unwrap_or_default() {
                LayoutWidth::Auto => (None, false),
                LayoutWidth::Px(px) => {
                    let pixels = resolve_size_metric(
                        px.metric,
                        px.number.get(),
                        constraints.available_size.width,
                        ctx.viewport_size,
                    );
                    (Some(pixels), true)
                }
                LayoutWidth::MinContent | LayoutWidth::MaxContent | LayoutWidth::FitContent(_) => (None, false),
                LayoutWidth::Calc(_) => (None, false), // TODO: resolve calc
            }
        })
        .unwrap_or((None, false))
}

/// Resolves explicit CSS height to pixel value for Taffy layout.
fn resolve_explicit_dimension_height<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
    constraints: &LayoutConstraints,
) -> (Option<f32>, bool) {
    node.dom_node_id
        .map(|id| {
            let height = get_css_height(
                ctx.styled_dom,
                id,
                &ctx.styled_dom.styled_nodes.as_container()[id].styled_node_state,
            );
            match height.unwrap_or_default() {
                LayoutHeight::Auto => (None, false),
                LayoutHeight::Px(px) => {
                    let pixels = resolve_size_metric(
                        px.metric,
                        px.number.get(),
                        constraints.available_size.height,
                        ctx.viewport_size,
                    );
                    (Some(pixels), true)
                }
                LayoutHeight::MinContent | LayoutHeight::MaxContent | LayoutHeight::FitContent(_) => (None, false),
                LayoutHeight::Calc(_) => (None, false), // TODO: resolve calc
            }
        })
        .unwrap_or((None, false))
}

// +spec:floats:167a2c - Float positioning rules (CSS 2.2 § 9.5.1): left/right/none, precise placement constraints
// +spec:floats:6a1769 - Float shortens line boxes, margins never collapse, stacking order
// +spec:floats:15bfd9 - float:right positions element at line-right edge within BFC
// +spec:floats:afc8e2 - Float positioning rules (CSS 2.2 § 9.5 rules 1-8): left/right edge containment, earlier-float stacking, outer-top constraints, and "move down" when insufficient space
/// Position a float within a BFC, considering existing floats.
/// Returns the LogicalRect (margin box) for the float.
// +spec:box-model:db0f02 - Float positioning: line boxes shortened by floats, floats shift down if no space, BFC elements must not overlap float margin boxes
// +spec:containing-block:136e45 - Float shifted left/right until outer edge touches containing block edge or another float
// +spec:containing-block:3ebb4e - Content moves below floats when containing block too narrow
// +spec:floats:45fce7 - Float positioning: pulled out of flow, line boxes shortened around float
// +spec:floats:f6c218 - float pulled out of flow, line boxes shorten around it
// +spec:height-calculation:86142a - CSS 2.2 §9.5 float positioning, clearance, and margin non-collapsing
// +spec:width-calculation:761677 - float positioning: content flows around floats, line boxes shortened by float presence
fn position_float(
    float_ctx: &FloatingContext,
    float_type: LayoutFloat,
    size: LogicalSize,
    margin: &EdgeSizes,
    current_main_offset: f32,
    bfc_cross_size: f32,
    wm: LayoutWritingMode,
) -> LogicalRect {
    // Start at the current main-axis position (Y in horizontal-tb)
    let mut main_start = current_main_offset;

    // Calculate total size including margins
    let total_main = size.main(wm) + margin.main_start(wm) + margin.main_end(wm);
    let total_cross = size.cross(wm) + margin.cross_start(wm) + margin.cross_end(wm);

    // +spec:floats:3d89d8 - shift float downward when not enough horizontal room
    // Find a position where the float fits
    let cross_start = loop {
        let (avail_start, avail_end) = float_ctx.available_line_box_space(
            main_start,
            main_start + total_main,
            bfc_cross_size,
            wm,
        );

        let available_width = avail_end - avail_start;

        if available_width >= total_cross {
            // +spec:floats:449158 - left float positioned at line-left, content flows on right
            // Found space that fits
            if float_type == LayoutFloat::Left {
                // +spec:writing-modes:84bcba - floats positioned at line-left / line-right
                // Position at line-left (avail_start)
                break avail_start + margin.cross_start(wm);
            } else {
                // Position at line-right (avail_end - size)
                break avail_end - total_cross + margin.cross_start(wm);
            }
        }

        // top is moved lower than earlier float's bottom (outer edge / margin box bottom)
        // Not enough space at this Y, move down past the lowest overlapping float's margin box bottom
        let next_main = float_ctx
            .floats
            .iter()
            .filter(|f| {
                let f_main_start = f.rect.origin.main(wm) - f.margin.main_start(wm);
                let f_main_end = f_main_start + f.rect.size.main(wm)
                    + f.margin.main_start(wm) + f.margin.main_end(wm);
                f_main_end > main_start && f_main_start < main_start + total_main
            })
            .map(|f| f.rect.origin.main(wm) + f.rect.size.main(wm) + f.margin.main_end(wm))
            .max_by(|a, b| a.partial_cmp(b).unwrap());

        if let Some(next) = next_main {
            main_start = next;
        } else {
            // No overlapping floats found, use current position anyway
            if float_type == LayoutFloat::Left {
                break avail_start + margin.cross_start(wm);
            } else {
                break avail_end - total_cross + margin.cross_start(wm);
            }
        }
    };

    LogicalRect {
        origin: LogicalPosition::from_main_cross(
            main_start + margin.main_start(wm),
            cross_start,
            wm,
        ),
        size,
    }
}

// Block Formatting Context (CSS 2.2 § 9.4.1)

/// Lays out a Block Formatting Context (BFC).
///
/// This is the corrected, architecturally-sound implementation. It solves the
/// "chicken-and-egg" problem by performing its own two-pass layout:
///
/// 1. **Sizing Pass:** It first iterates through its children and triggers their layout recursively
///    by calling `calculate_layout_for_subtree`. This ensures that the `used_size` property of each
///    child is correctly populated.
///
/// 2. **Positioning Pass:** It then iterates through the children again. Now that each child has a
///    valid size, it can apply the standard block-flow logic: stacking them vertically and
///    advancing a "pen" by each child's outer height.
///
/// # Margin Collapsing Architecture
///
/// CSS 2.1 Section 8.3.1 compliant margin collapsing:
///
/// ```text
/// layout_bfc()
///   ├─ Check parent border/padding blockers
///   ├─ For each child:
///   │   ├─ Check child border/padding blockers
///   │   ├─ is_first_child?
///   │   │   └─ Check parent-child top collapse
///   │   ├─ Sibling collapse?
///   │   │   └─ advance_pen_with_margin_collapse()
///   │   │       └─ collapse_margins(prev_bottom, curr_top)
///   │   ├─ Position child
///   │   ├─ is_empty_block()?
///   │   │   └─ Collapse own top+bottom margins (collapse through)
///   │   └─ Save bottom margin for next sibling
///   └─ Check parent-child bottom collapse
/// ```
///
/// **Collapsing Rules:**
///
/// - Sibling margins: Adjacent vertical margins collapse to max (or sum if mixed signs)
/// - Parent-child: First child's top margin can escape parent (if no border/padding)
/// - Parent-child: Last child's bottom margin can escape parent (if no border/padding/height)
/// - Empty blocks: Top+bottom margins collapse with each other, then with siblings
/// - Blockers: Border, padding, inline content, or new BFC prevents collapsing
///
/// This approach is compliant with the CSS visual formatting model and works within
/// the constraints of the existing layout engine architecture.
// +spec:display-property:f38f52 - BFC handles normal flow, relative positioning offsets, and float extraction (CSS 2.2 § 9.8)
fn layout_bfc<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints,
    float_cache: &mut HashMap<usize, FloatingContext>,
) -> Result<BfcLayoutResult> {
    let node = tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?
        .clone();
    // +spec:block-formatting-context:4f4ff6 - writing-mode determines block flow direction (main axis) for ordering block-level boxes in BFC
    let writing_mode = constraints.writing_mode;
    let mut output = LayoutOutput::default();

    debug_info!(
        ctx,
        "\n[layout_bfc] ENTERED for node_index={}, children.len()={}, incoming_bfc_state={}",
        node_index,
        tree.children(node_index).len(),
        constraints.bfc_state.is_some()
    );

    // Initialize FloatingContext for this BFC
    //
    // We always recalculate float positions in this pass, but we'll store them in the cache
    // so that subsequent layout passes (for auto-sizing) have access to the positioned floats
    let mut float_context = FloatingContext::default();

    // +spec:containing-block:42b75f - Block element establishes containing block for inline content (IFC)
    // Calculate this node's content-box size for use as containing block for children
    // CSS 2.2 § 10.1: The containing block for in-flow children is formed by the
    // content edge of the parent's content box.
    //
    // We use constraints.available_size directly as this already represents the
    // content-box available to this node (set by parent). For nodes with explicit
    // sizes, used_size contains the border-box which we convert to content-box.
    //
    // NOTE(writing-modes): The containing block size uses physical width/height.
    // In vertical writing modes, the block progression direction is horizontal,
    // so the "available width" for children maps to the physical height of
    // the containing block. The main_pen variable below tracks block progression
    // using logical main-axis coordinates; the WritingModeContext in constraints
    // determines how main/cross map to physical x/y via from_main_cross().
    // +spec:inline-block:17944a - orthogonal flow roots get infinite available inline space here (not yet detected)
    // +spec:inline-block:a60e22 - other layout models pass through infinite inline space to contained block containers
    let mut children_containing_block_size = if let Some(used_size) = node.used_size {
        // Node has explicit used_size (border-box) - convert to content-box
        node.box_props.inner_size(used_size, writing_mode)
    } else {
        // No used_size yet - use available_size directly (this is already content-box
        // when coming from parent's layout constraints)
        constraints.available_size
    };

    // +spec:overflow:ffe6f7 - scrollbar space subtracted from containing block per spec §11.1.1
    // Reserve space for vertical scrollbar when appropriate.
    //
    // - overflow: scroll  → ALWAYS reserve (CSS spec: scrollbar always shown)
    // - overflow: auto    → Reserve ONLY when a previous pass / the anti-jitter
    //   merge (`merge_scrollbar_info`) already determined a scrollbar is needed.
    //   On the very first pass the node has no scrollbar_info yet, so no space
    //   is reserved.  After `compute_scrollbar_info` detects overflow it sets
    //   `reflow_needed_for_scrollbars = true`, triggering a second pass where
    //   `node.scrollbar_info.needs_vertical == true` and space IS reserved.
    //   The merge uses `||` (keep once detected), preventing cross-frame jitter.
    let scrollbar_reservation = node
        .dom_node_id
        .map(|dom_id| {
            let styled_node_state = ctx
                .styled_dom
                .styled_nodes
                .as_container()
                .get(dom_id)
                .map(|s| s.styled_node_state.clone())
                .unwrap_or_default();
            let overflow_y =
                crate::solver3::getters::get_overflow_y(ctx.styled_dom, dom_id, &styled_node_state);
            use azul_css::props::layout::LayoutOverflow;
            match overflow_y.unwrap_or_default() {
                LayoutOverflow::Scroll => {
                    crate::solver3::getters::get_layout_scrollbar_width_px(ctx, dom_id, &styled_node_state)
                }
                LayoutOverflow::Auto => {
                    let already_needs = node.scrollbar_info
                        .as_ref()
                        .map(|s| s.needs_vertical)
                        .unwrap_or(false);
                    if already_needs {
                        crate::solver3::getters::get_layout_scrollbar_width_px(ctx, dom_id, &styled_node_state)
                    } else {
                        0.0
                    }
                }
                _ => 0.0,
            }
        })
        .unwrap_or(0.0);

    if scrollbar_reservation > 0.0 {
        children_containing_block_size.width =
            (children_containing_block_size.width - scrollbar_reservation).max(0.0);
    }

    // === Pass 1: Pre-compute child sizes (restored two-pass BFC) ===
    //
    // Inspired by Taffy's two-pass approach: first measure, then position.
    //
    // This was removed in commit 1a3e5850 and replaced with a single-pass approach
    // that computed sizes just-in-time during positioning. The single-pass approach
    // caused regression 8e092a2e because positioning decisions (margin collapsing,
    // float clearance, available width after floats) depend on knowing ALL sibling
    // sizes upfront, not just the ones visited so far.
    //
    // With the per-node cache (§9.1-§9.2), the re-added Pass 1 is efficient:
    // - Each child subtree is computed once and stored in NodeCache
    // - Pass 2 positioning reads sizes from tree nodes (used_size set by Pass 1)
    // - When calculate_layout_for_subtree recurses into children after layout_bfc
    //   returns, it hits the per-node cache (same available_size) — O(1) per child.
    //
    // Performance: O(n) for the tree. No double-computation thanks to caching.
    {
        let mut temp_positions: super::PositionVec = Vec::new();
        let mut temp_scrollbar_reflow = false;

        let bfc_children = tree.children(node_index).to_vec();
        for &child_index in &bfc_children {
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let child_dom_id = child_node.dom_node_id;

            // +spec:positioning:447b06 - Absolute positioning pulls element out of flow, skip from normal layout
            // Skip absolutely/fixed positioned children — they're laid out separately
            // +spec:positioning:c7e5c5 - out-of-flow elements ignored for word boundary / hyphenation
            let position_type = get_position_type(ctx.styled_dom, child_dom_id);
            if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
                continue;
            }

            // Compute the child's full subtree layout with temporary positions.
            // Position (0,0) is intentionally wrong — Pass 1 only cares about sizing.
            // The correct positions are determined in Pass 2 below.
            crate::solver3::cache::calculate_layout_for_subtree(
                ctx,
                tree,
                text_cache,
                child_index,
                LogicalPosition::zero(),
                children_containing_block_size,
                &mut temp_positions,
                &mut temp_scrollbar_reflow,
                float_cache,
                crate::solver3::cache::ComputeMode::ComputeSize,
            )?;
        }
    }

    // +spec:block-formatting-context:98b633 - CSS 2.2 § 9.4.1: boxes laid out vertically, margins collapse
    // === Pass 2: Position children using known sizes ===
    //
    // All children now have used_size set from Pass 1. This pass handles:
    // - Margin collapsing (parent-child + sibling-sibling)
    // - Float positioning and clearance
    // - Normal flow block positioning

    let mut main_pen = 0.0f32;
    let mut max_cross_size = 0.0f32;

    // Track escaped margins separately from content-box height
    // CSS 2.2 § 8.3.1: Escaped margins don't contribute to parent's content-box height,
    // but DO affect sibling positioning within the parent
    let mut total_escaped_top_margin = 0.0f32;
    // Track all inter-sibling margins (collapsed) - these are also not part of content height
    let mut total_sibling_margins = 0.0f32;

    // Margin collapsing state
    let mut last_margin_bottom = 0.0f32;
    let mut is_first_child = true;
    let mut first_child_index: Option<usize> = None;
    let mut last_child_index: Option<usize> = None;

    // Parent's own margins (for escape calculation)
    let parent_margin_top = node.box_props.margin.main_start(writing_mode);
    let parent_margin_bottom = node.box_props.margin.main_end(writing_mode);

    // +spec:margin-collapsing:2476d8 - margins do not collapse across formatting context boundaries
    // If this node establishes an independent formatting context (new BFC), its margins
    // must NOT collapse with its children's margins. The children are in a different FC.
    let is_bfc_root = node.parent.is_none() || establishes_new_bfc(ctx, &node);

    // Check if parent (this BFC root) has border/padding that prevents parent-child collapse
    let parent_has_top_blocker = is_bfc_root
        || has_margin_collapse_blocker(&node.box_props, writing_mode, true);
    let parent_has_bottom_blocker = is_bfc_root
        || has_margin_collapse_blocker(&node.box_props, writing_mode, false);

    // Track accumulated top margin for first-child escape
    let mut accumulated_top_margin = 0.0f32;
    let mut top_margin_resolved = false;
    // Track if first child's margin escaped (for return value)
    let mut top_margin_escaped = false;

    // Track if we have any actual content (non-empty blocks)
    let mut has_content = false;

    // +spec:display-property:9f6e18 - BFC dispatches normal flow, floats, and relative positioning (CSS 2.2 §9.8)
    let pos_children = tree.children(node_index).to_vec();
    for &child_index in &pos_children {
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_dom_id = child_node.dom_node_id;

        // +spec:floats:2cec1b - 'position' and 'float' determine the positioning algorithm
        // +spec:positioning:dccad6 - floats only apply to non-absolutely-positioned boxes
        let position_type = get_position_type(ctx.styled_dom, child_dom_id);
        if position_type == LayoutPosition::Absolute || position_type == LayoutPosition::Fixed {
            continue;
        }

        // +spec:floats:2cec1b - float property determines positioning algorithm (float path)
        // +spec:floats:f6c0b2 - floats only processed in BFC; other formatting contexts (flex/grid) inhibit floating
        // Check if this child is a float - if so, position it at current main_pen
        let is_float = if let Some(node_id) = child_dom_id {
            let float_type = get_float_property(ctx.styled_dom, Some(node_id));

            if float_type != LayoutFloat::None {
                // Calculate float size just-in-time if not already computed
                let float_size = match child_node.used_size {
                    Some(size) => size,
                    None => {
                        let intrinsic = child_node.intrinsic_sizes.unwrap_or_default();
                        let computed_size = crate::solver3::sizing::calculate_used_size_for_node(
                            ctx.styled_dom,
                            child_dom_id,
                            children_containing_block_size,
                            intrinsic,
                            &child_node.box_props,
                            ctx.viewport_size,
                        )?;
                        if let Some(node_mut) = tree.get_mut(child_index) {
                            node_mut.used_size = Some(computed_size);
                        }
                        computed_size
                    }
                };
                // Re-borrow after potential mutation
                let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
                let float_margin = &child_node.box_props.margin;

                // +spec:floats:d0d163 - clear on floats adds constraint #10: float top below cleared floats' bottom
                let float_clear = get_clear_property(ctx.styled_dom, Some(node_id));
                let float_y = if float_clear != LayoutClear::None {
                    float_context.clearance_offset(float_clear, main_pen + last_margin_bottom, writing_mode)
                } else {
                    // +spec:floats:ef96cb - Float margins never collapse with adjacent margins
                    // CSS 2.2 § 9.5: Float margins don't collapse with any other margins.
                    main_pen + last_margin_bottom
                };

                debug_info!(
                    ctx,
                    "[layout_bfc] Positioning float: index={}, type={:?}, size={:?}, at Y={} \
                     (main_pen={} + last_margin={})",
                    child_index,
                    float_type,
                    float_size,
                    float_y,
                    main_pen,
                    last_margin_bottom
                );

                // Position the float at the CURRENT main_pen + last margin (respects DOM order!)
                let float_rect = position_float(
                    &float_context,
                    float_type,
                    float_size,
                    float_margin,
                    // Include last_margin_bottom since float margins don't collapse!
                    float_y,
                    constraints.available_size.cross(writing_mode),
                    writing_mode,
                );

                debug_info!(ctx, "[layout_bfc] Float positioned at: {:?}", float_rect);

                // Add to float context BEFORE positioning next element
                float_context.add_float(float_type, float_rect, *float_margin);

                // Store position in output
                output.positions.insert(child_index, float_rect.origin);

                debug_info!(
                    ctx,
                    "[layout_bfc] *** FLOAT POSITIONED: child={}, main_pen={} (unchanged - floats \
                     don't advance pen)",
                    child_index,
                    main_pen
                );

                // Floats are taken out of normal flow - DON'T advance main_pen
                // Continue to next child
                continue;
            }
            false
        } else {
            false
        };

        // Early exit for floats (already handled above)
        if is_float {
            continue;
        }

        // From here: normal flow (non-float) children only

        // Track first and last in-flow children for parent-child collapse
        if first_child_index.is_none() {
            first_child_index = Some(child_index);
        }
        last_child_index = Some(child_index);

        // Calculate child's used_size just-in-time if not already computed
        // This replaces the old "Pass 1" that recursively laid out grandchildren with wrong positions
        let child_size = match child_node.used_size {
            Some(size) => size,
            None => {
                // Calculate size without recursive layout
                let intrinsic = child_node.intrinsic_sizes.unwrap_or_default();
                let child_used_size = crate::solver3::sizing::calculate_used_size_for_node(
                    ctx.styled_dom,
                    child_dom_id,
                    children_containing_block_size,
                    intrinsic,
                    &child_node.box_props,
                    ctx.viewport_size,
                )?;
                // Update the node with computed size (we need to re-borrow mutably)
                if let Some(node_mut) = tree.get_mut(child_index) {
                    node_mut.used_size = Some(child_used_size);
                }
                child_used_size
            }
        };
        // Re-borrow child_node after potential mutation
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_margin = &child_node.box_props.margin;

        debug_info!(
            ctx,
            "[layout_bfc] Child {} margin from box_props: top={}, right={}, bottom={}, left={}",
            child_index,
            child_margin.top,
            child_margin.right,
            child_margin.bottom,
            child_margin.left
        );

        // IMPORTANT: Use the ACTUAL margins from box_props, NOT escaped margins!
        //
        // Escaped margins are only relevant for the parent-child relationship WITHIN a node's
        // own BFC layout. When positioning this child in ITS parent's BFC, we use its actual
        // margins. CSS 2.2 § 8.3.1: Margin collapsing happens between ADJACENT margins,
        // which means:
        //
        // - Parent's top and first child's top (if no blocker)
        // - Sibling's bottom and next sibling's top
        // - Parent's bottom and last child's bottom (if no blocker)
        //
        // The escaped_top_margin stored in the child node is for its OWN children, not for itself!
        // +spec:block-formatting-context:0f802c - margins use containing block's writing mode for collapsing/auto expansion in orthogonal flows
        let child_margin_top = child_margin.main_start(writing_mode);
        let child_margin_bottom = child_margin.main_end(writing_mode);

        debug_info!(
            ctx,
            "[layout_bfc] Child {} final margins: margin_top={}, margin_bottom={}",
            child_index,
            child_margin_top,
            child_margin_bottom
        );

        // Check if this child has border/padding that prevents margin collapsing
        let child_has_top_blocker =
            has_margin_collapse_blocker(&child_node.box_props, writing_mode, true);
        let child_has_bottom_blocker =
            has_margin_collapse_blocker(&child_node.box_props, writing_mode, false);

        // +spec:floats:dc195a - Clear property only applies to block-level elements (CSS 2.2 § 9.5.2)
        // Check for clear property FIRST - clearance affects whether element is considered empty
        // CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing"
        // An element with clearance is NOT empty even if it has no content
        let child_clear = if let Some(node_id) = child_dom_id {
            get_clear_property(ctx.styled_dom, Some(node_id))
        } else {
            LayoutClear::None
        };
        debug_info!(
            ctx,
            "[layout_bfc] Child {} clear property: {:?}",
            child_index,
            child_clear
        );

        // PHASE 1: Empty Block Detection & Self-Collapse
        let is_empty = is_empty_block(tree, child_index);

        // Handle empty blocks FIRST (they collapse through and don't participate in layout)
        // EXCEPTION: Elements with clear property are NOT skipped even if empty!
        // CSS 2.2 § 9.5.2: Clear property affects positioning even for empty elements
        if is_empty
            && !child_has_top_blocker
            && !child_has_bottom_blocker
            && child_clear == LayoutClear::None
        {
            // Empty block: collapse its own top and bottom margins FIRST
            let self_collapsed = collapse_margins(child_margin_top, child_margin_bottom);

            // Then collapse with previous margin (sibling or parent)
            if is_first_child {
                is_first_child = false;
                // Empty first child: its collapsed margin can escape with parent's
                if !parent_has_top_blocker {
                    accumulated_top_margin = collapse_margins(parent_margin_top, self_collapsed);
                } else {
                    // Parent has blocker: add margins
                    if accumulated_top_margin == 0.0 {
                        accumulated_top_margin = parent_margin_top;
                    }
                    main_pen += accumulated_top_margin + self_collapsed;
                    top_margin_resolved = true;
                    accumulated_top_margin = 0.0;
                }
                last_margin_bottom = self_collapsed;
            } else {
                // Empty sibling: collapse with previous sibling's bottom margin
                last_margin_bottom = collapse_margins(last_margin_bottom, self_collapsed);
            }

            // Skip positioning and pen advance (empty has no visual presence)
            continue;
        }

        // From here on: non-empty blocks only (or empty blocks with clear property)

        // Apply clearance if needed
        // +spec:floats:148ee6 - clear:left pushes element below float; clearance added above top margin
        // CSS 2.2 § 9.5.2: Clearance inhibits margin collapsing
        let clearance_applied = if child_clear != LayoutClear::None {
            let cleared_offset =
                float_context.clearance_offset(child_clear, main_pen, writing_mode);
            debug_info!(
                ctx,
                "[layout_bfc] Child {} clearance check: cleared_offset={}, main_pen={}",
                child_index,
                cleared_offset,
                main_pen
            );
            if cleared_offset > main_pen {
                debug_info!(
                    ctx,
                    "[layout_bfc] Applying clearance: child={}, clear={:?}, old_pen={}, new_pen={}",
                    child_index,
                    child_clear,
                    main_pen,
                    cleared_offset
                );
                main_pen = cleared_offset;
                true // Signal that clearance was applied
            } else {
                false
            }
        } else {
            false
        };

        // PHASE 2: Parent-Child Top Margin Escape (First Child)
        //
        // CSS 2.2 § 8.3.1: "The top margin of a box is adjacent to the top margin of its first
        // in-flow child if the box has no top border, no top padding, and the child has no
        // clearance." CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing"

        if is_first_child {
            is_first_child = false;

            // Clearance prevents collapse (acts as invisible blocker)
            if clearance_applied {
                // Clearance inhibits all margin collapsing for this element
                // The clearance has already positioned main_pen past floats
                //
                // CSS 2.2 § 8.3.1: Parent's margin was already handled by parent's parent BFC
                // We only add child's margin in our content-box coordinate space
                main_pen += child_margin_top;
                debug_info!(
                    ctx,
                    "[layout_bfc] First child {} with CLEARANCE: no collapse, child_margin={}, \
                     main_pen={}",
                    child_index,
                    child_margin_top,
                    main_pen
                );
            } else if !parent_has_top_blocker {
                // Margin Escape Case
                //
                // CSS 2.2 § 8.3.1: "The top margin of an in-flow block element collapses with
                // its first in-flow block-level child's top margin if the element has no top
                // border, no top padding, and the child has no clearance."
                //
                // When margins collapse, they "escape" upward through the parent to be resolved
                // in the grandparent's coordinate space. This is critical for understanding the
                // coordinate system separation:
                //
                // Example:
                // <body padding=20>
                //  <div margin=0>
                //      <div margin=30></div>
                //  </div>
                // </body>
                //
                //   - Middle div (our parent) has no padding → margins can escape
                //   - Inner div's 30px margin collapses with middle div's 0px margin = 30px
                //   - This 30px margin "escapes" to be handled by body's BFC
                //   - Body positions middle div at Y=30 (relative to body's content-box)
                //   - Middle div's content-box height does NOT include the escaped 30px
                //   - Inner div is positioned at Y=0 in middle div's content-box
                //
                // **NOTE**: This is a subtle but critical distinction in coordinate systems:
                //
                //   - Parent's margin belongs to grandparent's coordinate space
                //   - Child's margin (when escaped) also belongs to grandparent's coordinate space
                //   - They collapse BEFORE entering this BFC's coordinate space
                //   - We return the collapsed margin so grandparent can position parent correctly
                //
                // **NOTE**: Child's own blocker status (padding/border) is IRRELEVANT for
                // parent-child  collapse. The child may have padding that prevents
                // collapse with ITS OWN  children, but this doesn't prevent its
                // margin from escaping  through its parent.
                //
                // **NOTE**: Previously, we incorrectly added parent_margin_top to main_pen in
                //  the blocked case, which double-counted the margin by mixing
                //  coordinate systems. The parent's margin is NEVER in our (the
                //  parent's content-box) coordinate system!
                //
                // We collapse the parent's margin with the child's margin.
                // This combined margin is what "escapes" to the grandparent.
                // The grandparent uses this to position the parent.
                //
                // Effectively, we are saying "The parent starts here, but its effective
                // top margin is now max(parent_margin, child_margin)".

                accumulated_top_margin = collapse_margins(parent_margin_top, child_margin_top);
                top_margin_resolved = true;
                top_margin_escaped = true;

                // Track escaped margin so it gets subtracted from content-box height
                // The escaped margin is NOT part of our content-box - it belongs to our
                // parent's parent
                total_escaped_top_margin = accumulated_top_margin;

                // Position child at pen (no margin applied - it escaped!)
                debug_info!(
                    ctx,
                    "[layout_bfc] First child {} margin ESCAPES: parent_margin={}, \
                     child_margin={}, collapsed={}, total_escaped={}",
                    child_index,
                    parent_margin_top,
                    child_margin_top,
                    accumulated_top_margin,
                    total_escaped_top_margin
                );
            } else {
                // Margin Blocked Case
                //
                // CSS 2.2 § 8.3.1: "no top padding and no top border" required for collapse.
                // When padding or border exists, margins do NOT collapse and exist in different
                // coordinate spaces.
                //
                // CRITICAL COORDINATE SYSTEM SEPARATION:
                //
                //   This is where the architecture becomes subtle. When layout_bfc() is called:
                //   1. We are INSIDE the parent's content-box coordinate space (main_pen starts at
                //      0)
                //   2. The parent's own margin was ALREADY RESOLVED by the grandparent's BFC
                //   3. The parent's margin is in the grandparent's coordinate space, not ours
                //   4. We NEVER reference the parent's margin in this BFC - it's outside our scope
                //
                // Example:
                //
                // <body padding=20>
                //   <div margin=30 padding=20>
                //      <div margin=30></div>
                //   </div>
                // </body>
                //
                //   - Middle div has padding=20 → blocker exists, margins don't collapse
                //   - Body's BFC positions middle div at Y=30 (middle div's margin, in body's
                //     space)
                //   - Middle div's BFC starts at its content-box (after the padding)
                //   - main_pen=0 at the top of middle div's content-box
                //   - Inner div has margin=30 → we add 30 to main_pen (in OUR coordinate space)
                //   - Inner div positioned at Y=30 (relative to middle div's content-box)
                //   - Absolute position: 20 (body padding) + 30 (middle margin) + 20 (middle
                //     padding) + 30 (inner margin) = 100px
                //
                // **NOTE**: Previous code incorrectly added parent_margin_top to main_pen here:
                //
                //     - main_pen += parent_margin_top;  // WRONG! Mixes coordinate systems
                //     - main_pen += child_margin_top;
                //
                //   This caused the "double margin" bug where margins were applied twice:
                //
                //   - Once by grandparent positioning parent (correct)
                //   - Again inside parent's BFC (INCORRECT - wrong coordinate system)
                //
                //   The parent's margin belongs to GRANDPARENT's coordinate space and was already
                //   used to position the parent. Adding it again here is like adding feet to
                //   meters.
                //
                //   We ONLY add the child's margin in our (parent's content-box) coordinate space.
                //   The parent's margin is irrelevant to us - it's outside our scope.

                main_pen += child_margin_top;
                debug_info!(
                    ctx,
                    "[layout_bfc] First child {} BLOCKED: parent_has_blocker={}, advanced by \
                     child_margin={}, main_pen={}",
                    child_index,
                    parent_has_top_blocker,
                    child_margin_top,
                    main_pen
                );
            }
        } else {
            // Not first child: handle sibling collapse
            // CSS 2.2 § 8.3.1 Rule 1: "Vertical margins of adjacent block boxes in the normal flow
            // collapse" CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing"

            // Resolve accumulated top margin if not yet done (for parent's first in-flow child)
            if !top_margin_resolved {
                main_pen += accumulated_top_margin;
                top_margin_resolved = true;
                debug_info!(
                    ctx,
                    "[layout_bfc] RESOLVED top margin for node {} at sibling {}: accumulated={}, \
                     main_pen={}",
                    node_index,
                    child_index,
                    accumulated_top_margin,
                    main_pen
                );
            }

            if clearance_applied {
                // Clearance inhibits collapsing - add full margin
                main_pen += child_margin_top;
                debug_info!(
                    ctx,
                    "[layout_bfc] Child {} with CLEARANCE: no collapse with sibling, \
                     child_margin_top={}, main_pen={}",
                    child_index,
                    child_margin_top,
                    main_pen
                );
            } else {
                // Sibling Margin Collapse
                //
                // CSS 2.2 § 8.3.1: "Vertical margins of adjacent block boxes in the normal
                // flow collapse." The collapsed margin is the maximum of the two margins.
                //
                // IMPORTANT: Sibling margins ARE part of the parent's content-box height!
                //
                // Unlike escaped margins (which belong to grandparent's space), sibling margins
                // are the space BETWEEN children within our content-box.
                //
                // Example:
                //
                // <div>
                //  <div margin-bottom=30></div>
                //  <div margin-top=40></div>
                // </div>
                //
                //   - First child ends at Y=100 (including its content + margins)
                //   - Collapsed margin = max(30, 40) = 40px
                //   - Second child starts at Y=140 (100 + 40)
                //   - Parent's content-box height includes this 40px gap
                //
                // We track total_sibling_margins for debugging, but NOTE: we do **not**
                // subtract these from content-box height! They are part of the layout space.
                //
                // Previously we subtracted total_sibling_margins from content-box height:
                //
                //   content_box_height = main_pen - total_escaped_top_margin -
                // total_sibling_margins;
                //
                // This was wrong because sibling margins are between boxes (part of content),
                // not outside boxes (like escaped margins).

                let collapsed = collapse_margins(last_margin_bottom, child_margin_top);
                main_pen += collapsed;
                total_sibling_margins += collapsed;
                debug_info!(
                    ctx,
                    "[layout_bfc] Sibling collapse for child {}: last_margin_bottom={}, \
                     child_margin_top={}, collapsed={}, main_pen={}, total_sibling_margins={}",
                    child_index,
                    last_margin_bottom,
                    child_margin_top,
                    collapsed,
                    main_pen,
                    total_sibling_margins
                );
            }
        }

        // Position child (non-empty blocks only reach here)
        //
        // +spec:block-formatting-context:1dada5 - Normal flow boxes in BFC touch containing block edge
        // +spec:block-formatting-context:9f56cb - each box's left outer edge touches containing block left edge; new BFC may shrink due to floats
        // CSS 2.2 § 9.4.1: "In a block formatting context, each box's left outer edge touches
        // the left edge of the containing block (for right-to-left formatting, right edges touch).
        // This is true even in the presence of floats (although a box's line boxes may shrink
        // due to the floats), unless the box establishes a new block formatting context
        // (in which case the box itself may become narrower due to the floats)."
        //
        // +spec:block-formatting-context:3d2811 - Float overlap with normal flow element borders
        // +spec:display-property:796059 - BFC/replaced/table border box must not overlap float margin boxes; line boxes shorten around floats
        // +spec:floats:5214a6 - BFC/replaced/table border box must not overlap float margin boxes; shrink or clear below
        // CSS 2.2 § 9.5: "The border box of a table, a block-level replaced element, or an element
        // in the normal flow that establishes a new block formatting context (such as an element
        // with 'overflow' other than 'visible') must not overlap any floats in the same block
        // formatting context as the element itself."

        // +spec:floats:a29f70 - BFC roots, tables, and block-level replaced elements must not overlap float margin boxes
        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let avoids_floats = establishes_new_bfc(ctx, child_node)
            || is_block_level_replaced(ctx, child_node);

        // Query available space considering floats ONLY if child avoids floats
        let (cross_start, cross_end, available_cross) = if avoids_floats {
            // New BFC / replaced / table: Must shrink or move down to avoid overlapping floats
            let child_cross_needed = child_size.cross(writing_mode);
            let bfc_cross = constraints.available_size.cross(writing_mode);

            let (mut start, mut end) = float_context.available_line_box_space(
                main_pen,
                main_pen + child_size.main(writing_mode),
                bfc_cross,
                writing_mode,
            );
            let mut available = end - start;

            // CSS 2.2 § 9.5: "If necessary, implementations should clear the said element
            // by placing it below any preceding floats, but may place it adjacent to such
            // floats if there is sufficient space."
            if available < child_cross_needed && !float_context.floats.is_empty() {
                let clear_to = float_context.floats.iter()
                    .filter(|f| {
                        let f_main_start = f.rect.origin.main(writing_mode) - f.margin.main_start(writing_mode);
                        let f_main_end = f_main_start + f.rect.size.main(writing_mode)
                            + f.margin.main_start(writing_mode) + f.margin.main_end(writing_mode);
                        f_main_end > main_pen && f_main_start < main_pen + child_size.main(writing_mode)
                    })
                    .map(|f| {
                        f.rect.origin.main(writing_mode) + f.rect.size.main(writing_mode)
                            + f.margin.main_end(writing_mode)
                    })
                    .fold(main_pen, f32::max);

                if clear_to > main_pen {
                    main_pen = clear_to;
                    let (s, e) = float_context.available_line_box_space(
                        main_pen,
                        main_pen + child_size.main(writing_mode),
                        bfc_cross,
                        writing_mode,
                    );
                    start = s;
                    end = e;
                    available = end - start;
                }
            }

            debug_info!(
                ctx,
                "[layout_bfc] Child {} avoids floats: shrinking to avoid floats, \
                 cross_range={}..{}, available_cross={}",
                child_index,
                start,
                end,
                available
            );

            (start, end, available)
        } else {
            // Normal flow: Overlaps floats, positioned at full width
            // Only the child's INLINE CONTENT (if any) wraps around floats
            let start = 0.0;
            let end = constraints.available_size.cross(writing_mode);
            let available = end - start;

            debug_info!(
                ctx,
                "[layout_bfc] Child {} is normal flow: overlapping floats at full width, \
                 available_cross={}",
                child_index,
                available
            );

            (start, end, available)
        };

        // Get child's margin, margin_auto, size, and formatting context
        let (child_margin_cloned, child_margin_auto, child_used_size, is_inline_fc, child_dom_id_for_debug) = {
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            (
                child_node.box_props.margin.clone(),
                child_node.box_props.margin_auto,
                child_node.used_size.unwrap_or_default(),
                child_node.formatting_context == FormattingContext::Inline,
                child_node.dom_node_id,
            )
        };
        let child_margin = &child_margin_cloned;

        debug_info!(
            ctx,
            "[layout_bfc] Child {} margin_auto: left={}, right={}, top={}, bottom={}",
            child_index,
            child_margin_auto.left,
            child_margin_auto.right,
            child_margin_auto.top,
            child_margin_auto.bottom
        );
        debug_info!(
            ctx,
            "[layout_bfc] Child {} used_size: width={}, height={}",
            child_index,
            child_used_size.width,
            child_used_size.height
        );

        // Position child
        // For normal flow blocks (including IFCs): position at full width (cross_start = 0)
        // For BFC-establishing blocks: position in available space between floats
        //
        // CSS 2.2 § 10.3.3: If margin-left and margin-right are both auto,
        // their used values are equal, centering the element horizontally.
        
        let (child_cross_pos, mut child_main_pos) = if avoids_floats {
            // BFC: Position in space between floats
            (
                cross_start + child_margin.cross_start(writing_mode),
                main_pen,
            )
        } else {
            // Normal flow: Check for margin: auto centering
            let available_cross = constraints.available_size.cross(writing_mode);
            let child_cross_size = child_used_size.cross(writing_mode);
            
            debug_info!(
                ctx,
                "[layout_bfc] Child {} centering check: available_cross={}, child_cross_size={}, margin_auto.left={}, margin_auto.right={}",
                child_index,
                available_cross,
                child_cross_size,
                child_margin_auto.left,
                child_margin_auto.right
            );
            
            // +spec:block-formatting-context:d52ce5 - auto margins resolved per containing block's writing mode for centering
            // +spec:width-calculation:0c5044 - auto margins center element on cross axis (respects writing mode)
            // +spec:width-calculation:25c2fc - §10.3.3: block-level margin auto centering and over-constrained resolution
            // +spec:width-calculation:ba691f - auto margins treated as zero when element overflows containing block (via .max(0.0) on remaining_space)
            // +spec:width-calculation:324e7e - both margin-left and margin-right auto => equal used values (centering)
            // CSS 2.2 § 10.3.3: If both margin-left and margin-right are auto,
            // center the element within the available space
            let cross_pos = if child_margin_auto.left && child_margin_auto.right {
                // Center: (available - child_width) / 2
                let remaining_space = (available_cross - child_cross_size).max(0.0);
                debug_info!(
                    ctx,
                    "[layout_bfc] Child {} CENTERING: remaining_space={}, cross_pos={}",
                    child_index,
                    remaining_space,
                    remaining_space / 2.0
                );
                remaining_space / 2.0
            } else if child_margin_auto.left {
                // Only left is auto: push element to the right
                let remaining_space = (available_cross - child_cross_size - child_margin.right).max(0.0);
                debug_info!(
                    ctx,
                    "[layout_bfc] Child {} margin-left:auto only, pushing right: remaining_space={}",
                    child_index,
                    remaining_space
                );
                remaining_space
            } else if child_margin_auto.right {
                // Only right is auto: element stays at left with its margin
                debug_info!(
                    ctx,
                    "[layout_bfc] Child {} margin-right:auto only, using left margin={}",
                    child_index,
                    child_margin.cross_start(writing_mode)
                );
                child_margin.cross_start(writing_mode)
            } else {
                // +spec:box-model:218643 - over-constrained: drop end margin per containing block writing mode
                // +spec:width-calculation:d172a4 - over-constrained: LTR ignores margin-right, RTL ignores margin-left
                // in LTR, margin-right is ignored (element positioned at margin-left);
                // in RTL, margin-left is ignored (element positioned from right edge)
                let is_rtl = tree.get(node_index)
                    .and_then(|n| n.dom_node_id)
                    .map_or(false, |cb_dom_id| {
                        let node_state = ctx.styled_dom.styled_nodes.as_container()
                            .get(cb_dom_id)
                            .map(|s| s.styled_node_state.clone())
                            .unwrap_or_default();
                        matches!(
                            get_direction_property(ctx.styled_dom, cb_dom_id, &node_state),
                            MultiValue::Exact(StyleDirection::Rtl)
                        )
                    });
                let cross_pos = if is_rtl {
                    // RTL: ignore margin-left, position from right edge
                    available_cross - child_cross_size - child_margin.cross_end(writing_mode)
                } else {
                    // LTR (default): ignore margin-right, position at margin-left
                    child_margin.cross_start(writing_mode)
                };
                debug_info!(
                    ctx,
                    "[layout_bfc] Child {} NO auto margins (over-constrained), is_rtl={}, cross_pos={}",
                    child_index,
                    is_rtl,
                    cross_pos
                );
                cross_pos
            };
            
            (cross_pos, main_pen)
        };

        // NOTE: We do NOT adjust child_main_pos based on child's escaped_top_margin here!
        // The escaped_top_margin represents margins that escaped FROM the child's own children.
        // The child's position in THIS BFC is determined by main_pen and the child's own margin
        // (which was already handled in the margin collapse logic above).
        //
        // Previously, this code incorrectly added child_escaped_margin to child_main_pos,
        // which caused double-application of margins because:
        // 1. The child's margin was used to calculate its position in THIS BFC
        // 2. Then its escaped_top_margin (which included its own margin) was added again
        //
        // The correct behavior per CSS 2.2 § 8.3.1 is:
        // - The child's escaped_top_margin is used by THIS node's parent to position THIS node
        // - It does NOT affect how we position the child within our content-box

        // final_pos is [CoordinateSpace::Parent] - relative to this BFC's content-box
        let final_pos =
            LogicalPosition::from_main_cross(child_main_pos, child_cross_pos, writing_mode);

        debug_info!(
            ctx,
            "[layout_bfc] *** NORMAL FLOW BLOCK POSITIONED: child={}, final_pos={:?}, \
             main_pen={}, avoids_floats={}",
            child_index,
            final_pos,
            main_pen,
            avoids_floats
        );

        // Re-layout IFC children with float context for correct text wrapping
        // Normal flow blocks WITH inline content need float context propagated
        if is_inline_fc && !avoids_floats {
            // Use cached floats if available (from previous layout passes),
            // otherwise use the floats positioned in this pass
            let floats_for_ifc = float_cache.get(&node_index).unwrap_or(&float_context);

            debug_info!(
                ctx,
                "[layout_bfc] Re-layouting IFC child {} (normal flow) with parent's float context \
                 at Y={}, child_cross_pos={}",
                child_index,
                main_pen,
                child_cross_pos
            );
            debug_info!(
                ctx,
                "[layout_bfc]   Using {} floats (from cache: {})",
                floats_for_ifc.floats.len(),
                float_cache.contains_key(&node_index)
            );

            // Translate float coordinates from BFC-relative to IFC-relative
            // The IFC child is positioned at (child_cross_pos, main_pen) in BFC coordinates
            // Floats need to be relative to the IFC's CONTENT-BOX origin (inside padding/border)
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let padding_border_cross = child_node.box_props.padding.cross_start(writing_mode)
                + child_node.box_props.border.cross_start(writing_mode);
            let padding_border_main = child_node.box_props.padding.main_start(writing_mode)
                + child_node.box_props.border.main_start(writing_mode);

            // Content-box origin in BFC coordinates
            let content_box_cross = child_cross_pos + padding_border_cross;
            let content_box_main = main_pen + padding_border_main;

            debug_info!(
                ctx,
                "[layout_bfc]   Border-box at ({}, {}), Content-box at ({}, {}), \
                 padding+border=({}, {})",
                child_cross_pos,
                main_pen,
                content_box_cross,
                content_box_main,
                padding_border_cross,
                padding_border_main
            );

            let mut ifc_floats = FloatingContext::default();
            for float_box in &floats_for_ifc.floats {
                // Convert float position from BFC coords to IFC CONTENT-BOX relative coords
                let float_rel_to_ifc = LogicalRect {
                    origin: LogicalPosition {
                        x: float_box.rect.origin.x - content_box_cross,
                        y: float_box.rect.origin.y - content_box_main,
                    },
                    size: float_box.rect.size,
                };

                debug_info!(
                    ctx,
                    "[layout_bfc] Float {:?}: BFC coords = {:?}, IFC-content-relative = {:?}",
                    float_box.kind,
                    float_box.rect,
                    float_rel_to_ifc
                );

                ifc_floats.add_float(float_box.kind, float_rel_to_ifc, float_box.margin);
            }

            // Create a BfcState with IFC-relative float coordinates
            let mut bfc_state = BfcState {
                pen: LogicalPosition::zero(), // IFC starts at its own origin
                floats: ifc_floats.clone(),
                margins: MarginCollapseContext::default(),
            };

            debug_info!(
                ctx,
                "[layout_bfc]   Created IFC-relative FloatingContext with {} floats",
                ifc_floats.floats.len()
            );

            // Get the IFC child's content-box size (after padding/border)
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let child_dom_id = child_node.dom_node_id;

            // +spec:containing-block:a8ada9 - line box width determined by containing block and floats
            // For inline elements (display: inline), use containing block width as available
            // width. Inline elements flow within the containing block and wrap at its width.
            // CSS 2.2 § 10.3.1: For inline elements, available width = containing block width.
            let display = get_display_property(ctx.styled_dom, child_dom_id).unwrap_or_default();
            let child_content_size = if display == LayoutDisplay::Inline {
                // Inline elements use the containing block's content-box width
                LogicalSize::new(
                    children_containing_block_size.width,
                    children_containing_block_size.height,
                )
            } else {
                // Block-level elements use their own content-box
                child_node.box_props.inner_size(child_size, writing_mode)
            };

            debug_info!(
                ctx,
                "[layout_bfc]   IFC child size: border-box={:?}, content-box={:?}",
                child_size,
                child_content_size
            );

            // Create new constraints with float context
            // IMPORTANT: Use the child's CONTENT-BOX width, not the BFC width!
            let ifc_constraints = LayoutConstraints {
                available_size: child_content_size,
                bfc_state: Some(&mut bfc_state),
                writing_mode,
                writing_mode_ctx: constraints.writing_mode_ctx,
                text_align: constraints.text_align,
                containing_block_size: constraints.containing_block_size,
                available_width_type: Text3AvailableSpace::Definite(child_content_size.width),
            };

            // Re-layout the IFC with float awareness
            // This will pass floats as exclusion zones to text3 for line wrapping
            let ifc_result = layout_formatting_context(
                ctx,
                tree,
                text_cache,
                child_index,
                &ifc_constraints,
                float_cache,
            )?;

            // DON'T update used_size - the box keeps its full width!
            // Only the text layout inside changes to wrap around floats

            debug_info!(
                ctx,
                "[layout_bfc] IFC child {} re-layouted with float context (text will wrap, box \
                 stays full width)",
                child_index
            );

            // NOTE: We do NOT merge inline-block positions from the IFC's output.positions here!
            // The IFC's inline-block children will be correctly positioned when 
            // calculate_layout_for_subtree recursively processes the IFC node (child_index).
            // At that point, layout_ifc will be called again, and the inline-block positions
            // will be relative to the IFC's content-box, which is what we want.
            //
            // Merging them here would cause them to be processed by process_inflow_child
            // with the BFC's content-box position (self_content_box_pos of the BFC), 
            // resulting in incorrect absolute positions.
        }

        output.positions.insert(child_index, final_pos);

        // CSS margin collapse: escaped margins are handled via accumulated_top_margin
        // at the START of layout, not by adjusting positions after layout.
        // We simply advance by the child's actual size.
        main_pen += child_size.main(writing_mode);
        has_content = true;

        // Update last margin for next sibling
        // CSS 2.2 § 8.3.1: The bottom margin of this box will collapse with the top margin
        // of the next sibling (if no clearance or blockers intervene)
        // element (between prev sibling's bottom and this element's top margin). The cleared
        // element's bottom margin is still available for normal collapsing with the next sibling.
        // CSS 2.2 § 9.5.2: "Clearance inhibits margin collapsing and acts as spacing above
        // the margin-top of an element."
        last_margin_bottom = child_margin_bottom;

        debug_info!(
            ctx,
            "[layout_bfc] Child {} positioned at final_pos={:?}, size={:?}, advanced main_pen to \
             {}, last_margin_bottom={}, clearance_applied={}",
            child_index,
            final_pos,
            child_size,
            main_pen,
            last_margin_bottom,
            clearance_applied
        );

        // Track the maximum cross-axis size to determine the BFC's overflow size.
        let child_cross_extent =
            child_cross_pos + child_size.cross(writing_mode) + child_margin.cross_end(writing_mode);
        max_cross_size = max_cross_size.max(child_cross_extent);
    }

    // Store the float context in cache for future layout passes
    // This happens after ALL children (floats and normal) have been positioned
    debug_info!(
        ctx,
        "[layout_bfc] Storing {} floats in cache for node {}",
        float_context.floats.len(),
        node_index
    );
    float_cache.insert(node_index, float_context.clone());

    // PHASE 3: Parent-Child Bottom Margin Escape
    let mut escaped_top_margin = None;
    let mut escaped_bottom_margin = None;

    // Handle top margin escape
    if top_margin_escaped {
        // First child's margin escaped through parent
        escaped_top_margin = Some(accumulated_top_margin);
        debug_info!(
            ctx,
            "[layout_bfc] Returning escaped top margin: accumulated={}, node={}",
            accumulated_top_margin,
            node_index
        );
    } else if !top_margin_resolved && accumulated_top_margin > 0.0 {
        // No content was positioned, all margins accumulated (empty blocks)
        escaped_top_margin = Some(accumulated_top_margin);
        debug_info!(
            ctx,
            "[layout_bfc] Escaping top margin (no content): accumulated={}, node={}",
            accumulated_top_margin,
            node_index
        );
    } else if !top_margin_resolved {
        // Unusual case: no content, zero margin
        escaped_top_margin = Some(accumulated_top_margin);
        debug_info!(
            ctx,
            "[layout_bfc] Escaping top margin (zero, no content): accumulated={}, node={}",
            accumulated_top_margin,
            node_index
        );
    } else {
        debug_info!(
            ctx,
            "[layout_bfc] NOT escaping top margin: top_margin_resolved={}, escaped={}, \
             accumulated={}, node={}",
            top_margin_resolved,
            top_margin_escaped,
            accumulated_top_margin,
            node_index
        );
    }

    // Handle bottom margin escape
    if let Some(last_idx) = last_child_index {
        let last_child = tree.get(last_idx).ok_or(LayoutError::InvalidTree)?;
        let last_has_bottom_blocker =
            has_margin_collapse_blocker(&last_child.box_props, writing_mode, false);

        debug_info!(
            ctx,
            "[layout_bfc] Bottom margin for node {}: parent_has_bottom_blocker={}, \
             last_has_bottom_blocker={}, last_margin_bottom={}, main_pen_before={}",
            node_index,
            parent_has_bottom_blocker,
            last_has_bottom_blocker,
            last_margin_bottom,
            main_pen
        );

        if !parent_has_bottom_blocker && !last_has_bottom_blocker && has_content {
            // Last child's bottom margin can escape
            let collapsed_bottom = collapse_margins(parent_margin_bottom, last_margin_bottom);
            escaped_bottom_margin = Some(collapsed_bottom);
            debug_info!(
                ctx,
                "[layout_bfc] Bottom margin ESCAPED for node {}: collapsed={}",
                node_index,
                collapsed_bottom
            );
            // Don't add last_margin_bottom to pen (it escaped)
        } else {
            // Can't escape: add to pen
            main_pen += last_margin_bottom;
            // NOTE: We do NOT add parent_margin_bottom to main_pen here!
            // parent_margin_bottom is added OUTSIDE the content-box (in the margin-box)
            // The content-box height should only include children's content and margins
            debug_info!(
                ctx,
                "[layout_bfc] Bottom margin BLOCKED for node {}: added last_margin_bottom={}, \
                 main_pen_after={}",
                node_index,
                last_margin_bottom,
                main_pen
            );
        }
    } else {
        // No children: just use parent's margins
        if !top_margin_resolved {
            main_pen += parent_margin_top;
        }
        main_pen += parent_margin_bottom;
    }

    // CRITICAL: If this is a root node (no parent), apply escaped margins directly
    // instead of propagating them upward (since there's no parent to receive them)
    let is_root_node = node.parent.is_none();
    if is_root_node {
        if let Some(top) = escaped_top_margin {
            // Adjust all child positions downward by the escaped top margin
            for (_, pos) in output.positions.iter_mut() {
                let current_main = pos.main(writing_mode);
                *pos = LogicalPosition::from_main_cross(
                    current_main + top,
                    pos.cross(writing_mode),
                    writing_mode,
                );
            }
            main_pen += top;
        }
        if let Some(bottom) = escaped_bottom_margin {
            main_pen += bottom;
        }
        // For root nodes, don't propagate margins further
        escaped_top_margin = None;
        escaped_bottom_margin = None;
    }

    // CSS 2.2 § 9.5: Floats don't contribute to container height with overflow:visible
    //
    // However, browsers DO expand containers to contain floats in specific cases:
    //
    // 1. If there's NO in-flow content (main_pen == 0), floats determine height
    // 2. If container establishes a BFC (overflow != visible)
    //
    // In this case, we have in-flow content (main_pen > 0) and overflow:visible,
    // so floats should NOT expand the container. Their margins can "bleed" beyond
    // the container boundaries into the parent.
    //
    // This matches Chrome/Firefox behavior where float margins escape through
    // the container's padding when there's existing in-flow content.

    // +spec:block-formatting-context:7954a2 - 10.6.3: auto height for block-level non-replaced elements in normal flow
    // Content-box Height Calculation
    //
    // CSS 2.2 § 8.3.1: "The top border edge of the box is defined to coincide with
    // the top border edge of the [first] child" when margins collapse/escape.
    //
    // This means escaped margins do NOT contribute to the parent's content-box height.
    //
    // Calculation:
    //
    //   main_pen = total vertical space used by all children and margins
    //
    //   Components of main_pen:
    //
    //   1. Children's border-boxes (always included)
    //   2. Sibling collapsed margins (space BETWEEN children - part of content)
    //   3. First child's position (0 if margin escaped, margin_top if blocked)
    //
    //   What to subtract:
    //
    //   - total_escaped_top_margin: First child's margin that went to grandparent's space This
    //     margin is OUTSIDE our content-box, so we must subtract it.
    //
    //   What NOT to subtract:
    //
    //   - total_sibling_margins: These are the gaps BETWEEN children, which are
    //    legitimately part of our content area's layout space.
    //
    // Example with escaped margin:
    //   <div class="parent" padding=0>              <!-- Node 2 -->
    //     <div class="child1" margin=30></div>      <!-- Node 3, margin escapes -->
    //     <div class="child2" margin=40></div>      <!-- Node 5 -->
    //   </div>
    //
    //   Layout process:
    //
    //   - Node 3 positioned at main_pen=0 (margin escaped)
    //   - Node 3 size=140px → main_pen advances to 140
    //   - Sibling collapse: max(30 child1 bottom, 40 child2 top) = 40px
    //   - main_pen advances to 180
    //   - Node 5 size=130px → main_pen advances to 310
    //   - total_escaped_top_margin = 30
    //   - total_sibling_margins = 40 (tracked but NOT subtracted)
    //   - content_box_height = 310 - 30 = 280px ✓
    //
    // Previously, we calculated:
    //
    //   content_box_height = main_pen - total_escaped_top_margin - total_sibling_margins
    //
    // This incorrectly subtracted sibling margins, making parent too small.
    // Sibling margins are *between* boxes (part of layout), not *outside* boxes
    // (like escaped margins).

    // +spec:box-model:4eebed - auto height for BFC = top margin-edge of topmost child to bottom margin-edge of bottommost child
    // +spec:box-model:4eebed - auto height = top margin-edge of topmost child to bottom margin-edge of bottommost child
    let mut content_box_height = main_pen - total_escaped_top_margin;

    // +spec:block-formatting-context:f73d3e - BFC root grows to fully contain its floats; floats from outside cannot protrude in
    // whose bottom margin edge exceeds bottom content edge; only floats participating
    // in this BFC are counted (not floats inside abspos descendants or nested BFCs)
    if is_bfc_root {
        for float_box in &float_context.floats {
            let float_bottom_margin_edge = float_box.rect.origin.main(writing_mode)
                + float_box.rect.size.main(writing_mode)
                + float_box.margin.main_end(writing_mode);
            if float_bottom_margin_edge > content_box_height {
                content_box_height = float_bottom_margin_edge;
            }
        }
    }

    // +spec:display-contents:f6de1a - content height overflow tracked via overflow_size
    // +spec:overflow:043182 - overflow computed from box bounds + children overflow
    output.overflow_size =
        LogicalSize::from_main_cross(content_box_height, max_cross_size, writing_mode);

    debug_info!(
        ctx,
        "[layout_bfc] FINAL for node {}: main_pen={}, total_escaped_top={}, \
         total_sibling_margins={}, content_box_height={}",
        node_index,
        main_pen,
        total_escaped_top_margin,
        total_sibling_margins,
        content_box_height
    );

    // Baseline calculation would happen here in a full implementation.
    output.baseline = None;

    // Store escaped margins in the LayoutNode for use by parent
    if let Some(node_mut) = tree.get_mut(node_index) {
        node_mut.escaped_top_margin = escaped_top_margin;
        node_mut.escaped_bottom_margin = escaped_bottom_margin;
    }

    if let Some(node_mut) = tree.get_mut(node_index) {
        node_mut.baseline = output.baseline;
    }

    Ok(BfcLayoutResult {
        output,
        escaped_top_margin,
        escaped_bottom_margin,
    })
}

// Inline Formatting Context (CSS 2.2 § 9.4.2)
// +spec:display-property:ede6f4 - inline layout: mixed stream of text and inline-level boxes

/// Lays out an Inline Formatting Context (IFC) by delegating to the `text3` engine.
///
/// This function acts as a bridge between the box-tree world of `solver3` and the
/// rich text layout world of `text3`. Its responsibilities are:
///
/// 1. **Collect Content**: Traverse the direct children of the IFC root and convert them into a
///    `Vec<InlineContent>`, the input format for `text3`. This involves:
///
///     - Recursively laying out `inline-block` children to determine their final size and baseline,
///       which are then passed to `text3` as opaque objects.
///     - Extracting raw text runs from inline text nodes.
///
/// 2. **Translate Constraints**: Convert the `LayoutConstraints` (available space, floats) from
///    `solver3` into the more detailed `UnifiedConstraints` that `text3` requires.
///
/// 3. **Invoke Text Layout**: Call the `text3` cache's `layout_flow` method to perform the complex
///    tasks of BIDI analysis, shaping, line breaking, justification, and vertical alignment.
/// +spec:display-property:e96c82 - inline formatting context: flow of elements/text wrapped into lines
///
/// 4. **Integrate Results**: Process the `UnifiedLayout` returned by `text3`:
///
///     - Store the rich layout result on the IFC root `LayoutNode` for the display list generation
///       pass.
///     - Update the `positions` map for all `inline-block` children based on the positions
///       calculated by `text3`.
///     - Extract the final overflow size and baseline for the IFC root itself
// NOTE(writing-modes): The IFC currently assumes inline direction = horizontal
// and block direction = vertical. In vertical writing modes, line boxes would
// stack horizontally and inline content would flow vertically. The writing mode
// is now available via constraints.writing_mode_ctx for agents to use when
// implementing vertical text layout in the text3 engine.
// +spec:display-property:574e7b - text-box-trim for inline boxes trims block-end to content edge (TODO: implement trimming per text-box-edge metric)
// +spec:display-property:da284a - IFC: flow inline-level boxes into line boxes, size/position each fragment
// +spec:inline-formatting-context:275f64 - IFC: boxes laid out horizontally into line boxes, respecting margins/borders/padding
fn layout_ifc<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    tree: &mut LayoutTree,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    let ifc_start = (ctx.get_system_time_fn.cb)();

    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;;
    let float_count = constraints
        .bfc_state
        .as_ref()
        .map(|s| s.floats.floats.len())
        .unwrap_or(0);
    debug_info!(
        ctx,
        "[layout_ifc] ENTRY: node_index={}, has_bfc_state={}, float_count={}",
        node_index,
        constraints.bfc_state.is_some(),
        float_count
    );
    debug_ifc_layout!(ctx, "CALLED for node_index={}", node_index);

    // +spec:display-property:7f3c1d - Anonymous inline boxes: text directly in block containers treated as anonymous inline elements in IFC
    // +spec:display-property:5a795c - root inline box: block container generates anonymous inline box holding all inline-level contents, inheriting from parent
    // For anonymous boxes, we need to find the DOM ID from a parent or child
    // CSS 2.2 § 9.2.1.1: Anonymous boxes inherit properties from their enclosing box
    let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
    let ifc_root_dom_id = match node.dom_node_id {
        Some(id) => id,
        None => {
            // Anonymous box - get DOM ID from parent or first child with DOM ID
            let parent_dom_id = node
                .parent
                .and_then(|p| tree.get(p))
                .and_then(|n| n.dom_node_id);

            if let Some(id) = parent_dom_id {
                id
            } else {
                // Try to find DOM ID from first child
                tree.children(node_index)
                    .iter()
                    .filter_map(|&child_idx| tree.get(child_idx))
                    .filter_map(|n| n.dom_node_id)
                    .next()
                    .ok_or(LayoutError::InvalidTree)?
            }
        }
    };

    debug_ifc_layout!(ctx, "ifc_root_dom_id={:?}", ifc_root_dom_id);

    // +spec:display-property:a469a6 - line boxes created as needed for inline-level content in IFC
    // +spec:display-property:f3c875 - calculate layout bounds (size contributions) of each inline-level box
    // Phase 1: Collect and measure all inline-level children.
    let phase1_start = (ctx.get_system_time_fn.cb)();
    let (inline_content, child_map) =
        collect_and_measure_inline_content(ctx, text_cache, tree, node_index, constraints)?;
    let _phase1_time = (ctx.get_system_time_fn.cb)().duration_since(&phase1_start);

    debug_info!(
        ctx,
        "[layout_ifc] Collected {} inline content items for node {}",
        inline_content.len(),
        node_index
    );
    if inline_content.len() > 10 {
        let _text_count = inline_content.iter().filter(|i| matches!(i, InlineContent::Text(_))).count();
        let _shape_count = inline_content.iter().filter(|i| matches!(i, InlineContent::Shape(_))).count();
    }
    for (i, item) in inline_content.iter().enumerate() {
        match item {
            InlineContent::Text(run) => debug_info!(ctx, "  [{}] Text: '{}'", i, run.text),
            InlineContent::Marker {
                run,
                position_outside,
            } => debug_info!(
                ctx,
                "  [{}] Marker: '{}' (outside={})",
                i,
                run.text,
                position_outside
            ),
            InlineContent::Shape(_) => debug_info!(ctx, "  [{}] Shape", i),
            InlineContent::Image(_) => debug_info!(ctx, "  [{}] Image", i),
            _ => debug_info!(ctx, "  [{}] Other", i),
        }
    }

    debug_ifc_layout!(
        ctx,
        "Collected {} inline content items",
        inline_content.len()
    );

    if inline_content.is_empty() {
        debug_warning!(ctx, "inline_content is empty, returning default output!");
        return Ok(LayoutOutput::default());
    }

    // === Phase 2c stub: IFC incremental relayout decision tree ===
    //
    // When a cached IFC layout exists and only specific items are dirty,
    // we can potentially skip full text_cache.layout_flow() and just:
    //   - Reshape only the dirty items (IfcOnly scope)
    //   - Shift x_offsets for subsequent items on the same line (nowrap fast path)
    //   - Or partial line-break reflow from the affected line onward
    //
    // For now, this is a no-op: we always fall through to full relayout.
    // The item_metrics on CachedInlineLayout enable this optimization
    // once Phase 2d is implemented.
    let _cached_ifc = tree
        .get(node_index)
        .and_then(|n| n.inline_layout_result.as_ref());
    // TODO(Phase 2d): Check dirty children's RelayoutScope via item_metrics.
    //   If max scope is None → return cached layout directly (repaint only).
    //   If max scope is IfcOnly and all dirty items are on nowrap lines
    //     → reshape + shift, skip layout_flow().
    //   Otherwise → full layout_flow() below.

    // Phase 2: Translate constraints and define a single layout fragment for text3.
    let text3_constraints =
        translate_to_text3_constraints(ctx, constraints, ctx.styled_dom, ifc_root_dom_id);

    // Clone constraints for caching (before they're moved into fragments)
    let cached_constraints = text3_constraints.clone();

    debug_info!(
        ctx,
        "[layout_ifc] CALLING text_cache.layout_flow for node {} with {} exclusions",
        node_index,
        text3_constraints.shape_exclusions.len()
    );

    let fragments = vec![LayoutFragment {
        id: "main".to_string(),
        constraints: text3_constraints,
    }];

    // Phase 3: Invoke the text layout engine.
    // Get pre-loaded fonts from font manager (fonts should be loaded before layout)
    let phase3_start = (ctx.get_system_time_fn.cb)();
    let loaded_fonts = ctx.font_manager.get_loaded_fonts();
    let text_layout_result = match text_cache.layout_flow(
        &inline_content,
        &[],
        &fragments,
        &ctx.font_manager.font_chain_cache,
        &ctx.font_manager.fc_cache,
        &loaded_fonts,
        ctx.debug_messages,
    ) {
        Ok(result) => result,
        Err(e) => {
            // Font errors should not stop layout of other elements.
            // Log the error and return a zero-sized layout.
            debug_warning!(ctx, "Text layout failed: {:?}", e);
            debug_warning!(
                ctx,
                "Continuing with zero-sized layout for node {}",
                node_index
            );

            let mut output = LayoutOutput::default();
            output.overflow_size = LogicalSize::new(0.0, 0.0);
            return Ok(output);
        }
    };
    let _phase3_time = (ctx.get_system_time_fn.cb)().duration_since(&phase3_start);
    let _total_ifc_time = (ctx.get_system_time_fn.cb)().duration_since(&ifc_start);

    // Phase 4: Integrate results back into the solver3 layout tree.
    let mut output = LayoutOutput::default();
    let node = tree.get_mut(node_index).ok_or(LayoutError::InvalidTree)?;

    debug_ifc_layout!(
        ctx,
        "text_layout_result has {} fragment_layouts",
        text_layout_result.fragment_layouts.len()
    );

    if let Some(main_frag) = text_layout_result.fragment_layouts.get("main") {
        let frag_bounds = main_frag.bounds();
        debug_ifc_layout!(
            ctx,
            "Found 'main' fragment with {} items, bounds={}x{}",
            main_frag.items.len(),
            frag_bounds.width,
            frag_bounds.height
        );
        debug_ifc_layout!(ctx, "Storing inline_layout_result on node {}", node_index);

        // Determine if we should store this layout result using the new
        // CachedInlineLayout system. The key insight is that inline layouts
        // depend on available width:
        //
        // - Min-content measurement uses width ≈ 0 (maximum line wrapping)
        // - Max-content measurement uses width = ∞ (no line wrapping)
        // - Final layout uses the actual column/container width
        //
        // We must track which constraint type was used, otherwise a min-content
        // measurement would incorrectly be reused for final rendering.
        let has_floats = constraints
            .bfc_state
            .as_ref()
            .map(|s| !s.floats.floats.is_empty())
            .unwrap_or(false);
        let current_width_type = constraints.available_width_type;

        let should_store = match &node.inline_layout_result {
            None => {
                // No cached result - always store
                debug_info!(
                    ctx,
                    "[layout_ifc] Storing NEW inline_layout_result for node {} (width_type={:?}, \
                     has_floats={})",
                    node_index,
                    current_width_type,
                    has_floats
                );
                true
            }
            Some(cached) => {
                // Check if the new result should replace the cached one
                if cached.should_replace_with(current_width_type, has_floats) {
                    debug_info!(
                        ctx,
                        "[layout_ifc] REPLACING inline_layout_result for node {} (old: \
                         width={:?}, floats={}) with (new: width={:?}, floats={})",
                        node_index,
                        cached.available_width,
                        cached.has_floats,
                        current_width_type,
                        has_floats
                    );
                    true
                } else {
                    debug_info!(
                        ctx,
                        "[layout_ifc] KEEPING cached inline_layout_result for node {} (cached: \
                         width={:?}, floats={}, new: width={:?}, floats={})",
                        node_index,
                        cached.available_width,
                        cached.has_floats,
                        current_width_type,
                        has_floats
                    );
                    false
                }
            }
        };

        if should_store {
            node.inline_layout_result = Some(CachedInlineLayout::new_with_constraints(
                main_frag.clone(),
                current_width_type,
                has_floats,
                cached_constraints,
            ));
        }

        // Extract the overall size and baseline for the IFC root.
        // +spec:display-property:a0d0ab - IFC height = top of topmost line box to bottom of bottommost line box
        // +spec:display-property:a63b8f - baseline-source defaults to auto (last baseline for inline-block/IFC)
        output.overflow_size = LogicalSize::new(frag_bounds.width, frag_bounds.height);
        output.baseline = main_frag.last_baseline();
        node.baseline = output.baseline;

        // Position all the inline-block children based on text3's calculations.
        // [CoordinateSpace::Parent] - positions are relative to IFC's content-box (0,0)
        for positioned_item in &main_frag.items {
            if let ShapedItem::Object { source, content, .. } = &positioned_item.item {
                if let Some(&child_node_index) = child_map.get(source) {
                    // new_relative_pos is [CoordinateSpace::Parent] - relative to this IFC's content-box
                    let new_relative_pos = LogicalPosition {
                        x: positioned_item.position.x,
                        y: positioned_item.position.y,
                    };
                    output.positions.insert(child_node_index, new_relative_pos);
                }
            }
        }
    }

    Ok(output)
}

fn translate_taffy_size(size: LogicalSize) -> TaffySize<Option<f32>> {
    TaffySize {
        width: Some(size.width),
        height: Some(size.height),
    }
}

/// Helper: Convert StyleFontStyle to text3::cache::FontStyle
pub fn convert_font_style(style: StyleFontStyle) -> crate::font_traits::FontStyle {
    match style {
        StyleFontStyle::Normal => crate::font_traits::FontStyle::Normal,
        StyleFontStyle::Italic => crate::font_traits::FontStyle::Italic,
        StyleFontStyle::Oblique => crate::font_traits::FontStyle::Oblique,
    }
}

/// Helper: Convert StyleFontWeight to FcWeight
pub fn convert_font_weight(weight: StyleFontWeight) -> FcWeight {
    match weight {
        StyleFontWeight::W100 => FcWeight::Thin,
        StyleFontWeight::W200 => FcWeight::ExtraLight,
        StyleFontWeight::W300 | StyleFontWeight::Lighter => FcWeight::Light,
        StyleFontWeight::Normal => FcWeight::Normal,
        StyleFontWeight::W500 => FcWeight::Medium,
        StyleFontWeight::W600 => FcWeight::SemiBold,
        StyleFontWeight::Bold => FcWeight::Bold,
        StyleFontWeight::W800 => FcWeight::ExtraBold,
        StyleFontWeight::W900 | StyleFontWeight::Bolder => FcWeight::Black,
    }
}

/// Resolves a CSS size metric to pixels.
///
/// - `metric`: The CSS unit (px, pt, em, vw, etc.)
/// - `value`: The numeric value
/// - `containing_block_size`: Size of containing block (for percentage)
/// - `viewport_size`: Viewport dimensions (for vw, vh, vmin, vmax)
#[inline]
fn resolve_size_metric(
    metric: SizeMetric,
    value: f32,
    containing_block_size: f32,
    viewport_size: LogicalSize,
) -> f32 {
    match metric {
        SizeMetric::Px => value,
        SizeMetric::Pt => value * PT_TO_PX,
        SizeMetric::Percent => value / 100.0 * containing_block_size,
        SizeMetric::Em | SizeMetric::Rem => value * DEFAULT_FONT_SIZE,
        SizeMetric::Vw => value / 100.0 * viewport_size.width,
        SizeMetric::Vh => value / 100.0 * viewport_size.height,
        SizeMetric::Vmin => value / 100.0 * viewport_size.width.min(viewport_size.height),
        SizeMetric::Vmax => value / 100.0 * viewport_size.width.max(viewport_size.height),
        // In, Cm, Mm: convert to pixels using standard DPI (96)
        SizeMetric::In => value * 96.0,
        SizeMetric::Cm => value * 96.0 / 2.54,
        SizeMetric::Mm => value * 96.0 / 25.4,
    }
}

pub fn translate_taffy_size_back(size: TaffySize<f32>) -> LogicalSize {
    LogicalSize {
        width: size.width,
        height: size.height,
    }
}

pub fn translate_taffy_point_back(point: taffy::Point<f32>) -> LogicalPosition {
    LogicalPosition {
        x: point.x,
        y: point.y,
    }
}

// +spec:block-formatting-context:40e03e - BFC root: block container establishing new BFC (contains floats, excludes external floats, suppresses margin collapsing)
/// Checks if a node establishes a new Block Formatting Context (BFC).
///
/// Per CSS 2.2 § 9.4.1, a BFC is established by:
/// - Floats (elements with float other than 'none')
/// - Absolutely positioned elements (position: absolute or fixed)
/// - Block containers that are not block boxes (e.g., inline-blocks, table-cells)
/// - Block boxes with 'overflow' other than 'visible' and 'clip'
/// - Elements with 'display: flow-root'
/// - Table cells, table captions, and inline-blocks
///
/// Normal flow block-level boxes do NOT establish a new BFC.
///
/// This is critical for correct float interaction: normal blocks should overlap floats
/// (not shrink around them), while their inline content wraps around floats.
// +spec:block-formatting-context:241d22 - block container establishes new BFC or continues parent's, based on overflow/position/float/display
// +spec:block-formatting-context:9fe441 - BFC establishment based on position, float, overflow, and display properties
// +spec:display-property:3c7369 - block boxes establishing independent FC create new BFC; flex containers already do; non-replaced inlines cannot
// +spec:positioning:1e94f6 - floats, abspos, inline-blocks/table-cells/table-captions, overflow!=visible establish new BFC
fn establishes_new_bfc<T: ParsedFontTrait>(ctx: &LayoutContext<'_, T>, node: &LayoutNode) -> bool {
    // +spec:block-formatting-context:f39cd3 - table wrapper box establishes a BFC (CSS 2.2 §17.4)
    // Anonymous table wrapper boxes have no dom_node_id but must still establish BFC
    if node.anonymous_type == Some(AnonymousBoxType::TableWrapper) {
        return true;
    }
    let Some(dom_id) = node.dom_node_id else {
        return false;
    };

    let node_state = &ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

    // 1. Floats establish BFC
    let float_val = get_float(ctx.styled_dom, dom_id, node_state);
    if matches!(
        float_val,
        MultiValue::Exact(LayoutFloat::Left | LayoutFloat::Right)
    ) {
        return true;
    }

    // 2. Absolutely positioned elements establish BFC
    let position = crate::solver3::positioning::get_position_type(ctx.styled_dom, Some(dom_id));
    if matches!(position, LayoutPosition::Absolute | LayoutPosition::Fixed) {
        return true;
    }

    // 3. Inline-blocks, table-cells, table-captions establish BFC
    let display = get_display_property(ctx.styled_dom, Some(dom_id));
    if matches!(
        display,
        MultiValue::Exact(
            LayoutDisplay::InlineBlock | LayoutDisplay::TableCell | LayoutDisplay::TableCaption
        )
    ) {
        return true;
    }

    // 4. display: flow-root establishes BFC
    // +spec:display-property:14bae6 - flow-root establishes a formatting context that contains/excludes floats
    if matches!(display, MultiValue::Exact(LayoutDisplay::FlowRoot)) {
        return true;
    }

    // +spec:overflow:0a944d - clip does NOT establish BFC; hidden/scroll/auto do establish BFC
    // +spec:overflow:631a4c - scroll containers establish independent formatting context (BFC)
    // +spec:overflow:f6a186 - overflow:clip does NOT establish BFC; use display:flow-root for that
    // +spec:overflow:717de1 - overflow != visible/clip establishes BFC per CSS 2.2 §9.4.1
    // +spec:positioning:6feb32 - overflow:clip does NOT establish new formatting context; hidden/scroll/auto do
    // 5. Block boxes with overflow other than 'visible' or 'clip' establish BFC
    // Note: 'clip' does NOT establish BFC per CSS Overflow Module Level 3
    let overflow_x = get_overflow_x(ctx.styled_dom, dom_id, node_state);
    let overflow_y = get_overflow_y(ctx.styled_dom, dom_id, node_state);

    let creates_bfc_via_overflow = |ov: &MultiValue<LayoutOverflow>| {
        matches!(
            ov,
            &MultiValue::Exact(
                LayoutOverflow::Hidden | LayoutOverflow::Scroll | LayoutOverflow::Auto
            )
        )
    };

    if creates_bfc_via_overflow(&overflow_x) || creates_bfc_via_overflow(&overflow_y) {
        return true;
    }

    // 6. Table, Flex, and Grid containers establish BFC (via FormattingContext)
    // +spec:block-formatting-context:f15b87 - display:table participates in a BFC
    if matches!(
        node.formatting_context,
        FormattingContext::Table | FormattingContext::Flex | FormattingContext::Grid
    ) {
        return true;
    }

    // Normal flow block boxes do NOT establish BFC
    // NOTE: align-content != normal should also establish BFC per CSS-DISPLAY-3, but align-content is not yet implemented for block containers
    false
}

// +spec:display-property:5e5420 - replaced element identification (glossary: replaced elements have natural dimensions, establish independent formatting context)
/// CSS 2.2 § 9.5: "The border box of a table, a block-level replaced element, or an element
/// in the normal flow that establishes a new block formatting context [...] must not overlap
/// the margin box of any floats in the same block formatting context as the element itself."
fn is_block_level_replaced<T: ParsedFontTrait>(ctx: &LayoutContext<'_, T>, node: &LayoutNode) -> bool {
    let Some(dom_id) = node.dom_node_id else {
        return false;
    };

    // Check display is block-level
    let display = get_display_property(ctx.styled_dom, Some(dom_id));
    let is_block_level = matches!(
        display,
        MultiValue::Exact(LayoutDisplay::Block | LayoutDisplay::ListItem | LayoutDisplay::FlowRoot)
    );

    if !is_block_level {
        return false;
    }

    // Check if the element is a replaced element (image, video, etc.)
    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    matches!(
        node_data.get_node_type(),
        NodeType::Image(_)
    )
}

/// Translates solver3 layout constraints into the text3 engine's unified constraints.
fn translate_to_text3_constraints<'a, T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    constraints: &'a LayoutConstraints<'a>,
    styled_dom: &StyledDom,
    dom_id: NodeId,
) -> UnifiedConstraints {
    // Convert floats into exclusion zones for text3 to flow around.
    let mut shape_exclusions = if let Some(ref bfc_state) = constraints.bfc_state {
        debug_info!(
            ctx,
            "[translate_to_text3] dom_id={:?}, converting {} floats to exclusions",
            dom_id,
            bfc_state.floats.floats.len()
        );
        bfc_state
            .floats
            .floats
            .iter()
            .enumerate()
            .map(|(i, float_box)| {
                let rect = crate::text3::cache::Rect {
                    x: float_box.rect.origin.x,
                    y: float_box.rect.origin.y,
                    width: float_box.rect.size.width,
                    height: float_box.rect.size.height,
                };
                debug_info!(
                    ctx,
                    "[translate_to_text3]   Exclusion #{}: {:?} at ({}, {}) size {}x{}",
                    i,
                    float_box.kind,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height
                );
                ShapeBoundary::Rectangle(rect)
            })
            .collect()
    } else {
        debug_info!(
            ctx,
            "[translate_to_text3] dom_id={:?}, NO bfc_state - no float exclusions",
            dom_id
        );
        Vec::new()
    };

    debug_info!(
        ctx,
        "[translate_to_text3] dom_id={:?}, available_size={}x{}, shape_exclusions.len()={}",
        dom_id,
        constraints.available_size.width,
        constraints.available_size.height,
        shape_exclusions.len()
    );

    // Map text-align and justify-content from CSS to text3 enums.
    let id = dom_id;
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;

    // Read CSS Shapes properties
    // For reference box, use the element's CSS height if available, otherwise available_size
    // This is important because available_size.height might be infinite during auto height
    // calculation
    let ref_box_height = if constraints.available_size.height.is_finite() {
        constraints.available_size.height
    } else {
        // Try to get explicit CSS height
        // NOTE: If height is infinite, we can't properly resolve % heights
        // This is a limitation - shape-inside with % heights requires finite containing block
        styled_dom
            .css_property_cache
            .ptr
            .get_height(node_data, &id, node_state)
            .and_then(|v| v.get_property())
            .and_then(|h| match h {
                LayoutHeight::Px(v) => {
                    // Only accept absolute units (px, pt, in, cm, mm) - no %, em, rem
                    // since we can't resolve relative units without proper context
                    match v.metric {
                        SizeMetric::Px => Some(v.number.get()),
                        SizeMetric::Pt => Some(v.number.get() * PT_TO_PX),
                        SizeMetric::In => Some(v.number.get() * 96.0),
                        SizeMetric::Cm => Some(v.number.get() * 96.0 / 2.54),
                        SizeMetric::Mm => Some(v.number.get() * 96.0 / 25.4),
                        _ => None, // Ignore %, em, rem
                    }
                }
                _ => None,
            })
            .unwrap_or(constraints.available_size.width) // Fallback: use width as height (square)
    };

    let reference_box = crate::text3::cache::Rect {
        x: 0.0,
        y: 0.0,
        width: constraints.available_size.width,
        height: ref_box_height,
    };

    // shape-inside: Text flows within the shape boundary
    debug_info!(ctx, "Checking shape-inside for node {:?}", id);
    debug_info!(
        ctx,
        "Reference box: {:?} (available_size height was: {})",
        reference_box,
        constraints.available_size.height
    );

    let shape_boundaries = styled_dom
        .css_property_cache
        .ptr
        .get_shape_inside(node_data, &id, node_state)
        .and_then(|v| {
            debug_info!(ctx, "Got shape-inside value: {:?}", v);
            v.get_property()
        })
        .and_then(|shape_inside| {
            debug_info!(ctx, "shape-inside property: {:?}", shape_inside);
            if let ShapeInside::Shape(css_shape) = shape_inside {
                debug_info!(
                    ctx,
                    "Converting CSS shape to ShapeBoundary: {:?}",
                    css_shape
                );
                let boundary =
                    ShapeBoundary::from_css_shape(css_shape, reference_box, ctx.debug_messages);
                debug_info!(ctx, "Created ShapeBoundary: {:?}", boundary);
                Some(vec![boundary])
            } else {
                debug_info!(ctx, "shape-inside is None");
                None
            }
        })
        .unwrap_or_default();

    debug_info!(
        ctx,
        "Final shape_boundaries count: {}",
        shape_boundaries.len()
    );

    // shape-outside: Text wraps around the shape (adds to exclusions)
    debug_info!(ctx, "Checking shape-outside for node {:?}", id);
    if let Some(shape_outside_value) = styled_dom
        .css_property_cache
        .ptr
        .get_shape_outside(node_data, &id, node_state)
    {
        debug_info!(ctx, "Got shape-outside value: {:?}", shape_outside_value);
        if let Some(shape_outside) = shape_outside_value.get_property() {
            debug_info!(ctx, "shape-outside property: {:?}", shape_outside);
            if let ShapeOutside::Shape(css_shape) = shape_outside {
                debug_info!(
                    ctx,
                    "Converting CSS shape-outside to ShapeBoundary: {:?}",
                    css_shape
                );
                let boundary =
                    ShapeBoundary::from_css_shape(css_shape, reference_box, ctx.debug_messages);
                debug_info!(ctx, "Created ShapeBoundary (exclusion): {:?}", boundary);
                shape_exclusions.push(boundary);
            }
        }
    } else {
        debug_info!(ctx, "No shape-outside value found");
    }

    // TODO: clip-path will be used for rendering clipping (not text layout)

    let writing_mode = get_writing_mode(styled_dom, id, node_state).unwrap_or_default();

    let text_align = get_text_align(styled_dom, id, node_state).unwrap_or_default();

    let text_justify = styled_dom
        .css_property_cache
        .ptr
        .get_text_justify(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    // Get font-size for resolving line-height
    // Use helper function which checks dependency chain first
    let font_size = get_element_font_size(styled_dom, id, node_state);

    let line_height_value = styled_dom
        .css_property_cache
        .ptr
        .get_line_height(node_data, &id, node_state)
        .and_then(|s| s.get_property().cloned())
        .unwrap_or_default();

    let hyphenation = styled_dom
        .css_property_cache
        .ptr
        .get_hyphens(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let word_break_css = styled_dom
        .css_property_cache
        .ptr
        .get_word_break(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let overflow_wrap_css = styled_dom
        .css_property_cache
        .ptr
        .get_overflow_wrap(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let line_break_css = styled_dom
        .css_property_cache
        .ptr
        .get_line_break(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let text_align_last_css = styled_dom
        .css_property_cache
        .ptr
        .get_text_align_last(node_data, &id, node_state)
        .and_then(|s| s.get_property().copied())
        .unwrap_or_default();

    let overflow_behaviour = get_overflow_x(styled_dom, id, node_state).unwrap_or_default();

    // +spec:display-property:21f728 - vertical-align shorthand resolves inline-level box alignment
    // +spec:display-property:98fa8e - alignment-baseline values for inline-level boxes in IFC (implemented via vertical-align shorthand)
    // +spec:display-property:1f71ad - baseline-shift + alignment-baseline longhands mapped through vertical-align
    // +spec:display-property:89dd7b - line-relative shift values (top/center/bottom) and aligned subtree alignment
    // +spec:inline-formatting-context:21da06 - vertical-align uses line-over/line-under sides via writing_mode logical mapping
    // +spec:inline-formatting-context:295603 - baseline alignment: vertical-align determines how inline boxes align (baseline, super, sub, etc.)
    // +spec:inline-formatting-context:7351bf - default alignment baseline is alphabetic in horizontal typographic mode
    // +spec:inline-formatting-context:85de3d - vertical-align shorthand: alignment within line box
    // +spec:inline-formatting-context:aa8af0 - alignment baseline chosen by vertical-align, defaults to parent's dominant baseline
    // +spec:inline-formatting-context:e475d2 - baseline and vertical-align control transverse alignment of inline content on line boxes
    // +spec:overflow:d44eac - vertical-align inline box alignment (CSS 2.2 model covers baseline/top/middle/bottom/sub/super/text-top/text-bottom)
    // +spec:writing-modes:313575 - alignment-baseline: inline-level boxes align baselines within parent inline box's alignment context along inline axis
    // +spec:writing-modes:60ad67 - inline layout aligns boxes in block axis via baselines
    // +spec:writing-modes:0127e5 - line-relative directions: line-over/under map to vertical-align top/bottom
    // Get vertical-align from CSS property cache (defaults to Baseline per CSS spec)
    let vertical_align = match get_vertical_align_property(styled_dom, id, node_state) {
        MultiValue::Exact(v) => v,
        _ => StyleVerticalAlign::default(),
    };

    // +spec:display-property:c03a6b - baseline-shift (sub/super/length/percentage) and line-relative (top/center/bottom) shifts handled via vertical-align
    let vertical_align = match vertical_align {
        StyleVerticalAlign::Baseline => text3::cache::VerticalAlign::Baseline,
        StyleVerticalAlign::Top => text3::cache::VerticalAlign::Top,
        StyleVerticalAlign::Middle => text3::cache::VerticalAlign::Middle,
        StyleVerticalAlign::Bottom => text3::cache::VerticalAlign::Bottom,
        StyleVerticalAlign::Sub => text3::cache::VerticalAlign::Sub,
        // +spec:inline-formatting-context:fe563c - vertical-align: super shifts inline to superscript position
        // +spec:inline-formatting-context:fe563c - vertical-align:super shifts child to superscript position
        StyleVerticalAlign::Superscript => text3::cache::VerticalAlign::Super,
        StyleVerticalAlign::TextTop => text3::cache::VerticalAlign::TextTop,
        StyleVerticalAlign::TextBottom => text3::cache::VerticalAlign::TextBottom,
        // §10.8.1: <percentage> refers to line-height of the element itself
        StyleVerticalAlign::Percentage(p) => {
            let offset = p.normalized() * line_height_value.inner.normalized() * font_size;
            text3::cache::VerticalAlign::Offset(offset)
        }
        // §10.8.1: <length> is absolute offset from baseline
        StyleVerticalAlign::Length(l) => {
            let offset = super::calc::resolve_pixel_value(&l, 0.0, font_size, font_size);
            text3::cache::VerticalAlign::Offset(offset)
        }
    };
    // +spec:block-formatting-context:987746 - text-orientation property (mixed/upright/sideways) for vertical writing modes
    // +spec:inline-formatting-context:cbe738 - text-orientation (mixed/upright/sideways) bi-orientational transform for vertical text
    // +spec:writing-modes:09a1bb - vertical typesetting orientation (upright/sideways) for vertical-rl/vertical-lr
    // +spec:writing-modes:2eb1b2 - text-orientation (mixed/upright/sideways) applied to vertical text layout
    let text_orientation = match get_text_orientation_property(styled_dom, id, node_state) {
        MultiValue::Exact(o) => match o {
            StyleTextOrientation::Mixed => text3::cache::TextOrientation::Mixed,
            StyleTextOrientation::Upright => text3::cache::TextOrientation::Upright,
            // +spec:block-formatting-context:a606e6 - sideways text typeset rotated 90° CW in vertical modes
            StyleTextOrientation::Sideways => text3::cache::TextOrientation::Sideways,
        },
        _ => text3::cache::TextOrientation::default(),
    };

    // +spec:display-property:8364c0 - direction property (ltr/rtl) sets paragraph embedding level for bidi algorithm
    // +spec:text-alignment-spacing:97b93a - direction property affects text-align:justify last-line alignment
    // +spec:writing-modes:73aaff - block elements inherit base direction from parent via CSS direction property
    // +spec:writing-modes:8a888b - line box inline base direction from containing block's direction
    // Get the direction property from the CSS cache (defaults to LTR if not set)
    let direction = match get_direction_property(styled_dom, id, node_state) {
        MultiValue::Exact(d) => Some(match d {
            StyleDirection::Ltr => text3::cache::BidiDirection::Ltr,
            StyleDirection::Rtl => text3::cache::BidiDirection::Rtl,
        }),
        _ => None,
    };

    debug_info!(
        ctx,
        "dom_id={:?}, available_size={}x{}, setting available_width={}",
        dom_id,
        constraints.available_size.width,
        constraints.available_size.height,
        constraints.available_size.width
    );

    // +spec:box-model:8113d7 - text-indent treated as margin on start edge of line box
    // +spec:display-contents:5f95ac - text-indent: percentage=0 for intrinsic sizing, each-line and hanging keywords
    // +spec:floats:17c74a - text-indent applied to first line (5em indentation with no floats)
    // +spec:positioning:1e32b1 - text-indent with hanging/each-line keywords resolved and passed to text layout
    let text_indent_prop = styled_dom
        .css_property_cache
        .ptr
        .get_text_indent(node_data, &id, node_state)
        .and_then(|s| s.get_property().cloned());
    let is_intrinsic_sizing = matches!(
        constraints.available_width_type,
        Text3AvailableSpace::MinContent | Text3AvailableSpace::MaxContent
    );
    // +spec:intrinsic-sizing:0e8625 - percentage text-indent treated as 0 for intrinsic size contributions
    let text_indent = text_indent_prop
        .map(|ti| {
            // CSS Text 3 §8.1: "Percentages must be treated as 0 for the purpose
            // of calculating intrinsic size contributions"
            if is_intrinsic_sizing && ti.inner.to_percent().is_some() {
                return 0.0;
            }
            let context = ResolutionContext {
                element_font_size: get_element_font_size(styled_dom, id, node_state),
                parent_font_size: get_parent_font_size(styled_dom, id, node_state),
                root_font_size: get_root_font_size(styled_dom, node_state),
                containing_block_size: PhysicalSize::new(constraints.available_size.width, 0.0),
                element_size: None,
                viewport_size: PhysicalSize::new(0.0, 0.0),
            };
            ti.inner
                .resolve_with_context(&context, PropertyContext::Other)
        })
        .unwrap_or(0.0);
    let text_indent_each_line = text_indent_prop.map(|ti| ti.each_line).unwrap_or(false);
    let text_indent_hanging = text_indent_prop.map(|ti| ti.hanging).unwrap_or(false);

    // Get column-count for multi-column layout (default: 1 = no columns)
    let columns = styled_dom
        .css_property_cache
        .ptr
        .get_column_count(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|cc| match cc {
            ColumnCount::Integer(n) => *n,
            ColumnCount::Auto => 1,
        })
        .unwrap_or(1);

    // Get column-gap for multi-column layout (default: normal = 1em)
    let column_gap = styled_dom
        .css_property_cache
        .ptr
        .get_column_gap(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|cg| {
            let context = ResolutionContext {
                element_font_size: get_element_font_size(styled_dom, id, node_state),
                parent_font_size: get_parent_font_size(styled_dom, id, node_state),
                root_font_size: get_root_font_size(styled_dom, node_state),
                containing_block_size: PhysicalSize::new(0.0, 0.0),
                element_size: None,
                viewport_size: PhysicalSize::new(0.0, 0.0),
            };
            cg.inner
                .resolve_with_context(&context, PropertyContext::Other)
        })
        .unwrap_or_else(|| {
            // Default: 1em
            get_element_font_size(styled_dom, id, node_state)
        });

    // +spec:line-breaking:b4928e - white-space values mapped to wrap/whitespace processing rules
    // Map white-space CSS property to TextWrap
    let resolved_ws = match get_white_space_property(styled_dom, id, node_state) {
        MultiValue::Exact(ws) => ws,
        _ => StyleWhiteSpace::Normal,
    };
    let text_wrap = match resolved_ws {
        StyleWhiteSpace::Normal => text3::cache::TextWrap::Wrap,
        StyleWhiteSpace::Nowrap => text3::cache::TextWrap::NoWrap,
        StyleWhiteSpace::Pre => text3::cache::TextWrap::NoWrap,
        StyleWhiteSpace::PreWrap => text3::cache::TextWrap::Wrap,
        StyleWhiteSpace::PreLine => text3::cache::TextWrap::Wrap,
        StyleWhiteSpace::BreakSpaces => text3::cache::TextWrap::Wrap,
    };
    let white_space_mode = match resolved_ws {
        StyleWhiteSpace::Normal => text3::cache::WhiteSpaceMode::Normal,
        StyleWhiteSpace::Nowrap => text3::cache::WhiteSpaceMode::Nowrap,
        StyleWhiteSpace::Pre => text3::cache::WhiteSpaceMode::Pre,
        StyleWhiteSpace::PreWrap => text3::cache::WhiteSpaceMode::PreWrap,
        StyleWhiteSpace::PreLine => text3::cache::WhiteSpaceMode::PreLine,
        StyleWhiteSpace::BreakSpaces => text3::cache::WhiteSpaceMode::BreakSpaces,
    };

    // +spec:block-formatting-context:fd60a8 - initial letter box is in-flow in its BFC, originating line box
    // +spec:block-formatting-context:c5ba02 - initial letter inline flow layout (alignment, white space collapsing)
    // +spec:block-formatting-context:83f8a7 - initial letter wrapping modes (none, all, first)
    // +spec:block-formatting-context:fef28d - initial letter box is in-flow in its BFC, part of originating line box
    // +spec:box-model:c3ce58 - initial letter block-start margin edge must be below containing block content edge
    // +spec:display-contents:568fe2 - initial letter participates in same IFC as its line
    // +spec:display-property:a89adb - initial letter boxes from non-replaced inline boxes and atomic inlines
    // +spec:display-property:4b59ce - initial-letter applies to inline-level boxes at start of first line
    // +spec:display-property:756cad - initial-letter sizing: drop/raise/sunken initial computation
    // +spec:display-property:8b08f4 - initial-letter applied to first inline-level child of block container
    // +spec:display-property:8c1dce - initial-letter property: size/sink for drop caps on inline-level boxes
    // +spec:display-property:b453a3 - initial-letter applies to inline-level boxes in IFC
    // +spec:display-property:b5e149 - initial letters are in-flow inline-level content, not floats
    // +spec:display-property:fa044e - initial-letter applies to first-child inline-level boxes
    // +spec:line-height:306d87 - initial-letter sizing must use containing block's line-height, not spanned lines' heights
    // +spec:writing-modes:903310 - atomic initial letters use normal sizing; only positioning is special
    // Get initial-letter for drop caps
    // +spec:display-property:5af252 - initial-letter on inline-level box not at line start uses normal
    // +spec:text-alignment-spacing:a17609 - sunken initial letters suppress letter-spacing and justification (not word-spacing) with adjacent content
    let initial_letter = styled_dom
        .css_property_cache
        .ptr
        .get_initial_letter(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|il| {
            use std::num::NonZeroUsize;
            let sink = match il.sink {
                azul_css::corety::OptionU32::Some(s) => s,
                azul_css::corety::OptionU32::None => il.size,
            };
            text3::cache::InitialLetter {
                size: il.size as f32,
                sink,
                count: NonZeroUsize::new(1).unwrap(),
            }
        });

    // If initial-letter is set, compute the drop cap exclusion area and add it
    // to the shape exclusions so that text wraps around the enlarged letter.
    // +spec:box-model:d4adf6 - ancestor inline boundaries excluded via geometric exclusion
    // +spec:floats:c5e23f - floats in subsequent lines adjacent to a sunk initial letter must clear it
    if let Some(ref il) = initial_letter {
        let computed_line_height = line_height_value.inner.normalized() * font_size;
        let (letter_w, letter_h) = layout_initial_letter(
            il.size,
            il.sink,
            constraints.available_size.width,
            computed_line_height,
        );
        if letter_w > 0.0 && letter_h > 0.0 {
            // Place the exclusion at the inline-start (x=0, y=0 relative to the IFC).
            // This creates a rectangular exclusion that text flows around.
            shape_exclusions.push(ShapeBoundary::Rectangle(crate::text3::cache::Rect {
                x: 0.0,
                y: 0.0,
                width: letter_w,
                height: letter_h,
            }));
        }
    }

    // Get line-clamp for limiting visible lines
    let line_clamp = styled_dom
        .css_property_cache
        .ptr
        .get_line_clamp(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .and_then(|lc| std::num::NonZeroUsize::new(lc.max_lines));

    // Get hanging-punctuation for hanging punctuation marks
    let hanging_punctuation = styled_dom
        .css_property_cache
        .ptr
        .get_hanging_punctuation(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|hp| hp.enabled)
        .unwrap_or(false);

    // Get text-combine-upright for vertical text combination
    // +spec:line-breaking:9f150a - text-combine-upright:all composes glyphs horizontally, ignoring letter-spacing and forced line breaks
    // +spec:line-breaking:1b88cd - text-combine-upright:all layout: inline-block with 1em square, ignoring forced line breaks
    let text_combine_upright = styled_dom
        .css_property_cache
        .ptr
        .get_text_combine_upright(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        // +spec:display-property:6f174d - text-combine-upright horizontal-in-vertical composition
        .map(|tcu| match tcu {
            StyleTextCombineUpright::None => text3::cache::TextCombineUpright::None,
            StyleTextCombineUpright::All => text3::cache::TextCombineUpright::All,
            StyleTextCombineUpright::Digits(n) => text3::cache::TextCombineUpright::Digits(*n),
        });

    // Get exclusion-margin for shape exclusions
    let exclusion_margin = styled_dom
        .css_property_cache
        .ptr
        .get_exclusion_margin(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .map(|em| em.inner.get() as f32)
        .unwrap_or(0.0);

    // Get hyphenation-language for language-specific hyphenation
    let hyphenation_language = styled_dom
        .css_property_cache
        .ptr
        .get_hyphenation_language(node_data, &id, node_state)
        .and_then(|s| s.get_property())
        .and_then(|hl| {
            #[cfg(feature = "text_layout_hyphenation")]
            {
                use hyphenation::{Language, Load};
                // Parse BCP 47 language code to hyphenation::Language
                match hl.inner.as_str() {
                    "en-US" | "en" => Some(Language::EnglishUS),
                    "de-DE" | "de" => Some(Language::German1996),
                    "fr-FR" | "fr" => Some(Language::French),
                    "es-ES" | "es" => Some(Language::Spanish),
                    "it-IT" | "it" => Some(Language::Italian),
                    "pt-PT" | "pt" => Some(Language::Portuguese),
                    "nl-NL" | "nl" => Some(Language::Dutch),
                    "pl-PL" | "pl" => Some(Language::Polish),
                    "ru-RU" | "ru" => Some(Language::Russian),
                    "zh-CN" | "zh" => Some(Language::Chinese),
                    _ => None, // Unsupported language
                }
            }
            #[cfg(not(feature = "text_layout_hyphenation"))]
            {
                None::<crate::text3::script::Language>
            }
        });

    UnifiedConstraints {
        exclusion_margin,
        hyphenation_language,
        text_indent,
        text_indent_each_line,
        text_indent_hanging,
        initial_letter,
        line_clamp,
        columns,
        column_gap,
        hanging_punctuation,
        text_wrap,
        white_space_mode,
        text_combine_upright,
        segment_alignment: SegmentAlignment::Total,
        overflow: match overflow_behaviour {
            LayoutOverflow::Visible => text3::cache::OverflowBehavior::Visible,
            LayoutOverflow::Hidden | LayoutOverflow::Clip => text3::cache::OverflowBehavior::Hidden,
            LayoutOverflow::Scroll => text3::cache::OverflowBehavior::Scroll,
            LayoutOverflow::Auto => text3::cache::OverflowBehavior::Auto,
        },
        // Use the semantic available_width_type directly instead of converting from float.
        // This preserves MinContent/MaxContent semantics for intrinsic sizing.
        available_width: constraints.available_width_type,
        // For scrollable containers (overflow: scroll/auto), don't constrain height
        // so that the full content is laid out and content_size is calculated correctly.
        available_height: match overflow_behaviour {
            LayoutOverflow::Scroll | LayoutOverflow::Auto => None,
            _ => Some(constraints.available_size.height),
        },
        shape_boundaries, // CSS shape-inside: text flows within shape
        shape_exclusions, // CSS shape-outside + floats: text wraps around shapes
        writing_mode: Some(match writing_mode {
            LayoutWritingMode::HorizontalTb => text3::cache::WritingMode::HorizontalTb,
            LayoutWritingMode::VerticalRl => text3::cache::WritingMode::VerticalRl,
            LayoutWritingMode::VerticalLr => text3::cache::WritingMode::VerticalLr,
        }),
        direction, // Use the CSS direction property (currently defaulting to LTR)
        // +spec:overflow:7ff7d1 - hyphens property: none/manual/auto hyphenation control
        hyphenation: match hyphenation {
            StyleHyphens::None => text3::cache::Hyphens::None,
            StyleHyphens::Manual => text3::cache::Hyphens::Manual,
            StyleHyphens::Auto => text3::cache::Hyphens::Auto,
        },
        text_orientation,
        // +spec:text-alignment-spacing:838967 - map text-align values (start/end/left/right/center/justify) to inline alignment
        // +spec:text-alignment-spacing:d9ea45 - property index: text-align, text-justify, letter-spacing mapped to layout
        // +spec:text-alignment-spacing:600fda - text-align values (left/right/center/justify) mapped per CSS Text §6.1
        text_align: match text_align {
            StyleTextAlign::Start => text3::cache::TextAlign::Start,
            StyleTextAlign::End => text3::cache::TextAlign::End,
            StyleTextAlign::Left => text3::cache::TextAlign::Left,
            StyleTextAlign::Right => text3::cache::TextAlign::Right,
            StyleTextAlign::Center => text3::cache::TextAlign::Center,
            StyleTextAlign::Justify => text3::cache::TextAlign::Justify,
        },
        // +spec:text-alignment-spacing:0ea31d - text-justify inter-word/inter-character/distribute mapped per §6.4
        text_justify: match text_justify {
            LayoutTextJustify::None => text3::cache::JustifyContent::None,
            LayoutTextJustify::Auto => text3::cache::JustifyContent::None,
            LayoutTextJustify::InterWord => text3::cache::JustifyContent::InterWord,
            LayoutTextJustify::InterCharacter => text3::cache::JustifyContent::InterCharacter,
            LayoutTextJustify::Distribute => text3::cache::JustifyContent::Distribute,
        },
        // +spec:line-height:79f3aa - line-height resolved: normal defaults to 1.2, <number>/<percentage> × font-size
        line_height: line_height_value.inner.normalized() * font_size,
        // container's first available font. Approximated as 80%/20% of font_size (typical
        // for Latin fonts). TODO: resolve actual font and use its OS/2 metrics.
        strut_ascent: font_size * 0.8,
        strut_descent: font_size * 0.2,
        // ch unit width: try to get actual space width from font, fall back to 0.5 * font_size
        ch_width: font_size * 0.5, // TODO: resolve from ParsedFontTrait::get_space_width()
        vertical_align,
        // +spec:inline-formatting-context:48ce44 - overflow-wrap property: break at otherwise disallowed points to prevent overflow
        overflow_wrap: match overflow_wrap_css {
            StyleOverflowWrap::Normal => text3::cache::OverflowWrap::Normal,
            StyleOverflowWrap::Anywhere | StyleOverflowWrap::BreakWord => text3::cache::OverflowWrap::Anywhere,
        },
        text_align_last: match text_align_last_css {
            StyleTextAlignLast::Auto => text3::cache::TextAlign::default(),
            StyleTextAlignLast::Start => text3::cache::TextAlign::Start,
            StyleTextAlignLast::End => text3::cache::TextAlign::End,
            StyleTextAlignLast::Left => text3::cache::TextAlign::Left,
            StyleTextAlignLast::Right => text3::cache::TextAlign::Right,
            StyleTextAlignLast::Center => text3::cache::TextAlign::Center,
            StyleTextAlignLast::Justify => text3::cache::TextAlign::Justify,
        },
        word_break: match word_break_css {
            StyleWordBreak::Normal => text3::cache::WordBreak::Normal,
            StyleWordBreak::BreakAll => text3::cache::WordBreak::BreakAll,
            StyleWordBreak::KeepAll => text3::cache::WordBreak::KeepAll,
        },
        // +spec:white-space-processing:bc5f7b - line-break with break-spaces allows breaking before first space
        // CSS Text Level 3 §5.3: The line-break property affects preserved white space behavior:
        // - normal/pre-line: preserved white space at end/start of line is discarded
        // - nowrap/pre: wrapping is forbidden altogether
        // - pre-wrap: preserved white space hangs
        // - break-spaces: allows breaking before first space of a sequence
        // break-spaces allows wrapping preserved spaces to next line; for other white-space values,
        // preserved spaces at line ends are either discarded (normal, pre-line), wrapping is
        // forbidden (nowrap, pre), or they hang (pre-wrap).
        line_break: match line_break_css {
            StyleLineBreak::Auto => text3::cache::LineBreakStrictness::Auto,
            StyleLineBreak::Loose => text3::cache::LineBreakStrictness::Loose,
            StyleLineBreak::Normal => text3::cache::LineBreakStrictness::Normal,
            StyleLineBreak::Strict => text3::cache::LineBreakStrictness::Strict,
            StyleLineBreak::Anywhere => text3::cache::LineBreakStrictness::Anywhere,
        },
    }
}

// Table Formatting Context (CSS 2.2 § 17)
// +spec:display-property:d887c0 - Table wrapper box BFC, caption-side, table grid layout (§17.4-17.5)

// +spec:inline-formatting-context:9c272d - CSS table model: row-primary structure, display-to-table-element mapping, visual formatting as rectangular grid
/// Lays out a Table Formatting Context.
/// Table column information for layout calculations
#[derive(Debug, Clone)]
pub struct TableColumnInfo {
    /// Minimum width required for this column
    pub min_width: f32,
    /// Maximum width desired for this column
    pub max_width: f32,
    /// Computed final width for this column
    pub computed_width: Option<f32>,
}

/// Information about a table cell for layout
#[derive(Debug, Clone)]
pub struct TableCellInfo {
    /// Node index in the layout tree
    pub node_index: usize,
    /// Column index (0-based)
    pub column: usize,
    /// Number of columns this cell spans
    pub colspan: usize,
    /// Row index (0-based)
    pub row: usize,
    /// Number of rows this cell spans
    pub rowspan: usize,
}

/// Table layout context - holds all information needed for table layout
#[derive(Debug)]
struct TableLayoutContext {
    /// Information about each column
    columns: Vec<TableColumnInfo>,
    /// Information about each cell
    cells: Vec<TableCellInfo>,
    /// Number of rows in the table
    num_rows: usize,
    /// Whether to use fixed or auto layout algorithm
    use_fixed_layout: bool,
    /// Computed height for each row
    row_heights: Vec<f32>,
    /// Computed baseline offset for each row (distance from row top to row baseline)
    row_baselines: Vec<f32>,
    // +spec:inline-formatting-context:440ca9 - border-collapse/border-spacing/visibility:collapse table properties (CSS 2.2 §17.5-17.6)
    /// Border collapse mode
    border_collapse: StyleBorderCollapse,
    /// Border spacing (only used when border_collapse is Separate)
    border_spacing: LayoutBorderSpacing,
    /// CSS 2.2 Section 17.4: Index of table-caption child, if any
    caption_index: Option<usize>,
    //   from display without forcing table re-layout
    /// CSS 2.2 Section 17.6: Rows with visibility:collapse (dynamic effects)
    /// Set of row indices that have visibility:collapse
    collapsed_rows: std::collections::HashSet<usize>,
    /// CSS 2.2 Section 17.6: Columns with visibility:collapse (dynamic effects)
    /// Set of column indices that have visibility:collapse
    collapsed_columns: std::collections::HashSet<usize>,
    /// Rows that are hidden-empty (zero height, border-spacing on only one side)
    hidden_empty_rows: std::collections::HashSet<usize>,
}

impl TableLayoutContext {
    fn new() -> Self {
        Self {
            columns: Vec::new(),
            cells: Vec::new(),
            num_rows: 0,
            use_fixed_layout: false,
            row_heights: Vec::new(),
            row_baselines: Vec::new(),
            border_collapse: StyleBorderCollapse::Separate,
            border_spacing: LayoutBorderSpacing::default(),
            caption_index: None,
            collapsed_rows: std::collections::HashSet::new(),
            collapsed_columns: std::collections::HashSet::new(),
            hidden_empty_rows: std::collections::HashSet::new(),
        }
    }
}

// +spec:table-layout:485791 - Six superimposed table layers: table, column-group, column, row-group, row, cell (bottom to top)
// +spec:table-layout:dcdf1b - Collapsing border model: border conflict resolution uses layer priority (cell > row > row-group > column > column-group > table)
/// Source of a border in the border conflict resolution algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BorderSource {
    Table = 0,
    ColumnGroup = 1,
    Column = 2,
    RowGroup = 3,
    Row = 4,
    Cell = 5,
}

/// Information about a border for conflict resolution
#[derive(Debug, Clone)]
pub struct BorderInfo {
    pub width: f32,
    pub style: BorderStyle,
    pub color: ColorU,
    pub source: BorderSource,
}

impl BorderInfo {
    pub fn new(width: f32, style: BorderStyle, color: ColorU, source: BorderSource) -> Self {
        Self {
            width,
            style,
            color,
            source,
        }
    }

    // +spec:block-formatting-context:f772ae - border style priority for table border conflict resolution
    /// Get the priority of a border style for conflict resolution
    /// Higher number = higher priority
    pub fn style_priority(style: &BorderStyle) -> u8 {
        match style {
            BorderStyle::Hidden => 255, // Highest - suppresses all borders
            BorderStyle::None => 0,     // Lowest - loses to everything
            BorderStyle::Double => 8,
            BorderStyle::Solid => 7,
            BorderStyle::Dashed => 6,
            BorderStyle::Dotted => 5,
            BorderStyle::Ridge => 4,
            BorderStyle::Outset => 3,
            BorderStyle::Groove => 2,
            BorderStyle::Inset => 1,
        }
    }

    // +spec:box-model:2255c2 - Collapsing border conflict resolution (hidden wins, then none loses, then wider wins, then style priority)
    // +spec:box-model:b42c79 - border conflict resolution: hidden wins, then wider, then style priority, then source
    // +spec:box-model:503e9e - border conflict resolution: hidden wins, then wider, then style priority, then source priority
    // +spec:box-model:7eb217 - Border conflict resolution: hidden > none < wider > style priority > source priority > left/top
    // +spec:overflow:1fb482 - Border conflict resolution per CSS 2.2 §17.6.2.1 (hidden wins, then wider, then style priority, then source priority)
    // +spec:table-layout:882560 - Border conflict resolution (17.6.2.1): hidden wins, none loses, wider wins, style priority, source priority
    /// Compare two borders for conflict resolution per CSS 2.2 Section 17.6.2.1
    /// Returns the winning border
    // +spec:table-layout:21053b - border conflict resolution: hidden suppresses all, style priorities
    // +spec:table-layout:076617 - border conflict resolution algorithm and border style semantics in collapsing model
    pub fn resolve_conflict(a: &BorderInfo, b: &BorderInfo) -> Option<BorderInfo> {
        // 1. 'hidden' wins and suppresses all borders
        if a.style == BorderStyle::Hidden || b.style == BorderStyle::Hidden {
            return None;
        }

        // 2. Filter out 'none' - if both are none, no border
        let a_is_none = a.style == BorderStyle::None;
        let b_is_none = b.style == BorderStyle::None;

        if a_is_none && b_is_none {
            return None;
        }
        if a_is_none {
            return Some(b.clone());
        }
        if b_is_none {
            return Some(a.clone());
        }

        // 3. Wider border wins
        if a.width > b.width {
            return Some(a.clone());
        }
        if b.width > a.width {
            return Some(b.clone());
        }

        // 4. If same width, compare style priority
        let a_priority = Self::style_priority(&a.style);
        let b_priority = Self::style_priority(&b.style);

        if a_priority > b_priority {
            return Some(a.clone());
        }
        if b_priority > a_priority {
            return Some(b.clone());
        }

        // 5. If same style, source priority:
        // Cell > Row > RowGroup > Column > ColumnGroup > Table
        if a.source > b.source {
            return Some(a.clone());
        }
        if b.source > a.source {
            return Some(b.clone());
        }

        // 6. Same priority - prefer first one (left/top in LTR)
        Some(a.clone())
    }
}

/// Get border information for a node
fn get_border_info<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
    source: BorderSource,
) -> (BorderInfo, BorderInfo, BorderInfo, BorderInfo) {
    use azul_css::props::{
        basic::{
            pixel::{PhysicalSize, PropertyContext, ResolutionContext},
            ColorU,
        },
        style::BorderStyle,
    };
    use get_element_font_size;
    use get_parent_font_size;
    use get_root_font_size;

    let default_border = BorderInfo::new(
        0.0,
        BorderStyle::None,
        ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        },
        source,
    );

    let Some(dom_id) = node.dom_node_id else {
        return (
            default_border.clone(),
            default_border.clone(),
            default_border.clone(),
            default_border.clone(),
        );
    };

    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state.clone();
    let cache = &ctx.styled_dom.css_property_cache.ptr;

    // FAST PATH: compact cache for normal state
    if let Some(ref cc) = cache.compact_cache {
        let idx = dom_id.index();

        // Border styles from packed u16
        let bts = cc.get_border_top_style(idx);
        let brs = cc.get_border_right_style(idx);
        let bbs = cc.get_border_bottom_style(idx);
        let bls = cc.get_border_left_style(idx);

        // Border colors from u32 RGBA
        let make_color = |raw: u32| -> ColorU {
            if raw == 0 {
                ColorU { r: 0, g: 0, b: 0, a: 0 }
            } else {
                ColorU {
                    r: ((raw >> 24) & 0xFF) as u8,
                    g: ((raw >> 16) & 0xFF) as u8,
                    b: ((raw >> 8) & 0xFF) as u8,
                    a: (raw & 0xFF) as u8,
                }
            }
        };

        let btc = make_color(cc.get_border_top_color_raw(idx));
        let brc = make_color(cc.get_border_right_color_raw(idx));
        let bbc = make_color(cc.get_border_bottom_color_raw(idx));
        let blc = make_color(cc.get_border_left_color_raw(idx));

        // Border widths from i16 × 10
        let decode_width = |raw: i16| -> f32 {
            if raw >= azul_css::compact_cache::I16_SENTINEL_THRESHOLD {
                0.0 // sentinel → fall back to 0
            } else {
                raw as f32 / 10.0
            }
        };

        let btw = decode_width(cc.get_border_top_width_raw(idx));
        let brw = decode_width(cc.get_border_right_width_raw(idx));
        let bbw = decode_width(cc.get_border_bottom_width_raw(idx));
        let blw = decode_width(cc.get_border_left_width_raw(idx));

        let top = if bts == BorderStyle::None { default_border.clone() }
            else { BorderInfo::new(btw, bts, btc, source) };
        let right = if brs == BorderStyle::None { default_border.clone() }
            else { BorderInfo::new(brw, brs, brc, source) };
        let bottom = if bbs == BorderStyle::None { default_border.clone() }
            else { BorderInfo::new(bbw, bbs, bbc, source) };
        let left = if bls == BorderStyle::None { default_border.clone() }
            else { BorderInfo::new(blw, bls, blc, source) };

        return (top, right, bottom, left);
    }

    // SLOW PATH: full cascade resolution
    let cache = &ctx.styled_dom.css_property_cache.ptr;

    // Create resolution context for border-width (em/rem support, no % support)
    let element_font_size = get_element_font_size(ctx.styled_dom, dom_id, &node_state);
    let parent_font_size = get_parent_font_size(ctx.styled_dom, dom_id, &node_state);
    let root_font_size = get_root_font_size(ctx.styled_dom, &node_state);

    let resolution_context = ResolutionContext {
        element_font_size,
        parent_font_size,
        root_font_size,
        // Not used for border-width
        containing_block_size: PhysicalSize::new(0.0, 0.0),
        // Not used for border-width
        element_size: None,
        viewport_size: PhysicalSize::new(0.0, 0.0),
    };

    // Top border
    let top = cache
        .get_border_top_style(node_data, &dom_id, &node_state)
        .and_then(|s| s.get_property())
        .map(|style_val| {
            let width = cache
                .get_border_top_width(node_data, &dom_id, &node_state)
                .and_then(|w| w.get_property())
                .map(|w| {
                    w.inner
                        .resolve_with_context(&resolution_context, PropertyContext::BorderWidth)
                })
                .unwrap_or(0.0);
            let color = cache
                .get_border_top_color(node_data, &dom_id, &node_state)
                .and_then(|c| c.get_property())
                .map(|c| c.inner)
                .unwrap_or(ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                });
            BorderInfo::new(width, style_val.inner, color, source)
        })
        .unwrap_or_else(|| default_border.clone());

    // Right border
    let right = cache
        .get_border_right_style(node_data, &dom_id, &node_state)
        .and_then(|s| s.get_property())
        .map(|style_val| {
            let width = cache
                .get_border_right_width(node_data, &dom_id, &node_state)
                .and_then(|w| w.get_property())
                .map(|w| {
                    w.inner
                        .resolve_with_context(&resolution_context, PropertyContext::BorderWidth)
                })
                .unwrap_or(0.0);
            let color = cache
                .get_border_right_color(node_data, &dom_id, &node_state)
                .and_then(|c| c.get_property())
                .map(|c| c.inner)
                .unwrap_or(ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                });
            BorderInfo::new(width, style_val.inner, color, source)
        })
        .unwrap_or_else(|| default_border.clone());

    // Bottom border
    let bottom = cache
        .get_border_bottom_style(node_data, &dom_id, &node_state)
        .and_then(|s| s.get_property())
        .map(|style_val| {
            let width = cache
                .get_border_bottom_width(node_data, &dom_id, &node_state)
                .and_then(|w| w.get_property())
                .map(|w| {
                    w.inner
                        .resolve_with_context(&resolution_context, PropertyContext::BorderWidth)
                })
                .unwrap_or(0.0);
            let color = cache
                .get_border_bottom_color(node_data, &dom_id, &node_state)
                .and_then(|c| c.get_property())
                .map(|c| c.inner)
                .unwrap_or(ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                });
            BorderInfo::new(width, style_val.inner, color, source)
        })
        .unwrap_or_else(|| default_border.clone());

    // Left border
    let left = cache
        .get_border_left_style(node_data, &dom_id, &node_state)
        .and_then(|s| s.get_property())
        .map(|style_val| {
            let width = cache
                .get_border_left_width(node_data, &dom_id, &node_state)
                .and_then(|w| w.get_property())
                .map(|w| {
                    w.inner
                        .resolve_with_context(&resolution_context, PropertyContext::BorderWidth)
                })
                .unwrap_or(0.0);
            let color = cache
                .get_border_left_color(node_data, &dom_id, &node_state)
                .and_then(|c| c.get_property())
                .map(|c| c.inner)
                .unwrap_or(ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                });
            BorderInfo::new(width, style_val.inner, color, source)
        })
        .unwrap_or_else(|| default_border.clone());

    (top, right, bottom, left)
}

// +spec:table-layout:c5e446 - table-layout property (auto|fixed) controls layout algorithm selection
/// Get the table-layout property for a table node
fn get_table_layout_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
) -> LayoutTableLayout {
    let Some(dom_id) = node.dom_node_id else {
        return LayoutTableLayout::Auto;
    };

    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state.clone();

    ctx.styled_dom
        .css_property_cache
        .ptr
        .get_table_layout(node_data, &dom_id, &node_state)
        .and_then(|prop| prop.get_property().copied())
        .unwrap_or(LayoutTableLayout::Auto)
}

/// Get the border-collapse property for a table node
fn get_border_collapse_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
) -> StyleBorderCollapse {
    let Some(dom_id) = node.dom_node_id else {
        return StyleBorderCollapse::Separate;
    };

    // FAST PATH: compact cache
    if let Some(ref cc) = ctx.styled_dom.css_property_cache.ptr.compact_cache {
        return cc.get_border_collapse(dom_id.index());
    }

    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state.clone();

    ctx.styled_dom
        .css_property_cache
        .ptr
        .get_border_collapse(node_data, &dom_id, &node_state)
        .and_then(|prop| prop.get_property().copied())
        .unwrap_or(StyleBorderCollapse::Separate)
}

/// Get the border-spacing property for a table node
fn get_border_spacing_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
) -> LayoutBorderSpacing {
    if let Some(dom_id) = node.dom_node_id {
        // FAST PATH: compact cache
        if let Some(ref cc) = ctx.styled_dom.css_property_cache.ptr.compact_cache {
            let idx = dom_id.index();
            let h_raw = cc.get_border_spacing_h_raw(idx);
            let v_raw = cc.get_border_spacing_v_raw(idx);
            // If both are non-sentinel, use compact values
            if h_raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD
                && v_raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD
            {
                return LayoutBorderSpacing::new_separate(
                    azul_css::props::basic::pixel::PixelValue::px(h_raw as f32 / 10.0),
                    azul_css::props::basic::pixel::PixelValue::px(v_raw as f32 / 10.0),
                );
            }
            // sentinel → fall through to slow path
        }

        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state.clone();

        if let Some(prop) = ctx.styled_dom.css_property_cache.ptr.get_border_spacing(
            node_data,
            &dom_id,
            &node_state,
        ) {
            if let Some(value) = prop.get_property() {
                return *value;
            }
        }
    }

    LayoutBorderSpacing::default() // Default: 0
}

/// Get the empty-cells property for a table-cell node.
/// Returns Show (default) or Hide.
fn get_empty_cells_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
) -> StyleEmptyCells {
    let Some(dom_id) = node.dom_node_id else {
        return StyleEmptyCells::Show;
    };

    let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
    let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state.clone();

    ctx.styled_dom
        .css_property_cache
        .ptr
        .get_empty_cells(node_data, &dom_id, &node_state)
        .and_then(|prop| prop.get_property().copied())
        .unwrap_or(StyleEmptyCells::Show)
}

/// CSS 2.2 Section 17.4 - Tables in the visual formatting model:
///
/// "The caption box is a block box that retains its own content, padding,
/// border, and margin areas. The caption-side property specifies the position
/// of the caption box with respect to the table box."
///
/// Get the caption-side property for a table node.
/// Returns Top (default) or Bottom.
fn get_caption_side_property<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
) -> StyleCaptionSide {
    if let Some(dom_id) = node.dom_node_id {
        let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state.clone();

        if let Some(prop) =
            ctx.styled_dom
                .css_property_cache
                .ptr
                .get_caption_side(node_data, &dom_id, &node_state)
        {
            if let Some(value) = prop.get_property() {
                return *value;
            }
        }
    }

    StyleCaptionSide::Top // Default per CSS 2.2
}

//   removes entire row or column from display; space made available for other content;
//   spanned content clipped; does not otherwise affect table layout
// +spec:inline-formatting-context:9f5f31 - visibility:collapse for table rows/columns, border-collapse and border-spacing
/// CSS 2.2 Section 17.6 - Dynamic row and column effects:
///
// +spec:box-model:547563 - visibility:collapse removes table rows/columns; elsewhere same as hidden
/// "The 'visibility' value 'collapse' removes a row or column from display,
/// but it has a different effect than 'visibility: hidden' on other elements.
/// When a row or column is collapsed, the space normally occupied by the row
/// or column is removed."
///
/// Check if a node has visibility:collapse set.
///
/// This is used for table rows and columns to optimize dynamic hiding.
/// // +spec:overflow:ebb1f9 - For non-table elements, collapse == hidden (no special handling needed)
fn is_visibility_collapsed<T: ParsedFontTrait>(
    ctx: &LayoutContext<'_, T>,
    node: &LayoutNode,
) -> bool {
    if let Some(dom_id) = node.dom_node_id {
        let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state.clone();

        if let MultiValue::Exact(value) = get_visibility(ctx.styled_dom, dom_id, &node_state) {
            return matches!(value, StyleVisibility::Collapse);
        }
    }

    false
}

// +spec:overflow:af97a8 - empty-cells in separated borders model; collapsing border overflow
// +spec:table-layout:dcdf1b - empty-cells property controls rendering of borders/backgrounds around empty cells in separated borders model
/// CSS 2.2 Section 17.6.1.1 - Borders and Backgrounds around empty cells
///
/// In the separated borders model, the 'empty-cells' property controls the rendering of
/// borders and backgrounds around cells that have no visible content. Empty means it has no
/// children, or has children that are only collapsed whitespace."
///
/// Check if a table cell is empty (has no visible content).
///
/// This is used by the rendering pipeline to decide whether to paint borders/backgrounds
/// when empty-cells: hide is set in separated border model.
///
//   in-flow content (including empty elements) other than collapsed whitespace
/// A cell is considered empty if:
///
/// - It has no children, OR
/// - It has children but no inline_layout_result (no rendered content)
///
/// Note: Full whitespace detection would require checking text content during rendering.
/// This function provides a basic check suitable for layout phase.
fn is_cell_empty(tree: &LayoutTree, cell_index: usize) -> bool {
    let cell_node = match tree.get(cell_index) {
        Some(node) => node,
        None => return true, // Invalid cell is considered empty
    };

    // No children = empty
    if tree.children(cell_index).is_empty() {
        return true;
    }

    // If cell has an inline layout result, check if it's empty
    if let Some(ref cached_layout) = cell_node.inline_layout_result {
        // Check if inline layout has any rendered content
        // Empty inline layouts have no items (glyphs/fragments)
        // Note: This is a heuristic - full detection requires text content analysis
        return cached_layout.layout.items.is_empty();
    }

    // Check if all children have no content
    // A more thorough check would recursively examine all descendants
    //
    // For now, we use a simple heuristic: if there are children, assume not empty
    // unless proven otherwise by inline_layout_result

    // Cell with children but no inline layout = likely has block-level content = not empty
    false
}

/// Main function to layout a table formatting context
// +spec:table-layout:235e8e - CSS 2.2 §17.1-17.2 table model: fixed/auto algorithms, row/column/cell/caption structure
// +spec:table-layout:a6422d - CSS table model: table structure analysis, row/column/cell layout, caption, border-collapse
pub fn layout_table_fc<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    node_index: usize,
    constraints: &LayoutConstraints,
) -> Result<LayoutOutput> {
    debug_log!(ctx, "Laying out table");

    debug_table_layout!(
        ctx,
        "node_index={}, available_size={:?}, writing_mode={:?}",
        node_index,
        constraints.available_size,
        constraints.writing_mode
    );

    // Multi-pass table layout algorithm:
    //
    // 1. Analyze table structure - identify rows, cells, columns
    // 2. Determine table-layout property (fixed vs auto)
    // 3. Calculate column widths
    // 4. Layout cells and calculate row heights
    // 5. Position cells in final grid

    // Get the table node to read CSS properties
    let table_node = tree
        .get(node_index)
        .ok_or(LayoutError::InvalidTree)?
        .clone();

    // Calculate the table's border-box width for column distribution
    // This accounts for the table's own width property (e.g., width: 100%)
    let table_border_box_width = if let Some(dom_id) = table_node.dom_node_id {
        // Use calculate_used_size_for_node to resolve table width (respects width:100%)
        let intrinsic = table_node.intrinsic_sizes.clone().unwrap_or_default();
        let containing_block_size = LogicalSize {
            width: constraints.available_size.width,
            height: constraints.available_size.height,
        };

        let table_size = crate::solver3::sizing::calculate_used_size_for_node(
            ctx.styled_dom,
            Some(dom_id),
            containing_block_size,
            intrinsic,
            &table_node.box_props,
            ctx.viewport_size,
        )?;

        table_size.width
    } else {
        constraints.available_size.width
    };

    // Subtract padding and border to get content-box width for column distribution
    let table_content_box_width = {
        let padding_width = table_node.box_props.padding.left + table_node.box_props.padding.right;
        let border_width = table_node.box_props.border.left + table_node.box_props.border.right;
        (table_border_box_width - padding_width - border_width).max(0.0)
    };

    debug_table_layout!(ctx, "Table Layout Debug");
    debug_table_layout!(ctx, "Node index: {}", node_index);
    debug_table_layout!(
        ctx,
        "Available size from parent: {:.2} x {:.2}",
        constraints.available_size.width,
        constraints.available_size.height
    );
    debug_table_layout!(ctx, "Table border-box width: {:.2}", table_border_box_width);
    debug_table_layout!(
        ctx,
        "Table content-box width: {:.2}",
        table_content_box_width
    );
    debug_table_layout!(
        ctx,
        "Table padding: L={:.2} R={:.2}",
        table_node.box_props.padding.left,
        table_node.box_props.padding.right
    );
    debug_table_layout!(
        ctx,
        "Table border: L={:.2} R={:.2}",
        table_node.box_props.border.left,
        table_node.box_props.border.right
    );
    debug_table_layout!(ctx, "=");

    // Phase 1: Analyze table structure
    let mut table_ctx = analyze_table_structure(tree, node_index, ctx)?;

    // +spec:table-layout:ff5671 - table-layout property (fixed vs auto) controls column width algorithm
    // +spec:width-calculation:7a5b23 - table-layout property determines fixed vs auto algorithm (CSS 2.2 §17.5.2)
    // Phase 2: Read CSS properties and determine layout algorithm
    let table_layout = get_table_layout_property(ctx, &table_node);
    table_ctx.use_fixed_layout = matches!(table_layout, LayoutTableLayout::Fixed);

    // +spec:containing-block:cc1453 - collapsing border model: border-collapse property drives table border handling
    // Read border properties
    table_ctx.border_collapse = get_border_collapse_property(ctx, &table_node);
    table_ctx.border_spacing = get_border_spacing_property(ctx, &table_node);

    debug_log!(
        ctx,
        "Table layout: {:?}, border-collapse: {:?}, border-spacing: {:?}",
        table_layout,
        table_ctx.border_collapse,
        table_ctx.border_spacing
    );

    // +spec:width-calculation:431d60 - fixed vs auto table layout column width algorithms (CSS 2.2 §17.5.2.1, §17.5.2.2)
    // Phase 3: Calculate column widths
    if table_ctx.use_fixed_layout {
        // DEBUG: Log available width passed into fixed column calculation
        debug_table_layout!(
            ctx,
            "FIXED layout: table_content_box_width={:.2}",
            table_content_box_width
        );
        calculate_column_widths_fixed(ctx, tree, &mut table_ctx, table_content_box_width);
    } else {
        // Pass table_content_box_width for column distribution in auto layout
        calculate_column_widths_auto_with_width(
            &mut table_ctx,
            tree,
            text_cache,
            ctx,
            constraints,
            table_content_box_width,
        )?;
    }

    debug_table_layout!(ctx, "After column width calculation:");
    debug_table_layout!(ctx, "  Number of columns: {}", table_ctx.columns.len());
    for (i, col) in table_ctx.columns.iter().enumerate() {
        debug_table_layout!(
            ctx,
            "  Column {}: width={:.2}",
            i,
            col.computed_width.unwrap_or(0.0)
        );
    }
    let total_col_width: f32 = table_ctx
        .columns
        .iter()
        .filter_map(|c| c.computed_width)
        .sum();
    debug_table_layout!(ctx, "  Total column width: {:.2}", total_col_width);

    // Phase 4: Calculate row heights based on cell content
    calculate_row_heights(&mut table_ctx, tree, text_cache, ctx, constraints)?;

    // Phase 5: Position cells in final grid and collect positions
    let mut cell_positions =
        position_table_cells(&mut table_ctx, tree, ctx, node_index, constraints)?;

    // Calculate final table size including border-spacing
    let mut table_width: f32 = table_ctx
        .columns
        .iter()
        .filter_map(|col| col.computed_width)
        .sum();
    let mut table_height: f32 = table_ctx.row_heights.iter().sum();

    debug_table_layout!(
        ctx,
        "After calculate_row_heights: table_height={:.2}, row_heights={:?}",
        table_height,
        table_ctx.row_heights
    );

    // +spec:box-model:494f6b - collapsing border model: row-width formula and table border width computation
    // +spec:box-model:e7d0a3 - Separated borders model: border-spacing, empty-cells, collapsing border width calculation
    // +spec:box-sizing:ee702c - separated borders model: border-spacing between adjoining cells
    // Add border-spacing to table size if border-collapse is separate
    if table_ctx.border_collapse == StyleBorderCollapse::Separate {
        use get_element_font_size;
        use get_parent_font_size;
        use get_root_font_size;
        use PhysicalSize;
        use PropertyContext;
        use ResolutionContext;

        let styled_dom = ctx.styled_dom;
        let table_id = tree.nodes[node_index].dom_node_id.unwrap();
        let table_state = &styled_dom.styled_nodes.as_container()[table_id].styled_node_state;

        let spacing_context = ResolutionContext {
            element_font_size: get_element_font_size(styled_dom, table_id, table_state),
            parent_font_size: get_parent_font_size(styled_dom, table_id, table_state),
            root_font_size: get_root_font_size(styled_dom, table_state),
            containing_block_size: PhysicalSize::new(0.0, 0.0),
            element_size: None,
            // TODO: Get actual DPI scale from ctx
            viewport_size: PhysicalSize::new(0.0, 0.0),
        };

        let h_spacing = table_ctx
            .border_spacing
            .horizontal
            .resolve_with_context(&spacing_context, PropertyContext::Other)
            .max(0.0);
        let v_spacing = table_ctx
            .border_spacing
            .vertical
            .resolve_with_context(&spacing_context, PropertyContext::Other)
            .max(0.0);

        // Add spacing: left + (n-1 between columns) + right = n+1 spacings
        let num_cols = table_ctx.columns.len();
        if num_cols > 0 {
            table_width += h_spacing * (num_cols + 1) as f32;
        }

        // Add spacing: top + (n-1 between rows) + bottom = n+1 spacings
        if table_ctx.num_rows > 0 {
            let full_spacings = (table_ctx.num_rows + 1) as f32;
            // Each hidden-empty row loses one side of border-spacing
            let hidden_empty_count = table_ctx.hidden_empty_rows.len() as f32;
            table_height += v_spacing * (full_spacings - hidden_empty_count);
        }
    }

    // +spec:table-layout:24dbf9 - §17.4 table wrapper box model: caption positioning, BFC establishment
    // +spec:width-calculation:600f98 - caption-side positions caption above/below table box (CSS 2.2 §17.4)
    // CSS 2.2 Section 17.4: Layout and position the caption if present
    //
    // "The caption box is a block box that retains its own content,
    // padding, border, and margin areas."
    let caption_side = get_caption_side_property(ctx, &table_node);
    let mut caption_height = 0.0;
    let mut table_y_offset = 0.0;

    if let Some(caption_idx) = table_ctx.caption_index {
        debug_log!(
            ctx,
            "Laying out caption with caption-side: {:?}",
            caption_side
        );

        // Layout caption as a block with the table's width as available width
        let caption_constraints = LayoutConstraints {
            available_size: LogicalSize {
                width: table_width,
                height: constraints.available_size.height,
            },
            writing_mode: constraints.writing_mode,
            writing_mode_ctx: constraints.writing_mode_ctx,
            bfc_state: None, // Caption creates its own BFC
            text_align: constraints.text_align,
            containing_block_size: constraints.containing_block_size,
            available_width_type: Text3AvailableSpace::Definite(table_width),
        };

        // Layout the caption node
        let mut empty_float_cache = HashMap::new();
        let caption_result = layout_formatting_context(
            ctx,
            tree,
            text_cache,
            caption_idx,
            &caption_constraints,
            &mut empty_float_cache,
        )?;
        caption_height = caption_result.output.overflow_size.height;

        let caption_position = match caption_side {
            StyleCaptionSide::Top => {
                // Caption on top: position at y=0, table starts below caption
                table_y_offset = caption_height;
                LogicalPosition { x: 0.0, y: 0.0 }
            }
            StyleCaptionSide::Bottom => {
                // Caption on bottom: table starts at y=0, caption below table
                LogicalPosition {
                    x: 0.0,
                    y: table_height,
                }
            }
        };

        // Add caption position to the positions map
        cell_positions.insert(caption_idx, caption_position);

        debug_log!(
            ctx,
            "Caption positioned at x={:.2}, y={:.2}, height={:.2}",
            caption_position.x,
            caption_position.y,
            caption_height
        );
    }

    // Adjust all table cell positions if caption is on top
    if table_y_offset > 0.0 {
        debug_log!(
            ctx,
            "Adjusting table cells by y offset: {:.2}",
            table_y_offset
        );

        // Adjust cell positions in the map
        for cell_info in &table_ctx.cells {
            if let Some(pos) = cell_positions.get_mut(&cell_info.node_index) {
                pos.y += table_y_offset;
            }
        }
    }

    let total_height = table_height + caption_height;

    debug_table_layout!(ctx, "Final table dimensions:");
    debug_table_layout!(ctx, "  Content width (columns): {:.2}", table_width);
    debug_table_layout!(ctx, "  Content height (rows): {:.2}", table_height);
    debug_table_layout!(ctx, "  Caption height: {:.2}", caption_height);
    debug_table_layout!(ctx, "  Total height: {:.2}", total_height);
    debug_table_layout!(ctx, "End Table Debug");

    // Create output with the table's final size and cell positions
    // +spec:box-model:52fcfe - overflow_size must include borders that spill into margin in collapsing border model
    let output = LayoutOutput {
        overflow_size: LogicalSize {
            width: table_width,
            height: total_height,
        },
        // Cell positions calculated in position_table_cells
        positions: cell_positions,
        // line box or first in-flow table-row; if none, bottom of content edge
        // TODO: implement proper table baseline propagation
        baseline: None,
    };

    Ok(output)
}

// +spec:display-property:f47f8a - Table structure analysis: caption positioning, row/column/row-group traversal per CSS 2.2 §17.4-17.5
/// Analyze the table structure to identify rows, cells, and columns
fn analyze_table_structure<T: ParsedFontTrait>(
    tree: &LayoutTree,
    table_index: usize,
    ctx: &mut LayoutContext<'_, T>,
) -> Result<TableLayoutContext> {
    let mut table_ctx = TableLayoutContext::new();

    let table_node = tree.get(table_index).ok_or(LayoutError::InvalidTree)?;

    // +spec:width-calculation:0a2766 - table internal elements form rectangular grid of rows/columns (CSS 2.2 §17.5)
    // CSS 2.2 Section 17.4: A table may have one table-caption child.
    // Traverse children to find caption, columns/colgroups, rows, and row groups
    for &child_idx in tree.children(table_index) {
        if let Some(child) = tree.get(child_idx) {
            // Check if this is a table caption
            if matches!(child.formatting_context, FormattingContext::TableCaption) {
                debug_log!(ctx, "Found table caption at index {}", child_idx);
                table_ctx.caption_index = Some(child_idx);
                continue;
            }

            // CSS 2.2 Section 17.2: Check for column groups
            if matches!(
                child.formatting_context,
                FormattingContext::TableColumnGroup
            ) {
                analyze_table_colgroup(tree, child_idx, &mut table_ctx, ctx)?;
                continue;
            }

            // Check if this is a table row or row group
            match child.formatting_context {
                FormattingContext::TableRow => {
                    analyze_table_row(tree, child_idx, &mut table_ctx, ctx)?;
                }
                FormattingContext::TableRowGroup => {
                    // Process rows within the row group
                    for &row_idx in tree.children(child_idx) {
                        if let Some(row) = tree.get(row_idx) {
                            if matches!(row.formatting_context, FormattingContext::TableRow) {
                                analyze_table_row(tree, row_idx, &mut table_ctx, ctx)?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    debug_log!(
        ctx,
        "Table structure: {} rows, {} columns, {} cells{}",
        table_ctx.num_rows,
        table_ctx.columns.len(),
        table_ctx.cells.len(),
        if table_ctx.caption_index.is_some() {
            ", has caption"
        } else {
            ""
        }
    );

    Ok(table_ctx)
}

/// Analyze a table column group to identify columns and track collapsed columns
///
/// - CSS 2.2 Section 17.2: Column groups contain columns
/// - CSS 2.2 Section 17.6: Columns can have visibility:collapse
fn analyze_table_colgroup<T: ParsedFontTrait>(
    tree: &LayoutTree,
    colgroup_index: usize,
    table_ctx: &mut TableLayoutContext,
    ctx: &mut LayoutContext<'_, T>,
) -> Result<()> {
    let colgroup_node = tree.get(colgroup_index).ok_or(LayoutError::InvalidTree)?;

    // Check if the colgroup itself has visibility:collapse
    if is_visibility_collapsed(ctx, colgroup_node) {
        // All columns in this group should be collapsed
        // TODO: For now, just mark the group (actual column indices will be determined later)
        debug_log!(
            ctx,
            "Column group at index {} has visibility:collapse",
            colgroup_index
        );
    }

    // Check for individual column elements within the group
    for &col_idx in tree.children(colgroup_index) {
        if let Some(col_node) = tree.get(col_idx) {
            // Note: Individual columns don't have a FormattingContext::TableColumn
            // They are represented as children of TableColumnGroup
            // Check visibility:collapse on each column
            if is_visibility_collapsed(ctx, col_node) {
                // We need to determine the actual column index this represents
                // For now, we'll track it during cell analysis
                debug_log!(ctx, "Column at index {} has visibility:collapse", col_idx);
            }
        }
    }

    Ok(())
}

// +spec:display-property:7f167c - Table grid cell placement: rows fill table top-to-bottom, cells placed left-to-right with colspan/rowspan
/// Analyze a table row to identify cells and update column count
fn analyze_table_row<T: ParsedFontTrait>(
    tree: &LayoutTree,
    row_index: usize,
    table_ctx: &mut TableLayoutContext,
    ctx: &mut LayoutContext<'_, T>,
) -> Result<()> {
    // +spec:inline-formatting-context:3f8091 - table visual layout: cells occupy grid cells, row/column spanning
    let row_node = tree.get(row_index).ok_or(LayoutError::InvalidTree)?;
    let row_num = table_ctx.num_rows;
    table_ctx.num_rows += 1;

    // CSS 2.2 Section 17.6: Check if this row has visibility:collapse
    if is_visibility_collapsed(ctx, row_node) {
        debug_log!(ctx, "Row {} has visibility:collapse", row_num);
        table_ctx.collapsed_rows.insert(row_num);
    }

    let mut col_index = 0;

    for &cell_idx in tree.children(row_index) {
        if let Some(cell) = tree.get(cell_idx) {
            if matches!(cell.formatting_context, FormattingContext::TableCell) {
                // Get colspan and rowspan (TODO: from CSS properties)
                let colspan = 1; // TODO: Get from CSS
                let rowspan = 1; // TODO: Get from CSS

                let cell_info = TableCellInfo {
                    node_index: cell_idx,
                    column: col_index,
                    colspan,
                    row: row_num,
                    rowspan,
                };

                table_ctx.cells.push(cell_info);

                // Update column count
                let max_col = col_index + colspan;
                while table_ctx.columns.len() < max_col {
                    table_ctx.columns.push(TableColumnInfo {
                        min_width: 0.0,
                        max_width: 0.0,
                        computed_width: None,
                    });
                }

                col_index += colspan;
            }
        }
    }

    Ok(())
}

// +spec:overflow:66f584 - Fixed table layout: cells use overflow property to clip overflowing content
// +spec:positioning:46070a - Fixed table layout (17.5.2.1) and auto table layout (17.5.2.2) column width algorithms
// +spec:table-layout:875401 - Fixed table layout algorithm (17.5.2.1): column widths from first-row cells, remaining columns divide space equally, table width = max(width property, sum of columns)
/// Calculate column widths using the fixed table layout algorithm
/// // +spec:overflow:de613c - Fixed table layout algorithm (CSS 2.2 Section 17.5.2.1)
// +spec:table-layout:8b72b3 - fixed table layout: column width from column elements/first-row cells, remaining columns equal division
///
/// CSS 2.2 Section 17.5.2.1: In fixed table layout, the horizontal layout
/// does not depend on cell contents. Column widths are determined by:
/// 1. Column elements with explicit (non-auto) width
/// 2. First-row cells with explicit (non-auto) width
/// 3. Remaining columns equally divide remaining horizontal space
///
/// CSS 2.2 Section 17.6: Columns with visibility:collapse are excluded
/// from width calculations
// +spec:table-layout:c5e446 - Fixed table layout algorithm: column widths from col elements or first-row cells, remaining columns divide equally
/// +spec:width-calculation:8c958a - Fixed table layout: column widths from col elements, first-row cells, then equal distribution (CSS 2.2 §17.5.2.1)
fn calculate_column_widths_fixed<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    table_ctx: &mut TableLayoutContext,
    available_width: f32,
) {
    debug_table_layout!(
        ctx,
        "calculate_column_widths_fixed: num_cols={}, available_width={:.2}",
        table_ctx.columns.len(),
        available_width
    );

    let num_cols = table_ctx.columns.len();
    if num_cols == 0 {
        return;
    }

    let num_visible_cols = num_cols - table_ctx.collapsed_columns.len();
    if num_visible_cols == 0 {
        for col in &mut table_ctx.columns {
            col.computed_width = Some(0.0);
        }
        return;
    }

    // Step 1 (column elements) is skipped because column elements don't store
    // explicit widths in the current table structure analysis.
    // Step 2: Check first-row cells for explicit width properties.
    let mut col_has_width = vec![false; num_cols];

    for cell_info in &table_ctx.cells {
        if cell_info.row != 0 {
            continue; // Only consider cells in the first row
        }
        if table_ctx.collapsed_columns.contains(&cell_info.column) {
            continue;
        }

        // Look up the cell's CSS width via its dom_node_id
        let dom_id = match tree.get(cell_info.node_index).and_then(|n| n.dom_node_id) {
            Some(id) => id,
            None => continue,
        };

        let node_state = &ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
        let css_width = get_css_width(ctx.styled_dom, dom_id, &node_state);

        let explicit_px = match css_width.unwrap_or_default() {
            LayoutWidth::Px(px) => {
                resolve_size_metric(
                    px.metric,
                    px.number.get(),
                    available_width,
                    ctx.viewport_size,
                )
            }
            LayoutWidth::Auto | LayoutWidth::MinContent | LayoutWidth::MaxContent
            | LayoutWidth::Calc(_) | LayoutWidth::FitContent(_) => continue,
        };

        if cell_info.colspan == 1 {
            table_ctx.columns[cell_info.column].computed_width = Some(explicit_px);
            col_has_width[cell_info.column] = true;
        } else {
            let mut visible_span_count = 0;
            for offset in 0..cell_info.colspan {
                let col_idx = cell_info.column + offset;
                if col_idx < num_cols && !table_ctx.collapsed_columns.contains(&col_idx) {
                    visible_span_count += 1;
                }
            }
            if visible_span_count > 0 {
                let per_col = explicit_px / visible_span_count as f32;
                for offset in 0..cell_info.colspan {
                    let col_idx = cell_info.column + offset;
                    if col_idx < num_cols
                        && !table_ctx.collapsed_columns.contains(&col_idx)
                        && !col_has_width[col_idx]
                    {
                        table_ctx.columns[col_idx].computed_width = Some(per_col);
                        col_has_width[col_idx] = true;
                    }
                }
            }
        }
    }

    let used_width: f32 = table_ctx.columns.iter().enumerate()
        .filter(|(idx, _)| col_has_width[*idx] && !table_ctx.collapsed_columns.contains(idx))
        .filter_map(|(_, c)| c.computed_width)
        .sum();
    let remaining_width = (available_width - used_width).max(0.0);
    let num_remaining = table_ctx.columns.iter().enumerate()
        .filter(|(idx, _)| !col_has_width[*idx] && !table_ctx.collapsed_columns.contains(idx))
        .count();

    if num_remaining > 0 {
        let width_per_remaining = remaining_width / num_remaining as f32;
        for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
            if table_ctx.collapsed_columns.contains(&col_idx) {
                col.computed_width = Some(0.0);
            } else if !col_has_width[col_idx] {
                col.computed_width = Some(width_per_remaining);
            }
        }
    }

    // Set collapsed columns to zero width
    for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
        if table_ctx.collapsed_columns.contains(&col_idx) {
            col.computed_width = Some(0.0);
        }
    }

    let total_col_width: f32 = table_ctx.columns.iter()
        .filter_map(|c| c.computed_width)
        .sum();
    if available_width > total_col_width && num_visible_cols > 0 {
        let extra = available_width - total_col_width;
        let extra_per_col = extra / num_visible_cols as f32;
        for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
            if !table_ctx.collapsed_columns.contains(&col_idx) {
                if let Some(ref mut w) = col.computed_width {
                    *w += extra_per_col;
                }
            }
        }
    }
}

/// Measure a cell's content width for a given intrinsic sizing mode.
///
/// CSS 2.2 Section 17.5.2.2: shared helper for min-content and max-content
/// width measurement. Lays out the cell subtree in ComputeSize mode and
/// returns the border-box width (content + padding + border).
fn measure_cell_content_width<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    cell_index: usize,
    constraints: &LayoutConstraints,
    sizing_mode: crate::text3::cache::AvailableSpace,
) -> Result<f32> {
    let width_type = match sizing_mode {
        crate::text3::cache::AvailableSpace::MinContent => Text3AvailableSpace::MinContent,
        crate::text3::cache::AvailableSpace::MaxContent => Text3AvailableSpace::MaxContent,
        crate::text3::cache::AvailableSpace::Definite(w) => Text3AvailableSpace::Definite(w),
    };
    let cell_constraints = LayoutConstraints {
        available_size: LogicalSize {
            width: sizing_mode.to_f32_for_layout(),
            height: f32::INFINITY,
        },
        writing_mode: constraints.writing_mode,
        writing_mode_ctx: constraints.writing_mode_ctx,
        bfc_state: None,
        text_align: constraints.text_align,
        containing_block_size: constraints.containing_block_size,
        available_width_type: width_type,
    };

    let mut temp_positions: super::PositionVec = Vec::new();
    let mut temp_scrollbar_reflow = false;
    let mut temp_float_cache = HashMap::new();

    crate::solver3::cache::calculate_layout_for_subtree(
        ctx,
        tree,
        text_cache,
        cell_index,
        LogicalPosition::zero(),
        cell_constraints.available_size,
        &mut temp_positions,
        &mut temp_scrollbar_reflow,
        &mut temp_float_cache,
        crate::solver3::cache::ComputeMode::ComputeSize,
    )?;

    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let size = cell_node.used_size.unwrap_or_default();
    let padding = &cell_node.box_props.padding;
    let border = &cell_node.box_props.border;
    let wm = constraints.writing_mode;

    Ok(size.width
        + padding.cross_start(wm) + padding.cross_end(wm)
        + border.cross_start(wm) + border.cross_end(wm))
}

/// Measure a cell's minimum content width (with maximum wrapping)
fn measure_cell_min_content_width<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    cell_index: usize,
    constraints: &LayoutConstraints,
) -> Result<f32> {
    measure_cell_content_width(
        ctx, tree, text_cache, cell_index, constraints,
        crate::text3::cache::AvailableSpace::MinContent,
    )
}

/// Measure a cell's maximum content width (without wrapping)
fn measure_cell_max_content_width<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    cell_index: usize,
    constraints: &LayoutConstraints,
) -> Result<f32> {
    measure_cell_content_width(
        ctx, tree, text_cache, cell_index, constraints,
        crate::text3::cache::AvailableSpace::MaxContent,
    )
}

/// Calculate column widths using the auto table layout algorithm
fn calculate_column_widths_auto<T: ParsedFontTrait>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    ctx: &mut LayoutContext<'_, T>,
    constraints: &LayoutConstraints,
) -> Result<()> {
    calculate_column_widths_auto_with_width(
        table_ctx,
        tree,
        text_cache,
        ctx,
        constraints,
        constraints.available_size.width,
    )
}

/// Calculate column widths using the auto table layout algorithm with explicit table width
// +spec:display-property:05c8e8 - CSS 2.2 §17.5.2.2 automatic table layout: column min/max widths, table width = max(W or CB, CAPMIN, MIN), extra width distributed over columns
/// +spec:overflow:29edde - CSS 2.2 §17.5.2.2 automatic table layout: MCW/max-content per cell, column min/max, colspan distribution, final width determination
// +spec:table-layout:23a215 - automatic table layout: MCW/max cell widths, column min/max, colspan distribution, table width from MAX/MIN/CAPMIN
// +spec:table-layout:5e1145 - Automatic table layout: MCW/max-content per cell, column min/max, colspan distribution, final width from MIN/MAX
// +spec:width-calculation:42dfca - CSS 2.2 §17.5.2.2 automatic table layout: MCW/max-content per cell, column min/max, multi-span distribution, final table width
/// +spec:width-calculation:335ef1 - Automatic table layout: width given by column widths and borders (CSS 2.2 §17.5.2.2)
fn calculate_column_widths_auto_with_width<T: ParsedFontTrait>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    ctx: &mut LayoutContext<'_, T>,
    constraints: &LayoutConstraints,
    table_width: f32,
) -> Result<()> {
    // Auto layout: calculate min/max content width for each cell
    let num_cols = table_ctx.columns.len();
    if num_cols == 0 {
        return Ok(());
    }

    // Step 1: Measure all cells to determine column min/max widths
    // CSS 2.2 Section 17.6: Skip cells in collapsed columns
    for cell_info in &table_ctx.cells {
        // Skip cells in collapsed columns
        if table_ctx.collapsed_columns.contains(&cell_info.column) {
            continue;
        }

        // Skip cells that span into collapsed columns
        let mut spans_collapsed = false;
        for col_offset in 0..cell_info.colspan {
            if table_ctx
                .collapsed_columns
                .contains(&(cell_info.column + col_offset))
            {
                spans_collapsed = true;
                break;
            }
        }
        if spans_collapsed {
            continue;
        }

        let min_width = measure_cell_min_content_width(
            ctx,
            tree,
            text_cache,
            cell_info.node_index,
            constraints,
        )?;

        let max_width = measure_cell_max_content_width(
            ctx,
            tree,
            text_cache,
            cell_info.node_index,
            constraints,
        )?;

        // Handle single-column cells
        if cell_info.colspan == 1 {
            let col = &mut table_ctx.columns[cell_info.column];
            col.min_width = col.min_width.max(min_width);
            col.max_width = col.max_width.max(max_width);
        } else {
            // Handle multi-column cells (colspan > 1)
            // Distribute the cell's min/max width across the spanned columns
            distribute_cell_width_across_columns(
                &mut table_ctx.columns,
                cell_info.column,
                cell_info.colspan,
                min_width,
                max_width,
                &table_ctx.collapsed_columns,
            );
        }
    }

    // Step 2: Calculate final column widths based on available space
    // Exclude collapsed columns from total width calculations
    let total_min_width: f32 = table_ctx
        .columns
        .iter()
        .enumerate()
        .filter(|(idx, _)| !table_ctx.collapsed_columns.contains(idx))
        .map(|(_, c)| c.min_width)
        .sum();
    let total_max_width: f32 = table_ctx
        .columns
        .iter()
        .enumerate()
        .filter(|(idx, _)| !table_ctx.collapsed_columns.contains(idx))
        .map(|(_, c)| c.max_width)
        .sum();
    let available_width = table_width; // Use table's content-box width, not constraints

    debug_table_layout!(
        ctx,
        "calculate_column_widths_auto: min={:.2}, max={:.2}, table_width={:.2}",
        total_min_width,
        total_max_width,
        table_width
    );

    // Handle infinity and NaN cases
    if !total_max_width.is_finite() || !available_width.is_finite() {
        // If max_width is infinite or unavailable, distribute available width equally
        let num_non_collapsed = table_ctx.columns.len() - table_ctx.collapsed_columns.len();
        let width_per_column = if num_non_collapsed > 0 {
            available_width / num_non_collapsed as f32
        } else {
            0.0
        };

        for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
            if table_ctx.collapsed_columns.contains(&col_idx) {
                col.computed_width = Some(0.0);
            } else {
                // Use the larger of min_width and equal distribution
                col.computed_width = Some(col.min_width.max(width_per_column));
            }
        }
    } else if available_width >= total_max_width {
        // Case 1: More space than max-content - distribute excess proportionally
        //
        // CSS 2.1 Section 17.5.2.2: Distribute extra space proportionally to
        // max-content widths
        let excess_width = available_width - total_max_width;

        // First pass: collect column info (max_width) to avoid borrowing issues
        let column_info: Vec<(usize, f32, bool)> = table_ctx
            .columns
            .iter()
            .enumerate()
            .map(|(idx, c)| (idx, c.max_width, table_ctx.collapsed_columns.contains(&idx)))
            .collect();

        // Calculate total weight for proportional distribution (use max_width as weight)
        let total_weight: f32 = column_info.iter()
            .filter(|(_, _, is_collapsed)| !is_collapsed)
            .map(|(_, max_w, _)| max_w.max(1.0)) // Avoid division by zero
            .sum();

        let num_non_collapsed = column_info
            .iter()
            .filter(|(_, _, is_collapsed)| !is_collapsed)
            .count();

        // Second pass: set computed widths
        for (col_idx, max_width, is_collapsed) in column_info {
            let col = &mut table_ctx.columns[col_idx];
            if is_collapsed {
                col.computed_width = Some(0.0);
            } else {
                // Start with max-content width, then add proportional share of excess
                let weight_factor = if total_weight > 0.0 {
                    max_width.max(1.0) / total_weight
                } else {
                    // If all columns have 0 max_width, distribute equally
                    1.0 / num_non_collapsed.max(1) as f32
                };

                let final_width = max_width + (excess_width * weight_factor);
                col.computed_width = Some(final_width);
            }
        }
    } else if available_width >= total_min_width {
        // Case 2: Between min and max - interpolate proportionally
        // Avoid division by zero if min == max
        let scale = if total_max_width > total_min_width {
            (available_width - total_min_width) / (total_max_width - total_min_width)
        } else {
            0.0 // If min == max, just use min width
        };
        for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
            if table_ctx.collapsed_columns.contains(&col_idx) {
                col.computed_width = Some(0.0);
            } else {
                let interpolated = col.min_width + (col.max_width - col.min_width) * scale;
                col.computed_width = Some(interpolated);
            }
        }
    } else {
        // Case 3: Not enough space - scale down from min widths
        let scale = available_width / total_min_width;
        for (col_idx, col) in table_ctx.columns.iter_mut().enumerate() {
            if table_ctx.collapsed_columns.contains(&col_idx) {
                col.computed_width = Some(0.0);
            } else {
                col.computed_width = Some(col.min_width * scale);
            }
        }
    }

    Ok(())
}

/// Distribute a multi-column cell's width across the columns it spans
fn distribute_cell_width_across_columns(
    columns: &mut [TableColumnInfo],
    start_col: usize,
    colspan: usize,
    cell_min_width: f32,
    cell_max_width: f32,
    collapsed_columns: &std::collections::HashSet<usize>,
) {
    let end_col = start_col + colspan;
    if end_col > columns.len() {
        return;
    }

    // Calculate current total of spanned non-collapsed columns
    let current_min_total: f32 = columns[start_col..end_col]
        .iter()
        .enumerate()
        .filter(|(idx, _)| !collapsed_columns.contains(&(start_col + idx)))
        .map(|(_, c)| c.min_width)
        .sum();
    let current_max_total: f32 = columns[start_col..end_col]
        .iter()
        .enumerate()
        .filter(|(idx, _)| !collapsed_columns.contains(&(start_col + idx)))
        .map(|(_, c)| c.max_width)
        .sum();

    // Count non-collapsed columns in the span
    let num_visible_cols = (start_col..end_col)
        .filter(|idx| !collapsed_columns.contains(idx))
        .count();

    if num_visible_cols == 0 {
        return; // All spanned columns are collapsed
    }

    // Only distribute if the cell needs more space than currently available
    if cell_min_width > current_min_total {
        let extra_min = cell_min_width - current_min_total;
        let per_col = extra_min / num_visible_cols as f32;
        for (idx, col) in columns[start_col..end_col].iter_mut().enumerate() {
            if !collapsed_columns.contains(&(start_col + idx)) {
                col.min_width += per_col;
            }
        }
    }

    if cell_max_width > current_max_total {
        let extra_max = cell_max_width - current_max_total;
        let per_col = extra_max / num_visible_cols as f32;
        for (idx, col) in columns[start_col..end_col].iter_mut().enumerate() {
            if !collapsed_columns.contains(&(start_col + idx)) {
                col.max_width += per_col;
            }
        }
    }
}

/// Layout a cell with its computed column width to determine its content height
fn layout_cell_for_height<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    cell_index: usize,
    cell_width: f32,
    constraints: &LayoutConstraints,
) -> Result<f32> {
    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let cell_dom_id = cell_node.dom_node_id.ok_or(LayoutError::InvalidTree)?;

    // Check if cell has text content directly in DOM (not in LayoutTree)
    // Text nodes are intentionally not included in LayoutTree per CSS spec,
    // but we need to measure them for table cell height calculation.
    let has_text_children = cell_dom_id
        .az_children(&ctx.styled_dom.node_hierarchy.as_container())
        .any(|child_id| {
            let node_data = &ctx.styled_dom.node_data.as_container()[child_id];
            matches!(node_data.get_node_type(), NodeType::Text(_))
        });

    debug_table_layout!(
        ctx,
        "layout_cell_for_height: cell_index={}, has_text_children={}",
        cell_index,
        has_text_children
    );

    // Get padding and border to calculate content width
    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let padding = &cell_node.box_props.padding;
    let border = &cell_node.box_props.border;
    let writing_mode = constraints.writing_mode;

    // cell_width is the border-box width (includes padding/border from column
    // width calculation) but layout functions need content-box width
    let content_width = cell_width
        - padding.cross_start(writing_mode)
        - padding.cross_end(writing_mode)
        - border.cross_start(writing_mode)
        - border.cross_end(writing_mode);

    debug_table_layout!(
        ctx,
        "Cell width: border_box={:.2}, content_box={:.2}",
        cell_width,
        content_width
    );

    let content_height = if has_text_children {
        // Cell contains text - use IFC to measure it
        debug_table_layout!(ctx, "Using IFC to measure text content");

        let cell_constraints = LayoutConstraints {
            available_size: LogicalSize {
                width: content_width, // Use content width, not border-box width
                height: f32::INFINITY,
            },
            writing_mode: constraints.writing_mode,
            writing_mode_ctx: constraints.writing_mode_ctx,
            bfc_state: None,
            text_align: constraints.text_align,
            containing_block_size: constraints.containing_block_size,
            // Use definite width for final cell layout!
            // This replaces any previous MinContent/MaxContent measurement.
            available_width_type: Text3AvailableSpace::Definite(content_width),
        };

        let output = layout_ifc(ctx, text_cache, tree, cell_index, &cell_constraints)?;

        debug_table_layout!(
            ctx,
            "IFC returned height={:.2}",
            output.overflow_size.height
        );

        output.overflow_size.height
    } else {
        // Cell contains block-level children or is empty - use regular layout
        debug_table_layout!(ctx, "Using regular layout for block children");

        let cell_constraints = LayoutConstraints {
            available_size: LogicalSize {
                width: content_width, // Use content width, not border-box width
                height: f32::INFINITY,
            },
            writing_mode: constraints.writing_mode,
            writing_mode_ctx: constraints.writing_mode_ctx,
            bfc_state: None,
            text_align: constraints.text_align,
            containing_block_size: constraints.containing_block_size,
            // Use Definite width for final cell layout!
            available_width_type: Text3AvailableSpace::Definite(content_width),
        };

        let mut temp_positions: super::PositionVec = Vec::new();
        let mut temp_scrollbar_reflow = false;
        let mut temp_float_cache = HashMap::new();

        crate::solver3::cache::calculate_layout_for_subtree(
            ctx,
            tree,
            text_cache,
            cell_index,
            LogicalPosition::zero(),
            cell_constraints.available_size,
            &mut temp_positions,
            &mut temp_scrollbar_reflow,
            &mut temp_float_cache,
            // PerformLayout: final table cell layout with definite width
            crate::solver3::cache::ComputeMode::PerformLayout,
        )?;

        let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
        cell_node.used_size.unwrap_or_default().height
    };

    // Add padding and border to get the total height
    let cell_node = tree.get(cell_index).ok_or(LayoutError::InvalidTree)?;
    let padding = &cell_node.box_props.padding;
    let border = &cell_node.box_props.border;
    let writing_mode = constraints.writing_mode;

    let total_height = content_height
        + padding.main_start(writing_mode)
        + padding.main_end(writing_mode)
        + border.main_start(writing_mode)
        + border.main_end(writing_mode);

    debug_table_layout!(
        ctx,
        "Cell total height: cell_index={}, content={:.2}, padding/border={:.2}, total={:.2}",
        cell_index,
        content_height,
        padding.main_start(writing_mode)
            + padding.main_end(writing_mode)
            + border.main_start(writing_mode)
            + border.main_end(writing_mode),
        total_height
    );

    Ok(total_height)
}

// or bottom of content edge if no such line box exists
// +spec:box-model:b64fa0 - Cell baseline is first in-flow line box or bottom of content edge
// +spec:overflow:3fa86f - Table cell baseline: first in-flow line box or bottom of content edge; scrolling boxes treated as at origin
fn compute_cell_baseline(cell_index: usize, tree: &LayoutTree) -> f32 {
    let Some(cell_node) = tree.nodes.get(cell_index) else {
        return 0.0;
    };

    // +spec:inline-formatting-context:27be38 - cell baseline is first in-flow line box or bottom of content edge
    // Check if the cell has inline layout (first in-flow line box)
    if let Some(ref cached_layout) = cell_node.inline_layout_result {
        let inline_result = &cached_layout.layout;
        // The baseline is the ascent of the first item from the top of the cell
        if let Some(first_item) = inline_result.items.first() {
            let (item_ascent, _) = crate::text3::cache::get_item_vertical_metrics_approx(&first_item.item);
            let padding_top = cell_node.box_props.padding.top;
            let border_top = cell_node.box_props.border.top;
            return padding_top + border_top + first_item.position.y + item_ascent;
        }
    }

    // Check children for first in-flow line box
    let children = &cell_node.children;
    for &child_idx in children {
        if child_idx < tree.nodes.len() {
            let child_node = &tree.nodes[child_idx];
            if child_node.inline_layout_result.is_some() {
                let child_baseline = compute_cell_baseline(child_idx, tree);
                let padding_top = cell_node.box_props.padding.top;
                let border_top = cell_node.box_props.border.top;
                return padding_top + border_top + child_baseline;
            }
        }
    }

    // No line box found: baseline is the bottom of the content edge
    let used_size = cell_node.used_size.unwrap_or_default();
    let padding_bottom = cell_node.box_props.padding.bottom;
    let border_bottom = cell_node.box_props.border.bottom;
    used_size.height - padding_bottom - border_bottom
}

/// +spec:box-model:72b495 - Table row height = max of computed height and MIN required by cells; baseline alignment
// +spec:display-property:728144 - Table height algorithm: row heights from cell content, rowspan distribution, vertical-align in cells (top/middle/bottom/baseline, sub/super/text-top/text-bottom/length/percentage fall back to baseline), cell baseline computation, and horizontal alignment via text-align
/// Calculate row heights based on cell content after column widths are determined
// +spec:inline-formatting-context:87b90d - Table height algorithms: row height = max(computed height, cell heights, MIN); vertical-align in cells (baseline/top/middle/bottom, sub/super/etc. fall back to baseline)
fn calculate_row_heights<T: ParsedFontTrait>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree,
    text_cache: &mut crate::font_traits::TextLayoutCache,
    ctx: &mut LayoutContext<'_, T>,
    constraints: &LayoutConstraints,
) -> Result<()> {
    debug_table_layout!(
        ctx,
        "calculate_row_heights: num_rows={}, available_size={:?}",
        table_ctx.num_rows,
        constraints.available_size
    );

    // +spec:inline-formatting-context:a7c7a0 - row height = max of computed height, cell heights, and MIN; vertical-align per cell
    // Initialize row heights and baselines
    table_ctx.row_heights = vec![0.0; table_ctx.num_rows];
    table_ctx.row_baselines = vec![0.0; table_ctx.num_rows];

    // CSS 2.2 Section 17.6: Set collapsed rows to height 0
    for &row_idx in &table_ctx.collapsed_rows {
        if row_idx < table_ctx.row_heights.len() {
            table_ctx.row_heights[row_idx] = 0.0;
        }
    }

    // required by content; 'height' property can influence row height but does not
    // increase cell box height
    // First pass: Calculate heights for cells that don't span multiple rows
    for cell_info in &table_ctx.cells {
        // Skip cells in collapsed rows
        if table_ctx.collapsed_rows.contains(&cell_info.row) {
            continue;
        }

        // Get the cell's width (sum of column widths if colspan > 1)
        let mut cell_width = 0.0;
        for col_idx in cell_info.column..(cell_info.column + cell_info.colspan) {
            if let Some(col) = table_ctx.columns.get(col_idx) {
                if let Some(width) = col.computed_width {
                    cell_width += width;
                }
            }
        }

        debug_table_layout!(
            ctx,
            "Cell layout: node_index={}, row={}, col={}, width={:.2}",
            cell_info.node_index,
            cell_info.row,
            cell_info.column,
            cell_width
        );

        // Layout the cell to get its height
        let cell_height = layout_cell_for_height(
            ctx,
            tree,
            text_cache,
            cell_info.node_index,
            cell_width,
            constraints,
        )?;

        debug_table_layout!(
            ctx,
            "Cell height calculated: node_index={}, height={:.2}",
            cell_info.node_index,
            cell_height
        );

        //   row height = max of all single-span cell heights in the row
        if cell_info.rowspan == 1 {
            let current_height = table_ctx.row_heights[cell_info.row];
            table_ctx.row_heights[cell_info.row] = current_height.max(cell_height);
        }

        // +spec:box-model:073652 - Table height: baseline-aligned cells establish row baseline, then top/bottom/middle cells positioned
        // The baseline of a cell is the baseline of its first line box (from inline layout)
        // or the bottom of the content box if no inline content.
        if cell_info.rowspan == 1 {
            let cell_baseline = compute_cell_baseline(cell_info.node_index, tree);
            let current_baseline = table_ctx.row_baselines[cell_info.row];
            table_ctx.row_baselines[cell_info.row] = current_baseline.max(cell_baseline);
        }
    }

    // involved must be great enough to encompass the cell spanning the rows
    // Second pass: Handle cells that span multiple rows (rowspan > 1)
    for cell_info in &table_ctx.cells {
        // Skip cells that start in collapsed rows
        if table_ctx.collapsed_rows.contains(&cell_info.row) {
            continue;
        }

        if cell_info.rowspan > 1 {
            // Get the cell's width
            let mut cell_width = 0.0;
            for col_idx in cell_info.column..(cell_info.column + cell_info.colspan) {
                if let Some(col) = table_ctx.columns.get(col_idx) {
                    if let Some(width) = col.computed_width {
                        cell_width += width;
                    }
                }
            }

            // Layout the cell to get its height
            let cell_height = layout_cell_for_height(
                ctx,
                tree,
                text_cache,
                cell_info.node_index,
                cell_width,
                constraints,
            )?;

            // Calculate the current total height of spanned rows (excluding collapsed rows)
            let end_row = cell_info.row + cell_info.rowspan;
            let current_total: f32 = table_ctx.row_heights[cell_info.row..end_row]
                .iter()
                .enumerate()
                .filter(|(idx, _)| !table_ctx.collapsed_rows.contains(&(cell_info.row + idx)))
                .map(|(_, height)| height)
                .sum();

            // If the cell needs more height, distribute extra height across
            // non-collapsed spanned rows
            if cell_height > current_total {
                let extra_height = cell_height - current_total;

                // Count non-collapsed rows in span
                let non_collapsed_rows = (cell_info.row..end_row)
                    .filter(|row_idx| !table_ctx.collapsed_rows.contains(row_idx))
                    .count();

                if non_collapsed_rows > 0 {
                    let per_row = extra_height / non_collapsed_rows as f32;

                    for row_idx in cell_info.row..end_row {
                        if !table_ctx.collapsed_rows.contains(&row_idx) {
                            table_ctx.row_heights[row_idx] += per_row;
                        }
                    }
                }
            }
        }
    }

    // CSS 2.2 Section 17.6: Final pass - ensure collapsed rows have height 0
    for &row_idx in &table_ctx.collapsed_rows {
        if row_idx < table_ctx.row_heights.len() {
            table_ctx.row_heights[row_idx] = 0.0;
        }
    }

    //   visible content, the row has zero height and v-spacing on only one side
    // +spec:table-layout:7370dc - empty-cells:hide in separated borders model
    if table_ctx.border_collapse == StyleBorderCollapse::Separate {
        for row_idx in 0..table_ctx.num_rows {
            if table_ctx.collapsed_rows.contains(&row_idx) {
                continue;
            }
            // Collect cells in this row
            let row_cells: Vec<usize> = table_ctx
                .cells
                .iter()
                .filter(|c| c.row == row_idx && c.rowspan == 1)
                .map(|c| c.node_index)
                .collect();
            if row_cells.is_empty() {
                continue;
            }
            // +spec:box-model:0ab9b0 - empty-cells:hide suppresses borders/backgrounds, row gets zero height if all cells hidden+empty
            // Check if ALL cells in this row have empty-cells:hide and are empty
            let all_hidden_empty = row_cells.iter().all(|&cell_idx| {
                if let Some(cell_node) = tree.get(cell_idx) {
                    let ec = get_empty_cells_property(ctx, cell_node);
                    ec == StyleEmptyCells::Hide && is_cell_empty(tree, cell_idx)
                } else {
                    true
                }
            });
            if all_hidden_empty {
                table_ctx.row_heights[row_idx] = 0.0;
                table_ctx.hidden_empty_rows.insert(row_idx);
            }
        }
    }

    Ok(())
}

/// Position all cells in the table grid with calculated widths and heights
fn position_table_cells<T: ParsedFontTrait>(
    table_ctx: &mut TableLayoutContext,
    tree: &mut LayoutTree,
    ctx: &mut LayoutContext<'_, T>,
    table_index: usize,
    constraints: &LayoutConstraints,
) -> Result<BTreeMap<usize, LogicalPosition>> {
    debug_log!(ctx, "Positioning table cells in grid");

    let mut positions = BTreeMap::new();

    // +spec:box-model:54e86a - Separated borders model: individual cell borders, border-spacing between cells, empty-cells handling
    //   rows, columns, row groups, column groups cannot have borders (UA must ignore border props);
    //   row/column/rowgroup/colgroup backgrounds are invisible in border-spacing area (table bg shows through);
    //   distance from table edge to edge-cell border = table padding + border-spacing
    //   (table padding is already accounted for by the containing block; h_spacing is the border-spacing)
    // Get border spacing values if border-collapse is separate
    let (h_spacing, v_spacing) = if table_ctx.border_collapse == StyleBorderCollapse::Separate {
        let styled_dom = ctx.styled_dom;
        let table_id = tree.nodes[table_index].dom_node_id.unwrap();
        let table_state = &styled_dom.styled_nodes.as_container()[table_id].styled_node_state;

        let spacing_context = ResolutionContext {
            element_font_size: get_element_font_size(styled_dom, table_id, table_state),
            parent_font_size: get_parent_font_size(styled_dom, table_id, table_state),
            root_font_size: get_root_font_size(styled_dom, table_state),
            containing_block_size: PhysicalSize::new(0.0, 0.0),
            element_size: None,
            viewport_size: PhysicalSize::new(0.0, 0.0), // TODO: Get actual DPI scale from ctx
        };

        let h = table_ctx
            .border_spacing
            .horizontal
            .resolve_with_context(&spacing_context, PropertyContext::Other)
            .max(0.0);

        let v = table_ctx
            .border_spacing
            .vertical
            .resolve_with_context(&spacing_context, PropertyContext::Other)
            .max(0.0);

        (h, v)
    } else {
        (0.0, 0.0)
    };

    debug_log!(
        ctx,
        "Border spacing: h={:.2}, v={:.2}",
        h_spacing,
        v_spacing
    );

    // Calculate cumulative column positions (x-offsets) with spacing
    let mut col_positions = vec![0.0; table_ctx.columns.len()];
    let mut x_offset = h_spacing; // Start with spacing on the left
    for (i, col) in table_ctx.columns.iter().enumerate() {
        col_positions[i] = x_offset;
        if let Some(width) = col.computed_width {
            // Collapsed columns: gutters on either side collapse (width is 0, skip spacing)
            if table_ctx.collapsed_columns.contains(&i) {
                // No width, no gutter added
            } else {
                x_offset += width + h_spacing; // Add spacing between columns
            }
        }
    }

    // Calculate cumulative row positions (y-offsets) with spacing
    let mut row_positions = vec![0.0; table_ctx.num_rows];
    let mut y_offset = v_spacing; // Start with spacing on the top
    for (i, &height) in table_ctx.row_heights.iter().enumerate() {
        row_positions[i] = y_offset;
        // Collapsed rows: gutters on either side collapse (height is 0, skip spacing)
        if table_ctx.collapsed_rows.contains(&i) {
            // No height, no gutter added
        } else if table_ctx.hidden_empty_rows.contains(&i) {
            // Hidden-empty row: zero height, only one side of spacing
            // (we already added spacing before this row, so skip the spacing after)
            y_offset += height; // height is 0.0
        } else {
            y_offset += height + v_spacing; // Add spacing between rows
        }
    }

    // Position each cell
    for cell_info in &table_ctx.cells {
        let precomputed_cell_baseline = compute_cell_baseline(cell_info.node_index, tree);

        let cell_node = tree
            .get_mut(cell_info.node_index)
            .ok_or(LayoutError::InvalidTree)?;

        // Calculate cell position
        let x = col_positions.get(cell_info.column).copied().unwrap_or(0.0);
        let y = row_positions.get(cell_info.row).copied().unwrap_or(0.0);

        // Calculate cell size (sum of spanned columns/rows)
        let mut width = 0.0;
        debug_info!(
            ctx,
            "[position_table_cells] Cell {}: calculating width from cols {}..{}",
            cell_info.node_index,
            cell_info.column,
            cell_info.column + cell_info.colspan
        );
        for col_idx in cell_info.column..(cell_info.column + cell_info.colspan) {
            if let Some(col) = table_ctx.columns.get(col_idx) {
                debug_info!(
                    ctx,
                    "[position_table_cells]   Col {}: computed_width={:?}",
                    col_idx,
                    col.computed_width
                );
                if let Some(col_width) = col.computed_width {
                    width += col_width;
                    // Add spacing between spanned columns (but not after the last one)
                    if col_idx < cell_info.column + cell_info.colspan - 1 {
                        width += h_spacing;
                    }
                } else {
                    debug_info!(
                        ctx,
                        "[position_table_cells]   WARN:  Col {} has NO computed_width!",
                        col_idx
                    );
                }
            } else {
                debug_info!(
                    ctx,
                    "[position_table_cells]   WARN:  Col {} not found in table_ctx.columns!",
                    col_idx
                );
            }
        }

        let mut height = 0.0;
        let end_row = cell_info.row + cell_info.rowspan;
        for row_idx in cell_info.row..end_row {
            if let Some(&row_height) = table_ctx.row_heights.get(row_idx) {
                height += row_height;
                // Add spacing between spanned rows (but not after the last one)
                if row_idx < end_row - 1 {
                    height += v_spacing;
                }
            }
        }

        // Update cell's used size and position
        let writing_mode = constraints.writing_mode;
        // Table layout works in main/cross axes, must convert back to logical width/height

        debug_info!(
            ctx,
            "[position_table_cells] Cell {}: BEFORE from_main_cross: width={}, height={}, \
             writing_mode={:?}",
            cell_info.node_index,
            width,
            height,
            writing_mode
        );

        cell_node.used_size = Some(LogicalSize::from_main_cross(height, width, writing_mode));

        debug_info!(
            ctx,
            "[position_table_cells] Cell {}: AFTER from_main_cross: used_size={:?}",
            cell_info.node_index,
            cell_node.used_size
        );

        debug_info!(
            ctx,
            "[position_table_cells] Cell {}: setting used_size to {}x{} (row_heights={:?})",
            cell_info.node_index,
            width,
            height,
            table_ctx.row_heights
        );

        // +spec:inline-formatting-context:20e8e8 - table cell vertical-align alignment order (baseline first, then top, then bottom/middle)
        // receive extra top or bottom padding; vertical-align determines alignment
        // Apply vertical-align to cell content if it has inline layout
        if let Some(ref cached_layout) = cell_node.inline_layout_result {
            let inline_result = &cached_layout.layout;
            use StyleVerticalAlign;

            // Get vertical-align property from styled_dom
            let vertical_align = if let Some(dom_id) = cell_node.dom_node_id {
                let node_state = ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state.clone();

                match get_vertical_align_property(ctx.styled_dom, dom_id, &node_state) {
                    MultiValue::Exact(v) => v,
                    _ => StyleVerticalAlign::Baseline,
                }
            } else {
                StyleVerticalAlign::Baseline
            };

            // Calculate content height from inline layout bounds
            let content_bounds = inline_result.bounds();
            let content_height = content_bounds.height;

            // Get padding and border to calculate content-box height
            // height is border-box, but vertical alignment should be within content-box
            let padding = &cell_node.box_props.padding;
            let border = &cell_node.box_props.border;
            let content_box_height = height
                - padding.main_start(writing_mode)
                - padding.main_end(writing_mode)
                - border.main_start(writing_mode)
                - border.main_end(writing_mode);

            // top: top of cell box aligned with top of first row it spans
            // bottom: bottom of cell box aligned with bottom of last row it spans
            // middle: center of cell aligned with center of rows it spans
            //   the cell is aligned at the baseline instead
            let y_offset = match vertical_align {
                StyleVerticalAlign::Top => 0.0,
                StyleVerticalAlign::Middle => (content_box_height - content_height) * 0.5,
                StyleVerticalAlign::Bottom => content_box_height - content_height,
                // align with the row baseline. cell_baseline = distance from top of cell box
                // to cell's baseline; row_baseline = distance from top of row to row's baseline
                StyleVerticalAlign::Baseline
                | StyleVerticalAlign::Sub
                | StyleVerticalAlign::Superscript
                | StyleVerticalAlign::TextTop
                | StyleVerticalAlign::TextBottom
                | StyleVerticalAlign::Percentage(_)
                | StyleVerticalAlign::Length(_) => {
                    let row_baseline = table_ctx.row_baselines.get(cell_info.row).copied().unwrap_or(0.0);
                    (row_baseline - precomputed_cell_baseline).max(0.0)
                }
            };

            debug_info!(
                ctx,
                "[position_table_cells] Cell {}: vertical-align={:?}, border_box_height={}, \
                 content_box_height={}, content_height={}, y_offset={}",
                cell_info.node_index,
                vertical_align,
                height,
                content_box_height,
                content_height,
                y_offset
            );

            // Create new layout with adjusted positions
            if y_offset.abs() > 0.01 {
                // Only adjust if offset is significant
                use std::sync::Arc;

                use crate::text3::cache::{PositionedItem, UnifiedLayout};

                let adjusted_items: Vec<PositionedItem> = inline_result
                    .items
                    .iter()
                    .map(|item| PositionedItem {
                        item: item.item.clone(),
                        position: crate::text3::cache::Point {
                            x: item.position.x,
                            y: item.position.y + y_offset,
                        },
                        line_index: item.line_index,
                    })
                    .collect();

                let adjusted_layout = UnifiedLayout {
                    items: adjusted_items,
                    overflow: inline_result.overflow.clone(),
                };

                // Keep the same constraint type from the cached layout
                cell_node.inline_layout_result = Some(CachedInlineLayout::new(
                    Arc::new(adjusted_layout),
                    cached_layout.available_width,
                    cached_layout.has_floats,
                ));
            }
        }

        // Store position relative to table origin
        let position = LogicalPosition::from_main_cross(y, x, writing_mode);

        // Insert position into map so cache module can position the cell
        positions.insert(cell_info.node_index, position);

        debug_log!(
            ctx,
            "Cell at row={}, col={}: pos=({:.2}, {:.2}), size=({:.2}x{:.2})",
            cell_info.row,
            cell_info.column,
            x,
            y,
            width,
            height
        );
    }

    Ok(positions)
}

/// Gathers all inline content for `text3`, recursively laying out `inline-block` children
/// to determine their size and baseline before passing them to the text engine.
///
/// This function also assigns IFC membership to all participating nodes:
/// - The IFC root gets an `ifc_id` assigned
/// - Each text/inline child gets `ifc_membership` set with a reference back to the IFC root
///
/// This mapping enables efficient cursor hit-testing: when a text node is clicked,
/// we can find its parent IFC's `inline_layout_result` via `ifc_membership.ifc_root_layout_index`.
// +spec:display-property:63a38b - inline box boundaries and out-of-flow elements are ignored for text adjacency (white space, line-breaking, text-transform)
fn collect_and_measure_inline_content<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    text_cache: &mut TextLayoutCache,
    tree: &mut LayoutTree,
    ifc_root_index: usize,
    constraints: &LayoutConstraints,
) -> Result<(Vec<InlineContent>, HashMap<ContentIndex, usize>)> {
    use crate::solver3::layout_tree::{IfcId, IfcMembership};
    use crate::text3::cache::InlineContent;

    let result = collect_and_measure_inline_content_impl(ctx, text_cache, tree, ifc_root_index, constraints)?;
    Ok(result)
}

fn collect_and_measure_inline_content_impl<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    text_cache: &mut TextLayoutCache,
    tree: &mut LayoutTree,
    ifc_root_index: usize,
    constraints: &LayoutConstraints,
) -> Result<(Vec<InlineContent>, HashMap<ContentIndex, usize>)> {
    use crate::solver3::layout_tree::{IfcId, IfcMembership};

    debug_ifc_layout!(
        ctx,
        "collect_and_measure_inline_content: node_index={}",
        ifc_root_index
    );

    // Generate a unique IFC ID for this inline formatting context
    let ifc_id = IfcId::unique();

    // Store IFC ID on the IFC root node
    if let Some(ifc_root_node) = tree.get_mut(ifc_root_index) {
        ifc_root_node.ifc_id = Some(ifc_id);
    }

    let mut content = Vec::new();
    // Maps the `ContentIndex` used by text3 back to the `LayoutNode` index.
    let mut child_map = HashMap::new();
    // Track the current run index for IFC membership assignment
    let mut current_run_index: u32 = 0;

    let ifc_root_node = tree.get(ifc_root_index).ok_or(LayoutError::InvalidTree)?;

    // Check if this is an anonymous IFC wrapper (has no DOM ID)
    let is_anonymous = ifc_root_node.dom_node_id.is_none();

    // Get the DOM node ID of the IFC root, or find it from parent/children for anonymous boxes
    // CSS 2.2 § 9.2.1.1: Anonymous boxes inherit properties from their enclosing box
    let ifc_root_dom_id = match ifc_root_node.dom_node_id {
        Some(id) => id,
        None => {
            // Anonymous box - get DOM ID from parent or first child with DOM ID
            let parent_dom_id = ifc_root_node
                .parent
                .and_then(|p| tree.get(p))
                .and_then(|n| n.dom_node_id);

            if let Some(id) = parent_dom_id {
                id
            } else {
                // Try to find DOM ID from first child
                match tree.children(ifc_root_index)
                    .iter()
                    .filter_map(|&child_idx| tree.get(child_idx))
                    .filter_map(|n| n.dom_node_id)
                    .next()
                {
                    Some(id) => id,
                    None => {
                        debug_warning!(ctx, "IFC root and all ancestors/children have no DOM ID");
                        return Ok((content, child_map));
                    }
                }
            }
        }
    };

    // Collect children to avoid holding an immutable borrow during iteration
    let children: Vec<_> = tree.children(ifc_root_index).to_vec();
    drop(ifc_root_node);

    debug_ifc_layout!(
        ctx,
        "Node {} has {} layout children, is_anonymous={}",
        ifc_root_index,
        children.len(),
        is_anonymous
    );

    // For anonymous IFC wrappers, we collect content from layout tree children
    // For regular IFC roots, we also check DOM children for text nodes
    if is_anonymous {
        // Anonymous IFC wrapper - iterate over layout tree children and collect their content
        for (item_idx, &child_index) in children.iter().enumerate() {
            let content_index = ContentIndex {
                run_index: ifc_root_index as u32,
                item_index: item_idx as u32,
            };

            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let Some(dom_id) = child_node.dom_node_id else {
                debug_warning!(
                    ctx,
                    "Anonymous IFC child at index {} has no DOM ID",
                    child_index
                );
                continue;
            };

            let node_data = &ctx.styled_dom.node_data.as_container()[dom_id];

            // Check if this is a text node
            if let NodeType::Text(ref text_content) = node_data.get_node_type() {
                debug_info!(
                    ctx,
                    "[collect_and_measure_inline_content] OK: Found text node (DOM {:?}) in anonymous wrapper: '{}'",
                    dom_id,
                    text_content.as_str()
                );
                // Get style from the TEXT NODE itself (dom_id), not the IFC root
                // This ensures inline styles like color: #666666 are applied to the text
                let style = Arc::new(get_style_properties(ctx.styled_dom, dom_id, ctx.system_style.as_ref()));
                let text_items = split_text_for_whitespace(
                    ctx.styled_dom,
                    dom_id,
                    text_content.as_str(),
                    style,
                );
                content.extend(text_items);
                child_map.insert(content_index, child_index);
                
                // Set IFC membership on the text node - drop child_node borrow first
                drop(child_node);
                if let Some(child_node_mut) = tree.get_mut(child_index) {
                    child_node_mut.ifc_membership = Some(IfcMembership {
                        ifc_id,
                        ifc_root_layout_index: ifc_root_index,
                        run_index: current_run_index,
                    });
                }
                current_run_index += 1;
                
                continue;
            }

            // Non-text inline child - add as shape for inline-block
            let display = get_display_property(ctx.styled_dom, Some(dom_id)).unwrap_or_default();

            if display != LayoutDisplay::Inline {
                // +spec:display-property:a37a9a - atomic inline-level boxes treated as neutral characters in bidi reordering
                // This is an atomic inline-level box (e.g., inline-block, image).
                // We must determine its size and baseline before passing it to text3.

                // The intrinsic sizing pass has already calculated its preferred size.
                let intrinsic_size = child_node.intrinsic_sizes.clone().unwrap_or_default();
                let box_props = child_node.box_props.clone();

                let styled_node_state = ctx
                    .styled_dom
                    .styled_nodes
                    .as_container()
                    .get(dom_id)
                    .map(|n| n.styled_node_state.clone())
                    .unwrap_or_default();

                // Calculate tentative border-box size based on CSS properties
                let tentative_size = crate::solver3::sizing::calculate_used_size_for_node(
                    ctx.styled_dom,
                    Some(dom_id),
                    constraints.containing_block_size,
                    intrinsic_size,
                    &box_props,
                    ctx.viewport_size,
                )?;

                let writing_mode = get_writing_mode(ctx.styled_dom, dom_id, &styled_node_state)
                    .unwrap_or_default();

                // Determine content-box size for laying out children
                let content_box_size = box_props.inner_size(tentative_size, writing_mode);

                // To find its height and baseline, we must lay out its contents.
                let child_wm_ctx = super::geometry::WritingModeContext {
                    writing_mode,
                    direction: get_direction_property(ctx.styled_dom, dom_id, &styled_node_state)
                        .unwrap_or_default(),
                    text_orientation: get_text_orientation_property(ctx.styled_dom, dom_id, &styled_node_state)
                        .unwrap_or_default(),
                };
                let child_constraints = LayoutConstraints {
                    available_size: LogicalSize::new(content_box_size.width, f32::INFINITY),
                    writing_mode,
                    writing_mode_ctx: child_wm_ctx,
                    bfc_state: None,
                    text_align: TextAlign::Start,
                    containing_block_size: constraints.containing_block_size,
                    available_width_type: Text3AvailableSpace::Definite(content_box_size.width),
                };

                // Drop the immutable borrow before calling layout_formatting_context
                drop(child_node);

                // Recursively lay out the inline-block to get its final height and baseline.
                let mut empty_float_cache = HashMap::new();
                let layout_result = layout_formatting_context(
                    ctx,
                    tree,
                    text_cache,
                    child_index,
                    &child_constraints,
                    &mut empty_float_cache,
                )?;

                let css_height = get_css_height(ctx.styled_dom, dom_id, &styled_node_state);

                // Determine final border-box height
                let final_height = match css_height.unwrap_or_default() {
                    LayoutHeight::Auto => {
                        let content_height = layout_result.output.overflow_size.height;
                        content_height
                            + box_props.padding.main_sum(writing_mode)
                            + box_props.border.main_sum(writing_mode)
                    }
                    _ => tentative_size.height,
                };

                let final_size = LogicalSize::new(tentative_size.width, final_height);

                // Update the node in the tree with its now-known used size.
                tree.get_mut(child_index).unwrap().used_size = Some(final_size);

                // +spec:inline-formatting-context:fcc686 - synthesize baseline from margin box for atomic inlines with no baseline
                let baseline_offset = layout_result.output.baseline.unwrap_or(final_height);

                // Get margins for inline-block positioning in the inline flow
                // The margin-box size is used so text3 positions inline-blocks with proper spacing
                let margin = &box_props.margin;
                let margin_box_width = final_size.width + margin.left + margin.right;
                let margin_box_height = final_size.height + margin.top + margin.bottom;

                // For inline-block shapes, text3 uses the content array index as run_index
                // and always item_index=0 for objects. We must match this when inserting into child_map.
                let shape_content_index = ContentIndex {
                    run_index: content.len() as u32,
                    item_index: 0,
                };
                content.push(InlineContent::Shape(InlineShape {
                    shape_def: ShapeDefinition::Rectangle {
                        size: crate::text3::cache::Size {
                            // Use margin-box size for positioning in inline flow
                            width: margin_box_width,
                            height: margin_box_height,
                        },
                        corner_radius: None,
                    },
                    fill: None,
                    stroke: None,
                    // Adjust baseline offset by top margin
                    baseline_offset: baseline_offset + margin.top,
                    alignment: crate::solver3::getters::get_vertical_align_for_node(ctx.styled_dom, dom_id),
                    source_node_id: Some(dom_id),
                }));
                child_map.insert(shape_content_index, child_index);
            } else {
                // Regular inline element - collect its text children
                let span_style = get_style_properties(ctx.styled_dom, dom_id, ctx.system_style.as_ref());
                collect_inline_span_recursive(
                    ctx,
                    tree,
                    dom_id,
                    span_style,
                    &mut content,
                    &mut child_map,
                    &children,
                    constraints,
                )?;
            }
        }

        return Ok((content, child_map));
    }

    // Regular (non-anonymous) IFC root - check for list markers and use DOM traversal

    // Check if this IFC root OR its parent is a list-item and needs a marker
    // Case 1: IFC root itself is list-item (e.g., <li> with display: list-item)
    // Case 2: IFC root's parent is list-item (e.g., <li><text>...</text></li>)
    let ifc_root_node = tree.get(ifc_root_index).ok_or(LayoutError::InvalidTree)?;
    let mut list_item_dom_id: Option<NodeId> = None;

    // Check IFC root itself
    if let Some(dom_id) = ifc_root_node.dom_node_id {
        use crate::solver3::getters::get_display_property;
        if let MultiValue::Exact(display) = get_display_property(ctx.styled_dom, Some(dom_id)) {
            use LayoutDisplay;
            if display == LayoutDisplay::ListItem {
                debug_ifc_layout!(ctx, "IFC root NodeId({:?}) is list-item", dom_id);
                list_item_dom_id = Some(dom_id);
            }
        }
    }

    // Check IFC root's parent
    if list_item_dom_id.is_none() {
        if let Some(parent_idx) = ifc_root_node.parent {
            if let Some(parent_node) = tree.get(parent_idx) {
                if let Some(parent_dom_id) = parent_node.dom_node_id {
                    use crate::solver3::getters::get_display_property;
                    if let MultiValue::Exact(display) = get_display_property(ctx.styled_dom, Some(parent_dom_id)) {
                        use LayoutDisplay;
                        if display == LayoutDisplay::ListItem {
                            debug_ifc_layout!(
                                ctx,
                                "IFC root parent NodeId({:?}) is list-item",
                                parent_dom_id
                            );
                            list_item_dom_id = Some(parent_dom_id);
                        }
                    }
                }
            }
        }
    }

    // If we found a list-item, generate markers
    if let Some(list_dom_id) = list_item_dom_id {
        debug_ifc_layout!(
            ctx,
            "Found list-item (NodeId({:?})), generating marker",
            list_dom_id
        );

        // Find the layout node index for the list-item DOM node
        let list_item_layout_idx = tree
            .nodes
            .iter()
            .enumerate()
            .find(|(_, node)| {
                node.dom_node_id == Some(list_dom_id) && node.pseudo_element.is_none()
            })
            .map(|(idx, _)| idx);

        if let Some(list_idx) = list_item_layout_idx {
            // Per CSS spec, the ::marker pseudo-element is the first child of the list-item
            // Find the ::marker pseudo-element in the list-item's children
            let marker_idx = tree.children(list_idx)
                .iter()
                .find(|&&child_idx| {
                    tree.get(child_idx)
                        .map(|child| child.pseudo_element == Some(PseudoElement::Marker))
                        .unwrap_or(false)
                })
                .copied();

            if let Some(marker_idx) = marker_idx {
                debug_ifc_layout!(ctx, "Found ::marker pseudo-element at index {}", marker_idx);

                // Get the DOM ID for style resolution (marker references the same DOM node as
                // list-item)
                let list_dom_id_for_style = tree
                    .get(marker_idx)
                    .and_then(|n| n.dom_node_id)
                    .unwrap_or(list_dom_id);

                // Get list-style-position to determine marker positioning
                // Default is 'outside' per CSS Lists Module Level 3

                let list_style_position =
                    get_list_style_position(ctx.styled_dom, Some(list_dom_id));
                let position_outside =
                    matches!(list_style_position, StyleListStylePosition::Outside);

                debug_ifc_layout!(
                    ctx,
                    "List marker list-style-position: {:?} (outside={})",
                    list_style_position,
                    position_outside
                );

                // Generate marker text segments - font fallback happens during shaping
                let base_style =
                    Arc::new(get_style_properties(ctx.styled_dom, list_dom_id_for_style, ctx.system_style.as_ref()));
                let marker_segments = generate_list_marker_segments(
                    tree,
                    ctx.styled_dom,
                    marker_idx, // Pass the marker index, not the list-item index
                    ctx.counters,
                    base_style,
                    ctx.debug_messages,
                );

                debug_ifc_layout!(
                    ctx,
                    "Generated {} list marker segments",
                    marker_segments.len()
                );

                // Add markers as InlineContent::Marker with position information
                // Outside markers will be positioned in the padding gutter by the layout engine
                for segment in marker_segments {
                    content.push(InlineContent::Marker {
                        run: segment,
                        position_outside,
                    });
                }
            } else {
                debug_ifc_layout!(
                    ctx,
                    "WARNING: List-item at index {} has no ::marker pseudo-element",
                    list_idx
                );
            }
        }
    }

    drop(ifc_root_node);

    // IMPORTANT: We need to traverse the DOM, not just the layout tree!
    //
    // According to CSS spec, a block container with inline-level children establishes
    // an IFC and should collect ALL inline content, including text nodes.
    // Text nodes exist in the DOM but might not have their own layout tree nodes.

    // Debug: Check what the node_hierarchy says about this node
    let node_hier_item = &ctx.styled_dom.node_hierarchy.as_container()[ifc_root_dom_id];
    debug_info!(
        ctx,
        "[collect_and_measure_inline_content] DEBUG: node_hier_item.first_child={:?}, \
         last_child={:?}",
        node_hier_item.first_child_id(ifc_root_dom_id),
        node_hier_item.last_child_id()
    );

    let dom_children: Vec<NodeId> = ifc_root_dom_id
        .az_children(&ctx.styled_dom.node_hierarchy.as_container())
        .collect();

    let ifc_root_node_data = &ctx.styled_dom.node_data.as_container()[ifc_root_dom_id];

    // SPECIAL CASE: If the IFC root itself is a text node (leaf node),
    // add its text content directly instead of iterating over children
    if let NodeType::Text(ref text_content) = ifc_root_node_data.get_node_type() {
        let style = Arc::new(get_style_properties(ctx.styled_dom, ifc_root_dom_id, ctx.system_style.as_ref()));
        let text_items = split_text_for_whitespace(
            ctx.styled_dom,
            ifc_root_dom_id,
            text_content.as_str(),
            style,
        );
        content.extend(text_items);
        return Ok((content, child_map));
    }

    let ifc_root_node_type = match ifc_root_node_data.get_node_type() {
        NodeType::Div => "Div",
        NodeType::Text(_) => "Text",
        NodeType::Body => "Body",
        _ => "Other",
    };

    debug_info!(
        ctx,
        "[collect_and_measure_inline_content] IFC root has {} DOM children",
        dom_children.len()
    );

    for (item_idx, &dom_child_id) in dom_children.iter().enumerate() {
        let content_index = ContentIndex {
            run_index: ifc_root_index as u32,
            item_index: item_idx as u32,
        };

        let node_data = &ctx.styled_dom.node_data.as_container()[dom_child_id];

        // Check if this is a text node
        if let NodeType::Text(ref text_content) = node_data.get_node_type() {
            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] OK: Found text node (DOM child {:?}): '{}'",
                dom_child_id,
                text_content.as_str()
            );
            
            // Get style from the TEXT NODE itself (dom_child_id), not the IFC root
            // This ensures inline styles like color: #666666 are applied to the text
            // Uses split_text_for_whitespace to correctly handle white-space: pre with \n
            let style = Arc::new(get_style_properties(ctx.styled_dom, dom_child_id, ctx.system_style.as_ref()));
            let text_items = split_text_for_whitespace(
                ctx.styled_dom,
                dom_child_id,
                text_content.as_str(),
                style,
            );
            content.extend(text_items);
            
            // Set IFC membership on the text node's layout node (if it exists)
            // Text nodes may or may not have their own layout tree entry depending on
            // whether they're wrapped in an anonymous IFC wrapper
            if let Some(&layout_idx) = tree.dom_to_layout.get(&dom_child_id).and_then(|v| v.first()) {
                if let Some(text_layout_node) = tree.get_mut(layout_idx) {
                    text_layout_node.ifc_membership = Some(IfcMembership {
                        ifc_id,
                        ifc_root_layout_index: ifc_root_index,
                        run_index: current_run_index,
                    });
                }
            }
            current_run_index += 1;
            
            continue;
        }

        // For non-text nodes, find their corresponding layout tree node
        let child_index = children
            .iter()
            .find(|&&idx| {
                tree.get(idx)
                    .and_then(|n| n.dom_node_id)
                    .map(|id| id == dom_child_id)
                    .unwrap_or(false)
            })
            .copied();

        let Some(child_index) = child_index else {
            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] WARN: DOM child {:?} has no layout node",
                dom_child_id
            );
            continue;
        };

        let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        // At this point we have a non-text DOM child with a layout node
        let dom_id = child_node.dom_node_id.unwrap();

        let display = get_display_property(ctx.styled_dom, Some(dom_id)).unwrap_or_default();
        if display != LayoutDisplay::Inline {
            // This is an atomic inline-level box (e.g., inline-block, image).
            // We must determine its size and baseline before passing it to text3.

            // The intrinsic sizing pass has already calculated its preferred size.
            let intrinsic_size = child_node.intrinsic_sizes.clone().unwrap_or_default();
            let box_props = child_node.box_props.clone();

            let styled_node_state = ctx
                .styled_dom
                .styled_nodes
                .as_container()
                .get(dom_id)
                .map(|n| n.styled_node_state.clone())
                .unwrap_or_default();

            // Calculate tentative border-box size based on CSS properties
            // This correctly handles explicit width/height, box-sizing, and constraints
            let tentative_size = crate::solver3::sizing::calculate_used_size_for_node(
                ctx.styled_dom,
                Some(dom_id),
                constraints.containing_block_size,
                intrinsic_size,
                &box_props,
                ctx.viewport_size,
            )?;

            let writing_mode =
                get_writing_mode(ctx.styled_dom, dom_id, &styled_node_state).unwrap_or_default();

            // Determine content-box size for laying out children
            let content_box_size = box_props.inner_size(tentative_size, writing_mode);

            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] Inline-block NodeId({:?}): \
                 tentative_border_box={:?}, content_box={:?}",
                dom_id,
                tentative_size,
                content_box_size
            );

            // To find its height and baseline, we must lay out its contents.
            let child_wm_ctx = super::geometry::WritingModeContext {
                writing_mode,
                direction: get_direction_property(ctx.styled_dom, dom_id, &styled_node_state)
                    .unwrap_or_default(),
                text_orientation: get_text_orientation_property(ctx.styled_dom, dom_id, &styled_node_state)
                    .unwrap_or_default(),
            };
            let child_constraints = LayoutConstraints {
                available_size: LogicalSize::new(content_box_size.width, f32::INFINITY),
                writing_mode,
                writing_mode_ctx: child_wm_ctx,
                // Inline-blocks establish a new BFC, so no state is passed in.
                bfc_state: None,
                // Does not affect size/baseline of the container.
                text_align: TextAlign::Start,
                containing_block_size: constraints.containing_block_size,
                available_width_type: Text3AvailableSpace::Definite(content_box_size.width),
            };

            // Drop the immutable borrow before calling layout_formatting_context
            drop(child_node);

            // Recursively lay out the inline-block to get its final height and baseline.
            // Note: This does not affect its final position, only its dimensions.
            let mut empty_float_cache = HashMap::new();
            let layout_result = layout_formatting_context(
                ctx,
                tree,
                text_cache,
                child_index,
                &child_constraints,
                &mut empty_float_cache,
            )?;

            let css_height = get_css_height(ctx.styled_dom, dom_id, &styled_node_state);

            // Determine final border-box height
            let final_height = match css_height.clone().unwrap_or_default() {
                LayoutHeight::Auto => {
                    // For auto height, add padding and border to the content height
                    let content_height = layout_result.output.overflow_size.height;
                    content_height
                        + box_props.padding.main_sum(writing_mode)
                        + box_props.border.main_sum(writing_mode)
                }
                // For explicit height, calculate_used_size_for_node already gave us the correct border-box height
                _ => tentative_size.height,
            };

            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] Inline-block NodeId({:?}): \
                 layout_content_height={}, css_height={:?}, final_border_box_height={}",
                dom_id,
                layout_result.output.overflow_size.height,
                css_height,
                final_height
            );

            let final_size = LogicalSize::new(tentative_size.width, final_height);

            // Update the node in the tree with its now-known used size.
            tree.get_mut(child_index).unwrap().used_size = Some(final_size);

            // align the bottom margin edge with the parent's baseline"
            // +spec:display-property:d8e10d - atomic inline baseline synthesis (alphabetic at under margin edge)
            // CSS 2.2 § 10.8.1: For inline-block elements, the baseline is the baseline of the
            // last line box in the normal flow, unless it has no in-flow line boxes, in which
            // case the baseline is the bottom margin edge.
            //
            // `layout_result.output.baseline` returns the Y-position of the baseline measured
            // from the TOP of the content box. But `get_item_vertical_metrics` expects
            // `baseline_offset` to be the distance from the BOTTOM to the baseline.
            //
            // Conversion: baseline_offset_from_bottom = height - baseline_from_top
            //
            // +spec:inline-block:0201e4 - synthesize baseline at bottom margin edge for atomic inlines without content-derived baseline
            // +spec:inline-block:e3044b - synthesize baseline at bottom margin edge for atomic inlines without a baseline
            // If no baseline is found (e.g., the inline-block has no text), we fall back to
            // the bottom margin edge (baseline_offset = 0, meaning baseline at bottom).
            let baseline_from_top = layout_result.output.baseline;
            let baseline_offset = match baseline_from_top {
                Some(baseline_y) => {
                    // baseline_y is measured from top of content box
                    // We need to add padding and border to get the position within the border-box
                    let content_box_top = box_props.padding.top + box_props.border.top;
                    let baseline_from_border_box_top = baseline_y + content_box_top;
                    // Convert to distance from bottom
                    (final_height - baseline_from_border_box_top).max(0.0)
                }
                None => {
                    // No baseline found - use bottom margin edge (baseline at bottom)
                    0.0
                }
            };
            
            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] Inline-block NodeId({:?}): \
                 baseline_from_top={:?}, final_height={}, baseline_offset_from_bottom={}",
                dom_id,
                baseline_from_top,
                final_height,
                baseline_offset
            );

            // Get margins for inline-block positioning
            // For inline-blocks, we need to include margins in the shape size
            // so that text3 positions them correctly with spacing
            let margin = &box_props.margin;
            let margin_box_width = final_size.width + margin.left + margin.right;
            let margin_box_height = final_size.height + margin.top + margin.bottom;

            // For inline-block shapes, text3 uses the content array index as run_index
            // and always item_index=0 for objects. We must match this when inserting into child_map.
            let shape_content_index = ContentIndex {
                run_index: content.len() as u32,
                item_index: 0,
            };
            // the box used for alignment is the margin box" - using margin_box_width/height here
            content.push(InlineContent::Shape(InlineShape {
                shape_def: ShapeDefinition::Rectangle {
                    size: crate::text3::cache::Size {
                        // Use margin-box size for positioning in inline flow
                        width: margin_box_width,
                        height: margin_box_height,
                    },
                    corner_radius: None,
                },
                fill: None,
                stroke: None,
                // Adjust baseline offset by top margin
                baseline_offset: baseline_offset + margin.top,
                alignment: crate::solver3::getters::get_vertical_align_for_node(ctx.styled_dom, dom_id),
                source_node_id: Some(dom_id),
            }));
            child_map.insert(shape_content_index, child_index);
        } else if let NodeType::Image(image_ref) =
            ctx.styled_dom.node_data.as_container()[dom_id].get_node_type()
        {
            // +spec:replaced-elements:31a782 - replaced elements (img) not rendered purely by CSS box concepts
            // Images are replaced elements - they have intrinsic dimensions
            // and CSS width/height can constrain them
            
            // Re-get child_node since we dropped it earlier for the inline-block case
            let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let box_props = child_node.box_props.clone();

            // Get intrinsic size from the image data or fall back to layout node
            let intrinsic_size = child_node
                .intrinsic_sizes
                .clone()
                .unwrap_or(IntrinsicSizes {
                    max_content_width: 50.0,
                    max_content_height: 50.0,
                    ..Default::default()
                });
            
            // Get styled node state for CSS property lookup
            let styled_node_state = ctx
                .styled_dom
                .styled_nodes
                .as_container()
                .get(dom_id)
                .map(|n| n.styled_node_state.clone())
                .unwrap_or_default();
            
            // Calculate the used size respecting CSS width/height constraints
            let tentative_size = crate::solver3::sizing::calculate_used_size_for_node(
                ctx.styled_dom,
                Some(dom_id),
                constraints.containing_block_size,
                intrinsic_size.clone(),
                &box_props,
                ctx.viewport_size,
            )?;
            
            // Drop immutable borrow before mutable access
            drop(child_node);
            
            // Set the used_size on the layout node so paint_rect works correctly
            let final_size = LogicalSize::new(tentative_size.width, tentative_size.height);
            tree.get_mut(child_index).unwrap().used_size = Some(final_size);
            
            // Calculate display size for text3 (this is what text3 uses for positioning)
            let display_width = if final_size.width > 0.0 { 
                Some(final_size.width) 
            } else { 
                None 
            };
            let display_height = if final_size.height > 0.0 { 
                Some(final_size.height) 
            } else { 
                None 
            };
            
            content.push(InlineContent::Image(InlineImage {
                source: ImageSource::Ref(image_ref.clone()),
                intrinsic_size: crate::text3::cache::Size {
                    width: intrinsic_size.max_content_width,
                    height: intrinsic_size.max_content_height,
                },
                display_size: if display_width.is_some() || display_height.is_some() {
                    Some(crate::text3::cache::Size {
                        width: display_width.unwrap_or(intrinsic_size.max_content_width),
                        height: display_height.unwrap_or(intrinsic_size.max_content_height),
                    })
                } else {
                    None
                },
                // Images are bottom-aligned with the baseline by default
                baseline_offset: 0.0,
                alignment: crate::text3::cache::VerticalAlign::Baseline,
                object_fit: ObjectFit::Fill,
            }));
            // For images, text3 uses the content array index as run_index
            // and always item_index=0 for objects. We must match this.
            let image_content_index = ContentIndex {
                run_index: (content.len() - 1) as u32,  // -1 because we just pushed
                item_index: 0,
            };
            child_map.insert(image_content_index, child_index);
        } else {
            // This is a regular inline box (display: inline) - e.g., <span>, <em>, <strong>
            //
            // According to CSS Inline-3 spec §2, inline boxes are "transparent" wrappers
            // We must recursively collect their text children with inherited style
            debug_info!(
                ctx,
                "[collect_and_measure_inline_content] Found inline span (DOM {:?}), recursing",
                dom_id
            );

            let span_style = get_style_properties(ctx.styled_dom, dom_id, ctx.system_style.as_ref());
            collect_inline_span_recursive(
                ctx,
                tree,
                dom_id,
                span_style,
                &mut content,
                &mut child_map,
                &children,
                constraints,
            )?;
        }
    }
    Ok((content, child_map))
}

// +spec:display-property:c05c53 - inlinifying boxes can't contain block-level boxes; children are recursively inlinified
// it recursively inlinifies all of its in-flow children, so that no block-level descendants
// break up the inline formatting context in which it participates.
/// Recursively collects inline content from an inline span (display: inline) element.
///
/// According to CSS Inline Layout Module Level 3 §2:
///
/// "Inline boxes are transparent wrappers that wrap their content."
///
/// They don't create a new formatting context - their children participate in the
/// same IFC as the parent. This function processes:
///
/// - Text nodes: collected with the span's inherited style
/// - Nested inline spans: recursively descended
/// - Inline-blocks, images: measured and added as shapes
fn collect_inline_span_recursive<T: ParsedFontTrait>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &mut LayoutTree,
    span_dom_id: NodeId,
    span_style: StyleProperties,
    content: &mut Vec<InlineContent>,
    child_map: &mut HashMap<ContentIndex, usize>,
    parent_children: &[usize], // Layout tree children of parent IFC
    constraints: &LayoutConstraints,
) -> Result<()> {
    debug_info!(
        ctx,
        "[collect_inline_span_recursive] Processing inline span {:?}",
        span_dom_id
    );

    // Get DOM children of this span
    let span_dom_children: Vec<NodeId> = span_dom_id
        .az_children(&ctx.styled_dom.node_hierarchy.as_container())
        .collect();

    debug_info!(
        ctx,
        "[collect_inline_span_recursive] Span has {} DOM children",
        span_dom_children.len()
    );

    // borders and a line height, and thus influence inline layout calculations
    if span_dom_children.is_empty() {
        let node_state = &ctx.styled_dom.styled_nodes.as_container()[span_dom_id].styled_node_state;
        let font_size = get_element_font_size(ctx.styled_dom, span_dom_id, node_state);

        let line_height_value = crate::solver3::getters::get_line_height_value(
            ctx.styled_dom, span_dom_id, &node_state
        );
        let line_height = line_height_value
            .map(|v| v.inner.normalized() * font_size)
            .unwrap_or(font_size * 1.2);

        let cb_width = constraints.containing_block_size.main(constraints.writing_mode);
        let padding_top = crate::solver3::getters::get_css_padding_top(ctx.styled_dom, span_dom_id, &node_state)
            .exact().map(|pv| pv.to_pixels_internal(cb_width, font_size)).unwrap_or(0.0);
        let padding_bottom = crate::solver3::getters::get_css_padding_bottom(ctx.styled_dom, span_dom_id, &node_state)
            .exact().map(|pv| pv.to_pixels_internal(cb_width, font_size)).unwrap_or(0.0);
        let padding_left = crate::solver3::getters::get_css_padding_left(ctx.styled_dom, span_dom_id, &node_state)
            .exact().map(|pv| pv.to_pixels_internal(cb_width, font_size)).unwrap_or(0.0);
        let padding_right = crate::solver3::getters::get_css_padding_right(ctx.styled_dom, span_dom_id, &node_state)
            .exact().map(|pv| pv.to_pixels_internal(cb_width, font_size)).unwrap_or(0.0);
        let border_top = crate::solver3::getters::get_css_border_top_width(ctx.styled_dom, span_dom_id, &node_state)
            .exact().map(|pv| pv.to_pixels_internal(cb_width, font_size)).unwrap_or(0.0);
        let border_bottom = crate::solver3::getters::get_css_border_bottom_width(ctx.styled_dom, span_dom_id, &node_state)
            .exact().map(|pv| pv.to_pixels_internal(cb_width, font_size)).unwrap_or(0.0);
        let border_left = crate::solver3::getters::get_css_border_left_width(ctx.styled_dom, span_dom_id, &node_state)
            .exact().map(|pv| pv.to_pixels_internal(cb_width, font_size)).unwrap_or(0.0);
        let border_right = crate::solver3::getters::get_css_border_right_width(ctx.styled_dom, span_dom_id, &node_state)
            .exact().map(|pv| pv.to_pixels_internal(cb_width, font_size)).unwrap_or(0.0);
        let margin_left = crate::solver3::getters::get_css_margin_left(ctx.styled_dom, span_dom_id, &node_state)
            .exact().map(|pv| pv.to_pixels_internal(cb_width, font_size)).unwrap_or(0.0);
        let margin_right = crate::solver3::getters::get_css_margin_right(ctx.styled_dom, span_dom_id, &node_state)
            .exact().map(|pv| pv.to_pixels_internal(cb_width, font_size)).unwrap_or(0.0);

        let total_height = line_height + padding_top + padding_bottom + border_top + border_bottom;
        let total_width = margin_left + padding_left + border_left
            + border_right + padding_right + margin_right;

        content.push(InlineContent::Shape(InlineShape {
            shape_def: ShapeDefinition::Rectangle {
                size: crate::text3::cache::Size {
                    width: total_width,
                    height: total_height,
                },
                corner_radius: None,
            },
            fill: None,
            stroke: None,
            baseline_offset: 0.0,
            alignment: crate::solver3::getters::get_vertical_align_for_node(ctx.styled_dom, span_dom_id),
            source_node_id: Some(span_dom_id),
        }));

        return Ok(());
    }

    for &child_dom_id in &span_dom_children {
        let node_data = &ctx.styled_dom.node_data.as_container()[child_dom_id];

        // CASE 1: Text node - collect with span's style
        if let NodeType::Text(ref text_content) = node_data.get_node_type() {
            debug_info!(
                ctx,
                "[collect_inline_span_recursive] ✓ Found text in span: '{}'",
                text_content.as_str()
            );
            let text_items = split_text_for_whitespace(
                ctx.styled_dom,
                child_dom_id,
                text_content.as_str(),
                Arc::new(span_style.clone()),
            );
            content.extend(text_items);
            continue;
        }

        // CASE 2: Element node - check its display type
        let child_display =
            get_display_property(ctx.styled_dom, Some(child_dom_id)).unwrap_or_default();

        // Find the corresponding layout tree node
        let child_index = parent_children
            .iter()
            .find(|&&idx| {
                tree.get(idx)
                    .and_then(|n| n.dom_node_id)
                    .map(|id| id == child_dom_id)
                    .unwrap_or(false)
            })
            .copied();

        match child_display {
            LayoutDisplay::Inline => {
                // Nested inline span - recurse with child's style
                debug_info!(
                    ctx,
                    "[collect_inline_span_recursive] Found nested inline span {:?}",
                    child_dom_id
                );
                let child_style = get_style_properties(ctx.styled_dom, child_dom_id, ctx.system_style.as_ref());
                collect_inline_span_recursive(
                    ctx,
                    tree,
                    child_dom_id,
                    child_style,
                    content,
                    child_map,
                    parent_children,
                    constraints,
                )?;
            }
            LayoutDisplay::InlineBlock => {
                // Inline-block inside span - measure and add as shape
                let Some(child_index) = child_index else {
                    debug_info!(
                        ctx,
                        "[collect_inline_span_recursive] WARNING: inline-block {:?} has no layout \
                         node",
                        child_dom_id
                    );
                    continue;
                };

                let child_node = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
                let intrinsic_size = child_node.intrinsic_sizes.clone().unwrap_or_default();
                let width = intrinsic_size.max_content_width;

                let styled_node_state = ctx
                    .styled_dom
                    .styled_nodes
                    .as_container()
                    .get(child_dom_id)
                    .map(|n| n.styled_node_state.clone())
                    .unwrap_or_default();
                let writing_mode =
                    get_writing_mode(ctx.styled_dom, child_dom_id, &styled_node_state)
                        .unwrap_or_default();
                let child_wm_ctx = super::geometry::WritingModeContext {
                    writing_mode,
                    direction: get_direction_property(ctx.styled_dom, child_dom_id, &styled_node_state)
                        .unwrap_or_default(),
                    text_orientation: get_text_orientation_property(ctx.styled_dom, child_dom_id, &styled_node_state)
                        .unwrap_or_default(),
                };
                let child_constraints = LayoutConstraints {
                    available_size: LogicalSize::new(width, f32::INFINITY),
                    writing_mode,
                    writing_mode_ctx: child_wm_ctx,
                    bfc_state: None,
                    text_align: TextAlign::Start,
                    containing_block_size: constraints.containing_block_size,
                    available_width_type: Text3AvailableSpace::Definite(width),
                };

                drop(child_node);

                let mut empty_float_cache = HashMap::new();
                let layout_result = layout_formatting_context(
                    ctx,
                    tree,
                    &mut TextLayoutCache::default(),
                    child_index,
                    &child_constraints,
                    &mut empty_float_cache,
                )?;
                let final_height = layout_result.output.overflow_size.height;
                let final_size = LogicalSize::new(width, final_height);

                tree.get_mut(child_index).unwrap().used_size = Some(final_size);
                let baseline_offset = layout_result.output.baseline.unwrap_or(final_height);

                content.push(InlineContent::Shape(InlineShape {
                    shape_def: ShapeDefinition::Rectangle {
                        size: crate::text3::cache::Size {
                            width,
                            height: final_height,
                        },
                        corner_radius: None,
                    },
                    fill: None,
                    stroke: None,
                    baseline_offset,
                    alignment: crate::solver3::getters::get_vertical_align_for_node(ctx.styled_dom, child_dom_id),
                    source_node_id: Some(child_dom_id),
                }));

                // Note: We don't add to child_map here because this is inside a span
                debug_info!(
                    ctx,
                    "[collect_inline_span_recursive] Added inline-block shape {}x{}",
                    width,
                    final_height
                );
            }
            _ => {
                // +spec:display-property:0684c4 - block box inlinified: inner display becomes flow-root (treated as atomic inline)
                // in-flow children of an inline box are recursively inlinified so they
                // don't break the IFC. Treat them as inline spans and recurse into their
                // children to collect text and inline content.
                debug_info!(
                    ctx,
                    "[collect_inline_span_recursive] Inlinifying block-level child {:?} \
                     (display: {:?}) inside inline span per css-display-3 §2.7",
                    child_dom_id,
                    child_display
                );
                let child_style = get_style_properties(ctx.styled_dom, child_dom_id, ctx.system_style.as_ref());
                collect_inline_span_recursive(
                    ctx,
                    tree,
                    child_dom_id,
                    child_style,
                    content,
                    child_map,
                    parent_children,
                    constraints,
                )?;
            }
        }
    }

    Ok(())
}

/// Positions a floated child within the BFC and updates the floating context.
/// This function is fully writing-mode aware.
fn position_floated_child(
    _child_index: usize,
    child_margin_box_size: LogicalSize,
    float_type: LayoutFloat,
    constraints: &LayoutConstraints,
    _bfc_content_box: LogicalRect,
    current_main_offset: f32,
    floating_context: &mut FloatingContext,
) -> Result<LogicalPosition> {
    let wm = constraints.writing_mode;
    let child_main_size = child_margin_box_size.main(wm);
    let child_cross_size = child_margin_box_size.cross(wm);
    let bfc_cross_size = constraints.available_size.cross(wm);
    let mut placement_main_offset = current_main_offset;

    loop {
        // 1. Determine the available cross-axis space at the current
        // `placement_main_offset`.
        let (available_cross_start, available_cross_end) = floating_context
            .available_line_box_space(
                placement_main_offset,
                placement_main_offset + child_main_size,
                bfc_cross_size,
                wm,
            );

        let available_cross_width = available_cross_end - available_cross_start;

        // 2. Check if the new float can fit in the available space.
        if child_cross_size <= available_cross_width {
            // It fits! Determine the final position and add it to the context.
            // +spec:floats:5cfc93 - float:right positions box at cross-end, content flows on left
            let final_cross_pos = match float_type {
                LayoutFloat::Left => available_cross_start,
                // +spec:floats:5cfc93 - float:right positions box at cross-end, content flows on left
                LayoutFloat::Right => available_cross_end - child_cross_size,
                LayoutFloat::None => unreachable!(),
            };
            let final_pos =
                LogicalPosition::from_main_cross(placement_main_offset, final_cross_pos, wm);

            let new_float_box = FloatBox {
                kind: float_type,
                rect: LogicalRect::new(final_pos, child_margin_box_size),
                margin: EdgeSizes::default(), // TODO: Pass actual margin if this function is used
            };
            floating_context.floats.push(new_float_box);
            return Ok(final_pos);
        } else {
            // +spec:floats:3d89d8 - shift float downward when not enough horizontal room
            // It doesn't fit. We must move the float down past an obstacle.
            // Find the lowest main-axis end of all floats that are blocking
            // the current line.
            let mut next_main_offset = f32::INFINITY;
            for existing_float in &floating_context.floats {
                let float_main_start = existing_float.rect.origin.main(wm);
                let float_main_end = float_main_start + existing_float.rect.size.main(wm);

                // Consider only floats that are above or at the current placement line.
                if placement_main_offset < float_main_end {
                    next_main_offset = next_main_offset.min(float_main_end);
                }
            }

            if next_main_offset.is_infinite() {
                // This indicates an unrecoverable state, e.g., a float wider
                // than the container.
                return Err(LayoutError::PositioningFailed);
            }
            placement_main_offset = next_main_offset;
        }
    }
}

// CSS Property Getters

/// Get the CSS `float` property for a node.
fn get_float_property(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> LayoutFloat {
    let Some(id) = dom_id else {
        return LayoutFloat::None;
    };
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    get_float(styled_dom, id, node_state).unwrap_or(LayoutFloat::None)
}

fn get_clear_property(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> LayoutClear {
    let Some(id) = dom_id else {
        return LayoutClear::None;
    };
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    get_clear(styled_dom, id, node_state).unwrap_or(LayoutClear::None)
}
/// Helper to determine if scrollbars are needed.
///
/// # CSS Spec Reference
/// CSS Overflow Module Level 3 § 3: Scrollable overflow
// +spec:block-formatting-context:50d915 - overflow-x handles horizontal, overflow-y handles vertical
// +spec:box-model:63d6f2 - scrollable overflow extends beyond padding edge, needs scroll mechanism
// +spec:box-model:45b5fb - scrollbar space subtracted from content area, inserted between inner border edge and outer padding edge
// +spec:box-model:70a0a4 - UAs must start assuming no scrollbars needed, recalculate if they are
// +spec:box-model:c1b0b2 - scrollbar gutter is space between inner border edge and outer padding edge
// +spec:overflow:4f5b99 - scrollable overflow rectangle: content_size is the minimal axis-aligned rect containing scrollable overflow
// +spec:overflow:e983f4 - overflow:auto/scroll boxes must allow user to access overflowed content via scrollbars
// +spec:overflow:97c257 - relative positioning causing overflow in auto/scroll boxes must trigger scrollbar creation
pub fn check_scrollbar_necessity(
    content_size: LogicalSize,
    container_size: LogicalSize,
    overflow_x: OverflowBehavior,
    overflow_y: OverflowBehavior,
    scrollbar_width_px: f32,
) -> ScrollbarRequirements {
    // Use epsilon for float comparisons to avoid showing scrollbars due to 
    // floating-point rounding errors. Without this, content that exactly fits
    // may show scrollbars due to sub-pixel differences (e.g., 299.9999 vs 300.0).
    const EPSILON: f32 = 1.0;

    // +spec:height-calculation:c5af64 - assume no scrollbars initially; only add if content overflows
    // Determine if scrolling is needed based on overflow properties.
    // Note: scrollbar_width_px can be 0 for overlay scrollbars (e.g. macOS),
    // but we still need to register scroll nodes so that scrolling works —
    // overlay scrollbars just don't reserve any layout space.
    let mut needs_horizontal = match overflow_x {
        OverflowBehavior::Visible | OverflowBehavior::Hidden | OverflowBehavior::Clip => false,
        OverflowBehavior::Scroll => true,
        OverflowBehavior::Auto => content_size.width > container_size.width + EPSILON,
    };

    let mut needs_vertical = match overflow_y {
        OverflowBehavior::Visible | OverflowBehavior::Hidden | OverflowBehavior::Clip => false,
        OverflowBehavior::Scroll => true,
        OverflowBehavior::Auto => content_size.height > container_size.height + EPSILON,
    };

    // +spec:box-model:c3d73f - scrollbar presence affects available content area; padding preserved at scroll end
    // A classic layout problem: a vertical scrollbar can reduce horizontal space,
    // causing a horizontal scrollbar to appear, which can reduce vertical space...
    // A full solution involves a loop, but this two-pass check handles most cases.
    // Only relevant when scrollbars reserve layout space (non-overlay).
    if scrollbar_width_px > 0.0 {
        if needs_vertical && !needs_horizontal && overflow_x == OverflowBehavior::Auto {
            if content_size.width > (container_size.width - scrollbar_width_px) + EPSILON {
                needs_horizontal = true;
            }
        }
        if needs_horizontal && !needs_vertical && overflow_y == OverflowBehavior::Auto {
            if content_size.height > (container_size.height - scrollbar_width_px) + EPSILON {
                needs_vertical = true;
            }
        }
    }

    ScrollbarRequirements {
        needs_horizontal,
        needs_vertical,
        scrollbar_width: if needs_vertical {
            scrollbar_width_px
        } else {
            0.0
        },
        scrollbar_height: if needs_horizontal {
            scrollbar_width_px
        } else {
            0.0
        },
        // visual_width_px is set by the caller (compute_scrollbar_info_core)
        // since this function doesn't have access to the CSS style context.
        visual_width_px: 0.0,
    }
}

/// Calculates a single collapsed margin from two adjoining vertical margins.
///
/// Implements the rules from CSS 2.1 section 8.3.1:
/// - If both margins are positive, the result is the larger of the two.
/// - If both margins are negative, the result is the more negative of the two.
/// - If the margins have mixed signs, they are effectively summed.
// +spec:margin-collapsing:814a26 - vertical margins between sibling blocks collapse
pub fn collapse_margins(a: f32, b: f32) -> f32 {
    if a.is_sign_positive() && b.is_sign_positive() {
        a.max(b)
    } else if a.is_sign_negative() && b.is_sign_negative() {
        a.min(b)
    } else {
        a + b
    }
}

/// Helper function to advance the pen position with margin collapsing.
///
/// This implements CSS 2.1 margin collapsing for adjacent block-level boxes in a BFC.
///
/// - `pen` - Current main-axis position (will be modified)
/// - `last_margin_bottom` - The bottom margin of the previous in-flow element
/// - `current_margin_top` - The top margin of the current element
///
/// # Returns
///
/// The new `last_margin_bottom` value (the bottom margin of the current element)
///
/// # CSS Spec Compliance
///
/// Per CSS 2.1 Section 8.3.1 "Collapsing margins":
///
/// - Adjacent vertical margins of block boxes collapse
/// - The resulting margin width is the maximum of the adjoining margins (if both positive)
/// - Or the sum of the most positive and most negative (if signs differ)
fn advance_pen_with_margin_collapse(
    pen: &mut f32,
    last_margin_bottom: f32,
    current_margin_top: f32,
) -> f32 {
    // Collapse the previous element's bottom margin with current element's top margin
    let collapsed_margin = collapse_margins(last_margin_bottom, current_margin_top);

    // Advance pen by the collapsed margin
    *pen += collapsed_margin;

    // Return collapsed_margin so caller knows how much space was actually added
    collapsed_margin
}

/// Checks if an element's border or padding prevents margin collapsing.
///
/// Per CSS 2.1 Section 8.3.1:
///
/// - Border between margins prevents collapsing
/// - Padding between margins prevents collapsing
///
/// # Arguments
///
/// - `box_props` - The box properties containing border and padding
/// - `writing_mode` - The writing mode to determine main axis
/// - `check_start` - If true, check main-start (top); if false, check main-end (bottom)
///
/// # Returns
///
/// `true` if border or padding exists and prevents collapsing
// +spec:box-model:ca8ceb - margin collapsing uses block-start/block-end per writing mode
fn has_margin_collapse_blocker(
    box_props: &crate::solver3::geometry::BoxProps,
    writing_mode: LayoutWritingMode,
    check_start: bool, // true = check top/start, false = check bottom/end
) -> bool {
    if check_start {
        // Check if there's border-top or padding-top
        let border_start = box_props.border.main_start(writing_mode);
        let padding_start = box_props.padding.main_start(writing_mode);
        border_start > 0.0 || padding_start > 0.0
    } else {
        // Check if there's border-bottom or padding-bottom
        let border_end = box_props.border.main_end(writing_mode);
        let padding_end = box_props.padding.main_end(writing_mode);
        border_end > 0.0 || padding_end > 0.0
    }
}

/// Checks if an element is empty (has no content).
///
/// Per CSS 2.1 Section 8.3.1:
///
/// > If a block element has no border, padding, inline content, height, or min-height,
/// > then its top and bottom margins collapse with each other.
///
/// # Arguments
///
/// - `node` - The layout node to check
///
/// # Returns
///
/// `true` if the element is empty and its margins can collapse internally
fn is_empty_block(tree: &LayoutTree, node_index: usize) -> bool {
    let node = match tree.get(node_index) {
        Some(n) => n,
        None => return true,
    };
    // Per CSS 2.2 § 8.3.1: An empty block is one that:
    // - Has zero computed 'min-height'
    // - Has zero or 'auto' computed 'height'
    // - Has no in-flow children
    // - Has no line boxes (no text/inline content)

    // Check if node has children
    if !tree.children(node_index).is_empty() {
        return false;
    }

    // Check if node has inline content (text)
    if node.inline_layout_result.is_some() {
        return false;
    }

    // Check if node has explicit height > 0
    // CSS 2.2 § 8.3.1: Elements with explicit height are NOT empty
    if let Some(size) = node.used_size {
        if size.height > 0.0 {
            return false;
        }
    }

    // Empty block: no children, no inline content, no height
    true
}

/// Generates marker text for a list item marker.
///
/// This function looks up the counter value from the cache and formats it
/// according to the list-style-type property.
///
/// Per CSS Lists Module Level 3, the ::marker pseudo-element is the first child
/// of the list-item, and references the same DOM node. Counter resolution happens
/// on the list-item (parent) node.
fn generate_list_marker_text(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    marker_index: usize,
    counters: &HashMap<(usize, String), i32>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> String {
    use crate::solver3::counters::format_counter;

    // Get the marker node
    let marker_node = match tree.get(marker_index) {
        Some(n) => n,
        None => return String::new(),
    };

    // Verify this is actually a ::marker pseudo-element
    // Per spec, markers must be pseudo-elements, not anonymous boxes
    if marker_node.pseudo_element != Some(PseudoElement::Marker) {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::warning(format!(
                "[generate_list_marker_text] WARNING: Node {} is not a ::marker pseudo-element \
                 (pseudo={:?}, anonymous_type={:?})",
                marker_index, marker_node.pseudo_element, marker_node.anonymous_type
            )));
        }
        // Fallback for old-style anonymous markers during transition
        if marker_node.anonymous_type != Some(AnonymousBoxType::ListItemMarker) {
            return String::new();
        }
    }

    // Get the parent list-item node (::marker is first child of list-item)
    let list_item_index = match marker_node.parent {
        Some(p) => p,
        None => {
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::error(
                    "[generate_list_marker_text] ERROR: Marker has no parent".to_string(),
                ));
            }
            return String::new();
        }
    };

    let list_item_node = match tree.get(list_item_index) {
        Some(n) => n,
        None => return String::new(),
    };

    let list_item_dom_id = match list_item_node.dom_node_id {
        Some(id) => id,
        None => {
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::error(
                    "[generate_list_marker_text] ERROR: List-item has no DOM ID".to_string(),
                ));
            }
            return String::new();
        }
    };

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[generate_list_marker_text] marker_index={}, list_item_index={}, \
             list_item_dom_id={:?}",
            marker_index, list_item_index, list_item_dom_id
        )));
    }

    // Get list-style-type from the list-item or its container
    let list_container_dom_id = if let Some(grandparent_index) = list_item_node.parent {
        if let Some(grandparent) = tree.get(grandparent_index) {
            grandparent.dom_node_id
        } else {
            None
        }
    } else {
        None
    };

    // Try to get list-style-type from the list container first,
    // then fall back to the list-item
    let list_style_type = if let Some(container_id) = list_container_dom_id {
        let container_type = get_list_style_type(styled_dom, Some(container_id));
        if container_type != StyleListStyleType::default() {
            container_type
        } else {
            get_list_style_type(styled_dom, Some(list_item_dom_id))
        }
    } else {
        get_list_style_type(styled_dom, Some(list_item_dom_id))
    };

    // Get the counter value for "list-item" counter from the LIST-ITEM node
    // Per CSS spec, counters are scoped to elements, and the list-item counter
    // is incremented at the list-item element, not the marker pseudo-element
    let counter_value = counters
        .get(&(list_item_index, "list-item".to_string()))
        .copied()
        .unwrap_or_else(|| {
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::warning(format!(
                    "[generate_list_marker_text] WARNING: No counter found for list-item at index \
                     {}, defaulting to 1",
                    list_item_index
                )));
            }
            1
        });

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[generate_list_marker_text] counter_value={} for list_item_index={}",
            counter_value, list_item_index
        )));
    }

    // Format the counter according to the list-style-type
    let marker_text = format_counter(counter_value, list_style_type);

    // For ordered lists (non-symbolic markers), add a period and space
    // For unordered lists (symbolic markers like •, ◦, ▪), just add a space
    if matches!(
        list_style_type,
        StyleListStyleType::Decimal
            | StyleListStyleType::DecimalLeadingZero
            | StyleListStyleType::LowerAlpha
            | StyleListStyleType::UpperAlpha
            | StyleListStyleType::LowerRoman
            | StyleListStyleType::UpperRoman
            | StyleListStyleType::LowerGreek
            | StyleListStyleType::UpperGreek
    ) {
        format!("{}. ", marker_text)
    } else {
        format!("{} ", marker_text)
    }
}

/// Generates marker text segments for a list item marker.
///
/// Simply returns a single StyledRun with the marker text using the base_style.
/// The font stack in base_style already includes fallbacks with 100% Unicode coverage,
/// so font resolution happens during text shaping, not here.
fn generate_list_marker_segments(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    marker_index: usize,
    counters: &HashMap<(usize, String), i32>,
    base_style: Arc<StyleProperties>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Vec<StyledRun> {
    // Generate the marker text
    let marker_text =
        generate_list_marker_text(tree, styled_dom, marker_index, counters, debug_messages);
    if marker_text.is_empty() {
        return Vec::new();
    }

    if let Some(msgs) = debug_messages {
        let font_families: Vec<&str> = match &base_style.font_stack {
            crate::text3::cache::FontStack::Stack(selectors) => {
                selectors.iter().map(|f| f.family.as_str()).collect()
            }
            crate::text3::cache::FontStack::Ref(_) => vec!["<embedded-font>"],
        };
        msgs.push(LayoutDebugMessage::info(format!(
            "[generate_list_marker_segments] Marker text: '{}' with font stack: {:?}",
            marker_text,
            font_families
        )));
    }

    // Return single segment - font fallback happens during shaping
    // List markers are generated content, not from DOM nodes
    vec![StyledRun {
        text: marker_text,
        style: base_style,
        logical_start_byte: 0,
        source_node_id: None,
    }]
}

/// Returns true if a character has Unicode line breaking class BK (mandatory break)
/// or NL (next line). Per CSS Text 3 §5.1, these must be treated as forced line
/// breaks regardless of the white-space property value.
#[inline]
fn is_bk_or_nl_class(c: char) -> bool {
    matches!(c, '\u{000B}' | '\u{000C}' | '\u{0085}' | '\u{2028}' | '\u{2029}')
}

/// Splits text at all forced break points: newlines (\n, \r\n, \r) and BK/NL class chars.
/// Used for white-space modes that preserve segment breaks (pre, pre-wrap, pre-line, break-spaces).
// +spec:white-space-processing:af4e3f - each newline/segment break in text is treated as a segment break, interpreted per white-space property
fn split_at_forced_breaks(text: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' {
            segments.push(std::mem::take(&mut current));
        } else if c == '\r' {
            segments.push(std::mem::take(&mut current));
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
        } else if is_bk_or_nl_class(c) {
            segments.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    segments.push(current);
    segments
}

/// Splits text only at BK/NL class characters (not \n which is collapsed in normal/nowrap).
/// Used for white-space: normal/nowrap where \n is collapsed to space but BK/NL chars
/// still produce forced breaks per CSS Text 3 §5.1.
fn split_at_bk_nl_chars(text: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if is_bk_or_nl_class(c) {
            segments.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    segments.push(current);
    segments
}

/// Returns true if the character is East Asian (CJK) for the purposes of
/// segment break transformation rules (CSS Text Level 3, §4.1.3).
fn is_east_asian_wide(c: char) -> bool {
    let cp = c as u32;
    // CJK Unified Ideographs
    (0x4E00..=0x9FFF).contains(&cp)
    || (0x3400..=0x4DBF).contains(&cp)
    || (0x20000..=0x2A6DF).contains(&cp)
    || (0xF900..=0xFAFF).contains(&cp)
    // Hiragana
    || (0x3040..=0x309F).contains(&cp)
    // Katakana
    || (0x30A0..=0x30FF).contains(&cp)
    || (0x31F0..=0x31FF).contains(&cp)
    // CJK Radicals / Kangxi / Ideographic Description
    || (0x2E80..=0x2EFF).contains(&cp)
    || (0x2F00..=0x2FDF).contains(&cp)
    || (0x2FF0..=0x2FFF).contains(&cp)
    // CJK Symbols and Punctuation
    || (0x3000..=0x303F).contains(&cp)
    || (0x3200..=0x32FF).contains(&cp)
    || (0x3300..=0x33FF).contains(&cp)
    // Bopomofo
    || (0x3100..=0x312F).contains(&cp)
    // Hangul Syllables
    || (0xAC00..=0xD7AF).contains(&cp)
    // Fullwidth forms
    || (0xFF01..=0xFF60).contains(&cp)
    || (0xFFE0..=0xFFE6).contains(&cp)
}

// +spec:block-formatting-context:b78223 - fullwidth/wide chars treated as vertical script, halfwidth as horizontal per UAX#11
fn is_east_asian_fullwidth_or_wide(ch: char) -> bool {
    let cp = ch as u32;
    // Exclude Hangul
    if (0x1100..=0x11FF).contains(&cp)
        || (0x3130..=0x318F).contains(&cp)
        || (0xAC00..=0xD7AF).contains(&cp)
        || (0xA960..=0xA97F).contains(&cp)
        || (0xD7B0..=0xD7FF).contains(&cp)
    {
        return false;
    }
    is_east_asian_wide(ch)
        || (0xFF61..=0xFFDC).contains(&cp)
        || (0xFFE8..=0xFFEE).contains(&cp)
        || (0xA000..=0xA4CF).contains(&cp)
}

/// +spec:white-space-processing:159dbf - segment breaks converted to spaces (default transform)
/// +spec:white-space-processing:79891b - segment break transform: convert to space or remove
// +spec:white-space-processing:7e9529 - Segment break transformation rules (§4.1.3): collapse consecutive breaks, remove around ZWSP/CJK, else convert to space
/// Transforms segment breaks (newlines) in text according to CSS Text Level 3 §4.1.3.
/// - If adjacent to a zero-width space (U+200B), the segment break is removed.
/// - If both adjacent chars are East Asian F/W/H (not Hangul), removed entirely.
/// - Otherwise, converted to a single space.
fn apply_segment_break_transform(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;

    while i < len {
        let ch = chars[i];
        if ch == '\n' || ch == '\r' {
            let break_end = if ch == '\r' && i + 1 < len && chars[i + 1] == '\n' {
                i + 2
            } else {
                i + 1
            };

            // +spec:white-space-processing:3c3680 - remove tabs/spaces around segment break before transform
            // §4.1.1: remove collapsible whitespace around segment breaks
            while result.ends_with(' ') || result.ends_with('\t') {
                result.pop();
            }

            let mut after_idx = break_end;
            while after_idx < len && (chars[after_idx] == ' ' || chars[after_idx] == '\t') {
                after_idx += 1;
            }

            let char_before = result.chars().last();
            let char_after = if after_idx < len { Some(chars[after_idx]) } else { None };

            // Rule 1: adjacent to zero-width space → remove
            if char_before == Some('\u{200B}') || char_after == Some('\u{200B}') {
                // remove segment break
            }
            // Rule 2: both sides East Asian F/W/H (not Hangul) → remove
            else if let (Some(before), Some(after)) = (char_before, char_after) {
                if is_east_asian_fullwidth_or_wide(before) && is_east_asian_fullwidth_or_wide(after) {
                    // remove segment break
                } else {
                    result.push(' ');
                }
            } else {
                result.push(' ');
            }

            i = after_idx;
        } else {
            result.push(ch);
            i += 1;
        }
    }

    result
}

// ============================================================================
// WHITE-SPACE PROCESSING PIPELINE (CSS Text Level 3 §4)
// ============================================================================
//
// +spec:white-space-processing:b64e38 - parser may normalize/collapse whitespace before CSS; CSS cannot restore
// The white-space processing pipeline is organized into four phases per the
// CSS Text Level 3 specification:
//
//   Phase 1 (Collapse): Collapse whitespace sequences per §4.1.1
//   Phase 2 (Segment Break Transform): Transform segment breaks per §4.1.3
//   Phase 3 (Edge Trimming): Trim spaces at line start/end per §4.1.2
//   Phase 4 (Tab Resolution): Resolve tab stops per §4.2
//
// Each phase is a standalone function that transforms a string, allowing
// spec patches to modify individual phases without touching others.

/// Phase 1: Collapse consecutive whitespace to a single space.
/// CSS Text 3 §4.1.1 - applies to `normal`, `nowrap`, and `pre-line` modes.
pub fn ws_phase1_collapse(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_was_space = false;
    for ch in text.chars() {
        if ch == ' ' || ch == '\t' {
            if !prev_was_space {
                result.push(' ');
                prev_was_space = true;
            }
        } else {
            result.push(ch);
            prev_was_space = false;
        }
    }
    result
}

/// Phase 2: Transform segment breaks (newlines) per CSS Text 3 §4.1.3.
/// Delegates to `apply_segment_break_transform` for the actual transformation rules.
pub fn ws_phase2_segment_break_transform(text: &str) -> String {
    apply_segment_break_transform(text)
}

/// Phase 3: Trim leading/trailing collapsible whitespace at line boundaries.
/// CSS Text 3 §4.1.2 - this is a no-op during text collection; actual trimming
/// happens during line breaking when line start/end positions are known.
/// Provided as a pipeline slot for patches to hook into.
pub fn ws_phase3_trim_edges(text: &str) -> String {
    text.to_string()
}

/// Phase 4: Resolve tab characters to spaces based on tab-size.
/// CSS Text 3 §4.2 - for `normal`/`nowrap`, tabs are collapsed to spaces in Phase 1.
/// For `pre`/`pre-wrap`/`break-spaces`, tabs are emitted as `InlineContent::Tab`
/// and resolved during line layout. This phase is a no-op during text collection.
pub fn ws_phase4_resolve_tabs(text: &str) -> String {
    text.to_string()
}

/// Splits text content into InlineContent items based on white-space CSS property.
///
///
/// For `white-space: pre`, `pre-wrap`, and `pre-line`, newlines (`\n`) are treated as
/// forced line breaks per CSS Text Level 3 specification:
/// https://www.w3.org/TR/css-text-3/#white-space-property
///
/// Additionally, Unicode characters with BK or NL line breaking class (VT, FF, NEL, LS, PS)
/// are always treated as forced line breaks regardless of the white-space value.
///
/// This function:
/// 1. Checks the white-space property of the node (or its parent for text nodes)
/// 2. If `pre`, `pre-wrap`, or `pre-line`: splits text by `\n` and inserts `InlineContent::LineBreak`
/// 3. Otherwise: returns the text as a single `InlineContent::Text`
/// 4. In ALL modes: BK/NL class chars (VT, FF, NEL, LS, PS) produce forced breaks
///
/// Returns a Vec of InlineContent items that correctly represent line breaks.

// +spec:display-property:1389e3 - bidi control characters per UAX #9 for Unicode bidirectional algorithm
// +spec:display-property:aad99b - inline boxes can be split into fragments due to bidi text processing
// Bidi_Control property (UAX #9). These characters are ignored during white-space processing.
fn is_bidi_control(c: char) -> bool {
    matches!(c,
        '\u{200E}' | // LEFT-TO-RIGHT MARK
        '\u{200F}' | // RIGHT-TO-LEFT MARK
        '\u{202A}' | // LEFT-TO-RIGHT EMBEDDING
        '\u{202B}' | // RIGHT-TO-LEFT EMBEDDING
        '\u{202C}' | // POP DIRECTIONAL FORMATTING
        '\u{202D}' | // LEFT-TO-RIGHT OVERRIDE
        '\u{202E}' | // RIGHT-TO-LEFT OVERRIDE
        '\u{2066}' | // LEFT-TO-RIGHT ISOLATE
        '\u{2067}' | // RIGHT-TO-LEFT ISOLATE
        '\u{2068}' | // FIRST STRONG ISOLATE
        '\u{2069}' | // POP DIRECTIONAL ISOLATE
        '\u{061C}'   // ARABIC LETTER MARK
    )
}

/// +spec:white-space-processing:1188f6 - only spaces, tabs, and segment breaks are document white space
/// Returns true if `c` is a CSS "document white space character" per CSS Text Level 3 §4.1.
/// Only spaces (U+0020), tabs (U+0009), and segment breaks (LF, CR, FF) qualify.
/// Other Unicode whitespace (e.g. U+00A0 non-breaking space) is NOT document white space.
#[inline]
pub fn is_css_document_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')
}

// +spec:white-space-processing:efbece - white-space property controls collapsing/preserving of formatting characters for rendering
// +spec:writing-modes:b87688 - inlines laid out with bidi reordering and white-space wrapping
// +spec:writing-modes:cdd4f1 - white space trimming before bidi reordering preserves end-of-line spaces per UAX9 L1
// white space characters are processed prior to line breaking and bidi reordering
// +spec:inline-block:381c0c - white-space property: collapsing, wrapping, and forced breaks per mode
pub fn split_text_for_whitespace(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    text: &str,
    style: Arc<StyleProperties>,
) -> Vec<InlineContent> {
    use crate::text3::cache::{BreakType, ClearType, InlineBreak};

    // (characters with the Bidi_Control property) as if they were not there"
    // Strip bidi control characters before white-space processing so they don't
    // interfere with collapsing (e.g. a bidi mark between two spaces).
    let text_owned;
    let text: &str = if text.chars().any(|c| is_bidi_control(c)) {
        text_owned = text.chars().filter(|c| !is_bidi_control(*c)).collect::<String>();
        &text_owned
    } else {
        text
    };

    // Get the white-space property - TEXT NODES inherit from parent!
    // We need to check the parent element's white-space, not the text node itself
    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    let parent_id = node_hierarchy[dom_id].parent_id();
    
    // Try parent first, then fall back to the node itself
    let white_space = if let Some(parent) = parent_id {
        let styled_nodes = styled_dom.styled_nodes.as_container();
        let parent_state = styled_nodes
            .get(parent)
            .map(|n| n.styled_node_state.clone())
            .unwrap_or_default();
        
        match get_white_space_property(styled_dom, parent, &parent_state) {
            MultiValue::Exact(ws) => ws,
            _ => StyleWhiteSpace::Normal,
        }
    } else {
        StyleWhiteSpace::Normal
    };
    
    let mut result = Vec::new();

    // +spec:white-space-processing:3a0f58 - HTML newlines normalized to U+000A, each treated as segment break
    // +spec:white-space-processing:6eb1a2 - CR (U+000D) not treated as segment break by HTML; handle if inserted via DOM
    // HTML parsers convert \r to \n during preprocessing, but \r can survive
    // via escape sequences (e.g. &#x0d;). Any remaining U+000D must be
    // treated identically to U+000A (line feed).
    let text_cr;
    let text: &str = if text.contains('\r') {
        text_cr = text.replace("\r\n", "\n").replace('\r', "\n");
        &text_cr
    } else {
        text
    };

    // +spec:white-space-processing:bd11da - white-space property: new lines, spaces/tabs, wrapping per value table
    // +spec:white-space-processing:b166c5 - segment breaks preserved as forced line feeds for pre/pre-wrap/break-spaces/pre-line
    // For `pre`, `pre-wrap`, `pre-line`, and `break-spaces`, newlines must be preserved as forced breaks
    // CSS Text Level 3: "Newlines in the source will be honored as forced line breaks."
    match white_space {
        StyleWhiteSpace::Pre | StyleWhiteSpace::PreWrap | StyleWhiteSpace::BreakSpaces => {
            // Pre, pre-wrap, break-spaces: preserve whitespace and honor newlines
            // Split by newlines and BK/NL class chars, insert LineBreak between parts
            // Also handle tab characters (\t) by inserting InlineContent::Tab
            let segments = split_at_forced_breaks(text);
            let segment_count = segments.len();
            let mut content_index = 0;

            for (seg_idx, segment) in segments.into_iter().enumerate() {
                // Split the segment by tab characters and insert Tab elements
                let mut tab_parts = segment.split('\t').peekable();
                while let Some(part) = tab_parts.next() {
                    if !part.is_empty() {
                        result.push(InlineContent::Text(StyledRun {
                            text: part.to_string(),
                            style: Arc::clone(&style),
                            logical_start_byte: 0,
                            source_node_id: Some(dom_id),
                        }));
                    }

                    if tab_parts.peek().is_some() {
                        result.push(InlineContent::Tab { style: Arc::clone(&style) });
                    }
                }

                if seg_idx + 1 < segment_count {
                    result.push(InlineContent::LineBreak(InlineBreak {
                        break_type: BreakType::Hard,
                        clear: ClearType::None,
                        content_index,
                    }));
                    content_index += 1;
                }
            }
        }
        StyleWhiteSpace::PreLine => {
            // Pre-line: collapse whitespace but honor newlines and BK/NL class chars
            let segments = split_at_forced_breaks(text);
            let segment_count = segments.len();
            let mut content_index = 0;

            for (seg_idx, segment) in segments.into_iter().enumerate() {
                // Collapse only CSS document white space within the line (not all Unicode whitespace)
                let collapsed: String = segment
                    .split(|c: char| is_css_document_whitespace(c))
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");

                if !collapsed.is_empty() {
                    result.push(InlineContent::Text(StyledRun {
                        text: collapsed,
                        style: Arc::clone(&style),
                        logical_start_byte: 0,
                        source_node_id: Some(dom_id),
                    }));
                }

                if seg_idx + 1 < segment_count {
                    result.push(InlineContent::LineBreak(InlineBreak {
                        break_type: BreakType::Hard,
                        clear: ClearType::None,
                        content_index,
                    }));
                    content_index += 1;
                }
            }
        }
        StyleWhiteSpace::Normal | StyleWhiteSpace::Nowrap => {
            // +spec:white-space-processing:adbebb - Phase I collapsing for normal/nowrap modes
            // CSS Text Level 3, Section 4.1.1 - Phase I: Collapsing and Transformation
            // https://www.w3.org/TR/css-text-3/#white-space-phase-1
            //
            // For `white-space: normal` and `nowrap`:
            // 1. Segment breaks are transformed per §4.1.3
            // 2. Any sequence of consecutive spaces/tabs is collapsed to a single space
            // 3. Leading/trailing spaces at line boundaries are handled during line layout
            //
            // are forced breaks regardless of white-space value. Split on them first,
            // then collapse whitespace within each segment.
            let segments = split_at_bk_nl_chars(text);
            let segment_count = segments.len();
            let mut content_index = 0;

            for (seg_idx, segment) in segments.into_iter().enumerate() {
                let after_segment_breaks = apply_segment_break_transform(&segment);

                // Collapse document white space within this segment (normal/nowrap rules)
                let collapsed: String = after_segment_breaks
                    .chars()
                    .map(|c| if is_css_document_whitespace(c) { ' ' } else { c })
                    .collect::<String>()
                    .split(' ')
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");

                let final_text = if collapsed.is_empty() && !segment.is_empty() {
                    " ".to_string()
                } else if !collapsed.is_empty() {
                    // Check if original had leading/trailing document whitespace
                    let had_leading = segment.chars().next().map(|c| is_css_document_whitespace(c)).unwrap_or(false);
                    let had_trailing = segment.chars().last().map(|c| is_css_document_whitespace(c)).unwrap_or(false);

                    let mut r = String::new();
                    if had_leading { r.push(' '); }
                    r.push_str(&collapsed);
                    if had_trailing && !had_leading { r.push(' '); }
                    else if had_trailing && had_leading && collapsed.is_empty() { /* already have one space */ }
                    else if had_trailing { r.push(' '); }
                    r
                } else {
                    collapsed
                };

                if !final_text.is_empty() {
                    result.push(InlineContent::Text(StyledRun {
                        text: final_text,
                        style: Arc::clone(&style),
                        logical_start_byte: 0,
                        source_node_id: Some(dom_id),
                    }));
                }

                // Insert forced break between segments (for BK/NL chars)
                if seg_idx + 1 < segment_count {
                    result.push(InlineContent::LineBreak(InlineBreak {
                        break_type: BreakType::Hard,
                        clear: ClearType::None,
                        content_index,
                    }));
                    content_index += 1;
                }
            }
        }
    }

    // but before Phase II (trimming/positioning). This means full-width only transforms
    // spaces (U+0020) to U+3000 IDEOGRAPHIC SPACE within preserved white space, because
    // non-preserved spaces were already collapsed in Phase I above.
    let text_transform = style.text_transform;
    if text_transform != crate::text3::cache::TextTransform::None {
        for item in result.iter_mut() {
            if let InlineContent::Text(run) = item {
                run.text = apply_text_transform(&run.text, text_transform);
            }
        }
    }

    result
}

fn apply_text_transform(text: &str, transform: crate::text3::cache::TextTransform) -> String {
    use crate::text3::cache::TextTransform;
    match transform {
        TextTransform::None => text.to_string(),
        TextTransform::Uppercase => text.to_uppercase(),
        TextTransform::Lowercase => text.to_lowercase(),
        TextTransform::Capitalize => {
            let mut result = String::with_capacity(text.len());
            let mut prev_is_word_boundary = true;
            for c in text.chars() {
                if prev_is_word_boundary && c.is_alphabetic() {
                    for uc in c.to_uppercase() {
                        result.push(uc);
                    }
                    prev_is_word_boundary = false;
                } else {
                    result.push(c);
                    prev_is_word_boundary = c.is_whitespace() || c.is_ascii_punctuation();
                }
            }
            result
        }
        TextTransform::FullWidth => {
            // Full-width transforms ASCII characters to their full-width equivalents.
            // Spaces (U+0020) become U+3000 IDEOGRAPHIC SPACE — but only those that
            // survived Phase I collapsing (i.e. preserved white space).
            text.chars().map(|c| match c {
                ' ' => '\u{3000}',  // U+0020 SPACE -> U+3000 IDEOGRAPHIC SPACE
                '!' ..= '~' => {
                    // ASCII printable range U+0021..U+007E -> fullwidth U+FF01..U+FF5E
                    char::from_u32(c as u32 - 0x0021 + 0xFF01).unwrap_or(c)
                }
                _ => c,
            }).collect()
        }
    }
}

// ============================================================================
// INITIAL LETTER / DROP CAPS STUB
// ============================================================================

/// Computes the geometric exclusion area for an initial letter (drop cap).
///
/// CSS Inline Layout Module Level 3, section 3:
/// The `initial-letter` property specifies styling for dropped, raised, and sunken
/// initial letters. When set, the first glyph(s) of the first line are enlarged to
/// span multiple lines, with the remaining text wrapping around them.
///
/// # Algorithm
///
/// 1. The letter box height spans `size` lines: `height = size * line_height`.
/// 2. The letter box width is estimated using a typical capital letter aspect ratio
///    (cap-height-to-advance-width ~0.7 for Latin text). A proper implementation
///    would measure the actual glyph, but this gives a reasonable default.
/// 3. The letter is positioned at the inline-start of the first line.
/// 4. The `sink` value determines how many lines the letter drops below the
///    first baseline. When `sink == size`, this is a classic drop cap.
///    When `sink < size`, the letter rises above the first line (raised cap).
/// 5. A small gap (4px default) is added between the letter box and adjacent text.
///
/// # Parameters
/// - `initial_letter_size`: The number of lines the initial letter should span (e.g., 3.0)
/// - `initial_letter_sink`: How many lines the letter sinks below the first line
/// - `content_box_width`: Available width in the content box (for clamping)
/// - `line_height`: The computed line height for the containing block
///
/// # Returns
/// A tuple of `(letter_width, letter_height)` representing the space reserved for
/// the initial letter exclusion, or `(0.0, 0.0)` if the parameters are invalid.
///
/// The caller should use these dimensions to create a float-like exclusion at the
/// start of the block container, causing subsequent lines to wrap around the letter.
// +spec:width-calculation:7f4f68 - initial-letter-wrap exclusion area (none behavior; first/grid require glyph outlines)
pub fn layout_initial_letter(
    initial_letter_size: f32,
    initial_letter_sink: u32,
    content_box_width: f32,
    line_height: f32,
) -> (f32, f32) {
    // Guard against degenerate values
    if initial_letter_size <= 0.0 || line_height <= 0.0 || content_box_width <= 0.0 {
        return (0.0, 0.0);
    }

    // +spec:overflow:dd0679 - auto-sized initial letter content box fits exactly to content; alignment props do not apply
    // +spec:width-calculation:170742 - atomic initial letters with auto block size use inline initial letter sizing
    // CSS Inline Level 3 section 3.3: The initial letter box height spans `size` lines.
    let letter_height = initial_letter_size * line_height;

    // Estimate the letter width using a typical Latin capital letter aspect ratio.
    // The advance width of a capital letter is approximately 0.7x the cap height.
    // This is a heuristic; a full implementation would measure the actual glyph(s).
    const CAP_WIDTH_RATIO: f32 = 0.7;
    let letter_width_raw = letter_height * CAP_WIDTH_RATIO;

    // Add a small gap between the letter box and the adjacent inline content.
    // CSS Inline Level 3 section 3.5: browsers typically add ~4px padding.
    const LETTER_GAP: f32 = 4.0;
    let letter_width = (letter_width_raw + LETTER_GAP).min(content_box_width);

    // +spec:containing-block:67fd99 - block-axis positioning: size >= sink shifts by (sink-1)*line_height toward block-end
    // The actual exclusion height accounts for the sink value.
    // sink == size means the letter is fully dropped (classic drop cap).
    // sink < size means part of the letter rises above the first line (raised cap).
    // The exclusion area height is always `sink * line_height` since that's how
    // many lines of subsequent text need to wrap around the letter.
    let exclusion_height = (initial_letter_sink as f32) * line_height;

    // Use the larger of exclusion_height and letter_height as the actual
    // vertical space consumed. For raised caps (sink < size), the letter
    // extends above the first line but the exclusion only covers sink lines.
    // For sunken caps (sink >= size), the exclusion covers the full letter height.
    let effective_height = exclusion_height.max(letter_height);

    (letter_width, effective_height)
}
